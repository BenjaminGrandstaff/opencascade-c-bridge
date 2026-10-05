//! Mathematical expression functions at parameter-table scale.

use super::*;
use occt_parametric::{DerivedParameterDefinition, RoundingMode};

/// 10,000 chained derived lengths, each rounding an interpolation toward a
/// hypotenuse by a cosine fraction, driving one box: evaluation is linear.
pub(crate) fn function_chain_case() -> Outcome {
    const LINKS: usize = 10_000;
    timed(
        format!("{LINKS} chained derived parameters with math functions"),
        ms(250),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let parameter = |id: &str| Box::new(ScalarExpr::Parameter(id.into()));
            let mut definition = block();
            definition.datums.clear();
            definition.requirements.clear();
            let target = ScalarExpr::Hypotenuse(parameter("width"), parameter("height"));
            for index in 0..LINKS {
                let previous = if index == 0 {
                    parameter("width")
                } else {
                    parameter(&format!("link-{}", index - 1))
                };
                definition
                    .derived_parameters
                    .push(DerivedParameterDefinition {
                        id: format!("link-{index}"),
                        dimension: Dimension::Length,
                        expression: ScalarExpr::RoundToStep {
                            value: Box::new(ScalarExpr::Interpolate {
                                from: previous,
                                to: Box::new(target.clone()),
                                fraction: Box::new(ScalarExpr::Cosine(Box::new(
                                    ScalarExpr::Literal(Quantity::scalar(1.4)),
                                ))),
                            }),
                            step: Box::new(ScalarExpr::Literal(Quantity::length(
                                1e-6,
                                LengthUnit::Millimeter,
                            ))),
                            mode: RoundingMode::Nearest,
                        },
                    });
            }
            let last = format!("link-{}", LINKS - 1);
            for feature in &mut definition.features {
                if let FeatureOperation::Box { size, .. } = &mut feature.operation {
                    *size = VectorExpr::Components {
                        x: ScalarExpr::Parameter(last.clone()),
                        y: ScalarExpr::Parameter("depth".into()),
                        z: ScalarExpr::Parameter("height".into()),
                    };
                }
            }
            let part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let generated = part.regenerate(&session)?;
            Ok(format!(
                "{LINKS} links resolved; {} features generated",
                generated.named_outputs().count()
            ))
        },
    )
}
