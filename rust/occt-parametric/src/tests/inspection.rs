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
        faces: None,
    }
}

pub(super) fn control(
    id: &str,
    anchor: &str,
    characteristic: GeometricCharacteristic,
    tolerance: f64,
    datums: &[&str],
) -> DrawingFeatureControlFrame {
    DrawingFeatureControlFrame {
        modifiers: Vec::new(),
        projected: None,
        tangent_plane: false,
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
                translation: false,
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
        annotation_views: Vec::new(),
        datum_reference_frames: vec![DrawingDatumReferenceFrame {
            id: "ABC".into(),
            datums: ["A", "B", "C"]
                .iter()
                .map(|d| DrawingDatumReference {
                    translation: false,
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
        uncertainty: None,
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
        refinement: None,
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
        ControlResult::NotEvaluated(r) if r.contains("regardless of material boundary")
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
        uncertainty: None,
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
    // u ⟂ w from the x axis (the y axis for an axis near x), v = w × u.
    let seed = if w[0].abs() < 0.9 { 0 } else { 1 };
    let d = w[seed];
    let u = {
        let mut u = [-d * w[0], -d * w[1], -d * w[2]];
        u[seed] += 1.0;
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
        uncertainty: None,
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
                    translation: false,
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
        uncertainty: None,
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
                    translation: false,
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
            translation: false,
            datum_feature: "A2".into(),
            boundary: DatumMaterialBoundary::Regardless,
            common_with: vec!["B".into()],
        }],
    }];
    page
}

#[test]
fn common_datum_axis_contracts_both_journal_simulators_together() {
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
    // (radius 6, z 30-40) sits 0.02 mm off it. Coaxial simulators
    // contracting together grow least, 0.0033 mm each, on the axis tilting
    // 0.02 mm over the 30 mm between the journals' midpoints.
    let f = cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| 5.0);
    let b = cylinder([HOLE[0] + 0.02, HOLE[1], 30.0], [0.0, 0.0, 1.0], |_| 6.0);
    let slope = 0.02 / 30.0;
    let surface = cylinder(
        [HOLE[0] + slope * 7.0, HOLE[1], 12.0],
        [slope, 0.0, 1.0],
        |_| 8.0,
    );
    let record = InspectionRecord {
        uncertainty: None,
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
    // With B twice as long (z 30-50), the simulators contract least on the
    // axis through F's axis at z = 0 rising 0.0005 per mm: the journals'
    // axes sit 0.005 mm from it at z = 10, 30 and 50, alternately on each
    // side. The line through the midpoints (z 5 and 40) would read about
    // 0.004 mm of runout on a surface about that axis.
    let mut long = cylinder([HOLE[0] + 0.02, HOLE[1], 30.0], [0.0, 0.0, 1.0], |_| 6.0);
    long.extend(cylinder(
        [HOLE[0] + 0.02, HOLE[1], 40.0],
        [0.0, 0.0, 1.0],
        |_| 6.0,
    ));
    let slope = 0.0005;
    let surface = cylinder(
        [HOLE[0] + slope * 12.0, HOLE[1], 12.0],
        [slope, 0.0, 1.0],
        |_| 8.0,
    );
    let record = InspectionRecord {
        uncertainty: None,
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: vec![
            measured(
                "A2",
                cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| 5.0),
            ),
            measured("B", long),
        ],
        controls: vec![
            measured("common", surface.clone()),
            measured("single", surface),
        ],
    };
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    let common = evaluated(&report, "common").deviation_mm;
    assert!(common < 1e-6, "{common}");
    let axis = &report.datum_axes[0];
    assert!((axis.direction.x - slope).abs() < 1e-7, "{axis:?}");
    assert!(axis.direction.y.abs() < 1e-9, "{axis:?}");
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
        uncertainty: None,
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
        uncertainty: None,
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
            translation: false,
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
fn width_datums_need_both_faces() {
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
}

fn slot_limits() -> DrawingSizeLimits {
    DrawingSizeLimits {
        kind: FeatureOfSizeKind::Internal,
        lower: Quantity::length(7.9, LengthUnit::Millimeter),
        upper: Quantity::length(8.1, LengthUnit::Millimeter),
    }
}

/// `slot_setup` with C a 7.9-8.1 slot referenced A | B(`b`) | C(`c`): its
/// center plane x = 30 passes through the bore, and the slot is measured
/// 21-29 mm from it.
fn clocking_setup(
    b: DatumMaterialBoundary,
    c: DatumMaterialBoundary,
) -> (FamilyDefinition, DrawingDefinition) {
    let (definition, mut page) = slot_setup(FeatureOfSizeKind::Internal);
    let feature = page
        .datum_features
        .iter_mut()
        .find(|f| f.id == "C")
        .unwrap();
    feature.size_limits = Some(slot_limits());
    for frame in &mut page.feature_control_frames {
        frame.datums[1].boundary = b;
        frame.datums[2].boundary = c;
    }
    (definition, page)
}

/// A measured part: Ø`bore` bore at the nominal center, a slot of `width`
/// 25 mm from it, and the pattern turned by `turn` about the bore and moved
/// by `t`.
fn clocking_record(bore: f64, width: f64, turn: f64, t: [f64; 2]) -> InspectionRecord {
    let mut record = shift_record(bore, [[0.0, 0.0]; 4]);
    let center = [HOLE[0], HOLE[1] + 25.0];
    let mut slot = wall(center, [1.0, 0.0], width / 2.0, |_| 0.0);
    slot.extend(wall(center, [1.0, 0.0], -width / 2.0, |_| 0.0));
    record.datum_features[2] = measured("C", slot);
    let (s, c) = turn.sin_cos();
    record.controls = PATTERN
        .iter()
        .enumerate()
        .map(|(i, [x, y])| {
            let (dx, dy) = (x - HOLE[0], y - HOLE[1]);
            let at = [
                HOLE[0] + c * dx - s * dy + t[0],
                HOLE[1] + s * dx + c * dy + t[1],
            ];
            measured(&format!("hole{i}"), hole(at, 6.0))
        })
        .collect();
    record
}

#[test]
fn a_slot_at_a_material_boundary_lets_the_frame_turn_about_the_bore() {
    use DatumMaterialBoundary::{Least, Maximum, Regardless};
    let lever = 15.0_f64.hypot(10.0);
    let (definition, page) = clocking_setup(Regardless, Maximum);
    let graph = graph_of(&definition);
    // An 8.0 slot clears its 7.9 MMB by 0.05 mm a side; at 29 mm from the
    // bore that lets the frame turn 1.72 mrad. A pattern turned 1.5 mrad
    // turns back to true position (negative about A's outward normal, -z).
    let report = page
        .evaluate_inspection(&graph, &clocking_record(10.0, 8.0, 0.0015, [0.0; 2]))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-4, "{m:?}");
        assert!((m.datum_shift_rotation_rad + 0.0015).abs() < 1e-5, "{m:?}");
        assert!(
            m.datum_shift_mm < 1e-9 && m.datum_shift_limit_mm == 0.0,
            "{m:?}"
        );
        assert!(m.conforms);
    }
    // Turned 3 mrad, the frame stops where the slot's far end meets the
    // boundary: 4·cos θ − 29·sin θ = 3.95.
    let report = page
        .evaluate_inspection(&graph, &clocking_record(10.0, 8.0, 0.003, [0.0; 2]))
        .unwrap();
    let limit = {
        let mut a = 0.0_f64;
        for _ in 0..50 {
            a -= (4.0 * a.cos() - 29.0 * a.sin() - 3.95) / (-4.0 * a.sin() - 29.0 * a.cos());
        }
        a
    };
    let m = evaluated(&report, "hole0");
    assert!((m.datum_shift_rotation_rad + limit).abs() < 1e-6, "{m:?}");
    let expected = 2.0 * 2.0 * lever * (0.5 * (0.003 - limit)).sin();
    assert!((m.deviation_mm - expected).abs() < 1e-5, "{m:?} {expected}");
    // Regardless of material boundary the slot clocks the frame square.
    let (_, rmb) = clocking_setup(Regardless, Regardless);
    let report = rmb
        .evaluate_inspection(&graph, &clocking_record(10.0, 8.0, 0.0015, [0.0; 2]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    assert!((m.deviation_mm - 2.0 * 2.0 * lever * 0.00075_f64.sin()).abs() < 1e-6);
    assert_eq!(m.datum_shift_rotation_rad, 0.0);
    assert!(!m.conforms);
    // At LMB the 8.1 boundary encloses the 8.0 slot with the same clearance.
    let (_, lmb) = clocking_setup(Regardless, Least);
    let report = lmb
        .evaluate_inspection(&graph, &clocking_record(10.0, 8.0, 0.0015, [0.0; 2]))
        .unwrap();
    assert!(evaluated(&report, "hole0").deviation_mm < 1e-4);
    // A slot narrower than its MMB violates the boundary.
    let report = page
        .evaluate_inspection(&graph, &clocking_record(10.0, 7.85, 0.0, [0.0; 2]))
        .unwrap();
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("violates its maximum material boundary")
        ),
        "{:?}",
        report.controls[0]
    );
}

#[test]
fn bore_and_slot_boundaries_shift_and_turn_the_frame_together() {
    use DatumMaterialBoundary::Maximum;
    let (definition, page) = clocking_setup(Maximum, Maximum);
    let graph = graph_of(&definition);
    // A Ø10.06 bore allows 0.08 mm of translation; the slot block moves with
    // it, so a 0.05 mm move and a 1.5 mrad turn (0.0125-0.0065 mm net at
    // the slot) are both taken up.
    let record = clocking_record(10.06, 8.0, 0.0015, [0.05, 0.0]);
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-4, "{m:?}");
        assert!((m.datum_shift_mm - 0.05).abs() < 1e-4, "{m:?}");
        assert!((m.datum_shift_rotation_rad + 0.0015).abs() < 1e-5, "{m:?}");
        assert!((m.datum_shift_limit_mm - 0.08).abs() < 1e-9, "{m:?}");
    }
    // Moving 0.05 mm the other way uses the slot's clearance against the
    // turn: the block would sit 0.05 + 29·θ from center, so the turn
    // cannot be taken up in full.
    let record = clocking_record(10.06, 8.0, 0.0015, [-0.05, 0.0]);
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    let m = evaluated(&report, "hole0");
    assert!(m.deviation_mm > 1e-3, "{m:?}");
}

