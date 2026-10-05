use super::*;
use std::collections::BTreeMap;

fn definition(instance: &str) -> MeshExportDefinition {
    MeshExportDefinition {
        id: format!("mesh-{instance}"),
        output: InstanceOutputRef {
            instance: instance.into(),
            output: "body".into(),
        },
        settings: MeshSettings::default(),
        manufacturing: Some(ManufacturingSettings::default()),
        face_tags: vec![MeshFaceTag {
            id: "fixed".into(),
            faces: vec![FaceSelector::NearestCenter {
                target: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    10.0,
                    15.0,
                    LengthUnit::Millimeter,
                )),
                maximum_distance: ScalarExpr::Literal(Quantity::length(
                    0.001,
                    LengthUnit::Millimeter,
                )),
            }],
        }],
    }
}
fn model() -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Required, 100000.0);
    definition.requirements.clear();
    definition
}

#[test]
fn physical_boundary_tags_follow_faces_and_round_trip_with_mesh_definitions() {
    let family = model();
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let definition = definition("part");
    let session = Session::new().unwrap();
    let mesh = definition.generate(&graph, &session).unwrap();
    assert_eq!(mesh.triangles.len(), 12);
    assert_eq!(mesh.face_tags.iter().filter(|tag| **tag == 1).count(), 1);
    let face = mesh
        .faces
        .iter()
        .find(|face| face.physical_tag == 1)
        .unwrap();
    assert!((face.area_mm2 - 600.0).abs() < 1e-8);
    assert_eq!(face.center_mm, [0.0, 10.0, 15.0]);
    assert_eq!(
        mesh.triangles
            .iter()
            .filter(|triangle| mesh.face_tags[triangle.face_index] == 1)
            .count(),
        2
    );
    let msh = mesh.to_msh().unwrap();
    assert!(msh.contains("2 1 \"fixed\""));
    assert!(msh.contains("$Nodes\n8\n"));
    assert!(msh.contains("$Elements\n12\n"));
    let mut document = ModelDocument::from_graph(&graph);
    document.mesh_exports.push(definition.clone());
    let encoded = document.to_json_pretty().unwrap();
    assert_eq!(ModelDocument::from_json(&encoded).unwrap(), document);
    let mut previous = serde_json::to_value(ModelDocument::from_graph(&graph)).unwrap();
    previous["schema_version"] = serde_json::json!(43);
    assert!(
        ModelDocument::from_json(&previous.to_string())
            .unwrap()
            .mesh_exports
            .is_empty()
    );
    let mut edited = document.clone();
    edited.mesh_exports[0].face_tags[0].id = "load".into();
    assert!(!document.semantic_diff(&edited).unwrap().is_empty());
    assert_eq!(
        document.change_impact(&edited).unwrap().mesh_exports,
        ["mesh-part"]
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    if std::env::var_os("OCCT_MESH_QA").is_some() {
        let directory = std::path::Path::new("/tmp/occb-fea-box");
        definition
            .write_fea_bundle(&graph, &session, directory)
            .unwrap();
        std::fs::write(
            "/tmp/occb-mesh-box.gltf",
            mesh.to_gltf(MaterialAppearance::default()).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn sampled_wall_and_draft_checks_match_rectangular_and_curved_solids() {
    let family = model();
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mesh = definition("part").generate(&graph, &session).unwrap();
    let report = mesh
        .check_manufacturability(ManufacturingSettings {
            minimum_wall: Quantity::length(11.0, LengthUnit::Millimeter),
            ..ManufacturingSettings::default()
        })
        .unwrap();
    assert_eq!(report.draft.len(), 4);
    assert!(
        report
            .draft
            .iter()
            .all(|face| face.minimum_signed_radians.abs() < 1e-10 && !face.meets_minimum)
    );
    assert!(report.overhang_triangles.is_empty());
    assert_eq!(report.wall_samples.len(), 12);
    assert_eq!(report.unresolved_wall_samples, 0);
    assert!((report.minimum_sampled_wall_mm.unwrap() - 10.0).abs() < 1e-8);
    assert_eq!(
        report
            .wall_samples
            .iter()
            .filter(|sample| sample.below_minimum)
            .count(),
        4
    );
    let mut open = mesh.clone();
    open.triangles.pop();
    assert!(
        open.check_manufacturability(ManufacturingSettings::default())
            .is_err()
    );
    let mut reversed = mesh.clone();
    reversed.triangles[0].points.swap(1, 2);
    assert!(
        reversed
            .check_manufacturability(ManufacturingSettings::default())
            .is_err()
    );
    let sphere = session
        .create_sphere(Vec3::new(0.0, 0.0, 0.0), 10.0)
        .unwrap();
    let triangles = session
        .surface_mesh(&sphere, occt_bridge::MeshOptions::default())
        .unwrap();
    let sphere_mesh = TaggedSurfaceMesh {
        id: "sphere".into(),
        triangles,
        face_tags: vec![0],
        names: vec![],
        faces: vec![],
    };
    let report = sphere_mesh
        .check_manufacturability(ManufacturingSettings {
            maximum_wall_samples: 50,
            ..ManufacturingSettings::default()
        })
        .unwrap();
    assert_eq!(report.wall_samples.len(), 50);
    assert_eq!(report.unresolved_wall_samples, 0);
    for sample in &report.wall_samples {
        let triangle = &sphere_mesh.triangles[sample.triangle];
        let [a, b, c] = triangle.points;
        let ab = [b.x - a.x, b.y - a.y, b.z - a.z];
        let ac = [c.x - a.x, c.y - a.y, c.z - a.z];
        let normal = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let length = normal.iter().map(|value| value * value).sum::<f64>().sqrt();
        let normal = normal.map(|value| value / length);
        let origin = [
            (a.x + b.x + c.x) / 3.0,
            (a.y + b.y + c.y) / 3.0,
            (a.z + b.z + c.z) / 3.0,
        ];
        let projection = (0..3).map(|axis| origin[axis] * normal[axis]).sum::<f64>();
        let radius_squared = origin.iter().map(|value| value * value).sum::<f64>();
        let analytic_exit = projection + (projection * projection + 100.0 - radius_squared).sqrt();
        let measured = sample.thickness_mm.unwrap();
        assert!(
            measured <= analytic_exit + 1e-7 && measured > analytic_exit - 0.5,
            "sample {sample:?}, analytic ray exit {analytic_exit}"
        );
    }
    assert!(!report.overhang_triangles.is_empty());
    assert_eq!(session.shape_count().unwrap(), 1);
}

#[test]
fn gltf_uses_meters_y_up_shared_geometry_and_inherited_appearance() {
    let family = model();
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    graph
        .add_clone("copy", "part", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "copy",
            Placement::translated(VectorQuantity::lengths(
                100.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph
        .add_material(Material {
            id: "steel".into(),
            name: "Steel".into(),
            density_kg_per_cubic_meter: 7800.0,
        })
        .unwrap();
    graph.assign_material("part", Some("steel")).unwrap();
    graph.assembly.material_appearances.insert(
        "steel".into(),
        MaterialAppearance {
            base_color: [0.2, 0.3, 0.4, 0.5],
            metallic: 0.9,
            roughness: 0.2,
            double_sided: true,
        },
    );
    let mut second = definition("copy");
    second.face_tags.clear();
    let session = Session::new().unwrap();
    let text = graph
        .export_gltf(&session, &[definition("part"), second])
        .unwrap();
    let gltf: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(gltf["asset"]["version"], "2.0");
    assert_eq!(gltf["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(gltf["meshes"].as_array().unwrap().len(), 1);
    assert_eq!(gltf["materials"][0]["alphaMode"], "BLEND");
    assert_eq!(
        gltf["materials"][0]["pbrMetallicRoughness"]["metallicFactor"],
        0.9
    );
    assert_eq!(
        gltf["nodes"][0]["translation"],
        serde_json::json!([0.005, 0.015, -0.01])
    );
    assert_eq!(
        gltf["nodes"][1]["translation"],
        serde_json::json!([0.105, 0.015, -0.01])
    );
    assert_eq!(gltf["extras"]["generatedVariants"], 1);
    assert_eq!(gltf["accessors"][0]["count"], 36);
    assert_eq!(gltf["buffers"][0]["byteLength"], 864);
    assert_eq!(session.shape_count().unwrap(), 0);
    let before = ModelDocument::from_graph(&graph);
    let mut after = before.clone();
    after
        .assembly
        .material_appearances
        .get_mut("steel")
        .unwrap()
        .roughness = 0.7;
    assert!(
        before
            .change_impact(&after)
            .unwrap()
            .instances
            .iter()
            .all(|impact| impact.material_changed && impact.features.is_empty())
    );
    assert_eq!(
        ModelDocument::from_json(&before.to_json_pretty().unwrap()).unwrap(),
        before
    );
    if std::env::var_os("OCCT_MESH_QA").is_some() {
        std::fs::write("/tmp/occb-mesh-assembly.gltf", text).unwrap();
    }
}

#[test]
fn invalid_mesh_tags_settings_appearance_and_bundles_release_all_handles() {
    let family = model();
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let valid = definition("part");
    for variant in 0..7 {
        let mut bad = valid.clone();
        match variant {
            0 => bad.id.clear(),
            1 => bad.output.output = "missing".into(),
            2 => bad.settings.linear_deflection = Quantity::scalar(0.1),
            3 => bad.face_tags[0].faces.clear(),
            4 => bad.face_tags[0].id = "line\ninjection".into(),
            5 => {
                let tag = bad.face_tags[0].clone();
                bad.face_tags.push(tag);
            }
            _ => {
                let mut tag = bad.face_tags[0].clone();
                tag.id = "overlap".into();
                bad.face_tags.push(tag);
            }
        }
        assert!(bad.generate(&graph, &session).is_err(), "variant {variant}");
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let appearance = MaterialAppearance {
        roughness: f64::NAN,
        ..MaterialAppearance::default()
    };
    let mesh = valid.generate(&graph, &session).unwrap();
    assert!(mesh.to_gltf(appearance).is_err());
    let mut collapsed = mesh.clone();
    collapsed.triangles = vec![occt_bridge::MeshTriangle {
        face_index: 0,
        points: [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1e10, 0.0, 0.0),
            Vec3::new(1e10, 1e-44, 0.0),
        ],
    }];
    assert!(
        collapsed
            .to_gltf(MaterialAppearance::default())
            .unwrap_err()
            .to_string()
            .contains("float32 conversion")
    );
    assert!(collapsed.to_msh().is_err());
    assert!(graph.export_gltf(&session, &[]).is_err());
    assert!(
        graph
            .export_gltf(&session, &[valid.clone(), valid.clone()])
            .is_err()
    );
    let settings = ManufacturingSettings {
        maximum_wall_samples: 0,
        ..ManufacturingSettings::default()
    };
    assert!(mesh.check_manufacturability(settings).is_err());
    let directory =
        std::env::temp_dir().join(format!("occt-existing-bundle-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("preserve"), "original").unwrap();
    assert!(
        valid
            .write_fea_bundle(&graph, &session, &directory)
            .is_err()
    );
    assert_eq!(
        std::fs::read(directory.join("preserve")).unwrap(),
        b"original"
    );
    std::fs::remove_dir_all(directory).unwrap();
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut document = ModelDocument::from_graph(&graph);
    document.assembly.material_appearances =
        BTreeMap::from([("missing".into(), MaterialAppearance::default())]);
    assert!(document.to_json_pretty().is_err());
}

#[test]
fn bore_boundaries_and_tapered_draft_keep_physical_geometry() {
    let mut family = model();
    family.features.push(FeatureDefinition {
        id: "tool".into(),
        operation: FeatureOperation::Cylinder {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                5.0,
                10.0,
                -1.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            radius: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
            height: ScalarExpr::Literal(Quantity::length(32.0, LengthUnit::Millimeter)),
        },
    });
    family.features.push(FeatureDefinition {
        id: "drilled".into(),
        operation: FeatureOperation::Cut {
            object: "body".into(),
            tool: "tool".into(),
        },
    });
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mut definition = definition("part");
    definition.output.output = "drilled".into();
    definition.face_tags.push(MeshFaceTag {
        id: "bore".into(),
        faces: vec![FaceSelector::NearestCenter {
            target: VectorExpr::Literal(VectorQuantity::lengths(
                5.0,
                10.0,
                15.0,
                LengthUnit::Millimeter,
            )),
            maximum_distance: ScalarExpr::Literal(Quantity::length(0.01, LengthUnit::Millimeter)),
        }],
    });
    let mesh = definition.generate(&graph, &session).unwrap();
    assert_eq!(mesh.faces.len(), 7);
    let bore = mesh
        .faces
        .iter()
        .find(|face| face.physical_tag == 2)
        .unwrap();
    assert!((bore.area_mm2 - 120.0 * std::f64::consts::PI).abs() < 1e-7);
    let report = mesh
        .check_manufacturability(ManufacturingSettings::default())
        .unwrap();
    assert_eq!(report.unresolved_wall_samples, 0);
    assert!(
        report.minimum_sampled_wall_mm.unwrap() > 2.5
            && report.minimum_sampled_wall_mm.unwrap() < 3.1
    );
    if std::env::var_os("OCCT_MESH_QA").is_some() {
        definition
            .write_fea_bundle(&graph, &session, std::path::Path::new("/tmp/occb-fea-hole"))
            .unwrap();
        std::fs::write(
            "/tmp/occb-mesh-hole.gltf",
            mesh.to_gltf(MaterialAppearance::default()).unwrap(),
        )
        .unwrap();
    }
    let cone = session
        .create_cone(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            10.0,
            5.0,
            30.0,
        )
        .unwrap();
    let triangles = session
        .surface_mesh(
            &cone,
            occt_bridge::MeshOptions {
                linear_deflection: 0.01,
                angular_deflection_radians: 0.1,
                ..occt_bridge::MeshOptions::default()
            },
        )
        .unwrap();
    let mesh = TaggedSurfaceMesh {
        id: "cone".into(),
        triangles,
        face_tags: vec![0; 3],
        names: vec![],
        faces: vec![],
    };
    let report = mesh
        .check_manufacturability(ManufacturingSettings {
            maximum_wall_samples: 50,
            ..ManufacturingSettings::default()
        })
        .unwrap();
    assert_eq!(report.draft.len(), 1);
    assert!(report.draft[0].meets_minimum);
    assert!(
        (report.draft[0].minimum_signed_radians - (5.0_f64 / 30.0).atan()).abs() < 0.01,
        "{:?}",
        report.draft
    );
    assert_eq!(report.unresolved_wall_samples, 0);
    drop(cone);
    assert_eq!(session.shape_count().unwrap(), 0);
}

/// World-space bounds in millimeters (Z up) of one glTF node's mesh.
fn gltf_node_bounds(gltf: &serde_json::Value, node: &serde_json::Value) -> ([f64; 3], [f64; 3]) {
    let accessor =
        &gltf["accessors"][gltf["meshes"][node["mesh"].as_u64().unwrap() as usize]["primitives"][0]
            ["attributes"]["POSITION"]
            .as_u64()
            .unwrap() as usize];
    let view = &gltf["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
    let uri = gltf["buffers"][0]["uri"].as_str().unwrap();
    let encoded = uri.split_once(',').unwrap().1.as_bytes();
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    } as u32;
    let mut bytes = Vec::new();
    for chunk in encoded.chunks(4) {
        let n = chunk.iter().take_while(|c| **c != b'=').count();
        let word = chunk[..n].iter().fold(0, |acc, c| (acc << 6) | value(*c)) << (6 * (4 - n));
        bytes.extend(&word.to_be_bytes()[1..n]);
    }
    let start = view["byteOffset"].as_u64().unwrap() as usize;
    let count = accessor["count"].as_u64().unwrap() as usize;
    let floats: Vec<f64> = bytes[start..start + count * 12]
        .chunks(4)
        .map(|b| f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])))
        .collect();
    let matrix: Vec<f64> = match node.get("matrix") {
        Some(m) => m
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect(),
        None => {
            let t: Vec<f64> = node["translation"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            vec![
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, t[0], t[1], t[2], 1.0,
            ]
        }
    };
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for p in floats.chunks(3) {
        let g: [f64; 3] = std::array::from_fn(|row| {
            (0..3).map(|k| matrix[4 * k + row] * p[k]).sum::<f64>() + matrix[12 + row]
        });
        // glTF (x, y, z) = model (x, z, -y) in meters.
        let model = [g[0] * 1000.0, -g[2] * 1000.0, g[1] * 1000.0];
        for axis in 0..3 {
            low[axis] = low[axis].min(model[axis]);
            high[axis] = high[axis].max(model[axis]);
        }
    }
    (low, high)
}

#[test]
fn gltf_tessellates_shared_variants_once_and_places_rotated_framed_clones() {
    let family = model();
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let turned = |axis: VectorQuantity, angle: f64, x: f64| Placement {
        translation: VectorQuantity::lengths(x, 7.0, -3.0, LengthUnit::Millimeter),
        rotation: Some(AxisAngle {
            origin: VectorQuantity::lengths(5.0, 5.0, 5.0, LengthUnit::Millimeter),
            axis,
            angle_radians: angle,
        }),
    };
    graph
        .add_frame(
            "arm",
            None,
            turned(VectorQuantity::scalars(0.0, 1.0, 0.0), 0.4, 1e5),
            "test",
        )
        .unwrap();
    for (id, placement, frame) in [
        (
            "turned",
            turned(VectorQuantity::scalars(0.0, 0.0, 1.0), 1.1, 200.0),
            None,
        ),
        (
            "tilted",
            turned(VectorQuantity::scalars(1.0, 2.0, 3.0), 2.3, -150.0),
            None,
        ),
        (
            "framed",
            turned(VectorQuantity::scalars(1.0, 0.0, 0.0), 0.7, 40.0),
            Some("arm"),
        ),
        (
            "moved",
            Placement::translated(VectorQuantity::lengths(
                0.0,
                90.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            None,
        ),
    ] {
        graph.add_clone(id, "part", HashMap::new(), "test").unwrap();
        graph.set_placement(id, placement).unwrap();
        if frame.is_some() {
            graph.set_instance_frame(id, frame).unwrap();
        }
    }
    let session = Session::new().unwrap();
    let gltf: serde_json::Value = serde_json::from_str(
        &graph
            .export_gltf_output(&session, "body", MeshSettings::default())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(gltf["meshes"].as_array().unwrap().len(), 1);
    // The first instance (by id) is tessellated in place; the others are
    // rotated relative to it, so they carry matrices.
    let matrices = gltf["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n.get("matrix").is_some());
    assert_eq!(matrices.count(), 4);
    let generation = graph.regenerate_all(&session).unwrap();
    for node in gltf["nodes"].as_array().unwrap() {
        let name = node["name"].as_str().unwrap();
        let shape = generation.result(name).unwrap().shape("body").unwrap();
        let exact = session.exact_bounds(shape).unwrap();
        let (low, high) = gltf_node_bounds(&gltf, node);
        let expected = [
            [exact.min.x, exact.min.y, exact.min.z],
            [exact.max.x, exact.max.y, exact.max.z],
        ];
        for axis in 0..3 {
            // float32 positions about 1e5 mm from the origin round near 1e-2 mm.
            assert!(
                (low[axis] - expected[0][axis]).abs() < 0.02,
                "{name} {low:?} {expected:?}"
            );
            assert!(
                (high[axis] - expected[1][axis]).abs() < 0.02,
                "{name} {high:?} {expected:?}"
            );
        }
    }
    drop(generation);
    assert_eq!(session.shape_count().unwrap(), 0);
}
