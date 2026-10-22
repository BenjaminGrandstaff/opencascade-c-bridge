//! Location-only part patterns. Bookkeeping O(N), topology validation O(N*T).
use super::*;
use occt_bridge::FacePointState;

const MAX_COPIES: usize = 10_000;
const MAX_ESTIMATED_TOPOLOGY: usize = 1_000_000;

pub(super) fn linear<'a>(
    session: &'a Session,
    source: &Shape<'_>,
    step: Vec3,
    count: f64,
    skip: &[usize],
    retain_source: bool,
) -> Result<Shape<'a>, ModelError> {
    if !count.is_finite() || count.fract() != 0.0 || !(1.0..=MAX_COPIES as f64).contains(&count) {
        return Err(ModelError::new(
            "linear pattern count must be an integer-valued scalar from 1 to 10000",
        ));
    }
    let count = count as usize;
    if count > 1 && step.x == 0.0 && step.y == 0.0 && step.z == 0.0 {
        return Err(ModelError::new(
            "linear pattern step must be nonzero for multiple copies",
        ));
    }
    let kept = kept(count, skip, "linear")?;
    check_topology(session, source, kept.len())?;
    let mut copies = Vec::with_capacity(kept.len());
    for i in kept {
        let i = i as f64;
        let offset = Vec3::new(step.x * i, step.y * i, step.z * i);
        if !offset.x.is_finite() || !offset.y.is_finite() || !offset.z.is_finite() {
            return Err(ModelError::new("linear pattern placement overflow"));
        }
        let placed = session.translate(source, offset)?;
        copies.push(if retain_source {
            session.compose_history(&placed, source)?
        } else {
            placed
        });
    }
    group(session, &copies)
}

fn check_topology(session: &Session, source: &Shape<'_>, count: usize) -> Result<(), ModelError> {
    // Bound nested patterns before placing copies. This is a conservative
    // sum of unique subshape counts; shared/coincident topology can cost less.
    let mut topology = 0usize;
    for kind in [
        ShapeType::Compound,
        ShapeType::CompSolid,
        ShapeType::Solid,
        ShapeType::Shell,
        ShapeType::Face,
        ShapeType::Wire,
        ShapeType::Edge,
        ShapeType::Vertex,
    ] {
        topology = topology.saturating_add(session.subshape_count(source, kind)?);
    }
    if topology.saturating_add(1).saturating_mul(count) > MAX_ESTIMATED_TOPOLOGY {
        return Err(ModelError::new(
            "part pattern exceeds the 1000000 estimated topology limit",
        ));
    }
    Ok(())
}

