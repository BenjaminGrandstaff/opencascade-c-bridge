//! Placing free instances so that their assembly relationships hold.
//!
//! Each free instance contributes six unknowns, a rotation vector and a
//! translation of its local placement. The rotation turns the instance about
//! its own pivot, the centroid of its involved datums, so rotating does not
//! move it; rotating about the model origin would make rotation and
//! translation nearly interchangeable for distant parts and stall the solve. Every relationship that touches a
//! free instance becomes a smooth residual vector, and Levenberg–Marquardt
//! drives those residuals to zero from the current placements. Unknowns that
//! no relationship constrains have no gradient and keep their current values,
//! so an under-constrained instance moves as little as possible.

use super::*;
use crate::assembly::{add, cross, dot, length, scale, subtract, transform_point};

const MAX_ITERATIONS: usize = 200;
/// Convergence tolerance relative to the problem's length scale (the largest
/// translation unknown, characteristic length, or 1 mm); a fixed absolute
/// tolerance is unreachable in floating point for parts far from the origin.
const RELATIVE_TOLERANCE: f64 = 1e-12;
/// Initial Marquardt damping. Assembly problems are close to linear, so the
/// solver starts near Gauss–Newton and damps only after a rejected step;
/// heavier initial damping crawls along chains, whose weakest mode shrinks
/// like 1/n^2.
const INITIAL_DAMPING: f64 = 1e-9;
const DIFFERENCE_STEP: f64 = 1e-7;
const MAX_DAMPING: f64 = 1e12;
const RANK_TOLERANCE: f64 = 1e-8;
/// Tikhonov weight relative to the largest normal-equation diagonal.
const REGULARIZATION: f64 = 1e-9;
const UNKNOWNS_PER_INSTANCE: usize = 6;

/// Outcome of [`InstanceGraph::solve_placements`].
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementSolution {
    /// Every involved relationship is satisfied and the placements were applied.
    pub solved: bool,
    pub iterations: usize,
    /// Largest residual component at the best fit, in millimeters or radians.
    pub max_residual: f64,
    /// Placement freedoms no relationship constrains (six per free instance
    /// minus the Jacobian rank).
    pub free_degrees: usize,
    /// Equations implied by others at the best fit; nonzero with an unsolved
    /// result points at conflicting relationships.
    pub redundant_equations: usize,
    /// Checks of every involved relationship at the best fit.
    pub checks: Vec<RelationshipCheck>,
}

/// One relationship endpoint: a fixed datum in model coordinates, or a free
/// instance's datum in its own placement's local coordinates.
enum Endpoint {
    Fixed(ResolvedDatum),
    Free {
        instance: usize,
        local: ResolvedDatum,
        frames: Vec<NormalizedPlacement>,
    },
}

struct Term {
    kind: RelationKind,
    first: Endpoint,
    second: Endpoint,
}

/// Residual terms plus the pivot of each free instance (in its placement's
/// local coordinates) and the length that converts angular residuals to
/// millimeters so both kinds carry comparable weight.
struct Problem {
    terms: Vec<Term>,
    pivots: Vec<Vec3>,
    angular_scale: f64,
}

impl Problem {
    /// Pivots are the centroids of each free instance's involved local datum
    /// origins; the angular scale is the largest pivot-to-datum distance,
    /// at least 1 mm. O(terms).
    fn new(terms: Vec<Term>, free_count: usize) -> Self {
        let mut sums = vec![(Vec3::new(0.0, 0.0, 0.0), 0usize); free_count];
        for (instance, local) in terms.iter().flat_map(Term::free_endpoints) {
            sums[instance].0 = add(sums[instance].0, datum_origin(local));
            sums[instance].1 += 1;
        }
        let pivots = sums
            .iter()
            .map(|(sum, count)| scale(*sum, 1.0 / (*count).max(1) as f64))
            .collect::<Vec<_>>();
        let angular_scale = terms
            .iter()
            .flat_map(Term::free_endpoints)
            .map(|(instance, local)| length(subtract(datum_origin(local), pivots[instance])))
            .fold(1.0_f64, f64::max);
        Self {
            terms,
            pivots,
            angular_scale,
        }
    }
}

impl Term {
    fn free_endpoints(&self) -> impl Iterator<Item = (usize, ResolvedDatum)> + '_ {
        [&self.first, &self.second]
            .into_iter()
            .filter_map(|endpoint| match endpoint {
                Endpoint::Free {
                    instance, local, ..
                } => Some((*instance, *local)),
                Endpoint::Fixed(_) => None,
            })
    }
}

