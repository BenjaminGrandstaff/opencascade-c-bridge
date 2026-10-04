//! Adaptive continuous checks using BREP distance and conservative motion bounds.
use super::*;
use crate::assembly::collisions::{Body, Node, inspect_pair};
use occt_bridge::Bounds;
mod coherent;
mod rotations;
use rotations::RigidPath;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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
    fn checked(self) -> Result<f64, ModelError> {
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
fn angle(joint: &AssemblyJoint) -> Result<f64, ModelError> {
    Ok(match &joint.kind {
        JointKind::Revolute { angle }
        | JointKind::Cylindrical { angle, .. }
        | JointKind::Planar { angle, .. } => angle.value.normalized()?,
        _ => 0.0,
    })
}
fn validate_translation(
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
#[derive(Clone)]
struct Movement {
    delta: Vec3,
    rounding_guard: f64,
    rotation: Option<RigidPath>,
}
impl Movement {
    fn key(&self) -> Vec<u64> {
        self.rotation.as_ref().map_or_else(
            || coherent::numbers([self.delta.x, self.delta.y, self.delta.z]),
            RigidPath::motion_key,
        )
    }

    fn speed(&self) -> f64 {
        self.rotation
            .as_ref()
            .map_or_else(|| length(self.delta), |path| path.speed)
    }
    fn interval_bounds(&self, bounds: Bounds, start: f64, end: f64) -> Result<Bounds, ModelError> {
        match &self.rotation {
            Some(path) => path.interval_bounds(start, end),
            None => {
                let shifted = Bounds {
                    min: add(bounds.min, scale(self.delta, start)),
                    max: add(bounds.max, scale(self.delta, start)),
                };
                swept_bounds(
                    shifted,
                    &Movement {
                        delta: scale(self.delta, end - start),
                        rounding_guard: self.rounding_guard,
                        rotation: None,
                    },
                )
            }
        }
    }
    fn place<'session>(
        &self,
        session: &'session Session,
        shape: &Shape<'_>,
        fraction: f64,
    ) -> Result<Shape<'session>, ModelError> {
        match &self.rotation {
            Some(path) => path.place(session, shape, fraction),
            None => Ok(session.translate(shape, scale(self.delta, fraction))?),
        }
    }
}
fn magnitude(point: Vec3) -> f64 {
    point.x.abs().max(point.y.abs()).max(point.z.abs())
}
fn finite(point: Vec3) -> bool {
    [point.x, point.y, point.z]
        .iter()
        .all(|value| value.is_finite())
}
fn frame_origin(graph: &InstanceGraph<'_>, frame: Option<&str>) -> Result<(Vec3, f64), ModelError> {
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
fn movements(
    start: &InstanceGraph<'_>,
    end: &InstanceGraph<'_>,
    outputs: &[InstanceOutputRef],
) -> Result<Vec<Movement>, ModelError> {
    let mut cache: HashMap<Option<&str>, Movement> = HashMap::new();
    outputs
        .iter()
        .map(|output| {
            let frame = start
                .nodes
                .get(&output.instance)
                .and_then(|node| node.frame());
            if let Some(movement) = cache.get(&frame) {
                return Ok(movement.clone());
            }
            let (first, first_guard) = frame_origin(start, frame)?;
            let (second, second_guard) = frame_origin(end, frame)?;
            let delta = subtract(second, first);
            let rounding_guard = first_guard + second_guard;
            if !finite(delta) || !length(delta).is_finite() || !rounding_guard.is_finite() {
                return Err(ModelError::new("continuous displacement bound overflows"));
            }
            let movement = Movement {
                delta,
                rounding_guard,
                rotation: None,
            };
            cache.insert(frame, movement.clone());
            Ok(movement)
        })
        .collect()
}
fn swept_bounds(bounds: Bounds, movement: &Movement) -> Result<Bounds, ModelError> {
    if let Some(path) = &movement.rotation {
        return Ok(path.swept);
    }
    let last_min = add(bounds.min, movement.delta);
    let last_max = add(bounds.max, movement.delta);
    if !finite(last_min) || !finite(last_max) {
        return Err(ModelError::new("continuous swept bounds overflow"));
    }
    Ok(Bounds {
        min: Vec3::new(
            bounds.min.x.min(last_min.x),
            bounds.min.y.min(last_min.y),
            bounds.min.z.min(last_min.z),
        ),
        max: Vec3::new(
            bounds.max.x.max(last_max.x),
            bounds.max.y.max(last_max.y),
            bounds.max.z.max(last_max.z),
        ),
    })
}
fn boxes_separated(first: Bounds, second: Bounds, margin: f64) -> bool {
    first.min.x - second.max.x > margin
        || second.min.x - first.max.x > margin
        || first.min.y - second.max.y > margin
        || second.min.y - first.max.y > margin
        || first.min.z - second.max.z > margin
        || second.min.z - first.max.z > margin
}
struct PairPath<'a, 'session> {
    first: &'a Body<'a, 'session>,
    second: &'a Body<'a, 'session>,
    first_motion: &'a Movement,
    second_motion: &'a Movement,
    collision_options: CollisionOptions,
    checked: (f64, f64),
    options: ContinuousCollisionOptions,
    guard: f64,
}
enum PairOutcome {
    Clear,
    Collision(f64, PairCheck),
    Unresolved([f64; 2]),
}
impl PairPath<'_, '_> {
    fn relative_speed(&self) -> f64 {
        if self.first_motion.key() == self.second_motion.key() {
            0.0
        } else if self.first_motion.rotation.is_none() && self.second_motion.rotation.is_none() {
            length(subtract(self.first_motion.delta, self.second_motion.delta))
        } else {
            self.first_motion.speed() + self.second_motion.speed()
        }
    }
    fn threshold(&self) -> f64 {
        self.checked.0.max(self.checked.1)
            + self.guard
            + self.first_motion.rounding_guard
            + self.second_motion.rounding_guard
    }
    fn inspect(
        &self,
        session: &Session,
        fraction: f64,
        queries: &mut usize,
    ) -> Result<Option<PairCheck>, ModelError> {
        if *queries >= self.options.maximum_queries {
            return Ok(None);
        }
        let first_shape = self
            .first_motion
            .place(session, self.first.shape, fraction)?;
        let second_shape = self
            .second_motion
            .place(session, self.second.shape, fraction)?;
        let first = Body {
            reference: self.first.reference,
            shape: &first_shape,
            bounds: self.first.bounds,
            volume: self.first.volume,
        };
        let second = Body {
            reference: self.second.reference,
            shape: &second_shape,
            bounds: self.second.bounds,
            volume: self.second.volume,
        };
        *queries += 1;
        Ok(Some(inspect_pair(
            session,
            &first,
            &second,
            self.collision_options,
            self.checked,
        )?))
    }
    fn check_endpoints(
        &self,
        session: &Session,
        queries: &mut usize,
        speed: f64,
    ) -> Result<Option<PairOutcome>, ModelError> {
        for fraction in [0.0, 1.0] {
            let Some(check) = self.inspect(session, fraction, queries)? else {
                return Ok(Some(PairOutcome::Unresolved([0.0, 1.0])));
            };
            if check.status != PairStatus::Clear {
                return Ok(Some(PairOutcome::Collision(fraction, check)));
            }
            if speed == 0.0 {
                return Ok(Some(if check.separation_mm > self.threshold() {
                    PairOutcome::Clear
                } else {
                    PairOutcome::Unresolved([0.0, 1.0])
                }));
            }
        }
        Ok(None)
    }
    fn check(
        &self,
        session: &Session,
        queries: &mut usize,
        rejected: &mut usize,
    ) -> Result<PairOutcome, ModelError> {
        let speed = self.relative_speed();
        if !speed.is_finite() {
            return Err(ModelError::new("continuous relative speed overflows"));
        }
        if let Some(outcome) = self.check_endpoints(session, queries, speed)? {
            return Ok(outcome);
        }
        let mut unresolved = None;
        let mut pending = vec![(0.0, 1.0, 0)];
        while let Some((start, end, depth)) = pending.pop() {
            let first = self
                .first_motion
                .interval_bounds(self.first.bounds, start, end)?;
            let second = self
                .second_motion
                .interval_bounds(self.second.bounds, start, end)?;
            if boxes_separated(first, second, self.threshold()) {
                *rejected += 1;
                continue;
            }
            let middle = start + (end - start) * 0.5;
            let Some(check) = self.inspect(session, middle, queries)? else {
                return Ok(PairOutcome::Unresolved([start, end]));
            };
            if check.status != PairStatus::Clear {
                return Ok(PairOutcome::Collision(middle, check));
            }
            let distance_bound = speed * ((end - start) * 0.5);
            if check.separation_mm > self.threshold() + distance_bound {
                continue;
            }
            if depth >= self.options.maximum_depth
                || end - start <= self.options.minimum_interval_fraction
            {
                unresolved.get_or_insert([start, end]);
                continue;
            }
            // Chronological subdivision. Report an observed witness, not a TOI.
            pending.push((middle, end, depth + 1));
            pending.push((start, middle, depth + 1));
        }
        Ok(unresolved.map_or(PairOutcome::Clear, PairOutcome::Unresolved))
    }
}
impl ContinuousMotionResult {
    fn record(
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
impl InstanceGraph<'_> {
    /// Checks all interpolated positions of translating bodies between samples.
    /// Angular coordinates must be constant along the study. Exact BREP queries
    /// plus a relative-displacement bound reject entire clear intervals; finite
    /// query/depth budgets report Unresolved, never silently Clear. Preserves the
    /// graph and accepted geometry, generating each local parameter variant once.
    pub fn check_translation_motion(
        &self,
        session: &Session,
        study: &MotionStudy,
        options: ContinuousCollisionOptions,
    ) -> Result<ContinuousMotionResult, ModelError> {
        self.check_motion(session, study, options, true)
    }
    /// Checks linearly interpolated joint coordinates, including unwrapped
    /// rotations and nested rotating/translating frames. Uses conservative speed
    /// bounds; ambiguous intervals or exhausted budgets remain Unresolved.
    pub fn check_continuous_motion(
        &self,
        session: &Session,
        study: &MotionStudy,
        options: ContinuousCollisionOptions,
    ) -> Result<ContinuousMotionResult, ModelError> {
        self.check_motion(session, study, options, false)
    }
    fn check_motion(
        &self,
        session: &Session,
        study: &MotionStudy,
        options: ContinuousCollisionOptions,
        translations_only: bool,
    ) -> Result<ContinuousMotionResult, ModelError> {
        let guard = options.checked()?;
        study.collision_options.validate()?;
        if !(2..=MAX_MOTION_SAMPLES).contains(&study.samples.len()) || study.outputs.is_empty() {
            return Err(ModelError::new(
                "continuous motion needs 2–10000 samples and solid outputs",
            ));
        }
        self.validate_joints()?;
        // Validate all coordinate endpoints before generating local geometry.
        let mut start = sample_graph(self, &study.samples[0], 0)?;
        for (index, sample) in study.samples.iter().enumerate().skip(1) {
            let end = sample_graph(self, sample, index)?;
            if translations_only {
                validate_translation(&start, &end)?;
            }
            rotations::validate_angles(&start, &end)?;
            start = end;
        }
        let (locals, bindings) = prepare_motion(session, self, &study.outputs)?;
        let mut result = ContinuousMotionResult {
            status: ContinuousStatus::Clear,
            pairs: vec![],
            segments: study.samples.len() - 1,
            candidate_pairs: 0,
            exact_queries: 0,
            bounds_rejected_intervals: 0,
            generated_variants: locals.len(),
            unresolved_pairs: 0,
        };
        start = sample_graph(self, &study.samples[0], 0)?;
        for (segment, sample) in study.samples.iter().enumerate().skip(1) {
            let end = sample_graph(self, sample, segment)?;
            let generation = motion_generation(session, &start, &bindings, &locals)?;
            let bodies = generation.bodies(session, &study.outputs)?;
            let movement = if rotations::has_rotation(&start, &end)? {
                rotations::movements(&start, &end, &study.outputs, &bodies)?
            } else {
                movements(&start, &end, &study.outputs)?
            };
            SegmentCheck {
                collision_options: study.collision_options,
                options,
                guard,
            }
            .inspect(session, segment - 1, &bodies, &movement, &mut result)?;
            start = end;
        }
        Ok(result)
    }
}
struct SegmentCheck {
    collision_options: CollisionOptions,
    options: ContinuousCollisionOptions,
    guard: f64,
}
impl SegmentCheck {
    fn inspect(
        &self,
        session: &Session,
        segment: usize,
        bodies: &[Body<'_, '_>],
        movements: &[Movement],
        result: &mut ContinuousMotionResult,
    ) -> Result<(), ModelError> {
        let Self {
            collision_options,
            options,
            guard,
        } = *self;
        if bodies.len() < 2 {
            return Ok(());
        }
        let checked = collision_options.checked()?;
        let swept = bodies
            .iter()
            .zip(movements)
            .map(|(body, movement)| {
                Ok(Body {
                    reference: body.reference,
                    shape: body.shape,
                    bounds: swept_bounds(body.bounds, movement)?,
                    volume: body.volume,
                })
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        let index = coherent::Index::new(bodies, &swept, movements);
        let bound_magnitude = swept
            .iter()
            .map(|body| magnitude(body.bounds.min).max(magnitude(body.bounds.max)))
            .fold(0.0, f64::max);
        let margin = bound_magnitude * (128.0 * f64::EPSILON)
            + checked.0.max(checked.1)
            + guard
            + movements
                .iter()
                .map(|value| value.rounding_guard)
                .fold(0.0, f64::max)
                * 2.0;
        if !margin.is_finite() {
            return Err(ModelError::new("continuous bounding margin overflows"));
        }
        let mut count = 0;
        for first in 0..bodies.len() {
            let mut candidates = Vec::new();
            index.query(first, bodies, &swept, margin, &mut candidates);
            candidates.sort_unstable();
            count += candidates.len();
            if count > options.maximum_candidate_pairs {
                return Err(ModelError::new(
                    "continuous candidate-pair budget exceeded; reduce the selection or split the study",
                ));
            }
            result.candidate_pairs += candidates.len();
            for second in candidates {
                let bound_magnitude = magnitude(bodies[first].bounds.min)
                    .max(magnitude(bodies[first].bounds.max))
                    .max(magnitude(bodies[second].bounds.min))
                    .max(magnitude(bodies[second].bounds.max));
                let path = PairPath {
                    first: &bodies[first],
                    second: &bodies[second],
                    first_motion: &movements[first],
                    second_motion: &movements[second],
                    collision_options,
                    checked,
                    options,
                    guard: guard + bound_magnitude * (128.0 * f64::EPSILON),
                };
                let outcome = path.check(
                    session,
                    &mut result.exact_queries,
                    &mut result.bounds_rejected_intervals,
                )?;
                result.record(
                    segment,
                    bodies[first].reference,
                    bodies[second].reference,
                    outcome,
                );
            }
        }
        Ok(())
    }
}