fn group<'a>(session: &'a Session, copies: &[Shape<'_>]) -> Result<Shape<'a>, ModelError> {
    let refs = copies.iter().collect::<Vec<_>>();
    let result = session.create_compound(&refs)?;
    if !session.is_valid(&result)? {
        return Err(ModelError::new("part pattern contains invalid geometry"));
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn circular<'a>(
    session: &'a Session,
    source: &Shape<'_>,
    origin: Vec3,
    axis: Vec3,
    count: f64,
    angle_step: f64,
    skip: &[usize],
    retain_source: bool,
) -> Result<Shape<'a>, ModelError> {
    if !count.is_finite() || count.fract() != 0.0 || !(1.0..=MAX_COPIES as f64).contains(&count) {
        return Err(ModelError::new(
            "circular pattern count must be an integer-valued scalar from 1 to 10000",
        ));
    }
    if !angle_step.is_finite()
        || (count > 1.0
            && (angle_step == 0.0 || ((count - 1.0) * angle_step).abs() >= std::f64::consts::TAU))
    {
        return Err(ModelError::new(
            "circular pattern requires nonzero angular spacing and a first-to-last sweep below one full turn",
        ));
    }
    let count = count as usize;
    let kept = kept(count, skip, "circular")?;
    check_topology(session, source, kept.len())?;
    let mut copies = Vec::with_capacity(kept.len());
    for i in kept {
        let placed = session.rotate(source, origin, axis, i as f64 * angle_step)?;
        copies.push(if retain_source {
            session.compose_history(&placed, source)?
        } else {
            placed
        });
    }
    group(session, &copies)
}

/// Copy indices in 0..count less those skipped (schema 184). Skipped
/// indices must exist and leave at least one copy.
fn kept(count: usize, skip: &[usize], name: &str) -> Result<Vec<usize>, ModelError> {
    if let Some(index) = skip.iter().find(|&&i| i >= count) {
        return Err(ModelError::new(format!(
            "{name} pattern skips copy {index}, but copies run 0 to {}",
            count - 1
        )));
    }
    let mut left = vec![true; count];
    for &i in skip {
        left[i] = false;
    }
    let kept: Vec<usize> = (0..count).filter(|&i| left[i]).collect();
    if kept.is_empty() {
        return Err(ModelError::new(format!("{name} pattern skips every copy")));
    }
    Ok(kept)
}

fn integer_count(count: f64, name: &str) -> Result<usize, ModelError> {
    if !count.is_finite() || count.fract() != 0.0 || !(1.0..=MAX_COPIES as f64).contains(&count) {
        return Err(ModelError::new(format!(
            "{name} pattern count must be an integer-valued scalar from 1 to 10000"
        )));
    }
    Ok(count as usize)
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}
fn scaled(a: Vec3, k: f64) -> Vec3 {
    Vec3::new(a.x * k, a.y * k, a.z * k)
}
fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}
fn unit(a: Vec3) -> Option<Vec3> {
    let l = dot(a, a).sqrt();
    (l.is_finite() && l > 1e-12).then(|| scaled(a, 1.0 / l))
}

/// A unit quaternion [w, x, y, z].
type Quaternion = [f64; 4];

