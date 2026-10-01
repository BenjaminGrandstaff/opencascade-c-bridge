//! Sparse least-squares algebra for the placement solver.
//!
//! Each assembly relationship touches at most two free instances, so the
//! Jacobian has at most twelve nonzeros per row and the normal matrix
//! `JᵀJ` is block sparse. Symmetric elimination with minimum-degree ordering
//! solves and ranks it: chains and trees produce almost no fill-in, and
//! disconnected groups of instances never interact. Normal matrices are
//! positive semidefinite, so elimination needs no pivoting, and a vanishing
//! pivot means its whole remaining row vanishes.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::hash::{BuildHasherDefault, Hasher};

/// Multiplicative hashing for column indices. The default SipHash is built
/// to resist adversarial keys, which column indices are not, and dominated
/// elimination time on grid-like assemblies.
#[derive(Default)]
struct IndexHasher(u64);

impl Hasher for IndexHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn write_usize(&mut self, value: usize) {
        self.0 = (value as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

type Row = HashMap<usize, f64, BuildHasherDefault<IndexHasher>>;

/// A Jacobian stored as sparse rows of `(column, value)` pairs.
pub(crate) struct SparseJacobian {
    pub(crate) rows: Vec<Vec<(usize, f64)>>,
    pub(crate) columns: usize,
}

impl SparseJacobian {
    /// `JᵀJ`. O(sum of squared row lengths).
    pub(crate) fn normal_matrix(&self) -> SymmetricMatrix {
        let mut normal = SymmetricMatrix::new(self.columns);
        for row in &self.rows {
            for &(i, a) in row {
                for &(j, b) in row {
                    normal.add(i, j, a * b);
                }
            }
        }
        normal
    }

    /// `Jᵀv`. O(nonzeros).
    pub(crate) fn transpose_times(&self, values: &[f64]) -> Vec<f64> {
        let mut result = vec![0.0; self.columns];
        for (row, value) in self.rows.iter().zip(values) {
            for &(column, entry) in row {
                result[column] += entry * value;
            }
        }
        result
    }
}

/// A symmetric matrix stored as full sparse rows.
#[derive(Clone)]
pub(crate) struct SymmetricMatrix {
    rows: Vec<Row>,
}

impl SymmetricMatrix {
    pub(crate) fn new(size: usize) -> Self {
        Self {
            rows: vec![Row::default(); size],
        }
    }

    pub(crate) fn size(&self) -> usize {
        self.rows.len()
    }

    /// Adds `value` at `(i, j)`; callers add each symmetric pair themselves,
    /// as `normal_matrix` does by visiting both orders.
    pub(crate) fn add(&mut self, i: usize, j: usize, value: f64) {
        if value != 0.0 {
            *self.rows[i].entry(j).or_insert(0.0) += value;
        }
    }

    pub(crate) fn diagonal(&self, index: usize) -> f64 {
        self.rows[index].get(&index).copied().unwrap_or(0.0)
    }

    pub(crate) fn largest_diagonal(&self) -> f64 {
        (0..self.size()).fold(0.0_f64, |largest, index| largest.max(self.diagonal(index)))
    }

    pub(crate) fn add_diagonal(&mut self, index: usize, value: f64) {
        *self.rows[index].entry(index).or_insert(0.0) += value;
    }

    /// `Av`. O(nonzeros).
    pub(crate) fn times(&self, vector: &[f64]) -> Vec<f64> {
        self.rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|(&column, &entry)| entry * vector[column])
                    .sum()
            })
            .collect()
    }

    /// Minimum-change solve of a positive semidefinite system: variables
    /// whose pivot is at or below `relative_tolerance` times the largest
    /// diagonal are unconstrained, receive zero, and drop out, while every
    /// constrained direction is solved exactly. `None` if the result is not
    /// finite.
    pub(crate) fn solve_dropping_null(
        mut self,
        mut rhs: Vec<f64>,
        relative_tolerance: f64,
    ) -> Option<Vec<f64>> {
        let floor = relative_tolerance * self.largest_diagonal().max(f64::MIN_POSITIVE);
        let steps = self.eliminate_with_floor(&mut rhs, floor);
        let solution = back_substitute(&steps, &rhs);
        solution
            .iter()
            .all(|value| value.is_finite())
            .then_some(solution)
    }

    /// Numerical rank: pivots at or below `relative_tolerance` times the
    /// largest diagonal count as zero.
    pub(crate) fn rank(mut self, relative_tolerance: f64) -> usize {
        let floor = relative_tolerance * self.largest_diagonal().max(f64::MIN_POSITIVE);
        let mut rhs = vec![0.0; self.size()];
        self.eliminate_with_floor(&mut rhs, floor).len()
    }

    /// Minimum-degree symmetric elimination. Variables whose pivot is at or
    /// below `floor` are dropped (their rows are numerically zero in a
    /// positive semidefinite matrix) and do not appear in the returned steps.
    /// O(updates * log n) to pick variables plus the fill-in work, which
    /// stays small for chains and trees and grows with grid-like cycles.
    fn eliminate_with_floor(&mut self, rhs: &mut [f64], floor: f64) -> Vec<Step> {
        let size = self.size();
        let mut active = vec![true; size];
        let mut steps = Vec::with_capacity(size);
        // Lazy minimum-degree queue: entries go stale when a row's degree
        // changes and are skipped when popped. O(nonzero updates * log n).
        let mut queue = (0..size)
            .map(|index| Reverse((self.rows[index].len(), index)))
            .collect::<BinaryHeap<_>>();
        while let Some(Reverse((degree, variable))) = queue.pop() {
            if !active[variable] || degree != self.rows[variable].len() {
                continue;
            }
            active[variable] = false;
            let row = std::mem::take(&mut self.rows[variable]);
            let pivot = row.get(&variable).copied().unwrap_or(0.0);
            let neighbors = row
                .iter()
                .filter(|&(&column, _)| column != variable)
                .map(|(&column, &entry)| (column, entry))
                .collect::<Vec<_>>();
            if pivot <= floor {
                for &(neighbor, _) in &neighbors {
                    self.rows[neighbor].remove(&variable);
                    queue.push(Reverse((self.rows[neighbor].len(), neighbor)));
                }
                continue;
            }
            for &(target, target_entry) in &neighbors {
                let factor = target_entry / pivot;
                let target_row = &mut self.rows[target];
                target_row.remove(&variable);
                for &(column, entry) in &neighbors {
                    *target_row.entry(column).or_insert(0.0) -= factor * entry;
                }
                rhs[target] -= factor * rhs[variable];
                queue.push(Reverse((target_row.len(), target)));
            }
            steps.push(Step {
                variable,
                pivot,
                row: neighbors,
            });
        }
        steps
    }
}

