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
        pattern: None,
        unequal: None,
        surface: None,
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
                common_with: Vec::new(),
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
        size_kind: None,
        size_limits: None,
    };
    DrawingDefinition {
        datum_reference_frames: vec![DrawingDatumReferenceFrame {
            id: "ABC".into(),
            datums: ["A", "B", "C"]
                .iter()
                .map(|d| DrawingDatumReference {
                    datum_feature: (*d).into(),
                    boundary: DatumMaterialBoundary::Regardless,
                    common_with: Vec::new(),
                })
                .collect(),
        }],
        datum_features: vec![
            datum("A", "A", "bottom"),
            datum("B", "B", "left"),
            datum("C", "C", "front"),
        ],
        surface_textures: Vec::new(),
        parts_list: None,
        balloons: Vec::new(),
        releases: Vec::new(),
        revision_table: None,
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
        nominals: Vec::new(),
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
    // Composite position is evaluated since schema 138: a single feature's
    // refinement relates it to nothing else, so only the upper segment binds.
    let composite = evaluated(&report, "position");
    let refined = composite.refinement.expect("refinement");
    assert!(refined.deviation_mm < 1e-6, "{refined:?}");
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

/// Points on a circle of radius 5 + `lobes(θ)` about `center` at height `z`.
fn section(center: [f64; 2], z: f64, lobes: impl Fn(f64) -> f64) -> Vec<[f64; 3]> {
    (0..72)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / 72.0;
            let r = 5.0 + lobes(a);
            [center[0] + r * a.cos(), center[1] + r * a.sin(), z]
        })
        .collect()
}

/// Points on a cylinder about the axis through `base` along `axis`, radius
/// `radius(s)` at distance `s` along it.
fn cylinder(base: [f64; 3], axis: [f64; 3], radius: impl Fn(f64) -> f64) -> Vec<[f64; 3]> {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let w = [axis[0] / n, axis[1] / n, axis[2] / n];
    // u ⟂ w from the x axis, v = w × u.
    let d = w[0];
    let u = {
        let u = [1.0 - d * w[0], -d * w[1], -d * w[2]];
        let m = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
        [u[0] / m, u[1] / m, u[2] / m]
    };
    let v = [
        w[1] * u[2] - w[2] * u[1],
        w[2] * u[0] - w[0] * u[2],
        w[0] * u[1] - w[1] * u[0],
    ];
    (0..=10)
        .flat_map(|k| (0..36).map(move |i| (k as f64, i as f64 * std::f64::consts::TAU / 36.0)))
        .map(|(s, a)| {
            let r = radius(s);
            [0, 1, 2].map(|j| base[j] + w[j] * s + r * (a.cos() * u[j] + a.sin() * v[j]))
        })
        .collect()
}

fn round_page() -> DrawingDefinition {
    let mut page = page();
    page.feature_control_frames = vec![
        control(
            "round",
            "hole",
            GeometricCharacteristic::Circularity,
            0.03,
            &[],
        ),
        control(
            "straight",
            "hole",
            GeometricCharacteristic::Cylindricity,
            0.05,
            &[],
        ),
    ];
    page
}

fn round_record(round: Vec<[f64; 3]>, straight: Vec<[f64; 3]>) -> InspectionRecord {
    InspectionRecord {
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: Vec::new(),
        controls: vec![measured("round", round), measured("straight", straight)],
    }
}

#[test]
fn circularity_takes_the_worst_minimum_zone_section_with_free_centers() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = round_page();
    // Three-lobed sections: the minimum zone is twice the lobe amplitude,
    // wherever each section's center sits.
    let mut round = section([30.02, 19.99], 1.0, |a| 0.01 * (3.0 * a).cos());
    round.extend(section([29.97, 20.03], 5.0, |a| 0.012 * (3.0 * a).cos()));
    round.extend(section([30.0, 20.0], 9.0, |a| {
        0.005 * (3.0 * a + 0.4).cos()
    }));
    let straight = cylinder([30.0, 20.0, 0.0], [0.0, 0.0, 1.0], |_| 5.0);
    let report = page
        .evaluate_inspection(&graph, &round_record(round, straight))
        .unwrap();
    let round = evaluated(&report, "round");
    assert!((round.deviation_mm - 0.024).abs() < 1e-6, "{round:?}");
    assert!(round.conforms);
    // A perfect cylinder has no cylindricity deviation.
    assert!(evaluated(&report, "straight").deviation_mm < 1e-7);
}

#[test]
fn cylindricity_frees_the_axis_and_keeps_taper() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = round_page();
    let base = [30.05, 19.97, 0.0];
    let axis = [0.002, -0.001, 1.0];
    // A perfect cylinder on a shifted, tilted axis: the fit finds the axis.
    let tilted = cylinder(base, axis, |_| 5.0);
    let round = section([30.0, 20.0], 5.0, |_| 0.0);
    let report = page
        .evaluate_inspection(&graph, &round_record(round.clone(), tilted))
        .unwrap();
    assert!(evaluated(&report, "straight").deviation_mm < 1e-6);
    // A 0.03 mm taper over the length cannot be removed by moving the axis.
    let tapered = cylinder(base, axis, |s| 5.0 + 0.003 * s);
    let report = page
        .evaluate_inspection(&graph, &round_record(round.clone(), tapered))
        .unwrap();
    let straight = evaluated(&report, "straight");
    assert!((straight.deviation_mm - 0.03).abs() < 1e-6, "{straight:?}");
    assert!(straight.conforms);
    // One section cannot fix an axis tilt, so cylindricity is reported, not guessed.
    let single = section([30.0, 20.0], 5.0, |_| 0.0);
    let report = page
        .evaluate_inspection(&graph, &round_record(round, single))
        .unwrap();
    let straight = report
        .controls
        .iter()
        .find(|c| c.control == "straight")
        .unwrap();
    assert!(
        matches!(&straight.result, ControlResult::NotEvaluated(r) if r.contains("determine")),
        "{straight:?}"
    );
}

/// Plate drawing with a bore datum feature D (internal, on the hole axis) and
/// circular/total runout controls on `anchor` relative to `datums`.
fn runout_page(
    anchor: &str,
    datums: &[&str],
    kind: Option<FeatureOfSizeKind>,
) -> DrawingDefinition {
    let mut page = page();
    page.datum_features.push(DrawingDatumFeature {
        id: "D".into(),
        label: "D".into(),
        feature_of_size: true,
        attachment: attachment("hole"),
        size_kind: kind,
        size_limits: None,
    });
    page.datum_reference_frames
        .push(DrawingDatumReferenceFrame {
            id: "AD".into(),
            datums: ["A", "D"]
                .iter()
                .map(|d| DrawingDatumReference {
                    datum_feature: (*d).into(),
                    boundary: DatumMaterialBoundary::Regardless,
                    common_with: Vec::new(),
                })
                .collect(),
        });
    page.feature_control_frames = vec![
        control(
            "circ",
            anchor,
            GeometricCharacteristic::CircularRunout,
            0.05,
            datums,
        ),
        control(
            "total",
            anchor,
            GeometricCharacteristic::TotalRunout,
            0.05,
            datums,
        ),
    ];
    page
}

fn runout_record(datum: Vec<[f64; 3]>, surface: Vec<[f64; 3]>) -> InspectionRecord {
    InspectionRecord {
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: vec![
            measured("A", grid(|u, v| [60.0 * u, 40.0 * v, 0.0])),
            measured("D", datum),
        ],
        controls: vec![
            measured("circ", surface.clone()),
            measured("total", surface),
        ],
    }
}

