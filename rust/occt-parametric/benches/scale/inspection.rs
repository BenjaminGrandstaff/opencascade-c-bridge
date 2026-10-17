//! Measured GD&T inspection at drawing scale: one shared datum reference
//! frame, 10,000 position controls with measured holes, and a dense surface.

use super::*;
use occt_parametric::{
    ControlResult, DatumMaterialBoundary, DecisionRule, DrawingCompositeRefinement,
    DrawingDatumFeature, DrawingDatumReference, DrawingDatumReferenceFrame, DrawingDefinition,
    DrawingFeatureControlFrame, DrawingGdtAttachment, DrawingProjectedZone, DrawingSizeLimits,
    DrawingView, DrawingViewKind, FeatureOfSizeKind, GeometricCharacteristic,
    GeometricToleranceZone, InspectionRecord, InstanceOutputRef, MeasuredFeature, MeasuredNominal,
    MeasuredThread, MeasurementUncertainty, NamedReference, ReferenceTarget,
    ToleranceMaterialCondition, ToleranceModifier, UncertaintySimulation, UnifiedClass,
    UnifiedSeries, UnifiedThreadSpec,
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
        faces: None,
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

/// The frame and zones of [`inspection_case`].
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Inspected {
    Square,
    /// Datum B is a 45° bevel along the plate's left edge, tilted to A, in
    /// place of the square left face.
    Oblique,
    /// Every position zone is projected 10 mm beyond A (Ⓟ).
    Projected,
    /// 1,000 controls and a 10,000-point surface, with uncertainty
    /// simulated over 32 trials of 1 µm probing error.
    Simulated,
}

pub(crate) fn inspection_case(definition: &FamilyDefinition, variant: Inspected) -> Outcome {
    let oblique = variant == Inspected::Oblique;
    let simulated = variant == Inspected::Simulated;
    let (count, surface) = if simulated {
        (CONTROLS / 10, SURFACE_POINTS / 10)
    } else {
        (CONTROLS, SURFACE_POINTS)
    };
    timed(
        format!(
            "inspection: {count} measured position controls + {surface}-point flatness{}",
            match variant {
                Inspected::Square => "",
                Inspected::Oblique => ", secondary datum at 45° to A",
                Inspected::Projected => ", projected zones",
                Inspected::Simulated => ", uncertainty simulated over 32 trials",
            }
        ),
        // Noisy trial points take the fits about 2.5 times the pivots of
        // the exact measurement.
        Duration::from_secs(if simulated { 3 } else { 1 }),
        Expectation::Required,
        || {
            let mut definition = definition.clone();
            let side = 20.0 * HOLES_PER_SIDE as f64;
            definition.datums = vec![
                plane("bottom", [0.0; 3], [0.0, 0.0, -1.0]),
                if oblique {
                    let s = std::f64::consts::FRAC_1_SQRT_2;
                    plane("left", [2.0, 0.0, 0.0], [-s, 0.0, -s])
                } else {
                    plane("left", [0.0; 3], [-1.0, 0.0, 0.0])
                },
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
                translation: false,
                datum_feature: id.into(),
                boundary: DatumMaterialBoundary::Regardless,
                common_with: Vec::new(),
            };
            let mut controls: Vec<DrawingFeatureControlFrame> = (0..count)
                .map(|i| DrawingFeatureControlFrame {
                    modifiers: Vec::new(),
                    projected: None,
                    tangent_plane: false,
                    pattern: None,
                    unequal: None,
                    surface: None,
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
            if variant == Inspected::Projected {
                for control in &mut controls {
                    control.projected = Some(DrawingProjectedZone {
                        height: Quantity::length(10.0, LengthUnit::Millimeter),
                        from: None,
                    });
                }
            }
            let mut flat = controls[0].clone();
            flat.id = "flat".into();
            flat.attachment = attachment("top");
            flat.characteristic = GeometricCharacteristic::Flatness;
            flat.zone = GeometricToleranceZone::Characteristic;
            flat.material = ToleranceMaterialCondition::Regardless;
            flat.feature_of_size = false;
            flat.size_limits = None;
            flat.datum_reference_frame = None;
            flat.projected = None;
            controls.push(flat);
            let drawing = DrawingDefinition {
                annotation_views: Vec::new(),
                dataset: None,
                approval_workflow: None,
                datum_reference_frames: vec![DrawingDatumReferenceFrame {
                    id: "ABC".into(),
                    datums: vec![reference("A"), reference("B"), reference("C")],
                }],
                datum_features: [("A", "bottom"), ("B", "left"), ("C", "front")]
                    .into_iter()
                    .map(|(id, anchor)| DrawingDatumFeature {
                        targets: Vec::new(),
                        id: id.into(),
                        label: id.into(),
                        feature_of_size: false,
                        attachment: attachment(anchor),
                        size_kind: None,
                        size_limits: None,
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
                    thread_representation: false,
                }],
            };
            let datum = |id: &str, point: &dyn Fn(f64, f64) -> [f64; 3]| MeasuredFeature {
                nominals: Vec::new(),
                id: id.into(),
                points_mm: grid(400, point),
            };
            let mut measured: Vec<MeasuredFeature> = (0..count)
                .map(|i| {
                    // Every tenth hole is 0.1 mm off position, beyond its bonus.
                    let [x, y] = hole_center(i % (HOLES_PER_SIDE * HOLES_PER_SIDE));
                    let shift = if i % 10 == 0 { 0.1 } else { 0.01 };
                    MeasuredFeature {
                        nominals: Vec::new(),
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
                nominals: Vec::new(),
                id: "flat".into(),
                points_mm: grid(surface, |u, v| {
                    [
                        side * u,
                        side * v,
                        10.0 + 0.0002 * side * u + 0.004 * (40.0 * u).sin(),
                    ]
                }),
            });
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: simulated.then(|| MeasurementUncertainty {
                    expanded_mm: 0.0,
                    rule: DecisionRule::SimpleAcceptance,
                    controls: Default::default(),
                    simulation: Some(UncertaintySimulation {
                        point_mm: 0.001,
                        trials: 32,
                        coverage: 2.0,
                        seed: 7,
                    }),
                }),
                drawing: "inspected".into(),
                surface_textures: Vec::new(),
                datum_features: vec![
                    datum("A", &|u, v| [side * u, side * v, 0.0]),
                    if oblique {
                        datum("B", &|u, v| [2.0 - 2.0 * v, side * u, 2.0 * v])
                    } else {
                        datum("B", &|u, v| [0.0, side * u, 10.0 * v])
                    },
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
                let expected = i == count || i % 10 != 0;
                if simulated && !(m.uncertainty_mm > 0.0 && m.uncertainty_mm < 0.01) {
                    return Err(failure(format!(
                        "{} simulated U {}",
                        control.control, m.uncertainty_mm
                    )));
                }
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
                count * 72 + surface + 1200
            ))
        },
    )
}

const FORM_CONTROLS: usize = 1_000;

/// Points on a hole of radius 3 about `center`: `sections` circles of
/// `per_section` points between z = 1 and 9, with a radial taper across the
/// length and `lobes`-lobed waviness of the given amplitude.
/// An external 30° cone (radius 3 + z·tan 30°) about `nominal` shifted `e`
/// in x: 3 sections of 72 points, with nominal points and outward normals.
fn cone(nominal: [f64; 2], e: f64, id: String) -> MeasuredFeature {
    let half = 30f64.to_radians();
    let (sa, ca) = half.sin_cos();
    let (points_mm, nominals) = (0..3)
        .flat_map(|s| (0..72).map(move |k| (s, k)))
        .map(|(s, k)| {
            let z = 1.0 + 4.0 * s as f64;
            let (sin, cos) = (k as f64 * std::f64::consts::TAU / 72.0).sin_cos();
            let r = 3.0 + z * half.tan();
            (
                [nominal[0] + e + r * cos, nominal[1] + r * sin, z],
                MeasuredNominal {
                    point_mm: [nominal[0] + r * cos, nominal[1] + r * sin, z],
                    normal: [ca * cos, ca * sin, -sa],
                },
            )
        })
        .unzip();
    MeasuredFeature {
        id,
        points_mm,
        nominals,
    }
}

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
                modifiers: Vec::new(),
                projected: None,
                tangent_plane: false,
                pattern: None,
                unequal: None,
                surface: None,
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
                annotation_views: Vec::new(),
                dataset: None,
                approval_workflow: None,
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
                    thread_representation: false,
                }],
            };
            // Out-of-tolerance holes taper 0.06 mm or have 0.02 mm lobes
            // (a 0.04 mm zone); the rest carry 0.005 mm lobes.
            let controls = (0..FORM_CONTROLS)
                .map(|i| MeasuredFeature {
                    nominals: Vec::new(),
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
                    nominals: Vec::new(),
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
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: None,
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
/// With `common`, each datum is the common axis D-J of two coaxial bore
/// journals (200 envelope fits), through their envelope-axis midpoints.
pub(crate) fn runout_case(definition: &FamilyDefinition, common: bool, conical: bool) -> Outcome {
    timed(
        format!(
            "inspection: {FORM_CONTROLS} circular + {FORM_CONTROLS} total runout controls{} on 100 fitted {} axes",
            if conical {
                " on 30° cones with nominals"
            } else {
                ""
            },
            if common { "common datum" } else { "datum" }
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
                modifiers: Vec::new(),
                projected: None,
                tangent_plane: false,
                pattern: None,
                unequal: None,
                surface: None,
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
                    translation: false,
                    datum_feature: format!("D{}", i % holes),
                    boundary: DatumMaterialBoundary::Regardless,
                    common_with: if common {
                        vec![format!("J{}", i % holes)]
                    } else {
                        Vec::new()
                    },
                }],
            };
            let drawing = DrawingDefinition {
                annotation_views: Vec::new(),
                dataset: None,
                approval_workflow: None,
                datum_reference_frames: Vec::new(),
                datum_features: (0..holes)
                    .map(|h| (format!("D{h}"), label(h), h))
                    .chain(
                        (0..holes)
                            .filter(|_| common)
                            .map(|h| (format!("J{h}"), label(holes + h), h)),
                    )
                    .map(|(id, label, h)| DrawingDatumFeature {
                        targets: Vec::new(),
                        id,
                        label,
                        feature_of_size: true,
                        attachment: attachment(&format!("hole-{h}")),
                        size_kind: Some(FeatureOfSizeKind::Internal),
                        size_limits: None,
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
                    thread_representation: false,
                }],
            };
            let eccentric = |i: usize, id: String| {
                let [x, y] = hole_center(i % holes);
                let e = if i.is_multiple_of(10) { 0.03 } else { 0.01 };
                if conical {
                    cone([x, y], e, id)
                } else {
                    MeasuredFeature {
                        nominals: Vec::new(),
                        id,
                        points_mm: bore([x + e, y], 3, 72, 0.0, 0.0),
                    }
                }
            };
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: None,
                drawing: "runout".into(),
                surface_textures: Vec::new(),
                datum_features: (0..holes)
                    .map(|h| MeasuredFeature {
                        nominals: Vec::new(),
                        id: format!("D{h}"),
                        points_mm: bore(hole_center(h), 6, 66, 0.0, 0.005),
                    })
                    .chain((0..holes).filter(|_| common).map(|h| {
                        MeasuredFeature {
                            nominals: Vec::new(),
                            id: format!("J{h}"),
                            // A second journal 20 mm further along the same axis.
                            points_mm: bore(hole_center(h), 6, 66, 0.0, 0.005)
                                .into_iter()
                                .map(|[x, y, z]| [x, y, z + 20.0])
                                .collect(),
                        }
                    }))
                    .collect(),
                controls: (0..FORM_CONTROLS)
                    .map(|i| eccentric(i, format!("circular-{i}")))
                    .chain((0..FORM_CONTROLS).map(|i| eccentric(i, format!("total-{i}"))))
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
                // Twice the eccentricity, read square to a cone's face.
                let expected = if (i % FORM_CONTROLS).is_multiple_of(10) {
                    0.06
                } else {
                    0.02
                } * if conical {
                    30f64.to_radians().cos()
                } else {
                    1.0
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
                holes * 396 * if common { 2 } else { 1 } + 2 * FORM_CONTROLS * 216
            ))
        },
    )
}

