//! Persistent references followed through long feature chains.

use super::*;
use occt_parametric::{EdgeSelector, HoleExtent, HoleFinish, NamedReference, ReferenceTarget};

/// A block's four top edges, named on the bare block, followed through 100
/// sequential holes to a chamfer. On the final shape the same "top edges"
/// rule would also catch every hole rim.
pub(crate) fn persistent_chain_case() -> Outcome {
    const HOLES: usize = 100;
    timed(
        format!("persistent edges through {HOLES} sequential holes to a chamfer"),
        ms(10_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let at = |x: f64, y: f64, z: f64| {
                VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
            };
            let mut definition = block();
            definition.datums.clear();
            definition.requirements.clear();
            definition.parameters.clear();
            definition.features = vec![FeatureDefinition {
                id: "plate".into(),
                operation: FeatureOperation::Box {
                    origin: at(0.0, 0.0, 0.0),
                    size: at(210.0, 210.0, 10.0),
                },
            }];
            for index in 0..HOLES {
                let (row, column) = ((index / 10) as f64, (index % 10) as f64);
                definition.features.push(FeatureDefinition {
                    id: format!("hole-{index}"),
                    operation: FeatureOperation::Hole {
                        bottom: HoleBottom::Flat,
                        input: if index == 0 {
                            "plate".into()
                        } else {
                            format!("hole-{}", index - 1)
                        },
                        position: at(15.0 + 20.0 * column, 15.0 + 20.0 * row, 10.0),
                        axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
                        diameter: ScalarExpr::Literal(Quantity::length(
                            6.0,
                            LengthUnit::Millimeter,
                        )),
                        extent: HoleExtent::ThroughAll,
                        finish: HoleFinish::Plain,
                        thread: None,
                    },
                });
            }
            let top_edges = EdgeSelector::AtExtreme {
                axis: CoordinateAxis::Z,
                extremum: Extremum::Maximum,
                tolerance: ScalarExpr::Literal(Quantity::length(1e-6, LengthUnit::Millimeter)),
            };
            definition.features.push(FeatureDefinition {
                id: "eased".into(),
                operation: FeatureOperation::Chamfer {
                    input: format!("hole-{}", HOLES - 1),
                    edges: vec![EdgeSelector::Persistent {
                        feature: "plate".into(),
                        select: Box::new(top_edges),
                    }],
                    distance: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
                },
            });
            let part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let generated = part.regenerate(&session)?;
            let drilled = session.volume(
                generated
                    .shape(&format!("hole-{}", HOLES - 1))
                    .ok_or_else(|| failure("drilled plate missing".into()))?,
            )?;
            let eased = session.volume(
                generated
                    .shape("eased")
                    .ok_or_else(|| failure("chamfer missing".into()))?,
            )?;
            // Four 210 mm edges lose a 1 x 1 mm right triangle each, less the
            // corner overlaps; only the outer edges were chamfered.
            let removed = drilled - eased;
            if !(4.0 * 0.5 * 205.0 < removed && removed < 4.0 * 0.5 * 210.0) {
                return Err(failure(format!("chamfer removed {removed} mm³")));
            }
            Ok(format!(
                "4 named edges followed through {HOLES} holes; chamfer removed {removed:.2} mm³"
            ))
        },
    )
}