#[test]
fn radial_runout_is_indicator_movement_about_the_fitted_bore_axis() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = runout_page("hole", &["D"], Some(FeatureOfSizeKind::Internal));
    // The bore sits on a shifted, tilted axis; runout is measured about it.
    let base = [30.05, 19.97, 0.0];
    let axis = [0.002, -0.001, 1.0];
    let bore = cylinder(base, axis, |_| 5.0);
    // A coaxial boss has no runout.
    let report = page
        .evaluate_inspection(
            &graph,
            &runout_record(bore.clone(), cylinder(base, axis, |_| 8.0)),
        )
        .unwrap();
    assert!(evaluated(&report, "circ").deviation_mm < 1e-6);
    assert!(evaluated(&report, "total").deviation_mm < 1e-6);
    // An eccentric boss (0.02 mm offset) reads twice its eccentricity; a
    // 0.03 mm taper over its length adds to total but not circular runout.
    let offset = [base[0] + 0.02, base[1], base[2]];
    let report = page
        .evaluate_inspection(
            &graph,
            &runout_record(bore.clone(), cylinder(offset, axis, |s| 8.0 + 0.003 * s)),
        )
        .unwrap();
    let circular = evaluated(&report, "circ");
    let total = evaluated(&report, "total");
    assert!((circular.deviation_mm - 0.04).abs() < 1e-5, "{circular:?}");
    assert!(circular.conforms);
    assert!((total.deviation_mm - 0.07).abs() < 1e-5, "{total:?}");
    assert!(!total.conforms);
    assert_eq!(circular.bonus_mm, 0.0);
    // The named A|D frame reports the secondary axis, held normal to A.
    let axis = &report.datum_axes[0];
    assert_eq!(axis.datum_features, ["A", "D"]);
    assert!((axis.direction.z.abs() - 1.0).abs() < 1e-9);
    assert!(
        report
            .datum_frames
            .iter()
            .all(|f| f.datum_features != ["A", "D"])
    );
}

#[test]
fn axis_datums_use_the_mating_envelope_for_holes_and_shafts() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    // A three-lobed datum: the inscribed (hole) and circumscribed (shaft)
    // envelopes have different sizes but the same centered axis.
    let lobed: Vec<[f64; 3]> = (0..=10)
        .flat_map(|k| section(HOLE, k as f64, |a| 0.01 * (3.0 * a).cos()))
        .collect();
    let surface = cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| 8.0);
    for (kind, size) in [
        (FeatureOfSizeKind::Internal, 9.98),
        (FeatureOfSizeKind::External, 10.02),
    ] {
        let mut page = runout_page("hole", &["D"], Some(kind));
        page.datum_reference_frames
            .push(DrawingDatumReferenceFrame {
                id: "D-only".into(),
                datums: vec![DrawingDatumReference {
                    datum_feature: "D".into(),
                    boundary: DatumMaterialBoundary::Regardless,
                    common_with: Vec::new(),
                }],
            });
        let report = page
            .evaluate_inspection(&graph, &runout_record(lobed.clone(), surface.clone()))
            .unwrap();
        let axis = report
            .datum_axes
            .iter()
            .find(|a| a.datum_features == ["D"])
            .unwrap();
        assert!(
            (axis.mating_sizes_mm[0] - size).abs() < 1e-6,
            "{kind:?} {axis:?}"
        );
        assert!((axis.origin_mm.x - HOLE[0]).abs() < 1e-6);
        assert!(evaluated(&report, "total").deviation_mm < 1e-5);
    }
}

#[test]
fn axial_runout_reads_face_wobble_per_circle_and_over_the_face() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    // Face "top" relative to A (bottom plane) then D (bore axis normal to A).
    let page = runout_page("top", &["A", "D"], Some(FeatureOfSizeKind::Internal));
    let bore = cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| 5.0);
    // Rings of radius 6-9 mm: a 0.001 tilt wobbles each circle by 2·0.001·ρ,
    // and a 0.002 dish adds 0.002·ρ between circles.
    let face: Vec<[f64; 3]> = [6.0, 7.0, 8.0, 9.0]
        .into_iter()
        .flat_map(|rho: f64| {
            (0..36).map(move |i| {
                let a = i as f64 * std::f64::consts::TAU / 36.0;
                [
                    HOLE[0] + rho * a.cos(),
                    HOLE[1] + rho * a.sin(),
                    10.0 + 0.001 * rho * a.cos() + 0.002 * rho,
                ]
            })
        })
        .collect();
    let report = page
        .evaluate_inspection(&graph, &runout_record(bore, face))
        .unwrap();
    let circular = evaluated(&report, "circ");
    let total = evaluated(&report, "total");
    assert!((circular.deviation_mm - 0.018).abs() < 1e-9, "{circular:?}");
    assert!((total.deviation_mm - 0.021).abs() < 1e-9, "{total:?}");
    assert!(circular.conforms && total.conforms);
}

#[test]
fn runout_reports_unsupported_setups_and_size_kind_validates() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let bore = cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| 5.0);
    let surface = cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| 8.0);
    let side: Vec<[f64; 3]> = grid(|u, v| [0.0, 40.0 * u, 10.0 * v]);
    for (page, points, reason) in [
        (
            runout_page("hole", &["A"], Some(FeatureOfSizeKind::Internal)),
            surface.clone(),
            "datum axis",
        ),
        (
            runout_page("hole", &["D"], None),
            surface.clone(),
            "size kind",
        ),
        (
            runout_page("left", &["D"], Some(FeatureOfSizeKind::Internal)),
            side,
            "not perpendicular",
        ),
    ] {
        let report = page
            .evaluate_inspection(&graph, &runout_record(bore.clone(), points))
            .unwrap();
        for control in &report.controls {
            assert!(
                matches!(&control.result, ControlResult::NotEvaluated(r) if r.contains(reason)),
                "{control:?}"
            );
        }
    }
    // Orientation to an axis datum is reported, not misread as a plane.
    let mut page = runout_page("top", &["D"], Some(FeatureOfSizeKind::Internal));
    page.feature_control_frames[0].characteristic = GeometricCharacteristic::Perpendicularity;
    let report = page
        .evaluate_inspection(
            &graph,
            &runout_record(bore, grid(|u, v| [60.0 * u, 40.0 * v, 10.0])),
        )
        .unwrap();
    assert!(matches!(
        &report.controls[0].result,
        ControlResult::NotEvaluated(_)
    ));
    // A size kind needs a declared feature of size; it persists in JSON.
    let mut invalid = runout_page("hole", &["D"], Some(FeatureOfSizeKind::Internal));
    invalid.datum_features.last_mut().unwrap().feature_of_size = false;
    assert!(
        invalid
            .validate(&graph)
            .unwrap_err()
            .message
            .contains("size kind")
    );
    let page = runout_page("hole", &["D"], Some(FeatureOfSizeKind::External));
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"size_kind\": \"external\""));
    assert_eq!(
        ModelDocument::from_json(&json).unwrap().drawings,
        document.drawings
    );
}

/// Datum features A and B on the hole axis (shaft journals), E on the
/// bottom plane, and a named frame with the common datum A-B.
fn common_page() -> DrawingDefinition {
    let mut page = page();
    page.datum_features.retain(|f| f.id != "B");
    for (id, anchor, kind) in [
        ("A2", "hole", Some(FeatureOfSizeKind::External)),
        ("B", "hole", Some(FeatureOfSizeKind::External)),
        ("E", "bottom", None),
    ] {
        page.datum_features.push(DrawingDatumFeature {
            id: id.into(),
            label: if id == "A2" { "F".into() } else { id.into() },
            feature_of_size: kind.is_some(),
            attachment: attachment(anchor),
            size_kind: kind,
            size_limits: None,
        });
    }
    page.datum_reference_frames = vec![DrawingDatumReferenceFrame {
        id: "FB".into(),
        datums: vec![DrawingDatumReference {
            datum_feature: "A2".into(),
            boundary: DatumMaterialBoundary::Regardless,
            common_with: vec!["B".into()],
        }],
    }];
    page
}

