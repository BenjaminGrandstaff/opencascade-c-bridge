//! Rigid in-plane motions of position zones: the datum shift a frame's
//! material boundaries allow (schema 137 for a secondary axis; width and
//! rotational shift for slot and tab datums), and the feature-relating fit of
//! a composite pattern.
//!
//! A motion translates the true positions by `t` in the measured frame's x-y
//! plane and turns them by `angle` about its z axis (the primary datum
//! normal) through the frame origin or the pattern centroid. One min-max LP
//! chooses the motion: each member's residual stays within its zone radius
//! plus the largest excess, inside the motions the datums allow. Rotation is
//! relinearized about each step; circles are circumscribed 256-gons (within
//! 0.008% of optimal) and a 1e-6 weight on |t|₁ and the lever-scaled |angle|
//! picks the smallest of near-optimal motions. An active set (the four worst
//! members, then the worst outside member) keeps each LP small; the min-max
//! is fixed by at most four members. Conformance is decided exactly at the
//! returned motion. O(iterations · linearizations · pivots · (active · 256 +
//! datum points) + n).
use super::*;

const SIDES: usize = 256;
const LINEARIZATIONS: usize = 8;
/// Largest rotation step per linearization, radians.
const STEP: f64 = 0.05;

/// A member: its offset from true position, its true position relative to
/// the center of rotation, and its allowed zone radius.
#[derive(Clone, Copy, Debug)]
pub(super) struct PatternMember {
    pub offset: [f64; 2],
    pub lever: [f64; 2],
    pub radius: f64,
}
impl PatternMember {
    /// Offset left after translating the true positions by `t` and turning
    /// them by `angle` about the center of rotation.
    pub fn residual(&self, motion: Motion) -> [f64; 2] {
        let turned = turn(self.lever, motion.angle);
        [
            self.offset[0] - motion.t[0] - (turned[0] - self.lever[0]),
            self.offset[1] - motion.t[1] - (turned[1] - self.lever[1]),
        ]
    }
    fn excess(&self, motion: Motion) -> f64 {
        let r = self.residual(motion);
        r[0].hypot(r[1]) - self.radius
    }
}

/// Translation (mm) and rotation (radians) of a group of true positions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Motion {
    pub t: [f64; 2],
    pub angle: f64,
}

/// A width datum feature of size referenced at a material boundary: the
/// boundary pair of parallel planes, centered on the datum's basic center
/// plane, moves with the frame and must stay clear of the measured faces.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct WidthBoundary {
    /// Unit normal and offset (`normal·x`) of the basic center plane in the
    /// measured frame before any motion.
    pub normal: [f64; 2],
    pub offset: f64,
    /// Half the boundary's width.
    pub half_width: f64,
    /// The measured faces stay outside the pair (a slot at MMB, a tab at
    /// LMB) or inside it (a tab at MMB, a slot at LMB).
    pub outside: bool,
    /// Measured points in the frame and their side: +1 toward `normal`.
    pub points: Vec<([f64; 2], f64)>,
    /// Each point's position along a primary datum axis (the frame's z).
    pub along: Vec<f64>,
}
impl WidthBoundary {
    /// Signed distance of `p` from the moved center plane, and its gradient
    /// in (tx, ty, angle).
    fn distance(&self, p: [f64; 2], motion: Motion) -> (f64, [f64; 3]) {
        let n = turn(self.normal, motion.angle);
        let d = [p[0] - motion.t[0], p[1] - motion.t[1]];
        let value = n[0] * d[0] + n[1] * d[1] - self.offset;
        (value, [-n[0], -n[1], n[0] * d[1] - n[1] * d[0]])
    }
}

/// The motions a datum frame's material boundaries allow.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ShiftSpace {
    /// Largest radial translation of a secondary axis datum.
    pub disk: Option<f64>,
    /// In-plane unit directions along which the frame cannot translate.
    /// Each with the translation along it the frame must take (nonzero
    /// only where a datum locates the frame at its measured place).
    pub fixed: Vec<([f64; 2], f64)>,
    pub rotation_fixed: bool,
    pub widths: Vec<WidthBoundary>,
    /// Largest translation of the frame origin the boundaries allow with the
    /// frame square to its mating envelopes.
    pub limit: f64,
    /// A primary datum axis feature's measured extent along the axis, where
    /// the simulator's ends float within `disk`.
    pub span: Option<[f64; 2]>,
    pub pins: Vec<PinBoundary>,
    /// For a common datum axis at a material boundary, in place of `disk`:
    /// where the axis must pass (basic `[0, 0]`) within each member's
    /// clearance of that member's own axis, at both ends of its extent.
    pub axis_bounds: Vec<PinBoundary>,
    /// A primary datum axis with no clocking datum: the frame turns freely
    /// about it.
    pub turn_free: bool,
}

