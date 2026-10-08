//! Geometry-driven constant-section prisms, with exact bounded face coverage.
use super::*;
use crate::assembly::{dot, scale, subtract, unit};

pub(super) fn execute<'a>(
    session: &'a Session,
    profile: &Shape<'_>,
    direction: Vec3,
    extent: &ExtrudeExtent,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'a>>,
    definitions: &Features<'_>,
) -> Result<Shape<'a>, ModelError> {
    match extent {
        ExtrudeExtent::Distance => Ok(session.create_prism_from_face(profile, direction)?),
        ExtrudeExtent::Symmetric => {
            let prism = session.create_prism_from_face(profile, direction)?;
            let placed = session.translate(&prism, scale(direction, -0.5))?;
            Ok(session.compose_history(&placed, &prism)?)
        }
        ExtrudeExtent::UpToFace { target, face } => {
            let faces = resolve_face_selector(
                session,
                shape(shapes, target)?,
                face,
                parameters,
                shapes,
                definitions,
            )?;
            if faces.len() != 1 {
                return Err(ModelError::new(
                    "up-to-face must select exactly one limiting face",
                ));
            }
            let travel =
                travel_to_face(session, profile, &faces[0], direction)?.ok_or_else(|| {
                    ModelError::new(
                        "limiting face must be parallel, forward, and cover the entire profile",
                    )
                })?;
            Ok(session.create_prism_from_face(profile, travel)?)
        }
        ExtrudeExtent::UpToNext { target } => {
            // One topology traversal, then one exact coplanar intersection per
            // eligible face. No ray sampling or quadratic face-pair search.
            let faces = session.subshapes(shape(shapes, target)?, ShapeType::Face)?;
            let axis = unit(direction)?;
            let mut nearest: Option<Vec3> = None;
            for face in &faces {
                if let Some(travel) = travel_to_face(session, profile, face, direction)?
                    && nearest.is_none_or(|old| dot(travel, axis) < dot(old, axis))
                {
                    nearest = Some(travel);
                }
            }
            let travel = nearest.ok_or_else(|| {
                ModelError::new("no forward parallel face covers the entire extrusion profile")
            })?;
            Ok(session.create_prism_from_face(profile, travel)?)
        }
    }
}

fn travel_to_face(
    session: &Session,
    profile: &Shape<'_>,
    face: &Shape<'_>,
    direction: Vec3,
) -> Result<Option<Vec3>, ModelError> {
    let axis = unit(direction)?;
    if !session.face_is_planar(face)? || !session.is_valid(face)? {
        return Ok(None);
    }
    let normal = unit(session.face_normal(profile)?)?;
    let other_normal = unit(session.face_normal(face)?)?;
    if dot(normal, other_normal).abs() < 1.0 - 1e-10 {
        return Ok(None);
    }
    let denominator = dot(normal, axis);
    if denominator.abs() < 1e-10 {
        return Err(ModelError::new(
            "extrusion direction is parallel to the profile plane",
        ));
    }
    let separation = subtract(
        session.center_of_mass(face)?,
        session.center_of_mass(profile)?,
    );
    let length = dot(normal, separation) / denominator;
    if !length.is_finite() || length <= 1e-7 {
        return Ok(None);
    }
    let travel = scale(axis, length);
    let cap = session.translate(profile, travel)?;
    let covered = session.common(&cap, face)?;
    let area = session.surface_area(profile)?;
    if (session.surface_area(&covered)? - area).abs() > area * 1e-9 {
        return Ok(None);
    }
    Ok(Some(travel))
}
