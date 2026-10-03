//! Application-level geometry recipes composed over `occt-bridge`.

use occt_bridge::{BridgeError, Session, Shape, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionalLight {
    pub position: Vec3,
    pub color: Vec3,
    pub intensity: f64,
    pub range: f64,
    pub cast_shadows: bool,
}

#[derive(Debug, PartialEq)]
pub struct WallTorch<'session> {
    pub fixture: Shape<'session>,
    pub flame: Shape<'session>,
    pub light: PositionalLight,
}

/// Builds a wall torch by composing generic cylinders, cones, and compounds.
pub fn create_wall_torch<'a>(
    session: &'a Session,
    wall_anchor: Vec3,
    wall_normal: Vec3,
    scale: f64,
) -> Result<WallTorch<'a>, BridgeError> {
    if !finite(wall_anchor) || !finite(wall_normal) || !scale.is_finite() || scale <= 0.0 {
        return Err(invalid_argument("invalid wall-torch parameters"));
    }
    let horizontal_length = wall_normal.x.hypot(wall_normal.y);
    if horizontal_length <= f64::EPSILON {
        return Err(invalid_argument("wall normal must have an XY component"));
    }
    let outward = Vec3::new(
        wall_normal.x / horizontal_length,
        wall_normal.y / horizontal_length,
        0.0,
    );
    let up = Vec3::new(0.0, 0.0, 1.0);

    let plate = session.create_cylinder(wall_anchor, outward, 16.0 * scale, 6.0 * scale)?;
    let arm_start = Vec3::new(
        wall_anchor.x + outward.x * 5.0 * scale,
        wall_anchor.y + outward.y * 5.0 * scale,
        wall_anchor.z - 7.0 * scale,
    );
    let arm_end = Vec3::new(
        wall_anchor.x + outward.x * 49.0 * scale,
        wall_anchor.y + outward.y * 49.0 * scale,
        wall_anchor.z + 4.0 * scale,
    );
    let arm_axis = subtract(arm_end, arm_start);
    let arm = session.create_cylinder(arm_start, arm_axis, 4.0 * scale, magnitude(arm_axis))?;
    let stem_start = Vec3::new(arm_end.x, arm_end.y, arm_end.z - 18.0 * scale);
    let stem = session.create_cylinder(stem_start, up, 4.5 * scale, 35.0 * scale)?;
    let cup_base = Vec3::new(arm_end.x, arm_end.y, wall_anchor.z + 12.0 * scale);
    let cup = session.create_cone(cup_base, up, 8.0 * scale, 15.0 * scale, 18.0 * scale)?;
    let fixture = session.create_compound(&[&plate, &arm, &stem, &cup])?;
    session.remove(plate)?;
    session.remove(arm)?;
    session.remove(stem)?;
    session.remove(cup)?;

    let flame_base = Vec3::new(arm_end.x, arm_end.y, wall_anchor.z + 30.0 * scale);
    let lower_flame =
        session.create_cone(flame_base, up, 11.0 * scale, 4.0 * scale, 25.0 * scale)?;
    let upper_flame_base = Vec3::new(flame_base.x, flame_base.y, flame_base.z + 17.0 * scale);
    let upper_flame = session.create_cone(upper_flame_base, up, 6.0 * scale, 0.0, 22.0 * scale)?;
    let flame = session.create_compound(&[&lower_flame, &upper_flame])?;
    session.remove(lower_flame)?;
    session.remove(upper_flame)?;

    Ok(WallTorch {
        fixture,
        flame,
        light: PositionalLight {
            position: Vec3::new(flame_base.x, flame_base.y, flame_base.z + 14.0 * scale),
            color: Vec3::new(1.0, 0.32, 0.06),
            intensity: 2_000_000.0,
            range: 0.0,
            cast_shadows: false,
        },
    })
}

/// Sewing tolerance for recipe facets, matching the legacy constructor.
const FACET_TOLERANCE: f64 = 1e-6;
/// Quarter-round steps between the side wall and the top ring.
const SHOULDER_SEGMENTS: usize = 5;

/// Builds a faceted natural-stone solid from generic faces, sewing, and
/// shell-to-solid construction.
///
/// Bottom and top rings must have the same point count and corresponding
/// winding. The bottom ring must be planar; top points and `top_center` may
/// have different Z values. `bottom_chamfer` insets the base toward the
/// outline's centroid and `top_fillet` rounds the top edge in quarter-round
/// steps; either may be zero. Every intermediate handle is released, so only
/// the returned solid remains in the session.
pub fn create_faceted_stone<'a>(
    session: &'a Session,
    bottom_points: &[Vec3],
    top_points: &[Vec3],
    top_center: Vec3,
    bottom_chamfer: f64,
    top_fillet: f64,
) -> Result<Shape<'a>, BridgeError> {
    let valid_treatment = |value: f64| value.is_finite() && value >= 0.0;
    if bottom_points.len() < 3
        || bottom_points.len() != top_points.len()
        || !bottom_points
            .iter()
            .chain(top_points)
            .all(|point| finite(*point))
        || !finite(top_center)
        || !valid_treatment(bottom_chamfer)
        || !valid_treatment(top_fillet)
    {
        return Err(invalid_argument("invalid faceted-stone parameters"));
    }
    let (base, lower_side) = chamfer_rings(bottom_points, bottom_chamfer)?;
    let rings = shoulder_rings(top_points, top_center, top_fillet)?;

    let mut facets = vec![base.clone()];
    if bottom_chamfer > 0.0 {
        add_ring_band(&mut facets, &base, &lower_side);
    }
    add_ring_band(&mut facets, &lower_side, &rings[0]);
    for pair in rings.windows(2) {
        add_ring_band(&mut facets, &pair[0], &pair[1]);
    }
    let crown = rings.last().expect("at least one shoulder ring");
    for (index, point) in crown.iter().enumerate() {
        facets.push(vec![*point, crown[(index + 1) % crown.len()], top_center]);
    }
    solid_from_facets(session, &facets)
}