#[test]
fn common_datum_axis_joins_the_midpoints_of_both_journal_envelopes() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = common_page();
    let mut common = control(
        "common",
        "hole",
        GeometricCharacteristic::TotalRunout,
        0.01,
        &["A2"],
    );
    common.datums[0].common_with = vec!["B".into()];
    let single = control(
        "single",
        "hole",
        GeometricCharacteristic::TotalRunout,
        0.01,
        &["A2"],
    );
    page.feature_control_frames = vec![common, single];
    // Journal F (radius 5, z 0-10) is on the nominal axis; journal B
    // (radius 6, z 30-40) sits 0.02 mm off it, so the common axis tilts
    // 0.02 mm over the 30 mm between their midpoints.
    let f = cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| 5.0);
    let b = cylinder([HOLE[0] + 0.02, HOLE[1], 30.0], [0.0, 0.0, 1.0], |_| 6.0);
    let slope = 0.02 / 30.0;
    let surface = cylinder(
        [HOLE[0] + slope * 7.0, HOLE[1], 12.0],
        [slope, 0.0, 1.0],
        |_| 8.0,
    );
    let record = InspectionRecord {
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: vec![measured("A2", f), measured("B", b)],
        controls: vec![
            measured("common", surface.clone()),
            measured("single", surface),
        ],
    };
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    // On the common axis the surface has no runout; about F alone its axis
    // drifts from 0.0047 to 0.011 mm off, reading twice the far offset.
    assert!(evaluated(&report, "common").deviation_mm < 1e-6);
    let single = evaluated(&report, "single").deviation_mm;
    let far = slope * (12.0 + 10.0 / (1.0 + slope * slope).sqrt() - 5.0);
    assert!(
        (single - 2.0 * far).abs() < 1e-5,
        "{single} vs {}",
        2.0 * far
    );
    let axis = &report.datum_axes[0];
    assert_eq!(axis.datum_features, ["A2", "B"]);
    assert!((axis.mating_sizes_mm[0] - 10.0).abs() < 1e-6);
    assert!((axis.mating_sizes_mm[1] - 12.0).abs() < 1e-6);
    assert!((axis.direction.x - slope).abs() < 1e-6);
}

#[test]
fn common_datum_plane_contacts_both_pads_at_once() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = common_page();
    let mut parallel = control(
        "parallel",
        "top",
        GeometricCharacteristic::Parallelism,
        0.02,
        &["A"],
    );
    parallel.datums[0].common_with = vec!["E".into()];
    let alone = control(
        "alone",
        "top",
        GeometricCharacteristic::Parallelism,
        0.02,
        &["A"],
    );
    page.feature_control_frames = vec![parallel, alone];
    // Pad A (x 0-20) is at z = 0 and pad E (x 40-60) stands 0.01 mm proud.
    // The common simulator's minimum-zone orientation rises 0.01 mm per
    // 40 mm, so a perfectly flat top reads 60 · 0.00025 = 0.015 mm.
    let record = InspectionRecord {
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: vec![
            measured("A", grid(|u, v| [20.0 * u, 40.0 * v, 0.0])),
            measured("E", grid(|u, v| [40.0 + 20.0 * u, 40.0 * v, -0.01])),
        ],
        controls: vec![
            measured("parallel", grid(|u, v| [60.0 * u, 40.0 * v, 10.0])),
            measured("alone", grid(|u, v| [60.0 * u, 40.0 * v, 10.0])),
        ],
    };
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    let common = evaluated(&report, "parallel").deviation_mm;
    assert!((common - 0.015).abs() < 1e-9, "{common}");
    assert!(evaluated(&report, "alone").deviation_mm < 1e-9);
}

#[test]
fn common_datums_validate_render_and_round_trip() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = common_page();
    let mut frame = control(
        "parallel",
        "top",
        GeometricCharacteristic::Parallelism,
        0.02,
        &["A"],
    );
    frame.datums[0].common_with = vec!["E".into()];
    page.feature_control_frames = vec![frame];
    let session = Session::new().unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(generated.gdt_labels.iter().any(|l| l.text == "A-E"));
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page.clone());
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"common_with\": ["));
    assert_eq!(
        ModelDocument::from_json(&json).unwrap().drawings,
        document.drawings
    );
    for (members, reason) in [
        // Top and bottom are parallel but not coplanar; an axis is not a plane.
        (vec!["top_feature"], "coplanar planes or coaxial axes"),
        (vec!["A2"], "coplanar planes or coaxial axes"),
        (vec!["A"], "repeats"),
        (vec!["E", "B", "A2"], "at most three"),
    ] {
        let mut invalid = page.clone();
        invalid.datum_features.push(DrawingDatumFeature {
            id: "top_feature".into(),
            label: "T".into(),
            feature_of_size: false,
            attachment: attachment("top"),
            size_kind: None,
            size_limits: None,
        });
        invalid.feature_control_frames[0].datums[0].common_with =
            members.into_iter().map(Into::into).collect();
        let message = invalid.validate(&graph).unwrap_err().message;
        assert!(message.contains(reason), "{message}");
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

const PATTERN: [[f64; 2]; 4] = [[15.0, 10.0], [45.0, 10.0], [15.0, 30.0], [45.0, 30.0]];

/// Plate with a center bore datum B (hole axis, Ø9.9-10.1) referenced
/// A | B(`boundary`) | C(front), and four Ø0.04 RFS position controls on a
/// 30 × 20 hole pattern around it.
fn shift_setup(boundary: DatumMaterialBoundary) -> (FamilyDefinition, DrawingDefinition) {
    let mut definition = plate();
    for (i, [x, y]) in PATTERN.into_iter().enumerate() {
        definition.datums.push(DatumDefinition {
            id: format!("p{i}"),
            kind: DatumKind::Axis {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    x,
                    y,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            },
        });
    }
    let mut page = page();
    page.datum_features.retain(|f| f.id != "B");
    let limits = DrawingSizeLimits {
        kind: FeatureOfSizeKind::Internal,
        lower: Quantity::length(9.9, LengthUnit::Millimeter),
        upper: Quantity::length(10.1, LengthUnit::Millimeter),
    };
    page.datum_features.push(DrawingDatumFeature {
        id: "B".into(),
        label: "B".into(),
        feature_of_size: true,
        attachment: attachment("hole"),
        size_kind: Some(FeatureOfSizeKind::Internal),
        size_limits: Some(limits.clone()),
    });
    page.datum_reference_frames.clear();
    page.feature_control_frames = (0..PATTERN.len())
        .map(|i| {
            let mut f = control(
                &format!("hole{i}"),
                &format!("p{i}"),
                GeometricCharacteristic::Position,
                0.04,
                &["A", "B", "C"],
            );
            f.datums[1].boundary = boundary;
            f.zone = GeometricToleranceZone::Diameter;
            f.feature_of_size = true;
            f.size_limits = Some(DrawingSizeLimits {
                kind: FeatureOfSizeKind::Internal,
                lower: Quantity::length(5.9, LengthUnit::Millimeter),
                upper: Quantity::length(6.1, LengthUnit::Millimeter),
            });
            f
        })
        .collect();
    (definition, page)
}

/// A measured part: bore of `bore` diameter at the nominal center, pattern
/// holes displaced by `offsets[i]` from their true positions.
fn shift_record(bore: f64, offsets: [[f64; 2]; 4]) -> InspectionRecord {
    InspectionRecord {
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: vec![
            measured("A", grid(|u, v| [60.0 * u, 40.0 * v, 0.0])),
            measured("B", hole(HOLE, bore)),
            measured("C", grid(|u, v| [60.0 * u, 0.0, 10.0 * v])),
        ],
        controls: (0..4)
            .map(|i| {
                let [x, y] = PATTERN[i];
                measured(
                    &format!("hole{i}"),
                    hole([x + offsets[i][0], y + offsets[i][1]], 6.0),
                )
            })
            .collect(),
    }
}

fn graph_of(definition: &FamilyDefinition) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    graph
}

