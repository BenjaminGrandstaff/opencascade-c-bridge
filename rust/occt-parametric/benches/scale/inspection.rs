//! Measured GD&T inspection at drawing scale: one shared datum reference
//! frame, 10,000 position controls with measured holes, and a dense surface.

use super::*;
use occt_parametric::{
    ControlResult, DatumMaterialBoundary, DrawingDatumFeature, DrawingDatumReference,
    DrawingDatumReferenceFrame, DrawingDefinition, DrawingFeatureControlFrame,
    DrawingGdtAttachment, DrawingSizeLimits, DrawingView, DrawingViewKind, FeatureOfSizeKind,
    GeometricCharacteristic, GeometricToleranceZone, InspectionRecord, InstanceOutputRef,
    MeasuredFeature, ToleranceMaterialCondition,
};

const HOLES_PER_SIDE: usize = 10;
const CONTROLS: usize = 10_000;
const SURFACE_POINTS: usize = 100_000;

fn literal(p: [f64; 3]) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(
        p[0],
        p[1],
        p[2],
        LengthUnit::Millimeter,
    ))
}

fn plane(id: &str, p: [f64; 3], n: [f64; 3]) -> DatumDefinition {
    DatumDefinition {
        id: id.into(),
        kind: DatumKind::Plane {
            origin: literal(p),
            normal: VectorExpr::Literal(VectorQuantity::scalars(n[0], n[1], n[2])),
        },
    }
}

fn hole_center(hole: usize) -> [f64; 2] {
    [
        10.0 + 20.0 * (hole % HOLES_PER_SIDE) as f64,
        10.0 + 20.0 * (hole / HOLES_PER_SIDE) as f64,
    ]
}