/// Plate frame A | B | C of two slot widths: B's center plane x = 30
/// (measured 11-19 mm above the origin) and C's y = 20 (measured 21-29 mm
/// left of it), both 7.9-8.1, with the hole pattern about (30, 20).
fn width_frame_setup(
    b: DatumMaterialBoundary,
    c: DatumMaterialBoundary,
) -> (FamilyDefinition, DrawingDefinition) {
    let (mut definition, mut page) = shift_setup(DatumMaterialBoundary::Regardless);
    definition
        .datums
        .push(plane("xslot", [HOLE[0], 0.0, 0.0], [1.0, 0.0, 0.0]));
    definition
        .datums
        .push(plane("yslot", [0.0, HOLE[1], 0.0], [0.0, 1.0, 0.0]));
    for (id, anchor) in [("B", "xslot"), ("C", "yslot")] {
        let feature = page.datum_features.iter_mut().find(|f| f.id == id).unwrap();
        feature.attachment = attachment(anchor);
        feature.feature_of_size = true;
        feature.size_kind = Some(FeatureOfSizeKind::Internal);
        feature.size_limits = Some(slot_limits());
    }
    for frame in &mut page.feature_control_frames {
        frame.datums[1].boundary = b;
        frame.datums[2].boundary = c;
    }
    (definition, page)
}

fn width_frame_record(turn: f64, t: [f64; 2]) -> InspectionRecord {
    let mut record = clocking_record(10.0, 8.0, turn, t);
    let slot = |center: [f64; 2], normal: [f64; 2]| {
        let mut points = wall(center, normal, 4.0, |_| 0.0);
        points.extend(wall(center, normal, -4.0, |_| 0.0));
        points
    };
    record.datum_features[1] = measured("B", slot([HOLE[0], HOLE[1] + 15.0], [1.0, 0.0]));
    record.datum_features[2] = measured("C", slot([HOLE[0] - 25.0, HOLE[1]], [0.0, 1.0]));
    record
}

#[test]
fn slot_and_tab_width_datums_shift_a_planar_frame() {
    use DatumMaterialBoundary::{Maximum, Regardless};
    let (definition, page) = width_frame_setup(Maximum, Maximum);
    let graph = graph_of(&definition);
    // Each 8.0 slot clears its 7.9 MMB by 0.05 mm: the origin may move up
    // to 0.05 mm along each normal.
    let report = page
        .evaluate_inspection(&graph, &width_frame_record(0.0, [0.03, -0.04]))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-4, "{m:?}");
        assert!((m.datum_shift_mm - 0.05).abs() < 1e-4, "{m:?}");
        assert!((m.datum_shift_limit_mm - 0.05 * 2.0_f64.sqrt()).abs() < 1e-9);
    }
    // The secondary slot also lets the frame turn: 1 mrad moves the
    // tertiary slot 21-29 µm, inside its clearance.
    let report = page
        .evaluate_inspection(&graph, &width_frame_record(0.001, [0.0; 2]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    assert!(m.deviation_mm < 1e-4, "{m:?}");
    assert!((m.datum_shift_rotation_rad + 0.001).abs() < 1e-5, "{m:?}");
    // A secondary slot regardless of size fixes rotation and its own
    // translation; the tertiary still shifts.
    let (_, square) = width_frame_setup(Regardless, Maximum);
    let report = square
        .evaluate_inspection(&graph, &width_frame_record(0.001, [0.0, 0.04]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    let lever = 15.0_f64.hypot(10.0);
    assert!(
        (m.deviation_mm - 2.0 * 2.0 * lever * 0.0005_f64.sin()).abs() < 1e-4,
        "{m:?}"
    );
    assert_eq!(m.datum_shift_rotation_rad, 0.0);
    // A shifting secondary slot with a tertiary regardless of size would
    // have to re-contact as it turns.
    let (_, coupled) = width_frame_setup(Maximum, Regardless);
    let report = coupled
        .evaluate_inspection(&graph, &width_frame_record(0.0, [0.0; 2]))
        .unwrap();
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("must be a width at a material boundary")
        ),
        "{:?}",
        report.controls[0]
    );
    // Tabs: the 8.1 MMB pair encloses an 8.0 tab with the same clearance.
    let (_, mut tabs) = width_frame_setup(Maximum, Maximum);
    for feature in &mut tabs.datum_features {
        if feature.feature_of_size {
            feature.size_kind = Some(FeatureOfSizeKind::External);
            if let Some(limits) = &mut feature.size_limits {
                limits.kind = FeatureOfSizeKind::External;
            }
        }
    }
    let report = tabs
        .evaluate_inspection(&graph, &width_frame_record(0.0, [0.03, -0.04]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    assert!(m.deviation_mm < 1e-4 && m.conforms, "{m:?}");
}

/// Plate drawing with a primary datum bore D (Ø9.9-10.1 on the hole axis,
/// referenced at `boundary`) and Ø0.04 RFS position controls `ids` on
/// Ø15.9-16.1 counterbores coaxial with it.
fn coaxial_page(boundary: DatumMaterialBoundary, ids: &[&str]) -> DrawingDefinition {
    let mut page = page();
    page.datum_features.push(DrawingDatumFeature {
        id: "D".into(),
        label: "D".into(),
        feature_of_size: true,
        attachment: attachment("hole"),
        size_kind: None,
        size_limits: Some(DrawingSizeLimits {
            kind: FeatureOfSizeKind::Internal,
            lower: Quantity::length(9.9, LengthUnit::Millimeter),
            upper: Quantity::length(10.1, LengthUnit::Millimeter),
        }),
    });
    page.datum_reference_frames.clear();
    page.feature_control_frames = ids
        .iter()
        .map(|id| {
            let mut f = control(id, "hole", GeometricCharacteristic::Position, 0.04, &["D"]);
            f.datums[0].boundary = boundary;
            f.zone = GeometricToleranceZone::Diameter;
            f.feature_of_size = true;
            f.size_limits = Some(DrawingSizeLimits {
                kind: FeatureOfSizeKind::Internal,
                lower: Quantity::length(15.9, LengthUnit::Millimeter),
                upper: Quantity::length(16.1, LengthUnit::Millimeter),
            });
            f
        })
        .collect();
    page
}

/// A measured Ø`bore` datum bore over z 0-10 on the nominal axis, and Ø16
/// counterbores over z 10-20 whose axes are offset in x by `ends[i]` at
/// z = 10 and z = 20.
fn coaxial_record(bore: f64, ends: &[[f64; 2]]) -> InspectionRecord {
    InspectionRecord {
        uncertainty: None,
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: vec![measured(
            "D",
            cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| bore / 2.0),
        )],
        controls: ends
            .iter()
            .enumerate()
            .map(|(i, [low, high])| {
                let base = [HOLE[0] + low, HOLE[1], 10.0];
                measured(
                    &format!("cbore{i}"),
                    cylinder(base, [high - low, 0.0, 10.0], |_| 8.0),
                )
            })
            .collect(),
    }
}

#[test]
fn coaxial_position_is_measured_over_the_feature_length_from_a_primary_axis() {
    let definition = plate();
    let graph = graph_of(&definition);
    let page = coaxial_page(DatumMaterialBoundary::Regardless, &["cbore0"]);
    // Offset 0.03 mm throughout: twice the offset.
    let report = page
        .evaluate_inspection(&graph, &coaxial_record(10.0, &[[0.03, 0.03]]))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    assert!((m.deviation_mm - 0.06).abs() < 1e-6, "{m:?}");
    assert!(!m.conforms);
    assert_eq!((m.datum_shift_mm, m.datum_shift_limit_mm), (0.0, 0.0));
    assert!((m.actual_size_mm.unwrap() - 16.0).abs() < 1e-6, "{m:?}");
    // Square at its base but leaning 0.015 mm at its top end: the worse end.
    let report = page
        .evaluate_inspection(&graph, &coaxial_record(10.0, &[[0.0, 0.015]]))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    assert!((m.deviation_mm - 0.03).abs() < 1e-6, "{m:?}");
    assert!(m.conforms);
    // A feature off the datum axis but measured on it: no turn about the
    // axis brings it back out to its true position.
    let mut definition = plate();
    definition.datums.push(DatumDefinition {
        id: "off".into(),
        kind: DatumKind::Axis {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                35.0,
                20.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        },
    });
    let graph = graph_of(&definition);
    let mut off = page.clone();
    off.feature_control_frames[0].attachment = attachment("off");
    let report = off
        .evaluate_inspection(&graph, &coaxial_record(10.0, &[[0.0, 0.0]]))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    assert!((m.deviation_mm - 10.0).abs() < 1e-6, "{m:?}");
}

#[test]
fn a_primary_axis_at_a_material_boundary_shifts_and_tilts() {
    use DatumMaterialBoundary::{Least, Maximum};
    let definition = plate();
    let graph = graph_of(&definition);
    let page = coaxial_page(Maximum, &["cbore0"]);
    // A Ø10.06 bore clears its Ø9.9 MMB by 0.08 mm a side: an offset of
    // 0.05 mm shifts away.
    let report = page
        .evaluate_inspection(&graph, &coaxial_record(10.06, &[[0.05, 0.05]]))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    assert!(m.deviation_mm < 1e-6 && m.conforms, "{m:?}");
    assert!((m.datum_shift_mm - 0.05).abs() < 1e-6, "{m:?}");
    assert!(m.datum_shift_rotation_rad < 1e-9, "{m:?}");
    assert!((m.datum_shift_limit_mm - 0.08).abs() < 1e-9, "{m:?}");
    // Leaning 0.1 mm over z 10-20 needs the datum axis to tilt beyond its
    // ends' 0.08 mm: with its far end at -0.08 mm and its near end at a,
    // the counterbore ends sit a and 0.02 - 2a off; a = 0.02/3 balances
    // them.
    let report = page
        .evaluate_inspection(&graph, &coaxial_record(10.06, &[[0.0, 0.1]]))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    assert!((m.deviation_mm - 0.04 / 3.0).abs() < 1e-6, "{m:?}");
    assert!(
        (m.datum_shift_rotation_rad - (0.08 + 0.02 / 3.0) / 10.0).abs() < 1e-6,
        "{m:?}"
    );
    // Coaxial controls with the same datum references share one shift.
    let pair = coaxial_page(Maximum, &["cbore0", "cbore1"]);
    let report = pair
        .evaluate_inspection(
            &graph,
            &coaxial_record(10.06, &[[0.05, 0.05], [-0.05, -0.05]]),
        )
        .unwrap();
    for id in ["cbore0", "cbore1"] {
        let m = evaluated(&report, id);
        assert!((m.deviation_mm - 0.1).abs() < 1e-6, "{m:?}");
        assert!(m.datum_shift_mm < 1e-6, "{m:?}");
    }
    // LMB: a Ø10.0 bore is 0.1 mm inside its Ø10.1 boundary.
    let report = coaxial_page(Least, &["cbore0"])
        .evaluate_inspection(&graph, &coaxial_record(10.0, &[[0.03, 0.03]]))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    assert!((m.datum_shift_limit_mm - 0.05).abs() < 1e-9, "{m:?}");
    assert!(m.deviation_mm < 1e-6, "{m:?}");
    // A bore inside its MMB violates the boundary.
    let report = page
        .evaluate_inspection(&graph, &coaxial_record(9.85, &[[0.0, 0.0]]))
        .unwrap();
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("violates its maximum material boundary")
        ),
        "{:?}",
        report.controls[0]
    );
}