/// A clocking hole or pin referenced at a material boundary: the boundary
/// cylinder at the datum's basic location moves with the frame and must
/// stay within `limit` of the measured envelope's axis (half the datum's
/// departure from the boundary; exact for a datum of perfect form).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PinBoundary {
    /// Basic and measured (mating, or minimum-material at LMB) axis
    /// positions in the measured frame before any motion.
    pub basic: [f64; 2],
    pub center: [f64; 2],
    pub limit: f64,
    /// Position along a primary datum axis (the frame's z).
    pub along: f64,
}
impl PinBoundary {
    /// Offset of the moved boundary from the measured axis, with the turn
    /// rate of the boundary's position.
    fn offset(&self, t: [f64; 2], angle: f64) -> ([f64; 2], [f64; 2]) {
        let q = turn(self.basic, angle);
        (
            [q[0] + t[0] - self.center[0], q[1] + t[1] - self.center[1]],
            [-q[1], q[0]],
        )
    }
}

/// Bound of a 256-gon inscribed in a disk of radius `limit`.
fn inscribed(limit: f64) -> f64 {
    limit * (std::f64::consts::PI / SIDES as f64).cos()
}
impl ShiftSpace {
    /// Free translation, and rotation when `rotate`: a composite pattern's
    /// feature-relating segment.
    pub fn free(rotate: bool) -> Self {
        Self {
            rotation_fixed: !rotate,
            ..Self::default()
        }
    }
}

/// The motion of `members` within `space` minimizing the largest excess of
/// any member over its zone (see the module documentation).
pub(super) fn fit_motion(members: &[PatternMember], space: &ShiftSpace) -> Motion {
    if members.is_empty() {
        return Motion::default();
    }
    let zero = Motion::default();
    let mut order: Vec<usize> = (0..members.len()).collect();
    order.sort_by(|&a, &b| members[b].excess(zero).total_cmp(&members[a].excess(zero)));
    let mut active: Vec<usize> = order.into_iter().take(4).collect();
    loop {
        let subset: Vec<PatternMember> = active.iter().map(|&i| members[i]).collect();
        let motion = motion_lp(&subset, space);
        let active_worst = subset
            .iter()
            .map(|m| m.excess(motion))
            .fold(f64::NEG_INFINITY, f64::max);
        let (worst, value) = members
            .iter()
            .enumerate()
            .map(|(i, m)| (i, m.excess(motion)))
            .fold((0, f64::NEG_INFINITY), |a, b| if b.1 > a.1 { b } else { a });
        if value <= active_worst + 1e-12 || active.contains(&worst) {
            return motion;
        }
        active.push(worst);
    }
}

fn motion_lp(members: &[PatternMember], space: &ShiftSpace) -> Motion {
    let directions: Vec<[f64; 2]> = (0..SIDES)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / SIDES as f64;
            [a.cos(), a.sin()]
        })
        .collect();
    let lever = members
        .iter()
        .map(|m| m.lever[0].hypot(m.lever[1]))
        .fold(1.0, f64::max);
    let mut motion = Motion::default();
    for _ in 0..LINEARIZATIONS {
        // Variables: Δtx, Δty, Δangle, z, wx ≥ |tx|, wy ≥ |ty|, wa ≥ |angle|.
        let mut rows: Vec<f64> = Vec::new();
        let mut bounds: Vec<f64> = Vec::new();
        let mut push = |row: [f64; 7], bound: f64| {
            rows.extend(row);
            bounds.push(bound);
        };
        for m in members {
            // Residual ≈ e − Δt − Δangle·q.
            let e = m.residual(motion);
            let turned = turn(m.lever, motion.angle);
            let q = [-turned[1], turned[0]];
            for u in &directions {
                let along = u[0] * q[0] + u[1] * q[1];
                push(
                    [-u[0], -u[1], -along, -1.0, 0.0, 0.0, 0.0],
                    m.radius - u[0] * e[0] - u[1] * e[1],
                );
            }
        }
        let step = if space.rotation_fixed { 0.0 } else { STEP };
        for sign in [1.0, -1.0] {
            push([0.0, 0.0, sign, 0.0, 0.0, 0.0, 0.0], step);
        }
        for (f, target) in &space.fixed {
            let along = f[0] * motion.t[0] + f[1] * motion.t[1];
            for sign in [1.0, -1.0] {
                push(
                    [sign * f[0], sign * f[1], 0.0, 0.0, 0.0, 0.0, 0.0],
                    sign * (target - along),
                );
            }
        }
        if let Some(limit) = space.disk {
            for u in &directions {
                push(
                    [u[0], u[1], 0.0, 0.0, 0.0, 0.0, 0.0],
                    limit - u[0] * motion.t[0] - u[1] * motion.t[1],
                );
            }
        }
        for width in &space.widths {
            for &(p, side) in &width.points {
                let (value, gradient) = width.distance(p, motion);
                // outside: side·(value + g·Δ) ≥ h; inside: side·(…) ≤ h.
                let sign = if width.outside { -side } else { side };
                push(
                    [
                        sign * gradient[0],
                        sign * gradient[1],
                        sign * gradient[2],
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                    ],
                    if width.outside {
                        side * value - width.half_width
                    } else {
                        width.half_width - side * value
                    },
                );
            }
        }
        for pin in &space.pins {
            let (e, q) = pin.offset(motion.t, motion.angle);
            for u in &directions {
                push(
                    [u[0], u[1], u[0] * q[0] + u[1] * q[1], 0.0, 0.0, 0.0, 0.0],
                    inscribed(pin.limit) - u[0] * e[0] - u[1] * e[1],
                );
            }
        }
        for (axis, current) in [(0, motion.t[0]), (1, motion.t[1]), (2, motion.angle)] {
            for sign in [1.0, -1.0] {
                let mut row = [0.0; 7];
                row[axis] = sign;
                row[4 + axis] = -1.0;
                push(row, -sign * current);
            }
        }
        let objective = [0.0, 0.0, 0.0, -1.0, -1e-6, -1e-6, -1e-6 * lever];
        let Ok(x) = fit::maximize(&objective, &rows, &bounds) else {
            break;
        };
        motion.t = [motion.t[0] + x[0], motion.t[1] + x[1]];
        motion.angle += x[2];
        if x[..3].iter().all(|v| v.abs() < 1e-12) {
            break;
        }
    }
    if space.rotation_fixed {
        motion.angle = 0.0;
    }
    if let Some(limit) = space.disk {
        let length = motion.t[0].hypot(motion.t[1]);
        if length > limit && length > 0.0 {
            motion.t = [motion.t[0] * limit / length, motion.t[1] * limit / length];
        }
    }
    motion
}

