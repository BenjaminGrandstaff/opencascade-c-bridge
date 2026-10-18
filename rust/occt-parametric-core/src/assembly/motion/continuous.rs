//! Adaptive continuous checks using BREP distance and conservative motion bounds.
use super::*;
use occt_types::Bounds;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContinuousCollisionOptions {
    /// Extra separation margin for kernel/numeric uncertainty, in length units.
    pub distance_guard: Quantity,
    /// Global budget for exact pair queries across all segments.
    pub maximum_queries: usize,
    /// At most this many swept-bound candidate pairs per segment.
    pub maximum_candidate_pairs: usize,
    pub maximum_depth: usize,
    /// Unresolved intervals are reported at this fraction of a segment.
    pub minimum_interval_fraction: f64,
}
impl Default for ContinuousCollisionOptions {
    fn default() -> Self {
        Self {
            distance_guard: Quantity::length(1e-6, LengthUnit::Millimeter),
            maximum_queries: 100_000,
            maximum_candidate_pairs: 100_000,
            maximum_depth: 32,
            minimum_interval_fraction: 1e-8,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContinuousStatus {
    Clear,
    Collision,
    Unresolved,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ContinuousPairResult {
    pub segment: usize,
    pub first: InstanceOutputRef,
    pub second: InstanceOutputRef,
    pub status: ContinuousStatus,
    /// An observed violating position, not the first time of contact.
    pub fraction: Option<f64>,
    pub check: Option<PairCheck>,
    pub unresolved_fraction_range: Option<[f64; 2]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ContinuousMotionResult {
    pub status: ContinuousStatus,
    /// Only witnessed violations and unresolved pairs. Clear pairs are omitted.
    pub pairs: Vec<ContinuousPairResult>,
    pub segments: usize,
    pub candidate_pairs: usize,
    pub exact_queries: usize,
    /// Narrow-phase subintervals certified clear by propagated box bounds.
    pub bounds_rejected_intervals: usize,
    pub generated_variants: usize,
    pub unresolved_pairs: usize,
}
impl ContinuousCollisionOptions {
    pub fn checked(self) -> Result<f64, ModelError> {
        if self.distance_guard.dimension != Dimension::Length {
            return Err(ModelError::new(
                "continuous distance guard must be a length",
            ));
        }
        let guard = self.distance_guard.normalized()?;
        if !guard.is_finite()
            || guard < 0.0
            || !(1..=1_000_000).contains(&self.maximum_queries)
            || !(1..=1_000_000).contains(&self.maximum_candidate_pairs)
            || !(1..=52).contains(&self.maximum_depth)
            || !self.minimum_interval_fraction.is_finite()
            || !(f64::EPSILON..=1.0).contains(&self.minimum_interval_fraction)
        {
            return Err(ModelError::new(
                "invalid continuous-check numeric margin or resource budget",
            ));
        }
        Ok(guard)
    }
}
pub fn angle(joint: &AssemblyJoint) -> Result<f64, ModelError> {
    Ok(match &joint.kind {
        JointKind::Revolute { angle }
        | JointKind::Cylindrical { angle, .. }
        | JointKind::Planar { angle, .. } => angle.value.normalized()?,
        _ => 0.0,
    })
}
pub fn validate_translation(
    start: &InstanceGraph<'_>,
    end: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    for (frame, joint) in &start.assembly.joints {
        if angle(joint)? != angle(&end.assembly.joints[frame])? {
            return Err(ModelError::new(
                "continuous translation checks reject changing angular coordinates, including full turns",
            ));
        }
    }
    Ok(())
}
pub fn magnitude(point: Vec3) -> f64 {
    point.x.abs().max(point.y.abs()).max(point.z.abs())
}
pub fn finite(point: Vec3) -> bool {
    [point.x, point.y, point.z]
        .iter()
        .all(|value| value.is_finite())
}
pub fn frame_origin(
    graph: &InstanceGraph<'_>,
    frame: Option<&str>,
) -> Result<(Vec3, f64), ModelError> {
    let chain = graph.frame_chain(frame)?;
    let mut point = Vec3::new(0.0, 0.0, 0.0);
    let mut magnitude_bound: f64 = 0.0;
    for placement in &chain {
        let placement = placement.normalized()?;
        magnitude_bound = magnitude_bound.max(magnitude(placement.translation));
        if let Some((origin, _, _)) = placement.rotation {
            magnitude_bound = magnitude_bound.max(magnitude(origin));
        }
        point = transform_point(point, &placement);
        if !finite(point) {
            return Err(ModelError::new(
                "continuous frame placement is not representable",
            ));
        }
        magnitude_bound = magnitude_bound.max(magnitude(point));
    }
    let guard = magnitude_bound * (128.0 * f64::EPSILON) * (chain.len() + 1) as f64;
    Ok((point, guard))
}
pub fn boxes_separated(first: Bounds, second: Bounds, margin: f64) -> bool {
    first.min.x - second.max.x > margin
        || second.min.x - first.max.x > margin
        || first.min.y - second.max.y > margin
        || second.min.y - first.max.y > margin
        || first.min.z - second.max.z > margin
        || second.min.z - first.max.z > margin
}
pub enum PairOutcome {
    Clear,
    Collision(f64, PairCheck),
    Unresolved([f64; 2]),
}
impl ContinuousMotionResult {
    pub fn record(
        &mut self,
        segment: usize,
        first: &InstanceOutputRef,
        second: &InstanceOutputRef,
        outcome: PairOutcome,
    ) {
        let (status, fraction, check, range) = match outcome {
            PairOutcome::Clear => return,
            PairOutcome::Collision(fraction, check) => {
                self.status = ContinuousStatus::Collision;
                (
                    ContinuousStatus::Collision,
                    Some(fraction),
                    Some(check),
                    None,
                )
            }
            PairOutcome::Unresolved(range) => {
                self.unresolved_pairs += 1;
                if self.status == ContinuousStatus::Clear {
                    self.status = ContinuousStatus::Unresolved;
                }
                (ContinuousStatus::Unresolved, None, None, Some(range))
            }
        };
        self.pairs.push(ContinuousPairResult {
            segment,
            first: first.clone(),
            second: second.clone(),
            status,
            fraction,
            check,
            unresolved_fraction_range: range,
        });
    }
}