/// `clocking_setup` with each control referenced to the datums `order`
/// (the bore B first), and a named frame B | A | C.
fn axis_clocking_setup(
    order: &[&str],
    b: DatumMaterialBoundary,
    c: DatumMaterialBoundary,
) -> (FamilyDefinition, DrawingDefinition) {
    let (definition, mut page) = clocking_setup(b, c);
    for frame in &mut page.feature_control_frames {
        let datums = std::mem::take(&mut frame.datums);
        frame.datums = order
            .iter()
            .map(|id| {
                datums
                    .iter()
                    .find(|d| d.datum_feature == *id)
                    .unwrap()
                    .clone()
            })
            .collect();
    }
    page.datum_reference_frames = vec![DrawingDatumReferenceFrame {
        id: "B-A-C".into(),
        datums: ["B", "A", "C"]
            .map(|id| DrawingDatumReference {
                translation: false,
                datum_feature: id.into(),
                boundary: DatumMaterialBoundary::Regardless,
                common_with: Vec::new(),
            })
            .to_vec(),
    }];
    (definition, page)
}

#[test]
fn a_slot_clocks_position_off_a_primary_datum_axis() {
    use DatumMaterialBoundary::Regardless;
    let lever = 15.0_f64.hypot(10.0);
    let (definition, page) = axis_clocking_setup(&["B", "A", "C"], Regardless, Regardless);
    let graph = graph_of(&definition);
    // Bore, slot and pattern all 0.05 mm off nominal: located from the bore
    // and clocked by the slot, every hole is at true position.
    let mut record = clocking_record(10.0, 8.0, 0.0, [0.05, 0.0]);
    record.datum_features[1] = measured("B", hole([HOLE[0] + 0.05, HOLE[1]], 10.0));
    let center = [HOLE[0] + 0.05, HOLE[1] + 25.0];
    let mut slot = wall(center, [1.0, 0.0], 4.0, |_| 0.0);
    slot.extend(wall(center, [1.0, 0.0], -4.0, |_| 0.0));
    record.datum_features[2] = measured("C", slot);
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-6 && m.conforms, "{m:?}");
        assert_eq!((m.datum_shift_mm, m.datum_shift_rotation_rad), (0.0, 0.0));
    }
    // The named frame: origin where the face meets the bore axis, z along
    // the bore, x along the slot's normal.
    let frame = &report.datum_frames[0];
    assert_eq!(frame.datum_features, ["B", "A", "C"]);
    let f = &frame.frame;
    assert!(
        (f.origin_mm.x - 30.05).abs() < 1e-6 && f.origin_mm.z.abs() < 1e-6,
        "{f:?}"
    );
    assert!(
        (f.z_axis.z - 1.0).abs() < 1e-9 && (f.x_axis.x - 1.0).abs() < 1e-9,
        "{f:?}"
    );
    // The pattern turned 1.5 mrad about the bore against a square slot.
    let turned = clocking_record(10.0, 8.0, 0.0015, [0.0; 2]);
    let expected = 2.0 * 2.0 * lever * 0.00075_f64.sin();
    let report = page.evaluate_inspection(&graph, &turned).unwrap();
    let m = evaluated(&report, "hole0");
    assert!((m.deviation_mm - expected).abs() < 1e-6, "{m:?}");
    // Without the face the slot still clocks the frame.
    let (_, unstopped) = axis_clocking_setup(&["B", "C"], Regardless, Regardless);
    let report = unstopped.evaluate_inspection(&graph, &turned).unwrap();
    assert!((evaluated(&report, "hole0").deviation_mm - expected).abs() < 1e-6);
    // A hole leaning 0.02 mm over its 10 mm is out by its worse end.
    let mut leaning = clocking_record(10.0, 8.0, 0.0, [0.0; 2]);
    leaning.controls[0] = measured(
        "hole0",
        cylinder(
            [PATTERN[0][0], PATTERN[0][1], 0.0],
            [0.02, 0.0, 10.0],
            |_| 3.0,
        ),
    );
    let report = page.evaluate_inspection(&graph, &leaning).unwrap();
    let m = evaluated(&report, "hole0");
    assert!((m.deviation_mm - 0.04).abs() < 1e-6, "{m:?}");
    // Without a clocking datum the frame turns freely about the bore, and
    // the pattern turns with it.
    let (_, unclocked) = axis_clocking_setup(&["B", "A"], Regardless, Regardless);
    let report = unclocked.evaluate_inspection(&graph, &turned).unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-6, "{m:?}");
        assert!((m.datum_shift_rotation_rad - 0.0015).abs() < 1e-6, "{m:?}");
    }
}

#[test]
fn a_clocking_slot_at_a_material_boundary_turns_the_frame_about_a_primary_axis() {
    use DatumMaterialBoundary::{Maximum, Regardless};
    let (definition, page) = axis_clocking_setup(&["B", "A", "C"], Regardless, Maximum);
    let graph = graph_of(&definition);
    // The 8.0 slot clears its 7.9 MMB: a 1.5 mrad turn of the pattern is
    // taken up, positive about the bore's +z axis.
    let report = page
        .evaluate_inspection(&graph, &clocking_record(10.0, 8.0, 0.0015, [0.0; 2]))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-4 && m.conforms, "{m:?}");
        assert!((m.datum_shift_rotation_rad - 0.0015).abs() < 1e-5, "{m:?}");
        assert!(m.datum_shift_mm < 1e-9, "{m:?}");
    }
    // Turned 3 mrad, the frame stops where the slot's far end meets the
    // boundary: 4·cos θ − 29·sin θ = 3.95.
    let limit = {
        let mut a = 0.0_f64;
        for _ in 0..50 {
            a -= (4.0 * a.cos() - 29.0 * a.sin() - 3.95) / (-4.0 * a.sin() - 29.0 * a.cos());
        }
        a
    };
    let report = page
        .evaluate_inspection(&graph, &clocking_record(10.0, 8.0, 0.003, [0.0; 2]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    assert!((m.datum_shift_rotation_rad - limit).abs() < 1e-6, "{m:?}");
    let lever = 15.0_f64.hypot(10.0);
    let expected = 2.0 * 2.0 * lever * (0.5 * (0.003 - limit)).sin();
    assert!((m.deviation_mm - expected).abs() < 1e-5, "{m:?} {expected}");
    // With the bore at MMB too, the frame shifts and turns together: the
    // slot block moves with the bore, so a 0.05 mm move and a 1.5 mrad turn
    // are both taken up, but not a move the other way.
    let (_, both) = axis_clocking_setup(&["B", "A", "C"], Maximum, Maximum);
    let report = both
        .evaluate_inspection(&graph, &clocking_record(10.06, 8.0, 0.0015, [0.05, 0.0]))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-4, "{m:?}");
        assert!((m.datum_shift_mm - 0.05).abs() < 1e-4, "{m:?}");
        assert!((m.datum_shift_rotation_rad - 0.0015).abs() < 1e-4, "{m:?}");
        assert!((m.datum_shift_limit_mm - 0.08).abs() < 1e-9, "{m:?}");
    }
    let report = both
        .evaluate_inspection(&graph, &clocking_record(10.06, 8.0, 0.0015, [-0.05, 0.0]))
        .unwrap();
    assert!(evaluated(&report, "hole0").deviation_mm > 1e-3);
}

/// `shift_setup` with C a Ø5.9-6.1 dowel hole 25 mm from the bore along +y,
/// each control referenced to the datums `order` with B at `b` and C at `c`.
fn pin_setup(
    order: &[&str],
    b: DatumMaterialBoundary,
    c: DatumMaterialBoundary,
) -> (FamilyDefinition, DrawingDefinition) {
    let (mut definition, mut page) = shift_setup(DatumMaterialBoundary::Regardless);
    definition.datums.push(DatumDefinition {
        id: "pin".into(),
        kind: DatumKind::Axis {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                HOLE[0],
                HOLE[1] + 25.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        },
    });
    let feature = page
        .datum_features
        .iter_mut()
        .find(|f| f.id == "C")
        .unwrap();
    feature.attachment = attachment("pin");
    feature.feature_of_size = true;
    feature.size_kind = Some(FeatureOfSizeKind::Internal);
    feature.size_limits = Some(DrawingSizeLimits {
        kind: FeatureOfSizeKind::Internal,
        lower: Quantity::length(5.9, LengthUnit::Millimeter),
        upper: Quantity::length(6.1, LengthUnit::Millimeter),
    });
    for frame in &mut page.feature_control_frames {
        let datums = std::mem::take(&mut frame.datums);
        frame.datums = order
            .iter()
            .map(|id| {
                let mut d = datums
                    .iter()
                    .find(|d| d.datum_feature == *id)
                    .unwrap()
                    .clone();
                d.boundary = match *id {
                    "B" => b,
                    "C" => c,
                    _ => DatumMaterialBoundary::Regardless,
                };
                d
            })
            .collect();
    }
    (definition, page)
}

/// `clocking_record` with C a Ø`pin` dowel hole turned `pin_turn` about the
/// bore from its nominal place.
fn pin_record(bore: f64, pin: f64, pin_turn: f64, turn: f64, t: [f64; 2]) -> InspectionRecord {
    let mut record = clocking_record(bore, 8.0, turn, t);
    let (s, c) = pin_turn.sin_cos();
    record.datum_features[2] = measured("C", hole([HOLE[0] - 25.0 * s, HOLE[1] + 25.0 * c], pin));
    record
}