/// A feature located to a primary datum axis: each end of its axis as an
/// offset from the nearest point of its zone axis (components along
/// `across`, two unit vectors square to the zone) with that point in the
/// frame, and its allowed zone radius. For a feature parallel to the datum
/// axis `across` is the frame's x and y.
#[derive(Clone, Copy, Debug)]
pub(super) struct AxialMember {
    pub ends: [([f64; 2], [f64; 3]); 2],
    pub across: [[f64; 3]; 2],
    pub radius: f64,
}
impl AxialMember {
    /// Each end's offset from its zone axis after the motion: the datum
    /// axis ends (at `span`) move by `motion.ends`, which translates the zone
    /// by their interpolation at its height and tilts it about the frame
    /// origin, and the frame turns by `motion.angle` about the axis.
    fn residuals(&self, motion: AxisMotion, span: [f64; 2]) -> [[f64; 2]; 2] {
        let tilt = motion.tilt(span);
        self.ends.map(|(offset, q)| {
            let t = motion.at(q[2], span);
            let turned = turn([q[0], q[1]], motion.angle);
            let moved = [
                t[0] + turned[0] - q[0],
                t[1] + turned[1] - q[1],
                -(tilt[0] * q[0] + tilt[1] * q[1]),
            ];
            [
                offset[0] - dot3(moved, self.across[0]),
                offset[1] - dot3(moved, self.across[1]),
            ]
        })
    }
    /// Diameter of the smallest zone about the moved true position axis
    /// holding both ends.
    pub fn deviation(&self, motion: AxisMotion, span: [f64; 2]) -> f64 {
        let r = self.residuals(motion, span);
        2.0 * r[0][0].hypot(r[0][1]).max(r[1][0].hypot(r[1][1]))
    }
    fn excess(&self, motion: AxisMotion, span: [f64; 2]) -> f64 {
        0.5 * self.deviation(motion, span) - self.radius
    }
    /// Distance of the zone from the datum axis, the lever arm of a turn.
    fn lever(&self) -> f64 {
        self.ends
            .iter()
            .map(|(_, q)| q[0].hypot(q[1]))
            .fold(0.0, f64::max)
    }
    /// The turn about the datum axis carrying the zone point farthest from
    /// it onto the measured end there.
    fn clocking(&self) -> f64 {
        let [first, second] = self.ends;
        let (offset, q) = if first.1[0].hypot(first.1[1]) >= second.1[0].hypot(second.1[1]) {
            first
        } else {
            second
        };
        let [a, b] = self.across;
        let end = [
            q[0] + offset[0] * a[0] + offset[1] * b[0],
            q[1] + offset[0] * a[1] + offset[1] * b[1],
        ];
        (q[0] * end[1] - q[1] * end[0]).atan2(q[0] * end[0] + q[1] * end[1])
    }
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Motion of a frame on a primary datum axis: the offsets of the datum
/// axis's ends (at the datum feature's measured extent) and the turn about
/// the axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct AxisMotion {
    pub ends: [[f64; 2]; 2],
    pub angle: f64,
}
impl AxisMotion {
    /// Translation of the axis at position `z` along it.
    fn at(&self, z: f64, span: [f64; 2]) -> [f64; 2] {
        let w = weight(z, span);
        [
            (1.0 - w) * self.ends[0][0] + w * self.ends[1][0],
            (1.0 - w) * self.ends[0][1] + w * self.ends[1][1],
        ]
    }
    /// Change in the axis's direction per unit length along it.
    fn tilt(&self, span: [f64; 2]) -> [f64; 2] {
        let length = span[1] - span[0];
        [
            (self.ends[1][0] - self.ends[0][0]) / length,
            (self.ends[1][1] - self.ends[0][1]) / length,
        ]
    }
    /// Largest movement of either end of the datum axis.
    pub fn translation(&self) -> f64 {
        self.ends[0][0]
            .hypot(self.ends[0][1])
            .max(self.ends[1][0].hypot(self.ends[1][1]))
    }
    /// Size of the rotation: the axis's tilt combined with the turn about
    /// it, signed by the turn.
    pub fn rotation(&self, span: [f64; 2]) -> f64 {
        let tilt = self.tilt(span);
        tilt[0]
            .hypot(tilt[1])
            .hypot(self.angle)
            .copysign(self.angle)
    }
}

