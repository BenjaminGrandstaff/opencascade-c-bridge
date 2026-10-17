//! Minimax (Chebyshev) fits of measured points by small dense linear programs.
//!
//! Each fit linearizes its geometry about the current estimate, solves a linear
//! program for the step, and repeats. Reported widths and radii are recomputed
//! exactly for the final orientation or center, so a fit that stops early can
//! only overstate a deviation, never understate it.
use super::*;

/// Relative tolerance on normalized (unit-scale) data.
const TOLERANCE: f64 = 1e-11;
const MAX_LINEARIZATIONS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Minimizes the largest absolute residual.
    Zone,
    /// Maximizes the smallest residual.
    Inscribed,
    /// Minimizes the largest residual.
    Circumscribed,
}

/// Fits residuals `r_i = t_i - phi_i·p` for free parameters `p` (`k` per row
/// of `phi`). O(pivots · m · k) time and O(m · k) memory for m residuals.
pub fn chebyshev(t: &[f64], phi: &[f64], k: usize, fit: Fit) -> Result<Vec<f64>, ModelError> {
    let n = k + 1;
    let rows_per = if fit == Fit::Zone { 2 } else { 1 };
    let mut rows = Vec::with_capacity(t.len() * rows_per * n);
    let mut bounds = Vec::with_capacity(t.len() * rows_per);
    for (i, &target) in t.iter().enumerate() {
        let p = &phi[i * k..(i + 1) * k];
        match fit {
            Fit::Zone => {
                rows.extend(p.iter().map(|v| -v));
                rows.push(-1.0);
                bounds.push(-target);
                rows.extend_from_slice(p);
                rows.push(-1.0);
                bounds.push(target);
            }
            Fit::Inscribed => {
                rows.extend_from_slice(p);
                rows.push(1.0);
                bounds.push(target);
            }
            Fit::Circumscribed => {
                rows.extend(p.iter().map(|v| -v));
                rows.push(-1.0);
                bounds.push(-target);
            }
        }
    }
    let mut objective = vec![0.0; n];
    objective[k] = if fit == Fit::Inscribed { 1.0 } else { -1.0 };
    let mut x = maximize(&objective, &rows, &bounds)?;
    x.truncate(k);
    Ok(x)
}

/// Maximizes `objective·x` over free `x` subject to `rows[i]·x ≤ bounds[i]`.
///
/// Solves the dual `min bounds·y, rowsᵀy = objective, y ≥ 0` by a two-phase
/// revised simplex whose basis is only `n × n` (n ≤ 6), so each pivot costs
/// O(m · n + n³). Dantzig pricing switches to Bland's rule after a run of
/// degenerate pivots, which guarantees termination.
pub fn maximize(objective: &[f64], rows: &[f64], bounds: &[f64]) -> Result<Vec<f64>, ModelError> {
    let n = objective.len();
    let m = bounds.len();
    let column = |j: usize| -> Vec<f64> {
        if j < m {
            rows[j * n..(j + 1) * n].to_vec()
        } else {
            let mut e = vec![0.0; n];
            e[j - m] = if objective[j - m] < 0.0 { -1.0 } else { 1.0 };
            e
        }
    };
    let undetermined = || ModelError::new("measured points do not determine the fitted geometry");
    let mut basis: Vec<usize> = (m..m + n).collect();
    // Phase 1 minimizes the artificial sum; phase 2 the bounds.
    for phase in 0..2 {
        let cost = |j: usize| -> f64 {
            match (phase, j < m) {
                (0, true) => 0.0,
                (0, false) => 1.0,
                (_, true) => bounds[j],
                (_, false) => 0.0,
            }
        };
        let mut degenerate = 0usize;
        let limit = 50 * (m + n) + 1000;
        let mut pivots = 0usize;
        loop {
            pivots += 1;
            if pivots > limit {
                return Err(ModelError::new("measured-point fit did not converge"));
            }
            let b = basis_matrix(&basis, &column, n);
            let x_b = solve(&b, objective, false).ok_or_else(undetermined)?;
            let c_b: Vec<f64> = basis.iter().map(|&j| cost(j)).collect();
            let pi = solve(&b, &c_b, true).ok_or_else(undetermined)?;
            let bland = degenerate > 2 * n + 10;
            let mut entering = None;
            let mut best = -TOLERANCE;
            let candidates = if phase == 0 { m + n } else { m };
            for j in 0..candidates {
                if basis.contains(&j) {
                    continue;
                }
                let priced = if j < m {
                    pi.iter()
                        .zip(&rows[j * n..(j + 1) * n])
                        .map(|(p, v)| p * v)
                        .sum()
                } else {
                    pi[j - m] * if objective[j - m] < 0.0 { -1.0 } else { 1.0 }
                };
                let reduced = cost(j) - priced;
                if reduced < best {
                    entering = Some(j);
                    if bland {
                        break;
                    }
                    best = reduced;
                }
            }
            let Some(entering) = entering else { break };
            let w = solve(&b, &column(entering), false).ok_or_else(undetermined)?;
            let mut leave: Option<(usize, f64)> = None;
            for i in 0..n {
                if w[i] > TOLERANCE {
                    let ratio = x_b[i].max(0.0) / w[i];
                    let better = match leave {
                        None => true,
                        Some((l, r)) => {
                            ratio < r - TOLERANCE || (ratio <= r + TOLERANCE && basis[i] < basis[l])
                        }
                    };
                    if better {
                        leave = Some((i, ratio));
                    }
                }
            }
            // An unbounded dual means the primal is infeasible, which the fits
            // in this module cannot produce; treat it as degenerate input.
            let (leave, ratio) = leave.ok_or_else(undetermined)?;
            degenerate = if ratio <= TOLERANCE {
                degenerate + 1
            } else {
                0
            };
            basis[leave] = entering;
        }
        if phase == 0 {
            let b = basis_matrix(&basis, &column, n);
            let x_b = solve(&b, objective, false).ok_or_else(undetermined)?;
            let scale = 1.0 + objective.iter().map(|v| v.abs()).sum::<f64>();
            let residual: f64 = basis
                .iter()
                .zip(&x_b)
                .filter(|(j, _)| **j >= m)
                .map(|(_, v)| v.abs())
                .sum();
            if residual > 1e-9 * scale {
                return Err(undetermined());
            }
            drive_out_artificials(&mut basis, &column, n, m).ok_or_else(undetermined)?;
        }
    }
    let b = basis_matrix(&basis, &column, n);
    let c_b: Vec<f64> = basis.iter().map(|&j| bounds[j]).collect();
    solve(&b, &c_b, true).ok_or_else(undetermined)
}