#[test]
fn position_is_measured_from_the_bore_in_a_plane_axis_plane_frame() {
    let (definition, page) = shift_setup(DatumMaterialBoundary::Regardless);
    let graph = graph_of(&definition);
    // The whole pattern sits 0.05 mm off its nominal place, but so does the
    // bore: located from B, every hole is exactly at true position.
    let mut record = shift_record(10.0, [[0.05, 0.0]; 4]);
    record.datum_features[1] = measured("B", hole([HOLE[0] + 0.05, HOLE[1]], 10.0));
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-6, "{m:?}");
        assert_eq!((m.datum_shift_mm, m.datum_shift_limit_mm), (0.0, 0.0));
        assert!(m.conforms);
    }
}

#[test]
fn maximum_material_boundary_lets_the_pattern_shift_together() {
    let (definition, page) = shift_setup(DatumMaterialBoundary::Maximum);
    let graph = graph_of(&definition);
    // A Ø10.06 bore departs 0.16 mm from its Ø9.9 MMB: the pattern may shift
    // up to 0.08 mm. A uniform 0.05 mm displacement shifts away entirely.
    let report = page
        .evaluate_inspection(&graph, &shift_record(10.06, [[0.05, 0.0]; 4]))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-3, "{m:?}");
        assert!((m.datum_shift_mm - 0.05).abs() < 1e-3, "{m:?}");
        assert!((m.datum_shift_limit_mm - 0.08).abs() < 1e-9, "{m:?}");
        assert!(m.conforms);
    }
    // Opposite displacements cannot be removed by one shared shift: the
    // simultaneous requirement keeps the pattern together.
    let spread = [[0.05, 0.0], [-0.05, 0.0], [0.05, 0.0], [-0.05, 0.0]];
    let report = page
        .evaluate_inspection(&graph, &shift_record(10.06, spread))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!((m.deviation_mm - 0.1).abs() < 1e-3, "{m:?}");
        assert!(m.datum_shift_mm < 1e-3, "{m:?}");
        assert!(!m.conforms);
    }
    // A bore near its MMB (Ø9.92, 0.01 mm shift) can take up only part of
    // the displacement.
    let report = page
        .evaluate_inspection(&graph, &shift_record(9.92, [[0.05, 0.0]; 4]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    assert!((m.datum_shift_mm - 0.01).abs() < 1e-9, "{m:?}");
    assert!((m.deviation_mm - 0.08).abs() < 1e-6, "{m:?}");
    assert!(!m.conforms);
    // Without the boundary (RMB) the same part reads its full displacement.
    let (_, rmb) = shift_setup(DatumMaterialBoundary::Regardless);
    let report = rmb
        .evaluate_inspection(&graph, &shift_record(10.06, [[0.05, 0.0]; 4]))
        .unwrap();
    assert!((evaluated(&report, "hole0").deviation_mm - 0.1).abs() < 1e-6);
}

#[test]
fn least_material_boundary_shift_uses_the_minimum_material_envelope() {
    let (definition, page) = shift_setup(DatumMaterialBoundary::Least);
    let graph = graph_of(&definition);
    // A round Ø10.0 bore is 0.1 mm inside its Ø10.1 LMB: 0.05 mm of shift.
    let report = page
        .evaluate_inspection(&graph, &shift_record(10.0, [[0.03, 0.04]; 4]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    assert!((m.datum_shift_limit_mm - 0.05).abs() < 1e-9, "{m:?}");
    assert!(m.deviation_mm < 1e-3 && m.conforms, "{m:?}");
}

#[test]
fn datum_shift_reports_unsupported_setups_and_validates_limits() {
    let (definition, page) = shift_setup(DatumMaterialBoundary::Maximum);
    let graph = graph_of(&definition);
    // A bore smaller than its MMB violates the boundary.
    let report = page
        .evaluate_inspection(&graph, &shift_record(9.85, [[0.0, 0.0]; 4]))
        .unwrap();
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("violates its maximum material boundary")
        ),
        "{:?}",
        report.controls[0]
    );
    // Without size limits the boundary is unknown.
    let mut unlimited = page.clone();
    unlimited.datum_features.last_mut().unwrap().size_limits = None;
    let report = unlimited
        .evaluate_inspection(&graph, &shift_record(10.0, [[0.0, 0.0]; 4]))
        .unwrap();
    assert!(matches!(
        &report.controls[0].result,
        ControlResult::NotEvaluated(r) if r.contains("needs size limits")
    ));
    // Runout is evaluated regardless of material boundary only.
    let mut runout = page.clone();
    runout.feature_control_frames = vec![control(
        "run",
        "hole",
        GeometricCharacteristic::TotalRunout,
        0.05,
        &["A", "B"],
    )];
    runout.feature_control_frames[0].datums[1].boundary = DatumMaterialBoundary::Maximum;
    let mut record = shift_record(10.0, [[0.0, 0.0]; 4]);
    record.controls = vec![measured("run", hole(HOLE, 10.0))];
    let report = runout.evaluate_inspection(&graph, &record).unwrap();
    assert!(matches!(
        &report.controls[0].result,
        ControlResult::NotEvaluated(r) if r.contains("regardless of material boundary")
    ));
    // Limits need a feature of size and must agree with its kind.
    let mut invalid = page.clone();
    invalid.datum_features.last_mut().unwrap().size_kind = Some(FeatureOfSizeKind::External);
    assert!(
        invalid
            .validate(&graph)
            .unwrap_err()
            .message
            .contains("disagree")
    );
    let mut invalid = page.clone();
    let b = invalid.datum_features.last_mut().unwrap();
    b.feature_of_size = false;
    b.size_kind = None;
    assert!(invalid.validate(&graph).is_err());
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"size_limits\""));
    assert_eq!(
        ModelDocument::from_json(&json).unwrap().drawings,
        document.drawings
    );
}

/// Points on a wall of the plane through `center` with unit in-plane
/// `along` and normal `normal`, offset `offset + wave(s)` along the normal.
fn wall(
    center: [f64; 2],
    normal: [f64; 2],
    offset: f64,
    wave: impl Fn(f64) -> f64,
) -> Vec<[f64; 3]> {
    let along = [-normal[1], normal[0]];
    (0..=8)
        .flat_map(|i| [2.0, 8.0].map(|z| (i as f64 - 4.0, z)))
        .map(|(s, z)| {
            let d = offset + wave(s);
            [
                center[0] + normal[0] * d + along[0] * s,
                center[1] + normal[1] * d + along[1] * s,
                z,
            ]
        })
        .collect()
}

/// `shift_setup` with C a width datum on the slot center plane through the
/// bore (x = 30, normal +x), of the given kind.
fn slot_setup(kind: FeatureOfSizeKind) -> (FamilyDefinition, DrawingDefinition) {
    let (mut definition, mut page) = shift_setup(DatumMaterialBoundary::Regardless);
    definition
        .datums
        .push(plane("slot", [HOLE[0], HOLE[1], 0.0], [1.0, 0.0, 0.0]));
    let c = page
        .datum_features
        .iter_mut()
        .find(|f| f.id == "C")
        .unwrap();
    c.attachment = attachment("slot");
    c.feature_of_size = true;
    c.size_kind = Some(kind);
    page.datum_reference_frames = vec![DrawingDatumReferenceFrame {
        id: "C-only".into(),
        datums: vec![DrawingDatumReference {
            datum_feature: "C".into(),
            boundary: DatumMaterialBoundary::Regardless,
            common_with: Vec::new(),
        }],
    }];
    (definition, page)
}