/// 10,000 position controls on 100 pattern holes located A | B(MMB) | C from
/// a center bore B (Ø9.9-10.1, measured Ø10.06: 0.08 mm of shift). Every
/// hole sits 0.05 mm off in x (±0.01 mm in y): one shared datum shift under
/// the simultaneous requirement brings all of them inside Ø0.04.
///
/// With `slot`, C is a 7.9-8.1 slot (width datum) through the bore at MMB,
/// measured square and 8.0 wide over 20 mm either side of the bore (2.5 mrad
/// of rotational shift). The pattern is turned 2 mrad about the bore and
/// sits 0.05 mm off along the slot: one shared translation and rotation of
/// the frame bring every hole inside Ø0.04.
/// The datum frame of [`datum_shift_case`].
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ShiftFrame {
    /// A | B(M) | C: a face, a bore and a contacting plane.
    Bore,
    /// A | B(M) | C(M): a face, a bore and a clocking slot.
    Slot,
    /// B(M) | A | C(M): the bore as primary axis, stopped by the face and
    /// clocked by the slot.
    AxisSlot,
    /// A | B(M) | C(M): a face, a bore and a dowel hole 25 mm from it.
    Pin,
}

pub(crate) fn datum_shift_case(definition: &FamilyDefinition, frame: ShiftFrame) -> Outcome {
    let slot = frame != ShiftFrame::Bore;
    timed(
        match frame {
            ShiftFrame::Bore => {
                format!("inspection: {CONTROLS} position controls sharing one MMB datum shift")
            }
            ShiftFrame::Slot => format!(
                "inspection: {CONTROLS} position controls with bore and slot MMB shift and rotation"
            ),
            ShiftFrame::AxisSlot => format!(
                "inspection: {CONTROLS} position controls off a primary bore axis clocked by a slot at MMB"
            ),
            ShiftFrame::Pin => format!(
                "inspection: {CONTROLS} position controls with bore and dowel hole MMB shift and rotation"
            ),
        },
        Duration::from_millis(500),
        Expectation::Required,
        || {
            let mut definition = definition.clone();
            let side = 20.0 * HOLES_PER_SIDE as f64;
            let bore = [side / 2.0 + 10.0, side / 2.0 + 10.0];
            definition.datums = vec![
                plane("bottom", [0.0; 3], [0.0, 0.0, -1.0]),
                plane("front", [0.0; 3], [0.0, -1.0, 0.0]),
                plane("slot", [bore[0], bore[1], 0.0], [1.0, 0.0, 0.0]),
                DatumDefinition {
                    id: "pin".into(),
                    kind: DatumKind::Axis {
                        origin: literal([bore[0], bore[1] + 25.0, 0.0]),
                        direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    },
                },
                DatumDefinition {
                    id: "bore".into(),
                    kind: DatumKind::Axis {
                        origin: literal([bore[0], bore[1], 0.0]),
                        direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    },
                },
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
            let reference = |id: &str, boundary| DrawingDatumReference {
                translation: false,
                datum_feature: id.into(),
                boundary,
                common_with: Vec::new(),
            };
            let limits = |kind, lower, upper| DrawingSizeLimits {
                kind,
                lower: Quantity::length(lower, LengthUnit::Millimeter),
                upper: Quantity::length(upper, LengthUnit::Millimeter),
            };
            let controls: Vec<DrawingFeatureControlFrame> = (0..CONTROLS)
                .map(|i| DrawingFeatureControlFrame {
                    modifiers: Vec::new(),
                    projected: None,
                    tangent_plane: false,
                    pattern: None,
                    unequal: None,
                    surface: None,
                    size_limits: Some(limits(FeatureOfSizeKind::Internal, 5.9, 6.1)),
                    datum_reference_frame: None,
                    refinement: None,
                    id: format!("position-{i}"),
                    attachment: attachment(&format!(
                        "hole-{}",
                        i % (HOLES_PER_SIDE * HOLES_PER_SIDE)
                    )),
                    characteristic: GeometricCharacteristic::Position,
                    tolerance: Quantity::length(0.04, LengthUnit::Millimeter),
                    display_unit: LengthUnit::Millimeter,
                    precision: 2,
                    zone: GeometricToleranceZone::Diameter,
                    material: ToleranceMaterialCondition::Regardless,
                    feature_of_size: true,
                    datums: {
                        let a = reference("A", DatumMaterialBoundary::Regardless);
                        let b = reference("B", DatumMaterialBoundary::Maximum);
                        let c = reference(
                            "C",
                            if slot {
                                DatumMaterialBoundary::Maximum
                            } else {
                                DatumMaterialBoundary::Regardless
                            },
                        );
                        if frame == ShiftFrame::AxisSlot {
                            vec![b, a, c]
                        } else {
                            vec![a, b, c]
                        }
                    },
                })
                .collect();
            let feature =
                |id: &str, anchor: &str, size: Option<DrawingSizeLimits>| DrawingDatumFeature {
                    targets: Vec::new(),
                    id: id.into(),
                    label: id.into(),
                    feature_of_size: size.is_some(),
                    attachment: attachment(anchor),
                    size_kind: None,
                    size_limits: size,
                };
            let drawing = DrawingDefinition {
                annotation_views: Vec::new(),
                dataset: None,
                approval_workflow: None,
                datum_reference_frames: Vec::new(),
                datum_features: vec![
                    feature("A", "bottom", None),
                    feature(
                        "B",
                        "bore",
                        Some(limits(FeatureOfSizeKind::Internal, 9.9, 10.1)),
                    ),
                    if frame == ShiftFrame::Pin {
                        DrawingDatumFeature {
                            targets: Vec::new(),
                            size_kind: Some(FeatureOfSizeKind::Internal),
                            ..feature(
                                "C",
                                "pin",
                                Some(limits(FeatureOfSizeKind::Internal, 5.9, 6.1)),
                            )
                        }
                    } else if slot {
                        DrawingDatumFeature {
                            targets: Vec::new(),
                            size_kind: Some(FeatureOfSizeKind::Internal),
                            feature_of_size: true,
                            ..feature(
                                "C",
                                "slot",
                                Some(limits(FeatureOfSizeKind::Internal, 7.9, 8.1)),
                            )
                        }
                    } else {
                        feature("C", "front", None)
                    },
                ],
                feature_control_frames: controls,
                surface_textures: Vec::new(),
                parts_list: None,
                balloons: Vec::new(),
                releases: Vec::new(),
                revision_table: None,
                sheet: None,
                id: "shifted".into(),
                title: "Datum shift".into(),
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
                    thread_representation: false,
                }],
            };
            let circle = |[x, y]: [f64; 2], radius: f64| -> Vec<[f64; 3]> {
                (0..36)
                    .flat_map(|k| [2.0, 8.0].map(|z| (k, z)))
                    .map(|(k, z)| {
                        let a = k as f64 * std::f64::consts::TAU / 36.0;
                        [x + radius * a.cos(), y + radius * a.sin(), z]
                    })
                    .collect()
            };
            let angle: f64 = if slot { 0.002 } else { 0.0 };
            let turn = |[x, y]: [f64; 2]| {
                let (s, c) = angle.sin_cos();
                [
                    bore[0] + c * (x - bore[0]) - s * (y - bore[1]),
                    bore[1] + s * (x - bore[0]) + c * (y - bore[1]),
                ]
            };
            let measured: Vec<MeasuredFeature> = (0..CONTROLS)
                .map(|i| {
                    let [x, y] = hole_center(i % (HOLES_PER_SIDE * HOLES_PER_SIDE));
                    let wobble = if i % 2 == 0 { 0.01 } else { -0.01 };
                    let offset = if slot {
                        [0.0, 0.05 + wobble]
                    } else {
                        [0.05, wobble]
                    };
                    MeasuredFeature {
                        nominals: Vec::new(),
                        id: format!("position-{i}"),
                        points_mm: circle(turn([x + offset[0], y + offset[1]]), 3.0),
                    }
                })
                .collect();
            // Square slot walls 4 mm either side of the bore.
            let walls: Vec<[f64; 3]> = [-4.0, 4.0]
                .into_iter()
                .flat_map(|d| {
                    (0..=40).flat_map(move |k| [2.0, 8.0].map(move |z| (d, k as f64 - 20.0, z)))
                })
                .map(|(d, t, z)| [bore[0] + d, bore[1] + t, z])
                .collect();
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: None,
                drawing: "shifted".into(),
                surface_textures: Vec::new(),
                datum_features: vec![
                    MeasuredFeature {
                        nominals: Vec::new(),
                        id: "A".into(),
                        points_mm: grid(400, |u, v| [side * u, side * v, 0.0]),
                    },
                    MeasuredFeature {
                        nominals: Vec::new(),
                        id: "B".into(),
                        points_mm: circle(bore, 5.03),
                    },
                    MeasuredFeature {
                        nominals: Vec::new(),
                        id: "C".into(),
                        // A Ø6.06 dowel hole clears its MMB by 0.08 mm.
                        points_mm: if frame == ShiftFrame::Pin {
                            circle([bore[0], bore[1] + 25.0], 3.03)
                        } else if slot {
                            walls
                        } else {
                            grid(400, |u, v| [side * u, 0.0, 10.0 * v])
                        },
                    },
                ],
                controls: measured,
            };
            let points: usize = record
                .datum_features
                .iter()
                .chain(&record.controls)
                .map(|m| m.points_mm.len())
                .sum();
            let report = drawing.evaluate_inspection(&graph, &record)?;
            let (mut shift, mut rotation) = (0.0, 0.0);
            for control in &report.controls {
                let ControlResult::Evaluated(m) = &control.result else {
                    return Err(failure(format!(
                        "{} was not evaluated: {:?}",
                        control.control, control.result
                    )));
                };
                if !m.conforms || (m.datum_shift_limit_mm - 0.08).abs() > 1e-9 {
                    return Err(failure(format!("{} measured {m:?}", control.control)));
                }
                shift = m.datum_shift_mm;
                rotation = m.datum_shift_rotation_rad.abs();
            }
            if (shift - 0.05).abs() > 1e-3 || (rotation - angle).abs() > 1e-5 {
                return Err(failure(format!(
                    "shared shift {shift}, rotation {rotation}"
                )));
            }
            Ok(format!(
                "{} of {} conform with a shared {shift:.4} mm, {:.2} mrad shift, {} measured points",
                report.controls.len(),
                report.controls.len(),
                1e3 * rotation,
                points
            ))
        },
    )
}