fn drive_out_artificials(
    basis: &mut [usize],
    column: &impl Fn(usize) -> Vec<f64>,
    n: usize,
    m: usize,
) -> Option<()> {
    for i in 0..n {
        if basis[i] < m {
            continue;
        }
        let b = basis_matrix(basis, column, n);
        let mut best: Option<(usize, f64)> = None;
        for j in 0..m {
            if basis.contains(&j) {
                continue;
            }
            let w = solve(&b, &column(j), false)?;
            if w[i].abs() > best.map_or(1e-9, |(_, v)| v) {
                best = Some((j, w[i].abs()));
            }
        }
        basis[i] = best?.0;
    }
    Some(())
}

fn basis_matrix(basis: &[usize], column: &impl Fn(usize) -> Vec<f64>, n: usize) -> Vec<f64> {
    let mut b = vec![0.0; n * n];
    for (c, &j) in basis.iter().enumerate() {
        for (r, v) in column(j).into_iter().enumerate() {
            b[r * n + c] = v;
        }
    }
    b
}

/// Solves `B x = rhs` (or `Bᵀ x = rhs`) by Gaussian elimination with partial pivoting.
fn solve(b: &[f64], rhs: &[f64], transpose: bool) -> Option<Vec<f64>> {
    let n = rhs.len();
    let mut a = vec![0.0; n * (n + 1)];
    for r in 0..n {
        for c in 0..n {
            a[r * (n + 1) + c] = if transpose {
                b[c * n + r]
            } else {
                b[r * n + c]
            };
        }
        a[r * (n + 1) + n] = rhs[r];
    }
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| {
            a[i * (n + 1) + col]
                .abs()
                .total_cmp(&a[j * (n + 1) + col].abs())
        })?;
        if a[pivot * (n + 1) + col].abs() < 1e-14 {
            return None;
        }
        for c in 0..=n {
            a.swap(col * (n + 1) + c, pivot * (n + 1) + c);
        }
        for r in 0..n {
            if r != col {
                let factor = a[r * (n + 1) + col] / a[col * (n + 1) + col];
                for c in col..=n {
                    a[r * (n + 1) + c] -= factor * a[col * (n + 1) + c];
                }
            }
        }
    }
    Some(
        (0..n)
            .map(|r| a[r * (n + 1) + n] / a[r * (n + 1) + r])
            .collect(),
    )
}

