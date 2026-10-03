//! Vector helpers and relationship residuals between resolved datums.

use super::*;

// ---- relationship geometry

pub(crate) type Residuals = (Option<f64>, Option<f64>);

pub(crate) fn residuals(
    kind: RelationKind,
    first: ResolvedDatum,
    second: ResolvedDatum,
) -> Result<Residuals, ModelError> {
    match kind {
        RelationKind::Coincident => coincident(first, second),
        RelationKind::Parallel => directional(first, second, false),
        RelationKind::Perpendicular => directional(first, second, true),
        RelationKind::Distance(value) => {
            if value.dimension != Dimension::Length {
                return Err(ModelError::new("relationship distance must be a length"));
            }
            let target = value.normalized()?;
            if target.is_nan() || target < 0.0 {
                return Err(ModelError::new("relationship distance must be nonnegative"));
            }
            let (measured, angular) = separation(first, second)?;
            Ok((Some((measured - target).abs()), angular))
        }
    }
}

pub(crate) fn coincident(
    first: ResolvedDatum,
    second: ResolvedDatum,
) -> Result<Residuals, ModelError> {
    use ResolvedDatum::{Axis, Plane};
    Ok(match (first, second) {
        (
            Axis { origin, direction },
            Plane {
                origin: plane,
                normal,
            },
        )
        | (
            Plane {
                origin: plane,
                normal,
            },
            Axis { origin, direction },
        ) => (
            Some(dot(subtract(origin, plane), normal).abs()),
            Some(perpendicular_angle(direction, normal)),
        ),
        _ => separation(first, second).map(|(linear, angular)| (Some(linear), angular))?,
    })
}

/// Distance between two datums, with the angular deviation from parallel
/// for axis and plane pairs whose distance is only defined when parallel.
pub(crate) fn separation(
    first: ResolvedDatum,
    second: ResolvedDatum,
) -> Result<(f64, Option<f64>), ModelError> {
    use ResolvedDatum::{Axis, Plane, Point};
    Ok(match (first, second) {
        (Point { origin: p }, Point { origin: q }) => (length(subtract(p, q)), None),
        (Point { origin: p }, Axis { origin, direction })
        | (Axis { origin, direction }, Point { origin: p }) => {
            (length(cross(subtract(p, origin), direction)), None)
        }
        (Point { origin: p }, Plane { origin, normal })
        | (Plane { origin, normal }, Point { origin: p }) => {
            (dot(subtract(p, origin), normal).abs(), None)
        }
        (
            Axis {
                origin: a,
                direction: d,
            },
            Axis {
                origin: b,
                direction: e,
            },
        ) => (length(cross(subtract(b, a), d)), Some(parallel_angle(d, e))),
        (
            Plane {
                origin: a,
                normal: n,
            },
            Plane {
                origin: b,
                normal: m,
            },
        ) => (dot(subtract(b, a), n).abs(), Some(parallel_angle(n, m))),
        (Axis { .. }, Plane { .. }) | (Plane { .. }, Axis { .. }) => {
            return Err(ModelError::new(
                "distance between an axis and a plane is not supported; use coincident",
            ));
        }
    })
}

/// Parallel or perpendicular intent between axis directions and plane
/// normals. An axis is parallel to a plane when it is perpendicular to the
/// plane normal, and the reverse.
pub(crate) fn directional(
    first: ResolvedDatum,
    second: ResolvedDatum,
    perpendicular: bool,
) -> Result<Residuals, ModelError> {
    let direction = |datum| match datum {
        ResolvedDatum::Axis { direction, .. } => Ok((direction, false)),
        ResolvedDatum::Plane { normal, .. } => Ok((normal, true)),
        ResolvedDatum::Point { .. } => Err(ModelError::new(
            "parallel and perpendicular relationships require axes or planes",
        )),
    };
    let (u, first_is_plane) = direction(first)?;
    let (v, second_is_plane) = direction(second)?;
    // For a mixed axis/plane pair the plane normal flips the sense.
    let wants_perpendicular = perpendicular != (first_is_plane != second_is_plane);
    let angle = if wants_perpendicular {
        perpendicular_angle(u, v)
    } else {
        parallel_angle(u, v)
    };
    Ok((None, Some(angle)))
}

pub(crate) fn parallel_angle(u: Vec3, v: Vec3) -> f64 {
    length(cross(u, v)).atan2(dot(u, v).abs())
}

pub(crate) fn perpendicular_angle(u: Vec3, v: Vec3) -> f64 {
    dot(u, v).abs().atan2(length(cross(u, v)))
}

// ---- vector math

pub(crate) fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

pub(crate) fn subtract(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

pub(crate) fn scale(a: Vec3, factor: f64) -> Vec3 {
    Vec3::new(a.x * factor, a.y * factor, a.z * factor)
}

pub(crate) fn length(a: Vec3) -> f64 {
    a.x.hypot(a.y.hypot(a.z))
}

pub(crate) fn unit(a: Vec3) -> Result<Vec3, ModelError> {
    let size = length(a);
    if size > f64::EPSILON {
        Ok(scale(a, 1.0 / size))
    } else {
        Err(ModelError::new("datum direction must be nonzero"))
    }
}

/// Rodrigues rotation of a direction by the placement's axis-angle, if any.
pub(crate) fn rotate_by(value: Vec3, placement: &NormalizedPlacement) -> Vec3 {
    let Some((_, axis, angle)) = placement.rotation else {
        return value;
    };
    let axis = scale(axis, 1.0 / length(axis));
    let (sin, cos) = angle.sin_cos();
    add(
        add(scale(value, cos), scale(cross(axis, value), sin)),
        scale(axis, dot(axis, value) * (1.0 - cos)),
    )
}

/// Rotates a point about the placement's axis through its origin, then translates.
pub(crate) fn transform_point(point: Vec3, placement: &NormalizedPlacement) -> Vec3 {
    let rotated = match placement.rotation {
        Some((origin, _, _)) => add(origin, rotate_by(subtract(point, origin), placement)),
        None => point,
    };
    add(rotated, placement.translation)
}
