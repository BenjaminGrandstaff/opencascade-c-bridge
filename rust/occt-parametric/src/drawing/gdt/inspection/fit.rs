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
pub(super) enum Fit {
    /// Minimizes the largest absolute residual.
    Zone,
    /// Maximizes the smallest residual.
    Inscribed,
    /// Minimizes the largest residual.
    Circumscribed,
}

/// Fits residuals `r_i = t_i - phi_i·p` for free parameters `p` (`k` per row
/// of `phi`). O(pivots · m · k) time and O(m · k) memory for m residuals.
pub(super) fn chebyshev(
    t: &[f64],
    phi: &[f64],
    k: usize,
    fit: Fit,
) -> Result<Vec<f64>, ModelError> {
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
pub(super) fn maximize(
    objective: &[f64],
    rows: &[f64],
    bounds: &[f64],
) -> Result<Vec<f64>, ModelError> {
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
pub(super) struct PlaneFit {
    pub normal: Vec3,
    /// Largest `normal·p`: the plane contacting the points from the normal side.
    pub contact: f64,
    /// Distance between the two parallel planes enclosing every point.
    pub width: f64,
}

fn rotate(v: Vec3, axis: Vec3, angle: f64) -> Vec3 {
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
pub(super) fn fit_plane(
    points: &[Vec3],
    normal: Vec3,
    axes: &[Vec3],
) -> Result<PlaneFit, ModelError> {
    if points.len() < 3 {
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
    if q.iter().all(|&p| length(cross(line, p)) < 1e-9) {
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
pub(super) fn fit_circle(points: &[[f64; 2]], fit: Fit) -> Result<([f64; 2], f64), ModelError> {
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
