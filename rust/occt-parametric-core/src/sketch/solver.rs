//! Newton least-squares solve of sketch constraints with a sparse Jacobian.

use super::*;

/// Column equilibration avoids dropping independent small derivatives simply
/// because a disconnected component has much larger ones. Rank additionally
/// normalizes equation rows; damping is excluded from rank reporting.
pub fn equilibrated(j: &SparseJacobian, normalize_rows: bool) -> (SparseJacobian, Vec<f64>) {
    let mut rows = j.rows.clone();
    if normalize_rows {
        for row in &mut rows {
            let norm = row
                .iter()
                .fold(0_f64, |a, (_, v)| a.hypot(*v))
                .max(f64::MIN_POSITIVE);
            for (_, v) in row {
                *v /= norm;
            }
        }
    }
    let mut norms = vec![0_f64; j.columns];
    for row in &rows {
        for &(c, v) in row {
            norms[c] = norms[c].hypot(v);
        }
    }
    for row in &mut rows {
        for (c, v) in row {
            *v /= norms[*c].max(f64::MIN_POSITIVE);
        }
    }
    (
        SparseJacobian {
            rows,
            columns: j.columns,
        },
        norms,
    )
}

pub fn least_squares_step(
    jacobian: &SparseJacobian,
    residual: &[f64],
    damping: f64,
    condition: bool,
) -> Option<Vec<f64>> {
    if !condition {
        let scale = max_abs(residual).max(1.);
        let rhs =
            jacobian.transpose_times(&residual.iter().map(|r| -r / scale).collect::<Vec<_>>());
        let mut normal = jacobian.normal_matrix();
        for i in 0..normal.size() {
            normal.add_diagonal(i, damping * normal.diagonal(i));
        }
        let step: Vec<_> = normal
            .solve_dropping_null(rhs, PIVOT_TOLERANCE)?
            .into_iter()
            .map(|v| v * scale)
            .collect();
        return step.iter().all(|v| v.is_finite()).then_some(step);
    }

    // Equilibrate equations as well as columns. Uniform damping in the
    // equation-scaled physical coordinates avoids sending a nearly free
    // coordinate to infinity to satisfy an angular equation approximately.
    let mut weighted = SparseJacobian {
        rows: jacobian.rows.clone(),
        columns: jacobian.columns,
    };
    let mut residual = residual.to_vec();
    for (row, r) in weighted.rows.iter_mut().zip(&mut residual) {
        let norm = row.iter().fold(0_f64, |a, (_, v)| a.hypot(*v));
        if norm > 0. {
            for (_, v) in row {
                *v /= norm;
            }
            *r /= norm;
        }
    }
    let (j, norms) = equilibrated(&weighted, false);
    let scale = max_abs(&residual).max(1.);
    let scaled: Vec<_> = residual.iter().map(|v| -v / scale).collect();
    let rhs = j.transpose_times(&scaled);
    let mut normal = j.normal_matrix();
    for (i, norm) in norms.iter().enumerate() {
        if *norm > 0. {
            normal.add_diagonal(i, damping / norm.powi(2).max(f64::MIN_POSITIVE));
        }
    }
    let step = normal.solve_dropping_null(rhs, PIVOT_TOLERANCE)?;
    let step: Vec<_> = step
        .iter()
        .zip(norms)
        .map(|(v, norm)| if norm == 0. { 0. } else { (v / norm) * scale })
        .collect();
    step.iter().all(|v| v.is_finite()).then_some(step)
}