fn slot_record(angle: f64, pattern_angle: f64, wave: f64) -> InspectionRecord {
    let rotate = |[x, y]: [f64; 2], a: f64| {
        let (s, c) = a.sin_cos();
        [
            HOLE[0] + c * (x - HOLE[0]) - s * (y - HOLE[1]),
            HOLE[1] + s * (x - HOLE[0]) + c * (y - HOLE[1]),
        ]
    };
    let normal = [angle.cos(), angle.sin()];
    // Equal bumps at the center and both ends: turning the planes cannot
    // shrink (tab) or widen (slot) the envelope, so it stays square.
    let bump = move |s: f64| wave * (std::f64::consts::FRAC_PI_4 * s).cos().abs();
    let mut slot = wall(HOLE, normal, 4.0, bump);
    slot.extend(wall(HOLE, normal, -4.0, move |s| -bump(s)));
    let mut record = shift_record(10.0, [[0.0, 0.0]; 4]);
    record.datum_features[2] = measured("C", slot);
    record.controls = (0..4)
        .map(|i| {
            measured(
                &format!("hole{i}"),
                hole(rotate(PATTERN[i], pattern_angle), 6.0),
            )
        })
        .collect();
    record
}

#[test]
fn width_datums_simulate_the_center_plane_of_the_mating_envelope() {
    for (kind, size) in [
        // A tab's faces bulge 0.01 mm: the enclosing envelope is 8.02 wide.
        (FeatureOfSizeKind::External, 8.02),
        // A slot's walls bulge inward 0.01 mm: the widest fit is 7.98.
        (FeatureOfSizeKind::Internal, 7.98),
    ] {
        let (definition, page) = slot_setup(kind);
        let graph = graph_of(&definition);
        let sign = if kind == FeatureOfSizeKind::External {
            1.0
        } else {
            -1.0
        };
        let report = page
            .evaluate_inspection(&graph, &slot_record(0.0, 0.0, sign * 0.01))
            .unwrap();
        let width = &report.datum_widths[0];
        assert_eq!(width.datum_feature, "C");
        assert!(
            (width.mating_size_mm - size).abs() < 1e-6,
            "{kind:?} {width:?}"
        );
        assert!(
            (width.point_mm.x * width.normal.x - HOLE[0]).abs() < 1e-6,
            "{width:?}"
        );
        assert!((width.normal.x.abs() - 1.0).abs() < 1e-9);
    }
    // A slot turned 10 mrad: the fit turns its planes with it and finds the
    // true 8 mm width instead of the wider projection.
    let (definition, page) = slot_setup(FeatureOfSizeKind::Internal);
    let graph = graph_of(&definition);
    let report = page
        .evaluate_inspection(&graph, &slot_record(0.01, 0.0, 0.0))
        .unwrap();
    let width = &report.datum_widths[0];
    assert!((width.mating_size_mm - 8.0).abs() < 1e-6, "{width:?}");
    assert!(
        (width.normal.y / width.normal.x - 0.01_f64.tan()).abs() < 1e-6,
        "{width:?}"
    );
}

#[test]
fn a_slot_center_plane_clocks_a_bore_frame() {
    let (definition, page) = slot_setup(FeatureOfSizeKind::Internal);
    let graph = graph_of(&definition);
    // The part turned 2 mrad about the bore, slot and pattern together: the
    // frame turns with the slot, so every hole is at true position.
    let report = page
        .evaluate_inspection(&graph, &slot_record(0.002, 0.002, 0.0))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-6, "{m:?}");
    }
    // The pattern turned 2 mrad but the slot did not: each hole is off by
    // its radius from the bore (18.03 mm) times the angle.
    let report = page
        .evaluate_inspection(&graph, &slot_record(0.0, 0.002, 0.0))
        .unwrap();
    let radius = 15.0_f64.hypot(10.0);
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(
            (m.deviation_mm - 2.0 * radius * 0.002).abs() < 1e-4,
            "{m:?}"
        );
        assert!(!m.conforms);
    }
}

#[test]
fn width_datums_report_one_sided_common_and_shifted_setups() {
    let (definition, page) = slot_setup(FeatureOfSizeKind::Internal);
    let graph = graph_of(&definition);
    let mut one_sided = slot_record(0.0, 0.0, 0.0);
    one_sided.datum_features[2] = measured("C", wall(HOLE, [1.0, 0.0], 4.0, |_| 0.0));
    let report = page.evaluate_inspection(&graph, &one_sided).unwrap();
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("both faces")
        ),
        "{:?}",
        report.controls[0]
    );
    let mut shifted = page.clone();
    for frame in &mut shifted.feature_control_frames {
        frame.datums[2].boundary = DatumMaterialBoundary::Maximum;
    }
    shifted
        .datum_features
        .iter_mut()
        .find(|f| f.id == "C")
        .unwrap()
        .size_limits = Some(DrawingSizeLimits {
        kind: FeatureOfSizeKind::Internal,
        lower: Quantity::length(7.9, LengthUnit::Millimeter),
        upper: Quantity::length(8.1, LengthUnit::Millimeter),
    });
    let report = shifted
        .evaluate_inspection(&graph, &slot_record(0.0, 0.0, 0.0))
        .unwrap();
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("planar datum features")
        ),
        "{:?}",
        report.controls[0]
    );
}

/// Four pattern holes with composite position: Ø`upper` to A | B | C (the
/// plate's three planes), refined to Ø`lower` relating the pattern with
/// `lower_datums` (a prefix of A, B, C).
fn composite_page(
    upper: f64,
    lower: f64,
    lower_datums: &[&str],
) -> (FamilyDefinition, DrawingDefinition) {
    let (definition, _) = shift_setup(DatumMaterialBoundary::Regardless);
    let mut page = page();
    page.feature_control_frames = (0..PATTERN.len())
        .map(|i| {
            let mut f = control(
                &format!("hole{i}"),
                &format!("p{i}"),
                GeometricCharacteristic::Position,
                upper,
                &["A", "B", "C"],
            );
            f.zone = GeometricToleranceZone::Diameter;
            f.feature_of_size = true;
            f.pattern = Some("4X".into());
            f.size_limits = Some(DrawingSizeLimits {
                kind: FeatureOfSizeKind::Internal,
                lower: Quantity::length(5.9, LengthUnit::Millimeter),
                upper: Quantity::length(6.1, LengthUnit::Millimeter),
            });
            f.refinement = Some(DrawingCompositeRefinement {
                tolerance: Quantity::length(lower, LengthUnit::Millimeter),
                datums: f.datums[..lower_datums.len()].to_vec(),
            });
            f
        })
        .collect();
    (definition, page)
}

/// The pattern moved by `t`, turned by `angle` about its center, and each
/// hole further displaced by `extra[i]`.
fn composite_record(t: [f64; 2], angle: f64, extra: [[f64; 2]; 4]) -> InspectionRecord {
    let center = [30.0, 20.0];
    let mut record = nominal_record();
    record.controls = (0..4)
        .map(|i| {
            let [x, y] = PATTERN[i];
            let (s, c) = f64::sin_cos(angle);
            let (dx, dy) = (x - center[0], y - center[1]);
            let p = [
                center[0] + c * dx - s * dy + t[0] + extra[i][0],
                center[1] + s * dx + c * dy + t[1] + extra[i][1],
            ];
            measured(&format!("hole{i}"), hole(p, 6.0))
        })
        .collect();
    record
}

#[test]
fn composite_position_relates_the_pattern_and_locates_it_loosely() {
    let (definition, page) = composite_page(0.2, 0.04, &["A"]);
    let graph = graph_of(&definition);
    // The whole pattern moved 0.06, 0.02 mm and turned 1 mrad: the loose
    // locating zone holds it, and relative to each other the holes are exact.
    let report = page
        .evaluate_inspection(
            &graph,
            &composite_record([0.06, 0.02], 0.001, [[0.0; 2]; 4]),
        )
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        let lower = m.refinement.clone().unwrap();
        assert!(m.deviation_mm > 0.08 && m.deviation_mm < 0.2, "{m:?}");
        assert!(lower.deviation_mm < 1e-4, "{lower:?}");
        // Rotation is about the frame's z axis, A's outward normal (world -z).
        assert!(
            (lower.pattern_rotation_rad + 0.001).abs() < 1e-5,
            "{lower:?}"
        );
        assert!((lower.pattern_translation_mm - 0.06_f64.hypot(0.02)).abs() < 1e-4);
        assert!(m.conforms);
    }
    // One hole 0.06 mm off its neighbours breaks the feature-relating zone
    // even though the pattern is still well located.
    let mut extra = [[0.0; 2]; 4];
    extra[1] = [0.06, 0.0];
    let report = page
        .evaluate_inspection(&graph, &composite_record([0.0, 0.0], 0.0, extra))
        .unwrap();
    let m = evaluated(&report, "hole1");
    assert!(m.deviation_mm <= 0.2 + 1e-9, "{m:?}");
    let lower = m.refinement.clone().unwrap();
    assert!(lower.deviation_mm > 0.04 && !lower.conforms, "{lower:?}");
    assert!(!m.conforms);
}

