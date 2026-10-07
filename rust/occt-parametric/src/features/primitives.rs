//! Additional round primitives over the existing bridge constructors.
use super::*;

/// O(1) expression-independent setup and returned topology/storage; construction
/// is a native primitive operation. Normalize direction by its largest component
/// to avoid overflow/underflow. Equal radii use the exact cylindrical limit.
pub(super) fn cone<'session>(
    session: &'session Session,
    origin: Vec3,
    axis: Vec3,
    base_radius: f64,
    top_radius: f64,
    height: f64,
) -> Result<Shape<'session>, ModelError> {
    if ![base_radius, top_radius, height]
        .iter()
        .all(|v| v.is_finite())
        || base_radius < 0.0
        || top_radius < 0.0
        || (base_radius == 0.0 && top_radius == 0.0)
        || height <= 0.0
    {
        return Err(ModelError::new(
            "cone needs nonnegative radii (not both zero) and positive finite height",
        ));
    }
    let maximum = axis.x.abs().max(axis.y.abs()).max(axis.z.abs());
    if maximum == 0.0 || ![axis.x, axis.y, axis.z].iter().all(|v| v.is_finite()) {
        return Err(ModelError::new("cone axis must be finite and nonzero"));
    }
    let direction = Vec3::new(axis.x / maximum, axis.y / maximum, axis.z / maximum);
    let length = direction.x.hypot(direction.y.hypot(direction.z));
    let direction = Vec3::new(
        direction.x / length,
        direction.y / length,
        direction.z / length,
    );
    if base_radius == top_radius {
        Ok(session.create_cylinder(origin, direction, base_radius, height)?)
    } else {
        Ok(session.create_cone(origin, direction, base_radius, top_radius, height)?)
    }
}
