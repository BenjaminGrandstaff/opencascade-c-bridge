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

/// A catalog standard part (schema 180) at its nominal dimensions, built in
/// place from cylinders, cones and hexagonal prisms. `origin` is the seat
/// and the part extends along `axis` (a screw head behind it). O(1) kernel
/// operations; intermediate handles are released on return.
pub(super) fn standard_part<'session>(
    session: &'session Session,
    part: StandardPartKind,
    nominal: f64,
    length: Option<f64>,
    origin: Vec3,
    axis: Vec3,
) -> Result<Shape<'session>, ModelError> {
    let data = part.data(nominal)?;
    let length = match (part.takes_length(), length) {
        (true, Some(length)) if length.is_finite() && length > 0.0 => length,
        (true, _) => {
            return Err(ModelError::new(format!(
                "{} needs a positive finite length",
                part.name()
            )));
        }
        (false, Some(_)) => {
            return Err(ModelError::new(format!("{} takes no length", part.name())));
        }
        (false, None) => 0.0,
    };
    let norm = (axis.x * axis.x + axis.y * axis.y + axis.z * axis.z).sqrt();
    if !norm.is_finite() || norm < 1e-12 {
        return Err(ModelError::new(
            "standard part axis must be finite and nonzero",
        ));
    }
    let w = Vec3::new(axis.x / norm, axis.y / norm, axis.z / norm);
    let seed = if w.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let cross = |a: Vec3, b: Vec3| {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    };
    let u = cross(w, seed);
    let un = (u.x * u.x + u.y * u.y + u.z * u.z).sqrt();
    let u = Vec3::new(u.x / un, u.y / un, u.z / un);
    let v = cross(w, u);
    let at = |along: f64| {
        Vec3::new(
            origin.x + w.x * along,
            origin.y + w.y * along,
            origin.z + w.z * along,
        )
    };
    let along = |d: f64| Vec3::new(w.x * d, w.y * d, w.z * d);
    // A hexagon across flats `s` in the plane through `center`, prismed by `depth`.
    let hexagon = |center: Vec3, s: f64, depth: f64| {
        let r = s / 3f64.sqrt();
        let points: Vec<Vec3> = (0..6)
            .map(|k| {
                let a = std::f64::consts::FRAC_PI_3 * k as f64;
                let (c, sn) = (r * a.cos(), r * a.sin());
                Vec3::new(
                    center.x + u.x * c + v.x * sn,
                    center.y + u.y * c + v.y * sn,
                    center.z + u.z * c + v.z * sn,
                )
            })
            .collect();
        session.create_polygon_prism(&points, along(depth))
    };
    let shape = match data {
        StandardPartData::Screw {
            diameter,
            head_diameter,
            head_height,
            socket_across_flats,
            socket_depth,
            ..
        } => {
            let head =
                session.create_cylinder(at(-head_height), w, head_diameter / 2.0, head_height)?;
            let shank = session.create_cylinder(origin, w, diameter / 2.0, length)?;
            let body = session.fuse(&head, &shank)?;
            let socket = hexagon(at(-head_height), socket_across_flats, socket_depth)?;
            session.cut(&body, &socket)?
        }
        StandardPartData::Nut {
            diameter,
            height,
            across_flats,
            ..
        } => {
            let prism = hexagon(origin, across_flats, height)?;
            let bore = session.create_cylinder(at(-1.0), w, diameter / 2.0, height + 2.0)?;
            session.cut(&prism, &bore)?
        }
        StandardPartData::Washer {
            inner_diameter,
            outer_diameter,
            thickness,
        } => {
            let disc = session.create_cylinder(origin, w, outer_diameter / 2.0, thickness)?;
            let hole =
                session.create_cylinder(at(-1.0), w, inner_diameter / 2.0, thickness + 2.0)?;
            session.cut(&disc, &hole)?
        }
        StandardPartData::Dowel { diameter, chamfer } => {
            if length <= 2.0 * chamfer {
                return Err(ModelError::new(format!(
                    "dowel pin length must exceed its two {chamfer} mm chamfers"
                )));
            }
            let r = diameter / 2.0;
            let start = session.create_cone(origin, w, r - chamfer, r, chamfer)?;
            let middle = session.create_cylinder(at(chamfer), w, r, length - 2.0 * chamfer)?;
            let end = session.create_cone(at(length - chamfer), w, r, r - chamfer, chamfer)?;
            let joined = session.fuse(&start, &middle)?;
            session.fuse(&joined, &end)?
        }
    };
    Ok(shape)
}
