//! Bounded deterministic multi-start discovery of alternative linkage poses.
use super::*;
mod seeds;
use seeds::{checked_axes, seed_graph};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointSeedAxis {
    pub variable: JointVariable,
    /// Explicit starting coordinates; their Cartesian product defines the search.
    pub seeds: Vec<Quantity>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointBranchEquivalence {
    /// Compare unwrapped coordinate values, including complete revolutions.
    Coordinates,
    /// Angular coordinates differing by whole revolutions represent one pose.
    #[default]
    PeriodicAngles,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JointBranchSearchOptions {
    pub joint_solver: JointSolveOptions,
    /// 1..=1,000,000 reported iterations across all starts.
    pub maximum_total_iterations: usize,
    /// 1..=128 distinct successful solutions retained.
    pub maximum_branches: usize,
    pub include_current_pose: bool,
    /// Coordinate-wise distance after translations are divided by characteristic length.
    pub distinct_normalized_distance: f64,
    pub equivalence: JointBranchEquivalence,
}
impl Default for JointBranchSearchOptions {
    fn default() -> Self {
        Self {
            joint_solver: JointSolveOptions::default(),
            maximum_total_iterations: 100_000,
            maximum_branches: 32,
            include_current_pose: true,
            distinct_normalized_distance: 1e-5,
            equivalence: JointBranchEquivalence::PeriodicAngles,
        }
    }
}
impl JointBranchSearchOptions {
    fn checked(self) -> Result<f64, ModelError> {
        let length = self.joint_solver.checked()?;
        if !(1..=1_000_000).contains(&self.maximum_total_iterations)
            || !(1..=128).contains(&self.maximum_branches)
            || !self.distinct_normalized_distance.is_finite()
            || self.distinct_normalized_distance <= 0.0
        {
            return Err(ModelError::new("invalid joint branch search options"));
        }
        Ok(length)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JointBranchSearchStatus {
    /// All requested seeds were attempted; completeness is not certified.
    SeedsExhausted,
    IterationBudgetExceeded,
    BranchLimitReached,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct JointBranchSearchResult {
    pub status: JointBranchSearchStatus,
    /// First-discovery order, with the current pose first when requested.
    pub branches: Vec<JointSolution>,
    pub attempted_starts: usize,
    pub planned_starts: usize,
    pub iterations: usize,
    pub failed_starts: usize,
    pub repeated_solutions: usize,
    /// Failed candidate with the smallest maximum normalized residual.
    pub best_unsolved: Option<JointSolution>,
}
impl JointBranchSearchResult {
    fn record(&mut self, solution: JointSolution, options: JointBranchSearchOptions, length: f64) {
        self.attempted_starts += 1;
        self.iterations += solution.iterations;
        if !solution.solved {
            self.failed_starts += 1;
            if self
                .best_unsolved
                .as_ref()
                .is_none_or(|best| solution.max_normalized_residual < best.max_normalized_residual)
            {
                self.best_unsolved = Some(solution);
            }
        } else if self
            .branches
            .iter()
            .any(|branch| equivalent(branch, &solution, options, length))
        {
            self.repeated_solutions += 1;
        } else {
            self.branches.push(solution);
        }
    }
}
fn equivalent(
    first: &JointSolution,
    second: &JointSolution,
    options: JointBranchSearchOptions,
    length: f64,
) -> bool {
    first.positions.iter().zip(&second.positions).all(|(a, b)| {
        // Solver positions have already passed unit and finiteness validation.
        let a_value = a.value.normalized().unwrap_or(f64::NAN);
        let b_value = b.value.normalized().unwrap_or(f64::NAN);
        let distance = if a.coordinate == JointDof::Angle {
            if options.equivalence == JointBranchEquivalence::PeriodicAngles {
                let tau = std::f64::consts::TAU;
                let difference = (a_value.rem_euclid(tau) - b_value.rem_euclid(tau)).abs();
                difference.min(tau - difference)
            } else {
                (a_value - b_value).abs()
            }
        } else {
            (a_value - b_value).abs() / length
        };
        distance <= options.distinct_normalized_distance
    })
}
impl InstanceGraph<'_> {
    /// Discovers distinct solutions using the current pose and an explicit seed
    /// grid. Seed axes affect starting poses only; local solves retain the joint's
    /// declared limits and may converge outside the seed interval. No mutation,
    /// geometry generation or proof of complete branch enumeration.
    pub fn search_joint_branches(
        &self,
        axes: &[JointSeedAxis],
        options: JointBranchSearchOptions,
    ) -> Result<JointBranchSearchResult, ModelError> {
        let length = options.checked()?;
        let (free, product) = checked_axes(self, axes, length, options.include_current_pose)?;
        let planned_starts = product + usize::from(options.include_current_pose);
        let mut result = JointBranchSearchResult {
            status: JointBranchSearchStatus::SeedsExhausted,
            branches: Vec::new(),
            attempted_starts: 0,
            planned_starts,
            iterations: 0,
            failed_starts: 0,
            repeated_solutions: 0,
            best_unsolved: None,
        };
        for index in 0..planned_starts {
            let remaining = options.maximum_total_iterations - result.iterations;
            if remaining == 0 {
                result.status = JointBranchSearchStatus::IterationBudgetExceeded;
                break;
            }
            let mut candidate = seed_graph(self, axes, index, options.include_current_pose)?;
            let solution = candidate.solve_joint_coordinates(
                &free,
                JointSolveOptions {
                    maximum_iterations: options.joint_solver.maximum_iterations.min(remaining),
                    ..options.joint_solver
                },
            )?;
            let solved = solution.solved;
            result.record(solution, options, length);
            if result.branches.len() >= options.maximum_branches
                && result.attempted_starts < planned_starts
            {
                result.status = JointBranchSearchStatus::BranchLimitReached;
                break;
            }
            if !solved && result.iterations == options.maximum_total_iterations {
                result.status = JointBranchSearchStatus::IterationBudgetExceeded;
                break;
            }
        }
        Ok(result)
    }
}