fn attachment(anchor: &str) -> DrawingGdtAttachment {
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

fn grid(count: usize, point: impl Fn(f64, f64) -> [f64; 3]) -> Vec<[f64; 3]> {
    let side = (count as f64).sqrt().ceil() as usize;
    (0..count)
        .map(|i| {
            point(
                (i % side) as f64 / side as f64,
                (i / side) as f64 / side as f64,
            )
        })
        .collect()
}

pub(crate) fn inspection_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        format!(
            "inspection: {CONTROLS} measured position controls + {SURFACE_POINTS}-point flatness"
        ),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let mut definition = definition.clone();
            let side = 20.0 * HOLES_PER_SIDE as f64;
            definition.datums = vec![
                plane("bottom", [0.0; 3], [0.0, 0.0, -1.0]),
                plane("left", [0.0; 3], [-1.0, 0.0, 0.0]),
                plane("front", [0.0; 3], [0.0, -1.0, 0.0]),
                plane("top", [0.0, 0.0, 10.0], [0.0, 0.0, 1.0]),
            ];
            definition
                .datums
                .extend((0..HOLES_PER_SIDE * HOLES_PER_SIDE).map(|h| {
                    let [x, y] = hole_center(h);
                    DatumDefinition {
                        id: format!("hole-{h}"),
                        kind: DatumKind::Axis {
                            origin: literal([x, y, 0.0]),
                            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                        },
                    }
                }));
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let reference = |id: &str| DrawingDatumReference {
                datum_feature: id.into(),
                boundary: DatumMaterialBoundary::Regardless,
            };
            let mut controls: Vec<DrawingFeatureControlFrame> = (0..CONTROLS)
                .map(|i| DrawingFeatureControlFrame {
                    size_limits: Some(DrawingSizeLimits {
                        kind: FeatureOfSizeKind::Internal,
                        lower: Quantity::length(5.9, LengthUnit::Millimeter),
                        upper: Quantity::length(6.1, LengthUnit::Millimeter),
                    }),
                    datum_reference_frame: Some("ABC".into()),
                    refinement: None,
                    id: format!("position-{i}"),
                    attachment: attachment(&format!(
                        "hole-{}",
                        i % (HOLES_PER_SIDE * HOLES_PER_SIDE)
                    )),
                    characteristic: GeometricCharacteristic::Position,
                    tolerance: Quantity::length(0.1, LengthUnit::Millimeter),
                    display_unit: LengthUnit::Millimeter,
                    precision: 2,
                    zone: GeometricToleranceZone::Diameter,
                    material: ToleranceMaterialCondition::Maximum,
                    feature_of_size: true,
                    datums: Vec::new(),
                })
                .collect();
            let mut flat = controls[0].clone();
            flat.id = "flat".into();
            flat.attachment = attachment("top");
            flat.characteristic = GeometricCharacteristic::Flatness;
            flat.zone = GeometricToleranceZone::Characteristic;
            flat.material = ToleranceMaterialCondition::Regardless;
            flat.feature_of_size = false;
            flat.size_limits = None;
            flat.datum_reference_frame = None;
            controls.push(flat);
            let drawing = DrawingDefinition {
                datum_reference_frames: vec![DrawingDatumReferenceFrame {
                    id: "ABC".into(),
                    datums: vec![reference("A"), reference("B"), reference("C")],
                }],
                datum_features: [("A", "bottom"), ("B", "left"), ("C", "front")]
                    .into_iter()
                    .map(|(id, anchor)| DrawingDatumFeature {
                        id: id.into(),
                        label: id.into(),
                        feature_of_size: false,
                        attachment: attachment(anchor),
                        size_kind: None,
                    })
                    .collect(),
                feature_control_frames: controls,
                surface_textures: Vec::new(),
                parts_list: None,
                balloons: Vec::new(),
                releases: Vec::new(),
                revision_table: None,
                sheet: None,
                id: "inspected".into(),
                title: "Inspected plate".into(),
                paper_size_mm: [1000.0, 1000.0],
                guides: Vec::new(),
                dimensions: Vec::new(),
                notes: Vec::new(),
                metadata: Default::default(),
                views: vec![DrawingView {
                    id: "top".into(),
                    outputs: vec![InstanceOutputRef {
                        instance: "part".into(),
                        output: "body".into(),
                    }],
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                    paper_origin_mm: [10.0, 100.0],
                    scale: 1.0,
                    show_hidden: false,
                    kind: DrawingViewKind::Orthographic,
                    detail: None,
                    hatching: None,
                    material_hatching: Default::default(),
                }],
            };
            let datum = |id: &str, point: &dyn Fn(f64, f64) -> [f64; 3]| MeasuredFeature {
                id: id.into(),
                points_mm: grid(400, point),
            };
            let mut measured: Vec<MeasuredFeature> = (0..CONTROLS)
                .map(|i| {
                    // Every tenth hole is 0.1 mm off position, beyond its bonus.
                    let [x, y] = hole_center(i % (HOLES_PER_SIDE * HOLES_PER_SIDE));
                    let shift = if i % 10 == 0 { 0.1 } else { 0.01 };
                    MeasuredFeature {
                        id: format!("position-{i}"),
                        points_mm: (0..36)
                            .flat_map(|k| [2.0, 8.0].map(|z| (k, z)))
                            .map(|(k, z)| {
                                let a = k as f64 * std::f64::consts::TAU / 36.0;
                                [x + shift + 3.0 * a.cos(), y + 3.0 * a.sin(), z]
                            })
                            .collect(),
                    }
                })
                .collect();
            measured.push(MeasuredFeature {
                id: "flat".into(),
                points_mm: grid(SURFACE_POINTS, |u, v| {
                    [
                        side * u,
                        side * v,
                        10.0 + 0.0002 * side * u + 0.004 * (40.0 * u).sin(),
                    ]
                }),
            });
            let record = InspectionRecord {
                drawing: "inspected".into(),
                surface_textures: Vec::new(),
                datum_features: vec![
                    datum("A", &|u, v| [side * u, side * v, 0.0]),
                    datum("B", &|u, v| [0.0, side * u, 10.0 * v]),
                    datum("C", &|u, v| [side * u, 0.0, 10.0 * v]),
                ],
                controls: measured,
            };
            let report = drawing.evaluate_inspection(&graph, &record)?;
            let mut conforming = 0;
            for (i, control) in report.controls.iter().enumerate() {
                let ControlResult::Evaluated(m) = &control.result else {
                    return Err(failure(format!("{} was not evaluated", control.control)));
                };
                let expected = i == CONTROLS || i % 10 != 0;
                if m.conforms != expected {
                    return Err(failure(format!(
                        "{} conformance {}",
                        control.control, m.conforms
                    )));
                }
                conforming += usize::from(m.conforms);
            }
            Ok(format!(
                "{conforming} of {} conform, {} measured points",
                report.controls.len(),
                CONTROLS * 72 + SURFACE_POINTS + 1200
            ))
        },
    )
}