/// A 200 mm square plate with outward datum planes and a 10 × 10 grid of
/// vertical hole axes `hole-{i}`.
fn hole_plate(definition: &FamilyDefinition) -> FamilyDefinition {
    let mut definition = definition.clone();
    definition.datums = vec![
        plane("bottom", [0.0; 3], [0.0, 0.0, -1.0]),
        plane("left", [0.0; 3], [-1.0, 0.0, 0.0]),
        plane("front", [0.0; 3], [0.0, -1.0, 0.0]),
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
    definition
}

/// A drawing of the hole plate with datum features A, B and C on its bottom,
/// left and front planes.
fn abc_drawing(id: &str, controls: Vec<DrawingFeatureControlFrame>) -> DrawingDefinition {
    DrawingDefinition {
        annotation_views: Vec::new(),
        dataset: None,
        approval_workflow: None,
        datum_reference_frames: Vec::new(),
        datum_features: [("A", "bottom"), ("B", "left"), ("C", "front")]
            .into_iter()
            .map(|(id, anchor)| DrawingDatumFeature {
                targets: Vec::new(),
                id: id.into(),
                label: id.into(),
                feature_of_size: false,
                attachment: attachment(anchor),
                size_kind: None,
                size_limits: None,
            })
            .collect(),
        feature_control_frames: controls,
        surface_textures: Vec::new(),
        parts_list: None,
        balloons: Vec::new(),
        releases: Vec::new(),
        revision_table: None,
        sheet: None,
        id: id.into(),
        title: id.into(),
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
            thread_representation: false,
        }],
    }
}