fn datum_origin(datum: ResolvedDatum) -> Vec3 {
    match datum {
        ResolvedDatum::Point { origin }
        | ResolvedDatum::Axis { origin, .. }
        | ResolvedDatum::Plane { origin, .. } => origin,
    }
}

impl<'definition> InstanceGraph<'definition> {
    /// Moves the `free` instances so that every relationship involving them
    /// holds, keeping all other instances fixed. The graph changes only when
    /// the solve satisfies every involved relationship; otherwise the report
    /// shows which relationships conflict at the best fit.
    pub fn solve_placements(&mut self, free: &[&str]) -> Result<PlacementSolution, ModelError> {
        self.validate_free_instances(free)?;
        let involved = self
            .assembly
            .relationships
            .iter()
            .filter(|relationship| {
                free.contains(&relationship.first.instance.as_str())
                    || free.contains(&relationship.second.instance.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        if involved.is_empty() {
            return Err(ModelError::new(
                "no relationship involves the free instances",
            ));
        }
        let terms = involved
            .iter()
            .map(|relationship| self.term(relationship, free))
            .collect::<Result<Vec<_>, _>>()?;
        let problem = Problem::new(terms, free.len());
        let start = free
            .iter()
            .zip(&problem.pivots)
            .map(|(instance, pivot)| self.placement_unknowns(instance, *pivot))
            .collect::<Result<Vec<_>, _>>()?
            .concat();

        let fit = closest_solution(&problem, start)?;
        let rank = matrix_rank(&fit.jacobian);
        let mut candidate = self.clone();
        for (index, (instance, pivot)) in free.iter().zip(&problem.pivots).enumerate() {
            let unknowns = &fit.unknowns[index * UNKNOWNS_PER_INSTANCE..][..UNKNOWNS_PER_INSTANCE];
            candidate.set_placement(instance, placement_from_unknowns(unknowns, *pivot))?;
        }
        let checks = involved
            .iter()
            .map(|relationship| candidate.check_relationship(relationship))
            .collect::<Result<Vec<_>, _>>()?;
        let solved = checks.iter().all(|check| check.satisfied);
        if solved {
            *self = candidate;
        }
        Ok(PlacementSolution {
            solved,
            iterations: fit.iterations,
            max_residual: max_abs(&fit.residuals),
            free_degrees: fit.unknowns.len() - rank,
            redundant_equations: fit.residuals.len().saturating_sub(rank),
            checks,
        })
    }

    fn validate_free_instances(&self, free: &[&str]) -> Result<(), ModelError> {
        if free.is_empty() {
            return Err(ModelError::new(
                "solving requires at least one free instance",
            ));
        }
        let mut seen = HashSet::new();
        for instance in free {
            if !seen.insert(*instance) {
                return Err(ModelError::new(format!(
                    "free instance '{instance}' is listed more than once"
                )));
            }
            if !self.nodes.contains_key(*instance) {
                return Err(ModelError::new(format!("unknown instance '{instance}'")));
            }
        }
        Ok(())
    }

    fn term(&self, relationship: &AssemblyRelationship, free: &[&str]) -> Result<Term, ModelError> {
        let context = |error: ModelError| {
            ModelError::new(format!(
                "relationship '{}': {}",
                relationship.id, error.message
            ))
        };
        Ok(Term {
            kind: relationship.kind,
            first: self.endpoint(&relationship.first, free).map_err(context)?,
            second: self.endpoint(&relationship.second, free).map_err(context)?,
        })
    }

    fn endpoint(&self, reference: &DatumRef, free: &[&str]) -> Result<Endpoint, ModelError> {
        let Some(instance) = free.iter().position(|id| *id == reference.instance) else {
            return self
                .datum(&reference.instance, &reference.datum)
                .map(Endpoint::Fixed);
        };
        let resolved = self.resolve_with_placement(&reference.instance)?;
        let definition = resolved.instance.definition;
        let declaration = definition
            .datums
            .iter()
            .find(|candidate| candidate.id == reference.datum)
            .ok_or_else(|| {
                ModelError::new(format!(
                    "unknown datum '{}' on instance '{}'",
                    reference.datum, reference.instance
                ))
            })?;
        let parameters = resolve_parameters(definition, &resolved.instance.overrides)?;
        Ok(Endpoint::Free {
            instance,
            local: declaration.kind.evaluate(&parameters)?,
            frames: resolved
                .frames
                .iter()
                .map(|frame| frame.normalized())
                .collect::<Result<_, _>>()?,
        })
    }

    /// The instance's placement as a rotation vector about `pivot` followed
    /// by a translation: `x -> R(x - pivot) + pivot + t` with `t = P(pivot) - pivot`.
    fn placement_unknowns(&self, instance: &str, pivot: Vec3) -> Result<Vec<f64>, ModelError> {
        let placement = self
            .node(instance)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{instance}'")))?
            .placement()
            .normalized()?;
        let rotation = match placement.rotation {
            Some((_, axis, angle)) => scale(axis, angle / length(axis)),
            None => Vec3::new(0.0, 0.0, 0.0),
        };
        let translation = subtract(transform_point(pivot, &placement), pivot);
        Ok(vec![
            rotation.x,
            rotation.y,
            rotation.z,
            translation.x,
            translation.y,
            translation.z,
        ])
    }
}

fn unknown_placement(unknowns: &[f64], pivot: Vec3) -> NormalizedPlacement {
    let rotation = Vec3::new(unknowns[0], unknowns[1], unknowns[2]);
    let angle = length(rotation);
    NormalizedPlacement {
        translation: Vec3::new(unknowns[3], unknowns[4], unknowns[5]),
        rotation: (angle > 0.0).then_some((pivot, rotation, angle)),
    }
}

fn placement_from_unknowns(unknowns: &[f64], pivot: Vec3) -> Placement {
    let rotation = Vec3::new(unknowns[0], unknowns[1], unknowns[2]);
    let angle = length(rotation);
    let origin = VectorQuantity::lengths(pivot.x, pivot.y, pivot.z, LengthUnit::Millimeter);
    Placement {
        translation: VectorQuantity::lengths(
            unknowns[3],
            unknowns[4],
            unknowns[5],
            LengthUnit::Millimeter,
        ),
        rotation: (angle > 0.0).then(|| AxisAngle {
            origin,
            axis: VectorQuantity::scalars(
                rotation.x / angle,
                rotation.y / angle,
                rotation.z / angle,
            ),
            angle_radians: angle,
        }),
    }
}

impl Endpoint {
    fn datum(&self, unknowns: &[f64], pivots: &[Vec3]) -> ResolvedDatum {
        match self {
            Self::Fixed(datum) => *datum,
            Self::Free {
                instance,
                local,
                frames,
            } => {
                let placement = unknown_placement(
                    &unknowns[instance * UNKNOWNS_PER_INSTANCE..][..UNKNOWNS_PER_INSTANCE],
                    pivots[*instance],
                );
                frames
                    .iter()
                    .fold(local.transformed(&placement), |datum, frame| {
                        datum.transformed(frame)
                    })
            }
        }
    }
}

// ---- residuals

fn residuals(problem: &Problem, unknowns: &[f64]) -> Vec<f64> {
    let mut values = Vec::new();
    for term in &problem.terms {
        term_residuals(
            term.kind,
            term.first.datum(unknowns, &problem.pivots),
            term.second.datum(unknowns, &problem.pivots),
            problem.angular_scale,
            &mut values,
        );
    }
    values
}

fn push(values: &mut Vec<f64>, vector: Vec3) {
    values.extend([vector.x, vector.y, vector.z]);
}

/// Smooth residual components that vanish exactly when the relationship
/// holds. Angular components are multiplied by `angular` (a length) so they
/// weigh like millimeters. Datum pairs were validated when the relationship
/// was added.
fn term_residuals(
    kind: RelationKind,
    first: ResolvedDatum,
    second: ResolvedDatum,
    angular: f64,
    values: &mut Vec<f64>,
) {
    match kind {
        RelationKind::Coincident => coincident_residuals(first, second, angular, values),
        RelationKind::Parallel => directional_residuals(first, second, false, angular, values),
        RelationKind::Perpendicular => {
            directional_residuals(first, second, true, angular, values);
        }
        RelationKind::Distance(target) => {
            let target = target.normalized().unwrap_or(0.0);
            distance_residuals(first, second, target, angular, values);
        }
    }
}

fn coincident_residuals(
    first: ResolvedDatum,
    second: ResolvedDatum,
    angular: f64,
    values: &mut Vec<f64>,
) {
    use ResolvedDatum::{Axis, Plane, Point};
    match (first, second) {
        (Point { origin: p }, Point { origin: q }) => push(values, subtract(p, q)),
        (Point { origin: p }, Axis { origin, direction })
        | (Axis { origin, direction }, Point { origin: p }) => {
            push(values, cross(subtract(p, origin), direction));
        }
        (Point { origin: p }, Plane { origin, normal })
        | (Plane { origin, normal }, Point { origin: p }) => {
            values.push(dot(subtract(p, origin), normal));
        }
        (
            Axis {
                origin: a,
                direction: d,
            },
            Axis {
                origin: b,
                direction: e,
            },
        ) => {
            push(values, scale(cross(d, e), angular));
            push(values, cross(subtract(b, a), d));
        }
        (
            Plane {
                origin: a,
                normal: n,
            },
            Plane {
                origin: b,
                normal: m,
            },
        ) => {
            push(values, scale(cross(n, m), angular));
            values.push(dot(subtract(b, a), n));
        }
        (
            Axis { origin, direction },
            Plane {
                origin: plane,
                normal,
            },
        )
        | (
            Plane {
                origin: plane,
                normal,
            },
            Axis { origin, direction },
        ) => {
            values.push(dot(subtract(origin, plane), normal));
            values.push(dot(direction, normal) * angular);
        }
    }
}

fn direction_of(datum: ResolvedDatum) -> (Vec3, bool) {
    match datum {
        ResolvedDatum::Axis { direction, .. } => (direction, false),
        ResolvedDatum::Plane { normal, .. } => (normal, true),
        ResolvedDatum::Point { .. } => (Vec3::new(0.0, 0.0, 0.0), false),
    }
}

fn directional_residuals(
    first: ResolvedDatum,
    second: ResolvedDatum,
    perpendicular: bool,
    angular: f64,
    values: &mut Vec<f64>,
) {
    let (u, first_is_plane) = direction_of(first);
    let (v, second_is_plane) = direction_of(second);
    if perpendicular != (first_is_plane != second_is_plane) {
        values.push(dot(u, v) * angular);
    } else {
        push(values, scale(cross(u, v), angular));
    }
}

fn distance_residuals(
    first: ResolvedDatum,
    second: ResolvedDatum,
    target: f64,
    angular: f64,
    values: &mut Vec<f64>,
) {
    use ResolvedDatum::{Axis, Plane, Point};
    match (first, second) {
        (Point { origin: p }, Point { origin: q }) if target == 0.0 => push(values, subtract(p, q)),
        (Point { origin: p }, Point { origin: q }) => values.push(length(subtract(p, q)) - target),
        (Point { origin: p }, Axis { origin, direction })
        | (Axis { origin, direction }, Point { origin: p }) => {
            values.push(length(cross(subtract(p, origin), direction)) - target);
        }
        (Point { origin: p }, Plane { origin, normal })
        | (Plane { origin, normal }, Point { origin: p }) => {
            values.push(dot(subtract(p, origin), normal).abs() - target);
        }
        (
            Axis {
                origin: a,
                direction: d,
            },
            Axis {
                origin: b,
                direction: e,
            },
        ) => {
            push(values, scale(cross(d, e), angular));
            values.push(length(cross(subtract(b, a), d)) - target);
        }
        (
            Plane {
                origin: a,
                normal: n,
            },
            Plane {
                origin: b,
                normal: m,
            },
        ) => {
            push(values, scale(cross(n, m), angular));
            values.push(dot(subtract(b, a), n).abs() - target);
        }
        (Axis { .. }, Plane { .. }) | (Plane { .. }, Axis { .. }) => {}
    }
}

// ---- Levenberg–Marquardt

struct Fit {
    unknowns: Vec<f64>,
    residuals: Vec<f64>,
    jacobian: Vec<Vec<f64>>,
    iterations: usize,
}

/// Upper bound on null-space restoration rounds after the first fit; each
/// round shrinks the remaining drift by about three orders of magnitude.
const RESTORATION_ROUNDS: usize = 8;

/// Solves, then repeatedly moves the solution back toward `start` along the
/// directions no relationship constrains and polishes it again. Solving
/// alone can drift along those directions on the way (a temporary tilt makes
/// a free slide matter), so restoration is what makes under-constrained
/// instances move as little as possible.
fn closest_solution(problem: &Problem, start: Vec<f64>) -> Result<Fit, ModelError> {
    let weights = unknown_weights(problem, start.len());
    let tolerance = problem_tolerance(problem, &start);
    let mut fit = least_squares(problem, start.clone())?;
    let mut previous = f64::INFINITY;
    for _ in 0..RESTORATION_ROUNDS {
        let offset = start
            .iter()
            .zip(&fit.unknowns)
            .map(|(initial, current)| initial - current)
            .collect::<Vec<_>>();
        let Some(restoring) = null_space_component(&fit.jacobian, &offset, &weights) else {
            break;
        };
        // Stop once restoration is negligible or no longer shrinking, which
        // happens at the floating-point floor.
        let size = max_abs(&restoring);
        if size <= tolerance || size >= 0.5 * previous {
            break;
        }
        previous = size;
        let iterations = fit.iterations;
        fit = least_squares(problem, add_step(&fit.unknowns, &restoring))?;
        fit.iterations += iterations;
    }
    Ok(fit)
}

fn problem_tolerance(problem: &Problem, unknowns: &[f64]) -> f64 {
    let largest_translation = unknowns
        .iter()
        .enumerate()
        .filter(|(index, _)| index % UNKNOWNS_PER_INSTANCE >= 3)
        .fold(0.0_f64, |largest, (_, value)| largest.max(value.abs()));
    RELATIVE_TOLERANCE * largest_translation.max(problem.angular_scale).max(1.0)
}

/// Rotation unknowns weigh the squared characteristic length so that every
/// move is measured in millimeters.
fn unknown_weights(problem: &Problem, count: usize) -> Vec<f64> {
    let rotation_weight = problem.angular_scale * problem.angular_scale;
    (0..count)
        .map(|index| {
            if index % UNKNOWNS_PER_INSTANCE < 3 {
                rotation_weight
            } else {
                1.0
            }
        })
        .collect()
}

/// Projection passes; each removes the regularization's row-space leak by a
/// further factor of about `eps / sigma^2`.
const PROJECTION_PASSES: usize = 3;

/// The part of `offset` the Jacobian does not see, in the weighted metric.
/// Each pass subtracts `y` minimizing `|J(y - r)|^2 + eps |y|_W^2` from the
/// remainder `r`, so `J r -> 0` while null-space components are untouched.
/// O(unknowns^3) per pass for the dense solve.
fn null_space_component(
    jacobian: &[Vec<f64>],
    offset: &[f64],
    weights: &[f64],
) -> Option<Vec<f64>> {
    let (normal, _) = normal_equations(jacobian, &vec![0.0; jacobian.len()]);
    let largest =
        (0..normal.len()).fold(0.0_f64, |largest, index| largest.max(normal[index][index]));
    let epsilon = REGULARIZATION * largest.max(1.0);
    let mut regularized = normal.clone();
    for (index, row) in regularized.iter_mut().enumerate() {
        row[index] += epsilon * weights[index];
    }
    let mut component = offset.to_vec();
    for _ in 0..PROJECTION_PASSES {
        let rhs = normal
            .iter()
            .map(|row| {
                row.iter()
                    .zip(&component)
                    .map(|(entry, value)| entry * value)
                    .sum()
            })
            .collect::<Vec<f64>>();
        let seen = solve_linear(regularized.clone(), rhs)?;
        for (value, seen) in component.iter_mut().zip(seen) {
            *value -= seen;
        }
    }
    Some(component)
}

fn least_squares(problem: &Problem, mut unknowns: Vec<f64>) -> Result<Fit, ModelError> {
    let mut values = residuals(problem, &unknowns);
    let mut cost = squared_norm(&values);
    let mut damping = INITIAL_DAMPING;
    let tolerance = problem_tolerance(problem, &unknowns);
    let mut iterations = 0;
    let weights = unknown_weights(problem, unknowns.len());
    while iterations < MAX_ITERATIONS && max_abs(&values) > tolerance {
        iterations += 1;
        let jacobian = numeric_jacobian(problem, &unknowns);
        let Some(step) = damped_step(&jacobian, &values, &weights, &mut damping, |step| {
            let trial = add_step(&unknowns, step);
            let trial_values = residuals(problem, &trial);
            (squared_norm(&trial_values) < cost).then_some((trial, trial_values))
        }) else {
            break;
        };
        (unknowns, values) = step;
        cost = squared_norm(&values);
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(ModelError::new("placement solve diverged"));
    }
    let jacobian = numeric_jacobian(problem, &unknowns);
    Ok(Fit {
        unknowns,
        residuals: values,
        jacobian,
        iterations,
    })
}

type Accepted = (Vec<f64>, Vec<f64>);

/// Increases damping until a step lowers the cost; `None` when damping
/// saturates without progress.
fn damped_step(
    jacobian: &[Vec<f64>],
    values: &[f64],
    weights: &[f64],
    damping: &mut f64,
    mut try_step: impl FnMut(&[f64]) -> Option<Accepted>,
) -> Option<Accepted> {
    let (normal, gradient) = normal_equations(jacobian, values);
    // A fixed Tikhonov term keeps unknowns that no relationship constrains at
    // their current values; Marquardt damping alone is zero along them, so
    // Jacobian noise would otherwise produce arbitrarily large free moves.
    let largest =
        (0..normal.len()).fold(0.0_f64, |largest, index| largest.max(normal[index][index]));
    let tikhonov = REGULARIZATION * largest.max(1.0);
    while *damping <= MAX_DAMPING {
        let mut damped = normal.clone();
        for (index, row) in damped.iter_mut().enumerate() {
            row[index] += *damping * normal[index][index] + tikhonov * weights[index];
        }
        let rhs = gradient.iter().map(|value| -value).collect::<Vec<_>>();
        if let Some(step) = solve_linear(damped, rhs)
            && let Some(accepted) = try_step(&step)
        {
            *damping = (*damping / 10.0).max(1e-12);
            return Some(accepted);
        }
        *damping *= 10.0;
    }
    None
}

fn numeric_jacobian(problem: &Problem, unknowns: &[f64]) -> Vec<Vec<f64>> {
    let columns = (0..unknowns.len())
        .map(|column| {
            // Relative steps keep rounding noise small for coordinates far
            // from the origin.
            let step = DIFFERENCE_STEP * unknowns[column].abs().max(1.0);
            let mut forward = unknowns.to_vec();
            let mut backward = unknowns.to_vec();
            forward[column] += step;
            backward[column] -= step;
            residuals(problem, &forward)
                .iter()
                .zip(residuals(problem, &backward))
                .map(|(ahead, behind)| (ahead - behind) / (2.0 * step))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let rows = columns.first().map_or(0, Vec::len);
    (0..rows)
        .map(|row| columns.iter().map(|column| column[row]).collect())
        .collect()
}

fn normal_equations(jacobian: &[Vec<f64>], values: &[f64]) -> (Vec<Vec<f64>>, Vec<f64>) {
    let columns = jacobian.first().map_or(0, Vec::len);
    let mut normal = vec![vec![0.0; columns]; columns];
    let mut gradient = vec![0.0; columns];
    for (row, value) in jacobian.iter().zip(values) {
        for i in 0..columns {
            gradient[i] += row[i] * value;
            for j in 0..columns {
                normal[i][j] += row[i] * row[j];
            }
        }
    }
    (normal, gradient)
}

/// Gaussian elimination with partial pivoting; `None` for a singular system.
fn solve_linear(mut matrix: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Option<Vec<f64>> {
    let size = rhs.len();
    for column in 0..size {
        let pivot = (column..size)
            .max_by(|&a, &b| matrix[a][column].abs().total_cmp(&matrix[b][column].abs()))?;
        if matrix[pivot][column].abs() <= f64::MIN_POSITIVE {
            return None;
        }
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        for row in column + 1..size {
            let factor = matrix[row][column] / matrix[column][column];
            subtract_row(&mut matrix, column, row, column, factor);
            rhs[row] -= factor * rhs[column];
        }
    }
    let mut solution = vec![0.0; size];
    for row in (0..size).rev() {
        let known = (row + 1..size)
            .map(|index| matrix[row][index] * solution[index])
            .sum::<f64>();
        solution[row] = (rhs[row] - known) / matrix[row][row];
    }
    solution
        .iter()
        .all(|value| value.is_finite())
        .then_some(solution)
}

/// Numerical rank by elimination with a tolerance relative to the largest entry.
fn matrix_rank(matrix: &[Vec<f64>]) -> usize {
    let mut rows = matrix.to_vec();
    let columns = rows.first().map_or(0, Vec::len);
    let largest = rows
        .iter()
        .flatten()
        .fold(0.0_f64, |largest, value| largest.max(value.abs()));
    let tolerance = RANK_TOLERANCE * largest.max(1.0);
    let mut rank = 0;
    for column in 0..columns {
        let Some(pivot) = (rank..rows.len())
            .max_by(|&a, &b| rows[a][column].abs().total_cmp(&rows[b][column].abs()))
        else {
            break;
        };
        if rows[pivot][column].abs() <= tolerance {
            continue;
        }
        rows.swap(rank, pivot);
        for row in rank + 1..rows.len() {
            let factor = rows[row][column] / rows[rank][column];
            subtract_row(&mut rows, rank, row, column, factor);
        }
        rank += 1;
    }
    rank
}

/// `rows[target] -= factor * rows[pivot]` from `column` onward, for `pivot < target`.
fn subtract_row(rows: &mut [Vec<f64>], pivot: usize, target: usize, column: usize, factor: f64) {
    let (upper, lower) = rows.split_at_mut(target);
    for (value, pivot_value) in lower[0][column..].iter_mut().zip(&upper[pivot][column..]) {
        *value -= factor * pivot_value;
    }
}

fn add_step(unknowns: &[f64], step: &[f64]) -> Vec<f64> {
    unknowns
        .iter()
        .zip(step)
        .map(|(value, delta)| value + delta)
        .collect()
}

fn squared_norm(values: &[f64]) -> f64 {
    values.iter().map(|value| value * value).sum()
}

fn max_abs(values: &[f64]) -> f64 {
    values
        .iter()
        .fold(0.0, |largest, value| largest.max(value.abs()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::tests::{assert_point, block, relationship, stacked, translated};
    use std::f64::consts::FRAC_PI_6;

    fn turned(angle: f64, x: f64, y: f64, z: f64) -> Placement {
        Placement {
            translation: VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter),
            rotation: Some(AxisAngle {
                origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                angle_radians: angle,
            }),
        }
    }

    /// Seats `b` on `a`: faces touch, axes align, and side faces stay parallel.
    fn seat(graph: &mut InstanceGraph<'_>, upper: &str, lower: &str) {
        for (id, kind, first, second) in [
            ("seated", RelationKind::Coincident, "top", "bottom"),
            ("aligned", RelationKind::Coincident, "axis", "axis"),
            ("square", RelationKind::Parallel, "right", "right"),
        ] {
            graph
                .add_relationship(relationship(
                    &format!("{upper}-{id}"),
                    kind,
                    (lower, first),
                    (upper, second),
                ))
                .unwrap();
        }
    }

    #[test]
    fn fully_constrained_instance_snaps_into_place() {
        let definition = block();
        let mut graph = stacked(&definition);
        graph
            .set_placement("b", turned(FRAC_PI_6, 40.0, -15.0, 70.0))
            .unwrap();
        seat(&mut graph, "b", "a");

        let solution = graph.solve_placements(&["b"]).unwrap();
        assert!(solution.solved, "{solution:?}");
        assert_eq!(solution.free_degrees, 0);
        assert!(solution.max_residual < 1e-9);
        assert_point(graph.datum("b", "top_center").unwrap(), (5.0, 10.0, 60.0));
        let ResolvedDatum::Plane { normal, .. } = graph.datum("b", "right").unwrap() else {
            panic!("right is a plane");
        };
        assert!(length(cross(normal, Vec3::new(1.0, 0.0, 0.0))) < 1e-9);
        assert!(
            graph
                .check_relationships()
                .unwrap()
                .iter()
                .all(|check| check.satisfied)
        );
    }

    #[test]
    fn under_constrained_freedoms_keep_their_current_values() {
        let definition = block();
        let mut graph = stacked(&definition);
        graph
            .set_placement("b", translated(40.0, -15.0, 70.0))
            .unwrap();
        graph
            .add_relationship(relationship(
                "seated",
                RelationKind::Coincident,
                ("a", "top"),
                ("b", "bottom"),
            ))
            .unwrap();

        let solution = graph.solve_placements(&["b"]).unwrap();
        assert!(solution.solved, "{solution:?}");
        // Sliding in X and Y and turning about Z remain free.
        assert_eq!(solution.free_degrees, 3);
        assert_point(graph.datum("b", "top_center").unwrap(), (45.0, -5.0, 60.0));
    }

    #[test]
    fn conflicting_relationships_leave_the_graph_unchanged() {
        let definition = block();
        let mut graph = stacked(&definition);
        let distance = |millimeters| {
            RelationKind::Distance(Quantity::length(millimeters, LengthUnit::Millimeter))
        };
        graph
            .add_relationship(relationship(
                "near",
                distance(30.0),
                ("a", "top"),
                ("b", "top"),
            ))
            .unwrap();
        graph
            .add_relationship(relationship(
                "far",
                distance(50.0),
                ("a", "top"),
                ("b", "top"),
            ))
            .unwrap();
        let before = graph.node("b").unwrap().placement();

        let solution = graph.solve_placements(&["b"]).unwrap();
        assert!(!solution.solved);
        assert!(solution.redundant_equations > 0);
        assert!(solution.checks.iter().any(|check| !check.satisfied));
        assert_eq!(graph.node("b").unwrap().placement(), before);
    }

    #[test]
    fn chains_of_free_instances_solve_together() {
        let definition = block();
        let mut graph = stacked(&definition);
        graph.add_clone("c", "a", HashMap::new(), "test").unwrap();
        graph
            .set_placement("b", turned(0.4, -30.0, 12.0, 5.0))
            .unwrap();
        graph
            .set_placement("c", turned(-0.7, 25.0, 40.0, -8.0))
            .unwrap();
        seat(&mut graph, "b", "a");
        seat(&mut graph, "c", "b");

        let solution = graph.solve_placements(&["b", "c"]).unwrap();
        assert!(solution.solved, "{solution:?}");
        assert_eq!(solution.free_degrees, 0);
        assert_point(graph.datum("c", "top_center").unwrap(), (5.0, 10.0, 90.0));
    }

    #[test]
    fn free_instances_inside_frames_are_solved_in_model_coordinates() {
        let definition = block();
        let mut graph = stacked(&definition);
        graph
            .add_frame("shelf", None, turned(FRAC_PI_6, 200.0, 0.0, 0.0), "layout")
            .unwrap();
        graph.set_instance_frame("b", Some("shelf")).unwrap();
        seat(&mut graph, "b", "a");

        let solution = graph.solve_placements(&["b"]).unwrap();
        assert!(solution.solved, "{solution:?}");
        assert_point(graph.datum("b", "top_center").unwrap(), (5.0, 10.0, 60.0));
        assert_eq!(graph.node("b").unwrap().frame(), Some("shelf"));
    }

    /// Far from the origin, positions are checked to the relationship
    /// tolerance; doubles cannot hold 1e-9 mm at meter-scale coordinates.
    fn assert_near(datum: ResolvedDatum, expected: (f64, f64, f64)) {
        let ResolvedDatum::Point { origin } = datum else {
            panic!("expected a point, got {datum:?}");
        };
        let error = length(subtract(
            origin,
            Vec3::new(expected.0, expected.1, expected.2),
        ));
        assert!(
            error < RELATIONSHIP_LINEAR_TOLERANCE,
            "{origin:?} != {expected:?}"
        );
    }

    /// Rotation about the model origin made rotation and translation nearly
    /// interchangeable for distant parts; this stack used to stall tilted.
    #[test]
    fn coincident_stack_far_from_the_origin_converges() {
        let definition = block();
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("i0", HashMap::new(), "test").unwrap();
        graph
            .set_placement("i0", translated(1000.0, -500.0, 0.0))
            .unwrap();
        let mut free = Vec::new();
        for index in 1..=5 {
            let id = format!("i{index}");
            graph
                .add_clone(id.clone(), "i0", HashMap::new(), "test")
                .unwrap();
            graph
                .set_placement(
                    &id,
                    translated(1000.0 + index as f64 * 3.0, -502.0, index as f64 * 25.0),
                )
                .unwrap();
            graph
                .add_relationship(relationship(
                    &format!("r{index}"),
                    RelationKind::Coincident,
                    (&format!("i{}", index - 1), "top"),
                    (&id, "bottom"),
                ))
                .unwrap();
            free.push(id);
        }
        let ids = free.iter().map(String::as_str).collect::<Vec<_>>();

        let solution = graph.solve_placements(&ids).unwrap();
        assert!(solution.solved, "{solution:?}");
        // Each block keeps its X/Y offset and rotation about Z free.
        assert_eq!(solution.free_degrees, 15);
        for index in 1..=5 {
            assert_near(
                graph.datum(&format!("i{index}"), "top_center").unwrap(),
                (
                    1005.0 + index as f64 * 3.0,
                    -492.0,
                    30.0 * (index as f64 + 1.0),
                ),
            );
        }
    }

    #[test]
    fn fully_constrained_seat_far_from_the_origin_converges() {
        let definition = block();
        let mut graph = stacked(&definition);
        graph
            .set_placement("a", translated(5000.0, -3000.0, 200.0))
            .unwrap();
        graph
            .set_placement("b", turned(FRAC_PI_6, 5040.0, -3015.0, 270.0))
            .unwrap();
        seat(&mut graph, "b", "a");

        let solution = graph.solve_placements(&["b"]).unwrap();
        assert!(solution.solved, "{solution:?}");
        assert_eq!(solution.free_degrees, 0);
        assert_near(
            graph.datum("b", "top_center").unwrap(),
            (5005.0, -2990.0, 260.0),
        );
    }

    #[test]
    fn solving_rejects_invalid_free_sets() {
        let definition = block();
        let mut graph = stacked(&definition);
        assert!(graph.solve_placements(&[]).is_err());
        assert!(graph.solve_placements(&["b", "b"]).is_err());
        assert!(graph.solve_placements(&["missing"]).is_err());
        let error = graph.solve_placements(&["b"]).unwrap_err();
        assert!(error.message.contains("no relationship"), "{error}");
    }
}