/// Back substitution in reverse elimination order; dropped variables stay 0.
fn back_substitute(steps: &[Step], rhs: &[f64]) -> Vec<f64> {
    let mut solution = vec![0.0; rhs.len()];
    for step in steps.iter().rev() {
        let known = step
            .row
            .iter()
            .map(|&(column, entry)| entry * solution[column])
            .sum::<f64>();
        solution[step.variable] = (rhs[step.variable] - known) / step.pivot;
    }
    solution
}

/// One elimination: the pivot and the row entries of variables eliminated
/// after it, used for back substitution.
struct Step {
    variable: usize,
    pivot: f64,
    row: Vec<(usize, f64)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jacobian(rows: Vec<Vec<(usize, f64)>>, columns: usize) -> SparseJacobian {
        SparseJacobian { rows, columns }
    }

    #[test]
    fn solves_positive_definite_systems_like_dense_elimination() {
        // A chain Laplacian plus identity, the shape of a stacked assembly.
        let size = 40;
        let mut matrix = SymmetricMatrix::new(size);
        for index in 0..size {
            matrix.add_diagonal(index, 3.0);
            if index + 1 < size {
                matrix.add(index, index + 1, -1.0);
                matrix.add(index + 1, index, -1.0);
            }
        }
        let expected = (0..size)
            .map(|index| (index as f64 * 0.37).sin())
            .collect::<Vec<_>>();
        let rhs = matrix.times(&expected);
        let solution = matrix.clone().solve_dropping_null(rhs, 1e-12).unwrap();
        for (value, wanted) in solution.iter().zip(&expected) {
            assert!((value - wanted).abs() < 1e-12);
        }
    }

    #[test]
    fn ranks_normal_matrices_with_dependent_and_empty_columns() {
        // Columns 0 and 1 are dependent, column 3 is unused.
        let jacobian = jacobian(
            vec![
                vec![(0, 1.0), (1, 2.0)],
                vec![(0, 2.0), (1, 4.0)],
                vec![(2, 1.0)],
                vec![(0, 1.0), (1, 2.0), (2, -1.0)],
            ],
            4,
        );
        assert_eq!(jacobian.normal_matrix().rank(1e-12), 2);
        let gradient = jacobian.transpose_times(&[1.0, 0.0, 0.0, 0.0]);
        assert_eq!(gradient, vec![1.0, 2.0, 0.0, 0.0]);
        // Dropped columns get zero: the dependent pair keeps one variable and
        // the unused column stays put.
        let normal = jacobian.normal_matrix();
        let rhs = normal.times(&[0.2, 0.4, -1.0, 0.0]);
        let solution = normal
            .clone()
            .solve_dropping_null(rhs.clone(), 1e-12)
            .unwrap();
        assert_eq!(solution[3], 0.0);
        assert!(solution[0] == 0.0 || solution[1] == 0.0);
        for (value, wanted) in normal.times(&solution).iter().zip(&rhs) {
            assert!((value - wanted).abs() < 1e-9);
        }
    }
}