/// 200 boxes each filleted through its own named persistent reference, among
/// 10,000 declared references, then regenerated again with nothing changed.
/// Reference lookups are indexed, so declarations that no feature uses cost
/// nothing beyond validation.
pub(crate) fn named_reference_case() -> Outcome {
    const PARTS: usize = 200;
    const DECLARED: usize = 10_000;
    timed(
        format!("{PARTS} fillets through named references among {DECLARED} declared"),
        ms(2_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let at = |x: f64, y: f64, z: f64| {
                VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
            };
            let mut definition = block();
            definition.datums.clear();
            definition.requirements.clear();
            definition.parameters.clear();
            definition.features.clear();
            for index in 0..DECLARED {
                let part = index % PARTS;
                definition.references.push(NamedReference {
                    name: format!("top-{index}"),
                    target: ReferenceTarget::Edges(EdgeSelector::Persistent {
                        feature: format!("box-{part}"),
                        select: Box::new(EdgeSelector::AtExtreme {
                            axis: CoordinateAxis::Z,
                            extremum: Extremum::Maximum,
                            tolerance: ScalarExpr::Literal(Quantity::length(
                                1e-6,
                                LengthUnit::Millimeter,
                            )),
                        }),
                    }),
                });
            }
            for index in 0..PARTS {
                definition.features.push(FeatureDefinition {
                    id: format!("box-{index}"),
                    operation: FeatureOperation::Box {
                        origin: at(20.0 * index as f64, 0.0, 0.0),
                        size: at(10.0, 10.0, 10.0),
                    },
                });
                definition.features.push(FeatureDefinition {
                    id: format!("eased-{index}"),
                    operation: FeatureOperation::Fillet {
                        input: format!("box-{index}"),
                        edges: vec![EdgeSelector::Named(format!(
                            "top-{}",
                            DECLARED - PARTS + index
                        ))],
                        radius: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
                    },
                });
            }
            let part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            let second = part.regenerate_incremental(&session, &first)?;
            if !second.regeneration.rebuilt.is_empty() {
                return Err(failure(format!(
                    "unchanged regeneration rebuilt {} features",
                    second.regeneration.rebuilt.len()
                )));
            }
            let eased = session.volume(
                second
                    .shape(&format!("eased-{}", PARTS - 1))
                    .ok_or_else(|| failure("fillet missing".into()))?,
            )?;
            // Four 10 mm edges lose (1 - pi/4) mm² of section each, less the
            // corner blends.
            let removed = 1000.0 - eased;
            let edge = 1.0 - std::f64::consts::FRAC_PI_4;
            if !(4.0 * 8.0 * edge < removed && removed < 4.0 * 10.0 * edge) {
                return Err(failure(format!("fillet removed {removed} mm³")));
            }
            Ok(format!(
                "{PARTS} fillets resolved by name; unchanged regeneration reused all {}",
                second.regeneration.reused.len()
            ))
        },
    )
}

/// 100 sequential holes in a plate, regenerated with the plate and every
/// hole colored and without colors: carrying colors is one face lookup per
/// input plus history queries for replaced faces at each feature.
pub(crate) fn feature_colors_case() -> Outcome {
    const HOLES: usize = 100;
    let chain = |colored: bool| -> Result<(std::time::Duration, usize), ModelError> {
        let session = Session::new().map_err(|error| failure(error.to_string()))?;
        let at = |x: f64, y: f64, z: f64| {
            VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
        };
        let mut definition = block();
        definition.datums.clear();
        definition.requirements.clear();
        definition.parameters.clear();
        definition.features = vec![FeatureDefinition {
            id: "plate".into(),
            operation: FeatureOperation::Box {
                origin: at(0.0, 0.0, 0.0),
                size: at(210.0, 210.0, 10.0),
            },
        }];
        for index in 0..HOLES {
            let (row, column) = ((index / 10) as f64, (index % 10) as f64);
            definition.features.push(FeatureDefinition {
                id: format!("hole-{index}"),
                operation: FeatureOperation::Hole {
                    bottom: HoleBottom::Flat,
                    input: if index == 0 {
                        "plate".into()
                    } else {
                        format!("hole-{}", index - 1)
                    },
                    position: at(15.0 + 20.0 * column, 15.0 + 20.0 * row, 10.0),
                    axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
                    diameter: ScalarExpr::Literal(Quantity::length(6.0, LengthUnit::Millimeter)),
                    extent: HoleExtent::ThroughAll,
                    finish: HoleFinish::Plain,
                    thread: None,
                },
            });
            if colored {
                definition
                    .feature_colors
                    .insert(format!("hole-{index}"), [0.0, 0.0, 1.0]);
            }
        }
        if colored {
            definition
                .feature_colors
                .insert("plate".into(), [1.0, 0.0, 0.0]);
        }
        let part = PartInstance {
            id: "part".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "bench".into(),
        };
        let start = Instant::now();
        let generated = part.regenerate(&session)?;
        let elapsed = start.elapsed();
        let last = format!("hole-{}", HOLES - 1);
        let bores = generated
            .face_colors(&last)
            .iter()
            .filter(|(_, color)| *color == [0.0, 0.0, 1.0])
            .count();
        Ok((elapsed, bores))
    };
    timed(
        format!("feature colors through {HOLES} sequential holes"),
        ms(15_000),
        Expectation::Required,
        || {
            let (plain, _) = chain(false)?;
            let (painted, bores) = chain(true)?;
            if bores != HOLES {
                return Err(failure(format!("{bores} blue bores, expected {HOLES}")));
            }
            let ratio = painted.as_secs_f64() / plain.as_secs_f64().max(1e-9);
            if ratio > 1.5 {
                return Err(failure(format!("colors cost {ratio:.2}x")));
            }
            Ok(format!(
                "{HOLES} blue bores on a red plate; {:.3} s vs {:.3} s uncolored ({ratio:.2}x)",
                painted.as_secs_f64(),
                plain.as_secs_f64()
            ))
        },
    )
}
