use super::*;

fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn scalar(value: Quantity) -> JointScalar {
    JointScalar {
        value,
        minimum: None,
        maximum: None,
    }
}
fn joint(frame: &str, kind: JointKind) -> AssemblyJoint {
    AssemblyJoint {
        id: format!("{frame}.joint"),
        frame: frame.into(),
        origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
        kind,
    }
}
fn output(instance: &str) -> InstanceOutputRef {
    InstanceOutputRef {
        instance: instance.into(),
        output: "body".into(),
    }
}
fn definition() -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition
}

#[test]
fn assembly_mass_properties_use_materials_current_joints_and_central_tensors() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_material(Material {
            id: "material".into(),
            name: "Test density".into(),
            density_kg_per_cubic_meter: 1000.0,
        })
        .unwrap();
    graph.add_base("first", HashMap::new(), "test").unwrap();
    graph.assign_material("first", Some("material")).unwrap();
    graph
        .add_clone("second", "first", HashMap::new(), "test")
        .unwrap();
    graph
        .add_frame("slide", None, Placement::identity(), "test")
        .unwrap();
    graph
        .add_joint(joint(
            "slide",
            JointKind::Prismatic {
                distance: scalar(mm(20.0)),
            },
        ))
        .unwrap();
    graph.set_instance_frame("second", Some("slide")).unwrap();
    let origin = 1_000_000.0;
    for id in ["first", "second"] {
        graph
            .set_placement(
                id,
                Placement::translated(VectorQuantity::lengths(
                    origin,
                    origin,
                    origin,
                    LengthUnit::Millimeter,
                )),
            )
            .unwrap();
    }
    let accepted = ModelDocument::from_graph(&graph);
    let session = Session::new().unwrap();
    let report = graph
        .mass_properties(&session, &[output("first"), output("second")])
        .unwrap();
    assert_eq!(report.generated_variants, 1);
    assert_eq!(report.components[1].material, "material");
    assert!((report.total.mass_kg - 0.012).abs() < 1e-12);
    assert!((report.total.volume_mm3 - 12000.0).abs() < 1e-6);
    assert!((report.total.center_mm.x - origin - 15.0).abs() < 1e-7);
    assert!((report.total.center_mm.y - origin - 10.0).abs() < 1e-7);
    assert!((report.total.center_mm.z - origin - 15.0).abs() < 1e-7);
    for (axis, expected) in [1.3, 2.2, 1.7].into_iter().enumerate() {
        assert!(
            (report.total.inertia_kg_mm2[axis][axis] - expected).abs() < 1e-8,
            "{:?}",
            report.total
        );
    }
    for row in 0..3 {
        for column in 0..3 {
            if row != column {
                assert!(report.total.inertia_kg_mm2[row][column].abs() < 1e-8);
            }
        }
    }
    assert_eq!(ModelDocument::from_graph(&graph), accepted);
    assert_eq!(session.shape_count().unwrap(), 0);
    graph
        .add_material(Material {
            id: "heavy".into(),
            name: "Heavy".into(),
            density_kg_per_cubic_meter: 2000.0,
        })
        .unwrap();
    graph.assign_material("second", Some("heavy")).unwrap();
    let report = graph
        .mass_properties(&session, &[output("first"), output("second")])
        .unwrap();
    assert!((report.total.mass_kg - 0.018).abs() < 1e-12);
    assert!((report.total.center_mm.x - origin - (5.0 + 40.0 / 3.0)).abs() < 1e-7);
    assert!(graph.mass_properties(&session, &[]).is_err());
    assert!(
        graph
            .mass_properties(&session, &[output("first"), output("first")])
            .is_err()
    );
    let mut missing = output("first");
    missing.output = "missing".into();
    assert!(graph.mass_properties(&session, &[missing]).is_err());
    graph.assign_material("first", None).unwrap();
    assert!(graph.mass_properties(&session, &[output("first")]).is_err());
    graph
        .add_material(Material {
            id: "underflow".into(),
            name: "Unrepresentable density".into(),
            density_kg_per_cubic_meter: 1e-320,
        })
        .unwrap();
    graph.assign_material("first", Some("underflow")).unwrap();
    assert!(graph.mass_properties(&session, &[output("first")]).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn component_mass_tensor_rotates_with_its_joint() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    graph
        .add_material(Material {
            id: "material".into(),
            name: "Test density".into(),
            density_kg_per_cubic_meter: 1000.0,
        })
        .unwrap();
    graph.assign_material("part", Some("material")).unwrap();
    graph
        .add_frame("hinge", None, Placement::identity(), "test")
        .unwrap();
    graph.set_instance_frame("part", Some("hinge")).unwrap();
    let mut hinge = joint(
        "hinge",
        JointKind::Revolute {
            angle: scalar(Quantity::scalar(std::f64::consts::FRAC_PI_4)),
        },
    );
    hinge.axis = VectorQuantity::scalars(0.0, 0.0, 1.0);
    graph.add_joint(hinge).unwrap();
    let session = Session::new().unwrap();
    let report = graph.mass_properties(&session, &[output("part")]).unwrap();
    let tensor = report.total.inertia_kg_mm2;
    assert!((tensor[0][0] - 0.575).abs() < 1e-10);
    assert!((tensor[1][1] - 0.575).abs() < 1e-10);
    assert!((tensor[0][1] - 0.075).abs() < 1e-10);
    assert_eq!(tensor[0][1], tensor[1][0]);
    assert!((tensor[2][2] - 0.25).abs() < 1e-10);
    assert_eq!(session.shape_count().unwrap(), 0);
}
fn bounds(graph: &InstanceGraph<'_>) -> occt_bridge::Bounds {
    let session = Session::new().unwrap();
    let result = graph
        .resolve_with_placement("moving")
        .unwrap()
        .regenerate(&session)
        .unwrap();
    session.exact_bounds(result.shape("body").unwrap()).unwrap()
}

