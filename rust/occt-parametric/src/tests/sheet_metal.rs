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
        start_hem: None,
        end_hem: None,
        edge_flanges: Vec::new(),
    }
}
fn definition(sheet: SheetMetalDefinition) -> FamilyDefinition {
    FamilyDefinition {
        history: Default::default(),
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
                    bend_table: None,
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
    // Cutouts stay on the sheet and inside the width (they may cross
    // bends, schema 178), name an existing flange, and have unique IDs;
    // slots are not short.
    for case in 0..6 {
        let mut bad = sheet.clone();
        match case {
            0 => bad.cutouts[1].along = length(-30.0),
            1 => bad.cutouts[1].across = length(9.0),
            2 => bad.cutouts[1].flange = 3,
            3 => bad.cutouts[2].id = "hole".into(),
            4 => {
                bad.cutouts[2].shape = SheetMetalCutoutShape::Slot {
                    length: length(3.0),
                    width: length(4.0),
                }
            }
            _ => bad.cutouts[0].along = length(1.0),
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
#[test]
fn bend_tables_replace_the_neutral_factor_on_the_blank() {
    let session = Session::new().unwrap();
    let quarter = std::f64::consts::FRAC_PI_2;
    let mut sheet = spec(&[quarter, -quarter]);
    sheet.cutouts = vec![cutout(
        "hole",
        2,
        SheetMetalCutoutShape::Circle {
            diameter: length(4.0),
        },
    )];
    // 2 mm sheet, 3 mm radius: 5.2 mm at 90°, interpolated from 60° and 120°.
    let table = BendTable {
        name: "AL-2".into(),
        kind: BendTableKind::Allowance,
        thickness_mm: 2.0,
        inside_radii_mm: vec![3.0],
        angles_degrees: vec![60.0, 120.0],
        values_mm: vec![vec![3.4, 7.0]],
    };
    let metrics = sheet
        .flat_pattern_with_table(&HashMap::new(), 0.5, &table)
        .unwrap();
    for allowance in &metrics.bend_allowances_mm {
        assert!((allowance - 5.2).abs() < 1e-12, "{allowance}");
    }
    assert!((metrics.length_mm - 70.4).abs() < 1e-12);
    assert!((metrics.cutouts[0].center_mm[0] - 60.4).abs() < 1e-12);
    let drawing = metrics.drawing("bracket").unwrap();
    assert_eq!(drawing.metadata["Bend table"], "AL-2");
    assert!(!drawing.metadata.contains_key("Neutral factor K"));

    let mut family = definition(sheet);
    let FeatureOperation::SheetMetalFlat { bend_table, .. } = &mut family.features[0].operation
    else {
        unreachable!()
    };
    *bend_table = Some(Box::new(table.clone()));
    let mut instance = part(&family);
    let first = instance.regenerate(&session).unwrap();
    let flat = session.volume(first.shape("flat").unwrap()).unwrap();
    let hole = std::f64::consts::PI * 4.0 * 2.0;
    assert!((flat - (70.4 * 20.0 - hole)).abs() < 1e-6, "{flat}");

    // A table for another thickness, or one that misses a bend, fails
    // without replacing the accepted blank.
    family.parameters.push(length_parameter("thickness", 2.0));
    let FeatureOperation::SheetMetal { definition: folded } = &mut family.features[1].operation
    else {
        unreachable!()
    };
    folded.thickness = ScalarExpr::Parameter("thickness".into());
    instance = part(&family);
    let accepted = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "thickness".into(),
        ParameterValue::Scalar(Quantity::length(2.5, LengthUnit::Millimeter)),
    );
    let Err(error) = instance.regenerate_incremental(&session, &accepted) else {
        panic!("a 2.5 mm sheet used the 2 mm table");
    };
    assert!(error.to_string().contains("is for 2 mm sheet"), "{error}");
    let narrow = BendTable {
        angles_degrees: vec![100.0, 120.0],
        ..table
    };
    assert!(
        spec(&[quarter])
            .flat_pattern_with_table(&HashMap::new(), 0.5, &narrow)
            .unwrap_err()
            .to_string()
            .contains("lies outside bend table 'AL-2'")
    );

    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"bend_table\""));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    drop((first, accepted));
    assert_eq!(session.shape_count().unwrap(), 0);
}
fn open_hem(side: HemSide, gap: f64, length_mm: f64) -> SheetMetalHem {
    SheetMetalHem::Open {
        side,
        gap: length(gap),
        length: length(length_mm),
    }
}
fn rolled_hem(side: HemSide, radius: f64, angle: f64) -> SheetMetalHem {
    SheetMetalHem::Rolled {
        side,
        inside_radius: length(radius),
        angle_radians: ScalarExpr::Literal(Quantity::scalar(angle)),
    }
}
#[test]
fn open_hems_fold_back_on_the_chosen_side_with_their_gap() {
    let session = Session::new().unwrap();
    let pi = std::f64::consts::PI;
    // The second flange runs down -z at x = 24; a hem 1 mm open returns
    // along it at midplane radius 1.5, so 3 mm away, on either side.
    for (side, gap_center) in [(HemSide::Positive, 22.5), (HemSide::Negative, 25.5)] {
        let mut sheet = spec(&[pi / 2.0]);
        sheet.end_hem = Some(open_hem(side, 1.0, 10.0));
        let midplane = 40.0 + pi / 2.0 * 4.0 + pi * 1.5 + 10.0;
        let metrics = sheet.flat_pattern(&HashMap::new(), 0.5).unwrap();
        assert!((metrics.length_mm - midplane).abs() < 1e-9);
        assert_eq!(metrics.bend_allowances_mm.len(), 2);
        let family = definition(sheet);
        let generated = part(&family).regenerate(&session).unwrap();
        for id in ["folded", "flat"] {
            let shape = generated.shape(id).unwrap();
            assert!(session.is_valid(shape).unwrap());
            assert_eq!(session.shape_type(shape).unwrap(), ShapeType::Solid);
            let volume = session.volume(shape).unwrap();
            assert!((volume - midplane * 20.0).abs() < 1e-6, "{id}: {volume}");
        }
        // A probe centered in the gap clears both faces by half the gap.
        let folded = generated.shape("folded").unwrap();
        let probe = session
            .create_sphere(Vec3::new(gap_center, 5.0, -20.0), 0.01)
            .unwrap();
        let clearance = session.distance(folded, &probe).unwrap().distance;
        assert!((clearance - 0.49).abs() < 1e-6, "{side:?}: {clearance}");
        let bounds = session.bounds(folded).unwrap();
        // The fold's outside is 2.5 mm past the flange's end at z = -24.
        assert!((bounds.min.z + 26.5).abs() < 1e-6, "{bounds:?}");
        drop((probe, generated));
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn rolled_start_hems_keep_the_origin_and_shift_cutouts_on_the_blank() {
    let session = Session::new().unwrap();
    let pi = std::f64::consts::PI;
    let mut sheet = spec(&[]);
    sheet.start_hem = Some(rolled_hem(HemSide::Negative, 1.0, 1.5 * pi));
    sheet.cutouts = vec![cutout(
        "hole",
        0,
        SheetMetalCutoutShape::Circle {
            diameter: length(4.0),
        },
    )];
    // The roll's midplane radius is 2, so it adds 3 pi before flange 0.
    let roll = 1.5 * pi * 2.0;
    let metrics = sheet.flat_pattern(&HashMap::new(), 0.5).unwrap();
    assert!((metrics.length_mm - (20.0 + roll)).abs() < 1e-9);
    assert!((metrics.cutouts[0].center_mm[0] - (roll + 10.0)).abs() < 1e-9);
    assert!((metrics.bend_lines_mm[0] - roll / 2.0).abs() < 1e-9);
    metrics.drawing("rolled").unwrap();
    let hole = pi * 4.0 * 2.0;
    let family = definition(sheet.clone());
    let generated = part(&family).regenerate(&session).unwrap();
    for id in ["folded", "flat"] {
        let shape = generated.shape(id).unwrap();
        assert!(session.is_valid(shape).unwrap());
        let volume = session.volume(shape).unwrap();
        assert!(
            (volume - ((20.0 + roll) * 20.0 - hole)).abs() < 1e-6,
            "{id}: {volume}"
        );
    }
    // Flange 0 still starts at the origin and runs to x = 20; the roll
    // curls behind it on the side away from the sheet normal (-z), so up.
    let bounds = session.bounds(generated.shape("folded").unwrap()).unwrap();
    assert!((bounds.max.x - 20.0).abs() < 1e-6, "{bounds:?}");
    assert!((bounds.min.x + 3.0).abs() < 1e-6, "{bounds:?}");
    assert!((bounds.max.z - 5.0).abs() < 1e-6, "{bounds:?}");
    assert!((bounds.min.z + 1.0).abs() < 1e-6, "{bounds:?}");
    // The hole is still in flange 0, 10 mm from the origin.
    let probe = session
        .create_sphere(Vec3::new(10.0, 5.0, 0.0), 0.01)
        .unwrap();
    let clearance = session
        .distance(generated.shape("folded").unwrap(), &probe)
        .unwrap()
        .distance;
    assert!((clearance - 1.99).abs() < 1e-6, "{clearance}");

    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"start_hem\"") && json.contains("\"rolled\""));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    drop((probe, generated));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn hems_that_close_or_reach_back_into_their_flange_are_rejected() {
    let pi = std::f64::consts::PI;
    // At r = 1, t = 2 the roll re-enters its flange past 2 pi - acos(1/3).
    let limit = 2.0 * pi - (1.0f64 / 3.0).acos();
    for hem in [
        open_hem(HemSide::Positive, 0.19, 10.0),
        open_hem(HemSide::Positive, 1.0, 0.0),
        rolled_hem(HemSide::Positive, 1.0, pi),
        rolled_hem(HemSide::Positive, 1.0, limit),
        rolled_hem(HemSide::Negative, 0.0, 1.5 * pi),
    ] {
        let mut sheet = spec(&[pi / 2.0]);
        sheet.end_hem = Some(hem.clone());
        assert!(sheet.evaluate(&HashMap::new()).is_err(), "{hem:?}");
    }
    let mut sheet = spec(&[pi / 2.0]);
    sheet.end_hem = Some(rolled_hem(HemSide::Positive, 1.0, limit - 0.01));
    let session = Session::new().unwrap();
    let family = definition(sheet);
    let generated = part(&family).regenerate(&session).unwrap();
    assert!(
        session
            .is_valid(generated.shape("folded").unwrap())
            .unwrap()
    );
}
#[test]
fn allowance_tables_can_cover_hems() {
    let pi = std::f64::consts::PI;
    let mut sheet = spec(&[pi / 2.0]);
    sheet.end_hem = Some(open_hem(HemSide::Positive, 1.0, 10.0));
    // Rows: 0.5 mm (the hem's inside radius) and 3 mm.
    let table = BendTable {
        name: "hems".into(),
        kind: BendTableKind::Allowance,
        thickness_mm: 2.0,
        inside_radii_mm: vec![0.5, 3.0],
        angles_degrees: vec![90.0, 180.0],
        values_mm: vec![vec![2.0, 4.1], vec![6.0, 12.0]],
    };
    let metrics = sheet
        .flat_pattern_with_table(&HashMap::new(), 0.5, &table)
        .unwrap();
    let [bend, hem] = metrics.bend_allowances_mm[..] else {
        panic!("two allowances");
    };
    assert!((bend - 6.0).abs() < 1e-9 && (hem - 4.1).abs() < 1e-9);
    assert!((metrics.length_mm - 60.1).abs() < 1e-9);
}

#[test]
fn flat_bend_lines_lie_across_the_blank_on_both_faces() {
    let half_turn = std::f64::consts::FRAC_PI_2;
    let sheet = spec(&[half_turn, half_turn]);
    let family = definition(sheet.clone());
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let generated = part.regenerate(&session).unwrap();
    let flat = session
        .exact_bounds(generated.shape("flat").unwrap())
        .unwrap();
    let metrics = sheet.flat_pattern(&HashMap::new(), 0.5).unwrap();
    let lines = sheet.flat_bend_lines(&HashMap::new(), 0.5, None).unwrap();
    // Two bends, each on the top and bottom face.
    assert_eq!(lines.len(), 4);
    for (index, [a, b]) in lines.iter().enumerate() {
        let along = metrics.bend_lines_mm[index % 2];
        for p in [a, b] {
            assert!((p.x - along).abs() < 1e-9, "{p:?} vs {along}");
            let on_face = (p.z - flat.min.z).abs() < 1e-9 || (p.z - flat.max.z).abs() < 1e-9;
            assert!(on_face, "{p:?} not on {flat:?}");
        }
        // Across the full width.
        assert!((a.y - flat.min.y).abs() < 1e-9 && (b.y - flat.max.y).abs() < 1e-9);
    }
    assert!(lines[0][0].z != lines[2][0].z);
    // No bends, no lines.
    assert!(
        spec(&[])
            .flat_bend_lines(&HashMap::new(), 0.5, None)
            .unwrap()
            .is_empty()
    );
}
