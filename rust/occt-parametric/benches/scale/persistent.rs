//! Persistent references followed through long feature chains.

use super::*;
use occt_parametric::{EdgeSelector, HoleExtent, HoleFinish};

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
