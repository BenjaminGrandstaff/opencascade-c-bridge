//! Modeled screw threads: an ISO 68-1 basic-profile groove swept along a
//! helix and cut from the input.
use super::*;
use crate::assembly::{cross, unit};

/// Basic profile dimensions for a 60° thread of `pitch` and major diameter.
/// The fundamental triangle height is H = √3/2 · P; the basic thread depth
/// from the major to the minor diameter is 5H/8.
pub(crate) fn basic_depth(pitch: f64) -> f64 {
    5.0 / 8.0 * 3f64.sqrt() / 2.0 * pitch
}

/// The groove a thread removes, in (radius, axial offset) coordinates about
/// the helix start, as a closed trapezoid listed counterclockwise, with its
/// in-material part's area and centroid radius.
///
/// External threads cut from the major radius in to the minor radius: the
/// groove is 7P/8 wide at the major radius (leaving the P/8 crest flat) and
/// P/4 wide at the minor radius (the root flat). Internal threads cut a hole
/// of minor diameter out to the major radius: 3P/4 wide at the minor radius
/// (leaving the P/4 crest flat) and P/8 at the major radius. Each groove
/// overshoots P/16 into free space along its 60° flanks so the cutting tool
/// never shares a face with the part's surface; the overshoot removes no
/// material and keeps the groove narrower than the pitch at its widest
/// (0.95 P outside, 0.82 P inside), so successive turns of the swept tool
/// never overlap each other.
pub(crate) fn groove(major_radius: f64, pitch: f64, internal: bool) -> ([[f64; 2]; 4], f64, f64) {
    let depth = basic_depth(pitch);
    let minor_radius = major_radius - depth;
    let overshoot = pitch / 16.0;
    let flank = 30f64.to_radians().tan();
    // Half-widths at the minor and major radius.
    let (minor_half, major_half) = if internal {
        (3.0 * pitch / 8.0, pitch / 16.0)
    } else {
        (pitch / 8.0, 7.0 * pitch / 16.0)
    };
    // Extend the free-space end of the groove along its flanks.
    let (inner, inner_half, outer, outer_half) = if internal {
        (
            minor_radius - overshoot,
            minor_half + overshoot * flank,
            major_radius,
            major_half,
        )
    } else {
        (
            minor_radius,
            minor_half,
            major_radius + overshoot,
            major_half + overshoot * flank,
        )
    };
    let points = [
        [inner, -inner_half],
        [outer, -outer_half],
        [outer, outer_half],
        [inner, inner_half],
    ];
    // In-material part: the trapezoid between the minor and major radius.
    let (a, b, h) = (2.0 * minor_half, 2.0 * major_half, depth);
    let area = (a + b) / 2.0 * h;
    let centroid = minor_radius + h * (a + 2.0 * b) / (3.0 * (a + b));
    (points, area, centroid)
}

/// A thread on `input` about the axis through `origin`, running `length`
/// along `axis` from `origin`. Turns are length / pitch, at most 10,000.
pub(crate) struct ThreadGeometry {
    pub origin: Vec3,
    pub axis: Vec3,
    pub major_diameter: f64,
    pub pitch: f64,
    pub length: f64,
    pub internal: bool,
    pub left_handed: bool,
}

/// Sweeps the groove along a helix at its centroid radius in binormal mode
/// (a screw motion that keeps the groove in planes through the axis) and
/// cuts it from `input`. Intermediate shapes are released on every path.
/// O(turns) sweep and boolean work.
pub(crate) fn execute_thread<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    thread: ThreadGeometry,
) -> Result<Shape<'session>, ModelError> {
    let ThreadGeometry {
        origin,
        axis,
        major_diameter,
        pitch,
        length: run,
        internal,
        left_handed,
    } = thread;
    let finite = [major_diameter, pitch, run].iter().all(|v| v.is_finite());
    if !finite || major_diameter <= 0.0 || pitch <= 0.0 || run <= 0.0 {
        return Err(ModelError::new(
            "thread needs a positive finite major diameter, pitch and length",
        ));
    }
    let turns = run / pitch;
    if turns > 10_000.0 {
        return Err(ModelError::new("thread is limited to 10,000 turns"));
    }
    let major_radius = major_diameter / 2.0;
    if basic_depth(pitch) >= 0.8 * major_radius {
        return Err(ModelError::new(
            "thread pitch is too coarse for its diameter: the minor diameter would vanish",
        ));
    }
    let axis = unit(axis)?;
    let seed = if axis.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let radial = unit(cross(cross(axis, seed), axis))?;
    let (points, _, centroid) = groove(major_radius, pitch, internal);
    let place = |[rho, z]: [f64; 2]| {
        Vec3::new(
            origin.x + radial.x * rho + axis.x * z,
            origin.y + radial.y * rho + axis.y * z,
            origin.z + radial.z * rho + axis.z * z,
        )
    };
    let section = session.create_polyline_wire(&points.map(place), true)?;
    let face = session.create_face_from_wire(&section)?;
    let path = session.create_helix_wire(occt_bridge::HelixOptions {
        origin,
        axis,
        start_direction: radial,
        radius: centroid,
        pitch,
        turns,
        left_handed,
    })?;
    let groove = session.sweep(&face, &path, occt_bridge::SweepOrientation::Binormal(axis))?;
    Ok(session.cut(input, &groove)?)
}