#[test]
fn a_tertiary_hole_clocks_and_turns_a_plane_axis_frame() {
    use DatumMaterialBoundary::{Maximum, Regardless};
    let lever = 15.0_f64.hypot(10.0);
    let (definition, page) = pin_setup(&["A", "B", "C"], Regardless, Regardless);
    let graph = graph_of(&definition);
    // The dowel hole and the pattern turned together: the frame follows.
    let report = page
        .evaluate_inspection(&graph, &pin_record(10.0, 6.0, 0.002, 0.002, [0.0; 2]))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-6 && m.conforms, "{m:?}");
    }
    // The pattern turned 1.5 mrad against a dowel hole in place.
    let record = pin_record(10.0, 6.0, 0.0, 0.0015, [0.0; 2]);
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    let m = evaluated(&report, "hole0");
    assert!(
        (m.deviation_mm - 4.0 * lever * 0.00075_f64.sin()).abs() < 1e-6,
        "{m:?}"
    );
    assert_eq!(m.datum_shift_rotation_rad, 0.0);
    // At MMB a Ø6.0 hole clears its Ø5.9 boundary by 0.05 mm, which at
    // 25 mm lets the frame turn 2 mrad: the 1.5 mrad turn is taken up.
    let (_, mmb) = pin_setup(&["A", "B", "C"], Regardless, Maximum);
    let report = mmb.evaluate_inspection(&graph, &record).unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-4 && m.conforms, "{m:?}");
        assert!(
            (m.datum_shift_rotation_rad.abs() - 0.0015).abs() < 1e-5,
            "{m:?}"
        );
        assert!(m.datum_shift_mm < 1e-9, "{m:?}");
    }
    // Turned 3 mrad, the boundary meets the hole after 2·asin(0.05/50).
    let report = mmb
        .evaluate_inspection(&graph, &pin_record(10.0, 6.0, 0.0, 0.003, [0.0; 2]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    let limit = 2.0 * (0.05_f64 / 50.0).asin();
    assert!(
        (m.datum_shift_rotation_rad.abs() - limit).abs() < 1e-6,
        "{m:?}"
    );
    let expected = 4.0 * lever * (0.5 * (0.003 - limit)).sin();
    assert!((m.deviation_mm - expected).abs() < 1e-5, "{m:?} {expected}");
    // With the bore at MMB too, a 0.05 mm move and the turn are both taken
    // up (the boundary moves 0.05 − 0.0375 mm), but not a move the other way.
    let (_, both) = pin_setup(&["A", "B", "C"], Maximum, Maximum);
    let report = both
        .evaluate_inspection(&graph, &pin_record(10.06, 6.0, 0.0, 0.0015, [0.05, 0.0]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    assert!(m.deviation_mm < 1e-4, "{m:?}");
    assert!((m.datum_shift_mm - 0.05).abs() < 1e-4, "{m:?}");
    let report = both
        .evaluate_inspection(&graph, &pin_record(10.06, 6.0, 0.0, 0.0015, [-0.05, 0.0]))
        .unwrap();
    assert!(evaluated(&report, "hole0").deviation_mm > 1e-3);
    // A dowel hole smaller than its MMB violates the boundary.
    let report = mmb
        .evaluate_inspection(&graph, &pin_record(10.0, 5.85, 0.0, 0.0, [0.0; 2]))
        .unwrap();
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("violates its maximum material boundary")
        ),
        "{:?}",
        report.controls[0]
    );
}

#[test]
fn a_hole_clocks_a_primary_datum_axis() {
    use DatumMaterialBoundary::{Maximum, Regardless};
    let lever = 15.0_f64.hypot(10.0);
    let (definition, page) = pin_setup(&["B", "A", "C"], Regardless, Regardless);
    let graph = graph_of(&definition);
    let report = page
        .evaluate_inspection(&graph, &pin_record(10.0, 6.0, 0.002, 0.002, [0.0; 2]))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-6 && m.conforms, "{m:?}");
    }
    let report = page
        .evaluate_inspection(&graph, &pin_record(10.0, 6.0, 0.0, 0.0015, [0.0; 2]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    assert!(
        (m.deviation_mm - 4.0 * lever * 0.00075_f64.sin()).abs() < 1e-6,
        "{m:?}"
    );
    // Bore and dowel hole both at MMB: the axis shifts and the frame turns,
    // positive about the bore's +z axis.
    let (_, both) = pin_setup(&["B", "A", "C"], Maximum, Maximum);
    let report = both
        .evaluate_inspection(&graph, &pin_record(10.06, 6.0, 0.0, 0.0015, [0.05, 0.0]))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-4, "{m:?}");
        assert!((m.datum_shift_mm - 0.05).abs() < 1e-4, "{m:?}");
        assert!((m.datum_shift_rotation_rad - 0.0015).abs() < 1e-4, "{m:?}");
    }
    let report = both
        .evaluate_inspection(&graph, &pin_record(10.06, 6.0, 0.0, 0.0015, [-0.05, 0.0]))
        .unwrap();
    assert!(evaluated(&report, "hole0").deviation_mm > 1e-3);
}

/// Plate with datum bore D (Ø9.9-10.1 on the hole axis, at `boundary`)
/// and Ø0.05 RFS position controls on Ø2.9-3.1 cross holes at z = 5 square
/// to it, along +x (`cross0`) and +y (`cross1`), referenced to `order`.
fn cross_setup(
    order: &[&str],
    boundary: DatumMaterialBoundary,
) -> (FamilyDefinition, DrawingDefinition) {
    let mut definition = plate();
    for (i, direction) in [[1.0, 0.0], [0.0, 1.0]].into_iter().enumerate() {
        definition.datums.push(DatumDefinition {
            id: format!("cross{i}"),
            kind: DatumKind::Axis {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    HOLE[0],
                    HOLE[1],
                    5.0,
                    LengthUnit::Millimeter,
                )),
                direction: VectorExpr::Literal(VectorQuantity::scalars(
                    direction[0],
                    direction[1],
                    0.0,
                )),
            },
        });
    }
    let mut page = coaxial_page(boundary, &[]);
    page.feature_control_frames = (0..2)
        .map(|i| {
            let id = format!("cross{i}");
            let mut f = control(&id, &id, GeometricCharacteristic::Position, 0.05, order);
            f.datums[0].boundary = boundary;
            f.zone = GeometricToleranceZone::Diameter;
            f.feature_of_size = true;
            f.size_limits = Some(DrawingSizeLimits {
                kind: FeatureOfSizeKind::Internal,
                lower: Quantity::length(2.9, LengthUnit::Millimeter),
                upper: Quantity::length(3.1, LengthUnit::Millimeter),
            });
            f
        })
        .collect();
    (definition, page)
}

/// A measured part: Ø`bore` bore over z 0-10 on the nominal axis, the
/// bottom face A and front face C, and Ø3 cross holes 5-15 mm from the axis
/// turned by `turn` about it, raised by `lift[i]` and moved square to their
/// own axis (in the plate's plane) by `side[i]`.
fn cross_record(bore: f64, turn: f64, lift: [f64; 2], side: [f64; 2]) -> InspectionRecord {
    InspectionRecord {
        uncertainty: None,
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: vec![
            measured(
                "D",
                cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| bore / 2.0),
            ),
            measured("A", grid(|u, v| [60.0 * u, 40.0 * v, 0.0])),
            measured("C", grid(|u, v| [60.0 * u, 0.0, 10.0 * v])),
        ],
        controls: (0..2)
            .map(|i| {
                let a = turn + i as f64 * std::f64::consts::FRAC_PI_2;
                let (s, c) = a.sin_cos();
                let base = [
                    HOLE[0] + 5.0 * c - side[i] * s,
                    HOLE[1] + 5.0 * s + side[i] * c,
                    5.0 + lift[i],
                ];
                measured(&format!("cross{i}"), cylinder(base, [c, s, 0.0], |_| 1.5))
            })
            .collect(),
    }
}

#[test]
fn position_is_measured_at_an_angle_to_a_primary_datum_axis() {
    use DatumMaterialBoundary::{Maximum, Regardless};
    let (definition, page) = cross_setup(&["D", "A"], Regardless);
    let graph = graph_of(&definition);
    let report = page
        .evaluate_inspection(&graph, &cross_record(10.0, 0.0, [0.0; 2], [0.0; 2]))
        .unwrap();
    for id in ["cross0", "cross1"] {
        let m = evaluated(&report, id);
        assert!(m.deviation_mm < 1e-6 && m.conforms, "{m:?}");
        assert!((m.actual_size_mm.unwrap() - 3.0).abs() < 1e-6, "{m:?}");
    }
    // Raised 0.02 mm along the datum axis: the face stops the frame there.
    let report = page
        .evaluate_inspection(&graph, &cross_record(10.0, 0.0, [0.02, 0.0], [0.0; 2]))
        .unwrap();
    let m = evaluated(&report, "cross0");
    assert!((m.deviation_mm - 0.04).abs() < 1e-6 && m.conforms, "{m:?}");
    // Without a clocking datum the frame turns freely about the bore: both
    // holes turned 10 mrad are at true position, sharing one turn.
    let report = page
        .evaluate_inspection(&graph, &cross_record(10.0, 0.01, [0.0; 2], [0.0; 2]))
        .unwrap();
    for id in ["cross0", "cross1"] {
        let m = evaluated(&report, id);
        assert!(m.deviation_mm < 1e-6, "{m:?}");
        assert!((m.datum_shift_rotation_rad - 0.01).abs() < 1e-6, "{m:?}");
        assert_eq!((m.datum_shift_mm, m.datum_shift_limit_mm), (0.0, 0.0));
    }
    // A hole 0.03 mm to the side of the axis, 5-15 mm out, turned by θ
    // with the other hole: its far end is 0.03 - 15θ off and the other
    // hole's 15θ, balanced at θ = 1.5 mrad.
    let report = page
        .evaluate_inspection(&graph, &cross_record(10.0, 0.0, [0.0; 2], [0.03, 0.0]))
        .unwrap();
    for id in ["cross0", "cross1"] {
        let m = evaluated(&report, id);
        assert!((m.deviation_mm - 0.045).abs() < 1e-5 && m.conforms, "{m:?}");
        assert!((m.datum_shift_rotation_rad - 0.0015).abs() < 1e-5, "{m:?}");
    }
    // The front face clocks the frame: nothing turns.
    let (_, clocked) = cross_setup(&["D", "A", "C"], Regardless);
    let report = clocked
        .evaluate_inspection(&graph, &cross_record(10.0, 0.0, [0.0; 2], [0.03, 0.0]))
        .unwrap();
    let m = evaluated(&report, "cross0");
    assert!((m.deviation_mm - 0.06).abs() < 1e-6, "{m:?}");
    assert_eq!(m.datum_shift_rotation_rad, 0.0);
    let report = clocked
        .evaluate_inspection(&graph, &cross_record(10.0, 0.001, [0.0; 2], [0.0; 2]))
        .unwrap();
    let m = evaluated(&report, "cross0");
    assert!((m.deviation_mm - 0.03).abs() < 1e-6, "{m:?}");
    // The bore at MMB clears its Ø9.9 boundary by 0.08 mm a side: tilting
    // the axis by g lowers the raised hole's ends by 5g and 15g, balanced
    // at g = 2 mrad, inside the 16 mrad the bore allows.
    let (_, shifted) = cross_setup(&["D", "A"], Maximum);
    let report = shifted
        .evaluate_inspection(&graph, &cross_record(10.06, 0.0, [0.02, 0.0], [0.0; 2]))
        .unwrap();
    let m = evaluated(&report, "cross0");
    assert!((m.deviation_mm - 0.02).abs() < 1e-5, "{m:?}");
    assert!((m.datum_shift_limit_mm - 0.08).abs() < 1e-9, "{m:?}");
    // A hole at 45° to the axis raised 0.02 mm is 0.02·sin 45° off its zone.
    let mut definition = definition.clone();
    let s = std::f64::consts::FRAC_1_SQRT_2;
    definition.datums.push(DatumDefinition {
        id: "slant".into(),
        kind: DatumKind::Axis {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                HOLE[0],
                HOLE[1],
                5.0,
                LengthUnit::Millimeter,
            )),
            direction: VectorExpr::Literal(VectorQuantity::scalars(s, 0.0, s)),
        },
    });
    let graph = graph_of(&definition);
    let mut slanted = page.clone();
    slanted.feature_control_frames.truncate(1);
    slanted.feature_control_frames[0].id = "slant".into();
    slanted.feature_control_frames[0].attachment = attachment("slant");
    let mut record = cross_record(10.0, 0.0, [0.0; 2], [0.0; 2]);
    record.controls = vec![measured(
        "slant",
        cylinder([HOLE[0] + 2.0, HOLE[1], 7.02], [1.0, 0.0, 1.0], |_| 1.5),
    )];
    let report = slanted.evaluate_inspection(&graph, &record).unwrap();
    let m = evaluated(&report, "slant");
    assert!((m.deviation_mm - 0.04 * s).abs() < 1e-6, "{m:?}");
    // Without the face nothing places the holes along the axis.
    let (_, unstopped) = cross_setup(&["D"], Regardless);
    let report = unstopped
        .evaluate_inspection(&graph, &cross_record(10.0, 0.0, [0.0; 2], [0.0; 2]))
        .unwrap();
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("perpendicular datum")
        ),
        "{:?}",
        report.controls[0]
    );
}

