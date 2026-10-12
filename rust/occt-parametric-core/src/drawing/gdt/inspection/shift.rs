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
    pub fixed: Vec<[f64; 2]>,
    pub rotation_fixed: bool,
    pub widths: Vec<WidthBoundary>,
    /// Largest translation of the frame origin the boundaries allow with the
    /// frame square to its mating envelopes.
    pub limit: f64,
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
        for f in &space.fixed {
            for sign in [1.0, -1.0] {
                push([sign * f[0], sign * f[1], 0.0, 0.0, 0.0, 0.0, 0.0], 0.0);
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

/// A feature coaxial with a primary datum axis: the offsets of its axis
/// ends from the datum simulator axis with their positions along it, and its
/// allowed zone radius.
#[derive(Clone, Copy, Debug)]
pub(super) struct AxialMember {
    pub ends: [([f64; 2], f64); 2],
    pub radius: f64,
}
impl AxialMember {
    /// Each end's offset from the datum axis after its ends (at `span`)
    /// move by `shift`, interpolated along the axis.
    fn residuals(&self, shift: [[f64; 2]; 2], span: [f64; 2]) -> [[f64; 2]; 2] {
        self.ends.map(|(offset, z)| {
            let w = (z - span[0]) / (span[1] - span[0]);
            [
                offset[0] - (1.0 - w) * shift[0][0] - w * shift[1][0],
                offset[1] - (1.0 - w) * shift[0][1] - w * shift[1][1],
            ]
        })
    }
    /// Diameter of the smallest zone about the shifted datum axis holding
    /// both ends.
    pub fn deviation(&self, shift: [[f64; 2]; 2], span: [f64; 2]) -> f64 {
        let r = self.residuals(shift, span);
        2.0 * r[0][0].hypot(r[0][1]).max(r[1][0].hypot(r[1][1]))
    }
    fn excess(&self, shift: [[f64; 2]; 2], span: [f64; 2]) -> f64 {
        0.5 * self.deviation(shift, span) - self.radius
    }
}

/// Shift of a primary datum axis at a material boundary shared by coaxial
/// controls: the simulator's ends, at the datum feature's measured extent
/// `span`, each move within `limit` of the mating (or minimum-material)
/// axis, which translates and tilts the axis. Exact for a datum feature of
/// perfect form; form error is not used to restrict the motion further. The
/// same min-max as [`fit_motion`] in four linear variables, with an active
/// set started from the five worst members. O(iterations · pivots ·
/// active · 512 + n).
pub(super) fn fit_axis_shift(members: &[AxialMember], limit: f64, span: [f64; 2]) -> [[f64; 2]; 2] {
    let zero = [[0.0; 2]; 2];
    if limit <= 0.0 || members.is_empty() {
        return zero;
    }
    let mut order: Vec<usize> = (0..members.len()).collect();
    order.sort_by(|&a, &b| {
        members[b]
            .excess(zero, span)
            .total_cmp(&members[a].excess(zero, span))
    });
    let mut active: Vec<usize> = order.into_iter().take(5).collect();
    loop {
        let subset: Vec<AxialMember> = active.iter().map(|&i| members[i]).collect();
        let shift = axis_shift_lp(&subset, limit, span);
        let active_worst = subset
            .iter()
            .map(|m| m.excess(shift, span))
            .fold(f64::NEG_INFINITY, f64::max);
        let (worst, value) = members
            .iter()
            .enumerate()
            .map(|(i, m)| (i, m.excess(shift, span)))
            .fold((0, f64::NEG_INFINITY), |a, b| if b.1 > a.1 { b } else { a });
        if value <= active_worst + 1e-12 || active.contains(&worst) {
            return shift;
        }
        active.push(worst);
    }
}

fn axis_shift_lp(members: &[AxialMember], limit: f64, span: [f64; 2]) -> [[f64; 2]; 2] {
    let directions: Vec<[f64; 2]> = (0..SIDES)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / SIDES as f64;
            [a.cos(), a.sin()]
        })
        .collect();
    // Variables: e0x, e0y, e1x, e1y, z, then w ≥ |e| per component.
    let mut rows: Vec<f64> = Vec::new();
    let mut bounds: Vec<f64> = Vec::new();
    let mut push = |row: [f64; 9], bound: f64| {
        rows.extend(row);
        bounds.push(bound);
    };
    for m in members {
        for (offset, z) in m.ends {
            let w = (z - span[0]) / (span[1] - span[0]);
            for u in &directions {
                push(
                    [
                        -(1.0 - w) * u[0],
                        -(1.0 - w) * u[1],
                        -w * u[0],
                        -w * u[1],
                        -1.0,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                    ],
                    m.radius - u[0] * offset[0] - u[1] * offset[1],
                );
            }
        }
    }
    for end in 0..2 {
        for u in &directions {
            let mut row = [0.0; 9];
            row[2 * end] = u[0];
            row[2 * end + 1] = u[1];
            push(row, limit);
        }
    }
    for i in 0..4 {
        for sign in [1.0, -1.0] {
            let mut row = [0.0; 9];
            row[i] = sign;
            row[5 + i] = -1.0;
            push(row, 0.0);
        }
    }
    let mut objective = [-1e-6; 9];
    objective[..4].fill(0.0);
    objective[4] = -1.0;
    let Ok(x) = fit::maximize(&objective, &rows, &bounds) else {
        return [[0.0; 2]; 2];
    };
    [[x[0], x[1]], [x[2], x[3]]].map(|e| {
        let length = e[0].hypot(e[1]);
        if length > limit {
            [e[0] * limit / length, e[1] * limit / length]
        } else {
            e
        }
    })
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
        let key: SimulatorKey = datums
            .iter()
            .map(|d| (d.members().map(str::to_owned).collect(), d.boundary))
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
        let axis_frame = simulators.iter().any(|s| matches!(s, Simulator::Axis(..)));
        let in_plane = |n: Vec3| [dot(n, measured.x_axis), dot(n, measured.y_axis)];
        let mut space = ShiftSpace::default();
        let mut locating = Vec::new();
        for (i, (datum, simulator)) in datums.iter().zip(&simulators).enumerate().skip(1) {
            let shifts = datum.boundary != DatumMaterialBoundary::Regardless;
            match simulator {
                Simulator::Axis(.., limit) if *limit > 0.0 => {
                    space.disk = Some(*limit);
                    space.limit += limit;
                }
                Simulator::Axis(..) => space.fixed.extend([[1.0, 0.0], [0.0, 1.0]]),
                Simulator::Plane(point, normal, width) => {
                    let n = in_plane(*normal);
                    let length = n[0].hypot(n[1]);
                    let n = [n[0] / length, n[1] / length];
                    if !shifts {
                        if i == 2 && !axis_frame && !space.rotation_fixed {
                            return Err("after a secondary width datum at a material boundary, the tertiary datum must be a width at a material boundary".into());
                        }
                        if !axis_frame {
                            space.fixed.push(n);
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
                    let (boundary, departure) =
                        self.width_boundary(datum, *normal, *width, n, offset, measured)?;
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

    /// The boundary pair of a width datum feature of size and half its
    /// departure from the boundary at the mating orientation. Sizes come from
    /// the feature's `size_limits`; geometric tolerances that would enlarge
    /// the boundary are not included.
    fn width_boundary(
        &self,
        datum: &DrawingDatumReference,
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
        let (origin, nominal) = self.nominal_datum(id)?;
        let facing = if dot(normal, nominal) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let points = &self.measured_datums[id];
        let mut faces = (Vec::new(), Vec::new());
        let mut projected = Vec::with_capacity(points.len());
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
        }
        let maximum = datum.boundary == DatumMaterialBoundary::Maximum;
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
            },
            0.5 * departure.max(0.0),
        ))
    }
}