/// 10,000 composite position controls in 100 patterns of 100 holes: Ø0.2 to
/// A | B | C, refined to Ø0.04 relating each pattern (lower segment to A, so
/// patterns may translate and turn). Every pattern sits 0.03, 0.01 mm off and
/// turned 0.5 mrad; every tenth also has one hole 0.1 mm off its neighbours,
/// which fails that whole pattern's feature-relating fit. Decided under
/// guarded acceptance with U = 0.001 mm, which leaves those results alone.
/// With `three` segments, a Ø0.15 segment to A | B (the pattern may
/// translate but stays square to B) sits between the upper segment and the
/// Ø0.04 segment to A.
pub(crate) fn composite_case(definition: &FamilyDefinition, three: bool) -> Outcome {
    timed(
        format!(
            "inspection: {CONTROLS} {} composite position controls in 100 patterns, guarded acceptance",
            if three {
                "three-segment"
            } else {
                "two-segment"
            }
        ),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let definition = hole_plate(definition);
            let side = 20.0 * HOLES_PER_SIDE as f64;
            let holes = HOLES_PER_SIDE * HOLES_PER_SIDE;
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let reference = |id: &str| DrawingDatumReference {
                translation: false,
                datum_feature: id.into(),
                boundary: DatumMaterialBoundary::Regardless,
                common_with: Vec::new(),
            };
            let controls: Vec<DrawingFeatureControlFrame> = (0..CONTROLS)
                .map(|i| DrawingFeatureControlFrame {
                    modifiers: Vec::new(),
                    projected: None,
                    tangent_plane: false,
                    pattern: Some(format!("P{}", i / holes)),
                    unequal: None,
                    surface: None,
                    size_limits: Some(DrawingSizeLimits {
                        kind: FeatureOfSizeKind::Internal,
                        lower: Quantity::length(5.9, LengthUnit::Millimeter),
                        upper: Quantity::length(6.1, LengthUnit::Millimeter),
                    }),
                    datum_reference_frame: None,
                    refinement: Some({
                        let relating = DrawingCompositeRefinement {
                            tolerance: Quantity::length(0.04, LengthUnit::Millimeter),
                            datums: vec![reference("A")],
                            refinement: None,
                        };
                        if three {
                            DrawingCompositeRefinement {
                                tolerance: Quantity::length(0.15, LengthUnit::Millimeter),
                                datums: vec![reference("A"), reference("B")],
                                refinement: Some(Box::new(relating)),
                            }
                        } else {
                            relating
                        }
                    }),
                    id: format!("composite-{i}"),
                    attachment: attachment(&format!("hole-{}", i % holes)),
                    characteristic: GeometricCharacteristic::Position,
                    tolerance: Quantity::length(0.2, LengthUnit::Millimeter),
                    display_unit: LengthUnit::Millimeter,
                    precision: 2,
                    zone: GeometricToleranceZone::Diameter,
                    material: ToleranceMaterialCondition::Regardless,
                    feature_of_size: true,
                    datums: vec![reference("A"), reference("B"), reference("C")],
                })
                .collect();
            let drawing = abc_drawing("composite", controls);
            let center = [side / 2.0, side / 2.0];
            let (s, c) = 0.0005_f64.sin_cos();
            let measured: Vec<MeasuredFeature> = (0..CONTROLS)
                .map(|i| {
                    let [x, y] = hole_center(i % holes);
                    let (dx, dy) = (x - center[0], y - center[1]);
                    let outlier = (i / holes).is_multiple_of(10) && i % holes == 0;
                    let p = [
                        center[0] + c * dx - s * dy + 0.03 + if outlier { 0.1 } else { 0.0 },
                        center[1] + s * dx + c * dy + 0.01,
                    ];
                    MeasuredFeature {
                        nominals: Vec::new(),
                        id: format!("composite-{i}"),
                        points_mm: (0..36)
                            .flat_map(|k| [2.0, 8.0].map(|z| (k, z)))
                            .map(|(k, z)| {
                                let a = k as f64 * std::f64::consts::TAU / 36.0;
                                [p[0] + 3.0 * a.cos(), p[1] + 3.0 * a.sin(), z]
                            })
                            .collect(),
                    }
                })
                .collect();
            let datum = |id: &str, point: &dyn Fn(f64, f64) -> [f64; 3]| MeasuredFeature {
                nominals: Vec::new(),
                id: id.into(),
                points_mm: grid(400, point),
            };
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: Some(MeasurementUncertainty {
                    expanded_mm: 0.001,
                    rule: DecisionRule::GuardedAcceptance,
                    controls: Default::default(),
                    simulation: None,
                }),
                drawing: "composite".into(),
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
                    return Err(failure(format!(
                        "{} was not evaluated: {:?}",
                        control.control, control.result
                    )));
                };
                let mut lower = m
                    .refinement
                    .as_ref()
                    .ok_or_else(|| failure(format!("{} has no refinement", control.control)))?;
                if three {
                    if lower.pattern_rotation_rad != 0.0 {
                        return Err(failure(format!("{} turned square to B", control.control)));
                    }
                    lower = lower.refinement.as_deref().ok_or_else(|| {
                        failure(format!("{} has no third segment", control.control))
                    })?;
                }
                let bad = (i / holes).is_multiple_of(10);
                if m.uncertainty_mm != 0.001
                    || m.conforms == bad
                    || (!bad
                        && (m.deviation_mm > 0.2
                            || (lower.pattern_rotation_rad.abs() - 0.0005).abs() > 2e-5))
                {
                    return Err(failure(format!("{} measured {m:?}", control.control)));
                }
                conforming += usize::from(m.conforms);
            }
            Ok(format!(
                "{conforming} of {} conform; 10 patterns with an outlier fail their relating fit",
                report.controls.len()
            ))
        },
    )
}

