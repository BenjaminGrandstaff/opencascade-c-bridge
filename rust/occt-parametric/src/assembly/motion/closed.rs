//! Pose continuation for driven, closed-linkage sampled motion.
use super::*;
#[allow(unused_imports)]
pub use occt_parametric_core::assembly::motion::closed::*;
pub trait InstanceGraphClosedMotion {
    /// Closes all sample poses before generating shared geometry and checking
    /// sampled collisions. Closure failure returns a report with no kernel work.
    /// Kernel errors release temporary geometry. The source graph is preserved.
    fn run_closed_motion_study(
        &self,
        session: &Session,
        study: &MotionStudy,
        free: &[JointVariable],
        options: ClosedMotionOptions,
    ) -> Result<ClosedMotionResult, ModelError>;
}

impl InstanceGraphClosedMotion for InstanceGraph<'_> {
    fn run_closed_motion_study(
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