/// Centroid and size used to fit on unit-scale coordinates far from the origin.
fn normalization(points: &[Vec3]) -> (Vec3, f64) {
    let count = points.len() as f64;
    let sum = points
        .iter()
        .fold(Vec3::new(0.0, 0.0, 0.0), |s, &p| add(s, p));
    let center = scale(sum, 1.0 / count);
    let size = points
        .iter()
        .map(|&p| length(subtract(p, center)))
        .fold(0.0, f64::max);
    (center, if size > 0.0 { size } else { 1.0 })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneFit {
    pub normal: Vec3,
    /// Largest `normal·p`: the plane contacting the points from the normal side.
    pub contact: f64,
    /// Distance between the two parallel planes enclosing every point.
    pub width: f64,
}

pub(super) fn rotate(v: Vec3, axis: Vec3, angle: f64) -> Vec3 {
    let (s, c) = angle.sin_cos();
    add(
        add(scale(v, c), scale(cross(axis, v), s)),
        scale(axis, dot(axis, v) * (1.0 - c)),
    )
}

/// Minimum-zone plane whose normal starts at `normal` and may rotate only
/// about the unit `axes` (none: fixed orientation; one: rotation about a datum
/// normal; two: free). The contact side is `normal`, so the contacting plane
/// is the constrained-L∞ datum simulator. O(linearizations · pivots · n).
pub fn fit_plane(points: &[Vec3], normal: Vec3, axes: &[Vec3]) -> Result<PlaneFit, ModelError> {
    fit_plane_from(points, normal, axes, false)
}

/// [`fit_plane`] for a datum established by targets: it needs only one
/// point more than its free rotations (three for a primary, two for a
/// secondary, one for a tertiary), and points need not span a plane.
pub fn fit_target_plane(
    points: &[Vec3],
    normal: Vec3,
    axes: &[Vec3],
) -> Result<PlaneFit, ModelError> {
    fit_plane_from(points, normal, axes, true)
}

fn fit_plane_from(
    points: &[Vec3],
    normal: Vec3,
    axes: &[Vec3],
    targets: bool,
) -> Result<PlaneFit, ModelError> {
    if targets {
        if points.len() < axes.len() + 1 {
            return Err(ModelError::new(format!(
                "a datum established by targets with {} free rotation{} needs at least {} measured point{}",
                axes.len(),
                if axes.len() == 1 { "" } else { "s" },
                axes.len() + 1,
                if axes.is_empty() { "" } else { "s" },
            )));
        }
    } else if points.len() < 3 {
        return Err(ModelError::new(
            "a measured plane needs at least three points",
        ));
    }
    let (center, size) = normalization(points);
    let q: Vec<Vec3> = points
        .iter()
        .map(|&p| scale(subtract(p, center), 1.0 / size))
        .collect();
    // The farthest point from the centroid sets a line; a plane also needs a
    // point well off that line (normalized coordinates, so the size is one).
    let far = q
        .iter()
        .copied()
        .max_by(|a, b| length(*a).total_cmp(&length(*b)))
        .unwrap_or(Vec3::new(0.0, 0.0, 0.0));
    let line = scale(far, 1.0 / length(far).max(f64::MIN_POSITIVE));
    if (!targets || axes.len() >= 2) && q.iter().all(|&p| length(cross(line, p)) < 1e-9) {
        return Err(ModelError::new("measured plane points are collinear"));
    }
    let spread = |n: Vec3| {
        q.iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &p| {
                let d = dot(n, p);
                (lo.min(d), hi.max(d))
            })
    };
    let mut n = unit(normal)?;
    let (lo, hi) = spread(n);
    let mut best = (hi - lo, n, hi);
    for _ in 0..if axes.is_empty() {
        0
    } else {
        MAX_LINEARIZATIONS
    } {
        let k = axes.len() + 1;
        let t: Vec<f64> = q.iter().map(|&p| -dot(n, p)).collect();
        let mut phi = Vec::with_capacity(q.len() * k);
        for &p in &q {
            phi.extend(axes.iter().map(|&a| dot(cross(a, n), p)));
            phi.push(-1.0);
        }
        let step = chebyshev(&t, &phi, k, Fit::Zone)?;
        for (&a, &angle) in axes.iter().zip(&step) {
            n = rotate(n, a, angle);
        }
        n = unit(n)?;
        let (lo, hi) = spread(n);
        if hi - lo < best.0 {
            best = (hi - lo, n, hi);
        }
        if step[..axes.len()].iter().all(|a| a.abs() < 1e-13) {
            break;
        }
    }
    let (width, normal, hi) = best;
    Ok(PlaneFit {
        normal,
        contact: dot(normal, center) + hi * size,
        width: width * size,
    })
}