#[test]
fn a_refinement_datum_plane_fixes_the_pattern_rotation() {
    // Lower segment to A and B (both planes): the pattern may translate but
    // must stay square to B, so a 1 mrad turn reads 2 · 18.03 · 0.001 mm.
    let (definition, page) = composite_page(0.2, 0.04, &["A", "B"]);
    let graph = graph_of(&definition);
    let report = page
        .evaluate_inspection(&graph, &composite_record([0.05, 0.0], 0.001, [[0.0; 2]; 4]))
        .unwrap();
    let radius = 15.0_f64.hypot(10.0);
    for i in 0..4 {
        let lower = evaluated(&report, &format!("hole{i}")).refinement.unwrap();
        assert_eq!(lower.pattern_rotation_rad, 0.0);
        assert!(
            (lower.deviation_mm - 2.0 * radius * 0.001).abs() < 1e-3,
            "{lower:?}"
        );
        assert!(lower.conforms);
    }
}

#[test]
fn patterns_validate_and_round_trip() {
    let (definition, page) = composite_page(0.2, 0.04, &["A"]);
    let graph = graph_of(&definition);
    let mut mismatched = page.clone();
    mismatched.feature_control_frames[2].tolerance = Quantity::length(0.3, LengthUnit::Millimeter);
    assert!(
        mismatched
            .validate(&graph)
            .unwrap_err()
            .message
            .contains("pattern")
    );
    let mut flat = page.clone();
    flat.feature_control_frames.push({
        let mut f = control("flat", "top", GeometricCharacteristic::Flatness, 0.05, &[]);
        f.pattern = Some("F".into());
        f
    });
    assert!(
        flat.validate(&graph)
            .unwrap_err()
            .message
            .contains("position or profile")
    );
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"pattern\": \"4X\""));
    assert_eq!(
        ModelDocument::from_json(&json).unwrap().drawings,
        document.drawings
    );
}

/// Profile of the top face (a plane, so nominals are optional) to `datums`.
fn top_profile(tolerance: f64, datums: &[&str]) -> DrawingFeatureControlFrame {
    control(
        "top_profile",
        "top",
        GeometricCharacteristic::ProfileSurface,
        tolerance,
        datums,
    )
}

fn profile_report(
    page_controls: Vec<DrawingFeatureControlFrame>,
    controls: Vec<MeasuredFeature>,
) -> InspectionReport {
    let (definition, _) = shift_setup(DatumMaterialBoundary::Regardless);
    let graph = graph_of(&definition);
    let mut page = page();
    page.feature_control_frames = page_controls;
    let mut record = nominal_record();
    record.controls = controls;
    page.evaluate_inspection(&graph, &record).unwrap()
}

/// The top face at `height(x)`.
fn top(height: impl Fn(f64) -> f64) -> MeasuredFeature {
    measured(
        "top_profile",
        grid(|u, v| [60.0 * u, 40.0 * v, height(60.0 * u)]),
    )
}

#[test]
fn surface_profile_is_located_by_its_datums() {
    // Raised 0.03 mm: 2 · 0.03 wide about the true profile 10 mm above A.
    let report = profile_report(vec![top_profile(0.1, &["A"])], vec![top(|_| 10.03)]);
    let m = evaluated(&report, "top_profile");
    let profile = m.profile.clone().unwrap();
    assert!((m.deviation_mm - 0.06).abs() < 1e-9, "{m:?}");
    assert!((profile.min_deviation_mm - 0.03).abs() < 1e-9);
    assert!((profile.max_deviation_mm - 0.03).abs() < 1e-9);
    assert!(m.conforms);
    // Raised 0.06 mm it leaves the 0.1 zone; without datums the profile is
    // form only, so the offset fits away.
    let report = profile_report(vec![top_profile(0.1, &["A"])], vec![top(|_| 10.06)]);
    assert!(!evaluated(&report, "top_profile").conforms);
    let report = profile_report(vec![top_profile(0.1, &[])], vec![top(|_| 10.06)]);
    let m = evaluated(&report, "top_profile");
    assert!(m.deviation_mm < 1e-9 && m.conforms, "{m:?}");
    assert!((m.profile.unwrap().translation_mm - 0.06).abs() < 1e-9);
    // A 0.04 mm tilt over the face stays with A as the datum, and is removed
    // (with a turn of 0.04 / 60 rad) without datums.
    let tilt = |x: f64| 10.0 + 0.04 * x / 60.0;
    let report = profile_report(vec![top_profile(0.1, &["A"])], vec![top(tilt)]);
    assert!((evaluated(&report, "top_profile").deviation_mm - 0.08).abs() < 1e-9);
    let report = profile_report(vec![top_profile(0.1, &[])], vec![top(tilt)]);
    let m = evaluated(&report, "top_profile");
    assert!(m.deviation_mm < 1e-6, "{m:?}");
    assert!((m.profile.unwrap().rotation_rad - (0.04_f64 / 60.0).atan()).abs() < 1e-7);
}

#[test]
fn unequally_disposed_zones_move_outward() {
    let unequal = |outward: f64| {
        let mut f = top_profile(0.1, &["A"]);
        f.unequal = Some(Quantity::length(outward, LengthUnit::Millimeter));
        f
    };
    // All 0.1 outside the material: +0.08 fits, −0.02 does not.
    let report = profile_report(vec![unequal(0.1)], vec![top(|_| 10.08)]);
    let m = evaluated(&report, "top_profile");
    assert!((m.profile.clone().unwrap().zone_center_mm - 0.05).abs() < 1e-12);
    assert!((m.deviation_mm - 0.06).abs() < 1e-9 && m.conforms, "{m:?}");
    let report = profile_report(vec![unequal(0.1)], vec![top(|_| 9.98)]);
    assert!(!evaluated(&report, "top_profile").conforms);
    // All inside: the same +0.08 is out.
    let report = profile_report(vec![unequal(0.0)], vec![top(|_| 10.08)]);
    assert!(!evaluated(&report, "top_profile").conforms);
}

/// Bore points (as `hole`) of `diameter` about `center`, with the nominal
/// Ø10 surface points and normals about `HOLE` pointing out of the material
/// (toward the axis).
fn bore(id: &str, nominal: [f64; 2], center: [f64; 2], diameter: f64) -> MeasuredFeature {
    let mut m = measured(id, hole(center, diameter));
    m.nominals = m
        .points_mm
        .iter()
        .map(|p| {
            let a = (p[1] - center[1]).atan2(p[0] - center[0]);
            MeasuredNominal {
                point_mm: [nominal[0] + 5.0 * a.cos(), nominal[1] + 5.0 * a.sin(), p[2]],
                normal: [-a.cos(), -a.sin(), 0.0],
            }
        })
        .collect();
    m
}

