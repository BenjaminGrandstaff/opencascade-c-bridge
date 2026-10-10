//! GD&T feature control frames, composite frames, and named datum reference frames.

use super::*;

fn gdt_page() -> DrawingDefinition {
    let mut page = hatched_slice("body");
    for (id, label, anchor, size) in [
        ("primary", "A", "origin", false),
        ("secondary", "B", "corner", true),
        ("tertiary", "C", "origin", true),
    ] {
        page.datum_features.push(DrawingDatumFeature {
            id: id.into(),
            label: label.into(),
            feature_of_size: size,
            attachment: DrawingGdtAttachment {
                view: "slice".into(),
                output: InstanceOutputRef {
                    instance: "part".into(),
                    output: "body".into(),
                },
                anchor: DatumRef::new("part", anchor),
                offset_mm: [20.0, 30.0],
            },
        });
    }
    page.feature_control_frames
        .push(DrawingFeatureControlFrame {
            size_limits: None,
            datum_reference_frame: None,
            refinement: None,
            id: "position".into(),
            attachment: DrawingGdtAttachment {
                view: "slice".into(),
                output: InstanceOutputRef {
                    instance: "part".into(),
                    output: "body".into(),
                },
                anchor: DatumRef::new("part", "corner"),
                offset_mm: [30.0, 40.0],
            },
            characteristic: GeometricCharacteristic::Position,
            tolerance: Quantity::length(0.1, LengthUnit::Millimeter),
            display_unit: LengthUnit::Millimeter,
            precision: 2,
            zone: GeometricToleranceZone::Diameter,
            material: ToleranceMaterialCondition::Maximum,
            feature_of_size: true,
            datums: vec![
                DrawingDatumReference {
                    datum_feature: "primary".into(),
                    boundary: DatumMaterialBoundary::Regardless,
                },
                DrawingDatumReference {
                    datum_feature: "secondary".into(),
                    boundary: DatumMaterialBoundary::Maximum,
                },
                DrawingDatumReference {
                    datum_feature: "tertiary".into(),
                    boundary: DatumMaterialBoundary::Least,
                },
            ],
        });
    page
}
#[test]
fn gdt_frames_render_all_characteristics_units_modifiers_and_ordered_datums() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mut page = gdt_page();
    for (i, c) in [
        GeometricCharacteristic::Straightness,
        GeometricCharacteristic::Flatness,
        GeometricCharacteristic::Circularity,
        GeometricCharacteristic::Cylindricity,
        GeometricCharacteristic::ProfileLine,
        GeometricCharacteristic::ProfileSurface,
        GeometricCharacteristic::Parallelism,
        GeometricCharacteristic::Perpendicularity,
        GeometricCharacteristic::Angularity,
        GeometricCharacteristic::Position,
        GeometricCharacteristic::CircularRunout,
        GeometricCharacteristic::TotalRunout,
    ]
    .into_iter()
    .enumerate()
    {
        let mut frame = page.feature_control_frames[0].clone();
        frame.id = format!("control-{i}");
        frame.characteristic = c;
        frame.attachment.offset_mm = [20.0, 20.0 + i as f64 * 14.0];
        if i < 4 {
            frame.datums.clear();
        }
        frame.zone = if c == GeometricCharacteristic::Position {
            GeometricToleranceZone::Diameter
        } else {
            GeometricToleranceZone::Characteristic
        };
        frame.material = ToleranceMaterialCondition::Regardless;
        page.feature_control_frames.push(frame);
    }
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(
        generated
            .gdt_lines
            .iter()
            .map(|l| l.points_mm.len())
            .sum::<usize>(),
        crate::drawing::gdt::vertex_count(
            &page.datum_features,
            &page.feature_control_frames,
            &page.datum_reference_frames
        )
        .unwrap()
    );
    assert!(generated.gdt_labels.iter().any(|l| l.text == "0.10 mm"));
    assert!(generated.gdt_labels.iter().any(|l| l.text == "M"));
    assert!(generated.gdt_labels.iter().any(|l| l.text == "L"));
    let texts: Vec<_> = generated
        .gdt_labels
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    assert!(texts.windows(5).any(|w| w == ["A", "B", "M", "C", "L"]));
    assert!(generated.to_svg().contains("0.10 mm"));
    assert!(generated.to_dxf().contains("0.10 mm"));
    page.feature_control_frames[0].display_unit = LengthUnit::Inch;
    page.feature_control_frames[0].precision = 4;
    let inches = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(inches.gdt_labels.iter().any(|l| l.text == "0.0039 in"));
    if std::env::var_os("OCCT_DRAWING_QA").is_some() {
        page.sheet = Some(standard_sheet(
            DrawingSheetSize::IsoA3,
            Some(ProjectionConvention::ThirdAngle),
        ));
        page.feature_control_frames.truncate(1);
        page.views[0].paper_origin_mm = [60.0, 130.0];
        page.views[0].scale = 3.0;
        for (i, f) in page.datum_features.iter_mut().enumerate() {
            f.attachment.offset_mm = [-20.0, 15.0 + i as f64 * 10.0];
        }
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        std::fs::write("/tmp/occb-gdt.svg", generated.to_svg()).unwrap();
        std::fs::write("/tmp/occb-gdt.dxf", generated.to_dxf()).unwrap();
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn gdt_rejects_invalid_references_units_zones_material_rules_and_budgets() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    for case in 0..17 {
        let mut page = gdt_page();
        let f = &mut page.feature_control_frames[0];
        match case {
            0 => f.tolerance = Quantity::scalar(0.1),
            1 => f.tolerance.value = -0.1,
            2 => f.tolerance.value = f64::INFINITY,
            3 => f.precision = 9,
            4 => f.tolerance.value = 1e-8,
            5 => f.zone = GeometricToleranceZone::Characteristic,
            6 => f.feature_of_size = false,
            7 => f.datums.clear(),
            8 => f.characteristic = GeometricCharacteristic::Flatness,
            9 => {
                f.characteristic = GeometricCharacteristic::ProfileSurface;
                f.zone = GeometricToleranceZone::Characteristic;
            }
            10 => f.datums[1].datum_feature = "unknown".into(),
            11 => f.datums[1] = f.datums[0].clone(),
            12 => f.datums[0].boundary = DatumMaterialBoundary::Maximum,
            13 => f.attachment.output.output = "missing".into(),
            14 => f.attachment.anchor.instance = "missing".into(),
            15 => f.attachment.offset_mm = [0.0, 0.0],
            _ => f.attachment.view = "missing".into(),
        }
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err(),
            "case {case}"
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    for case in 0..7 {
        let mut page = gdt_page();
        match case {
            0 => page.datum_features[0].label = "a".into(),
            1 => page.datum_features[1].label = "A".into(),
            2 => page.datum_features[1].id = "primary".into(),
            4 => page.datum_features[0].label = "I".into(),
            5 => page.datum_features[0].label = "O".into(),
            6 => page.datum_features[0].label = "Q".into(),
            _ => page
                .feature_control_frames
                .push(page.feature_control_frames[0].clone()),
        };
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
    }
    let page = gdt_page();
    assert!(
        page.generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_tolerance_mm: 0.01,
                exact_curves: false,
                curve_samples: 2,
                maximum_vertices: 100
            }
        )
        .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn gdt_persists_migrates_and_merges_stable_ids_preserving_datum_precedence() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(gdt_page());
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    let mut reorder = document.clone();
    reorder.drawings[0].datum_features.reverse();
    assert!(document.semantic_diff(&reorder).unwrap().is_empty());
    reorder.drawings[0].feature_control_frames[0]
        .datums
        .reverse();
    assert!(!document.semantic_diff(&reorder).unwrap().is_empty());
    let mut ours = document.clone();
    ours.drawings[0].feature_control_frames[0].tolerance.value = 0.2;
    let changes = document.semantic_diff(&ours).unwrap();
    assert!(
        changes[0]
            .path
            .contains(&DocumentPathSegment::Entity("position".into()))
    );
    let mut theirs = document.clone();
    theirs.drawings[0].datum_features[1].attachment.offset_mm = [40.0, 30.0];
    let DocumentMerge::Merged(merged) =
        ModelDocument::three_way_merge(&document, &ours, &theirs).unwrap()
    else {
        panic!("independent GD&T edits should merge")
    };
    assert_eq!(
        merged.drawings[0].feature_control_frames[0].tolerance.value,
        0.2
    );
    assert_eq!(
        merged.drawings[0]
            .datum_features
            .iter()
            .find(|f| f.id == "secondary")
            .unwrap()
            .attachment
            .offset_mm,
        [40.0, 30.0]
    );
    let mut old: serde_json::Value =
        serde_json::from_str(&document.to_json_pretty().unwrap()).unwrap();
    old["schema_version"] = 61.into();
    let drawing = old["drawings"][0].as_object_mut().unwrap();
    drawing.remove("datum_features");
    drawing.remove("feature_control_frames");
    let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
    assert!(migrated.drawings[0].datum_features.is_empty());
    assert!(migrated.drawings[0].feature_control_frames.is_empty());
}
#[test]
fn gdt_leaders_follow_datum_edits_scale_and_detail_offsets_with_fixed_paper_frames() {
    let mut definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mut page = gdt_page();
    let before = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    page.views[0].scale = 2.0;
    page.views[0].paper_origin_mm = [50.0, 60.0];
    page.views[0].detail = Some(DrawingDetail {
        minimum_mm: [2.0, 3.0],
        maximum_mm: [8.0, 7.0],
    });
    let after = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let b = &before.gdt_lines[0].points_mm;
    let a = &after.gdt_lines[0].points_mm;
    assert!((a[1][0] - a[0][0] - (b[1][0] - b[0][0])).abs() < 1e-8);
    assert!((a[2][1] - a[1][1] - 8.0).abs() < 1e-8);
    assert_ne!(before.gdt_lines, after.gdt_lines);
    drop(graph);
    definition.datums[0].kind = DatumKind::Point {
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            3.0,
            4.0,
            0.0,
            LengthUnit::Millimeter,
        )),
    };
    let mut edited = InstanceGraph::new(&definition);
    edited.add_base("part", HashMap::new(), "test").unwrap();
    let regenerated = page
        .generate(&edited, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_ne!(after.gdt_lines[0], regenerated.gdt_lines[0]);
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn composite_gdt_page() -> DrawingDefinition {
    let mut page = gdt_page();
    let f = &mut page.feature_control_frames[0];
    f.refinement = Some(DrawingCompositeRefinement {
        tolerance: Quantity::length(0.05, LengthUnit::Millimeter),
        datums: vec![f.datums[0].clone()],
    });
    page.datum_reference_frames
        .push(DrawingDatumReferenceFrame {
            id: "ABC".into(),
            datums: std::mem::take(&mut f.datums),
        });
    f.datum_reference_frame = Some("ABC".into());
    page
}
#[test]
fn composite_frames_export_one_shared_symbol_two_tolerance_rows_and_exact_budgets() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mut page = composite_gdt_page();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(generated.gdt_labels.iter().any(|l| l.text == "0.10 mm"));
    assert!(generated.gdt_labels.iter().any(|l| l.text == "0.05 mm"));
    assert_eq!(
        generated
            .gdt_lines
            .iter()
            .map(|l| l.points_mm.len())
            .sum::<usize>(),
        crate::drawing::gdt::vertex_count(
            &page.datum_features,
            &page.feature_control_frames,
            &page.datum_reference_frames
        )
        .unwrap()
    );
    let outline = &generated.gdt_lines[9].points_mm;
    assert_eq!(outline.len(), 7);
    assert!((outline[5][1] - outline[0][1] - 16.0).abs() < 1e-8);
    let divider = &generated.gdt_lines[11].points_mm;
    assert!((divider[0][0] - outline[0][0] - 8.0).abs() < 1e-8);
    assert_eq!(
        generated
            .gdt_lines
            .iter()
            .filter(|l| l.points_mm.len() == 33)
            .count(),
        7
    ); // One characteristic, two diameter marks, four modifiers.
    assert!(generated.to_svg().contains("0.05 mm"));
    assert!(generated.to_dxf().contains("0.05 mm"));
    assert!(generated.to_dxf().contains("8\nGD_T\n"));
    for c in [
        GeometricCharacteristic::ProfileLine,
        GeometricCharacteristic::ProfileSurface,
    ] {
        let frame = &mut page.feature_control_frames[0];
        frame.characteristic = c;
        frame.zone = GeometricToleranceZone::Characteristic;
        frame.material = ToleranceMaterialCondition::Regardless;
        frame.refinement.as_mut().unwrap().datums.clear();
        let profiles = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        assert!(profiles.gdt_labels.iter().any(|l| l.text == "0.05 mm"));
        assert_eq!(
            profiles
                .gdt_lines
                .iter()
                .map(|l| l.points_mm.len())
                .sum::<usize>(),
            crate::drawing::gdt::vertex_count(
                &page.datum_features,
                &page.feature_control_frames,
                &page.datum_reference_frames
            )
            .unwrap()
        );
    }
    assert!(
        page.generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_tolerance_mm: 0.01,
                exact_curves: false,
                curve_samples: 2,
                maximum_vertices: 100
            }
        )
        .is_err()
    );
    if std::env::var_os("OCCT_DRAWING_QA").is_some() {
        page = composite_gdt_page();
        page.sheet = Some(standard_sheet(
            DrawingSheetSize::IsoA3,
            Some(ProjectionConvention::ThirdAngle),
        ));
        page.views[0].paper_origin_mm = [60.0, 130.0];
        page.views[0].scale = 3.0;
        for (i, f) in page.datum_features.iter_mut().enumerate() {
            f.attachment.offset_mm = [-20.0, 15.0 + i as f64 * 10.0];
        }
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        std::fs::write("/tmp/occb-composite-gdt.svg", generated.to_svg()).unwrap();
        std::fs::write("/tmp/occb-composite-gdt.dxf", generated.to_dxf()).unwrap();
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn composite_and_named_datum_frames_reject_unsupported_or_ambiguous_semantics() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    for case in 0..12 {
        let mut page = composite_gdt_page();
        let f = &mut page.feature_control_frames[0];
        let lower = f.refinement.as_mut().unwrap();
        match case {
            0 => lower.tolerance.value = 0.2,
            1 => lower.tolerance.value = -0.1,
            2 => lower.tolerance = Quantity::scalar(0.01),
            3 => lower.tolerance.value = 0.099,
            4 => f.characteristic = GeometricCharacteristic::Parallelism,
            5 => lower.datums = vec![page.datum_reference_frames[0].datums[1].clone()],
            6 => lower.datums[0].boundary = DatumMaterialBoundary::Maximum,
            7 => f.datum_reference_frame = Some("missing".into()),
            8 => f.datums = vec![page.datum_reference_frames[0].datums[0].clone()],
            9 => page.datum_reference_frames[0].datums.clear(),
            10 => page
                .datum_reference_frames
                .push(page.datum_reference_frames[0].clone()),
            _ => page.datum_reference_frames[0].datums[0].datum_feature = "missing".into(),
        }
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err(),
            "case {case}"
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
#[test]
fn composite_named_frames_persist_migrate_and_merge_with_ordered_reference_semantics() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(composite_gdt_page());
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    let mut ours = document.clone();
    ours.drawings[0].feature_control_frames[0]
        .refinement
        .as_mut()
        .unwrap()
        .tolerance
        .value = 0.03;
    let mut theirs = document.clone();
    theirs.drawings[0].datum_reference_frames[0].datums[1].boundary =
        DatumMaterialBoundary::Regardless;
    let changes = document.semantic_diff(&theirs).unwrap();
    assert!(
        changes[0]
            .path
            .contains(&DocumentPathSegment::Entity("ABC".into()))
    );
    let DocumentMerge::Merged(merged) =
        ModelDocument::three_way_merge(&document, &ours, &theirs).unwrap()
    else {
        panic!("independent composite and named-frame edits should merge")
    };
    assert_eq!(
        merged.drawings[0].feature_control_frames[0]
            .refinement
            .as_ref()
            .unwrap()
            .tolerance
            .value,
        0.03
    );
    assert_eq!(
        merged.drawings[0].datum_reference_frames[0].datums[1].boundary,
        DatumMaterialBoundary::Regardless
    );
    let mut reorder = document.clone();
    reorder.drawings[0].datum_reference_frames[0]
        .datums
        .reverse();
    assert!(!document.semantic_diff(&reorder).unwrap().is_empty());
    let mut old: serde_json::Value =
        serde_json::from_str(&document.to_json_pretty().unwrap()).unwrap();
    old["schema_version"] = 62.into();
    let drawing = old["drawings"][0].as_object_mut().unwrap();
    drawing.remove("datum_reference_frames");
    let frame = drawing["feature_control_frames"][0]
        .as_object_mut()
        .unwrap();
    frame.remove("refinement");
    frame.remove("datum_reference_frame");
    frame.insert(
        "datums".into(),
        serde_json::to_value(&document.drawings[0].datum_reference_frames[0].datums).unwrap(),
    );
    let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
    assert!(migrated.drawings[0].datum_reference_frames.is_empty());
    assert!(
        migrated.drawings[0].feature_control_frames[0]
            .refinement
            .is_none()
    );
}
#[test]
fn named_datum_frames_resolve_precedence_and_nominal_321_coordinates_at_current_poses() {
    let mut definition = family_with_datums();
    definition.datums.clear();
    for (id, p, n) in [
        ("origin", [3.0, 4.0, 5.0], [0.0, 0.0, 1.0]),
        ("corner", [7.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        ("third", [0.0, 11.0, 0.0], [0.0, 1.0, 0.0]),
    ] {
        definition.datums.push(DatumDefinition {
            id: id.into(),
            kind: DatumKind::Plane {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    p[0],
                    p[1],
                    p[2],
                    LengthUnit::Millimeter,
                )),
                normal: VectorExpr::Literal(VectorQuantity::scalars(n[0], n[1], n[2])),
            },
        });
    }
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = composite_gdt_page();
    page.datum_features[2].attachment.anchor.datum = "third".into();
    for r in &mut page.datum_reference_frames[0].datums {
        r.boundary = DatumMaterialBoundary::Regardless;
    }
    let resolved = page.resolve_datum_reference_frame("ABC", &graph).unwrap();
    assert_eq!(
        resolved
            .datums
            .iter()
            .map(|d| d.precedence)
            .collect::<Vec<_>>(),
        vec![
            DatumPrecedence::Primary,
            DatumPrecedence::Secondary,
            DatumPrecedence::Tertiary
        ]
    );
    assert_eq!(
        page.resolve_datum_reference_frames(&graph).unwrap(),
        vec![resolved.clone()]
    );
    let frame = resolved.nominal_planar_321().unwrap();
    assert_eq!(frame.origin_mm, Vec3::new(7.0, 11.0, 5.0));
    assert_eq!(
        frame.coordinates_mm(Vec3::new(9.0, 14.0, 9.0)).unwrap(),
        Vec3::new(2.0, 3.0, 4.0)
    );
    graph
        .set_placement(
            "part",
            Placement {
                translation: VectorQuantity::lengths(1e6, -1e6, 3e5, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: std::f64::consts::FRAC_PI_2,
                }),
            },
        )
        .unwrap();
    let moved = page
        .resolve_datum_reference_frame("ABC", &graph)
        .unwrap()
        .nominal_planar_321()
        .unwrap();
    let expected = Vec3::new(1e6 - 11.0, -1e6 + 7.0, 3e5 + 5.0);
    assert!((moved.origin_mm.x - expected.x).abs() < 1e-8);
    assert!((moved.origin_mm.y - expected.y).abs() < 1e-8);
    let coordinates = moved
        .coordinates_mm(Vec3::new(1e6 - 14.0, -1e6 + 9.0, 3e5 + 9.0))
        .unwrap();
    assert!((coordinates.x - 2.0).abs() < 1e-8);
    assert!((coordinates.y - 3.0).abs() < 1e-8);
    assert!((coordinates.z - 4.0).abs() < 1e-8);
    assert!(
        page.resolve_datum_reference_frame("missing", &graph)
            .is_err()
    );
    let mut bad = resolved.clone();
    bad.datums.pop();
    assert!(bad.nominal_planar_321().is_err());
    bad = resolved.clone();
    bad.datums[1].boundary = DatumMaterialBoundary::Maximum;
    assert!(bad.nominal_planar_321().is_err());
    bad = resolved.clone();
    bad.datums[1].geometry = ResolvedDatum::Point {
        origin: Vec3::new(0.0, 0.0, 0.0),
    };
    assert!(bad.nominal_planar_321().is_err());
    bad = resolved.clone();
    bad.datums[1].geometry = ResolvedDatum::Plane {
        origin: Vec3::new(0.0, 0.0, 0.0),
        normal: Vec3::new(0.0, 0.0, 1.0),
    };
    assert!(bad.nominal_planar_321().is_err());
    bad = resolved.clone();
    bad.datums[0].geometry = ResolvedDatum::Plane {
        origin: Vec3::new(f64::INFINITY, 0.0, 0.0),
        normal: Vec3::new(0.0, 0.0, 1.0),
    };
    assert!(bad.nominal_planar_321().is_err());
    let mut scaled = resolved.clone();
    for datum in &mut scaled.datums {
        if let ResolvedDatum::Plane { normal, .. } = &mut datum.geometry {
            normal.x *= 2.0;
            normal.y *= 2.0;
            normal.z *= 2.0;
        }
    }
    assert_eq!(scaled.nominal_planar_321().unwrap(), frame);
    assert!(
        frame
            .coordinates_mm(Vec3::new(f64::INFINITY, 0.0, 0.0))
            .is_err()
    );
}