/// `plate` with a 45° bevel along its bottom-left edge (x + z = 2), and a
/// drawing whose position control on the hole and perpendicularity of the
/// right face are referenced to `frame`, with datum features D (the
/// chamfer, square to A at 45° to B) and E (the bevel, at 45° to A).
fn oblique_setup(frame: [&str; 3]) -> (FamilyDefinition, DrawingDefinition) {
    let mut definition = plate();
    let s = std::f64::consts::FRAC_1_SQRT_2;
    definition
        .datums
        .push(plane("bevel", [2.0, 0.0, 0.0], [-s, 0.0, -s]));
    let mut page = page();
    for (id, anchor) in [("D", "chamfer"), ("E", "bevel")] {
        let mut feature = page.datum_features[0].clone();
        feature.id = id.into();
        feature.label = id.into();
        feature.attachment = attachment(anchor);
        page.datum_features.push(feature);
    }
    page.datum_reference_frames[0].datums = frame
        .iter()
        .map(|id| DrawingDatumReference {
            translation: false,
            datum_feature: (*id).into(),
            boundary: DatumMaterialBoundary::Regardless,
            common_with: Vec::new(),
        })
        .collect();
    page.feature_control_frames
        .retain(|f| f.id == "position" || f.id == "perpendicular");
    page.feature_control_frames[0].datums = page.datum_reference_frames[0].datums[..2].to_vec();
    (definition, page)
}

/// The nominal plate measured with the chamfer and bevel moved out along
/// their normals by `chamfer` and `bevel`, then the whole part turned by
/// `turn` (radians about a fixed skew axis) and moved by `moved`.
fn oblique_record(chamfer: f64, bevel: f64, turn: f64, moved: [f64; 3]) -> InspectionRecord {
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let mut record = nominal_record();
    record
        .controls
        .retain(|c| c.id == "position" || c.id == "perpendicular");
    record.datum_features.push(measured(
        "D",
        grid(|u, v| {
            [
                55.0 + 5.0 * u + chamfer * s,
                40.0 - 5.0 * u + chamfer * s,
                10.0 * v,
            ]
        }),
    ));
    record.datum_features.push(measured(
        "E",
        grid(|u, v| [2.0 - 2.0 * v - bevel * s, 40.0 * u, 2.0 * v - bevel * s]),
    ));
    let k = {
        let k = [1.0, 2.0, 3.0_f64];
        let n = (k[0] * k[0] + k[1] * k[1] + k[2] * k[2]).sqrt();
        k.map(|c| c / n)
    };
    let (sin, cos) = turn.sin_cos();
    // Rodrigues' rotation about k, then the move.
    let place = |p: [f64; 3]| {
        let kxp = [
            k[1] * p[2] - k[2] * p[1],
            k[2] * p[0] - k[0] * p[2],
            k[0] * p[1] - k[1] * p[0],
        ];
        let kdp = k[0] * p[0] + k[1] * p[1] + k[2] * p[2];
        [0, 1, 2].map(|i| p[i] * cos + kxp[i] * sin + k[i] * kdp * (1.0 - cos) + moved[i])
    };
    for feature in record.datum_features.iter_mut().chain(&mut record.controls) {
        for p in &mut feature.points_mm {
            *p = place(*p);
        }
    }
    record
}

#[test]
fn datum_simulators_keep_their_basic_angles() {
    // A | B | D: the chamfer at 45° to B, square to A, places the origin
    // along B. Moved out 0.1 mm along its normal, it moves the origin
    // 0.1·√2 along y, which the hole's position reads at twice that.
    let (definition, page) = oblique_setup(["A", "B", "D"]);
    let graph = graph_of(&definition);
    let report = page
        .evaluate_inspection(&graph, &oblique_record(0.0, 0.0, 0.0, [0.0; 3]))
        .unwrap();
    let m = evaluated(&report, "position");
    assert!(m.deviation_mm < 1e-6, "{m:?}");
    let report = page
        .evaluate_inspection(&graph, &oblique_record(0.1, 0.0, 0.0, [0.0; 3]))
        .unwrap();
    let m = evaluated(&report, "position");
    assert!(
        (m.deviation_mm - 0.2 * 2.0_f64.sqrt()).abs() < 1e-6,
        "{m:?}"
    );
    // A | E | C: the bevel at 45° to A is the secondary. Its simulator turns
    // only about A's normal, keeping 45° to A, and places the origin along
    // x.
    let (definition, page) = oblique_setup(["A", "E", "C"]);
    let graph = graph_of(&definition);
    let report = page
        .evaluate_inspection(&graph, &oblique_record(0.0, 0.1, 0.0, [0.0; 3]))
        .unwrap();
    let m = evaluated(&report, "position");
    assert!(
        (m.deviation_mm - 0.2 * 2.0_f64.sqrt()).abs() < 1e-6,
        "{m:?}"
    );
    let f = &report.datum_frames[0].frame;
    assert!(
        (f.origin_mm.x - (2.0 - 0.1 * 2.0_f64.sqrt())).abs() < 1e-6
            && (f.x_axis.x + 1.0).abs() < 1e-9,
        "{f:?}"
    );
    // The whole part turned 3 mrad about a skew axis and moved: every
    // simulator follows at its basic angle, so the part still measures at
    // true position, and the right face stays square to A | E.
    for frame in [["A", "B", "D"], ["A", "E", "C"]] {
        let (definition, page) = oblique_setup(frame);
        let graph = graph_of(&definition);
        let report = page
            .evaluate_inspection(&graph, &oblique_record(0.0, 0.0, 0.003, [0.5, -0.3, 0.2]))
            .unwrap();
        for id in ["position", "perpendicular"] {
            let m = evaluated(&report, id);
            assert!(m.deviation_mm < 1e-6, "{frame:?} {id} {m:?}");
        }
    }
    // The bevel's normal lies in the plane of A's and B's, so as a tertiary
    // it cannot place the origin along the line where A and B meet.
    let (definition, page) = oblique_setup(["A", "B", "E"]);
    let graph = graph_of(&definition);
    let report = page
        .evaluate_inspection(&graph, &oblique_record(0.0, 0.0, 0.0, [0.0; 3]))
        .unwrap();
    let result = &report
        .controls
        .iter()
        .find(|c| c.control == "position")
        .unwrap()
        .result;
    assert!(
        matches!(result, ControlResult::NotEvaluated(r) if r.contains("meet")),
        "{result:?}"
    );
    // Datum shift is not evaluated for a datum tilted out of square with
    // the primary: here the bevel as one face of a 4 mm tab.
    let (definition, mut page) = oblique_setup(["A", "E", "C"]);
    page.datum_reference_frames[0].datums[1].boundary = DatumMaterialBoundary::Maximum;
    let tab = page
        .datum_features
        .iter_mut()
        .find(|f| f.id == "E")
        .unwrap();
    tab.feature_of_size = true;
    tab.size_kind = Some(FeatureOfSizeKind::External);
    tab.size_limits = Some(DrawingSizeLimits {
        kind: FeatureOfSizeKind::External,
        lower: Quantity::length(3.9, LengthUnit::Millimeter),
        upper: Quantity::length(4.1, LengthUnit::Millimeter),
    });
    let mut record = oblique_record(0.0, 0.0, 0.0, [0.0; 3]);
    let inward = 4.0 * std::f64::consts::FRAC_1_SQRT_2;
    record
        .datum_features
        .last_mut()
        .unwrap()
        .points_mm
        .extend(grid(|u, v| {
            [2.0 - 2.0 * v + inward, 40.0 * u, 2.0 * v + inward]
        }));
    let graph = graph_of(&definition);
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    let result = &report
        .controls
        .iter()
        .find(|c| c.control == "position")
        .unwrap()
        .result;
    assert!(
        matches!(result, ControlResult::NotEvaluated(r) if r.contains("basic angle")),
        "{result:?}"
    );
}

/// `coaxial_page` located to the common axis D-E of two Ø9.9-10.1 bores on
/// the hole axis, at `boundary`.
fn common_shift_page(boundary: DatumMaterialBoundary) -> DrawingDefinition {
    let mut page = coaxial_page(boundary, &["cbore0"]);
    let mut e = page.datum_features.last().unwrap().clone();
    e.id = "E".into();
    e.label = "E".into();
    page.datum_features.push(e);
    page.feature_control_frames[0].datums[0].common_with = vec!["E".into()];
    page
}

/// Bores D (z 0-10) and E (z 30-40) of diameters `d` and `e` on the nominal
/// axis, and a Ø16 counterbore over z 12-22 offset `offset` in x.
fn common_shift_record(d: f64, e: f64, offset: f64) -> InspectionRecord {
    InspectionRecord {
        uncertainty: None,
        drawing: "plate".into(),
        surface_textures: Vec::new(),
        datum_features: vec![
            measured(
                "D",
                cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| d / 2.0),
            ),
            measured(
                "E",
                cylinder([HOLE[0], HOLE[1], 30.0], [0.0, 0.0, 1.0], |_| e / 2.0),
            ),
        ],
        controls: vec![measured(
            "cbore0",
            cylinder([HOLE[0] + offset, HOLE[1], 12.0], [0.0, 0.0, 1.0], |_| 8.0),
        )],
    }
}

