use super::*;
fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}
fn spec(turns: &[f64]) -> SheetMetalDefinition {
    SheetMetalDefinition {
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        width_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        start_direction: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        width: length(10.0),
        thickness: length(2.0),
        flanges: vec![length(20.0); turns.len() + 1],
        bends: turns
            .iter()
            .map(|angle| SheetMetalBend {
                angle_radians: ScalarExpr::Literal(Quantity::scalar(*angle)),
                inside_radius: length(3.0),
            })
            .collect(),
        cutouts: Vec::new(),
    }
}
fn definition(sheet: SheetMetalDefinition) -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "sheet".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        datums: vec![],
        requirements: vec![],
        features: vec![
            FeatureDefinition {
                id: "flat".into(),
                operation: FeatureOperation::SheetMetalFlat {
                    input: "folded".into(),
                    neutral_factor: ScalarExpr::Literal(Quantity::scalar(0.5)),
                },
            },
            FeatureDefinition {
                id: "folded".into(),
                operation: FeatureOperation::SheetMetal {
                    definition: Box::new(sheet),
                },
            },
        ],
    }
}
fn part(family: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition: family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
#[test]
fn exact_flange_bend_volumes_and_neutral_blank() {
    let session = Session::new().unwrap();
    for turns in [
        vec![],
        vec![std::f64::consts::FRAC_PI_2],
        vec![-std::f64::consts::FRAC_PI_2],
        vec![std::f64::consts::FRAC_PI_2; 2],
        vec![std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2],
        vec![2.5],
    ] {
        let sheet = spec(&turns);
        let expected_length = 20.0 * (turns.len() + 1) as f64
            + turns.iter().map(|angle| angle.abs() * 4.0).sum::<f64>();
        let metrics = sheet.flat_pattern(&HashMap::new(), 0.5).unwrap();
        assert!((metrics.length_mm - expected_length).abs() < 1e-9);
        let family = definition(sheet);
        let generated = part(&family).regenerate(&session).unwrap();
        for id in ["folded", "flat"] {
            let shape = generated.shape(id).unwrap();
            assert!(session.is_valid(shape).unwrap());
            assert_eq!(session.shape_type(shape).unwrap(), ShapeType::Solid);
            assert!((session.volume(shape).unwrap() - expected_length * 20.0).abs() < 1e-6);
        }
        if turns.len() == 1 && turns[0] > 0.0 && turns[0] < 2.0 {
            let bounds = session.bounds(generated.shape("folded").unwrap()).unwrap();
            assert!((bounds.min.z + 24.0).abs() < 1e-6);
            assert!((bounds.max.x - 25.0).abs() < 1e-6);
        }
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn flat_edit_reuse_source_edits_and_document_round_trip() {
    let session = Session::new().unwrap();
    let mut sheet = spec(&[std::f64::consts::FRAC_PI_2]);
    sheet.thickness = ScalarExpr::Parameter("thickness".into());
    let mut family = definition(sheet);
    family.parameters.push(length_parameter("thickness", 2.0));
    family.parameters.push(ParameterDefinition {
        id: "k".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(0.5)),
        minimum: None,
        maximum: None,
    });
    if let FeatureOperation::SheetMetalFlat { neutral_factor, .. } =
        &mut family.features[0].operation
    {
        *neutral_factor = ScalarExpr::Parameter("k".into());
    }
    let mut instance = part(&family);
    let first = instance.regenerate(&session).unwrap();
    instance
        .overrides
        .insert("k".into(), ParameterValue::Scalar(Quantity::scalar(0.3)));
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.reused, vec!["folded"]);
    assert_eq!(second.regeneration.rebuilt, vec!["flat"]);
    assert!(
        session.volume(second.shape("flat").unwrap()).unwrap()
            < session.volume(first.shape("flat").unwrap()).unwrap()
    );
    instance.overrides.insert(
        "thickness".into(),
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
    );
    let third = instance.regenerate_incremental(&session, &second).unwrap();
    assert_eq!(third.regeneration.rebuilt, vec!["folded", "flat"]);
    instance
        .overrides
        .insert("k".into(), ParameterValue::Scalar(Quantity::scalar(2.0)));
    assert!(instance.regenerate_incremental(&session, &third).is_err());
    assert!(session.is_valid(third.shape("flat").unwrap()).unwrap());
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    let current = format!("\"schema_version\": {CURRENT_SCHEMA_VERSION}");
    assert!(json.contains(&current));
    let old = json.replace(&current, "\"schema_version\": 44");
    assert_eq!(ModelDocument::from_json(&old).unwrap(), document);
    drop((first, second, third));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn rotated_distant_and_small_sheets_keep_volume() {
    let session = Session::new().unwrap();
    for size in [0.01, 1.0, 100.0] {
        let mut sheet = spec(&[-1.2, 0.6]);
        sheet.origin = VectorExpr::Literal(VectorQuantity::lengths(
            1e6,
            -1e6,
            1e6,
            LengthUnit::Millimeter,
        ));
        sheet.width_axis = VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0));
        sheet.start_direction = VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.6, 0.8));
        sheet.width = length(10.0 * size);
        sheet.thickness = length(2.0 * size);
        sheet.flanges = vec![length(20.0 * size); 3];
        for bend in &mut sheet.bends {
            bend.inside_radius = length(3.0 * size);
        }
        let metrics = sheet.flat_pattern(&HashMap::new(), 0.5).unwrap();
        let family = definition(sheet);
        let generated = part(&family).regenerate(&session).unwrap();
        let expected = metrics.length_mm * metrics.width_mm * metrics.thickness_mm;
        assert!(
            (session.volume(generated.shape("folded").unwrap()).unwrap() - expected).abs()
                < expected * 1e-5
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn invalid_sheets_fail_without_partial_outputs() {
    let session = Session::new().unwrap();
    let original = spec(&[1.0]);
    for index in 0..9 {
        let mut sheet = original.clone();
        match index {
            0 => sheet.flanges.clear(),
            1 => sheet.bends.clear(),
            2 => sheet.thickness = length(-2.0),
            3 => sheet.bends[0].inside_radius = length(0.0),
            4 => {
                sheet.bends[0].angle_radians =
                    ScalarExpr::Literal(Quantity::scalar(std::f64::consts::PI))
            }
            5 => sheet.width_axis = sheet.start_direction.clone(),
            6 => {
                sheet.start_direction = VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0))
            }
            7 => sheet.width = ScalarExpr::Literal(Quantity::scalar(10.0)),
            _ => sheet.width = length(f64::MAX),
        }
        assert!(
            part(&definition(sheet)).regenerate(&session).is_err(),
            "case {index}"
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    // A tight inward spiral crosses itself. Refuse an overlapping outline.
    let mut spiral = spec(&[2.5; 5]);
    spiral.flanges = vec![length(1.0); 6];
    assert!(part(&definition(spiral)).regenerate(&session).is_err());
    let mut family = definition(original);
    family.features[1].operation = FeatureOperation::Box {
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        size: VectorExpr::Literal(VectorQuantity::lengths(
            1.0,
            1.0,
            1.0,
            LengthUnit::Millimeter,
        )),
    };
    assert!(
        part(&family)
            .regenerate(&session)
            .err()
            .unwrap()
            .message
            .contains("directly name")
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn allowance_drawings_validate_and_export() {
    let sheet = spec(&[1.0, -1.0]);
    let metrics = sheet.flat_pattern(&HashMap::new(), 0.3).unwrap();
    assert_eq!(metrics.bend_allowances_mm, vec![3.6; 2]);
    assert_eq!(metrics.bend_lines_mm, vec![21.8, 45.4]);
    let drawing = metrics.drawing("bracket").unwrap();
    assert_eq!(drawing.polylines.len(), 3);
    assert!(drawing.polylines[1].hidden);
    assert!(drawing.to_svg().contains("stroke-dasharray"));
    assert!(drawing.to_dxf().contains("LWPOLYLINE"));
    for k in [-0.1, 1.1, f64::NAN] {
        assert!(sheet.flat_pattern(&HashMap::new(), k).is_err());
    }
    let mut invalid = metrics.clone();
    invalid.bend_lines_mm[0] = f64::NAN;
    assert!(invalid.drawing("bad").is_err());
    assert!(metrics.drawing("").is_err());
    if std::env::var_os("OCCT_SHEET_QA").is_some() {
        std::fs::write("/tmp/occb-sheet-flat.dxf", drawing.to_dxf()).unwrap();
        std::fs::write("/tmp/occb-sheet-flat.svg", drawing.to_svg()).unwrap();
    }
}

fn cutout(id: &str, flange: usize, shape: SheetMetalCutoutShape) -> SheetMetalCutout {
    SheetMetalCutout {
        id: id.into(),
        flange,
        along: length(10.0),
        across: length(5.0),
        shape,
    }
}

#[test]
fn cutouts_cut_folded_flanges_and_their_flat_blank_alike() {
    let session = Session::new().unwrap();
    let quarter = std::f64::consts::FRAC_PI_2;
    let mut sheet = spec(&[quarter, -quarter]);
    sheet.cutouts = vec![
        cutout(
            "notch",
            0,
            SheetMetalCutoutShape::Rectangle {
                length: length(6.0),
                width: length(3.0),
            },
        ),
        cutout(
            "hole",
            1,
            SheetMetalCutoutShape::Circle {
                diameter: length(4.0),
            },
        ),
        cutout(
            "slot",
            2,
            SheetMetalCutoutShape::Slot {
                length: length(8.0),
                width: length(4.0),
            },
        ),
    ];
    // Each bend allows pi/2 * (3 + 0.5 * 2) on the blank.
    let allowance = quarter * 4.0;
    let metrics = sheet.flat_pattern(&HashMap::new(), 0.5).unwrap();
    let centers: Vec<[f64; 2]> = metrics.cutouts.iter().map(|c| c.center_mm).collect();
    assert_eq!(centers[0], [10.0, 5.0]);
    assert!((centers[1][0] - (30.0 + allowance)).abs() < 1e-12);
    assert!((centers[2][0] - (50.0 + 2.0 * allowance)).abs() < 1e-12);
    // Folded and flat lose the same cut-out area through the 2 mm sheet.
    let pi = std::f64::consts::PI;
    let removed = (18.0 + pi * 4.0 + (16.0 + pi * 4.0)) * 2.0;
    let full = (60.0 + 2.0 * allowance) * 20.0;
    let family = definition(sheet.clone());
    let generated = part(&family).regenerate(&session).unwrap();
    for id in ["folded", "flat"] {
        let shape = generated.shape(id).unwrap();
        assert!(session.is_valid(shape).unwrap());
        assert_eq!(session.shape_type(shape).unwrap(), ShapeType::Solid);
        let volume = session.volume(shape).unwrap();
        assert!((volume - (full - removed)).abs() < 1e-6, "{id}: {volume}");
    }
    // The hole is on the second flange, which the first bend turns down to
    // run from (24, 0, -4): its center is 2 mm from the nearest material.
    let probe = session
        .create_sphere(Vec3::new(24.0, 5.0, -14.0), 0.01)
        .unwrap();
    let clearance = session
        .distance(generated.shape("folded").unwrap(), &probe)
        .unwrap()
        .distance;
    assert!((clearance - 1.99).abs() < 1e-6, "{clearance}");
    // The blank's drawing outlines each cutout.
    let drawing = metrics.drawing("bracket").unwrap();
    // Outline, two bend lines, then each cutout closed: the rectangle in 5
    // points, the circle in two 32-chord arcs, the slot in two lines and
    // two arcs.
    assert_eq!(drawing.polylines.len(), 1 + 2 + 3);
    let counts: Vec<usize> = drawing.polylines[3..]
        .iter()
        .map(|p| p.points_mm.len())
        .collect();
    assert_eq!(counts, [5, 65, 67]);
    for cutout in &drawing.polylines[3..] {
        assert_eq!(cutout.points_mm.first(), cutout.points_mm.last());
    }
    if let Some(directory) = std::env::var_os("OCCB_SHEET_QA_DIR") {
        std::fs::write(
            std::path::Path::new(&directory).join("cutouts.svg"),
            drawing.to_svg(),
        )
        .unwrap();
    }
    // Cutouts stay inside their flange's straight region and the width,
    // name an existing flange, and have unique IDs; slots are not short.
    for case in 0..6 {
        let mut bad = sheet.clone();
        match case {
            0 => bad.cutouts[1].along = length(1.0),
            1 => bad.cutouts[1].across = length(9.0),
            2 => bad.cutouts[1].flange = 3,
            3 => bad.cutouts[2].id = "hole".into(),
            4 => {
                bad.cutouts[2].shape = SheetMetalCutoutShape::Slot {
                    length: length(3.0),
                    width: length(4.0),
                }
            }
            _ => bad.cutouts[0].along = length(18.0),
        }
        assert!(
            bad.flat_pattern(&HashMap::new(), 0.5).is_err(),
            "case {case}"
        );
    }
    // Cutouts persist (schema 160).
    let json = serde_json::to_string(&family).unwrap();
    assert!(json.contains("\"cutouts\"") && json.contains("\"slot\""));
    let restored: FamilyDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, family);
    drop(probe);
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}