/// Center and radius of the largest inscribed (`Fit::Inscribed`) or smallest
/// circumscribed (`Fit::Circumscribed`) circle of planar points; the radius is
/// exact for the returned center. O(linearizations · pivots · n).
pub fn fit_circle(points: &[[f64; 2]], fit: Fit) -> Result<([f64; 2], f64), ModelError> {
    if points.len() < 3 || fit == Fit::Zone {
        return Err(ModelError::new(
            "a measured circle needs at least three points",
        ));
    }
    let lifted: Vec<Vec3> = points.iter().map(|p| Vec3::new(p[0], p[1], 0.0)).collect();
    let (center, size) = normalization(&lifted);
    let q: Vec<[f64; 2]> = points
        .iter()
        .map(|p| [(p[0] - center.x) / size, (p[1] - center.y) / size])
        .collect();
    let radius = |c: [f64; 2]| {
        let distances = q.iter().map(|p| (p[0] - c[0]).hypot(p[1] - c[1]));
        if fit == Fit::Inscribed {
            distances.fold(f64::INFINITY, f64::min)
        } else {
            distances.fold(0.0, f64::max)
        }
    };
    let better = |a: f64, b: f64| if fit == Fit::Inscribed { a > b } else { a < b };
    let mut c = [0.0, 0.0];
    let mut best = (radius(c), c);
    for _ in 0..MAX_LINEARIZATIONS {
        let mut t = Vec::with_capacity(q.len());
        let mut phi = Vec::with_capacity(q.len() * 2);
        for p in &q {
            let d = [p[0] - c[0], p[1] - c[1]];
            let rho = d[0].hypot(d[1]);
            if rho < 1e-12 {
                return Err(ModelError::new(
                    "measured circle points coincide with its center",
                ));
            }
            t.push(rho);
            phi.extend([d[0] / rho, d[1] / rho]);
        }
        let step = chebyshev(&t, &phi, 2, fit)?;
        c = [c[0] + step[0], c[1] + step[1]];
        let r = radius(c);
        if better(r, best.0) {
            best = (r, c);
        }
        if step[0].hypot(step[1]) < 1e-13 {
            break;
        }
    }
    let (r, c) = best;
    if !(r.is_finite() && r > 0.0) {
        return Err(ModelError::new("measured circle has no positive radius"));
    }
    Ok(([center.x + c[0] * size, center.y + c[1] * size], r * size))
}

/// Radial width of the narrowest pair of concentric circles holding planar
/// points (circularity), with the center free. Exact for the returned
/// center. O(linearizations · pivots · n).
pub fn circle_zone(points: &[[f64; 2]]) -> Result<f64, ModelError> {
    if points.len() < 4 {
        return Err(ModelError::new(
            "a circularity section needs at least four points",
        ));
    }
    let lifted: Vec<Vec3> = points.iter().map(|p| Vec3::new(p[0], p[1], 0.0)).collect();
    let (center, size) = normalization(&lifted);
    let q: Vec<[f64; 2]> = points
        .iter()
        .map(|p| [(p[0] - center.x) / size, (p[1] - center.y) / size])
        .collect();
    let width = |c: [f64; 2]| {
        let (lo, hi) = q.iter().fold((f64::INFINITY, 0.0_f64), |(lo, hi), p| {
            let rho = (p[0] - c[0]).hypot(p[1] - c[1]);
            (lo.min(rho), hi.max(rho))
        });
        hi - lo
    };
    let mut c = [0.0, 0.0];
    let mut best = width(c);
    for _ in 0..MAX_LINEARIZATIONS {
        let mut t = Vec::with_capacity(q.len());
        let mut phi = Vec::with_capacity(q.len() * 3);
        for p in &q {
            let d = [p[0] - c[0], p[1] - c[1]];
            let rho = d[0].hypot(d[1]);
            if rho < 1e-12 {
                return Err(ModelError::new(
                    "measured section points coincide with its center",
                ));
            }
            t.push(rho);
            phi.extend([d[0] / rho, d[1] / rho, 1.0]);
        }
        let step = chebyshev(&t, &phi, 3, Fit::Zone)?;
        c = [c[0] + step[0], c[1] + step[1]];
        best = best.min(width(c));
        if step[0].hypot(step[1]) < 1e-13 {
            break;
        }
    }
    Ok(best * size)
}

/// Unit vectors completing a right-handed basis with unit `n`.
fn basis(n: Vec3) -> (Vec3, Vec3) {
    let seed = if n.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u = scale(cross(n, seed), 1.0 / length(cross(n, seed)));
    (u, cross(n, u))
}