/// The base ring inset toward the centroid by the chamfer, and the side ring
/// raised by it. Without a chamfer both are the bottom ring.
fn chamfer_rings(bottom: &[Vec3], chamfer: f64) -> Result<(Vec<Vec3>, Vec<Vec3>), BridgeError> {
    if chamfer <= 0.0 {
        return Ok((bottom.to_vec(), bottom.to_vec()));
    }
    let count = bottom.len() as f64;
    let center_x = bottom.iter().map(|point| point.x).sum::<f64>() / count;
    let center_y = bottom.iter().map(|point| point.y).sum::<f64>() / count;
    let mut base = Vec::with_capacity(bottom.len());
    let mut lower_side = Vec::with_capacity(bottom.len());
    for point in bottom {
        let (dx, dy) = (center_x - point.x, center_y - point.y);
        let length = dx.hypot(dy);
        if length <= chamfer {
            return Err(invalid_geometry(
                "bottom chamfer does not fit inside the outline",
            ));
        }
        base.push(Vec3::new(
            point.x + chamfer * dx / length,
            point.y + chamfer * dy / length,
            point.z,
        ));
        lower_side.push(Vec3::new(point.x, point.y, point.z + chamfer));
    }
    Ok((base, lower_side))
}

/// Rings stepping a quarter round from the side wall into the top ring, or
/// the top ring alone without a fillet.
fn shoulder_rings(top: &[Vec3], center: Vec3, fillet: f64) -> Result<Vec<Vec<Vec3>>, BridgeError> {
    if fillet <= 0.0 {
        return Ok(vec![top.to_vec()]);
    }
    (0..=SHOULDER_SEGMENTS)
        .map(|step| {
            let angle = std::f64::consts::FRAC_PI_2 * step as f64 / SHOULDER_SEGMENTS as f64;
            let horizontal = fillet * angle.cos();
            let vertical = fillet * (1.0 - angle.sin());
            top.iter()
                .map(|point| {
                    let (dx, dy) = (point.x - center.x, point.y - center.y);
                    let length = dx.hypot(dy);
                    if length <= fillet {
                        return Err(invalid_geometry(
                            "top fillet does not fit inside the top ring",
                        ));
                    }
                    Ok(Vec3::new(
                        point.x + horizontal * dx / length,
                        point.y + horizontal * dy / length,
                        point.z - vertical,
                    ))
                })
                .collect()
        })
        .collect()
}

/// Two triangles per segment between corresponding points of two rings.
fn add_ring_band(facets: &mut Vec<Vec<Vec3>>, lower: &[Vec3], upper: &[Vec3]) {
    for index in 0..lower.len() {
        let next = (index + 1) % lower.len();
        facets.push(vec![lower[index], lower[next], upper[next]]);
        facets.push(vec![lower[index], upper[next], upper[index]]);
    }
}

/// Builds planar faces, sews them, and closes the shell into a solid,
/// releasing every intermediate handle on success and failure.
fn solid_from_facets<'a>(
    session: &'a Session,
    facets: &[Vec<Vec3>],
) -> Result<Shape<'a>, BridgeError> {
    let mut faces = Vec::with_capacity(facets.len());
    for facet in facets {
        match planar_face(session, facet) {
            Ok(face) => faces.push(face),
            Err(error) => {
                release(session, faces);
                return Err(invalid_geometry(&format!(
                    "faceted stone facet failed: {}",
                    error.message
                )));
            }
        }
    }
    let sewn = session.sew(&faces.iter().collect::<Vec<_>>(), FACET_TOLERANCE);
    release(session, faces);
    let shell = sewn?;
    let solid = session.make_solid(&shell);
    let _ = session.remove(shell);
    solid
}

fn planar_face<'a>(session: &'a Session, points: &[Vec3]) -> Result<Shape<'a>, BridgeError> {
    let wire = session.create_polyline_wire(points, true)?;
    let face = session.create_face_from_wire(&wire);
    let _ = session.remove(wire);
    face
}

fn release(session: &Session, shapes: Vec<Shape<'_>>) {
    for shape in shapes {
        let _ = session.remove(shape);
    }
}

fn finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}

fn subtract(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(left.x - right.x, left.y - right.y, left.z - right.z)
}

fn magnitude(value: Vec3) -> f64 {
    value.x.hypot(value.y.hypot(value.z))
}

fn invalid_geometry(message: &str) -> BridgeError {
    BridgeError {
        status: 4,
        category: "invalid geometry".into(),
        message: message.into(),
        diagnostics: Vec::new(),
    }
}

fn invalid_argument(message: &str) -> BridgeError {
    BridgeError {
        status: 1,
        category: "invalid argument".into(),
        message: message.into(),
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