/// Bore points about a measured center `p` (36 around at two heights) with
/// their nominal Ø6 surface points about `nominal` and normals toward the
/// axis (out of the material).
fn bore_profile(id: String, nominal: [f64; 2], p: [f64; 2]) -> MeasuredFeature {
    let (points_mm, nominals) = (0..36)
        .flat_map(|k| [2.0, 8.0].map(|z| (k, z)))
        .map(|(k, z)| {
            let (s, c) = (k as f64 * std::f64::consts::TAU / 36.0).sin_cos();
            (
                [p[0] + 3.0 * c, p[1] + 3.0 * s, z],
                MeasuredNominal {
                    point_mm: [nominal[0] + 3.0 * c, nominal[1] + 3.0 * s, z],
                    normal: [-c, -s, 0.0],
                },
            )
        })
        .unzip();
    MeasuredFeature {
        id,
        points_mm,
        nominals,
    }
}

/// 10,000 composite surface profile controls on bores (720,000 points with
/// nominals) in 100 patterns of 100: 0.4 to A | B | C, refined to 0.04 with
/// A orienting only, so each pattern's 7,200 points move as one rigid group.
/// Every pattern sits 0.03, 0.01 mm off and turned 0.5 mrad; every tenth
/// also has one bore 0.1 mm off its neighbours, failing its relating fit.
///
/// With `dynamic`, the upper segment's zone is dynamic (△), free to grow or
/// shrink with the bores.
pub(crate) fn profile_case(definition: &FamilyDefinition, dynamic: bool) -> Outcome {
    timed(
        format!(
            "inspection: {CONTROLS} composite surface profiles in 100 patterns{}",
            if dynamic { ", dynamic upper zones" } else { "" }
        ),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let definition = hole_plate(definition);
            let side = 20.0 * HOLES_PER_SIDE as f64;
            let holes = HOLES_PER_SIDE * HOLES_PER_SIDE;
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let reference = |id: &str| DrawingDatumReference {
                translation: false,
                datum_feature: id.into(),
                boundary: DatumMaterialBoundary::Regardless,
                common_with: Vec::new(),
            };
            let controls: Vec<DrawingFeatureControlFrame> = (0..CONTROLS)
                .map(|i| DrawingFeatureControlFrame {
                    modifiers: if dynamic {
                        vec![ToleranceModifier::DynamicProfile]
                    } else {
                        Vec::new()
                    },
                    projected: None,
                    tangent_plane: false,
                    pattern: Some(format!("P{}", i / holes)),
                    unequal: None,
                    surface: None,
                    size_limits: None,
                    datum_reference_frame: None,
                    refinement: Some(DrawingCompositeRefinement {
                        tolerance: Quantity::length(0.04, LengthUnit::Millimeter),
                        datums: vec![reference("A")],
                        refinement: None,
                    }),
                    id: format!("profile-{i}"),
                    attachment: attachment(&format!("hole-{}", i % holes)),
                    characteristic: GeometricCharacteristic::ProfileSurface,
                    tolerance: Quantity::length(0.4, LengthUnit::Millimeter),
                    display_unit: LengthUnit::Millimeter,
                    precision: 2,
                    zone: GeometricToleranceZone::Characteristic,
                    material: ToleranceMaterialCondition::Regardless,
                    feature_of_size: false,
                    datums: vec![reference("A"), reference("B"), reference("C")],
                })
                .collect();
            let drawing = abc_drawing("profile", controls);
            let center = [side / 2.0, side / 2.0];
            let (s, c) = 0.0005_f64.sin_cos();
            let measured: Vec<MeasuredFeature> = (0..CONTROLS)
                .map(|i| {
                    let [x, y] = hole_center(i % holes);
                    let (dx, dy) = (x - center[0], y - center[1]);
                    let outlier = (i / holes).is_multiple_of(10) && i % holes == 0;
                    let p = [
                        center[0] + c * dx - s * dy + 0.03 + if outlier { 0.1 } else { 0.0 },
                        center[1] + s * dx + c * dy + 0.01,
                    ];
                    bore_profile(format!("profile-{i}"), [x, y], p)
                })
                .collect();
            let datum = |id: &str, point: &dyn Fn(f64, f64) -> [f64; 3]| MeasuredFeature {
                nominals: Vec::new(),
                id: id.into(),
                points_mm: grid(400, point),
            };
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: None,
                drawing: "profile".into(),
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
                    return Err(failure(format!(
                        "{} was not evaluated: {:?}",
                        control.control, control.result
                    )));
                };
                let lower = m
                    .refinement
                    .as_ref()
                    .ok_or_else(|| failure(format!("{} has no refinement", control.control)))?;
                let bad = (i / holes).is_multiple_of(10);
                if m.conforms == bad
                    || m.deviation_mm > 0.4
                    || (!bad
                        && (lower.deviation_mm > 1e-3
                            || (lower.pattern_rotation_rad - 0.0005).abs() > 2e-5))
                {
                    return Err(failure(format!("{} measured {m:?}", control.control)));
                }
                conforming += usize::from(m.conforms);
            }
            Ok(format!(
                "{conforming} of {} conform; 10 patterns with an outlier fail their relating fit",
                report.controls.len()
            ))
        },
    )
}

/// One surface profile of 1,000,000 points with no datums: all six rigid
/// motions are fitted (active set) to a tilted, offset face with a ±0.01 mm
/// wave, leaving a 0.02 mm zone.
pub(crate) fn profile_surface_case(definition: &FamilyDefinition) -> Outcome {
    const POINTS: usize = 1_000_000;
    timed(
        format!("inspection: one surface profile of {POINTS} points, six free motions"),
        Duration::from_secs(2),
        Expectation::Required,
        || {
            let definition = hole_plate(definition);
            let side = 20.0 * HOLES_PER_SIDE as f64;
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let drawing = abc_drawing(
                "surface",
                vec![DrawingFeatureControlFrame {
                    modifiers: Vec::new(),
                    projected: None,
                    tangent_plane: false,
                    pattern: None,
                    unequal: None,
                    surface: None,
                    size_limits: None,
                    datum_reference_frame: None,
                    refinement: None,
                    id: "surface".into(),
                    attachment: attachment("bottom"),
                    characteristic: GeometricCharacteristic::ProfileSurface,
                    tolerance: Quantity::length(0.03, LengthUnit::Millimeter),
                    display_unit: LengthUnit::Millimeter,
                    precision: 2,
                    zone: GeometricToleranceZone::Characteristic,
                    material: ToleranceMaterialCondition::Regardless,
                    feature_of_size: false,
                    datums: Vec::new(),
                }],
            );
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: None,
                drawing: "surface".into(),
                surface_textures: Vec::new(),
                datum_features: Vec::new(),
                controls: vec![MeasuredFeature {
                    nominals: Vec::new(),
                    id: "surface".into(),
                    points_mm: grid(POINTS, |u, v| {
                        let wave = 0.01 * (u * 37.0).sin() * (v * 23.0).cos();
                        [side * u, side * v, 0.2 + 0.002 * side * u + wave]
                    }),
                }],
            };
            let report = drawing.evaluate_inspection(&graph, &record)?;
            let ControlResult::Evaluated(m) = &report.controls[0].result else {
                return Err(failure(format!("not evaluated: {:?}", report.controls[0])));
            };
            if !m.conforms || m.deviation_mm > 0.0201 || m.deviation_mm < 0.019 {
                return Err(failure(format!("measured {m:?}")));
            }
            Ok(format!("zone {:.5} mm", m.deviation_mm))
        },
    )
}