fn weight(z: f64, span: [f64; 2]) -> f64 {
    (z - span[0]) / (span[1] - span[0])
}

/// Datum shift in a frame on a primary datum axis, shared by the controls
/// that reference it: the simulator's ends, at the datum feature's
/// measured extent, each move within the axis's material-boundary limit
/// (`space.disk`) of the mating (or minimum-material) axis, which
/// translates and tilts the axis; a clocking width at a material boundary
/// turns the frame about the axis while its boundary pair stays clear of
/// the measured faces; without a clocking datum the frame turns freely,
/// starting from the turn that clocks the member farthest from the axis.
/// Tilting the axis carries the frame about its origin, which also moves a
/// zone at an angle to the axis along it. Exact for datum features of
/// perfect form; form error
/// is not used to restrict the motion further. The same min-max as
/// [`fit_motion`] in five variables, relinearized in the turn, with an
/// active set started from the five worst members. O(iterations ·
/// linearizations · pivots · (active · 512 + datum points) + n).
pub(super) fn fit_axis_motion(members: &[AxialMember], space: &ShiftSpace) -> AxisMotion {
    let zero = AxisMotion::default();
    let Some(span) = space.span else {
        return zero;
    };
    let turns = !space.rotation_fixed && members.iter().any(|m| m.lever() > 1e-9);
    if members.is_empty() || (space.disk.is_none() && space.axis_bounds.is_empty() && !turns) {
        return zero;
    }
    // A frame free to turn starts from the turn that clocks the member
    // farthest from the axis onto its measurement.
    let start = AxisMotion {
        angle: if space.turn_free && turns {
            members
                .iter()
                .max_by(|a, b| a.lever().total_cmp(&b.lever()))
                .map_or(0.0, AxialMember::clocking)
        } else {
            0.0
        },
        ..zero
    };
    let mut order: Vec<usize> = (0..members.len()).collect();
    order.sort_by(|&a, &b| {
        members[b]
            .excess(start, span)
            .total_cmp(&members[a].excess(start, span))
    });
    let mut active: Vec<usize> = order.into_iter().take(5).collect();
    loop {
        let subset: Vec<AxialMember> = active.iter().map(|&i| members[i]).collect();
        let motion = axis_motion_lp(&subset, space, span, start);
        let active_worst = subset
            .iter()
            .map(|m| m.excess(motion, span))
            .fold(f64::NEG_INFINITY, f64::max);
        let (worst, value) = members
            .iter()
            .enumerate()
            .map(|(i, m)| (i, m.excess(motion, span)))
            .fold((0, f64::NEG_INFINITY), |a, b| if b.1 > a.1 { b } else { a });
        if value <= active_worst + 1e-12 || active.contains(&worst) {
            return motion;
        }
        active.push(worst);
    }
}