#[test]
fn a_common_datum_axis_at_a_material_boundary_shifts_within_each_member() {
    use DatumMaterialBoundary::{Least, Maximum, Regardless};
    let definition = plate();
    let graph = graph_of(&definition);
    let page = common_shift_page(Maximum);
    // Both bores Ø10.06 clear their Ø9.9 MMB by 0.08 mm: a 0.05 mm offset
    // shifts away; a 0.1 mm offset leaves 0.02 mm a side.
    let report = page
        .evaluate_inspection(&graph, &common_shift_record(10.06, 10.06, 0.05))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    assert!(m.deviation_mm < 1e-6 && m.conforms, "{m:?}");
    assert!((m.datum_shift_mm - 0.05).abs() < 1e-6, "{m:?}");
    assert!((m.datum_shift_limit_mm - 0.08).abs() < 1e-9, "{m:?}");
    let report = page
        .evaluate_inspection(&graph, &common_shift_record(10.06, 10.06, 0.1))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    assert!((m.deviation_mm - 0.04).abs() < 1e-4, "{m:?}");
    // E at Ø9.94 clears only 0.02 mm: the axis may sit 0.08 mm off at z = 0
    // but 0.02 mm at z = 30, so at z = 22 it reaches (8·0.08 + 22·0.02)/30
    // and the counterbore's top is 0.05 minus that off.
    let report = page
        .evaluate_inspection(&graph, &common_shift_record(10.06, 9.94, 0.05))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    let reach = (8.0 * 0.08 + 22.0 * 0.02) / 30.0;
    assert!(
        (m.deviation_mm - 2.0 * (0.05 - reach)).abs() < 1e-4,
        "{m:?}"
    );
    assert!(m.datum_shift_rotation_rad > 1e-3, "{m:?}");
    // Regardless of boundary the common axis does not move.
    let report = common_shift_page(Regardless)
        .evaluate_inspection(&graph, &common_shift_record(10.06, 10.06, 0.05))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    assert!((m.deviation_mm - 0.1).abs() < 1e-6, "{m:?}");
    // LMB: Ø10.0 bores are 0.1 mm inside their Ø10.1 boundary.
    let report = common_shift_page(Least)
        .evaluate_inspection(&graph, &common_shift_record(10.0, 10.0, 0.03))
        .unwrap();
    let m = evaluated(&report, "cbore0");
    assert!(m.deviation_mm < 1e-6, "{m:?}");
    assert!((m.datum_shift_limit_mm - 0.05).abs() < 1e-9, "{m:?}");
    // A member inside its MMB violates the boundary.
    let report = page
        .evaluate_inspection(&graph, &common_shift_record(10.06, 9.85, 0.0))
        .unwrap();
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("violates its maximum material boundary")
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
                refinement: None,
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
fn each_lower_segment_of_a_composite_relates_the_pattern_on_its_own() {
    // Ø0.2 to A | B | C, then Ø0.1 to A | B (the pattern may translate but
    // stays square to B), then Ø0.04 to A (it may also turn).
    let (definition, mut page) = composite_page(0.2, 0.1, &["A", "B"]);
    for f in &mut page.feature_control_frames {
        f.refinement.as_mut().unwrap().refinement = Some(Box::new(DrawingCompositeRefinement {
            tolerance: Quantity::length(0.04, LengthUnit::Millimeter),
            datums: f.datums[..1].to_vec(),
            refinement: None,
        }));
    }
    let graph = graph_of(&definition);
    let radius = 15.0_f64.hypot(10.0);
    // Turned 1 mrad: 2 · 18.03 · 0.001 mm in the middle segment, nothing in
    // the lowest.
    let report = page
        .evaluate_inspection(&graph, &composite_record([0.0; 2], 0.001, [[0.0; 2]; 4]))
        .unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        let middle = m.refinement.clone().unwrap();
        let lowest = middle.refinement.clone().unwrap();
        assert!(
            (middle.deviation_mm - 2.0 * radius * 0.001).abs() < 1e-3,
            "{middle:?}"
        );
        assert_eq!(middle.pattern_rotation_rad, 0.0);
        assert!(lowest.deviation_mm < 1e-4, "{lowest:?}");
        assert!(
            (lowest.pattern_rotation_rad + 0.001).abs() < 1e-5,
            "{lowest:?}"
        );
        assert!(lowest.refinement.is_none());
        assert!(m.conforms && middle.conforms && lowest.conforms);
    }
    // Turned 3 mrad: 0.108 mm breaks the middle segment alone.
    let report = page
        .evaluate_inspection(&graph, &composite_record([0.0; 2], 0.003, [[0.0; 2]; 4]))
        .unwrap();
    let m = evaluated(&report, "hole0");
    let middle = m.refinement.clone().unwrap();
    let lowest = middle.refinement.clone().unwrap();
    assert!(m.deviation_mm < 0.2, "{m:?}");
    assert!(!middle.conforms && lowest.conforms, "{middle:?}");
    assert!(!m.conforms);
    // A three-segment surface profile on a face leaning 0.02 mm over its
    // height: 0.04 to A | B, 0.02 to A (free to slide), 0 with no datums.
    let lean = |amount: f64| {
        measured(
            "right_profile",
            grid(|u, v| [60.0 + amount * v, 40.0 * u, 10.0 * v]),
        )
    };
    let mut profile = right_profile(GeometricCharacteristic::ProfileSurface, &["A", "B"]);
    profile.refinement = Some(DrawingCompositeRefinement {
        tolerance: Quantity::length(0.03, LengthUnit::Millimeter),
        datums: profile.datums[..1].to_vec(),
        refinement: Some(Box::new(DrawingCompositeRefinement {
            tolerance: Quantity::length(0.01, LengthUnit::Millimeter),
            datums: Vec::new(),
            refinement: None,
        })),
    });
    let report = profile_report(vec![profile.clone()], vec![lean(0.02)]);
    let m = evaluated(&report, "right_profile");
    let middle = m.refinement.clone().unwrap();
    let lowest = middle.refinement.clone().unwrap();
    assert!((m.deviation_mm - 0.04).abs() < 1e-9, "{m:?}");
    assert!((middle.deviation_mm - 0.02).abs() < 1e-9, "{middle:?}");
    assert!(lowest.deviation_mm < 1e-9, "{lowest:?}");
    assert!(m.conforms);
    // Leaning 0.04 mm fails the upper and middle segments but not the lowest.
    let report = profile_report(vec![profile], vec![lean(0.04)]);
    let m = evaluated(&report, "right_profile");
    let middle = m.refinement.clone().unwrap();
    assert!(!m.conforms && !middle.conforms && middle.refinement.unwrap().conforms);
}

#[test]
fn a_projected_zone_extends_the_hole_axis_beyond_the_part() {
    let definition = plate();
    let graph = graph_of(&definition);
    // Ø0.1 RFS to A | B | C on the Ø10.04 hole, its axis leaning 2 mrad in
    // x: 0.02 mm across the plate's 10 mm.
    let mut page = page();
    page.feature_control_frames = page
        .feature_control_frames
        .into_iter()
        .filter(|f| f.id == "position")
        .map(|mut f| {
            f.material = ToleranceMaterialCondition::Regardless;
            f.tolerance = Quantity::length(0.1, LengthUnit::Millimeter);
            f
        })
        .collect();
    let mut record = nominal_record();
    record.controls = vec![measured(
        "position",
        cylinder([HOLE[0], HOLE[1], 0.0], [0.002, 0.0, 1.0], |_| 5.02),
    )];
    let projected = |from: Option<&str>| {
        let mut page = page.clone();
        page.feature_control_frames[0].projected = Some(DrawingProjectedZone {
            height: Quantity::length(20.0, LengthUnit::Millimeter),
            from: from.map(Into::into),
        });
        page.evaluate_inspection(&graph, &record).unwrap()
    };
    // Square to A, the related envelope reads the lean only as a smaller
    // size; the plate itself holds the hole well inside its zone.
    let m = evaluated(
        &page.evaluate_inspection(&graph, &record).unwrap(),
        "position",
    );
    assert!(m.deviation_mm < 0.05 && m.conforms, "{m:?}");
    // Projected 20 mm above the top face (z 10-30), the axis leaves the zone
    // 0.06 mm off true position: 0.12 mm.
    let m = evaluated(&projected(Some("top")), "position");
    assert!((m.deviation_mm - 0.12).abs() < 1e-4 && !m.conforms, "{m:?}");
    assert!((m.actual_size_mm.unwrap() - 10.04).abs() < 1e-4, "{m:?}");
    // Without a starting plane the zone runs 20 mm below A (z 0 to -20).
    let m = evaluated(&projected(None), "position");
    assert!((m.deviation_mm - 0.08).abs() < 1e-4 && m.conforms, "{m:?}");
    // The zone starts from a plane square to the hole.
    let report = projected(Some("right"));
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("square to the feature axis")
        ),
        "{:?}",
        report.controls[0]
    );
}

#[test]
fn a_tangent_plane_orients_only_the_high_points() {
    let definition = plate();
    let graph = graph_of(&definition);
    let mut page = page();
    page.feature_control_frames.retain(|f| f.id == "parallel");
    page.feature_control_frames[0].tolerance = Quantity::length(0.02, LengthUnit::Millimeter);
    let mut tangent = page.clone();
    tangent.feature_control_frames[0].tangent_plane = true;
    let top = |lift: &dyn Fn(f64, f64) -> f64| {
        let mut record = nominal_record();
        record.controls = vec![measured(
            "parallel",
            grid(|u, v| [60.0 * u, 40.0 * v, 10.0 + lift(u, v)]),
        )];
        record
    };
    // A level top with a 0.03 mm ridge across its middle: every point needs
    // a 0.03 mm zone, but the plane on the ridge is level.
    let ridge = top(&|u, _| if (u - 0.5).abs() < 0.1 { 0.03 } else { 0.0 });
    let m = evaluated(
        &page.evaluate_inspection(&graph, &ridge).unwrap(),
        "parallel",
    );
    assert!((m.deviation_mm - 0.03).abs() < 1e-9 && !m.conforms, "{m:?}");
    let m = evaluated(
        &tangent.evaluate_inspection(&graph, &ridge).unwrap(),
        "parallel",
    );
    assert!(m.deviation_mm < 1e-9 && m.conforms, "{m:?}");
    // Tilted 0.025 mm over its length, the tangent plane tilts with it.
    let tilted = top(&|u, _| 0.025 * u);
    let m = evaluated(
        &tangent.evaluate_inspection(&graph, &tilted).unwrap(),
        "parallel",
    );
    assert!(
        (m.deviation_mm - 0.025).abs() < 1e-9 && !m.conforms,
        "{m:?}"
    );
}

