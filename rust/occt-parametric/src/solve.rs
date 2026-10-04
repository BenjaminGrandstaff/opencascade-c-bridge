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
use crate::sparse::{SparseJacobian, SymmetricMatrix};

const MAX_ITERATIONS: usize = 200;
/// Scale-relative convergence target used when it is stricter than the
/// model's configured relationship tolerances.
const RELATIVE_TOLERANCE: f64 = 1e-12;
/// The solver works on vector components and sine-like angular residuals,
/// while final checks use vector lengths and angles. Staying inside half the
/// configured boundary guarantees those final checks have geometric margin.
const CHECK_TOLERANCE_MARGIN: f64 = 0.5;
/// Initial Marquardt damping. Assembly problems are close to linear, so the
/// solver starts near Gauss–Newton and damps only after a rejected step;
/// heavier initial damping crawls along chains, whose weakest mode shrinks
/// like 1/n^2.
const INITIAL_DAMPING: f64 = 1e-15;
const DIFFERENCE_STEP: f64 = 1e-7;
const MAX_DAMPING: f64 = 1e12;
/// Rank tolerance on `JᵀJ` pivots relative to its largest diagonal; it
/// corresponds to singular values of `J` above about 1e-6 of the largest.
const NORMAL_RANK_TOLERANCE: f64 = 1e-12;
/// Levenberg share of the damping, relative to the largest diagonal. It
/// tames directions that are only nearly unconstrained (weakly coupled while
/// parts are slightly tilted) and vanishes with the damping itself.
const LEVENBERG_FLOOR: f64 = 1e-6;
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
    /// The distinct free instances this term depends on (at most two).
    fn free_instances(&self) -> Vec<usize> {
        let mut instances = self
            .free_endpoints()
            .map(|(instance, _)| instance)
            .collect::<Vec<_>>();
        instances.dedup();
        instances
    }

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

        let fit = closest_solution(&problem, start, self.assembly.tolerances)?;
        let rank = fit.jacobian.normal_matrix().rank(NORMAL_RANK_TOLERANCE);
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
        push_term_values(problem, term, unknowns, &mut values);
    }
    values
}

fn push_term_values(problem: &Problem, term: &Term, unknowns: &[f64], values: &mut Vec<f64>) {
    term_residuals(
        term.kind,
        term.first.datum(unknowns, &problem.pivots),
        term.second.datum(unknowns, &problem.pivots),
        problem.angular_scale,
        values,
    );
}

fn push(values: &mut Vec<f64>, vector: Vec3) {
    values.extend([vector.x, vector.y, vector.z]);
}

/// Smooth residual components that vanish exactly when the relationship
/// holds. Angular components are multiplied by `angular` (a length) so they
/// weigh like millimeters. Datum pairs were validated when the relationship
/// was added.
pub(crate) fn term_residuals(
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
    jacobian: SparseJacobian,
    iterations: usize,
}

/// Largest residual growth a restoring move may cause before restoration
/// stops; true null-space moves change the residual only at second order.
const RESTORATION_GROWTH: f64 = 10.0;

/// Upper bound on null-space restoration rounds after the first fit; each
/// round shrinks the remaining drift by about three orders of magnitude.
const RESTORATION_ROUNDS: usize = 8;

/// Solves, then repeatedly moves the solution back toward `start` along the
/// directions no relationship constrains and polishes it again. Solving
/// alone can drift along those directions on the way (a temporary tilt makes
/// a free slide matter), so restoration is what makes under-constrained
/// instances move as little as possible.
fn closest_solution(
    problem: &Problem,
    start: Vec<f64>,
    tolerances: RelationshipTolerances,
) -> Result<Fit, ModelError> {
    let tolerance = problem_tolerance(problem, &start, tolerances);
    let mut fit = least_squares(problem, start.clone(), tolerances)?;
    let mut previous = f64::INFINITY;
    for _ in 0..RESTORATION_ROUNDS {
        let offset = start
            .iter()
            .zip(&fit.unknowns)
            .map(|(initial, current)| initial - current)
            .collect::<Vec<_>>();
        let Some(restoring) = null_space_component(&fit.jacobian, &offset) else {
            break;
        };
        // Stop once restoration is negligible or no longer shrinking, which
        // happens at the floating-point floor.
        let size = max_abs(&restoring);
        if size <= tolerance || size >= 0.5 * previous {
            break;
        }
        previous = size;
        // A null-space move leaves the residual unchanged to first order; a
        // move that raises it means the fit is not converged enough for its
        // Jacobian to identify the free directions, so keep the fit.
        let restored = add_step(&fit.unknowns, &restoring);
        let current = max_abs(&fit.residuals);
        if max_abs(&residuals(problem, &restored)) > RESTORATION_GROWTH * current.max(tolerance) {
            break;
        }
        let iterations = fit.iterations;
        fit = least_squares(problem, restored, tolerances)?;
        fit.iterations += iterations;
    }
    Ok(fit)
}

fn problem_tolerance(
    problem: &Problem,
    unknowns: &[f64],
    tolerances: RelationshipTolerances,
) -> f64 {
    let largest_translation = unknowns
        .iter()
        .enumerate()
        .filter(|(index, _)| index % UNKNOWNS_PER_INSTANCE >= 3)
        .fold(0.0_f64, |largest, (_, value)| largest.max(value.abs()));
    let scale_target = RELATIVE_TOLERANCE * largest_translation.max(problem.angular_scale).max(1.0);
    let relationship_target = tolerances
        .linear_millimeters
        .min(tolerances.angular_radians * problem.angular_scale)
        * CHECK_TOLERANCE_MARGIN;
    scale_target.min(relationship_target)
}