/// Radial width of the narrowest pair of coaxial cylinders holding the
/// points (cylindricity), with the axis free to shift and tilt from
/// `axis`. Exact for the returned axis. O(linearizations · pivots · n).
pub fn cylinder_zone(points: &[Vec3], axis: Vec3) -> Result<f64, ModelError> {
    if points.len() < 6 {
        return Err(ModelError::new(
            "a cylindricity measurement needs at least six points",
        ));
    }
    let (center, size) = normalization(points);
    let q: Vec<Vec3> = points
        .iter()
        .map(|&p| scale(subtract(p, center), 1.0 / size))
        .collect();
    let mut origin = Vec3::new(0.0, 0.0, 0.0);
    let mut direction = unit(axis)?;
    // Radial distance and unit direction of a point from the current axis,
    // and its position along it.
    let radial = |p: Vec3, origin: Vec3, direction: Vec3| {
        let d = subtract(p, origin);
        let along = dot(d, direction);
        let r = subtract(d, scale(direction, along));
        (length(r), r, along)
    };
    let width = |origin: Vec3, direction: Vec3| {
        let (lo, hi) = q.iter().fold((f64::INFINITY, 0.0_f64), |(lo, hi), &p| {
            let (rho, _, _) = radial(p, origin, direction);
            (lo.min(rho), hi.max(rho))
        });
        hi - lo
    };
    let mut best = width(origin, direction);
    for _ in 0..MAX_LINEARIZATIONS {
        let (e1, e2) = basis(direction);
        let mut t = Vec::with_capacity(q.len());
        let mut phi = Vec::with_capacity(q.len() * 5);
        for &p in &q {
            let (rho, r, along) = radial(p, origin, direction);
            if rho < 1e-12 {
                return Err(ModelError::new("measured points lie on the cylinder axis"));
            }
            let (a, b) = (dot(r, e1) / rho, dot(r, e2) / rho);
            t.push(rho);
            phi.extend([a, b, a * along, b * along, 1.0]);
        }
        let step = chebyshev(&t, &phi, 5, Fit::Zone)?;
        origin = add(origin, add(scale(e1, step[0]), scale(e2, step[1])));
        direction = unit(add(direction, add(scale(e1, step[2]), scale(e2, step[3]))))?;
        best = best.min(width(origin, direction));
        if step[..4].iter().all(|s| s.abs() < 1e-13) {
            break;
        }
    }
    Ok(best * size)
}

/// Axis (point, unit direction) and radius of the smallest circumscribed
/// (`Fit::Circumscribed`) or largest inscribed (`Fit::Inscribed`) cylinder of
/// the points, the axis free to shift and tilt from `axis`: the actual mating
/// envelope of an external or internal cylindrical feature. The radius is
/// exact for the returned axis. O(linearizations · pivots · n).
pub fn fit_cylinder(
    points: &[Vec3],
    axis: Vec3,
    fit: Fit,
) -> Result<(Vec3, Vec3, f64), ModelError> {
    if points.len() < 6 || fit == Fit::Zone {
        return Err(ModelError::new(
            "a measured cylinder needs at least six points",
        ));
    }
    let (center, size) = normalization(points);
    let q: Vec<Vec3> = points
        .iter()
        .map(|&p| scale(subtract(p, center), 1.0 / size))
        .collect();
    let radial = |p: Vec3, origin: Vec3, direction: Vec3| {
        let d = subtract(p, origin);
        let along = dot(d, direction);
        let r = subtract(d, scale(direction, along));
        (length(r), r, along)
    };
    let radius = |origin: Vec3, direction: Vec3| {
        let distances = q.iter().map(|&p| radial(p, origin, direction).0);
        if fit == Fit::Inscribed {
            distances.fold(f64::INFINITY, f64::min)
        } else {
            distances.fold(0.0, f64::max)
        }
    };
    let better = |a: f64, b: f64| if fit == Fit::Inscribed { a > b } else { a < b };
    let mut origin = Vec3::new(0.0, 0.0, 0.0);
    let mut direction = unit(axis)?;
    let mut best = (radius(origin, direction), origin, direction);
    for _ in 0..MAX_LINEARIZATIONS {
        let (e1, e2) = basis(direction);
        let mut t = Vec::with_capacity(q.len());
        let mut phi = Vec::with_capacity(q.len() * 4);
        for &p in &q {
            let (rho, r, along) = radial(p, origin, direction);
            if rho < 1e-12 {
                return Err(ModelError::new("measured points lie on the cylinder axis"));
            }
            let (a, b) = (dot(r, e1) / rho, dot(r, e2) / rho);
            t.push(rho);
            phi.extend([a, b, a * along, b * along]);
        }
        let step = chebyshev(&t, &phi, 4, fit)?;
        origin = add(origin, add(scale(e1, step[0]), scale(e2, step[1])));
        direction = unit(add(direction, add(scale(e1, step[2]), scale(e2, step[3]))))?;
        let r = radius(origin, direction);
        if better(r, best.0) {
            best = (r, origin, direction);
        }
        if step.iter().all(|s| s.abs() < 1e-13) {
            break;
        }
    }
    let (r, origin, direction) = best;
    if !(r.is_finite() && r > 0.0) {
        return Err(ModelError::new("measured cylinder has no positive radius"));
    }
    Ok((add(center, scale(origin, size)), direction, r * size))
}