const FORM_CONTROLS: usize = 1_000;

/// Points on a hole of radius 3 about `center`: `sections` circles of
/// `per_section` points between z = 1 and 9, with a radial taper across the
/// length and `lobes`-lobed waviness of the given amplitude.
fn bore(
    [x, y]: [f64; 2],
    sections: usize,
    per_section: usize,
    taper: f64,
    waviness: f64,
) -> Vec<[f64; 3]> {
    (0..sections)
        .flat_map(|s| (0..per_section).map(move |k| (s, k)))
        .map(|(s, k)| {
            let along = s as f64 / (sections - 1) as f64;
            let a = k as f64 * std::f64::consts::TAU / per_section as f64;
            let r = 3.0 + taper * along + waviness * (3.0 * a).cos();
            [x + r * a.cos(), y + r * a.sin(), 1.0 + 8.0 * along]
        })
        .collect()
}

/// Minimum-zone round form: 1,000 cylindricity controls (6 sections of 66
/// points) and 1,000 circularity controls (3 sections of 72 points), every
/// tenth of each out of tolerance.
pub(crate) fn form_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        format!("inspection: {FORM_CONTROLS} cylindricity + {FORM_CONTROLS} circularity controls"),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let mut definition = definition.clone();
            definition.datums = (0..HOLES_PER_SIDE * HOLES_PER_SIDE)
                .map(|h| {
                    let [x, y] = hole_center(h);
                    DatumDefinition {
                        id: format!("hole-{h}"),
                        kind: DatumKind::Axis {
                            origin: literal([x, y, 0.0]),
                            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                        },
                    }
                })
                .collect();
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let holes = HOLES_PER_SIDE * HOLES_PER_SIDE;
            let control = |i: usize, cylindricity: bool| DrawingFeatureControlFrame {
                size_limits: None,
                datum_reference_frame: None,
                refinement: None,
                id: format!("{}-{i}", if cylindricity { "cyl" } else { "round" }),
                attachment: attachment(&format!("hole-{}", i % holes)),
                characteristic: if cylindricity {
                    GeometricCharacteristic::Cylindricity
                } else {
                    GeometricCharacteristic::Circularity
                },
                tolerance: Quantity::length(
                    if cylindricity { 0.05 } else { 0.03 },
                    LengthUnit::Millimeter,
                ),
                display_unit: LengthUnit::Millimeter,
                precision: 2,
                zone: GeometricToleranceZone::Characteristic,
                material: ToleranceMaterialCondition::Regardless,
                feature_of_size: false,
                datums: Vec::new(),
            };
            let drawing = DrawingDefinition {
                datum_reference_frames: Vec::new(),
                datum_features: Vec::new(),
                feature_control_frames: (0..FORM_CONTROLS)
                    .map(|i| control(i, true))
                    .chain((0..FORM_CONTROLS).map(|i| control(i, false)))
                    .collect(),
                surface_textures: Vec::new(),
                parts_list: None,
                balloons: Vec::new(),
                releases: Vec::new(),
                revision_table: None,
                sheet: None,
                id: "round".into(),
                title: "Round form".into(),
                paper_size_mm: [1000.0, 1000.0],
                guides: Vec::new(),
                dimensions: Vec::new(),
                notes: Vec::new(),
                metadata: Default::default(),
                views: vec![DrawingView {
                    id: "top".into(),
                    outputs: vec![InstanceOutputRef {
                        instance: "part".into(),
                        output: "body".into(),
                    }],
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                    paper_origin_mm: [10.0, 100.0],
                    scale: 1.0,
                    show_hidden: false,
                    kind: DrawingViewKind::Orthographic,
                    detail: None,
                    hatching: None,
                    material_hatching: Default::default(),
                }],
            };
            // Out-of-tolerance holes taper 0.06 mm or have 0.02 mm lobes
            // (a 0.04 mm zone); the rest carry 0.005 mm lobes.
            let controls = (0..FORM_CONTROLS)
                .map(|i| MeasuredFeature {
                    id: format!("cyl-{i}"),
                    points_mm: bore(
                        hole_center(i % holes),
                        6,
                        66,
                        if i.is_multiple_of(10) { 0.06 } else { 0.0 },
                        0.005,
                    ),
                })
                .chain((0..FORM_CONTROLS).map(|i| MeasuredFeature {
                    id: format!("round-{i}"),
                    points_mm: bore(
                        hole_center(i % holes),
                        3,
                        72,
                        0.0,
                        if i.is_multiple_of(10) { 0.02 } else { 0.005 },
                    ),
                }))
                .collect();
            let record = InspectionRecord {
                drawing: "round".into(),
                surface_textures: Vec::new(),
                datum_features: Vec::new(),
                controls,
            };
            let report = drawing.evaluate_inspection(&graph, &record)?;
            let mut conforming = 0;
            for (i, control) in report.controls.iter().enumerate() {
                let ControlResult::Evaluated(m) = &control.result else {
                    return Err(failure(format!("{} was not evaluated", control.control)));
                };
                if m.conforms == (i % FORM_CONTROLS).is_multiple_of(10) {
                    return Err(failure(format!(
                        "{} measured {} conformance {}",
                        control.control, m.deviation_mm, m.conforms
                    )));
                }
                conforming += usize::from(m.conforms);
            }
            Ok(format!(
                "{conforming} of {} conform, {} measured points",
                report.controls.len(),
                FORM_CONTROLS * (396 + 216)
            ))
        },
    )
}