#[test]
fn a_bore_profile_against_nominal_points_controls_size_and_location() {
    let bore_profile = |datums: &[&str]| {
        control(
            "bore",
            "hole",
            GeometricCharacteristic::ProfileSurface,
            0.06,
            datums,
        )
    };
    // Ø10.04 and 0.02 mm off center: from A | B | C the surface lies 0 to
    // 0.04 mm into the material.
    let measured = || bore("bore", HOLE, [HOLE[0] + 0.02, HOLE[1]], 10.04);
    let report = profile_report(vec![bore_profile(&["A", "B", "C"])], vec![measured()]);
    let m = evaluated(&report, "bore");
    let profile = m.profile.clone().unwrap();
    assert!((m.deviation_mm - 0.08).abs() < 1e-4, "{m:?}");
    assert!((profile.min_deviation_mm + 0.04).abs() < 1e-4);
    assert!(profile.max_deviation_mm.abs() < 1e-4);
    assert!(!m.conforms);
    // Unlocated, the offset fits away but the oversize does not: profile
    // controls size.
    let report = profile_report(vec![bore_profile(&[])], vec![measured()]);
    let m = evaluated(&report, "bore");
    assert!((m.deviation_mm - 0.04).abs() < 1e-4, "{m:?}");
    assert!((m.profile.unwrap().translation_mm - 0.02).abs() < 1e-4);
    assert!(m.conforms);
    // A primary axis cannot be measured here, but a plane anchor does not
    // need nominals and other anchors do.
    let mut bare = measured();
    bare.nominals.clear();
    let report = profile_report(vec![bore_profile(&["A"])], vec![bare]);
    assert!(matches!(
        &report.controls[0].result,
        ControlResult::NotEvaluated(reason) if reason.contains("plane datum geometry")
    ));
}

#[test]
fn a_profile_pattern_is_one_simultaneous_requirement_with_a_composite_segment() {
    // 2× profile of two pattern bores to A only: the pair may slide and turn
    // in A together, not separately.
    let pair = |lower: Option<&[&str]>| -> Vec<DrawingFeatureControlFrame> {
        (0..2)
            .map(|i| {
                let mut f = control(
                    &format!("bore{i}"),
                    &format!("p{i}"),
                    GeometricCharacteristic::ProfileSurface,
                    0.1,
                    &["A"],
                );
                f.pattern = Some("2X".into());
                f.refinement = lower.map(|datums| DrawingCompositeRefinement {
                    tolerance: Quantity::length(0.02, LengthUnit::Millimeter),
                    datums: f.datums[..datums.len()].to_vec(),
                });
                f
            })
            .collect()
    };
    let bores = |shift: [[f64; 2]; 2]| -> Vec<MeasuredFeature> {
        (0..2)
            .map(|i| {
                let [x, y] = PATTERN[i];
                let mut m = bore(
                    &format!("bore{i}"),
                    [x, y],
                    [x + shift[i][0], y + shift[i][1]],
                    6.0,
                );
                // Ø6 pattern bores: rescale the Ø10 nominal.
                for (n, p) in m.nominals.iter_mut().zip(&m.points_mm) {
                    n.point_mm = [x - 3.0 * n.normal[0], y - 3.0 * n.normal[1], p[2]];
                }
                m
            })
            .collect()
    };
    // Shifted together: the pair slides back as one.
    let report = profile_report(pair(None), bores([[0.03, 0.0], [0.03, 0.0]]));
    for i in 0..2 {
        assert!(evaluated(&report, &format!("bore{i}")).deviation_mm < 1e-4);
    }
    // Spread apart along their common line, no single move fits both: each
    // keeps its 0.03 mm, where alone it would fit exactly.
    let spread = bores([[-0.03, 0.0], [0.03, 0.0]]);
    let report = profile_report(pair(None), spread.clone());
    for i in 0..2 {
        let m = evaluated(&report, &format!("bore{i}"));
        assert!((m.deviation_mm - 0.06).abs() < 1e-4, "{m:?}");
    }
    let mut alone = pair(None);
    alone.iter_mut().for_each(|f| f.pattern = None);
    let report = profile_report(alone, spread);
    assert!(evaluated(&report, "bore0").deviation_mm < 1e-4);
    // Composite: located loosely to A (0.1), related tightly (0.02) with no
    // datums. Both bores off by 0.04 mm across their line: one 0.08 move
    // fits them together, at 0.02 they relate exactly.
    let report = profile_report(pair(Some(&[])), bores([[0.0, 0.04], [0.0, 0.04]]));
    for i in 0..2 {
        let m = evaluated(&report, &format!("bore{i}"));
        let lower = m.refinement.clone().unwrap();
        assert!(lower.deviation_mm < 1e-4 && lower.conforms, "{lower:?}");
        assert!((lower.pattern_translation_mm - 0.04).abs() < 1e-4);
        assert!(m.conforms);
    }
    // Apart along their line the pair keeps 0.02 mm each: 0.04 > 0.02.
    let report = profile_report(pair(Some(&[])), bores([[-0.02, 0.0], [0.02, 0.0]]));
    let lower = evaluated(&report, "bore0").refinement.unwrap();
    assert!((lower.deviation_mm - 0.04).abs() < 1e-4, "{lower:?}");
    assert!(!lower.conforms);
}

#[test]
fn profile_records_and_unequal_zones_validate_render_and_round_trip() {
    let (definition, _) = shift_setup(DatumMaterialBoundary::Regardless);
    let graph = graph_of(&definition);
    let mut page = page();
    let mut f = top_profile(0.1, &["A"]);
    f.unequal = Some(Quantity::length(0.03, LengthUnit::Millimeter));
    page.feature_control_frames = vec![f.clone()];
    let session = Session::new().unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    for text in ["0.100 mm", "U", "0.030"] {
        assert!(
            generated.gdt_labels.iter().any(|l| l.text == text),
            "{text}"
        );
    }
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
    for (characteristic, outward, reason) in [
        (GeometricCharacteristic::Flatness, 0.03, "profile controls"),
        (
            GeometricCharacteristic::ProfileSurface,
            0.2,
            "from zero to the tolerance",
        ),
        (
            GeometricCharacteristic::ProfileSurface,
            -0.01,
            "from zero to the tolerance",
        ),
    ] {
        let mut bad = page.clone();
        let g = &mut bad.feature_control_frames[0];
        g.characteristic = characteristic;
        if characteristic == GeometricCharacteristic::Flatness {
            g.datums.clear();
        }
        g.unequal = Some(Quantity::length(outward, LengthUnit::Millimeter));
        let error = bad.validate(&graph).unwrap_err().message;
        assert!(error.contains(reason), "{error}");
    }
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page.clone());
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"unequal\""));
    assert_eq!(
        ModelDocument::from_json(&json).unwrap().drawings,
        document.drawings
    );
    // Records: one nominal per point, finite, only on profile controls.
    let mut record = nominal_record();
    let mut top_points = top(|_| 10.0);
    top_points.nominals = vec![
        MeasuredNominal {
            point_mm: [0.0, 0.0, 10.0],
            normal: [0.0, 0.0, 1.0],
        };
        top_points.points_mm.len() - 1
    ];
    record.controls = vec![top_points.clone()];
    let error = page
        .evaluate_inspection(&graph, &record)
        .unwrap_err()
        .message;
    assert!(error.contains("one nominal per measured point"), "{error}");
    top_points.nominals.push(MeasuredNominal {
        point_mm: [0.0, 0.0, 10.0],
        normal: [0.0, 0.0, 0.0],
    });
    record.controls = vec![top_points];
    let error = page
        .evaluate_inspection(&graph, &record)
        .unwrap_err()
        .message;
    assert!(error.contains("nonzero finite normals"), "{error}");
    let mut flat_page = page.clone();
    flat_page.feature_control_frames = vec![control(
        "flat",
        "top",
        GeometricCharacteristic::Flatness,
        0.05,
        &[],
    )];
    let mut flat = measured("flat", grid(|u, v| [60.0 * u, 40.0 * v, 10.0]));
    flat.nominals = vec![
        MeasuredNominal {
            point_mm: [0.0, 0.0, 10.0],
            normal: [0.0, 0.0, 1.0],
        };
        flat.points_mm.len()
    ];
    record.controls = vec![flat];
    let error = flat_page
        .evaluate_inspection(&graph, &record)
        .unwrap_err()
        .message;
    assert!(error.contains("profile controls"), "{error}");
    let json = serde_json::to_string(&record).unwrap();
    assert_eq!(
        serde_json::from_str::<InspectionRecord>(&json).unwrap(),
        record
    );
    // Line profiles and shifted datums are reported, not evaluated.
    let mut line = top_profile(0.1, &["A"]);
    line.characteristic = GeometricCharacteristic::ProfileLine;
    let report = profile_report(vec![line], vec![top(|_| 10.0)]);
    assert!(matches!(
        &report.controls[0].result,
        ControlResult::NotEvaluated(reason) if reason.contains("profile of a line")
    ));
}

