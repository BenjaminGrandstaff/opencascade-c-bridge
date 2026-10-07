use super::*;

const HOLE: [f64; 2] = [30.0, 20.0];

fn plane(id: &str, p: [f64; 3], n: [f64; 3]) -> DatumDefinition {
    DatumDefinition {
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
    }
}

/// A 60 × 40 × 10 mm plate: outward datum planes, a top, a side, a 45° chamfer
/// face, and a vertical hole axis.
pub(super) fn plate() -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Required, 100000.0);
    definition.requirements.clear();
    let s = std::f64::consts::FRAC_1_SQRT_2;
    definition.datums = vec![
        plane("bottom", [0.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        plane("left", [0.0, 0.0, 0.0], [-1.0, 0.0, 0.0]),
        plane("front", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
        plane("top", [0.0, 0.0, 10.0], [0.0, 0.0, 1.0]),
        plane("right", [60.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        plane("chamfer", [60.0, 35.0, 0.0], [s, s, 0.0]),
        DatumDefinition {
            id: "hole".into(),
            kind: DatumKind::Axis {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    HOLE[0],
                    HOLE[1],
                    0.0,
                    LengthUnit::Millimeter,
                )),
                direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            },
        },
    ];
    definition
}

pub(super) fn attachment(anchor: &str) -> DrawingGdtAttachment {
    DrawingGdtAttachment {
        view: "top".into(),
        output: InstanceOutputRef {
            instance: "part".into(),
            output: "body".into(),
        },
        anchor: DatumRef::new("part", anchor),
        offset_mm: [20.0, 30.0],
    }
}

fn control(
    id: &str,
    anchor: &str,
    characteristic: GeometricCharacteristic,
    tolerance: f64,
    datums: &[&str],
) -> DrawingFeatureControlFrame {
    DrawingFeatureControlFrame {
        size_limits: None,
        datum_reference_frame: None,
        refinement: None,
        id: id.into(),
        attachment: attachment(anchor),
        characteristic,
        tolerance: Quantity::length(tolerance, LengthUnit::Millimeter),
        display_unit: LengthUnit::Millimeter,
        precision: 3,
        zone: GeometricToleranceZone::Characteristic,
        material: ToleranceMaterialCondition::Regardless,
        feature_of_size: false,
        datums: datums
            .iter()
            .map(|d| DrawingDatumReference {
                datum_feature: (*d).into(),
                boundary: DatumMaterialBoundary::Regardless,
            })
            .collect(),
    }
}

fn position(
    id: &str,
    tolerance: f64,
    material: ToleranceMaterialCondition,
) -> DrawingFeatureControlFrame {
    let mut f = control(
        id,
        "hole",
        GeometricCharacteristic::Position,
        tolerance,
        &[],
    );
    f.datum_reference_frame = Some("ABC".into());
    f.zone = GeometricToleranceZone::Diameter;
    f.material = material;
    f.feature_of_size = true;
    f.size_limits = Some(DrawingSizeLimits {
        kind: FeatureOfSizeKind::Internal,
        lower: Quantity::length(9.9, LengthUnit::Millimeter),
        upper: Quantity::length(10.1, LengthUnit::Millimeter),
    });
    f
}

pub(super) fn page() -> DrawingDefinition {
    let datum = |id: &str, label: &str, anchor: &str| DrawingDatumFeature {
        id: id.into(),
        label: label.into(),
        feature_of_size: false,
        attachment: attachment(anchor),
    };
    DrawingDefinition {
        datum_reference_frames: vec![DrawingDatumReferenceFrame {
            id: "ABC".into(),
            datums: ["A", "B", "C"]
                .iter()
                .map(|d| DrawingDatumReference {
                    datum_feature: (*d).into(),
                    boundary: DatumMaterialBoundary::Regardless,
                })
                .collect(),
        }],
        datum_features: vec![
            datum("A", "A", "bottom"),
            datum("B", "B", "left"),
            datum("C", "C", "front"),
        ],
        surface_textures: Vec::new(),
        feature_control_frames: vec![
            control("flat", "top", GeometricCharacteristic::Flatness, 0.05, &[]),
            control(
                "parallel",
                "top",
                GeometricCharacteristic::Parallelism,
                0.05,
                &["A"],
            ),
            control(
                "perpendicular",
                "right",
                GeometricCharacteristic::Perpendicularity,
                0.05,
                &["A"],
            ),
            control(
                "angular",
                "chamfer",
                GeometricCharacteristic::Angularity,
                0.05,
                &["A", "B"],
            ),
            position("position", 0.06, ToleranceMaterialCondition::Maximum),
        ],
        sheet: None,
        id: "plate".into(),
        title: "Plate".into(),
        paper_size_mm: [297.0, 210.0],
        views: vec![DrawingView {
            id: "top".into(),
            outputs: vec![InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            }],
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [40.0, 60.0],
            scale: 1.0,
            show_hidden: false,
            kind: DrawingViewKind::Orthographic,
            detail: None,
            hatching: None,
            material_hatching: BTreeMap::new(),
        }],
        dimensions: Vec::new(),
        guides: Vec::new(),
        notes: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

/// Grid points on a plane given by `point(u, v)` over the unit square.
fn grid(point: impl Fn(f64, f64) -> [f64; 3]) -> Vec<[f64; 3]> {
    (0..=6)
        .flat_map(|i| (0..=4).map(move |j| (i as f64 / 6.0, j as f64 / 4.0)))
        .map(|(u, v)| point(u, v))
        .collect()
}

fn hole(center: [f64; 2], diameter: f64) -> Vec<[f64; 3]> {
    (0..36)
        .flat_map(|i| [1.0, 5.0, 9.0].map(|z| (i, z)))
        .map(|(i, z)| {
            let a = i as f64 * std::f64::consts::TAU / 36.0;
            [
                center[0] + diameter / 2.0 * a.cos(),
                center[1] + diameter / 2.0 * a.sin(),
                z,
            ]
        })
        .collect()
}

fn measured(id: &str, points_mm: Vec<[f64; 3]>) -> MeasuredFeature {
    MeasuredFeature {
        id: id.into(),
        points_mm,
    }
}

/// Exact datum and feature surfaces of the nominal plate.
pub(super) fn nominal_record() -> InspectionRecord {
    InspectionRecord {
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: vec![
            measured("A", grid(|u, v| [60.0 * u, 40.0 * v, 0.0])),
            measured("B", grid(|u, v| [0.0, 40.0 * u, 10.0 * v])),
            measured("C", grid(|u, v| [60.0 * u, 0.0, 10.0 * v])),
        ],
        controls: vec![
            measured("flat", grid(|u, v| [60.0 * u, 40.0 * v, 10.0])),
            measured("parallel", grid(|u, v| [60.0 * u, 40.0 * v, 10.0])),
            measured("perpendicular", grid(|u, v| [60.0, 40.0 * u, 10.0 * v])),
            measured(
                "angular",
                grid(|u, v| [55.0 + 5.0 * u, 40.0 - 5.0 * u, 10.0 * v]),
            ),
            measured("position", hole(HOLE, 10.04)),
        ],
    }
}

fn evaluated(report: &InspectionReport, id: &str) -> ControlMeasurement {
    match &report
        .controls
        .iter()
        .find(|c| c.control == id)
        .unwrap()
        .result
    {
        ControlResult::Evaluated(m) => m.clone(),
        other => panic!("{id}: {other:?}"),
    }
}

fn record_with(id: &str, points: Vec<[f64; 3]>) -> InspectionRecord {
    let mut record = nominal_record();
    for m in record.datum_features.iter_mut().chain(&mut record.controls) {
        if m.id == id {
            m.points_mm = points.clone();
        }
    }
    record
}

#[test]
fn nominal_part_conforms_and_measured_frame_matches_nominal_frame() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = page();
    let report = page.evaluate_inspection(&graph, &nominal_record()).unwrap();
    assert!(report.conforms(), "{report:?}");
    for id in ["flat", "parallel", "perpendicular", "angular"] {
        assert!(evaluated(&report, id).deviation_mm < 1e-9, "{id}");
    }
    let position = evaluated(&report, "position");
    assert!(position.deviation_mm < 1e-9);
    assert!((position.actual_size_mm.unwrap() - 10.04).abs() < 0.01);
    assert_eq!(report.datum_frames.len(), 1);
    let nominal = page
        .resolve_datum_reference_frame("ABC", &graph)
        .unwrap()
        .nominal_planar_321()
        .unwrap();
    let measured = report.datum_frames[0].frame;
    assert!(length_between(measured.origin_mm, nominal.origin_mm) < 1e-9);
    assert!(length_between(measured.z_axis, nominal.z_axis) < 1e-12);
    assert!(length_between(measured.x_axis, nominal.x_axis) < 1e-12);
}

fn length_between(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).hypot((a.y - b.y).hypot(a.z - b.z))
}

#[test]
fn flatness_is_minimum_zone_after_removing_tilt() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = page();
    let tilt = 0.001;
    // A tilted plane with one interior bump: the minimum zone is the bump,
    // while a zone parallel to the nominal top would also include the tilt.
    let mut top = grid(|u, v| [60.0 * u, 40.0 * v, 10.0 + tilt * 60.0 * u]);
    top.push([30.0, 20.0, 10.0 + tilt * 30.0 + 0.03]);
    let report = page
        .evaluate_inspection(&graph, &record_with("flat", top.clone()))
        .unwrap();
    let flat = evaluated(&report, "flat");
    assert!((flat.deviation_mm - 0.03).abs() < 1e-6, "{flat:?}");
    assert!(flat.conforms);
    page.feature_control_frames[0].tolerance = Quantity::length(0.02, LengthUnit::Millimeter);
    let report = page
        .evaluate_inspection(&graph, &record_with("flat", top))
        .unwrap();
    assert!(!evaluated(&report, "flat").conforms);
    assert!(!report.conforms());
}