/// Simulator of a common datum axis (Y14.5 simultaneous requirement): coaxial
/// cylinders, one per member, contracting about shafts (`Fit::Circumscribed`)
/// or expanding inside holes (`Fit::Inscribed`) at one rate about one shared
/// axis free to shift and tilt from `axis`. Each member's cylinder starts at
/// that member's own envelope, so the axis minimizes the largest growth
/// (shafts) or shrinkage (holes) any member needs. Returns the axis point,
/// unit direction and each member's simulator radius, exact for the returned
/// axis. O(members · own fit + linearizations · pivots · n).
pub fn fit_common_cylinder(
    members: &[(&[Vec3], Fit)],
    axis: Vec3,
) -> Result<(Vec3, Vec3, Vec<f64>), ModelError> {
    if members.is_empty() || members.iter().any(|(_, fit)| *fit == Fit::Zone) {
        return Err(ModelError::new(
            "a common datum axis needs members fitted as holes or shafts",
        ));
    }
    let own = members
        .iter()
        .map(|&(points, fit)| fit_cylinder(points, axis, fit))
        .collect::<Result<Vec<_>, _>>()?;
    let all: Vec<Vec3> = members
        .iter()
        .flat_map(|(points, _)| points.iter().copied())
        .collect();
    let (center, size) = normalization(&all);
    let normalized = |p: Vec3| scale(subtract(p, center), 1.0 / size);
    // Ties between equally small simulators (sampling leaves a flat set of
    // optimal axes) go to the axis nearest the line through the first and
    // last members' own envelope-axis midpoints.
    let midpoint = |i: usize| {
        let (points, _) = members[i];
        let (origin, direction, _) = own[i];
        let (lo, hi) = points
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &p| {
                let t = dot(subtract(p, origin), direction);
                (lo.min(t), hi.max(t))
            });
        normalized(add(origin, scale(direction, 0.5 * (lo + hi))))
    };
    let (first, last) = (midpoint(0), midpoint(members.len() - 1));
    let reference_direction = match unit(subtract(last, first)) {
        Ok(d) if members.len() > 1 => {
            if dot(d, axis) < 0.0 {
                scale(d, -1.0)
            } else {
                d
            }
        }
        _ => own[0].1,
    };
    let reference = (first, reference_direction);
    // Points scaled about the center, each with its member's own radius
    // and the sign that turns a hole's expansion into a minimized growth.
    let q: Vec<(Vec3, f64, f64)> = members
        .iter()
        .zip(&own)
        .flat_map(|(&(points, fit), &(.., r))| {
            let sign = if fit == Fit::Inscribed { -1.0 } else { 1.0 };
            points.iter().map(move |&p| (normalized(p), r / size, sign))
        })
        .collect();
    let radial = |p: Vec3, origin: Vec3, direction: Vec3| {
        let d = subtract(p, origin);
        let along = dot(d, direction);
        let r = subtract(d, scale(direction, along));
        (length(r), r, along)
    };
    // The largest growth over every point: sign · (distance − own radius).
    let growth = |origin: Vec3, direction: Vec3| {
        q.iter()
            .map(|&(p, r, sign)| sign * (radial(p, origin, direction).0 - r))
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let (mut origin, mut direction) = reference;
    let mut best = (growth(origin, direction), origin, direction);
    for _ in 0..MAX_LINEARIZATIONS {
        let (e1, e2) = basis(direction);
        // Variables: four axis steps, the growth z, and w ≥ |offset from the
        // reference axis| per step component.
        let mut rows = Vec::with_capacity((q.len() + 8) * 9);
        let mut bounds = Vec::with_capacity(q.len() + 8);
        for &(p, r, sign) in &q {
            let (rho, v, along) = radial(p, origin, direction);
            if rho < 1e-12 {
                return Err(ModelError::new("measured points lie on the datum axis"));
            }
            let (a, b) = (dot(v, e1) / rho, dot(v, e2) / rho);
            rows.extend([a, b, a * along, b * along].map(|g| -sign * g));
            rows.extend([-1.0, 0.0, 0.0, 0.0, 0.0]);
            bounds.push(-sign * (rho - r));
        }
        let foot = add(
            reference.0,
            scale(reference.1, dot(subtract(origin, reference.0), reference.1)),
        );
        let offset = subtract(origin, foot);
        let current = [
            dot(offset, e1),
            dot(offset, e2),
            -dot(reference.1, e1),
            -dot(reference.1, e2),
        ];
        for (i, c) in current.into_iter().enumerate() {
            for sign in [1.0, -1.0] {
                let mut row = [0.0; 9];
                row[i] = sign;
                row[5 + i] = -1.0;
                rows.extend(row);
                bounds.push(-sign * c);
            }
        }
        let mut objective = [-1e-6; 9];
        objective[..4].fill(0.0);
        objective[4] = -1.0;
        let step = maximize(&objective, &rows, &bounds)?;
        origin = add(origin, add(scale(e1, step[0]), scale(e2, step[1])));
        direction = unit(add(direction, add(scale(e1, step[2]), scale(e2, step[3]))))?;
        let g = growth(origin, direction);
        if g <= best.0 + 1e-12 {
            best = (g, origin, direction);
        }
        if step[..4].iter().all(|s| s.abs() < 1e-13) {
            break;
        }
    }
    let (g, origin, direction) = best;
    let radii = members
        .iter()
        .zip(&own)
        .map(|(&(_, fit), &(.., r))| {
            if fit == Fit::Inscribed {
                r - g * size
            } else {
                r + g * size
            }
        })
        .collect();
    Ok((add(center, scale(origin, size)), direction, radii))
}