fn axis_motion_lp(
    members: &[AxialMember],
    space: &ShiftSpace,
    span: [f64; 2],
    start: AxisMotion,
) -> AxisMotion {
    let directions: Vec<[f64; 2]> = (0..SIDES)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / SIDES as f64;
            [a.cos(), a.sin()]
        })
        .collect();
    let lever = members.iter().map(AxialMember::lever).fold(1.0, f64::max);
    let limit = space.disk.unwrap_or(0.0);
    let mut motion = start;
    for _ in 0..LINEARIZATIONS {
        // Variables: Δe0x, Δe0y, Δe1x, Δe1y, Δangle, z, then w ≥ |e| per
        // component and wa ≥ |angle|.
        let mut rows: Vec<f64> = Vec::new();
        let mut bounds: Vec<f64> = Vec::new();
        let mut push = |row: [f64; 11], bound: f64| {
            rows.extend(row);
            bounds.push(bound);
        };
        // A translation of the axis at height z, as a row over the end
        // variables, plus a turn coefficient.
        let row = |g: [f64; 2], w: f64, turn: f64, z: f64| {
            [
                (1.0 - w) * g[0],
                (1.0 - w) * g[1],
                w * g[0],
                w * g[1],
                turn,
                z,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
            ]
        };
        let length = span[1] - span[0];
        for m in members {
            let residuals = m.residuals(motion, span);
            for ((_, q), e) in m.ends.iter().zip(residuals) {
                let w = weight(q[2], span);
                let turned = turn([q[0], q[1]], motion.angle);
                for u in &directions {
                    // The residual's direction in the frame; its height
                    // component sees the tilt about the origin.
                    let g: [f64; 3] =
                        std::array::from_fn(|i| u[0] * m.across[0][i] + u[1] * m.across[1][i]);
                    let along = g[0] * -turned[1] + g[1] * turned[0];
                    let lift = [g[2] * q[0] / length, g[2] * q[1] / length];
                    let mut r = row([-g[0], -g[1]], w, -along, -1.0);
                    r[0] -= lift[0];
                    r[1] -= lift[1];
                    r[2] += lift[0];
                    r[3] += lift[1];
                    push(r, m.radius - u[0] * e[0] - u[1] * e[1]);
                }
            }
        }
        let step = if space.rotation_fixed { 0.0 } else { STEP };
        for sign in [1.0, -1.0] {
            push(row([0.0; 2], 0.0, sign, 0.0), step);
        }
        for end in 0..2 {
            if !space.axis_bounds.is_empty() {
                break;
            }
            for u in &directions {
                let mut r = [0.0; 11];
                r[2 * end] = u[0];
                r[2 * end + 1] = u[1];
                push(
                    r,
                    limit - u[0] * motion.ends[end][0] - u[1] * motion.ends[end][1],
                );
            }
        }
        for width in &space.widths {
            for (&(p, side), &z) in width.points.iter().zip(&width.along) {
                let t = motion.at(z, span);
                let (value, gradient) = width.distance(
                    p,
                    Motion {
                        t,
                        angle: motion.angle,
                    },
                );
                let sign = if width.outside { -side } else { side };
                push(
                    row(
                        [sign * gradient[0], sign * gradient[1]],
                        weight(z, span),
                        sign * gradient[2],
                        0.0,
                    ),
                    if width.outside {
                        side * value - width.half_width
                    } else {
                        width.half_width - side * value
                    },
                );
            }
        }
        for pin in space.pins.iter().chain(&space.axis_bounds) {
            let (e, q) = pin.offset(motion.at(pin.along, span), motion.angle);
            for u in &directions {
                push(
                    row(
                        [u[0], u[1]],
                        weight(pin.along, span),
                        u[0] * q[0] + u[1] * q[1],
                        0.0,
                    ),
                    inscribed(pin.limit) - u[0] * e[0] - u[1] * e[1],
                );
            }
        }
        let current = [
            motion.ends[0][0],
            motion.ends[0][1],
            motion.ends[1][0],
            motion.ends[1][1],
            motion.angle,
        ];
        for (axis, value) in current.into_iter().enumerate() {
            for sign in [1.0, -1.0] {
                let mut r = [0.0; 11];
                r[axis] = sign;
                r[6 + axis] = -1.0;
                push(r, -sign * value);
            }
        }
        let mut objective = [-1e-6; 11];
        objective[..5].fill(0.0);
        objective[5] = -1.0;
        objective[10] = -1e-6 * lever;
        let Ok(x) = fit::maximize(&objective, &rows, &bounds) else {
            break;
        };
        motion.ends = [
            [motion.ends[0][0] + x[0], motion.ends[0][1] + x[1]],
            [motion.ends[1][0] + x[2], motion.ends[1][1] + x[3]],
        ];
        motion.angle += x[4];
        if x[..5].iter().all(|v| v.abs() < 1e-12) {
            break;
        }
    }
    if space.rotation_fixed {
        motion.angle = 0.0;
    }
    motion.ends = motion.ends.map(|e| {
        let length = e[0].hypot(e[1]);
        if space.axis_bounds.is_empty() && length > limit && length > 0.0 {
            [e[0] * limit / length, e[1] * limit / length]
        } else {
            e
        }
    });
    motion
}

fn turn(v: [f64; 2], angle: f64) -> [f64; 2] {
    let (s, c) = angle.sin_cos();
    [c * v[0] - s * v[1], s * v[0] + c * v[1]]
}

impl Context<'_, '_> {
    /// Motions the material boundaries of `datums` allow, in `measured`
    /// (the frame on the mating envelopes); cached per datum precedence.
    /// Frames are a primary plane with a secondary axis and an optional
    /// tertiary plane or width, or three planes whose secondary and tertiary
    /// may be widths. A secondary width at a boundary lets the frame turn, so
    /// its tertiary must be a width at a boundary too: a contacting plane or
    /// a regardless-of-size width would have to re-contact as it turns.
    pub(super) fn shift_space(
        &mut self,
        datums: &[DrawingDatumReference],
        measured: &DrawingDatumCoordinateFrame,
        nominal: &DrawingDatumCoordinateFrame,
    ) -> Result<Rc<ShiftSpace>, String> {
        let key: SpaceKey = datums
            .iter()
            .map(|d| {
                (
                    d.members().map(str::to_owned).collect(),
                    d.boundary,
                    d.translation,
                )
            })
            .collect();
        if let Some(cached) = self.spaces.get(&key) {
            return cached.clone();
        }
        let result = self
            .build_shift_space(datums, measured, nominal)
            .map(Rc::new);
        self.spaces.insert(key, result.clone());
        result
    }