#[test]
fn orientation_is_measured_from_fitted_datum_simulators() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = page();
    let tilt = 0.0005;
    // Datum A is tilted and has a low spot; its simulator contacts the high
    // (outward) points, so a top tilted the same way is perfectly parallel.
    let mut a = grid(|u, v| [60.0 * u, 40.0 * v, tilt * 60.0 * u]);
    a.push([30.0, 20.0, tilt * 30.0 + 0.2]);
    let mut record = record_with("A", a);
    let tilted_top = grid(|u, v| [60.0 * u, 40.0 * v, 10.0 + tilt * 60.0 * u]);
    record.controls[1].points_mm = tilted_top;
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    assert!(evaluated(&report, "parallel").deviation_mm < 1e-9);
    // A level top is out of parallel by the datum tilt across its length.
    record.controls[1].points_mm = grid(|u, v| [60.0 * u, 40.0 * v, 10.0]);
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    let parallel = evaluated(&report, "parallel");
    assert!(
        (parallel.deviation_mm - tilt * 60.0).abs() < 1e-6,
        "{parallel:?}"
    );

    // Perpendicularity to A alone leaves rotation about A's normal free.
    let yawed = grid(|u, v| [60.0 + 0.002 * 40.0 * u, 40.0 * u, 10.0 * v]);
    let report = page
        .evaluate_inspection(&graph, &record_with("perpendicular", yawed))
        .unwrap();
    assert!(evaluated(&report, "perpendicular").deviation_mm < 1e-9);
    let leaning = grid(|u, v| [60.0 + 0.001 * 10.0 * v, 40.0 * u, 10.0 * v]);
    let report = page
        .evaluate_inspection(&graph, &record_with("perpendicular", leaning))
        .unwrap();
    let perpendicular = evaluated(&report, "perpendicular");
    assert!((perpendicular.deviation_mm - 0.01).abs() < 1e-6);
    assert!(perpendicular.conforms);

    // Angularity to A and B fixes the basic 45° orientation completely.
    let rotated = grid(|u, v| [55.0 + 5.0 * u, 40.0 - 5.0 * u + 0.004 * u, 10.0 * v]);
    let report = page
        .evaluate_inspection(&graph, &record_with("angular", rotated))
        .unwrap();
    let angular = evaluated(&report, "angular");
    assert!((angular.deviation_mm - 0.004 * std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6);
}