#[test]
fn joints_cover_all_kinds_parent_coordinates_and_persistence() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame(
            "parent",
            None,
            Placement::translated(VectorQuantity::lengths(
                100.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            "test",
        )
        .unwrap();
    graph
        .add_frame("moving", Some("parent"), Placement::identity(), "test")
        .unwrap();
    graph.add_base("moving", HashMap::new(), "test").unwrap();
    graph.set_instance_frame("moving", Some("moving")).unwrap();
    let cases = [
        (JointKind::Fixed, Vec3::new(100.0, 0.0, 0.0)),
        (
            JointKind::Prismatic {
                distance: scalar(Quantity::length(2.0, LengthUnit::Centimeter)),
            },
            Vec3::new(120.0, 0.0, 0.0),
        ),
        (
            JointKind::Revolute {
                angle: scalar(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
            },
            Vec3::new(100.0, -30.0, 0.0),
        ),
        (
            JointKind::Cylindrical {
                angle: scalar(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
                distance: scalar(mm(20.0)),
            },
            Vec3::new(120.0, -30.0, 0.0),
        ),
        (
            JointKind::Planar {
                x_axis: VectorQuantity::scalars(0.0, 1.0, 0.0),
                x: scalar(mm(3.0)),
                y: scalar(mm(4.0)),
                angle: scalar(Quantity::scalar(0.0)),
            },
            Vec3::new(100.0, 3.0, 4.0),
        ),
    ];
    for (kind, minimum) in cases {
        graph.add_joint(joint("moving", kind)).unwrap();
        let result = bounds(&graph);
        assert!((result.min.x - minimum.x).abs() < 1e-6, "{result:?}");
        assert!((result.min.y - minimum.y).abs() < 1e-6, "{result:?}");
        assert!((result.min.z - minimum.z).abs() < 1e-6, "{result:?}");
        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
        assert_eq!(loaded.instance_graph().unwrap().joints().count(), 1);
        graph.remove_joint("moving").unwrap();
    }
    // Child axial translation is rotated by the parent joint.
    let mut parent = joint(
        "parent",
        JointKind::Revolute {
            angle: scalar(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
        },
    );
    parent.axis = VectorQuantity::scalars(0.0, 0.0, 1.0);
    graph
        .add_joints([
            parent,
            joint(
                "moving",
                JointKind::Prismatic {
                    distance: scalar(mm(20.0)),
                },
            ),
        ])
        .unwrap();
    let result = bounds(&graph);
    assert!((result.min.x + 20.0).abs() < 1e-6);
    assert!((result.min.y - 120.0).abs() < 1e-6);
}

#[test]
fn invalid_joint_edits_and_batches_preserve_the_accepted_state() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame("slide", None, Placement::identity(), "test")
        .unwrap();
    let bounded = JointScalar {
        value: mm(5.0),
        minimum: Some(mm(0.0)),
        maximum: Some(mm(10.0)),
    };
    graph
        .add_joint(joint("slide", JointKind::Prismatic { distance: bounded }))
        .unwrap();
    let accepted = ModelDocument::from_graph(&graph);
    for value in [mm(-1.0), mm(11.0), mm(f64::NAN), Quantity::scalar(2.0)] {
        assert!(
            graph
                .set_joint_coordinate("slide", JointDof::Axial, value)
                .is_err()
        );
        assert_eq!(ModelDocument::from_graph(&graph), accepted);
    }
    assert!(
        graph
            .set_joint_coordinate("slide", JointDof::Angle, Quantity::scalar(1.0))
            .is_err()
    );
    assert!(
        graph
            .add_joints([joint("missing", JointKind::Fixed)])
            .is_err()
    );
    assert!(graph.add_joint(joint("slide", JointKind::Fixed)).is_err());
    graph
        .add_frame("bad", None, Placement::identity(), "test")
        .unwrap();
    let mut invalid = joint("bad", JointKind::Fixed);
    invalid.axis = VectorQuantity::scalars(0.0, 0.0, 0.0);
    assert!(graph.add_joint(invalid).is_err());
    let mut invalid = joint("bad", JointKind::Fixed);
    invalid.origin = VectorQuantity::lengths(f64::INFINITY, 0.0, 0.0, LengthUnit::Millimeter);
    assert!(graph.add_joint(invalid).is_err());
    let invalid = joint(
        "bad",
        JointKind::Planar {
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            x: scalar(mm(0.0)),
            y: scalar(mm(0.0)),
            angle: scalar(Quantity::scalar(0.0)),
        },
    );
    assert!(graph.add_joint(invalid).is_err());
    graph
        .set_joint_coordinate("slide", JointDof::Axial, mm(10.0))
        .unwrap();
    assert_eq!(graph.joints().count(), 1);
}

#[test]
fn collision_bvh_matches_all_pairs_and_releases_query_temporaries() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    let locations = [0.0, 5.0, 10.0, 12.0, 30.0, 100.0, -10.0];
    let mut outputs = Vec::new();
    for (index, x) in locations.iter().enumerate() {
        let id = format!("part{index}");
        graph.add_base(&id, HashMap::new(), "test").unwrap();
        graph
            .set_placement(
                &id,
                Placement::translated(VectorQuantity::lengths(
                    *x,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            )
            .unwrap();
        outputs.push(output(&id));
    }
    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    let handles = session.shape_count().unwrap();
    let options = CollisionOptions {
        minimum_clearance: Quantity::length(0.3, LengthUnit::Centimeter),
        ..Default::default()
    };
    let indexed = generation
        .check_collisions(&session, &outputs, options)
        .unwrap();
    let mut exhaustive = Vec::new();
    for first in 0..outputs.len() {
        for second in first + 1..outputs.len() {
            let check = generation
                .check_pair(&session, &outputs[first], &outputs[second], options)
                .unwrap();
            if check.status != PairStatus::Clear {
                exhaustive.push(check);
            }
        }
    }
    assert_eq!(indexed, exhaustive);
    assert!(
        indexed
            .iter()
            .any(|check| check.status == PairStatus::Interference)
    );
    assert!(
        indexed
            .iter()
            .any(|check| check.status == PairStatus::InsufficientClearance)
    );
    let touching = generation
        .check_pair(
            &session,
            &outputs[0],
            &outputs[2],
            CollisionOptions::default(),
        )
        .unwrap();
    assert_eq!(touching.status, PairStatus::Touching);
    assert_eq!(touching.overlap_volume_mm3, 0.0);
    let overlap = generation
        .check_pair(
            &session,
            &outputs[0],
            &outputs[1],
            CollisionOptions::default(),
        )
        .unwrap();
    assert!((overlap.overlap_volume_mm3 - 3000.0).abs() < 1e-6);
    assert!(
        generation
            .check_collisions(&session, &[outputs[0].clone(), outputs[0].clone()], options)
            .is_err()
    );
    assert!(
        generation
            .check_collisions(&session, &[output("missing")], options)
            .is_err()
    );
    assert!(
        generation
            .check_collisions(
                &session,
                &outputs,
                CollisionOptions {
                    contact_tolerance: Quantity::scalar(0.0),
                    ..options
                }
            )
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), handles);
    drop(generation);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn motion_samples_reuse_local_geometry_report_crossings_and_preserve_graph() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("moving", HashMap::new(), "test").unwrap();
    graph
        .add_clone("obstacle", "moving", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "obstacle",
            Placement::translated(VectorQuantity::lengths(
                20.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph
        .add_frame("slide", None, Placement::identity(), "test")
        .unwrap();
    graph.set_instance_frame("moving", Some("slide")).unwrap();
    graph
        .add_joint(joint(
            "slide",
            JointKind::Prismatic {
                distance: JointScalar {
                    value: mm(0.0),
                    minimum: Some(mm(0.0)),
                    maximum: Some(mm(40.0)),
                },
            },
        ))
        .unwrap();
    let accepted = ModelDocument::from_graph(&graph);
    let session = Session::new().unwrap();
    let study = MotionStudy::linear(
        "slide",
        JointDof::Axial,
        mm(0.0),
        Quantity::length(4.0, LengthUnit::Centimeter),
        5,
        vec![output("moving"), output("obstacle")],
        CollisionOptions::default(),
    )
    .unwrap();
    let result = graph.run_motion_study(&session, &study).unwrap();
    assert_eq!(result.generated_variants, 1);
    assert_eq!(result.samples.len(), 5);
    assert!(result.samples[0].collisions.is_empty());
    assert_eq!(result.samples[1].collisions[0].status, PairStatus::Touching);
    assert_eq!(
        result.samples[2].collisions[0].status,
        PairStatus::Interference
    );
    assert_eq!(result.samples[3].collisions[0].status, PairStatus::Touching);
    assert!(result.samples[4].collisions.is_empty());
    assert_eq!(ModelDocument::from_graph(&graph), accepted);
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut invalid = study.clone();
    invalid.samples[4].positions[0].value = mm(41.0);
    assert!(
        graph
            .run_motion_study(&session, &invalid)
            .unwrap_err()
            .message
            .contains("sample 4")
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    invalid.samples[4].positions[0].value = mm(40.0);
    let repeated = invalid.samples[0].positions[0].clone();
    invalid.samples[0].positions.push(repeated);
    assert!(graph.run_motion_study(&session, &invalid).is_err());
    invalid = study.clone();
    invalid.outputs[0].output = "missing".into();
    assert!(graph.run_motion_study(&session, &invalid).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(
        MotionStudy::linear(
            "slide",
            JointDof::Axial,
            mm(0.0),
            mm(1.0),
            MAX_MOTION_SAMPLES + 1,
            study.outputs,
            CollisionOptions::default()
        )
        .is_err()
    );
}

#[test]
fn joint_batches_limits_and_documents_reject_inconsistent_joints() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    for frame in ["a", "b"] {
        graph
            .add_frame(frame, None, Placement::identity(), "test")
            .unwrap();
    }
    let prismatic = |distance| JointKind::Prismatic { distance };

    // A failure late in a batch keeps none of its earlier joints.
    let error = graph
        .add_joints([
            joint("a", JointKind::Fixed),
            joint("missing", JointKind::Fixed),
        ])
        .unwrap_err();
    assert!(error.message.contains("unknown assembly frame 'missing'"));
    assert_eq!(graph.joints().count(), 0);

    // Joint ids are unique across frames, within and across batches.
    let mut duplicate = joint("b", JointKind::Fixed);
    duplicate.id = "a.joint".into();
    assert!(
        graph
            .add_joints([joint("a", JointKind::Fixed), duplicate.clone()])
            .unwrap_err()
            .message
            .contains("joint ids must be unique")
    );
    graph.add_joint(joint("a", JointKind::Fixed)).unwrap();
    assert!(graph.add_joint(duplicate).is_err());

    let reversed = JointScalar {
        value: mm(5.0),
        minimum: Some(mm(10.0)),
        maximum: Some(mm(0.0)),
    };
    let error = graph
        .add_joint(joint("b", prismatic(reversed)))
        .unwrap_err();
    assert!(error.message.contains("limits are reversed"));
    let wrong_limit = JointScalar {
        value: mm(5.0),
        minimum: Some(Quantity::scalar(0.0)),
        maximum: None,
    };
    let error = graph
        .add_joint(joint("b", prismatic(wrong_limit)))
        .unwrap_err();
    assert!(error.message.contains("wrong dimension"));
    let error = graph
        .set_joint_coordinate("b", JointDof::Axial, mm(1.0))
        .unwrap_err();
    assert!(error.message.contains("frame 'b' has no joint"));
    assert!(graph.remove_joint("b").is_err());

    // Hand-edited documents fail the same checks on load and on save.
    let bounded = JointScalar {
        value: mm(5.0),
        minimum: Some(mm(0.0)),
        maximum: Some(mm(10.0)),
    };
    graph.add_joint(joint("b", prismatic(bounded))).unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    let rejects = |edit: &dyn Fn(&mut ModelDocument), message: &str| {
        let mut edited = document.clone();
        edit(&mut edited);
        let error = edited.to_json_pretty().unwrap_err();
        assert!(error.message.contains(message), "{}", error.message);
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value["assembly"]["joints"] = serde_json::to_value(&edited.assembly.joints).unwrap();
        let error = ModelDocument::from_json(&value.to_string()).unwrap_err();
        assert!(error.message.contains(message), "{}", error.message);
    };
    rejects(
        &|edited| edited.assembly.joints.get_mut("b").unwrap().frame = "a".into(),
        "invalid joint id or frame binding",
    );
    rejects(
        &|edited| edited.assembly.joints.get_mut("b").unwrap().id = "a.joint".into(),
        "invalid joint id or frame binding",
    );
    rejects(
        &|edited| {
            let joint = edited.assembly.joints.remove("b").unwrap();
            edited.assembly.joints.insert("gone".into(), joint);
        },
        "invalid joint id or frame binding",
    );
    rejects(
        &|edited| {
            if let JointKind::Prismatic { distance } =
                &mut edited.assembly.joints.get_mut("b").unwrap().kind
            {
                distance.value = mm(11.0);
            }
        },
        "outside its limits",
    );
}