/// 100 bore datum features of size (internal), each simulated once by its
/// largest inscribed cylinder, and 1,000 circular plus 1,000 total runout
/// controls on eccentric surfaces about them; every tenth surface reads
/// 0.06 mm, over its 0.05 mm tolerance.
pub(crate) fn runout_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        format!(
            "inspection: {FORM_CONTROLS} circular + {FORM_CONTROLS} total runout controls on 100 fitted datum axes"
        ),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let mut definition = definition.clone();
            let holes = HOLES_PER_SIDE * HOLES_PER_SIDE;
            definition.datums = (0..holes)
                .map(|h| {
                    let [x, y] = hole_center(h);
                    DatumDefinition {
                        id: format!("hole-{h}"),
                        kind: DatumKind::Axis {
                            origin: literal([x, y, 0.0]),
                            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                        },
                    }
                })
                .collect();
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            const LETTERS: &[u8] = b"ABCDEFGHJKLMNPRSTUVWXYZ";
            let label = |h: usize| {
                [LETTERS[h / LETTERS.len()], LETTERS[h % LETTERS.len()]]
                    .iter()
                    .map(|&c| char::from(c))
                    .collect::<String>()
            };
            let control = |i: usize, total: bool| DrawingFeatureControlFrame {
                size_limits: None,
                datum_reference_frame: None,
                refinement: None,
                id: format!("{}-{i}", if total { "total" } else { "circular" }),
                attachment: attachment(&format!("hole-{}", i % holes)),
                characteristic: if total {
                    GeometricCharacteristic::TotalRunout
                } else {
                    GeometricCharacteristic::CircularRunout
                },
                tolerance: Quantity::length(0.05, LengthUnit::Millimeter),
                display_unit: LengthUnit::Millimeter,
                precision: 2,
                zone: GeometricToleranceZone::Characteristic,
                material: ToleranceMaterialCondition::Regardless,
                feature_of_size: false,
                datums: vec![DrawingDatumReference {
                    datum_feature: format!("D{}", i % holes),
                    boundary: DatumMaterialBoundary::Regardless,
                }],
            };
            let drawing = DrawingDefinition {
                datum_reference_frames: Vec::new(),
                datum_features: (0..holes)
                    .map(|h| DrawingDatumFeature {
                        id: format!("D{h}"),
                        label: label(h),
                        feature_of_size: true,
                        attachment: attachment(&format!("hole-{h}")),
                        size_kind: Some(FeatureOfSizeKind::Internal),
                    })
                    .collect(),
                feature_control_frames: (0..FORM_CONTROLS)
                    .map(|i| control(i, false))
                    .chain((0..FORM_CONTROLS).map(|i| control(i, true)))
                    .collect(),
                surface_textures: Vec::new(),
                parts_list: None,
                balloons: Vec::new(),
                releases: Vec::new(),
                revision_table: None,
                sheet: None,
                id: "runout".into(),
                title: "Runout".into(),
                paper_size_mm: [1000.0, 1000.0],
                guides: Vec::new(),
                dimensions: Vec::new(),
                notes: Vec::new(),
                metadata: Default::default(),
                views: vec![DrawingView {
                    id: "top".into(),
                    outputs: vec![InstanceOutputRef {
                        instance: "part".into(),
                        output: "body".into(),
                    }],
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                    paper_origin_mm: [10.0, 100.0],
                    scale: 1.0,
                    show_hidden: false,
                    kind: DrawingViewKind::Orthographic,
                    detail: None,
                    hatching: None,
                    material_hatching: Default::default(),
                }],
            };
            let eccentric = |i: usize| {
                let [x, y] = hole_center(i % holes);
                let e = if i.is_multiple_of(10) { 0.03 } else { 0.01 };
                bore([x + e, y], 3, 72, 0.0, 0.0)
            };
            let record = InspectionRecord {
                drawing: "runout".into(),
                surface_textures: Vec::new(),
                datum_features: (0..holes)
                    .map(|h| MeasuredFeature {
                        id: format!("D{h}"),
                        points_mm: bore(hole_center(h), 6, 66, 0.0, 0.005),
                    })
                    .collect(),
                controls: (0..FORM_CONTROLS)
                    .map(|i| MeasuredFeature {
                        id: format!("circular-{i}"),
                        points_mm: eccentric(i),
                    })
                    .chain((0..FORM_CONTROLS).map(|i| MeasuredFeature {
                        id: format!("total-{i}"),
                        points_mm: eccentric(i),
                    }))
                    .collect(),
            };
            let report = drawing.evaluate_inspection(&graph, &record)?;
            let mut conforming = 0;
            for (i, control) in report.controls.iter().enumerate() {
                let ControlResult::Evaluated(m) = &control.result else {
                    return Err(failure(format!(
                        "{} was not evaluated: {:?}",
                        control.control, control.result
                    )));
                };
                let expected = if (i % FORM_CONTROLS).is_multiple_of(10) {
                    0.06
                } else {
                    0.02
                };
                if (m.deviation_mm - expected).abs() > 1e-6
                    || m.conforms == (i % FORM_CONTROLS).is_multiple_of(10)
                {
                    return Err(failure(format!(
                        "{} measured {} conformance {}",
                        control.control, m.deviation_mm, m.conforms
                    )));
                }
                conforming += usize::from(m.conforms);
            }
            Ok(format!(
                "{conforming} of {} conform, {} measured points",
                report.controls.len(),
                holes * 396 + 2 * FORM_CONTROLS * 216
            ))
        },
    )
}
