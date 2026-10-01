//! Constraint-solved two-dimensional line sketches.

use super::*;
use crate::assembly::{add, dot, scale, unit};
use std::collections::{HashMap, HashSet};

const MAX_ITERATIONS: usize = 100;
const RESIDUAL_TOLERANCE: f64 = 1e-9;
const DIFFERENCE_STEP: f64 = 1e-6;
const PIVOT_TOLERANCE: f64 = 1e-10;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchPoint {
    pub id: String,
    pub x: ScalarExpr,
    pub y: ScalarExpr,
    #[serde(default)]
    pub fixed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchLine {
    pub id: String,
    pub start: String,
    pub end: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchConstraint {
    Coincident {
        first: String,
        second: String,
    },
    Horizontal {
        line: String,
    },
    Vertical {
        line: String,
    },
    Parallel {
        first: String,
        second: String,
    },
    Perpendicular {
        first: String,
        second: String,
    },
    EqualLength {
        first: String,
        second: String,
    },
    Distance {
        first: String,
        second: String,
        value: ScalarExpr,
    },
}

/// A line sketch in a typed 3D plane. Lines are ordered as one closed profile
/// when used by `FeatureOperation::SketchFace`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchDefinition {
    pub id: String,
    pub origin: VectorExpr,
    pub x_axis: VectorExpr,
    pub y_axis: VectorExpr,
    pub points: Vec<SketchPoint>,
    pub lines: Vec<SketchLine>,
    #[serde(default)]
    pub constraints: Vec<SketchConstraint>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SketchPoint2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SketchSolution {
    pub points: HashMap<String, SketchPoint2>,
    pub solved: bool,
    pub iterations: usize,
    pub max_residual: f64,
    pub free_degrees: usize,
    pub redundant_equations: usize,
}

impl SketchDefinition {
    pub(crate) fn collect_parameters<'a>(&'a self, names: &mut HashSet<&'a str>) {
        collect_vector_parameters(&self.origin, names);
        collect_vector_parameters(&self.x_axis, names);
        collect_vector_parameters(&self.y_axis, names);
        for point in &self.points {
            collect_scalar_parameters(&point.x, names);
            collect_scalar_parameters(&point.y, names);
        }
        for constraint in &self.constraints {
            if let SketchConstraint::Distance { value, .. } = constraint {
                collect_scalar_parameters(value, names);
            }
        }
    }

    /// Solves the sketch with a finite-difference dense least-squares system.
    /// For `v` free coordinates, `c` residual components, and `i` iterations,
    /// time is O(i(c v^2 + v^3)) and memory is O(c v + v^2). This first slice
    /// targets small feature sketches; sparse decomposition remains required
    /// before the roadmap item can be considered complete for large sketches.
    pub fn solve(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<SketchSolution, ModelError> {
        self.validate(parameters)?;
        let mut values = Vec::new();
        let mut variables = HashMap::new();
        let mut fixed = HashMap::new();
        for point in &self.points {
            let value = SketchPoint2 {
                x: scalar(&point.x, parameters, Dimension::Length)?,
                y: scalar(&point.y, parameters, Dimension::Length)?,
            };
            if point.fixed {
                fixed.insert(point.id.as_str(), value);
            } else {
                variables.insert(point.id.as_str(), values.len());
                values.extend([value.x, value.y]);
            }
        }
        let problem = SketchProblem {
            sketch: self,
            parameters,
            variables,
            fixed,
        };
        let mut residual = problem.residuals(&values)?;
        let mut iterations = 0;
        while iterations < MAX_ITERATIONS && max_abs(&residual) > RESIDUAL_TOLERANCE {
            iterations += 1;
            let jacobian = problem.jacobian(&values, &residual)?;
            let Some(step) = least_squares_step(&jacobian, &residual) else {
                break;
            };
            let current_cost = squared_norm(&residual);
            let mut scale = 1.0;
            let mut accepted = None;
            while scale >= 1e-6 {
                let trial = values
                    .iter()
                    .zip(&step)
                    .map(|(value, step)| value + scale * step)
                    .collect::<Vec<_>>();
                let trial_residual = problem.residuals(&trial)?;
                if squared_norm(&trial_residual) < current_cost {
                    accepted = Some((trial, trial_residual));
                    break;
                }
                scale *= 0.5;
            }
            let Some((trial, trial_residual)) = accepted else {
                break;
            };
            values = trial;
            residual = trial_residual;
        }
        let jacobian = problem.jacobian(&values, &residual)?;
        let rank = matrix_rank(&jacobian);
        let mut points = self
            .points
            .iter()
            .filter(|point| point.fixed)
            .map(|point| (point.id.clone(), problem.fixed[point.id.as_str()]))
            .collect::<HashMap<_, _>>();
        for (id, index) in &problem.variables {
            points.insert(
                (*id).to_owned(),
                SketchPoint2 {
                    x: values[*index],
                    y: values[*index + 1],
                },
            );
        }
        Ok(SketchSolution {
            points,
            solved: max_abs(&residual) <= RESIDUAL_TOLERANCE,
            iterations,
            max_residual: max_abs(&residual),
            free_degrees: values.len().saturating_sub(rank),
            redundant_equations: residual.len().saturating_sub(rank),
        })
    }

    pub(crate) fn face<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Shape<'session>, ModelError> {
        let solution = self.solve(parameters)?;
        if !solution.solved {
            return Err(ModelError::new(format!(
                "sketch '{}' constraints conflict; maximum residual {}",
                self.id, solution.max_residual
            )));
        }
        if self.lines.len() < 3 {
            return Err(ModelError::new(format!(
                "sketch '{}' profile requires at least three lines",
                self.id
            )));
        }
        for pair in self.lines.windows(2) {
            if pair[0].end != pair[1].start {
                return Err(ModelError::new(format!(
                    "sketch '{}' lines are not a continuous ordered profile",
                    self.id
                )));
            }
        }
        if self.lines.last().unwrap().end != self.lines[0].start {
            return Err(ModelError::new(format!(
                "sketch '{}' profile is not closed",
                self.id
            )));
        }
        let origin = vector(&self.origin, parameters, Dimension::Length)?;
        let x_axis = unit(vector(&self.x_axis, parameters, Dimension::Scalar)?)?;
        let y_axis = unit(vector(&self.y_axis, parameters, Dimension::Scalar)?)?;
        if dot(x_axis, y_axis).abs() > 1e-9 {
            return Err(ModelError::new("sketch plane axes must be perpendicular"));
        }
        let points = self
            .lines
            .iter()
            .map(|line| {
                let point = solution.points[&line.start];
                add(origin, add(scale(x_axis, point.x), scale(y_axis, point.y)))
            })
            .collect::<Vec<_>>();
        let wire = session.create_polyline_wire(&points, true)?;
        match session.create_face_from_wire(&wire) {
            Ok(face) => {
                session.remove(wire)?;
                Ok(face)
            }
            Err(error) => {
                let _ = session.remove(wire);
                Err(error.into())
            }
        }
    }

    pub(crate) fn validate(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<(), ModelError> {
        let (point_ids, line_ids) = self.validate_structure()?;
        for constraint in &self.constraints {
            constraint.validate(&point_ids, &line_ids, parameters)?;
        }
        Ok(())
    }

    pub(crate) fn validate_structure(&self) -> Result<(HashSet<&str>, HashSet<&str>), ModelError> {
        if self.id.is_empty() {
            return Err(ModelError::new("sketch id must be nonempty"));
        }
        let mut point_ids = HashSet::new();
        for point in &self.points {
            if point.id.is_empty() || !point_ids.insert(point.id.as_str()) {
                return Err(ModelError::new(
                    "sketch point ids must be nonempty and unique",
                ));
            }
        }
        let mut line_ids = HashSet::new();
        for line in &self.lines {
            if line.id.is_empty() || !line_ids.insert(line.id.as_str()) {
                return Err(ModelError::new(
                    "sketch line ids must be nonempty and unique",
                ));
            }
            if line.start == line.end
                || !point_ids.contains(line.start.as_str())
                || !point_ids.contains(line.end.as_str())
            {
                return Err(ModelError::new(format!(
                    "sketch line '{}' has invalid point references",
                    line.id
                )));
            }
        }
        for constraint in &self.constraints {
            constraint.validate_references(&point_ids, &line_ids)?;
        }
        Ok((point_ids, line_ids))
    }
}

impl SketchConstraint {
    fn validate_references(
        &self,
        points: &HashSet<&str>,
        lines: &HashSet<&str>,
    ) -> Result<(), ModelError> {
        let point = |id: &str| {
            points
                .contains(id)
                .then_some(())
                .ok_or_else(|| ModelError::new(format!("unknown sketch point '{id}'")))
        };
        let line = |id: &str| {
            lines
                .contains(id)
                .then_some(())
                .ok_or_else(|| ModelError::new(format!("unknown sketch line '{id}'")))
        };
        match self {
            Self::Coincident { first, second } | Self::Distance { first, second, .. } => {
                point(first)?;
                point(second)
            }
            Self::Horizontal { line: id } | Self::Vertical { line: id } => line(id),
            Self::Parallel { first, second }
            | Self::Perpendicular { first, second }
            | Self::EqualLength { first, second } => {
                line(first)?;
                line(second)
            }
        }
    }

    fn validate(
        &self,
        points: &HashSet<&str>,
        lines: &HashSet<&str>,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<(), ModelError> {
        self.validate_references(points, lines)?;
        match self {
            Self::Distance { value, .. } => {
                let value = scalar(value, parameters, Dimension::Length)?;
                if value >= 0.0 {
                    Ok(())
                } else {
                    Err(ModelError::new("sketch distance must be nonnegative"))
                }
            }
            _ => Ok(()),
        }
    }
}

struct SketchProblem<'a> {
    sketch: &'a SketchDefinition,
    parameters: &'a HashMap<String, ParameterValue>,
    variables: HashMap<&'a str, usize>,
    fixed: HashMap<&'a str, SketchPoint2>,
}

impl SketchProblem<'_> {
    fn point(&self, id: &str, values: &[f64]) -> SketchPoint2 {
        self.variables.get(id).map_or_else(
            || self.fixed[id],
            |index| SketchPoint2 {
                x: values[*index],
                y: values[*index + 1],
            },
        )
    }

    fn line(&self, id: &str, values: &[f64]) -> (SketchPoint2, SketchPoint2) {
        let line = self.sketch.lines.iter().find(|line| line.id == id).unwrap();
        (
            self.point(&line.start, values),
            self.point(&line.end, values),
        )
    }

    fn residuals(&self, values: &[f64]) -> Result<Vec<f64>, ModelError> {
        let mut residuals = Vec::new();
        for constraint in &self.sketch.constraints {
            match constraint {
                SketchConstraint::Coincident { first, second } => {
                    let (a, b) = (self.point(first, values), self.point(second, values));
                    residuals.extend([a.x - b.x, a.y - b.y]);
                }
                SketchConstraint::Horizontal { line } => {
                    let (a, b) = self.line(line, values);
                    residuals.push(b.y - a.y);
                }
                SketchConstraint::Vertical { line } => {
                    let (a, b) = self.line(line, values);
                    residuals.push(b.x - a.x);
                }
                SketchConstraint::Parallel { first, second } => {
                    residuals.push(normalized_cross(
                        self.line(first, values),
                        self.line(second, values),
                    )?);
                }
                SketchConstraint::Perpendicular { first, second } => {
                    residuals.push(normalized_dot(
                        self.line(first, values),
                        self.line(second, values),
                    )?);
                }
                SketchConstraint::EqualLength { first, second } => {
                    residuals.push(
                        line_length(self.line(first, values))
                            - line_length(self.line(second, values)),
                    );
                }
                SketchConstraint::Distance {
                    first,
                    second,
                    value,
                } => {
                    let (a, b) = (self.point(first, values), self.point(second, values));
                    let target = scalar(value, self.parameters, Dimension::Length)?;
                    residuals.push((b.x - a.x).hypot(b.y - a.y) - target);
                }
            }
        }
        Ok(residuals)
    }

    fn jacobian(&self, values: &[f64], base: &[f64]) -> Result<Vec<Vec<f64>>, ModelError> {
        let mut jacobian = vec![vec![0.0; values.len()]; base.len()];
        for column in 0..values.len() {
            let mut shifted = values.to_vec();
            let step = DIFFERENCE_STEP * values[column].abs().max(1.0);
            shifted[column] += step;
            for (row, (shifted, base)) in self.residuals(&shifted)?.iter().zip(base).enumerate() {
                jacobian[row][column] = (shifted - base) / step;
            }
        }
        Ok(jacobian)
    }
}

fn line_length((a, b): (SketchPoint2, SketchPoint2)) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

fn normalized_cross(
    a: (SketchPoint2, SketchPoint2),
    b: (SketchPoint2, SketchPoint2),
) -> Result<f64, ModelError> {
    let (u, v) = (
        (a.1.x - a.0.x, a.1.y - a.0.y),
        (b.1.x - b.0.x, b.1.y - b.0.y),
    );
    let scale = u.0.hypot(u.1) * v.0.hypot(v.1);
    if scale <= f64::EPSILON {
        return Err(ModelError::new("sketch line has zero length"));
    }
    Ok((u.0 * v.1 - u.1 * v.0) / scale)
}

fn normalized_dot(
    a: (SketchPoint2, SketchPoint2),
    b: (SketchPoint2, SketchPoint2),
) -> Result<f64, ModelError> {
    let (u, v) = (
        (a.1.x - a.0.x, a.1.y - a.0.y),
        (b.1.x - b.0.x, b.1.y - b.0.y),
    );
    let scale = u.0.hypot(u.1) * v.0.hypot(v.1);
    if scale <= f64::EPSILON {
        return Err(ModelError::new("sketch line has zero length"));
    }
    Ok((u.0 * v.0 + u.1 * v.1) / scale)
}

fn least_squares_step(jacobian: &[Vec<f64>], residual: &[f64]) -> Option<Vec<f64>> {
    let columns = jacobian.first().map_or(0, Vec::len);
    if columns == 0 {
        return None;
    }
    let mut normal = vec![vec![0.0; columns + 1]; columns];
    for row in 0..columns {
        for column in 0..columns {
            normal[row][column] = jacobian
                .iter()
                .map(|values| values[row] * values[column])
                .sum();
        }
        normal[row][row] += 1e-12;
        normal[row][columns] = -jacobian
            .iter()
            .zip(residual)
            .map(|(values, residual)| values[row] * residual)
            .sum::<f64>();
    }
    solve_dense(normal)
}

fn solve_dense(mut matrix: Vec<Vec<f64>>) -> Option<Vec<f64>> {
    let size = matrix.len();
    for column in 0..size {
        let pivot = (column..size).max_by(|a, b| {
            matrix[*a][column]
                .abs()
                .total_cmp(&matrix[*b][column].abs())
        })?;
        if matrix[pivot][column].abs() <= PIVOT_TOLERANCE {
            continue;
        }
        matrix.swap(column, pivot);
        let divisor = matrix[column][column];
        for value in &mut matrix[column][column..] {
            *value /= divisor;
        }
        for row in 0..size {
            if row == column {
                continue;
            }
            let factor = matrix[row][column];
            let pivot = matrix[column][column..=size].to_vec();
            for (value, pivot) in matrix[row][column..=size].iter_mut().zip(pivot) {
                *value -= factor * pivot;
            }
        }
    }
    Some(
        (0..size)
            .map(|row| {
                if matrix[row][row].abs() > PIVOT_TOLERANCE {
                    matrix[row][size]
                } else {
                    0.0
                }
            })
            .collect(),
    )
}

fn matrix_rank(matrix: &[Vec<f64>]) -> usize {
    if matrix.is_empty() {
        return 0;
    }
    let mut matrix = matrix.to_vec();
    let (rows, columns) = (matrix.len(), matrix[0].len());
    let mut rank = 0;
    for column in 0..columns {
        let Some(pivot) = (rank..rows).max_by(|a, b| {
            matrix[*a][column]
                .abs()
                .total_cmp(&matrix[*b][column].abs())
        }) else {
            break;
        };
        if matrix[pivot][column].abs() <= PIVOT_TOLERANCE {
            continue;
        }
        matrix.swap(rank, pivot);
        let divisor = matrix[rank][column];
        for value in &mut matrix[rank][column..] {
            *value /= divisor;
        }
        for row in rank + 1..rows {
            let factor = matrix[row][column];
            let pivot = matrix[rank][column..columns].to_vec();
            for (value, pivot) in matrix[row][column..columns].iter_mut().zip(pivot) {
                *value -= factor * pivot;
            }
        }
        rank += 1;
        if rank == rows {
            break;
        }
    }
    rank
}

fn max_abs(values: &[f64]) -> f64 {
    values
        .iter()
        .fold(0.0_f64, |largest, value| largest.max(value.abs()))
}
fn squared_norm(values: &[f64]) -> f64 {
    values.iter().map(|value| value * value).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn length(value: f64) -> ScalarExpr {
        ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
    }

    fn point(id: &str, x: f64, y: f64, fixed: bool) -> SketchPoint {
        SketchPoint {
            id: id.into(),
            x: length(x),
            y: length(y),
            fixed,
        }
    }

    fn rectangle() -> SketchDefinition {
        SketchDefinition {
            id: "rectangle".into(),
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
            y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
            points: vec![
                point("p0", 0.0, 0.0, true),
                point("p1", 9.0, 1.0, false),
                point("p2", 9.0, 8.0, false),
                point("p3", 1.0, 8.0, false),
                point("anchor", 1.0, 1.0, false),
            ],
            lines: vec![
                SketchLine {
                    id: "bottom".into(),
                    start: "p0".into(),
                    end: "p1".into(),
                },
                SketchLine {
                    id: "right".into(),
                    start: "p1".into(),
                    end: "p2".into(),
                },
                SketchLine {
                    id: "top".into(),
                    start: "p2".into(),
                    end: "p3".into(),
                },
                SketchLine {
                    id: "left".into(),
                    start: "p3".into(),
                    end: "p0".into(),
                },
            ],
            constraints: vec![
                SketchConstraint::Coincident {
                    first: "anchor".into(),
                    second: "p0".into(),
                },
                SketchConstraint::Horizontal {
                    line: "bottom".into(),
                },
                SketchConstraint::Vertical {
                    line: "right".into(),
                },
                SketchConstraint::Horizontal { line: "top".into() },
                SketchConstraint::Vertical {
                    line: "left".into(),
                },
                SketchConstraint::Parallel {
                    first: "bottom".into(),
                    second: "top".into(),
                },
                SketchConstraint::Perpendicular {
                    first: "bottom".into(),
                    second: "right".into(),
                },
                SketchConstraint::EqualLength {
                    first: "bottom".into(),
                    second: "top".into(),
                },
                SketchConstraint::Distance {
                    first: "p0".into(),
                    second: "p1".into(),
                    value: length(10.0),
                },
                SketchConstraint::Distance {
                    first: "p1".into(),
                    second: "p2".into(),
                    value: length(5.0),
                },
            ],
        }
    }

    #[test]
    fn solves_dimensioned_rectangle_and_generates_face() {
        let sketch = rectangle();
        let solution = sketch.solve(&HashMap::new()).unwrap();
        assert!(solution.solved, "{solution:?}");
        assert_eq!(solution.free_degrees, 0);
        assert!(solution.redundant_equations > 0);
        assert!((solution.points["p2"].x - 10.0).abs() < 1e-8);
        assert!((solution.points["p2"].y - 5.0).abs() < 1e-8);

        let session = Session::new().unwrap();
        let face = sketch.face(&session, &HashMap::new()).unwrap();
        assert!((session.surface_area(&face).unwrap() - 50.0).abs() < 1e-7);
        session.remove(face).unwrap();

        let family = FamilyDefinition {
            id: "SketchPart".into(),
            version: 1,
            parameters: Vec::new(),
            derived_parameters: Vec::new(),
            derived_vector_parameters: Vec::new(),
            constraints: Vec::new(),
            features: vec![FeatureDefinition {
                id: "profile".into(),
                operation: FeatureOperation::SketchFace { sketch },
            }],
            requirements: Vec::new(),
            datums: Vec::new(),
        };
        let instance = PartInstance {
            id: "part".into(),
            definition: &family,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let generated = instance.regenerate(&session).unwrap();
        assert!(
            (session
                .surface_area(generated.shape("profile").unwrap())
                .unwrap()
                - 50.0)
                .abs()
                < 1e-7
        );
        let mut graph = InstanceGraph::new(&family);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
    }

    #[test]
    fn reports_conflicts_free_degrees_and_invalid_profiles() {
        let mut sketch = rectangle();
        sketch.points[1].fixed = true;
        let conflict = sketch.solve(&HashMap::new()).unwrap();
        assert!(!conflict.solved);
        assert!(conflict.max_residual > 0.1);

        let mut free = rectangle();
        free.constraints.clear();
        let solution = free.solve(&HashMap::new()).unwrap();
        assert!(solution.solved);
        assert_eq!(solution.free_degrees, 8);

        let mut open = rectangle();
        open.lines.pop();
        let session = Session::new().unwrap();
        assert!(open.face(&session, &HashMap::new()).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);

        let mut invalid = rectangle();
        invalid.constraints.push(SketchConstraint::Horizontal {
            line: "missing".into(),
        });
        assert!(invalid.solve(&HashMap::new()).is_err());
    }
}
