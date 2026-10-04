//! Local, bounded joint-coordinate solving of closed assembly relationships.
use super::*;
use crate::solve::{damped_step, term_residuals};
use crate::sparse::SparseJacobian;

/// Coordinate the solver may change. Every other coordinate remains driven.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JointVariable {
    pub frame: String,
    pub coordinate: JointDof,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointSolveOptions {
    /// Between 1 and 1000; each iteration evaluates a finite-difference Jacobian.
    pub maximum_iterations: usize,
    /// Representative linkage length; balances angular and linear residuals.
    pub characteristic_length: Quantity,
}
impl Default for JointSolveOptions {
    fn default() -> Self {
        Self {
            maximum_iterations: 200,
            characteristic_length: Quantity::length(1.0, LengthUnit::Millimeter),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JointSolution {
    /// All assembly relationships passed their authoritative geometric checks.
    pub solved: bool,
    pub iterations: usize,
    /// Largest dimensionless residual component at the reported best fit.
    pub max_normalized_residual: f64,
    /// Jacobian nullity before considering active coordinate bounds.
    pub free_degrees: usize,
    pub redundant_equations: usize,
    /// Best fit, including unsuccessful solves; exact bounds retain the limit unit.
    pub positions: Vec<JointPosition>,
    pub active_limits: Vec<JointVariable>,
    pub checks: Vec<RelationshipCheck>,
}

struct Variable {
    reference: JointVariable,
    original: Quantity,
    start: f64,
    scale: f64,
    lower: f64,
    upper: f64,
    minimum: Option<Quantity>,
    maximum: Option<Quantity>,
}
impl Variable {
    fn new(
        graph: &InstanceGraph<'_>,
        reference: &JointVariable,
        length: f64,
    ) -> Result<Self, ModelError> {
        let mut joint = graph
            .assembly
            .joints
            .get(&reference.frame)
            .ok_or_else(|| ModelError::new("free joint frame does not exist"))?
            .clone();
        let scalar = joint.kind.coordinate_mut(reference.coordinate)?;
        let start = scalar.value.normalized()?;
        let scale = if reference.coordinate == JointDof::Angle {
            1.0
        } else {
            length
        };
        let lower = scalar
            .minimum
            .map(Quantity::normalized)
            .transpose()?
            .unwrap_or(f64::NEG_INFINITY);
        let upper = scalar
            .maximum
            .map(Quantity::normalized)
            .transpose()?
            .unwrap_or(f64::INFINITY);
        if lower == upper {
            return Err(ModelError::new(
                "a fixed-limit coordinate cannot be a free joint variable",
            ));
        }
        Ok(Self {
            reference: reference.clone(),
            original: scalar.value,
            start,
            scale,
            lower: (lower - start) / scale,
            upper: (upper - start) / scale,
            minimum: scalar.minimum,
            maximum: scalar.maximum,
        })
    }
    fn position(&self, offset: f64) -> JointPosition {
        let normalized = self.start + offset * self.scale;
        let factor = self
            .original
            .unit
            .map_or(1.0, LengthUnit::millimeter_factor);
        let bound = if offset == self.lower {
            self.minimum
        } else if offset == self.upper {
            self.maximum
        } else {
            None
        };
        JointPosition {
            frame: self.reference.frame.clone(),
            coordinate: self.reference.coordinate,
            value: bound.unwrap_or(if offset == 0.0 {
                self.original
            } else {
                Quantity {
                    value: normalized / factor,
                    ..self.original
                }
            }),
        }
    }
}

struct Problem<'graph, 'definition> {
    graph: &'graph InstanceGraph<'definition>,
    variables: Vec<Variable>,
    length: f64,
}
impl<'definition> Problem<'_, 'definition> {
    fn candidate(&self, offsets: &[f64]) -> Result<InstanceGraph<'definition>, ModelError> {
        let mut graph = self.graph.clone();
        for (variable, offset) in self.variables.iter().zip(offsets) {
            let position = variable.position(*offset);
            graph.set_joint_coordinate(&position.frame, position.coordinate, position.value)?;
        }
        Ok(graph)
    }
    fn residuals(&self, offsets: &[f64]) -> Result<Vec<f64>, ModelError> {
        let graph = self.candidate(offsets)?;
        let mut values = Vec::new();
        for relation in &graph.assembly.relationships {
            term_residuals(
                relation.kind,
                graph.datum(&relation.first.instance, &relation.first.datum)?,
                graph.datum(&relation.second.instance, &relation.second.datum)?,
                self.length,
                &mut values,
            );
        }
        for value in &mut values {
            *value /= self.length;
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(ModelError::new("joint solver residual is not finite"));
        }
        Ok(values)
    }
    fn jacobian(&self, offsets: &[f64], rows: usize) -> Result<SparseJacobian, ModelError> {
        let mut jacobian = SparseJacobian {
            rows: vec![Vec::new(); rows],
            columns: offsets.len(),
        };
        let mut perturbed = offsets.to_vec();
        for (column, variable) in self.variables.iter().enumerate() {
            let step = 1e-6 * offsets[column].abs().max(1.0);
            let ahead = (offsets[column] + step).min(variable.upper);
            let behind = (offsets[column] - step).max(variable.lower);
            perturbed[column] = ahead;
            let forward = self.residuals(&perturbed)?;
            perturbed[column] = behind;
            let backward = self.residuals(&perturbed)?;
            perturbed[column] = offsets[column];
            for (row, (a, b)) in jacobian.rows.iter_mut().zip(forward.iter().zip(backward)) {
                let derivative = (a - b) / (ahead - behind);
                if !derivative.is_finite() {
                    return Err(ModelError::new("joint solver derivative is not finite"));
                }
                if derivative != 0.0 {
                    row.push((column, derivative));
                }
            }
        }
        Ok(jacobian)
    }
    fn optimize(
        &self,
        maximum_iterations: usize,
    ) -> Result<(Vec<f64>, Vec<f64>, usize), ModelError> {
        let mut offsets = vec![0.0; self.variables.len()];
        let mut values = self.residuals(&offsets)?;
        let tolerance = 0.25
            * (self.graph.assembly.tolerances.linear_millimeters / self.length)
                .min(self.graph.assembly.tolerances.angular_radians);
        let mut damping = 1e-15;
        let mut iterations = 0;
        while maximum(&values) > tolerance && iterations < maximum_iterations {
            let jacobian = self.jacobian(&offsets, values.len())?;
            let Some((next, residuals)) = damped_step(&jacobian, &values, &mut damping, |step| {
                let next = self.project(&offsets, step);
                let residuals = self.residuals(&next).ok()?;
                (cost(&residuals) < cost(&values)).then_some((next, residuals))
            }) else {
                break;
            };
            offsets = next;
            values = residuals;
            iterations += 1;
        }
        Ok((offsets, values, iterations))
    }
    fn project(&self, offsets: &[f64], step: &[f64]) -> Vec<f64> {
        offsets
            .iter()
            .zip(step)
            .zip(&self.variables)
            .map(|((value, delta), variable)| (value + delta).clamp(variable.lower, variable.upper))
            .collect()
    }
}
fn cost(values: &[f64]) -> f64 {
    values.iter().map(|value| value * value).sum()
}
fn maximum(values: &[f64]) -> f64 {
    values
        .iter()
        .fold(0.0, |largest, value| largest.max(value.abs()))
}

impl InstanceGraph<'_> {
    /// Solves every recorded relationship by adjusting only `free` coordinates.
    /// Uses the current pose as a local seed; singular poses and disconnected
    /// assembly branches may require another seed. Bounds are enforced throughout.
    /// A failed solve reports its best fit and leaves this graph untouched.
    /// Limited to 32 coordinates and 256 relationships; geometry is never generated.
    pub fn solve_joint_coordinates(
        &mut self,
        free: &[JointVariable],
        options: JointSolveOptions,
    ) -> Result<JointSolution, ModelError> {
        let length = options.characteristic_length.normalized()?;
        if options.characteristic_length.dimension != Dimension::Length
            || length <= 0.0
            || !length.is_finite()
            || !(1..=1000).contains(&options.maximum_iterations)
        {
            return Err(ModelError::new("invalid joint solver options"));
        }
        if free.is_empty()
            || free.len() > 32
            || self.assembly.relationships.is_empty()
            || self.assembly.relationships.len() > 256
        {
            return Err(ModelError::new(
                "joint solving requires 1..32 coordinates and 1..256 relationships",
            ));
        }
        self.validate_joints()?;
        self.assembly.tolerances.validate()?;
        self.check_relationships()?;
        let mut seen = HashSet::new();
        if free.iter().any(|variable| !seen.insert(variable)) {
            return Err(ModelError::new("duplicate free joint coordinate"));
        }
        let problem = Problem {
            graph: self,
            variables: free
                .iter()
                .map(|variable| Variable::new(self, variable, length))
                .collect::<Result<_, _>>()?,
            length,
        };
        let (offsets, values, iterations) = problem.optimize(options.maximum_iterations)?;
        let jacobian = problem.jacobian(&offsets, values.len())?;
        let rank = jacobian.normal_matrix().rank(1e-12);
        let candidate = problem.candidate(&offsets)?;
        let checks = candidate.check_relationships()?;
        let solved = checks.iter().all(|check| check.satisfied);
        let positions = problem
            .variables
            .iter()
            .zip(&offsets)
            .map(|(variable, value)| variable.position(*value))
            .collect();
        let active_limits = problem
            .variables
            .iter()
            .zip(&offsets)
            .filter(|(variable, value)| **value == variable.lower || **value == variable.upper)
            .map(|(variable, _)| variable.reference.clone())
            .collect();
        let result = JointSolution {
            solved,
            iterations,
            max_normalized_residual: maximum(&values),
            free_degrees: free.len() - rank,
            redundant_equations: values.len() - rank,
            positions,
            active_limits,
            checks,
        };
        if solved {
            *self = candidate;
        }
        Ok(result)
    }
}
