//! Smooth airfoil lofts at the wing tool's 200-station limit.

use super::*;
use occt_parametric::{GeneratedResult, LoftSection};

const STATIONS: usize = 200;

fn naca_0012() -> Vec<[f64; 2]> {
    let thickness = |x: f64| {
        0.6 * (0.2969 * x.sqrt() - 0.126 * x - 0.3516 * x * x + 0.2843 * x.powi(3)
            - 0.1036 * x.powi(4))
    };
    let x = |i: usize| (1.0 + (std::f64::consts::PI * i as f64 / 40.0).cos()) / 2.0;
    (0..=40)
        .map(|i| [x(i), thickness(x(i))])
        .chain((1..40).rev().map(|i| [x(i), -thickness(x(i))]))
        .collect()
}

pub(crate) fn loft_case() -> Outcome {
    timed(
        format!("smooth airfoil loft: {STATIONS} stations x 80 points, build and chord edit"),
        ms(5_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let profile = naca_0012();
            let mut definition = block();
            definition.datums.clear();
            definition.requirements.clear();
            definition.parameters = vec![ParameterDefinition {
                id: "root".into(),
                parameter_type: ParameterType::Scalar(Dimension::Length),
                default: length(300.0),
                minimum: None,
                maximum: None,
            }];
            let sections = (0..STATIONS)
                .map(|index| {
                    let fraction = index as f64 / (STATIONS - 1) as f64;
                    // Chord tapers from the `root` parameter to 40% of it.
                    LoftSection {
                        profile: profile.clone(),
                        origin: VectorExpr::Literal(VectorQuantity::lengths(
                            200.0 * fraction,
                            1_000.0 * fraction,
                            0.0,
                            LengthUnit::Millimeter,
                        )),
                        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                        scale: ScalarExpr::Multiply(
                            Box::new(ScalarExpr::Literal(Quantity::scalar(1.0 - 0.6 * fraction))),
                            Box::new(ScalarExpr::Parameter("root".into())),
                        ),
                        rotation_radians: Some(ScalarExpr::Literal(Quantity::scalar(
                            -3f64.to_radians() * fraction,
                        ))),
                        pivot: [0.25, 0.0],
                    }
                })
                .collect();
            definition.features = vec![FeatureDefinition {
                id: "wing".into(),
                operation: FeatureOperation::Loft {
                    sections,
                    smooth: true,
                    ruled: true,
                },
            }];
            let mut part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            part.overrides.insert("root".into(), length(330.0));
            let second = part.regenerate_incremental(&session, &first)?;
            let volume = |result: &GeneratedResult<'_>| -> Result<f64, ModelError> {
                let shape = result
                    .shape("wing")
                    .ok_or_else(|| failure("wing missing".into()))?;
                if !session.is_valid(shape)? {
                    return Err(failure("wing is invalid".into()));
                }
                Ok(session.volume(shape)?)
            };
            // Every section scales by 1.1, so the volume scales by 1.1 squared.
            let ratio = volume(&second)? / volume(&first)?;
            if (ratio - 1.21).abs() > 1e-6 {
                return Err(failure(format!("chord edit scaled volume by {ratio}")));
            }
            drop((first, second));
            if session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?
                != 0
            {
                return Err(failure("loft handles retained".into()));
            }
            Ok(format!(
                "valid smooth loft; chord edit scaled volume by {ratio:.6}"
            ))
        },
    )
}
