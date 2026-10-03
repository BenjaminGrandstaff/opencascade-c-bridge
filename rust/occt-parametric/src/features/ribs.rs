//! Bounded, closed-profile reinforcing walls joined to one body.

use super::*;
use crate::assembly::{dot, scale, unit};

pub(super) fn execute_rib<'session>(
    session: &'session Session,
    input: &Shape<'_>,
    profile: &Shape<'_>,
    thickness: f64,
    direction: Vec3,
) -> Result<Shape<'session>, ModelError> {
    if thickness <= 0.0 || !thickness.is_finite() {
        return Err(ModelError::new("rib thickness must be finite and positive"));
    }
    let direction = unit(direction)?;
    if session.subshape_count(input, ShapeType::Solid)? != 1 || !session.is_valid(input)? {
        return Err(ModelError::new("rib input must contain one valid solid"));
    }
    let temporary = match session.shape_type(profile)? {
        ShapeType::Wire => Some(session.create_face_from_wire(profile)?),
        ShapeType::Face => None,
        _ => {
            return Err(ModelError::new(
                "rib profile must be a planar face or closed planar wire",
            ));
        }
    };
    let face = temporary.as_ref().unwrap_or(profile);
    if !session.face_is_planar(face)? || !session.is_valid(face)? {
        return Err(ModelError::new(
            "rib profile must define a valid planar face",
        ));
    }
    let normal = unit(session.face_normal(face)?)?;
    if dot(normal, direction).abs() < 1.0 - 1e-9 {
        return Err(ModelError::new(
            "rib direction must be normal to the profile plane",
        ));
    }
    let wall = session.create_prism_from_face(face, scale(direction, thickness))?;
    if session.subshape_count(&wall, ShapeType::Solid)? != 1
        || !session.is_valid(&wall)?
        || session.volume(&wall)? <= 0.0
    {
        return Err(ModelError::new(
            "rib extrusion must produce one valid solid with positive volume",
        ));
    }
    let before = session.volume(input)?;
    let result = session.fuse(input, &wall)?;
    if session.subshape_count(&result, ShapeType::Solid)? != 1
        || !session.is_valid(&result)?
        || session.volume(&result)? <= before
    {
        return Err(ModelError::new(
            "rib must add material and join the input as one valid solid",
        ));
    }
    Ok(result)
}
