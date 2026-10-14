//! Saved directional point links evaluated from the current rational basis.
use super::*;

impl SketchConstraint {
    pub fn dependent_point(&self) -> Option<&str> {
        match self {
            Self::WeightedPoint { point, .. } | Self::SplinePoint { point, .. } => Some(point),
            _ => None,
        }
    }
}
impl SketchDefinition {
    pub fn point_link_sources<'a>(
        &'a self,
        constraint: &'a SketchConstraint,
    ) -> Result<Vec<&'a str>, ModelError> {
        match constraint {
            SketchConstraint::WeightedPoint { terms, .. } => {
                Ok(terms.iter().map(|t| t.point.as_str()).collect())
            }
            SketchConstraint::SplinePoint { spline, point, .. } => {
                let source = self
                    .splines
                    .iter()
                    .find(|s| s.id == *spline)
                    .ok_or_else(|| {
                        ModelError::new(format!("unknown spline point source '{spline}'"))
                    })?;
                source.basis.as_ref().ok_or_else(|| {
                    ModelError::new("spline point links require an explicit B-spline")
                })?;
                if source.points.contains(point) {
                    return Err(ModelError::new(
                        "spline point target cannot be a source control pole",
                    ));
                }
                Ok(source.points.iter().map(String::as_str).collect())
            }
            _ => Ok(Vec::new()),
        }
    }
    /// Positive homogeneous contributions. Zero basis functions are omitted.
    pub fn point_link_terms<'a>(
        &'a self,
        constraint: &'a SketchConstraint,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Vec<(&'a str, f64)>, ModelError> {
        match constraint {
            SketchConstraint::WeightedPoint { terms, .. } => terms
                .iter()
                .map(|t| {
                    Ok((
                        t.point.as_str(),
                        scalar(&t.weight, parameters, Dimension::Scalar)?,
                    ))
                })
                .collect(),
            SketchConstraint::SplinePoint {
                spline, fraction, ..
            } => {
                self.point_link_sources(constraint)?;
                let source = self.splines.iter().find(|s| s.id == *spline).unwrap();
                validation::validate_spline_points(
                    source,
                    &source
                        .points
                        .iter()
                        .map(String::as_str)
                        .chain(
                            source
                                .endpoints
                                .iter()
                                .flat_map(|e| [e.start.as_str(), e.end.as_str()]),
                        )
                        .collect(),
                )?;
                let b = source.basis.as_ref().unwrap();
                let fraction = scalar(fraction, parameters, Dimension::Scalar)?;
                if !(0.0..=1.0).contains(&fraction) {
                    return Err(ModelError::new("spline point fraction must be in [0, 1]"));
                }
                // Structural validation bounds degree, knot expansion and pole count.
                let p = b.degree as usize;
                let mut knots = b
                    .knots
                    .iter()
                    .zip(&b.multiplicities)
                    .flat_map(|(&k, &m)| std::iter::repeat_n(k, m as usize))
                    .collect::<Vec<_>>();
                let n = source.points.len();
                if b.periodic {
                    // OCCT extends the repeated sequence by degree+1-seam multiplicity
                    // knots on either side and repeats the first control poles.
                    let seam = b.multiplicities[0] as usize;
                    let extension = p + 1 - seam;
                    let period = b.knots.last().unwrap() - b.knots[0];
                    let prefix = knots[n - extension..n].iter().map(|k| k - period);
                    let suffix = knots[seam..seam + extension].iter().map(|k| k + period);
                    knots = prefix.chain(knots.iter().copied()).chain(suffix).collect();
                    if knots.iter().any(|k| !k.is_finite()) {
                        return Err(ModelError::new(
                            "periodic spline point knot extension must be finite",
                        ));
                    }
                }
                let effective_count = knots.len() - p - 1;
                let (start, end) = (knots[p], knots[effective_count]);
                let u = if b.periodic && fraction == 1.0 {
                    start
                } else if fraction == 1.0 {
                    end
                } else {
                    start + fraction * (end - start)
                };
                let span = if u >= end {
                    effective_count - 1
                } else {
                    knots
                        .partition_point(|k| *k <= u)
                        .saturating_sub(1)
                        .clamp(p, effective_count - 1)
                };
                let mut basis = vec![0.; p + 1];
                let mut left = vec![0.; p + 1];
                let mut right = vec![0.; p + 1];
                basis[0] = 1.;
                for j in 1..=p {
                    left[j] = u - knots[span + 1 - j];
                    right[j] = knots[span + j] - u;
                    let mut saved = 0.;
                    for r in 0..j {
                        let denominator = right[r + 1] + left[j - r];
                        let term = if denominator > 0. {
                            basis[r] / denominator
                        } else {
                            0.
                        };
                        basis[r] = saved + right[r + 1] * term;
                        saved = left[j - r] * term;
                    }
                    basis[j] = saved;
                }
                let weights = if b.weights.is_empty() {
                    vec![1.; n]
                } else {
                    b.weights(parameters)?
                };
                let scale = basis
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| **c > 0.)
                    .map(|(j, _)| weights[(span - p + j) % n])
                    .fold(0_f64, f64::max);
                let terms = basis
                    .into_iter()
                    .enumerate()
                    .filter_map(|(j, c)| {
                        let i = (span - p + j) % n;
                        let weight = c * (weights[i] / scale);
                        (weight > 0.).then_some((source.points[i].as_str(), weight))
                    })
                    .collect::<Vec<_>>();
                if terms.is_empty() || terms.iter().any(|(_, w)| !w.is_finite()) {
                    return Err(ModelError::new(
                        "spline point basis must produce finite positive contributions",
                    ));
                }
                Ok(terms)
            }
            _ => Err(ModelError::new(
                "constraint is not a directional point link",
            )),
        }
    }
}