#[test]
fn position_uses_mating_envelope_bonus_tolerance_and_measured_datums() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = page();
    let offset = [HOLE[0] + 0.04, HOLE[1] + 0.03];
    let report = page
        .evaluate_inspection(&graph, &record_with("position", hole(offset, 10.04)))
        .unwrap();
    let mmc = evaluated(&report, "position");
    assert!((mmc.deviation_mm - 0.1).abs() < 1e-6, "{mmc:?}");
    assert!((mmc.bonus_mm - 0.14).abs() < 1e-3);
    assert_eq!(mmc.size_conforms, Some(true));
    assert!(mmc.conforms);

    // The same hole fails regardless of feature size, and gains a smaller
    // least-material bonus.
    page.feature_control_frames[4].material = ToleranceMaterialCondition::Regardless;
    let report = page
        .evaluate_inspection(&graph, &record_with("position", hole(offset, 10.04)))
        .unwrap();
    assert!(!evaluated(&report, "position").conforms);
    page.feature_control_frames[4].material = ToleranceMaterialCondition::Least;
    let report = page
        .evaluate_inspection(&graph, &record_with("position", hole(offset, 10.04)))
        .unwrap();
    let lmc = evaluated(&report, "position");
    assert!((lmc.bonus_mm - 0.06).abs() < 1e-3);
    assert!(lmc.conforms);

    // A shifted datum B moves the measured frame with it.
    let mut record = record_with("position", hole(offset, 10.04));
    record.datum_features[1].points_mm = grid(|u, v| [0.03, 40.0 * u, 10.0 * v]);
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    let shifted = evaluated(&report, "position");
    assert!((shifted.deviation_mm - 2.0 * 0.01f64.hypot(0.03)).abs() < 1e-6);

    // An undersized hole fails its size and therefore its position.
    page.feature_control_frames[4].material = ToleranceMaterialCondition::Maximum;
    let report = page
        .evaluate_inspection(&graph, &record_with("position", hole(HOLE, 9.8)))
        .unwrap();
    let small = evaluated(&report, "position");
    assert_eq!(small.size_conforms, Some(false));
    assert_eq!(small.bonus_mm, 0.0);
    assert!(!small.conforms);
}

