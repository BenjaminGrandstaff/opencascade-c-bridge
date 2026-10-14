//! Source-linked refinement operates on freshly imported homogeneous poles.
use super::*;
use occt_bridge::{BSplineBasis, BSplineRefinementAction};

pub(super) fn refine(
    session: &Session,
    projection: &SketchProjection,
    parameters: &HashMap<String, ParameterValue>,
    mut poles: Vec<[f64; 2]>,
    mut basis: SketchBSplineBasis,
) -> Result<(Vec<[f64; 2]>, SketchBSplineBasis), ModelError> {
    for (index, operation) in projection.refinements.iter().enumerate() {
        let apply = |poles: &mut Vec<[f64; 2]>,
                     basis: &mut SketchBSplineBasis,
                     action|
         -> Result<(), ModelError> {
            let input = BSplineBasis {
                degree: basis.degree,
                periodic: basis.periodic,
                pole_count: poles.len(),
                knots: &basis.knots,
                multiplicities: &basis.multiplicities,
            };
            let transform = session.refine_bspline(&input, action)?;
            let mut weights = basis.weights(parameters)?;
            if weights.is_empty() {
                weights = vec![1.; poles.len()];
            }
            let scale = weights.iter().copied().fold(0_f64, f64::max);
            let mut refined = Vec::with_capacity(transform.coefficients.len());
            let mut refined_weights = Vec::with_capacity(transform.coefficients.len());
            for row in transform.coefficients {
                let scaled = row
                    .iter()
                    .zip(&weights)
                    .map(|(c, w)| c * (w / scale))
                    .collect::<Vec<_>>();
                let sum = scaled.iter().sum::<f64>();
                let weight = sum * scale;
                let mut point = [0., 0.];
                for (p, w) in poles.iter().zip(scaled) {
                    for axis in 0..2 {
                        point[axis] += (w / sum) * p[axis];
                    }
                }
                if !weight.is_finite() || weight <= 0. || point.iter().any(|v| !v.is_finite()) {
                    return Err(ModelError::new(
                        "refined projection needs finite poles and positive weights",
                    ));
                }
                refined.push(point);
                refined_weights.push(ScalarExpr::Literal(Quantity::scalar(weight)));
            }
            *poles = refined;
            basis.degree = transform.degree;
            basis.knots = transform.knots;
            basis.multiplicities = transform.multiplicities;
            basis.weights = refined_weights;
            Ok(())
        };
        let result = (|| {
            match operation {
                SketchProjectionRefinement::ElevateDegree { degree } if *degree > basis.degree => {
                    apply(
                        &mut poles,
                        &mut basis,
                        BSplineRefinementAction::ElevateDegree(*degree),
                    )?
                }
                SketchProjectionRefinement::InsertKnot {
                    fraction,
                    multiplicity,
                } => {
                    if *multiplicity > basis.degree {
                        return Err(ModelError::new(
                            "projection knot multiplicity exceeds its current degree; elevate degree first",
                        ));
                    }
                    let first = basis.knots[0];
                    let last = *basis.knots.last().unwrap();
                    let knot = first + (last - first) * fraction;
                    for _ in 0..*multiplicity {
                        let current = basis
                            .knots
                            .iter()
                            .position(|k| {
                                (k - knot).abs() <= 64. * f64::EPSILON * (last - first).max(1.)
                            })
                            .map_or(0, |i| basis.multiplicities[i]);
                        if current >= *multiplicity {
                            break;
                        }
                        apply(
                            &mut poles,
                            &mut basis,
                            BSplineRefinementAction::InsertKnot(knot),
                        )?;
                    }
                }
                _ => {}
            }
            Ok(())
        })();
        result.map_err(|e: ModelError| {
            e.context(&format!(
                "sketch projection '{}' refinement {}",
                projection.id,
                index + 1
            ))
        })?;
    }
    Ok((poles, basis))
}