/// 1,000 surface profiles, each 1,000 measured points on its own band of a
/// Ø50 × 100 mm cylinder's side and recorded without nominals: the part is
/// regenerated once, each control selects the named side face, and all
/// 1,000,000 points are projected onto it for their nominal points and
/// outward normals. A ±0.005 mm lobing leaves a 0.01 mm zone.
pub(crate) fn profile_projection_case(definition: &FamilyDefinition) -> Outcome {
    const PROFILES: usize = 1_000;
    const POINTS: usize = 1_000;
    timed(
        format!(
            "inspection: {PROFILES} surface profiles projecting {} points onto the model",
            PROFILES * POINTS
        ),
        Duration::from_secs(2),
        Expectation::Required,
        || {
            let mut definition = hole_plate(definition);
            let mm = |v: f64| ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter));
            definition.features = vec![FeatureDefinition {
                id: "body".into(),
                operation: FeatureOperation::Cylinder {
                    origin: literal([0.0; 3]),
                    axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    radius: mm(25.0),
                    height: mm(100.0),
                },
            }];
            definition.requirements.clear();
            definition.references = vec![NamedReference {
                name: "side".into(),
                target: ReferenceTarget::Faces(FaceSelector::NearestCenter {
                    target: literal([0.0, 0.0, 50.0]),
                    maximum_distance: mm(1.0),
                }),
            }];
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let controls = (0..PROFILES)
                .map(|i| DrawingFeatureControlFrame {
                    modifiers: Vec::new(),
                    projected: None,
                    tangent_plane: false,
                    pattern: None,
                    unequal: None,
                    surface: Some(FaceSelector::Named("side".into())),
                    size_limits: None,
                    datum_reference_frame: None,
                    refinement: None,
                    id: format!("band-{i}"),
                    attachment: attachment("hole-0"),
                    characteristic: GeometricCharacteristic::ProfileSurface,
                    tolerance: Quantity::length(0.02, LengthUnit::Millimeter),
                    display_unit: LengthUnit::Millimeter,
                    precision: 2,
                    zone: GeometricToleranceZone::Characteristic,
                    material: ToleranceMaterialCondition::Regardless,
                    feature_of_size: false,
                    datums: Vec::new(),
                })
                .collect();
            let drawing = abc_drawing("bands", controls);
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: None,
                drawing: "bands".into(),
                surface_textures: Vec::new(),
                datum_features: Vec::new(),
                controls: (0..PROFILES)
                    .map(|i| MeasuredFeature {
                        id: format!("band-{i}"),
                        nominals: Vec::new(),
                        points_mm: (0..POINTS)
                            .map(|k| {
                                let a = k as f64 * std::f64::consts::TAU / 50.0;
                                let z = 0.05 + 0.1 * i as f64 + 0.002 * (k / 50) as f64;
                                let r = 25.0 + 0.005 * (5.0 * a).cos();
                                [r * a.cos(), r * a.sin(), z]
                            })
                            .collect(),
                    })
                    .collect(),
            };
            let session = Session::new()?;
            let report = drawing.evaluate_inspection_on_model(&graph, &session, &record)?;
            for control in &report.controls {
                let ControlResult::Evaluated(m) = &control.result else {
                    return Err(failure(format!(
                        "{} was not evaluated: {:?}",
                        control.control, control.result
                    )));
                };
                if !m.conforms || (m.deviation_mm - 0.01).abs() > 2e-4 {
                    return Err(failure(format!("{} measured {m:?}", control.control)));
                }
            }
            Ok(format!(
                "{} profiles conform with 0.01 mm zones",
                report.controls.len()
            ))
        },
    )
}

/// One profile of a line on a 200 × 10 mm side face seen from the top view:
/// 1,000,000 points in 1,000 line elements (rows), each bowed out as a whole
/// by up to 0.01 mm and waved ±0.002 mm along its length. Each element is
/// fitted alone within its section (active set per element), so the bow
/// fits away and a 0.004 mm zone remains.
pub(crate) fn profile_line_case(definition: &FamilyDefinition) -> Outcome {
    const ROWS: usize = 1_000;
    const PER_ROW: usize = 1_000;
    timed(
        format!(
            "inspection: one line profile of {} points in {ROWS} elements",
            ROWS * PER_ROW
        ),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let definition = hole_plate(definition);
            let side = 20.0 * HOLES_PER_SIDE as f64;
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let drawing = abc_drawing(
                "rows",
                vec![DrawingFeatureControlFrame {
                    modifiers: Vec::new(),
                    projected: None,
                    tangent_plane: false,
                    pattern: None,
                    unequal: None,
                    surface: None,
                    size_limits: None,
                    datum_reference_frame: None,
                    refinement: None,
                    id: "rows".into(),
                    attachment: attachment("left"),
                    characteristic: GeometricCharacteristic::ProfileLine,
                    tolerance: Quantity::length(0.005, LengthUnit::Millimeter),
                    display_unit: LengthUnit::Millimeter,
                    precision: 3,
                    zone: GeometricToleranceZone::Characteristic,
                    material: ToleranceMaterialCondition::Regardless,
                    feature_of_size: false,
                    datums: Vec::new(),
                }],
            );
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: None,
                drawing: "rows".into(),
                surface_textures: Vec::new(),
                datum_features: Vec::new(),
                controls: vec![MeasuredFeature {
                    nominals: Vec::new(),
                    id: "rows".into(),
                    points_mm: (0..ROWS * PER_ROW)
                        .map(|i| {
                            let (row, k) = (i / PER_ROW, i % PER_ROW);
                            let y = side * k as f64 / PER_ROW as f64;
                            let bow = 0.01 * (row as f64 * 0.37).sin();
                            let wave = 0.002 * (y * 0.37).sin();
                            [-(bow + wave), y, 10.0 * row as f64 / ROWS as f64]
                        })
                        .collect(),
                }],
            };
            let report = drawing.evaluate_inspection(&graph, &record)?;
            let ControlResult::Evaluated(m) = &report.controls[0].result else {
                return Err(failure(format!("not evaluated: {:?}", report.controls[0])));
            };
            let elements = m.profile.as_ref().map_or(0, |p| p.elements);
            if !m.conforms || elements != ROWS || m.deviation_mm > 0.0041 || m.deviation_mm < 0.0039
            {
                return Err(failure(format!("measured {m:?}")));
            }
            Ok(format!(
                "zone {:.5} mm over {elements} elements",
                m.deviation_mm
            ))
        },
    )
}

