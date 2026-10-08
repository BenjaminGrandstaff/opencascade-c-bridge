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
            Ok(to_face(session, profile, &faces[0], direction)?.solid)
        }
        ExtrudeExtent::UpToNext { target } => {
            Ok(to_next(session, profile, shape(shapes, target)?, direction)?.solid)
        }
    }
}

pub(super) fn to_face<'a>(
    session: &'a Session,
    profile: &Shape<'_>,
    face: &Shape<'_>,
    direction: Vec3,
) -> Result<Candidate<'a>, ModelError> {
    limited_face(session, profile, face, direction)?.ok_or_else(|| {
        ModelError::new("limiting face must terminate the whole profile strictly forward")
    })
}

pub(super) fn to_next<'a>(
    session: &'a Session,
    profile: &Shape<'_>,
    target: &Shape<'_>,
    direction: Vec3,
) -> Result<Candidate<'a>, ModelError> {
    let faces = session.subshapes(target, ShapeType::Face)?;
    let mut candidates = Vec::new();
    for face in &faces {
        if let Some(candidate) = limited_face(session, profile, face, direction)? {
            candidates.push(candidate);
        }
    }
    let nearest = candidates
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.volume.total_cmp(&b.volume))
        .map(|(i, _)| i)
        .ok_or_else(|| ModelError::new("no forward face terminates the whole profile"))?;
    let chosen = &candidates[nearest];
    // A least-volume cutoff must also precede every other complete cutoff.
    // One linear scan and at most N native containment intersections.
    for (index, other) in candidates.iter().enumerate() {
        if index == nearest || (chosen.uniform && other.uniform) {
            continue;
        }
        let overlap = session.overlap_volume(&chosen.solid, &other.solid)?;
        if (overlap - chosen.volume).abs() > chosen.volume * 1e-9 {
            return Err(ModelError::new(
                "next-face limits cross; select an explicit up-to-face limit",
            ));
        }
    }
    Ok(candidates.swap_remove(nearest))
}

pub(super) struct Candidate<'a> {
    pub solid: Shape<'a>,
    pub limiting_face: Shape<'a>,
    volume: f64,
    uniform: bool,
}

fn limited_face<'a>(
    session: &'a Session,
    profile: &Shape<'_>,
    face: &Shape<'_>,
    direction: Vec3,
) -> Result<Option<Candidate<'a>>, ModelError> {
    let axis = unit(direction)?;
    let normal = unit(session.face_normal(profile)?)?;
    if dot(normal, axis).abs() < 1e-10 {
        return Err(ModelError::new(
            "extrusion direction is parallel to the profile plane",
        ));
    }
    // For principal-axis travel the two transverse coordinates are invariant.
    // Exact profile extrema must fit the conservative target face box. This
    // cheaply rejects side faces without replacing the native coverage check.
    let transverse = if axis.x == 0.0 && axis.y == 0.0 {
        Some([0, 1])
    } else if axis.x == 0.0 && axis.z == 0.0 {
        Some([0, 2])
    } else if axis.y == 0.0 && axis.z == 0.0 {
        Some([1, 2])
    } else {
        None
    };
    if let Some(indices) = transverse {
        let profile_bounds = session.exact_bounds(profile)?;
        let target_bounds = session.bounds(face)?;
        let p_min = [
            profile_bounds.min.x,
            profile_bounds.min.y,
            profile_bounds.min.z,
        ];
        let p_max = [
            profile_bounds.max.x,
            profile_bounds.max.y,
            profile_bounds.max.z,
        ];
        let t_min = [
            target_bounds.min.x,
            target_bounds.min.y,
            target_bounds.min.z,
        ];
        let t_max = [
            target_bounds.max.x,
            target_bounds.max.y,
            target_bounds.max.z,
        ];
        if indices
            .into_iter()
            .any(|i| p_min[i] < t_min[i] - 1e-7 || p_max[i] > t_max[i] + 1e-7)
        {
            return Ok(None);
        }
    }
    let planar = session.face_is_planar(face)?;
    let face_normal = if planar {
        Some(unit(session.face_normal(face)?)?)
    } else {
        None
    };
    if face_normal.is_some_and(|n| dot(n, axis).abs() < 1e-10) {
        return Ok(None);
    }
    let uniform = face_normal.is_some_and(|n| dot(normal, n).abs() >= 1.0 - 1e-10);
    let solid = if uniform {
        let Some(travel) = travel_to_face(session, profile, face, direction)? else {
            return Ok(None);
        };
        session.create_prism_from_face(profile, travel)?
    } else {
        let target_bounds = session.bounds(face)?;
        let profile_bounds = session.bounds(profile)?;
        let range = |bounds: occt_bridge::Bounds| {
            let minimum = dot(
                Vec3::new(
                    if axis.x >= 0.0 {
                        bounds.min.x
                    } else {
                        bounds.max.x
                    },
                    if axis.y >= 0.0 {
                        bounds.min.y
                    } else {
                        bounds.max.y
                    },
                    if axis.z >= 0.0 {
                        bounds.min.z
                    } else {
                        bounds.max.z
                    },
                ),
                axis,
            );
            let maximum = dot(
                Vec3::new(
                    if axis.x >= 0.0 {
                        bounds.max.x
                    } else {
                        bounds.min.x
                    },
                    if axis.y >= 0.0 {
                        bounds.max.y
                    } else {
                        bounds.min.y
                    },
                    if axis.z >= 0.0 {
                        bounds.max.z
                    } else {
                        bounds.min.z
                    },
                ),
                axis,
            );
            (minimum, maximum)
        };
        let length = range(target_bounds).1 - range(profile_bounds).0;
        if length <= 1e-7 {
            return Ok(None);
        }
        let search = scale(axis, length + length.abs().mul_add(0.01, 1.0));
        match session.create_prism_until_face(profile, search, face) {
            Ok(solid) => solid,
            Err(error) if error.status == 4 => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    };
    let volume = session.volume(&solid)?;
    Ok(Some(Candidate {
        solid,
        limiting_face: session.subshape(face, ShapeType::Face, 0)?,
        volume,
        uniform,
    }))
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