#[test]
fn far_rotated_placements_keep_measured_deviations() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let placement = Placement {
        translation: VectorQuantity::lengths(1e6, -1e6, 3e5, LengthUnit::Millimeter),
        rotation: Some(AxisAngle {
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(1.0, 2.0, 3.0),
            angle_radians: 0.7,
        }),
    };
    graph.set_placement("part", placement).unwrap();
    let page = page();
    let mut record = record_with("position", hole([HOLE[0] + 0.04, HOLE[1] + 0.03], 10.04));
    let mut top = grid(|u, v| [60.0 * u, 40.0 * v, 10.0 + 0.001 * 60.0 * u]);
    top.push([30.0, 20.0, 10.0 + 0.03 + 0.03]);
    record.controls[0].points_mm = top;
    let normalized = placement.normalized().unwrap();
    for m in record.datum_features.iter_mut().chain(&mut record.controls) {
        for p in &mut m.points_mm {
            let moved = crate::assembly::transform_point(Vec3::new(p[0], p[1], p[2]), &normalized);
            *p = [moved.x, moved.y, moved.z];
        }
    }
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    assert!((evaluated(&report, "position").deviation_mm - 0.1).abs() < 1e-5);
    assert!((evaluated(&report, "flat").deviation_mm - 0.03).abs() < 1e-5);
    assert!(evaluated(&report, "perpendicular").deviation_mm < 1e-6);
}