/// 10,000 position controls on bands of one long bore, all coaxial with a
/// primary datum bore A (Ø9.9-10.1, measured Ø10.06: each end of its axis
/// may float 0.08 mm). Every band sits 0.05 mm off in x (±0.01 mm in y):
/// the shared shift of the datum axis brings all of them inside Ø0.04.
/// Coaxial controls to a primary datum bore A at MMB, or with `common` to
/// the common axis A-E of two bores either side of them.
pub(crate) fn coaxial_shift_case(definition: &FamilyDefinition, common: bool) -> Outcome {
    timed(
        if common {
            format!(
                "inspection: {CONTROLS} coaxial position controls sharing a common-axis MMB shift"
            )
        } else {
            format!(
                "inspection: {CONTROLS} coaxial position controls sharing a primary-axis MMB shift"
            )
        },
        Duration::from_millis(500),
        Expectation::Required,
        || {
            let mut definition = definition.clone();
            definition.datums = vec![DatumDefinition {
                id: "bore".into(),
                kind: DatumKind::Axis {
                    origin: literal([0.0; 3]),
                    direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                },
            }];
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let limits = |lower, upper| DrawingSizeLimits {
                kind: FeatureOfSizeKind::Internal,
                lower: Quantity::length(lower, LengthUnit::Millimeter),
                upper: Quantity::length(upper, LengthUnit::Millimeter),
            };
            let controls: Vec<DrawingFeatureControlFrame> = (0..CONTROLS)
                .map(|i| DrawingFeatureControlFrame {
                    modifiers: Vec::new(),
                    projected: None,
                    tangent_plane: false,
                    pattern: None,
                    unequal: None,
                    surface: None,
                    size_limits: Some(limits(15.9, 16.1)),
                    datum_reference_frame: None,
                    refinement: None,
                    id: format!("coaxial-{i}"),
                    attachment: attachment("bore"),
                    characteristic: GeometricCharacteristic::Position,
                    tolerance: Quantity::length(0.04, LengthUnit::Millimeter),
                    display_unit: LengthUnit::Millimeter,
                    precision: 2,
                    zone: GeometricToleranceZone::Diameter,
                    material: ToleranceMaterialCondition::Regardless,
                    feature_of_size: true,
                    datums: vec![DrawingDatumReference {
                        translation: false,
                        datum_feature: "A".into(),
                        boundary: DatumMaterialBoundary::Maximum,
                        common_with: if common { vec!["E".into()] } else { Vec::new() },
                    }],
                })
                .collect();
            let drawing = DrawingDefinition {
                annotation_views: Vec::new(),
                dataset: None,
                approval_workflow: None,
                datum_reference_frames: Vec::new(),
                datum_features: ["A", "E"]
                    .map(|id| DrawingDatumFeature {
                        targets: Vec::new(),
                        id: id.into(),
                        label: id.into(),
                        feature_of_size: true,
                        attachment: attachment("bore"),
                        size_kind: None,
                        size_limits: Some(limits(9.9, 10.1)),
                    })
                    .to_vec(),
                feature_control_frames: controls,
                surface_textures: Vec::new(),
                parts_list: None,
                balloons: Vec::new(),
                releases: Vec::new(),
                revision_table: None,
                sheet: None,
                id: "coaxial".into(),
                title: "Coaxial".into(),
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
                    thread_representation: false,
                }],
            };
            // Rings of 36 points of `radius` about (x, y) at each height.
            let rings = |[x, y]: [f64; 2], radius: f64, heights: &[f64]| -> Vec<[f64; 3]> {
                heights
                    .iter()
                    .flat_map(|&z| {
                        (0..36).map(move |k| {
                            let a = k as f64 * std::f64::consts::TAU / 36.0;
                            [x + radius * a.cos(), y + radius * a.sin(), z]
                        })
                    })
                    .collect()
            };
            let datum_heights: Vec<f64> = (0..=50).map(|k| k as f64).collect();
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: None,
                drawing: "coaxial".into(),
                surface_textures: Vec::new(),
                datum_features: [("A", 0.0), ("E", 200.0)]
                    .into_iter()
                    .take(if common { 2 } else { 1 })
                    .map(|(id, base)| MeasuredFeature {
                        nominals: Vec::new(),
                        id: id.into(),
                        points_mm: rings(
                            [0.0, 0.0],
                            5.03,
                            &datum_heights.iter().map(|z| z + base).collect::<Vec<_>>(),
                        ),
                    })
                    .collect(),
                controls: (0..CONTROLS)
                    .map(|i| {
                        let wobble = if i % 2 == 0 { 0.01 } else { -0.01 };
                        let z = 60.0 + (i % 100) as f64;
                        MeasuredFeature {
                            nominals: Vec::new(),
                            id: format!("coaxial-{i}"),
                            points_mm: rings([0.05, wobble], 8.0, &[z, z + 0.8]),
                        }
                    })
                    .collect(),
            };
            let points: usize = record
                .datum_features
                .iter()
                .chain(&record.controls)
                .map(|m| m.points_mm.len())
                .sum();
            let report = drawing.evaluate_inspection(&graph, &record)?;
            let mut shift = 0.0;
            for control in &report.controls {
                let ControlResult::Evaluated(m) = &control.result else {
                    return Err(failure(format!(
                        "{} was not evaluated: {:?}",
                        control.control, control.result
                    )));
                };
                if !m.conforms || (m.datum_shift_limit_mm - 0.08).abs() > 1e-9 {
                    return Err(failure(format!("{} measured {m:?}", control.control)));
                }
                shift = m.datum_shift_mm;
            }
            if (shift - 0.05).abs() > 1e-3 {
                return Err(failure(format!("shared shift {shift}")));
            }
            Ok(format!(
                "{} of {} conform with a shared {shift:.4} mm datum axis shift, {} measured points",
                report.controls.len(),
                report.controls.len(),
                points
            ))
        },
    )
}