/// The simultaneous simulator of a common datum from width features of
/// size (two slots or tabs, A-B): one center plane shared by every member's
/// actual mating envelope, its normal starting at `normal` and turning only
/// about the unit `axes`. Each member starts at its own mating width
/// ([`fit_width`]) and all change by the same amount, slots narrowing and
/// tabs widening, until one center plane fits them all; that change is
/// minimized. `members` holds each member's points on the face toward
/// `normal`, on the opposite face, and its fit. Returns the normal, the
/// center plane offset (`normal·p`) and each member's width, exact for the
/// returned plane. O(linearizations · pivots · n).
pub fn fit_common_width(
    members: &[(&[Vec3], &[Vec3], Fit)],
    normal: Vec3,
    axes: &[Vec3],
) -> Result<(Vec3, f64, Vec<f64>), ModelError> {
    if members.is_empty() {
        return Err(ModelError::new("a common datum width needs members"));
    }
    let own = members
        .iter()
        .map(|&(upper, lower, fit)| fit_width(upper, lower, normal, axes, fit))
        .collect::<Result<Vec<_>, _>>()?;
    let all: Vec<Vec3> = members
        .iter()
        .flat_map(|(upper, lower, _)| upper.iter().chain(lower.iter()).copied())
        .collect();
    let (center, size) = normalization(&all);
    // Each point: normalized position, its member's own half width, its
    // face (+1 toward the normal) and whether the member is a slot.
    let q: Vec<(Vec3, f64, f64, bool)> = members
        .iter()
        .zip(&own)
        .flat_map(|(&(upper, lower, fit), &(.., width))| {
            let half = 0.5 * width / size;
            let internal = fit == Fit::Inscribed;
            upper
                .iter()
                .map(move |&p| (p, 1.0))
                .chain(lower.iter().map(move |&p| (p, -1.0)))
                .map(move |(p, side)| {
                    (scale(subtract(p, center), 1.0 / size), half, side, internal)
                })
        })
        .collect();
    // The change every member needs for a center plane (n, c).
    let excess = |(p, half, side, internal): (Vec3, f64, f64, bool), n: Vec3, c: f64| {
        let reach = side * (dot(n, p) - c);
        if internal { half - reach } else { reach - half }
    };
    let growth = |n: Vec3, c: f64| {
        q.iter()
            .map(|&point| excess(point, n, c))
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let mut n = unit(normal)?;
    let mut c = own
        .iter()
        .map(|&(_, offset, _)| (offset - dot(n, center)) / size)
        .sum::<f64>()
        / own.len() as f64;
    let mut best = (growth(n, c), n, c);
    let k = axes.len();
    let mut radius = 0.1;
    for _ in 0..if k == 0 { 1 } else { MAX_LINEARIZATIONS } {
        // Variables: rotation angles, the center plane step and the change.
        let vars = k + 2;
        let mut rows = Vec::with_capacity((q.len() + 2 * k) * vars);
        let mut bounds = Vec::with_capacity(q.len() + 2 * k);
        for &(p, half, side, internal) in &q {
            let reach = side * (dot(n, p) - c);
            let sign = if internal { -1.0 } else { 1.0 };
            rows.extend(axes.iter().map(|&a| sign * side * dot(cross(a, n), p)));
            rows.extend([-sign * side, -1.0]);
            bounds.push(if internal { reach - half } else { half - reach });
        }
        for j in 0..k {
            for sign in [1.0, -1.0] {
                let mut row = vec![0.0; vars];
                row[j] = sign;
                rows.extend(row);
                bounds.push(radius);
            }
        }
        let mut objective = vec![0.0; vars];
        objective[k + 1] = -1.0;
        let step = maximize(&objective, &rows, &bounds)?;
        let mut trial = n;
        for (&a, &angle) in axes.iter().zip(&step) {
            trial = rotate(trial, a, angle);
        }
        let trial = unit(trial)?;
        let trial_c = c + step[k];
        let g = growth(trial, trial_c);
        if g < best.0 - 1e-15 || k == 0 {
            best = (g, trial, trial_c);
            (n, c) = (trial, trial_c);
        } else {
            radius *= 0.5;
        }
        if step[..k].iter().all(|a| a.abs() < 1e-13) || radius < 1e-12 {
            break;
        }
    }
    let (g, n, c) = best;
    let widths = members
        .iter()
        .zip(&own)
        .map(|(&(_, _, fit), &(.., width))| {
            if fit == Fit::Inscribed {
                width - 2.0 * g * size
            } else {
                width + 2.0 * g * size
            }
        })
        .collect::<Vec<_>>();
    if widths.iter().any(|&w| w <= 0.0) {
        return Err(ModelError::new("measured slot faces overlap"));
    }
    Ok((n, c * size + dot(n, center), widths))
}

/// Actual mating envelope of a width feature: two parallel planes with
/// normal starting at `normal` and turning only about the unit `axes`, the
/// widest pair fitting between the faces of a slot (`Fit::Inscribed`) or the
/// narrowest pair enclosing the faces of a tab (`Fit::Circumscribed`).
/// `upper` holds points on the face toward `normal`, `lower` the opposite
/// face. Returns the normal, the center plane offset (`normal·p`) and the
/// width, exact for the returned normal. O(linearizations · pivots · n).
pub fn fit_width(
    upper: &[Vec3],
    lower: &[Vec3],
    normal: Vec3,
    axes: &[Vec3],
    fit: Fit,
) -> Result<(Vec3, f64, f64), ModelError> {
    if upper.is_empty() || lower.is_empty() || upper.len() + lower.len() < 3 || fit == Fit::Zone {
        return Err(ModelError::new(
            "a measured width needs points on both faces",
        ));
    }
    let all: Vec<Vec3> = upper.iter().chain(lower).copied().collect();
    let (center, size) = normalization(&all);
    let scaled = |p: &[Vec3]| -> Vec<Vec3> {
        p.iter()
            .map(|&q| scale(subtract(q, center), 1.0 / size))
            .collect()
    };
    let (up, low) = (scaled(upper), scaled(lower));
    let internal = fit == Fit::Inscribed;
    // Exact bounding offsets of the two faces along `n`: the slot's walls
    // nearest each other, or the tab's farthest faces.
    let planes = |n: Vec3| {
        let along = |p: &[Vec3], high: bool| {
            p.iter().map(|&q| dot(n, q)).fold(
                if high {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                },
                |a, b| if high { a.max(b) } else { a.min(b) },
            )
        };
        if internal {
            (along(&up, false), along(&low, true))
        } else {
            (along(&up, true), along(&low, false))
        }
    };
    let width_of = |(a, b): (f64, f64)| a - b;
    let better = |a: f64, b: f64| if internal { a > b } else { a < b };
    let mut n = unit(normal)?;
    let mut best = (width_of(planes(n)), n);
    let k = axes.len();
    let mut radius = 0.1;
    for _ in 0..if k == 0 { 0 } else { MAX_LINEARIZATIONS } {
        // Variables: rotation angles, upper plane, lower plane.
        let vars = k + 2;
        let mut rows = Vec::new();
        let mut bounds = Vec::new();
        let gradient = |q: Vec3| axes.iter().map(move |&a| dot(cross(a, n), q));
        for &q in &up {
            let mut row: Vec<f64> = gradient(q).map(|g| if internal { -g } else { g }).collect();
            row.extend(if internal { [1.0, 0.0] } else { [-1.0, 0.0] });
            rows.extend(row);
            bounds.push(if internal { dot(n, q) } else { -dot(n, q) });
        }
        for &q in &low {
            let mut row: Vec<f64> = gradient(q).map(|g| if internal { g } else { -g }).collect();
            row.extend(if internal { [0.0, -1.0] } else { [0.0, 1.0] });
            rows.extend(row);
            bounds.push(if internal { -dot(n, q) } else { dot(n, q) });
        }
        for j in 0..k {
            for sign in [1.0, -1.0] {
                let mut row = vec![0.0; vars];
                row[j] = sign;
                rows.extend(row);
                bounds.push(radius);
            }
        }
        let mut objective = vec![0.0; vars];
        objective[k] = if internal { 1.0 } else { -1.0 };
        objective[k + 1] = -objective[k];
        let step = maximize(&objective, &rows, &bounds)?;
        let mut trial = n;
        for (&a, &angle) in axes.iter().zip(&step) {
            trial = rotate(trial, a, angle);
        }
        let trial = unit(trial)?;
        let width = width_of(planes(trial));
        if better(width, best.0) {
            best = (width, trial);
            n = trial;
        } else {
            radius *= 0.5;
        }
        if step[..k].iter().all(|a| a.abs() < 1e-13) || radius < 1e-12 {
            break;
        }
    }
    let (width, n) = best;
    let (a, b) = planes(n);
    if internal && width <= 0.0 {
        return Err(ModelError::new("measured slot faces overlap"));
    }
    Ok((n, dot(n, center) + 0.5 * (a + b) * size, width * size))
}