#[test]
fn unsupported_or_unmeasured_controls_are_reported_and_bad_records_fail() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = page();
    let mut record = nominal_record();
    record.controls.retain(|m| m.id != "flat");
    record.datum_features.retain(|m| m.id != "B");
    record.controls[0].points_mm = (0..5).map(|i| [10.0 * i as f64, 0.0, 10.0]).collect();
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    let result = |id: &str| {
        report
            .controls
            .iter()
            .find(|c| c.control == id)
            .unwrap()
            .result
            .clone()
    };
    assert_eq!(result("flat"), ControlResult::NotMeasured);
    assert!(
        matches!(result("parallel"), ControlResult::NotEvaluated(r) if r.contains("collinear"))
    );
    assert!(matches!(result("angular"), ControlResult::NotEvaluated(r) if r.contains("'B'")));
    assert!(report.datum_frames.is_empty());
    assert!(!report.conforms());

    page.feature_control_frames[4].refinement = Some(DrawingCompositeRefinement {
        tolerance: Quantity::length(0.02, LengthUnit::Millimeter),
        datums: Vec::new(),
    });
    page.datum_reference_frames[0].datums[0].boundary = DatumMaterialBoundary::Regardless;
    page.feature_control_frames[3].datums[1].boundary = DatumMaterialBoundary::Maximum;
    page.datum_features[1].feature_of_size = true;
    let report = page.evaluate_inspection(&graph, &nominal_record()).unwrap();
    assert!(matches!(
        &report.controls[4].result,
        ControlResult::NotEvaluated(r) if r.contains("composite")
    ));
    assert!(matches!(
        &report.controls[3].result,
        ControlResult::NotEvaluated(r) if r.contains("datum shift")
    ));

    let page = self::page();
    for (case, edit) in ["drawing", "unknown", "duplicate", "nan", "budget"]
        .into_iter()
        .enumerate()
    {
        let mut record = nominal_record();
        match edit {
            "drawing" => record.drawing = "other".into(),
            "unknown" => record.controls[0].id = "missing".into(),
            "duplicate" => record.datum_features.push(record.datum_features[0].clone()),
            "nan" => record.controls[0].points_mm[0][2] = f64::NAN,
            _ => record.controls[0].points_mm = vec![[0.0; 3]; MAX_INSPECTION_POINTS + 1],
        }
        assert!(page.evaluate_inspection(&graph, &record).is_err(), "{case}");
    }
}

#[test]
fn size_limits_validate_persist_and_inspection_records_round_trip() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    for edit in 0..4 {
        let mut page = page();
        let frame = &mut page.feature_control_frames[4];
        let limits = frame.size_limits.as_mut().unwrap();
        match edit {
            0 => frame.feature_of_size = false,
            1 => limits.upper = Quantity::length(9.0, LengthUnit::Millimeter),
            2 => limits.lower = Quantity::length(f64::NAN, LengthUnit::Millimeter),
            _ => limits.upper = Quantity::scalar(1.0),
        }
        assert!(page.validate(&graph).is_err(), "{edit}");
    }
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page());
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"size_limits\""));
    let restored = ModelDocument::from_json(&json).unwrap();
    assert_eq!(restored, document);
    assert_eq!(restored.schema_version, CURRENT_SCHEMA_VERSION);
    let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
    legacy["schema_version"] = serde_json::json!(67);
    for frame in legacy["drawings"][0]["feature_control_frames"]
        .as_array_mut()
        .unwrap()
    {
        frame.as_object_mut().unwrap().remove("size_limits");
    }
    let migrated = ModelDocument::from_json(&legacy.to_string()).unwrap();
    assert!(
        migrated.drawings[0].feature_control_frames[4]
            .size_limits
            .is_none()
    );
    let record = InspectionRecord {
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: vec![measured("A", vec![[0.5, -2.0, 1e6]])],
        controls: vec![measured("flat", vec![[1.0, 2.0, 3.0]])],
    };
    let parsed: InspectionRecord =
        serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    assert_eq!(parsed, record);
}