#[test]
fn a_translation_modifier_frees_a_boundary_from_its_basic_location() {
    use DatumMaterialBoundary::{Maximum, Regardless};
    // A | B(slot, RFS) | C(slot, MMB): C clears its 7.9 boundary by 0.05 mm,
    // so a pattern 0.2 mm off along C's normal reads 2 · 0.15 mm. With ▷ C
    // only orients, which B already does, and the pattern is in place.
    let (definition, page) = width_frame_setup(Regardless, Maximum);
    let graph = graph_of(&definition);
    let record = width_frame_record(0.0, [0.0, 0.2]);
    let m = evaluated(&page.evaluate_inspection(&graph, &record).unwrap(), "hole0");
    assert!((m.deviation_mm - 0.3).abs() < 1e-4, "{m:?}");
    let mut translated = page.clone();
    for frame in &mut translated.feature_control_frames {
        frame.datums[2].translation = true;
    }
    let report = translated.evaluate_inspection(&graph, &record).unwrap();
    for i in 0..4 {
        let m = evaluated(&report, &format!("hole{i}"));
        assert!(m.deviation_mm < 1e-6, "{m:?}");
        assert!((m.datum_shift_mm - 0.2).abs() < 1e-6, "{m:?}");
    }
    // B still locates along x.
    let report = translated
        .evaluate_inspection(&graph, &width_frame_record(0.0, [0.03, 0.2]))
        .unwrap();
    assert!((evaluated(&report, "hole0").deviation_mm - 0.06).abs() < 1e-4);
    // A | B(bore, MMB ▷) | C: the bore no longer locates, C still fixes y.
    let (definition, mut page) = shift_setup(Maximum);
    let graph = graph_of(&definition);
    let record = shift_record(10.0, [[0.2, 0.0]; 4]);
    let m = evaluated(&page.evaluate_inspection(&graph, &record).unwrap(), "hole0");
    assert!((m.deviation_mm - 0.3).abs() < 1e-4, "{m:?}");
    for frame in &mut page.feature_control_frames {
        frame.datums[1].translation = true;
    }
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    assert!(evaluated(&report, "hole0").deviation_mm < 1e-6);
    let report = page
        .evaluate_inspection(&graph, &shift_record(10.0, [[0.0, 0.2]; 4]))
        .unwrap();
    assert!((evaluated(&report, "hole0").deviation_mm - 0.4).abs() < 1e-4);
    // C measured 0.1 mm proud of its basic place carries the frame with it.
    let mut record = shift_record(10.0, [[0.0, -0.1]; 4]);
    for p in &mut record.datum_features[2].points_mm {
        p[1] -= 0.1;
    }
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    assert!(evaluated(&report, "hole0").deviation_mm < 1e-6);
    // After a secondary width at a boundary the tertiary turns with it;
    // translating it as well is reported.
    let (definition, mut page) = width_frame_setup(Maximum, Maximum);
    let graph = graph_of(&definition);
    for frame in &mut page.feature_control_frames {
        frame.datums[2].translation = true;
    }
    let report = page
        .evaluate_inspection(&graph, &width_frame_record(0.0, [0.0; 2]))
        .unwrap();
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("translation modifier")
        ),
        "{:?}",
        report.controls[0]
    );
}

#[test]
fn a_dynamic_profile_zone_grows_or_shrinks_with_the_surface() {
    let bow = |z: f64| 0.02 * (std::f64::consts::PI * z / 10.0).sin();
    // The right face 0.03 mm proud and bowed 0.02 mm, located to A | B: the
    // fixed zone reads 2 · 0.05 mm, the dynamic zone only the bow.
    let face = |out: f64| right_face(move |z| out + bow(z));
    let fixed = right_profile(GeometricCharacteristic::ProfileSurface, &["A", "B"]);
    let mut dynamic = fixed.clone();
    dynamic.modifiers = vec![ToleranceModifier::DynamicProfile];
    let m = evaluated(
        &profile_report(vec![fixed], vec![face(0.03)]),
        "right_profile",
    );
    assert!((m.deviation_mm - 0.1).abs() < 1e-9 && !m.conforms, "{m:?}");
    let m = evaluated(
        &profile_report(vec![dynamic.clone()], vec![face(0.03)]),
        "right_profile",
    );
    assert!((m.deviation_mm - 0.02).abs() < 1e-9 && m.conforms, "{m:?}");
    let profile = m.profile.unwrap();
    assert!((profile.offset_mm - 0.04).abs() < 1e-9, "{profile:?}");
    // A statistical tolerance is not judged on one part.
    let mut statistical = dynamic;
    statistical.modifiers = vec![ToleranceModifier::Statistical];
    let report = profile_report(vec![statistical], vec![face(0.0)]);
    assert!(
        matches!(
            &report.controls[0].result,
            ControlResult::NotEvaluated(r) if r.contains("statistical")
        ),
        "{:?}",
        report.controls[0]
    );
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
                    refinement: None,
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
    assert!(error.contains("profile and runout controls"), "{error}");
    let json = serde_json::to_string(&record).unwrap();
    assert_eq!(
        serde_json::from_str::<InspectionRecord>(&json).unwrap(),
        record
    );
    // A line element of a single point is reported, not evaluated.
    let mut line = top_profile(0.1, &["A"]);
    line.characteristic = GeometricCharacteristic::ProfileLine;
    let mut stray = top(|_| 10.0);
    stray.points_mm.push([5.0, 5.0, 9.0]);
    let report = profile_report(vec![line], vec![stray]);
    assert!(matches!(
        &report.controls[0].result,
        ControlResult::NotEvaluated(reason) if reason.contains("at least two points")
    ));
}

/// Profile of the right face (x = 60) seen in the top view: its line
/// elements are the horizontal rows at each height.
fn right_profile(
    characteristic: GeometricCharacteristic,
    datums: &[&str],
) -> DrawingFeatureControlFrame {
    control("right_profile", "right", characteristic, 0.05, datums)
}

/// The right face bowed out by `bow(z)` at each height, every row straight.
fn right_face(bow: impl Fn(f64) -> f64) -> MeasuredFeature {
    measured(
        "right_profile",
        grid(|u, v| [60.0 + bow(10.0 * v), 40.0 * u, 10.0 * v]),
    )
}

#[test]
fn line_profile_fits_each_element_in_its_section() {
    let bow = |z: f64| 0.02 * (std::f64::consts::PI * z / 10.0).sin();
    // Without datums a surface profile keeps the bow; each straight row of
    // a line profile slides onto its line.
    let report = profile_report(
        vec![right_profile(GeometricCharacteristic::ProfileSurface, &[])],
        vec![right_face(bow)],
    );
    let surface = evaluated(&report, "right_profile");
    assert!(surface.deviation_mm > 0.015, "{surface:?}");
    assert_eq!(surface.profile.unwrap().elements, 1);
    let report = profile_report(
        vec![right_profile(GeometricCharacteristic::ProfileLine, &[])],
        vec![right_face(bow)],
    );
    let line = evaluated(&report, "right_profile");
    let profile = line.profile.clone().unwrap();
    assert!(line.deviation_mm < 1e-9, "{line:?}");
    assert_eq!(profile.elements, 5);
    assert!((profile.translation_mm - 0.02).abs() < 1e-9);
    // Located by A and B, every row keeps its bulge out of B's plane...
    let report = profile_report(
        vec![right_profile(
            GeometricCharacteristic::ProfileLine,
            &["A", "B"],
        )],
        vec![right_face(bow)],
    );
    let located = evaluated(&report, "right_profile");
    assert!((located.deviation_mm - 0.04).abs() < 1e-9, "{located:?}");
    assert!((located.profile.unwrap().max_deviation_mm - 0.02).abs() < 1e-9);
    // ...but a row turned in its plane still turns back: A leaves rotation
    // about its normal, the view direction, free.
    let turned = |row: f64| {
        measured(
            "right_profile",
            grid(|u, v| {
                let y = 40.0 * u;
                [60.0 + row * (y - 20.0) * 0.001 * v, y, 10.0 * v]
            }),
        )
    };
    let report = profile_report(
        vec![right_profile(GeometricCharacteristic::ProfileLine, &["A"])],
        vec![turned(1.0)],
    );
    let m = evaluated(&report, "right_profile");
    assert!(m.deviation_mm < 1e-6, "{m:?}");
    assert!((m.profile.unwrap().rotation_rad - 0.001).abs() < 1e-6);
    // A composite lower segment with no datums relates each row alone.
    let mut composite = right_profile(GeometricCharacteristic::ProfileLine, &["A", "B"]);
    composite.refinement = Some(DrawingCompositeRefinement {
        tolerance: Quantity::length(0.01, LengthUnit::Millimeter),
        datums: Vec::new(),
        refinement: None,
    });
    let report = profile_report(vec![composite], vec![right_face(bow)]);
    let m = evaluated(&report, "right_profile");
    let lower = m.refinement.clone().unwrap();
    assert!(lower.deviation_mm < 1e-9 && lower.conforms, "{lower:?}");
    assert!(m.conforms);
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
    assert!(error.contains("profile and runout controls"), "{error}");
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

/// An external cone about `center` (shifted `axial` along +z): radius
/// `8 + z·tan(half_angle)` at each of three heights, 36 points around, with
/// nominal points on the Ø16-based cone about `HOLE` and outward normals.
fn cone(
    id: &str,
    center: [f64; 2],
    axial: f64,
    half_angle: f64,
    nominal_angle: f64,
) -> MeasuredFeature {
    let mut points = Vec::new();
    let mut nominals = Vec::new();
    for k in 0..36 {
        let t = k as f64 * std::f64::consts::TAU / 36.0;
        let (s, c) = t.sin_cos();
        for z in [1.0, 5.0, 9.0] {
            let r = 8.0 + (z - axial) * half_angle.tan();
            points.push([center[0] + r * c, center[1] + r * s, z]);
            let n = 8.0 + z * nominal_angle.tan();
            let (sa, ca) = nominal_angle.sin_cos();
            nominals.push(MeasuredNominal {
                point_mm: [HOLE[0] + n * c, HOLE[1] + n * s, z],
                normal: [ca * c, ca * s, -sa],
            });
        }
    }
    let mut m = measured(id, points);
    m.nominals = nominals;
    m
}

#[test]
fn conical_runout_reads_normal_to_the_cone() {
    let definition = plate();
    let graph = graph_of(&definition);
    let page = runout_page("hole", &["D"], Some(FeatureOfSizeKind::Internal));
    let bore = cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| 5.0);
    let half = 30f64.to_radians();
    let record = |cone_id: &dyn Fn(&str) -> MeasuredFeature| {
        let mut record = runout_record(bore.clone(), Vec::new());
        record.controls = vec![cone_id("circ"), cone_id("total")];
        record
    };
    // A 0.02 mm eccentric cone reads 2 · 0.02 · cos 30° square to its face,
    // not the 0.04 a radial indicator would.
    let eccentric = [HOLE[0] + 0.02, HOLE[1]];
    let report = page
        .evaluate_inspection(&graph, &record(&|id| cone(id, eccentric, 0.0, half, half)))
        .unwrap();
    let expected = 0.04 * half.cos();
    for id in ["circ", "total"] {
        let m = evaluated(&report, id);
        assert!((m.deviation_mm - expected).abs() < 1e-4, "{id}: {m:?}");
    }
    // Shifted 0.05 mm along the axis, every reading moves together.
    let report = page
        .evaluate_inspection(&graph, &record(&|id| cone(id, eccentric, 0.05, half, half)))
        .unwrap();
    assert!((evaluated(&report, "total").deviation_mm - expected).abs() < 1e-4);
    // A cone ground 0.1° steep stays round (no circular runout) but its
    // face walks away from the indicator along its length.
    let steep = half + 0.1f64.to_radians();
    let report = page
        .evaluate_inspection(&graph, &record(&|id| cone(id, HOLE, 0.0, steep, half)))
        .unwrap();
    assert!(evaluated(&report, "circ").deviation_mm < 1e-4);
    // Over 8 mm the radius drifts 8 · (tan 30.1° − tan 30°), read square to
    // the face.
    let total = evaluated(&report, "total");
    let drift = 8.0 * (steep.tan() - half.tan()) * half.cos();
    assert!((total.deviation_mm - drift).abs() < 1e-5, "{total:?}");
}

