//! Swept spheres and point-speed bounds through nested rigid frame paths.
use super::*;
use crate::regeneration::place_shape;

pub(super) fn validate_angles(
    start: &InstanceGraph<'_>,
    end: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    for (frame, joint) in &start.assembly.joints {
        if !(angle(&end.assembly.joints[frame])? - angle(joint)?).is_finite() {
            return Err(ModelError::new("continuous angular displacement overflows"));
        }
    }
    Ok(())
}
pub(super) fn has_rotation(
    start: &InstanceGraph<'_>,
    end: &InstanceGraph<'_>,
) -> Result<bool, ModelError> {
    for (frame, joint) in &start.assembly.joints {
        if angle(joint)? != angle(&end.assembly.joints[frame])? {
            return Ok(true);
        }
    }
    Ok(false)
}
#[derive(Clone)]
struct Step {
    start: NormalizedPlacement,
    translation_delta: Vec3,
    angular_delta: f64,
    pivot: Vec3,
    axis: Vec3,
}
impl Step {
    fn new(start: NormalizedPlacement, end: NormalizedPlacement) -> Self {
        let (pivot, axis, _) = start.rotation.or(end.rotation).unwrap_or((
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            0.0,
        ));
        Self {
            start,
            translation_delta: subtract(end.translation, start.translation),
            angular_delta: end.rotation.map_or(0.0, |(_, _, angle)| angle)
                - start.rotation.map_or(0.0, |(_, _, angle)| angle),
            pivot,
            axis,
        }
    }
    fn at(&self, fraction: f64) -> NormalizedPlacement {
        let angle =
            self.start.rotation.map_or(0.0, |(_, _, angle)| angle) + self.angular_delta * fraction;
        NormalizedPlacement {
            translation: add(
                self.start.translation,
                scale(self.translation_delta, fraction),
            ),
            rotation: (angle != 0.0).then_some((self.pivot, self.axis, angle)),
        }
    }
}
#[derive(Clone)]
pub(super) struct RigidPath {
    inverse: Vec<NormalizedPlacement>,
    steps: Vec<Step>,
    pub(super) swept: Bounds,
    pub(super) speed: f64,
    rounding_guard: f64,
}
fn inverse_chain(steps: &[Step]) -> Vec<NormalizedPlacement> {
    let mut inverse = Vec::new();
    for step in steps.iter().rev() {
        inverse.push(NormalizedPlacement {
            translation: scale(step.start.translation, -1.0),
            rotation: None,
        });
        if let Some((pivot, axis, angle)) = step.start.rotation {
            inverse.push(NormalizedPlacement {
                translation: Vec3::new(0.0, 0.0, 0.0),
                rotation: Some((pivot, axis, -angle)),
            });
        }
    }
    inverse
}
impl RigidPath {
    fn new(start: &[Placement], end: &[Placement], bounds: Bounds) -> Result<Self, ModelError> {
        let steps = start
            .iter()
            .zip(end)
            .map(|(first, second)| Ok(Step::new(first.normalized()?, second.normalized()?)))
            .collect::<Result<Vec<_>, ModelError>>()?;
        let inverse = inverse_chain(&steps);
        let mut center = add(scale(bounds.min, 0.5), scale(bounds.max, 0.5));
        let mut radius = length(subtract(bounds.max, bounds.min)) * 0.5;
        for placement in &inverse {
            center = transform_point(center, placement);
        }
        let mut speed = 0.0;
        let mut rounding_guard = 0.0;
        for step in &steps {
            // Input sphere encloses all child poses. Rigid rotation preserves
            // its radius; the moving center travels at most 2*lever across an
            // arbitrary turn, or |delta angle|*lever for a shorter arc.
            let lever = length(subtract(center, step.pivot));
            let translation = length(step.translation_delta);
            let first_angle = step.start.rotation.map_or(0.0, |(_, _, angle)| angle);
            let angle_magnitude = first_angle
                .abs()
                .max((first_angle + step.angular_delta).abs())
                + step.angular_delta.abs();
            rounding_guard += angle_magnitude * (128.0 * f64::EPSILON) * (lever + radius);
            speed += step.angular_delta.abs() * (lever + radius) + translation;
            radius += step.angular_delta.abs().min(2.0) * lever + translation;
            center = transform_point(center, &step.start);
        }
        let extent = Vec3::new(radius, radius, radius);
        let swept = Bounds {
            min: subtract(center, extent),
            max: add(center, extent),
        };
        if !finite(swept.min)
            || !finite(swept.max)
            || !speed.is_finite()
            || !rounding_guard.is_finite()
        {
            return Err(ModelError::new(
                "continuous rotational motion bound overflows",
            ));
        }
        Ok(Self {
            inverse,
            steps,
            swept,
            speed,
            rounding_guard,
        })
    }
    pub(super) fn place<'session>(
        &self,
        session: &'session Session,
        shape: &Shape<'_>,
        fraction: f64,
    ) -> Result<Shape<'session>, ModelError> {
        let mut placed = session.translate(shape, Vec3::new(0.0, 0.0, 0.0))?;
        if fraction == 0.0 {
            return Ok(placed);
        }
        for placement in self
            .inverse
            .iter()
            .copied()
            .chain(self.steps.iter().map(|step| step.at(fraction)))
        {
            if placement.rotation.is_some() || !placement.translation_is_zero() {
                placed = place_shape(session, &placed, &placement)?;
            }
        }
        Ok(placed)
    }
}
pub(super) fn movements(
    start: &InstanceGraph<'_>,
    end: &InstanceGraph<'_>,
    outputs: &[InstanceOutputRef],
    bodies: &[Body<'_, '_>],
) -> Result<Vec<Movement>, ModelError> {
    outputs
        .iter()
        .zip(bodies)
        .map(|(output, body)| {
            let frame = start
                .nodes
                .get(&output.instance)
                .and_then(|node| node.frame());
            let first = start.frame_chain(frame)?;
            let second = end.frame_chain(frame)?;
            let (origin_start, a) = frame_origin(start, frame)?;
            let (origin_end, b) = frame_origin(end, frame)?;
            let path = RigidPath::new(&first, &second, body.bounds)?;
            if path.steps.iter().all(|step| step.angular_delta == 0.0) {
                let delta = subtract(origin_end, origin_start);
                if !finite(delta) || !length(delta).is_finite() || !(a + b).is_finite() {
                    return Err(ModelError::new("continuous displacement bound overflows"));
                }
                return Ok(Movement {
                    delta,
                    rounding_guard: a + b,
                    rotation: None,
                });
            }
            let guard = a
                + b
                + path.rounding_guard
                + magnitude(path.swept.min).max(magnitude(path.swept.max))
                    * (128.0 * f64::EPSILON)
                    * (first.len() + 1) as f64;
            if !guard.is_finite() {
                return Err(ModelError::new(
                    "continuous rotational rounding margin overflows",
                ));
            }
            Ok(Movement {
                delta: Vec3::new(0.0, 0.0, 0.0),
                rounding_guard: guard,
                rotation: Some(path),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/rotational_bounds.rs"]
mod tests;
