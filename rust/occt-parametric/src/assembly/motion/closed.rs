//! Pose continuation for driven, closed-linkage sampled motion.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClosedMotionOptions {
    pub joint_solver: JointSolveOptions,
    /// Sum of reported solver iterations; 1..=1,000,000 across the study.
    pub maximum_total_iterations: usize,
}
impl Default for ClosedMotionOptions {
    fn default() -> Self {
        Self {
            joint_solver: JointSolveOptions::default(),
            maximum_total_iterations: 100_000,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointMotionStatus {
    Complete,
    /// A pose failed closure within its per-pose solver limits.
    ClosureFailed,
    /// The total iteration budget stopped the study.
    BudgetExceeded,
}
#[derive(Clone, Debug, PartialEq)]
pub struct JointMotionSolution {
    pub status: JointMotionStatus,
    /// First failed or unattempted sample; None only when Complete.
    pub failed_sample: Option<usize>,
    /// Includes successful closures and the unsuccessful best fit, if attempted.
    pub solutions: Vec<JointSolution>,
    /// Available only when every sample closed; contains drivers and solved freedoms.
    pub closed_study: Option<MotionStudy>,
    pub iterations: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ClosedMotionResult {
    pub closure: JointMotionSolution,
    /// Collision checks only run after every pose closes successfully.
    pub motion: Option<MotionResult>,
}
fn validate_roles(
    graph: &InstanceGraph<'_>,
    study: &MotionStudy,
    free: &[JointVariable],
    options: ClosedMotionOptions,
) -> Result<(), ModelError> {
    options.joint_solver.checked()?;
    let entries = free
        .len()
        .checked_add(graph.assembly.relationships.len())
        .and_then(|per_sample| per_sample.checked_mul(study.samples.len()))
        .and_then(|count| {
            study.samples.iter().try_fold(count, |total, sample| {
                total.checked_add(sample.positions.len())
            })
        });
    if entries.is_none_or(|count| count > 4_000_000) {
        return Err(ModelError::new(
            "closed motion report entry budget exceeded",
        ));
    }
    if !(1..=1_000_000).contains(&options.maximum_total_iterations) {
        return Err(ModelError::new(
            "invalid closed motion total iteration budget",
        ));
    }
    let variables = free
        .iter()
        .map(|variable| (variable.frame.as_str(), variable.coordinate))
        .collect::<HashSet<_>>();
    if study
        .samples
        .iter()
        .flat_map(|sample| &sample.positions)
        .any(|position| variables.contains(&(position.frame.as_str(), position.coordinate)))
    {
        return Err(ModelError::new(
            "closed motion cannot drive a selected free joint coordinate",
        ));
    }
    Ok(())
}
fn apply_seed(graph: &mut InstanceGraph<'_>, seed: &[JointPosition]) -> Result<(), ModelError> {
    for position in seed {
        graph.set_joint_coordinate(&position.frame, position.coordinate, position.value)?;
    }
    Ok(())
}
fn failure_status(solution: &JointSolution, remaining: usize) -> JointMotionStatus {
    if solution.iterations >= remaining {
        JointMotionStatus::BudgetExceeded
    } else {
        JointMotionStatus::ClosureFailed
    }
}
impl InstanceGraph<'_> {
    /// Solves each independently driven sample, seeding free coordinates from
    /// the previous successful pose. No geometry generation or graph mutation.
    /// All driver edits validate before solving. A failure reports its index and
    /// best fit and exposes no partial MotionStudy. This is local continuation;
    /// it does not guarantee one global branch or closure between sample poses.
    pub fn solve_motion_study(
        &self,
        study: &MotionStudy,
        free: &[JointVariable],
        options: ClosedMotionOptions,
    ) -> Result<JointMotionSolution, ModelError> {
        validate_roles(self, study, free, options)?;
        validate_study(self, study)?;
        let mut result = JointMotionSolution {
            status: JointMotionStatus::Complete,
            failed_sample: None,
            solutions: Vec::with_capacity(study.samples.len()),
            closed_study: None,
            iterations: 0,
        };
        let mut closed = MotionStudy {
            samples: Vec::with_capacity(study.samples.len()),
            outputs: study.outputs.clone(),
            excluded_pairs: study.excluded_pairs.clone(),
            collision_options: study.collision_options,
        };
        let mut seed = Vec::new();
        for (index, sample) in study.samples.iter().enumerate() {
            let remaining = options.maximum_total_iterations - result.iterations;
            if remaining == 0 {
                result.status = JointMotionStatus::BudgetExceeded;
                result.failed_sample = Some(index);
                return Ok(result);
            }
            let mut candidate = sample_graph(self, sample, index)?;
            apply_seed(&mut candidate, &seed)?;
            let solver = JointSolveOptions {
                maximum_iterations: options.joint_solver.maximum_iterations.min(remaining),
                ..options.joint_solver
            };
            let solution = candidate.solve_joint_coordinates(free, solver)?;
            result.iterations += solution.iterations;
            let solved = solution.solved;
            let status = failure_status(&solution, remaining);
            seed = solution.positions.clone();
            result.solutions.push(solution);
            if !solved {
                result.status = status;
                result.failed_sample = Some(index);
                return Ok(result);
            }
            let mut positions = sample.positions.clone();
            positions.extend(seed.iter().cloned());
            closed.samples.push(MotionSample { positions });
        }
        result.closed_study = Some(closed);
        Ok(result)
    }
    /// Closes all sample poses before generating shared geometry and checking
    /// sampled collisions. Closure failure returns a report with no kernel work.
    /// Kernel errors release temporary geometry. The source graph is preserved.
    pub fn run_closed_motion_study(
        &self,
        session: &Session,
        study: &MotionStudy,
        free: &[JointVariable],
        options: ClosedMotionOptions,
    ) -> Result<ClosedMotionResult, ModelError> {
        let closure = self.solve_motion_study(study, free, options)?;
        let motion = closure
            .closed_study
            .as_ref()
            .map(|closed| self.run_motion_study(session, closed))
            .transpose()?;
        Ok(ClosedMotionResult { closure, motion })
    }
}