/// Projection passes that remove rounding residue from the null-space part.
const PROJECTION_PASSES: usize = 3;

/// The part of `offset` the Jacobian does not see: `offset - y`, where `y`
/// solves `JᵀJ y = JᵀJ r` with unconstrained variables held at zero, so
/// `J (offset - y) = 0`. Repeated passes remove rounding residue. One sparse
/// factorization per pass.
fn null_space_component(jacobian: &SparseJacobian, offset: &[f64]) -> Option<Vec<f64>> {
    let normal = jacobian.normal_matrix();
    let mut component = offset.to_vec();
    for _ in 0..PROJECTION_PASSES {
        let seen = normal
            .clone()
            .solve_dropping_null(normal.times(&component), NORMAL_RANK_TOLERANCE)?;
        for (value, seen) in component.iter_mut().zip(seen) {
            *value -= seen;
        }
    }
    Some(component)
}

fn least_squares(
    problem: &Problem,
    mut unknowns: Vec<f64>,
    tolerances: RelationshipTolerances,
) -> Result<Fit, ModelError> {
    let mut values = residuals(problem, &unknowns);
    let mut cost = squared_norm(&values);
    let mut damping = INITIAL_DAMPING;
    let tolerance = problem_tolerance(problem, &unknowns, tolerances);
    let mut iterations = 0;
    while iterations < MAX_ITERATIONS && max_abs(&values) > tolerance {
        iterations += 1;
        let jacobian = sparse_jacobian(problem, &unknowns);
        let Some(step) = damped_step(&jacobian, &values, &mut damping, |step| {
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
    let jacobian = sparse_jacobian(problem, &unknowns);
    Ok(Fit {
        unknowns,
        residuals: values,
        jacobian,
        iterations,
    })
}

type Accepted = (Vec<f64>, Vec<f64>);

/// Increases damping until a step lowers the cost; `None` when damping
/// saturates without progress. Steps solve the Marquardt-damped normal
/// equations with unconstrained directions held fixed, so free unknowns never
/// move on rounding noise and constrained ones carry no regularization bias,
/// which matters for chains whose solution moves far from the start.
pub(crate) fn damped_step(
    jacobian: &SparseJacobian,
    values: &[f64],
    damping: &mut f64,
    mut try_step: impl FnMut(&[f64]) -> Option<Accepted>,
) -> Option<Accepted> {
    let normal = jacobian.normal_matrix();
    let rhs = jacobian
        .transpose_times(values)
        .iter()
        .map(|value| -value)
        .collect::<Vec<_>>();
    let levenberg = LEVENBERG_FLOOR * normal.largest_diagonal().max(f64::MIN_POSITIVE);
    while *damping <= MAX_DAMPING {
        let mut damped: SymmetricMatrix = normal.clone();
        for index in 0..normal.size() {
            damped.add_diagonal(index, *damping * (normal.diagonal(index) + levenberg));
        }
        if let Some(step) = damped.solve_dropping_null(rhs.clone(), NORMAL_RANK_TOLERANCE)
            && let Some(accepted) = try_step(&step)
        {
            *damping = (*damping / 10.0).max(INITIAL_DAMPING);
            return Some(accepted);
        }
        *damping *= 10.0;
    }
    None
}

/// Central differences per relationship: each term depends on at most two
/// free instances, so only their twelve unknowns are perturbed and only that
/// term is re-evaluated. O(terms) term evaluations instead of
/// O(unknowns x terms) for a dense Jacobian.
fn sparse_jacobian(problem: &Problem, unknowns: &[f64]) -> SparseJacobian {
    let mut rows = Vec::new();
    let mut perturbed = unknowns.to_vec();
    let mut ahead = Vec::new();
    let mut behind = Vec::new();
    for term in &problem.terms {
        let mut base = Vec::new();
        push_term_values(problem, term, unknowns, &mut base);
        let mut term_rows = vec![Vec::new(); base.len()];
        for instance in term.free_instances() {
            for column in instance * UNKNOWNS_PER_INSTANCE..(instance + 1) * UNKNOWNS_PER_INSTANCE {
                // Relative steps keep rounding noise small for coordinates
                // far from the origin.
                let step = DIFFERENCE_STEP * unknowns[column].abs().max(1.0);
                ahead.clear();
                behind.clear();
                perturbed[column] = unknowns[column] + step;
                push_term_values(problem, term, &perturbed, &mut ahead);
                perturbed[column] = unknowns[column] - step;
                push_term_values(problem, term, &perturbed, &mut behind);
                perturbed[column] = unknowns[column];
                for (row, (forward, backward)) in
                    term_rows.iter_mut().zip(ahead.iter().zip(&behind))
                {
                    let derivative = (forward - backward) / (2.0 * step);
                    if derivative != 0.0 {
                        row.push((column, derivative));
                    }
                }
            }
        }
        rows.extend(term_rows);
    }
    SparseJacobian {
        rows,
        columns: unknowns.len(),
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
mod tests;