    fn build_shift_space(
        &mut self,
        datums: &[DrawingDatumReference],
        measured: &DrawingDatumCoordinateFrame,
        nominal: &DrawingDatumCoordinateFrame,
    ) -> Result<ShiftSpace, String> {
        let simulators = self.simulators(datums)?;
        if let Some(Simulator::Axis(.., limit)) = simulators.first() {
            return self.axis_space(datums, &simulators, *limit, measured, nominal);
        }
        let axis_frame = simulators.iter().any(|s| matches!(s, Simulator::Axis(..)));
        let in_plane = |n: Vec3| [dot(n, measured.x_axis), dot(n, measured.y_axis)];
        let mut space = ShiftSpace::default();
        let mut locating = Vec::new();
        // A secondary hole or pin freed by ▷ no longer locates the frame; a
        // tertiary face then locates it along its normal.
        let mut free_secondary = false;
        for (i, (datum, simulator)) in datums.iter().zip(&simulators).enumerate().skip(1) {
            let shifts = datum.boundary != DatumMaterialBoundary::Regardless;
            // ▷ frees a boundary from its basic location. At RFS the
            // simulators already engage the feature where it is.
            if shifts && datum.translation {
                match simulator {
                    // A secondary hole or pin no longer locates the frame.
                    Simulator::Axis(..) if i == 1 => {
                        free_secondary = true;
                        continue;
                    }
                    // A tertiary width after a fixed secondary no longer
                    // locates along its normal.
                    Simulator::Plane(_, _, Some(_))
                        if i == 2 && !axis_frame && space.rotation_fixed =>
                    {
                        continue;
                    }
                    _ => {
                        return Err(
                            "a translation modifier is evaluated on a secondary hole or pin, or a tertiary width after a fixed secondary, at a material boundary"
                                .into(),
                        );
                    }
                }
            }
            if free_secondary && (shifts || matches!(simulator, Simulator::Axis(..))) {
                return Err(
                    "after a translated secondary hole or pin, a tertiary face regardless of material boundary locates the frame"
                        .into(),
                );
            }
            match simulator {
                Simulator::Axis(origin, ..) if i == 2 => {
                    // A tertiary hole or pin clocks the plane-axis frame.
                    if shifts {
                        let pin =
                            self.pin_boundary(datum, simulator, *origin, measured, nominal)?;
                        space.pins.push(pin);
                    } else {
                        space.rotation_fixed = true;
                    }
                }
                Simulator::Axis(.., limit) if *limit > 0.0 => {
                    space.disk = Some(*limit);
                    space.limit += limit;
                }
                Simulator::Axis(..) => space.fixed.extend([([1.0, 0.0], 0.0), ([0.0, 1.0], 0.0)]),
                Simulator::Plane(point, normal, width) => {
                    // Shift moves the frame within the primary's plane; a
                    // datum tilted out of square with it would also move
                    // along the primary.
                    let tilted = || -> Result<bool, String> {
                        let (_, n) = self.nominal_datum(&datum.datum_feature)?;
                        Ok(dot(n, nominal.z_axis).abs() > 1e-9)
                    };
                    if shifts && tilted()? {
                        return Err("datum shift of a datum plane at a basic angle to the primary other than 90° is not evaluated".into());
                    }
                    let n = in_plane(*normal);
                    let length = n[0].hypot(n[1]);
                    let n = [n[0] / length, n[1] / length];
                    if !shifts {
                        if i == 2 && !axis_frame && !space.rotation_fixed {
                            return Err("after a secondary width datum at a material boundary, the tertiary datum must be a width at a material boundary".into());
                        }
                        if !axis_frame {
                            space.fixed.push((n, 0.0));
                        } else if free_secondary {
                            // The face's departure from its basic place
                            // along its normal.
                            let (origin, _) = self.nominal_datum(&datum.datum_feature)?;
                            let along = |frame: &DrawingDatumCoordinateFrame, p: Vec3| {
                                frame
                                    .coordinates_mm(p)
                                    .map(|c| n[0] * c.x + n[1] * c.y)
                                    .map_err(|e| e.to_string())
                            };
                            let departure = along(measured, *point)? - along(nominal, origin)?;
                            space.fixed.push((n, departure));
                        }
                        space.rotation_fixed |= axis_frame || i == 1;
                        continue;
                    }
                    let Some(width) = width else {
                        return Err("datum shift applies to datum features of size".into());
                    };
                    // The pair sits at its basic place from the frame origin:
                    // a plane-axis frame's tertiary width at its nominal
                    // offset from the axis, a planar frame's widths on the
                    // planes that define the origin.
                    let offset = if axis_frame {
                        let (origin, _) = self.nominal_datum(&datum.datum_feature)?;
                        nominal.coordinates_mm(origin).map_err(|e| e.to_string())?.x
                    } else {
                        let c = measured.coordinates_mm(*point).map_err(|e| e.to_string())?;
                        n[0] * c.x + n[1] * c.y
                    };
                    let (boundary, departure) = self.width_boundary(
                        datum,
                        &datums[..i],
                        *normal,
                        *width,
                        n,
                        offset,
                        measured,
                    )?;
                    if !axis_frame {
                        locating.push(departure);
                    }
                    space.widths.push(boundary);
                }
            }
        }
        if axis_frame && datums.len() == 2 {
            space.rotation_fixed = true;
        }
        space.limit += locating.iter().fold(0.0, |a: f64, &b| a.hypot(b));
        Ok(space)
    }

