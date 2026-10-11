//! Planar splits by half-space booleans: each kept side is the input cut by
//! a plane-aligned prism covering the other side of the input's bounds. O(1)
//! native operations (one cut per kept side); cuts keep input history. The
//! plane-crossing check uses tight bounds of a rotated copy rather than
//! volume integration, which costs several times a cut on bored parts.
use super::*;
use crate::assembly::{add, cross, dot, scale, subtract, unit};

/// Relative tolerance for the plane crossing the input (of its size) and for
/// volume conservation, well above boolean noise and far below any real piece.
const TOLERANCE: f64 = 1e-7;

/// Least and greatest `p · n` over the input, from tight native bounds of a
/// copy rotated so the unit `n` points along +z. Rotation only relocates.
fn extent_along(session: &Session, input: &Shape<'_>, n: Vec3) -> Result<(f64, f64), ModelError> {
    let z = Vec3::new(0.0, 0.0, 1.0);
    let axis = cross(n, z);
    let sine = dot(axis, axis).sqrt();
    let bounds = if sine < 1e-12 && n.z > 0.0 {
        session.exact_bounds(input)?
    } else {
        let (axis, angle) = if sine < 1e-12 {
            (Vec3::new(1.0, 0.0, 0.0), std::f64::consts::PI)
        } else {
            (scale(axis, 1.0 / sine), sine.atan2(n.z))
        };
        let turned = session.rotate(input, Vec3::new(0.0, 0.0, 0.0), axis, angle)?;
        session.exact_bounds(&turned)?
    };
    Ok((bounds.min.z, bounds.max.z))
}

pub(super) fn execute<'session>(
    session: &'session Session,
    input: &Shape<'_>,
    origin: Vec3,
    normal: Vec3,
    keep: SplitKeep,
) -> Result<Shape<'session>, ModelError> {
    if session.subshape_count(input, ShapeType::Solid)? == 0 || !session.is_valid(input)? {
        return Err(ModelError::new("split input must contain valid solids"));
    }
    let normal = unit(normal).map_err(|_| ModelError::new("split normal must be nonzero"))?;
    let bounds = session.exact_bounds(input)?;
    let center = scale(add(bounds.min, bounds.max), 0.5);
    let diagonal = subtract(bounds.max, bounds.min);
    // Far enough that the prism covers every point of the input on its side.
    let size = dot(diagonal, diagonal).sqrt();
    let reach = size + dot(subtract(center, origin), subtract(center, origin)).sqrt() + 1.0;
    // A regular solid has material on both sides exactly when its extent
    // along the normal straddles the plane.
    let (low, high) = extent_along(session, input, normal)?;
    let level = dot(origin, normal);
    let margin = TOLERANCE * size.max(1.0);
    if !(low < level - margin && high > level + margin) {
        return Err(ModelError::new(
            "split plane must cross the input, leaving material on both sides",
        ));
    }
    let seed = if normal.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u = unit(cross(normal, seed))?;
    let v = cross(normal, u);
    let corner = |a: f64, b: f64| add(origin, add(scale(u, a * reach), scale(v, b * reach)));
    // The half-space on the `sign` side of the plane, within reach.
    let half_space = |sign: f64| -> Result<Shape<'session>, ModelError> {
        let square = session.create_polyline_wire(
            &[
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ],
            true,
        )?;
        let face = session.create_face_from_wire(&square)?;
        Ok(session.create_prism_from_face(&face, scale(normal, sign * reach))?)
    };
    // The side `sign` keeps: the input with the opposite half-space removed.
    let side = |sign: f64| -> Result<Shape<'session>, ModelError> {
        let piece = session.cut(input, &half_space(-sign)?)?;
        if session.subshape_count(&piece, ShapeType::Solid)? == 0 || !session.is_valid(&piece)? {
            return Err(ModelError::new("split produced an empty or invalid piece"));
        }
        Ok(piece)
    };
    match keep {
        SplitKeep::Positive => side(1.0),
        SplitKeep::Negative => side(-1.0),
        SplitKeep::Both => {
            let (positive, negative) = (side(1.0)?, side(-1.0)?);
            let volume = session.volume(input)?;
            let (a, b) = (session.volume(&positive)?, session.volume(&negative)?);
            if (a + b - volume).abs() > volume * TOLERANCE {
                return Err(ModelError::new(format!(
                    "split pieces do not conserve volume ({} of {volume} mm³)",
                    a + b
                )));
            }
            Ok(session.create_compound(&[&positive, &negative])?)
        }
    }
}
