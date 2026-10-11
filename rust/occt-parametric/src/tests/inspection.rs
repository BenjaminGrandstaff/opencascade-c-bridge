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