#[test]
fn runout_controls_take_surfaces_and_nominals() {
    let definition = solid_plate();
    let graph = graph_of(&definition);
    let session = Session::new().unwrap();
    // The bore wall itself, as a runout surface about the bore's datum axis,
    // with nominals projected from the part: a round bore reads nothing.
    let mut page = runout_page("hole", &["D"], Some(FeatureOfSizeKind::Internal));
    for f in &mut page.feature_control_frames {
        f.surface = Some(FaceSelector::Named("bore_wall".into()));
    }
    page.validate(&graph).unwrap();
    let bore = cylinder([HOLE[0], HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| 5.0);
    let wall = cylinder([HOLE[0] + 0.01, HOLE[1], 0.0], [0.0, 0.0, 1.0], |_| 5.0);
    let report = page
        .evaluate_inspection_on_model(&graph, &session, &runout_record(bore, wall))
        .unwrap();
    // The wall 0.01 mm off the datum bore reads 0.02.
    assert!((evaluated(&report, "circ").deviation_mm - 0.02).abs() < 1e-5);
    let mut flat = page.clone();
    flat.feature_control_frames[0].characteristic = GeometricCharacteristic::Flatness;
    flat.feature_control_frames[0].datums.clear();
    let error = flat.validate(&graph).unwrap_err().message;
    assert!(error.contains("profile and runout controls"), "{error}");
}

fn uncertain(expanded: f64, rule: DecisionRule) -> Option<MeasurementUncertainty> {
    Some(MeasurementUncertainty {
        expanded_mm: expanded,
        rule,
        controls: BTreeMap::new(),
        simulation: None,
    })
}

/// The top face profile (0.1 mm to A) raised `raise` mm, under `uncertainty`.
fn decided(raise: f64, uncertainty: Option<MeasurementUncertainty>) -> ControlMeasurement {
    let (definition, _) = shift_setup(DatumMaterialBoundary::Regardless);
    let graph = graph_of(&definition);
    let mut page = page();
    page.feature_control_frames = vec![top_profile(0.1, &["A"])];
    let mut record = nominal_record();
    record.controls = vec![top(|_| 10.0 + raise)];
    record.uncertainty = uncertainty;
    evaluated(
        &page.evaluate_inspection(&graph, &record).unwrap(),
        "top_profile",
    )
}

#[test]
fn decision_rules_apply_the_expanded_uncertainty() {
    use DecisionRule::*;
    // 0.09 mm against 0.1 with U = 0.02: inside, but within U of the limit.
    let simple = decided(0.045, uncertain(0.02, SimpleAcceptance));
    assert!(simple.conforms && simple.indeterminate, "{simple:?}");
    assert_eq!(simple.uncertainty_mm, 0.02);
    let guarded = decided(0.045, uncertain(0.02, GuardedAcceptance));
    assert!(!guarded.conforms && guarded.indeterminate);
    // 0.11 fails simply, but nonconformance is not proven.
    assert!(!decided(0.055, uncertain(0.02, SimpleAcceptance)).conforms);
    let rejection = decided(0.055, uncertain(0.02, GuardedRejection));
    assert!(rejection.conforms && rejection.indeterminate);
    // Far from the limit the rules agree and nothing is indeterminate.
    for rule in [SimpleAcceptance, GuardedAcceptance, GuardedRejection] {
        let m = decided(0.01, uncertain(0.02, rule));
        assert!(m.conforms && !m.indeterminate, "{rule:?}");
        assert!(!decided(0.09, uncertain(0.02, rule)).conforms, "{rule:?}");
    }
    // Without a record uncertainty, results are unchanged.
    let plain = decided(0.045, None);
    assert!(plain.conforms && !plain.indeterminate && plain.uncertainty_mm == 0.0);
    // A per-control U overrides the record's.
    let mut exact = uncertain(0.02, GuardedAcceptance).unwrap();
    exact.controls.insert("top_profile".into(), 0.0);
    let m = decided(0.045, Some(exact));
    assert!(m.conforms && !m.indeterminate && m.uncertainty_mm == 0.0);
}

#[test]
fn uncertainty_is_simulated_from_the_measurement() {
    use DecisionRule::*;
    let simulated = |supplied: f64, point: f64, seed: u64, rule| {
        let mut u = uncertain(supplied, rule).unwrap();
        u.simulation = Some(UncertaintySimulation {
            point_mm: point,
            trials: 64,
            coverage: 2.0,
            seed,
        });
        Some(u)
    };
    // Without probing error every trial repeats the measurement: only the
    // supplied U remains.
    let m = decided(0.02, simulated(0.02, 0.0, 1, SimpleAcceptance));
    assert_eq!(m.uncertainty_mm, 0.02);
    // With 2 µm per coordinate the profile's deviation spreads; the same
    // seed repeats it exactly, and twice the error about doubles it.
    let u = decided(0.02, simulated(0.0, 0.002, 1, SimpleAcceptance)).uncertainty_mm;
    assert!(u > 0.001 && u < 0.02, "{u}");
    assert_eq!(
        decided(0.02, simulated(0.0, 0.002, 1, SimpleAcceptance)).uncertainty_mm,
        u
    );
    assert_ne!(
        decided(0.02, simulated(0.0, 0.002, 2, SimpleAcceptance)).uncertainty_mm,
        u
    );
    let doubled = decided(0.02, simulated(0.0, 0.004, 1, SimpleAcceptance)).uncertainty_mm;
    assert!(doubled / u > 1.5 && doubled / u < 2.5, "{u} {doubled}");
    // A supplied U for the other contributions combines in quadrature.
    let combined = decided(0.02, simulated(0.01, 0.002, 1, SimpleAcceptance)).uncertainty_mm;
    assert!((combined - 0.01_f64.hypot(u)).abs() < 1e-12, "{combined}");
    // Guarded acceptance applies it: 0.096 mm against 0.1 is inside alone
    // but not with the simulated U.
    assert!(decided(0.048, None).conforms);
    let m = decided(0.048, simulated(0.0, 0.002, 1, GuardedAcceptance));
    assert!(!m.conforms && m.indeterminate, "{m:?}");
    // Settings are checked.
    let (definition, _) = shift_setup(DatumMaterialBoundary::Regardless);
    let graph = graph_of(&definition);
    let mut page = page();
    page.feature_control_frames = vec![top_profile(0.1, &["A"])];
    let mut record = nominal_record();
    record.controls = vec![top(|_| 10.0)];
    for bad in [(0.002, 1, 2.0), (-0.001, 32, 2.0), (0.002, 32, 0.0)] {
        let mut u = uncertain(0.0, SimpleAcceptance).unwrap();
        u.simulation = Some(UncertaintySimulation {
            point_mm: bad.0,
            trials: bad.1,
            coverage: bad.2,
            seed: 0,
        });
        record.uncertainty = Some(u);
        assert!(
            page.evaluate_inspection(&graph, &record).is_err(),
            "{bad:?}"
        );
    }
}

#[test]
fn guarded_decisions_cover_composite_segments_and_validate() {
    let (definition, page) = composite_page(0.2, 0.04, &["A"]);
    let graph = graph_of(&definition);
    // The lower segment reads ~0 against 0.04, the upper ~0.13 against 0.2.
    let mut record = composite_record([0.06, 0.02], 0.001, [[0.0; 2]; 4]);
    record.uncertainty = uncertain(0.039, DecisionRule::GuardedAcceptance);
    let m = evaluated(&page.evaluate_inspection(&graph, &record).unwrap(), "hole0");
    assert!(m.conforms && m.refinement.unwrap().conforms);
    record.uncertainty = uncertain(0.041, DecisionRule::GuardedAcceptance);
    let m = evaluated(&page.evaluate_inspection(&graph, &record).unwrap(), "hole0");
    let lower = m.refinement.clone().unwrap();
    assert!(!lower.conforms && !m.conforms && m.indeterminate, "{m:?}");
    // Bad uncertainties fail the record; records round-trip.
    for (expanded, control, reason) in [
        (-0.01, None, "finite and nonnegative"),
        (f64::NAN, None, "finite and nonnegative"),
        (0.01, Some(("hole0", -1.0)), "finite and nonnegative"),
        (0.01, Some(("missing", 0.01)), "unknown control 'missing'"),
    ] {
        let mut bad = uncertain(expanded, DecisionRule::GuardedRejection).unwrap();
        if let Some((id, u)) = control {
            bad.controls.insert(id.into(), u);
        }
        record.uncertainty = Some(bad);
        let error = page
            .evaluate_inspection(&graph, &record)
            .unwrap_err()
            .message;
        assert!(error.contains(reason), "{error}");
    }
    let mut good = uncertain(0.004, DecisionRule::GuardedRejection).unwrap();
    good.controls.insert("hole1".into(), 0.006);
    record.uncertainty = Some(good);
    let json = serde_json::to_string(&record).unwrap();
    assert!(json.contains("\"guarded_rejection\""));
    assert_eq!(
        serde_json::from_str::<InspectionRecord>(&json).unwrap(),
        record
    );
}
