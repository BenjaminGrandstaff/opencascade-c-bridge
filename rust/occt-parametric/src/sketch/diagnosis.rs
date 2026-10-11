//! Bounded sparse analysis of the final Jacobian. This describes local freedom
//! and linearized equation dependencies, not global nonlinear inconsistency.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

const TOLERANCE: f64 = 1e-8;
const WORK_LIMIT: usize = 2_000_000;
const FILL_LIMIT: usize = 200_000;
const TRACE_LIMIT: usize = 64;
const GROUP_LIMIT: usize = 128;
const MODE_LIMIT: usize = 16;
const POINT_LIMIT: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SketchEquationSource {
    Constraint { index: usize },
    ArcRadii { entity: String },
    EllipseAxes { entity: String },
    SplineEndpoints { entity: String },
    PatternLink { pattern: String, point: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchCoordinateAxis {
    X,
    Y,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SketchFreeCoordinate {
    pub point: String,
    pub axis: SketchCoordinateAxis,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SketchPointMotion {
    pub point: String,
    /// Relative direction, not a finite edit or an additional constraint.
    pub dx: f64,
    pub dy: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SketchFreeMotion {
    pub coordinate: SketchFreeCoordinate,
    pub points: Vec<SketchPointMotion>,
    pub omitted_points: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SketchConflictGroup {
    pub sources: Vec<SketchEquationSource>,
    /// `fixed_geometry` is an actual residual with no free-coordinate influence;
    /// `linearized_dependency` is evidence at this iterate, not a global proof.
    pub evidence: String,
    pub normalized_residual: f64,
    pub trace_complete: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SketchDiagnosis {
    pub complete: bool,
    pub limitation: Option<String>,
    pub local_rank: Option<usize>,
    pub rank_agrees_with_solver: bool,
    pub failed_constraints: Vec<usize>,
    pub locally_redundant_constraints: Vec<usize>,
    pub fixed_satisfied_constraints: Vec<usize>,
    pub conflict_groups: Vec<SketchConflictGroup>,
    pub omitted_conflict_groups: usize,
    /// Independent coordinate choices for the local nullspace, not axes which
    /// can necessarily move alone. `free_motions` explains their coupling.
    pub free_coordinates: Vec<SketchFreeCoordinate>,
    pub free_motions: Vec<SketchFreeMotion>,
    pub omitted_free_motions: usize,
    pub work_units: usize,
}
struct BasisRow {
    pivot: usize,
    coefficients: BTreeMap<usize, f64>,
    residual: f64,
    trace: Option<BTreeMap<usize, f64>>,
}

pub(super) fn analyze(
    jacobian: &SparseJacobian,
    residuals: &[f64],
    sources: &[SketchEquationSource],
    structural: &[bool],
    coordinates: &[SketchFreeCoordinate],
    solver_rank: usize,
) -> SketchDiagnosis {
    let mut report = SketchDiagnosis {
        complete: true,
        limitation: None,
        local_rank: None,
        rank_agrees_with_solver: false,
        failed_constraints: Vec::new(),
        locally_redundant_constraints: Vec::new(),
        fixed_satisfied_constraints: Vec::new(),
        conflict_groups: Vec::new(),
        omitted_conflict_groups: 0,
        free_coordinates: Vec::new(),
        free_motions: Vec::new(),
        omitted_free_motions: 0,
        work_units: 0,
    };
    let mut counts = BTreeMap::<usize, (usize, usize, usize, bool, usize)>::new();
    let mut frequencies = vec![0usize; jacobian.columns];
    for (row_index, ((row, &residual), source)) in
        jacobian.rows.iter().zip(residuals).zip(sources).enumerate()
    {
        for &(column, _) in row {
            frequencies[column] += 1;
        }
        if let SketchEquationSource::Constraint { index } = source {
            let entry = counts.entry(*index).or_default();
            entry.0 += 1;
            entry.3 |= residual.abs() > RESIDUAL_TOLERANCE;
            if !structural[row_index] && residual.abs() <= RESIDUAL_TOLERANCE {
                entry.2 += 1;
            }
        }
    }
    report.failed_constraints = counts
        .iter()
        .filter_map(|(&i, c)| c.3.then_some(i))
        .collect();
    report.fixed_satisfied_constraints = counts
        .iter()
        .filter_map(|(&i, c)| (c.0 == c.2).then_some(i))
        .collect();
    let mut basis = Vec::<BasisRow>::new();
    let mut pivots: Vec<Option<usize>> = vec![None; jacobian.columns];
    let mut fill = 0usize;
    let mut groups = BTreeSet::new();
    let mut rank_complete = true;
    'rows: for (index, input) in jacobian.rows.iter().enumerate() {
        let scale = input
            .iter()
            .map(|(_, v)| v.abs())
            .fold(0., f64::max)
            .max(f64::MIN_POSITIVE);
        let mut row = input
            .iter()
            .map(|&(c, v)| (c, v / scale))
            .collect::<BTreeMap<_, _>>();
        let mut rhs = if input.is_empty() {
            residuals[index]
        } else {
            residuals[index] / scale
        };
        let mut trace = Some(BTreeMap::from([(index, 1.)]));
        if !rhs.is_finite() {
            report.complete = false;
            report.limitation = Some("nonfinite local normalization".into());
            rank_complete = false;
            break;
        }

        while let Some((&column, &factor)) = row.iter().find(|(c, _)| pivots[**c].is_some()) {
            let prior = &basis[pivots[column].unwrap()];
            report.work_units += row.len()
                + prior.coefficients.len()
                + prior.trace.as_ref().map_or(0, BTreeMap::len);
            if report.work_units > WORK_LIMIT {
                report.complete = false;
                report.limitation = Some("sparse analysis work limit reached".into());
                rank_complete = false;
                break 'rows;
            }
            for (&c, &v) in &prior.coefficients {
                let value = row.get(&c).copied().unwrap_or(0.) - factor * v;
                if value.abs() <= TOLERANCE {
                    row.remove(&c);
                } else {
                    row.insert(c, value);
                }
            }
            rhs -= factor * prior.residual;
            trace = match (trace, &prior.trace) {
                (Some(mut trace), Some(prior)) => {
                    for (&source, &v) in prior {
                        let value = trace.get(&source).copied().unwrap_or(0.) - factor * v;
                        if value.abs() <= TOLERANCE {
                            trace.remove(&source);
                        } else {
                            trace.insert(source, value);
                        }
                    }
                    (trace.len() <= TRACE_LIMIT).then_some(trace)
                }
                _ => None,
            };
            if !rhs.is_finite()
                || row
                    .values()
                    .chain(trace.iter().flat_map(|t| t.values()))
                    .any(|v| !v.is_finite())
            {
                report.complete = false;
                report.limitation = Some("nonfinite local elimination".into());
                rank_complete = false;
                break 'rows;
            }
        }
        row.retain(|_, v| v.abs() > TOLERANCE);
        if row.is_empty() {
            let threshold = if input.is_empty() {
                RESIDUAL_TOLERANCE
            } else {
                TOLERANCE
            };
            if rhs.abs() <= threshold
                && !input.is_empty()
                && let SketchEquationSource::Constraint { index } = sources[index]
            {
                counts.get_mut(&index).unwrap().4 += 1;
            }
            if rhs.abs() > threshold {
                let trace_complete = trace.is_some();
                let labels = trace
                    .as_ref()
                    .map(|trace| {
                        trace
                            .keys()
                            .map(|&i| sources[i].clone())
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| vec![sources[index].clone()]);
                if !trace_complete {
                    report.complete = false;
                    report.limitation =
                        Some("conflict dependency trace capped; source list is partial".into());
                }
                if groups.insert(labels.clone()) {
                    if report.conflict_groups.len() < GROUP_LIMIT {
                        report.conflict_groups.push(SketchConflictGroup {
                            sources: labels,
                            evidence: if !structural[index] {
                                "fixed_geometry"
                            } else if input.is_empty() {
                                "stationary_linearization"
                            } else {
                                "linearized_dependency"
                            }
                            .into(),
                            normalized_residual: rhs.abs(),
                            trace_complete,
                        });
                    } else {
                        report.omitted_conflict_groups += 1;
                    }
                }
            }
            continue;
        }
        // Prefer leaf coordinates (low incidence) over shared pattern/source
        // coordinates. This avoids quadratic chains in large linked patterns.
        let pivot = *row
            .keys()
            .min_by_key(|&&c| (frequencies[c], std::cmp::Reverse(c)))
            .unwrap();
        let divisor = row[&pivot];
        for v in row.values_mut() {
            *v /= divisor;
        }
        rhs /= divisor;
        for v in trace.iter_mut().flat_map(|t| t.values_mut()) {
            *v /= divisor;
        }
        if !rhs.is_finite()
            || row
                .values()
                .chain(trace.iter().flat_map(|t| t.values()))
                .any(|v| !v.is_finite())
        {
            report.complete = false;
            report.limitation = Some("nonfinite local normalization".into());
            rank_complete = false;
            break;
        }
        fill += row.len() + trace.as_ref().map_or(0, BTreeMap::len);
        report.work_units += row.len() + trace.as_ref().map_or(0, BTreeMap::len);
        if fill > FILL_LIMIT || report.work_units > WORK_LIMIT {
            report.complete = false;
            report.limitation = Some("sparse analysis storage/work limit reached".into());
            rank_complete = false;
            break;
        }
        if let SketchEquationSource::Constraint { index } = sources[index] {
            counts.get_mut(&index).unwrap().1 += 1;
        }
        pivots[pivot] = Some(basis.len());
        basis.push(BasisRow {
            pivot,
            coefficients: row,
            residual: rhs,
            trace,
        });
    }
    if !rank_complete {
        return report;
    }
    report.local_rank = Some(basis.len());
    report.rank_agrees_with_solver = basis.len() == solver_rank;
    report.locally_redundant_constraints = counts
        .iter()
        .filter_map(|(&i, c)| (c.1 == 0 && c.4 > 0 && c.0 == c.2 + c.4 && !c.3).then_some(i))
        .collect();
    let free = (0..jacobian.columns)
        .filter(|&c| pivots[c].is_none())
        .collect::<Vec<_>>();
    report.free_coordinates = free.iter().map(|&c| coordinates[c].clone()).collect();
    report.omitted_free_motions = free.len().saturating_sub(MODE_LIMIT);
    for &column in free.iter().take(MODE_LIMIT) {
        let mut direction = vec![0.; jacobian.columns];
        direction[column] = 1.;
        for row in basis.iter().rev() {
            direction[row.pivot] = -row
                .coefficients
                .iter()
                .filter(|(c, _)| **c != row.pivot)
                .map(|(&c, &v)| v * direction[c])
                .sum::<f64>();
        }
        let scale = direction.iter().map(|v| v.abs()).fold(0., f64::max);
        if !scale.is_finite() || scale == 0. {
            report.complete = false;
            report.limitation = Some("nonfinite free-motion direction".into());
            break;
        }
        let mut motions = Vec::new();
        let mut omitted = 0;
        // Include the chosen coordinate's point even if many linked copies move.
        let chosen = column / 2;
        for point in
            std::iter::once(chosen).chain((0..coordinates.len() / 2).filter(|&p| p != chosen))
        {
            let (dx, dy) = (
                direction[point * 2] / scale,
                direction[point * 2 + 1] / scale,
            );
            if point != chosen && dx.abs().max(dy.abs()) <= TOLERANCE {
                continue;
            }
            if motions.len() < POINT_LIMIT {
                motions.push(SketchPointMotion {
                    point: coordinates[point * 2].point.clone(),
                    dx,
                    dy,
                });
            } else {
                omitted += 1;
            }
        }
        report.free_motions.push(SketchFreeMotion {
            coordinate: coordinates[column].clone(),
            points: motions,
            omitted_points: omitted,
        });
    }
    report.omitted_free_motions = free.len().saturating_sub(report.free_motions.len());
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_limit_does_not_publish_a_partial_rank_as_complete() {
        let columns = 700;
        let jacobian = SparseJacobian {
            columns,
            rows: (0..columns)
                .map(|i| (i..columns).map(|c| (c, 1.)).collect())
                .collect(),
        };
        let coordinates = (0..columns)
            .map(|c| SketchFreeCoordinate {
                point: format!("p{}", c / 2),
                axis: if c % 2 == 0 {
                    SketchCoordinateAxis::X
                } else {
                    SketchCoordinateAxis::Y
                },
            })
            .collect::<Vec<_>>();
        let sources = (0..columns)
            .map(|index| SketchEquationSource::Constraint { index })
            .collect::<Vec<_>>();
        let report = analyze(
            &jacobian,
            &vec![0.; columns],
            &sources,
            &vec![true; columns],
            &coordinates,
            columns,
        );
        assert!(!report.complete);
        assert!(report.limitation.unwrap().contains("storage/work limit"));
        assert_eq!(report.local_rank, None);
        assert!(report.free_coordinates.is_empty() && report.free_motions.is_empty());
    }
}