    /// Motions a frame on a primary datum axis allows: the axis's ends float
    /// within its material-boundary limit over the datum feature's measured
    /// extent, and a clocking width at a material boundary turns the frame
    /// about the axis, its boundary pair at the width's basic offset from the
    /// axis. A plane perpendicular to the axis only stops translation along
    /// it. Without a clocking datum the frame turns freely about the axis.
    fn axis_space(
        &mut self,
        datums: &[DrawingDatumReference],
        simulators: &[Simulator],
        limit: f64,
        measured: &DrawingDatumCoordinateFrame,
        nominal: &DrawingDatumCoordinateFrame,
    ) -> Result<ShiftSpace, String> {
        let mut span = [f64::INFINITY, f64::NEG_INFINITY];
        for member in datums[0].members() {
            for &p in &self.measured_datums[member] {
                let z = measured.coordinates_mm(p).map_err(|e| e.to_string())?.z;
                span = [span[0].min(z), span[1].max(z)];
            }
        }
        if span[1] - span[0] <= 1e-9 {
            return Err("the primary datum axis feature has no measured length".into());
        }
        let mut space = ShiftSpace {
            disk: (limit > 0.0).then_some(limit),
            rotation_fixed: true,
            limit,
            span: Some(span),
            ..ShiftSpace::default()
        };
        if datums[0].members().nth(1).is_some() && limit > 0.0 {
            space.disk = None;
            space.axis_bounds = self.common_bounds(&datums[0], measured)?;
        }
        let clocked = simulators.iter().skip(1).any(|s| match s {
            Simulator::Plane(_, normal, _) => dot(*normal, measured.z_axis).abs() < 0.5,
            Simulator::Axis(..) => true,
        });
        if !clocked {
            space.rotation_fixed = false;
            space.turn_free = true;
        }
        for (i, (datum, simulator)) in datums.iter().zip(simulators).enumerate().skip(1) {
            if datum.translation && datum.boundary != DatumMaterialBoundary::Regardless {
                return Err(
                    "a translation modifier is not evaluated on datums after a primary datum axis"
                        .into(),
                );
            }
            let (normal, width) = match simulator {
                Simulator::Plane(_, normal, width) => (normal, width),
                Simulator::Axis(origin, ..) => {
                    if datum.boundary != DatumMaterialBoundary::Regardless {
                        let pin =
                            self.pin_boundary(datum, simulator, *origin, measured, nominal)?;
                        space.pins.push(pin);
                        space.rotation_fixed = false;
                    }
                    continue;
                }
            };
            if datum.boundary == DatumMaterialBoundary::Regardless
                || dot(*normal, measured.z_axis).abs() > 0.5
            {
                continue;
            }
            let Some(width) = width else {
                return Err("datum shift applies to datum features of size".into());
            };
            let (origin, _) = self.nominal_datum(&datum.datum_feature)?;
            let offset = nominal.coordinates_mm(origin).map_err(|e| e.to_string())?.x;
            let n = [dot(*normal, measured.x_axis), dot(*normal, measured.y_axis)];
            let (boundary, _) =
                self.width_boundary(datum, &datums[..i], *normal, *width, n, offset, measured)?;
            space.widths.push(boundary);
            space.rotation_fixed = false;
        }
        Ok(space)
    }