/// The plate as real geometry, a 60 × 40 × 10 block cut by a Ø10 bore at
/// `HOLE`, with named face references for the bore wall and the top.
fn solid_plate() -> FamilyDefinition {
    let mut definition = plate();
    let at = |x: f64, y: f64, z: f64| {
        VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
    };
    let mm = |v: f64| ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter));
    definition.features = vec![
        FeatureDefinition {
            id: "block".into(),
            operation: FeatureOperation::Box {
                origin: at(0.0, 0.0, 0.0),
                size: at(60.0, 40.0, 10.0),
            },
        },
        FeatureDefinition {
            id: "bore".into(),
            operation: FeatureOperation::Cylinder {
                origin: at(HOLE[0], HOLE[1], -1.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: mm(5.0),
                height: mm(12.0),
            },
        },
        FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Cut {
                object: "block".into(),
                tool: "bore".into(),
            },
        },
    ];
    definition.references = vec![
        NamedReference {
            name: "bore_wall".into(),
            target: ReferenceTarget::Faces(FaceSelector::NearestCenter {
                target: at(HOLE[0], HOLE[1], 5.0),
                maximum_distance: mm(1.0),
            }),
        },
        NamedReference {
            name: "top".into(),
            target: ReferenceTarget::Faces(FaceSelector::NormalAligned {
                direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999)),
            }),
        },
    ];
    definition
}

fn surface_profile(
    id: &str,
    anchor: &str,
    face: &str,
    tolerance: f64,
    datums: &[&str],
) -> DrawingFeatureControlFrame {
    let mut f = control(
        id,
        anchor,
        GeometricCharacteristic::ProfileSurface,
        tolerance,
        datums,
    );
    f.surface = Some(FaceSelector::Named(face.into()));
    f
}

#[test]
fn profile_nominals_come_from_the_regenerated_part() {
    let definition = solid_plate();
    let graph = graph_of(&definition);
    let session = Session::new().unwrap();
    let mut page = page();
    page.feature_control_frames = vec![
        surface_profile("bore", "hole", "bore_wall", 0.06, &["A", "B", "C"]),
        surface_profile("top_profile", "top", "top", 0.1, &["A"]),
    ];
    // The same Ø10.04 bore 0.02 mm off center as with explicit nominals.
    let mut record = nominal_record();
    record.controls = vec![
        measured("bore", hole([HOLE[0] + 0.02, HOLE[1]], 10.04)),
        top(|_| 10.03),
    ];
    let projected = page
        .project_profile_nominals(&graph, &session, &record)
        .unwrap();
    for (n, p) in projected.controls[0]
        .nominals
        .iter()
        .zip(&projected.controls[0].points_mm)
    {
        // On the Ø10 wall, normal toward the axis (out of the material).
        let radial = [n.point_mm[0] - HOLE[0], n.point_mm[1] - HOLE[1]];
        assert!((radial[0].hypot(radial[1]) - 5.0).abs() < 1e-7);
        assert!((n.normal[0] * radial[0] + n.normal[1] * radial[1] + 5.0).abs() < 1e-6);
        assert!((n.point_mm[2] - p[2]).abs() < 1e-7);
    }
    let report = page
        .evaluate_inspection_on_model(&graph, &session, &record)
        .unwrap();
    let wall = evaluated(&report, "bore");
    assert!((wall.deviation_mm - 0.08).abs() < 1e-4, "{wall:?}");
    assert!(!wall.conforms);
    let top_face = evaluated(&report, "top_profile");
    assert!((top_face.deviation_mm - 0.06).abs() < 1e-9, "{top_face:?}");
    // Nominals already in the record are kept.
    let mut given = record.clone();
    given.controls[0] = bore("bore", HOLE, [HOLE[0] + 0.02, HOLE[1]], 10.04);
    let kept = page
        .project_profile_nominals(&graph, &session, &given)
        .unwrap();
    assert_eq!(kept.controls[0], given.controls[0]);
}

#[test]
fn profile_nominals_follow_the_instance_placement() {
    let definition = solid_plate();
    let mut graph = graph_of(&definition);
    // Turned a quarter about z and moved: measured points are in the model.
    graph
        .set_placement(
            "part",
            Placement {
                translation: VectorQuantity::lengths(100.0, 50.0, 5.0, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: std::f64::consts::FRAC_PI_2,
                }),
            },
        )
        .unwrap();
    let session = Session::new().unwrap();
    let mut page = page();
    page.feature_control_frames = vec![surface_profile("bore", "hole", "bore_wall", 0.06, &[])];
    let place = |p: [f64; 3]| [100.0 - p[1], 50.0 + p[0], 5.0 + p[2]];
    let axis = place([HOLE[0], HOLE[1], 0.0]);
    let mut record = nominal_record();
    record.datum_features.clear();
    record.controls = vec![measured(
        "bore",
        hole([HOLE[0] + 0.02, HOLE[1]], 10.04)
            .into_iter()
            .map(place)
            .collect(),
    )];
    let projected = page
        .project_profile_nominals(&graph, &session, &record)
        .unwrap();
    for n in &projected.controls[0].nominals {
        let radial = [n.point_mm[0] - axis[0], n.point_mm[1] - axis[1]];
        assert!((radial[0].hypot(radial[1]) - 5.0).abs() < 1e-7, "{n:?}");
        assert!((n.normal[0] * radial[0] + n.normal[1] * radial[1] + 5.0).abs() < 1e-6);
    }
    // Unlocated, only the 0.02 mm oversize per side remains.
    let report = page.evaluate_inspection(&graph, &projected).unwrap();
    let bore = evaluated(&report, "bore");
    assert!((bore.deviation_mm - 0.04).abs() < 1e-4, "{bore:?}");
}

#[test]
fn profile_surfaces_validate_and_round_trip() {
    let definition = solid_plate();
    let graph = graph_of(&definition);
    let mut page = page();
    page.feature_control_frames = vec![surface_profile("bore", "hole", "bore_wall", 0.06, &[])];
    page.validate(&graph).unwrap();
    let mut flat = page.clone();
    flat.feature_control_frames[0].characteristic = GeometricCharacteristic::Flatness;
    let error = flat.validate(&graph).unwrap_err().message;
    assert!(error.contains("profile controls"), "{error}");
    let mut unknown = page.clone();
    unknown.feature_control_frames[0].surface = Some(FaceSelector::Named("missing".into()));
    let error = unknown.validate(&graph).unwrap_err().message;
    assert!(
        error.contains("unknown face reference 'missing'"),
        "{error}"
    );
    // Pattern members name their own faces.
    let mut pattern = page.clone();
    pattern.feature_control_frames[0].pattern = Some("2X".into());
    let mut second = pattern.feature_control_frames[0].clone();
    second.id = "top_profile".into();
    second.attachment = attachment("top");
    second.surface = Some(FaceSelector::Named("top".into()));
    pattern.feature_control_frames.push(second);
    pattern.validate(&graph).unwrap();
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(pattern);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"surface\""));
    assert_eq!(
        ModelDocument::from_json(&json).unwrap().drawings,
        document.drawings
    );
}