fn multiply(a: Quaternion, b: Quaternion) -> Quaternion {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

/// The smallest rotation taking unit `a` to unit `b`, which must not be
/// opposite.
fn between(a: Vec3, b: Vec3) -> Option<Quaternion> {
    let c = cross(a, b);
    let w = 1.0 + dot(a, b);
    if w < 1e-9 {
        return None;
    }
    let q = [w, c.x, c.y, c.z];
    let l = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    Some(q.map(|v| v / l))
}

/// Axis and angle of a unit quaternion, or `None` for no rotation.
fn axis_angle(q: Quaternion) -> Option<(Vec3, f64)> {
    let v = Vec3::new(q[1], q[2], q[3]);
    let s = dot(v, v).sqrt();
    if s < 1e-15 {
        return None;
    }
    Some((scaled(v, 1.0 / s), 2.0 * s.atan2(q[0])))
}

/// Copies along a path by arc length (schema 184). Each member costs O(path
/// edges) per station; tangent copies step the rotation through at least 8
/// stations per gap (256 in all) so no step turns by much.
#[allow(clippy::too_many_arguments)]
pub(super) fn curve<'a>(
    session: &'a Session,
    source: &Shape<'_>,
    path: &Shape<'_>,
    count: f64,
    orientation: CurvePatternOrientation,
    skip: &[usize],
    retain_source: bool,
) -> Result<Shape<'a>, ModelError> {
    let count = integer_count(count, "curve")?;
    if !matches!(session.shape_type(path)?, ShapeType::Edge | ShapeType::Wire) {
        return Err(ModelError::new(
            "curve pattern path must be one edge or wire, such as a sketch open wire",
        ));
    }
    let kept = kept(count, skip, "curve")?;
    check_topology(session, source, kept.len())?;
    let (start, start_tangent) = session.path_station(path, 0.0)?;
    let (end, _) = session.path_station(path, 1.0)?;
    let size = {
        let b = session.exact_bounds(path)?;
        dot(sub(b.max, b.min), sub(b.max, b.min)).sqrt().max(1e-9)
    };
    let closed = dot(sub(end, start), sub(end, start)).sqrt() <= 1e-9 * size.max(1.0);
    let gaps = if closed {
        count
    } else {
        count.saturating_sub(1)
    }
    .max(1);
    let fraction = |i: usize| {
        if count == 1 {
            0.0
        } else {
            i as f64 / gaps as f64
        }
    };
    // Rotation of each member, carried station by station.
    let mut turns = vec![[1.0, 0.0, 0.0, 0.0]; count];
    if orientation == CurvePatternOrientation::Tangent && count > 1 {
        let per_gap = 256usize.div_ceil(gaps).max(8);
        let mut q: Quaternion = [1.0, 0.0, 0.0, 0.0];
        let mut previous = unit(start_tangent)
            .ok_or_else(|| ModelError::new("curve pattern path has no tangent at its start"))?;
        for (member, turn_of) in turns.iter_mut().enumerate().skip(1) {
            for step in 1..=per_gap {
                let f = (member - 1) as f64 / gaps as f64 + step as f64 / (per_gap * gaps) as f64;
                let (_, tangent) = session.path_station(path, f.min(1.0))?;
                let tangent = unit(tangent).ok_or_else(|| {
                    ModelError::new("curve pattern path has no tangent somewhere along it")
                })?;
                let turn = between(previous, tangent).ok_or_else(|| {
                    ModelError::new(
                        "curve pattern path turns back on itself (a cusp); split it there",
                    )
                })?;
                q = multiply(turn, q);
                previous = tangent;
            }
            *turn_of = q;
        }
    }
    let mut copies = Vec::with_capacity(kept.len());
    for i in kept {
        let (point, _) = session.path_station(path, fraction(i))?;
        let mut placed = session.translate(source, sub(point, start))?;
        if let Some((axis, angle)) = axis_angle(turns[i]) {
            placed = session.rotate(&placed, point, axis, angle)?;
        }
        copies.push(if retain_source {
            session.compose_history(&placed, source)?
        } else {
            placed
        });
    }
    group(session, &copies)
}

/// Copies at each offset (schema 184).
pub(super) fn table<'a>(
    session: &'a Session,
    source: &Shape<'_>,
    offsets: &[Vec3],
    retain_source: bool,
) -> Result<Shape<'a>, ModelError> {
    if offsets.is_empty() || offsets.len() > MAX_COPIES {
        return Err(ModelError::new("table pattern needs 1 to 10000 offsets"));
    }
    check_topology(session, source, offsets.len())?;
    let mut copies = Vec::with_capacity(offsets.len());
    for offset in offsets {
        if !(offset.x.is_finite() && offset.y.is_finite() && offset.z.is_finite()) {
            return Err(ModelError::new("table pattern offsets must be finite"));
        }
        let placed = session.translate(source, *offset)?;
        copies.push(if retain_source {
            session.compose_history(&placed, source)?
        } else {
            placed
        });
    }
    group(session, &copies)
}

/// Most grid sites a fill pattern may examine.
const MAX_FILL_SITES: usize = 1_000_000;

/// A fill pattern's grid and region.
pub(super) struct Fill {
    pub anchor: Vec3,
    pub direction: Vec3,
    pub spacing: f64,
    pub row_spacing: f64,
    pub layout: FillPatternLayout,
    pub margin: f64,
}