    /// Where a common datum axis at a material boundary may pass: within
    /// each member's clearance of that member's own mating (or
    /// minimum-material) axis at both ends of its measured extent. Exact for
    /// members of perfect form.
    fn common_bounds(
        &self,
        datum: &DrawingDatumReference,
        measured: &DrawingDatumCoordinateFrame,
    ) -> Result<Vec<PinBoundary>, String> {
        let ResolvedDatum::Axis { direction, .. } = self.nominal(
            &self.features[datum.datum_feature.as_str()]
                .attachment
                .anchor,
        )?
        else {
            return Err("common datum axis has no axis geometry".into());
        };
        let mut bounds = Vec::new();
        for member in datum.members() {
            let points = &self.measured_datums[member];
            let Simulator::Axis(origin, axis, _, limit) = self.axis_simulator(
                self.features[member],
                points,
                direction,
                &[],
                datum.boundary,
                &[],
            )?
            else {
                unreachable!("axis simulator");
            };
            let (lo, hi) =
                points
                    .iter()
                    .try_fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &p| {
                        measured
                            .coordinates_mm(p)
                            .map(|c| (lo.min(c.z), hi.max(c.z)))
                            .map_err(|e| e.to_string())
                    })?;
            let start = measured.coordinates_mm(origin).map_err(|e| e.to_string())?;
            let rise = dot(axis, measured.z_axis);
            for z in [lo, hi] {
                let at = measured
                    .coordinates_mm(add(origin, scale(axis, (z - start.z) / rise)))
                    .map_err(|e| e.to_string())?;
                bounds.push(PinBoundary {
                    basic: [0.0, 0.0],
                    center: [at.x, at.y],
                    limit,
                    along: z,
                });
            }
        }
        Ok(bounds)
    }

    /// The boundary of a clocking hole or pin at a material boundary: its
    /// basic place in `nominal`, its measured axis (`origin`) in `measured`
    /// at the middle of its measured extent along z, and its limit.
    fn pin_boundary(
        &self,
        datum: &DrawingDatumReference,
        simulator: &Simulator,
        origin: Vec3,
        measured: &DrawingDatumCoordinateFrame,
        nominal: &DrawingDatumCoordinateFrame,
    ) -> Result<PinBoundary, String> {
        let Simulator::Axis(.., limit) = simulator else {
            unreachable!("axis simulator");
        };
        let id = datum.datum_feature.as_str();
        let ResolvedDatum::Axis { origin: basic, .. } =
            self.nominal(&self.features[id].attachment.anchor)?
        else {
            return Err(format!("datum feature '{id}' has no axis geometry"));
        };
        let basic = nominal.coordinates_mm(basic).map_err(|e| e.to_string())?;
        let center = measured.coordinates_mm(origin).map_err(|e| e.to_string())?;
        let (lo, hi) = self.measured_datums[id].iter().try_fold(
            (f64::INFINITY, f64::NEG_INFINITY),
            |(lo, hi), &p| {
                measured
                    .coordinates_mm(p)
                    .map(|c| (lo.min(c.z), hi.max(c.z)))
                    .map_err(|e| e.to_string())
            },
        )?;
        Ok(PinBoundary {
            basic: [basic.x, basic.y],
            center: [center.x, center.y],
            limit: *limit,
            along: 0.5 * (lo + hi),
        })
    }

    /// The boundary pair of a width datum feature of size and half its
    /// departure from the boundary at the mating orientation. Sizes come from
    /// the feature's `size_limits`, moved out by the geometric tolerance on
    /// the feature relative to `higher` (see [`Self::enlargement`]).
    #[allow(clippy::too_many_arguments)]
    fn width_boundary(
        &self,
        datum: &DrawingDatumReference,
        higher: &[DrawingDatumReference],
        normal: Vec3,
        mating: f64,
        in_plane: [f64; 2],
        offset: f64,
        measured: &DrawingDatumCoordinateFrame,
    ) -> Result<(WidthBoundary, f64), String> {
        let id = datum.datum_feature.as_str();
        let feature = self.features[id];
        let limits = feature.size_limits.as_ref().ok_or_else(|| {
            format!("datum feature '{id}' needs size limits for its material boundary")
        })?;
        let (lower, upper) = limits.bounds().map_err(|e| e.to_string())?;
        let internal = feature.kind() == Some(FeatureOfSizeKind::Internal);
        // The boundary moves away from the material: a slot's MMB narrows
        // and its LMB widens; a tab's the reverse.
        let t = self.enlargement(feature, higher, datum.boundary);
        let maximum = datum.boundary == DatumMaterialBoundary::Maximum;
        let (lower, upper) = if internal == maximum {
            (lower - t, upper)
        } else {
            (lower, upper + t)
        };
        let (origin, nominal) = self.nominal_datum(id)?;
        let facing = if dot(normal, nominal) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let points = &self.measured_datums[id];
        let mut faces = (Vec::new(), Vec::new());
        let mut projected = Vec::with_capacity(points.len());
        let mut along = Vec::with_capacity(points.len());
        for &p in points {
            let side = if dot(subtract(p, origin), nominal) >= 0.0 {
                facing
            } else {
                -facing
            };
            if side > 0.0 {
                faces.0.push(p);
            } else {
                faces.1.push(p);
            }
            let c = measured.coordinates_mm(p).map_err(|e| e.to_string())?;
            projected.push(([c.x, c.y], side));
            along.push(c.z);
        }
        let (size, departure) = if maximum {
            (
                if internal { lower } else { upper },
                if internal {
                    mating - lower
                } else {
                    upper - mating
                },
            )
        } else {
            // The minimum-material envelope at the mating orientation: the
            // narrowest pair around a slot, the widest inside a tab.
            let opposite = if internal {
                Fit::Circumscribed
            } else {
                Fit::Inscribed
            };
            let (_, _, minimum) =
                fit_width(&faces.0, &faces.1, normal, &[], opposite).map_err(|e| e.to_string())?;
            (
                if internal { upper } else { lower },
                if internal {
                    upper - minimum
                } else {
                    minimum - lower
                },
            )
        };
        if departure < -1e-9 {
            return Err(format!(
                "datum feature '{id}' violates its {} material boundary",
                if maximum { "maximum" } else { "least" }
            ));
        }
        Ok((
            WidthBoundary {
                normal: in_plane,
                offset,
                half_width: 0.5 * size,
                outside: internal == maximum,
                points: projected,
                along,
            },
            0.5 * departure.max(0.0),
        ))
    }
}