/// 10,000 radial holes square to a datum bore at MMB, stopped along it by a
/// face and free to turn about it: every hole turned 1 mrad and half raised,
/// half lowered 0.01 mm, sharing one turn of the frame.
pub(crate) fn radial_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        format!(
            "inspection: {CONTROLS} radial position controls sharing a primary-axis frame turn"
        ),
        // Each hole's two envelope fits relinearize their 1 mrad turn.
        Duration::from_secs(1),
        Expectation::Required,
        || {
            // 100 nominal holes, 25 around at each of 4 heights, as the
            // other cases share 100 hole datums.
            const HOLES: usize = 100;
            let angle = |h: usize| std::f64::consts::TAU * (h % 25) as f64 / 25.0;
            let height = |h: usize| 5.0 + 10.0 * (h / 25) as f64;
            let mut definition = definition.clone();
            definition.datums = vec![
                DatumDefinition {
                    id: "bore".into(),
                    kind: DatumKind::Axis {
                        origin: literal([0.0; 3]),
                        direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    },
                },
                plane("stop", [0.0; 3], [0.0, 0.0, -1.0]),
            ];
            definition
                .datums
                .extend((0..HOLES).map(|h| DatumDefinition {
                    id: format!("radial-{h}"),
                    kind: DatumKind::Axis {
                        origin: literal([0.0, 0.0, height(h)]),
                        direction: VectorExpr::Literal(VectorQuantity::scalars(
                            angle(h).cos(),
                            angle(h).sin(),
                            0.0,
                        )),
                    },
                }));
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let limits = |lower, upper| DrawingSizeLimits {
                kind: FeatureOfSizeKind::Internal,
                lower: Quantity::length(lower, LengthUnit::Millimeter),
                upper: Quantity::length(upper, LengthUnit::Millimeter),
            };
            let controls: Vec<DrawingFeatureControlFrame> = (0..CONTROLS)
                .map(|i| DrawingFeatureControlFrame {
                    modifiers: Vec::new(),
                    projected: None,
                    tangent_plane: false,
                    pattern: None,
                    unequal: None,
                    surface: None,
                    size_limits: Some(limits(2.9, 3.1)),
                    datum_reference_frame: None,
                    refinement: None,
                    id: format!("radial-{i}"),
                    attachment: attachment(&format!("radial-{}", i % HOLES)),
                    characteristic: GeometricCharacteristic::Position,
                    tolerance: Quantity::length(0.05, LengthUnit::Millimeter),
                    display_unit: LengthUnit::Millimeter,
                    precision: 2,
                    zone: GeometricToleranceZone::Diameter,
                    material: ToleranceMaterialCondition::Regardless,
                    feature_of_size: true,
                    datums: vec![
                        DrawingDatumReference {
                            translation: false,
                            datum_feature: "A".into(),
                            boundary: DatumMaterialBoundary::Maximum,
                            common_with: Vec::new(),
                        },
                        DrawingDatumReference {
                            translation: false,
                            datum_feature: "B".into(),
                            boundary: DatumMaterialBoundary::Regardless,
                            common_with: Vec::new(),
                        },
                    ],
                })
                .collect();
            let drawing = DrawingDefinition {
                annotation_views: Vec::new(),
                dataset: None,
                approval_workflow: None,
                datum_reference_frames: Vec::new(),
                datum_features: vec![
                    DrawingDatumFeature {
                        targets: Vec::new(),
                        id: "A".into(),
                        label: "A".into(),
                        feature_of_size: true,
                        attachment: attachment("bore"),
                        size_kind: None,
                        size_limits: Some(limits(9.9, 10.1)),
                    },
                    DrawingDatumFeature {
                        targets: Vec::new(),
                        id: "B".into(),
                        label: "B".into(),
                        feature_of_size: false,
                        attachment: attachment("stop"),
                        size_kind: None,
                        size_limits: None,
                    },
                ],
                feature_control_frames: controls,
                surface_textures: Vec::new(),
                parts_list: None,
                balloons: Vec::new(),
                releases: Vec::new(),
                revision_table: None,
                sheet: None,
                id: "radial".into(),
                title: "Radial".into(),
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
                    thread_representation: false,
                }],
            };
            // Rings of 36 points of `radius` about the line through `base`
            // along `axis` (square to z), at each distance along it.
            let rings = |base: [f64; 3], axis: [f64; 3], radius: f64, along: &[f64]| {
                let side = [-axis[1], axis[0], 0.0];
                along
                    .iter()
                    .flat_map(|&s| {
                        (0..36).map(move |k| {
                            let (sin, cos) = (k as f64 * std::f64::consts::TAU / 36.0).sin_cos();
                            [0, 1, 2].map(|j| {
                                base[j]
                                    + s * axis[j]
                                    + radius * (cos * side[j] + sin * [0.0, 0.0, 1.0][j])
                            })
                        })
                    })
                    .collect::<Vec<_>>()
            };
            let datum_heights: Vec<f64> = (0..=50).map(|k| k as f64).collect();
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                threads: Vec::new(),
                uncertainty: None,
                drawing: "radial".into(),
                surface_textures: Vec::new(),
                datum_features: vec![
                    MeasuredFeature {
                        nominals: Vec::new(),
                        id: "A".into(),
                        points_mm: datum_heights
                            .iter()
                            .flat_map(|&z| {
                                (0..36).map(move |k| {
                                    let (sin, cos) =
                                        (k as f64 * std::f64::consts::TAU / 36.0).sin_cos();
                                    [5.03 * cos, 5.03 * sin, z]
                                })
                            })
                            .collect(),
                    },
                    MeasuredFeature {
                        nominals: Vec::new(),
                        id: "B".into(),
                        points_mm: grid(100, |u, v| [40.0 * u - 20.0, 40.0 * v - 20.0, 0.0]),
                    },
                ],
                controls: (0..CONTROLS)
                    .map(|i| {
                        let h = i % HOLES;
                        let (sin, cos) = (angle(h) + 0.001).sin_cos();
                        let lift = if (i / HOLES).is_multiple_of(2) {
                            0.01
                        } else {
                            -0.01
                        };
                        MeasuredFeature {
                            nominals: Vec::new(),
                            id: format!("radial-{i}"),
                            points_mm: rings(
                                [0.0, 0.0, height(h) + lift],
                                [cos, sin, 0.0],
                                1.5,
                                &[10.0, 20.0],
                            ),
                        }
                    })
                    .collect(),
            };
            let points: usize = record
                .datum_features
                .iter()
                .chain(&record.controls)
                .map(|m| m.points_mm.len())
                .sum();
            let report = drawing.evaluate_inspection(&graph, &record)?;
            let mut turn = 0.0;
            for control in &report.controls {
                let ControlResult::Evaluated(m) = &control.result else {
                    return Err(failure(format!(
                        "{} was not evaluated: {:?}",
                        control.control, control.result
                    )));
                };
                if !m.conforms || (m.deviation_mm - 0.02).abs() > 1e-3 {
                    return Err(failure(format!("{} measured {m:?}", control.control)));
                }
                turn = m.datum_shift_rotation_rad;
            }
            if (turn - 0.001).abs() > 1e-5 {
                return Err(failure(format!("shared turn {turn}")));
            }
            Ok(format!(
                "{} of {} conform with a shared {:.2} mrad frame turn, {} measured points",
                report.controls.len(),
                report.controls.len(),
                turn * 1e3,
                points
            ))
        },
    )
}

/// 10,000 measured 1/4-20 UNC-2B tapped holes: 100 per instance on 100
/// instances, every tenth with an oversize pitch diameter, judged under
/// guarded acceptance.
pub(crate) fn thread_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        "inspection: 10000 measured Unified tapped holes on 100 instances".to_owned(),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            const PER_PART: usize = 100;
            let mut definition = hole_plate(definition);
            let mm =
                |value: f64| ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter));
            definition
                .features
                .extend((0..PER_PART).map(|h| FeatureDefinition {
                    id: format!("tapped-{h}"),
                    operation: FeatureOperation::Hole {
                        input: "body".into(),
                        position: literal([1.0 + h as f64, 1.0, 1.0]),
                        axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
                        diameter: mm(0.201 * 25.4),
                        extent: HoleExtent::ThroughAll,
                        bottom: HoleBottom::Flat,
                        finish: HoleFinish::Plain,
                        thread: Some(Box::new(ThreadSpecification {
                            designation: "1/4-20 UNC-2B".into(),
                            nominal_diameter: mm(6.35),
                            pitch: mm(1.27),
                            handedness: ThreadHandedness::Right,
                            modeled: None,
                            unified: Some(UnifiedThreadSpec {
                                series: UnifiedSeries::Unc,
                                class: UnifiedClass::TwoB,
                                rounded_root: false,
                                length_of_engagement: None,
                                strength: None,
                            }),
                        })),
                    },
                }));
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            for i in 1..PER_PART {
                graph.add_base(format!("part-{i}"), HashMap::new(), "bench")?;
            }
            let drawing = abc_drawing("threads", Vec::new());
            let threads = (0..PER_PART * PER_PART)
                .map(|t| MeasuredThread {
                    instance: match t / PER_PART {
                        0 => "part".into(),
                        i => format!("part-{i}"),
                    },
                    feature: format!("tapped-{}", t % PER_PART),
                    pitch_diameter_mm: if t % 10 == 9 { 0.2230 } else { 0.2200 } * 25.4,
                    major_diameter_mm: Some(0.2510 * 25.4),
                    minor_diameter_mm: Some(0.201 * 25.4),
                })
                .collect();
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                drawing: "threads".into(),
                datum_features: Vec::new(),
                controls: Vec::new(),
                surface_textures: Vec::new(),
                threads,
                uncertainty: Some(MeasurementUncertainty {
                    expanded_mm: 0.002,
                    rule: DecisionRule::GuardedAcceptance,
                    controls: Default::default(),
                    simulation: None,
                }),
            };
            let report = drawing.evaluate_inspection(&graph, &record)?;
            let failing = report.threads.iter().filter(|t| !t.conforms).count();
            if report.threads.len() != PER_PART * PER_PART || failing != PER_PART * PER_PART / 10 {
                return Err(failure(format!(
                    "{} threads, {failing} failing",
                    report.threads.len()
                )));
            }
            Ok(format!(
                "{} threads judged, {failing} oversize",
                report.threads.len()
            ))
        },
    )
}