/// Copies at the grid sites inside a planar face, at least `margin` from its
/// boundary (schema 184). The boundary is sampled into segments in the plane:
/// each row tests its sites against that row's crossings, and a spatial hash
/// finds segments within the margin. Sites whose sampled answer lies within
/// the sampling error of the boundary or the margin are decided by the exact
/// face classification and distance. O(rows · segments + sites + ambiguous
/// sites · face edges).
pub(super) fn fill<'a>(
    session: &'a Session,
    source: &Shape<'_>,
    face: &Shape<'_>,
    fill: Fill,
    retain_source: bool,
) -> Result<Shape<'a>, ModelError> {
    let Fill {
        anchor,
        direction,
        spacing,
        row_spacing,
        layout,
        margin,
    } = fill;
    if !(spacing.is_finite() && spacing > 0.0 && row_spacing.is_finite() && row_spacing > 0.0) {
        return Err(ModelError::new(
            "fill pattern spacing and row_spacing must be positive lengths",
        ));
    }
    if !(margin.is_finite() && margin >= 0.0) {
        return Err(ModelError::new(
            "fill pattern margin must be a nonnegative length",
        ));
    }
    if session.shape_type(face)? != ShapeType::Face || !session.face_is_planar(face)? {
        return Err(ModelError::new(
            "fill pattern region must be one planar face",
        ));
    }
    let normal = unit(session.face_normal(face)?)
        .ok_or_else(|| ModelError::new("fill pattern region has no normal"))?;
    let center = session.center_of_mass(face)?;
    let a = unit(sub(direction, scaled(normal, dot(direction, normal)))).ok_or_else(|| {
        ModelError::new("fill pattern direction must not be normal to the region")
    })?;
    let b = cross(normal, a);
    let base = sub(anchor, scaled(normal, dot(sub(anchor, center), normal)));
    let plane = |p: Vec3| {
        let d = sub(p, base);
        [dot(d, a), dot(d, b)]
    };
    // Boundary segments in plane coordinates, sampled finely enough that a
    // straight chord stays near its curve; `band` bounds that error.
    let edges = session.subshapes(face, ShapeType::Edge)?;
    let mut segments: Vec<[[f64; 2]; 2]> = Vec::new();
    let mut band = 0.0f64;
    for edge in &edges {
        let straight = matches!(
            session.edge_analytic_curve(edge)?,
            Some(occt_bridge::AnalyticCurve::Line { .. })
        );
        let count = if straight { 2 } else { 129 };
        let points: Vec<[f64; 2]> = session
            .edge_sample_points(edge, count)?
            .into_iter()
            .map(plane)
            .collect();
        if !straight {
            // Sag of each chord pair against the midpoint between them.
            for w in points.windows(3).step_by(2) {
                let (p, m, q) = (w[0], w[1], w[2]);
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let length = dx.hypot(dy).max(1e-300);
                band = band.max(((m[0] - p[0]) * dy - (m[1] - p[1]) * dx).abs() / length);
            }
        }
        segments.extend(points.windows(2).map(|w| [w[0], w[1]]));
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for s in &segments {
        for p in s {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    let size = (hi[0] - lo[0]).hypot(hi[1] - lo[1]);
    // Chords sampled at half the step sag about a quarter as much again.
    let band = 2.0 * band + 1e-7 * size.max(1.0);
    let rows = (
        (lo[1] / row_spacing).floor() as i64,
        (hi[1] / row_spacing).ceil() as i64,
    );
    let columns = (
        (lo[0] / spacing).floor() as i64 - 1,
        (hi[0] / spacing).ceil() as i64 + 1,
    );
    let sites = (rows.1 - rows.0 + 1).max(0) as u128 * (columns.1 - columns.0 + 1).max(0) as u128;
    if sites > MAX_FILL_SITES as u128 {
        return Err(ModelError::new(format!(
            "fill pattern grid covers more than {MAX_FILL_SITES} sites; increase the spacing"
        )));
    }
    // Spatial hash of segments by cell, for the margin test.
    let cell = (margin + band).max(size * 1e-3).max(1e-9);
    let key = |p: [f64; 2]| ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64);
    let mut hash: std::collections::HashMap<(i64, i64), Vec<usize>> =
        std::collections::HashMap::new();
    for (index, s) in segments.iter().enumerate() {
        let (k0, k1) = (
            key([s[0][0].min(s[1][0]), s[0][1].min(s[1][1])]),
            key([s[0][0].max(s[1][0]), s[0][1].max(s[1][1])]),
        );
        if ((k1.0 - k0.0 + 1) as i128) * ((k1.1 - k0.1 + 1) as i128) > 1_000_000 {
            return Err(ModelError::new(
                "fill pattern margin is too small for the region's size",
            ));
        }
        for x in k0.0..=k1.0 {
            for y in k0.1..=k1.1 {
                hash.entry((x, y)).or_default().push(index);
            }
        }
    }
    let near = |p: [f64; 2]| -> f64 {
        let (cx, cy) = key(p);
        let mut best = f64::INFINITY;
        for x in cx - 1..=cx + 1 {
            for y in cy - 1..=cy + 1 {
                for &i in hash.get(&(x, y)).map(Vec::as_slice).unwrap_or(&[]) {
                    let [s, t] = segments[i];
                    let (dx, dy) = (t[0] - s[0], t[1] - s[1]);
                    let l2 = dx * dx + dy * dy;
                    let u = if l2 == 0.0 {
                        0.0
                    } else {
                        (((p[0] - s[0]) * dx + (p[1] - s[1]) * dy) / l2).clamp(0.0, 1.0)
                    };
                    best = best.min((p[0] - s[0] - u * dx).hypot(p[1] - s[1] - u * dy));
                }
            }
        }
        best
    };
    let boundary = {
        let refs: Vec<_> = edges.iter().collect();
        session.create_compound(&refs)?
    };
    let to_space = |p: [f64; 2]| add(base, add(scaled(a, p[0]), scaled(b, p[1])));
    let mut members: Vec<Vec3> = Vec::new();
    for row in rows.0..=rows.1 {
        let v = row as f64 * row_spacing;
        // Crossings of this row's line with the boundary, sorted.
        let mut crossings: Vec<f64> = segments
            .iter()
            .filter(|[s, t]| (s[1] > v) != (t[1] > v))
            .map(|[s, t]| s[0] + (v - s[1]) * (t[0] - s[0]) / (t[1] - s[1]))
            .collect();
        crossings.sort_by(f64::total_cmp);
        let shift = if layout == FillPatternLayout::Staggered && row.rem_euclid(2) == 1 {
            spacing / 2.0
        } else {
            0.0
        };
        for column in columns.0..=columns.1 {
            let u = column as f64 * spacing + shift;
            let inside = crossings.partition_point(|&c| c < u) % 2 == 1;
            let distance = near([u, v]);
            let keep = if distance > margin + band {
                // Clear of the boundary and the margin: the sampled test holds.
                inside
            } else if distance < margin - band {
                false
            } else {
                // Within the sampling error of the boundary or the margin:
                // decide exactly. Sites on the boundary are left out.
                let point = to_space([u, v]);
                if session.planar_face_point_state(face, point)? != FacePointState::Inside {
                    false
                } else if margin == 0.0 {
                    true
                } else {
                    let probe = session.create_circle_wire(point, normal, 1e-6)?;
                    session.distance(&boundary, &probe)?.distance + 1e-6 >= margin
                }
            };
            if keep {
                members.push(to_space([u, v]));
                if members.len() > MAX_COPIES {
                    return Err(ModelError::new(
                        "fill pattern places more than 10000 copies; increase the spacing or margin",
                    ));
                }
            }
        }
    }
    if members.is_empty() {
        return Err(ModelError::new(
            "fill pattern places no copies inside the region",
        ));
    }
    check_topology(session, source, members.len())?;
    let mut copies = Vec::with_capacity(members.len());
    for site in members {
        let placed = session.translate(source, sub(site, base))?;
        copies.push(if retain_source {
            session.compose_history(&placed, source)?
        } else {
            placed
        });
    }
    group(session, &copies)
}
