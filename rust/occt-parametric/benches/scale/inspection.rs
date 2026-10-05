//! Measured GD&T inspection at drawing scale: one shared datum reference
//! frame, 10,000 position controls with measured holes, and a dense surface.

use super::*;
use occt_parametric::{
    ControlResult, DatumMaterialBoundary, DrawingDatumFeature, DrawingDatumReference,
    DrawingDatumReferenceFrame, DrawingDefinition, DrawingFeatureControlFrame,
    DrawingGdtAttachment, DrawingView, DrawingViewKind, FeatureSizeLimits, GeometricCharacteristic,
    GeometricToleranceZone, InspectionRecord, InstanceOutputRef, MeasuredFeature,
    ToleranceMaterialCondition,
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
                    size_limits: Some(FeatureSizeLimits {
                        lower: Quantity::length(5.9, LengthUnit::Millimeter),
                        upper: Quantity::length(6.1, LengthUnit::Millimeter),
                        internal: true,
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
                    })
                    .collect(),
                feature_control_frames: controls,
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
