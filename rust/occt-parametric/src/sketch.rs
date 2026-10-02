//! Constraint-solved two-dimensional sketches with exact circular geometry.

use super::*;
use crate::assembly::{add, cross, dot, scale, unit};
use crate::sparse::SparseJacobian;
use occt_bridge::WireSegment;
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

/// A full circle. The center-to-rim distance defines its radius.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchCircle {
    pub id: String,
    pub center: String,
    pub rim: String,
}

/// A circular arc; the solver enforces equal start/end radii automatically.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchArc {
    pub id: String,
    pub center: String,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub clockwise: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchConstraint {
    /// Tangency at a shared named endpoint (or a circle's rim point).
    Tangent {
        first: String,
        second: String,
        point: String,
    },
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

/// A sketch in a typed 3D plane. `profile` names an ordered, closed boundary;
/// omitted entities are construction geometry. An empty profile uses all lines
/// in their original order, or a sole circle when there are no lines/arcs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchDefinition {
    pub id: String,
    /// Optional plane datum in the owning family. Its origin/normal replace
    /// `origin`/`y_axis`; `x_axis` must be perpendicular to the datum normal.
    #[serde(default)]
    pub datum_plane: Option<String>,
    pub origin: VectorExpr,
    pub x_axis: VectorExpr,
    pub y_axis: VectorExpr,
    pub points: Vec<SketchPoint>,
    pub lines: Vec<SketchLine>,
    #[serde(default)]
    pub circles: Vec<SketchCircle>,
    #[serde(default)]
    pub arcs: Vec<SketchArc>,
    #[serde(default)]
    pub profile: Vec<String>,
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

#[derive(Clone, Copy)]
enum Entity<'a> {
    Line(&'a SketchLine),
    Circle(&'a SketchCircle),
    Arc(&'a SketchArc),
}

impl<'a> Entity<'a> {
    fn endpoints(self) -> (&'a str, &'a str) {
        match self {
            Self::Line(line) => (&line.start, &line.end),
            Self::Circle(circle) => (&circle.rim, &circle.rim),
            Self::Arc(arc) => (&arc.start, &arc.end),
        }
    }

    fn tangent_points(self, contact: &'a str) -> (&'a str, &'a str) {
        match self {
            Self::Line(line) => (&line.start, &line.end),
            Self::Circle(circle) => (&circle.center, contact),
            Self::Arc(arc) => (&arc.center, contact),
        }
    }
}

impl SketchDefinition {
    fn entities(&self) -> HashMap<&str, Entity<'_>> {
        self.lines
            .iter()
            .map(|line| (line.id.as_str(), Entity::Line(line)))
            .chain(
                self.circles
                    .iter()
                    .map(|circle| (circle.id.as_str(), Entity::Circle(circle))),
            )
            .chain(
                self.arcs
                    .iter()
                    .map(|arc| (arc.id.as_str(), Entity::Arc(arc))),
            )
            .collect()
    }

    fn validate_curve_geometry(
        &self,
        points: &HashMap<String, SketchPoint2>,
    ) -> Result<(), ModelError> {
        for (center, rim) in
            self.circles
                .iter()
                .map(|c| (&c.center, &c.rim))
                .chain(self.arcs.iter().flat_map(|a| {
                    [
                        (&a.center, &a.start),
                        (&a.center, &a.end),
                        (&a.start, &a.end),
                    ]
                }))
        {
            let distance = line_length((points[center], points[rim]));
            if !distance.is_finite() || distance <= RESIDUAL_TOLERANCE {
                return Err(ModelError::new(
                    "sketch curve has coincident or non-finite defining points",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn collect_parameters<'a>(&'a self, names: &mut HashSet<&'a str>) {
        collect_vector_parameters(&self.x_axis, names);
        if self.datum_plane.is_none() {
            collect_vector_parameters(&self.origin, names);
            collect_vector_parameters(&self.y_axis, names);
        }
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

    /// Solves using the assembly solver's sparse normal-matrix algebra.
    /// Each constraint touches at most eight coordinates. Jacobian assembly
    /// takes O(points + constraints) time and memory per iteration; elimination
    /// cost depends on fill-in (small for chains and disconnected components).
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
            entities: self.entities(),
            lines: self
                .lines
                .iter()
                .map(|line| (line.id.as_str(), line))
                .collect(),
        };
        let mut residual = problem.residuals(&values)?;
        let mut iterations = 0;
        while iterations < MAX_ITERATIONS && max_abs(&residual) > RESIDUAL_TOLERANCE {
            iterations += 1;
            let jacobian = problem.jacobian(&values)?;
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
        let jacobian = problem.jacobian(&values)?;
        let rank = jacobian.normal_matrix().rank(PIVOT_TOLERANCE);
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
        self.validate_curve_geometry(&points)?;
        Ok(SketchSolution {
            points,
            solved: max_abs(&residual) <= RESIDUAL_TOLERANCE,
            iterations,
            max_residual: max_abs(&residual),
            free_degrees: values.len().saturating_sub(rank),
            redundant_equations: residual.len().saturating_sub(rank),
        })
    }

    #[cfg(test)]
    pub(crate) fn face<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Shape<'session>, ModelError> {
        self.face_on_plane(session, parameters, None)
    }

    pub(crate) fn face_on_plane<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
        datum: Option<ResolvedDatum>,
    ) -> Result<Shape<'session>, ModelError> {
        let wire = self.wire(session, parameters, datum)?;
        let face = session.create_face_from_wire(&wire)?;
        if !session.is_valid(&face)? {
            return Err(ModelError::new("sketch profile produced an invalid face"));
        }
        Ok(face)
    }

    pub(crate) fn wire<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
        datum: Option<ResolvedDatum>,
    ) -> Result<Shape<'session>, ModelError> {
        let solution = self.solve(parameters)?;
        if !solution.solved {
            return Err(ModelError::new(format!(
                "sketch '{}' constraints conflict; maximum residual {}",
                self.id, solution.max_residual
            )));
        }
        let x_axis = unit(vector(&self.x_axis, parameters, Dimension::Scalar)?)?;
        let (origin, y_axis) = match (self.datum_plane.as_ref(), datum) {
            (Some(_), Some(ResolvedDatum::Plane { origin, normal })) => {
                if dot(x_axis, normal).abs() > 1e-9 {
                    return Err(ModelError::new("sketch x axis must lie in its datum plane"));
                }
                (origin, unit(cross(normal, x_axis))?)
            }
            (Some(_), _) => return Err(ModelError::new("sketch requires a resolved plane datum")),
            (None, _) => (
                vector(&self.origin, parameters, Dimension::Length)?,
                unit(vector(&self.y_axis, parameters, Dimension::Scalar)?)?,
            ),
        };
        if dot(x_axis, y_axis).abs() > 1e-9 {
            return Err(ModelError::new("sketch plane axes must be perpendicular"));
        }
        let transform =
            |point: SketchPoint2| add(origin, add(scale(x_axis, point.x), scale(y_axis, point.y)));
        let entities = self.entities();
        let profile = if !self.profile.is_empty() {
            self.profile
                .iter()
                .map(|id| entities[id.as_str()])
                .collect::<Vec<_>>()
        } else if self.circles.len() == 1 && self.lines.is_empty() && self.arcs.is_empty() {
            vec![Entity::Circle(&self.circles[0])]
        } else if self.arcs.is_empty() && self.circles.is_empty() {
            self.lines.iter().map(Entity::Line).collect()
        } else {
            return Err(ModelError::new(
                "mixed sketch geometry requires an explicit profile",
            ));
        };
        let wire = if let [Entity::Circle(circle)] = profile.as_slice() {
            let center = solution.points[&circle.center];
            let radius = line_length((center, solution.points[&circle.rim]));
            session.create_circle_wire(transform(center), cross(x_axis, y_axis), radius)?
        } else {
            if profile.is_empty()
                || profile
                    .iter()
                    .any(|entity| matches!(entity, Entity::Circle(_)))
            {
                return Err(ModelError::new(
                    "profile must contain connected lines/arcs or one circle",
                ));
            }
            let mut segments = Vec::with_capacity(profile.len());
            for (index, entity) in profile.iter().enumerate() {
                let (start, end) = entity.endpoints();
                if end != profile[(index + 1) % profile.len()].endpoints().0 {
                    return Err(ModelError::new(
                        "sketch profile is not a continuous closed boundary",
                    ));
                }
                let start_point = solution.points[start];
                let end_point = solution.points[end];
                let segment = match entity {
                    Entity::Line(_) => WireSegment::Line {
                        start: transform(start_point),
                        end: transform(end_point),
                    },
                    Entity::Arc(arc) => {
                        let center = solution.points[&arc.center];
                        let angle = (start_point.y - center.y).atan2(start_point.x - center.x);
                        let end_angle = (end_point.y - center.y).atan2(end_point.x - center.x);
                        let sweep = if arc.clockwise {
                            -((angle - end_angle).rem_euclid(std::f64::consts::TAU))
                        } else {
                            (end_angle - angle).rem_euclid(std::f64::consts::TAU)
                        };
                        let radius = line_length((center, start_point));
                        let middle_angle = angle + sweep / 2.0;
                        let middle = SketchPoint2 {
                            x: center.x + radius * middle_angle.cos(),
                            y: center.y + radius * middle_angle.sin(),
                        };
                        WireSegment::Arc {
                            start: transform(start_point),
                            middle: transform(middle),
                            end: transform(end_point),
                        }
                    }
                    Entity::Circle(_) => unreachable!("circles handled above"),
                };
                segments.push(segment);
            }
            session.create_segment_wire(&segments, true)?
        };
        if !session.is_valid(&wire)? {
            return Err(ModelError::new("sketch profile produced an invalid wire"));
        }
        Ok(wire)
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
        let mut entity_ids = line_ids.clone();
        for (id, refs) in self
            .circles
            .iter()
            .map(|c| (c.id.as_str(), vec![c.center.as_str(), c.rim.as_str()]))
            .chain(self.arcs.iter().map(|a| {
                (
                    a.id.as_str(),
                    vec![a.center.as_str(), a.start.as_str(), a.end.as_str()],
                )
            }))
        {
            if id.is_empty() || !entity_ids.insert(id) {
                return Err(ModelError::new(
                    "sketch entity ids must be nonempty and unique",
                ));
            }
            let unique = refs.iter().copied().collect::<HashSet<_>>();
            if unique.len() != refs.len() || refs.iter().any(|id| !point_ids.contains(id)) {
                return Err(ModelError::new(format!(
                    "sketch curve '{id}' has invalid point references"
                )));
            }
        }
        let mut profile_ids = HashSet::new();
        for id in &self.profile {
            if !entity_ids.contains(id.as_str()) || !profile_ids.insert(id.as_str()) {
                return Err(ModelError::new(
                    "sketch profile has unknown or repeated entities",
                ));
            }
        }
        let entities = self.entities();
        for constraint in &self.constraints {
            constraint.validate_references(&point_ids, &line_ids)?;
            if let SketchConstraint::Tangent {
                first,
                second,
                point,
            } = constraint
            {
                if first == second {
                    return Err(ModelError::new("tangency requires two distinct entities"));
                }
                for id in [first, second] {
                    let entity = entities
                        .get(id.as_str())
                        .ok_or_else(|| ModelError::new(format!("unknown sketch entity '{id}'")))?;
                    let (start, end) = entity.endpoints();
                    if point != start && point != end {
                        return Err(ModelError::new(
                            "tangency point must be a shared endpoint or circle rim",
                        ));
                    }
                }
            }
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
            Self::Tangent { point: id, .. } => point(id),
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
    lines: HashMap<&'a str, &'a SketchLine>,
    entities: HashMap<&'a str, Entity<'a>>,
}

#[derive(Clone, Copy)]
enum Equation<'a> {
    Constraint(&'a SketchConstraint),
    Arc(&'a SketchArc),
}

impl SketchProblem<'_> {
    fn equations(&self) -> impl Iterator<Item = Equation<'_>> {
        self.sketch
            .constraints
            .iter()
            .map(Equation::Constraint)
            .chain(self.sketch.arcs.iter().map(Equation::Arc))
    }

    fn equation_residuals(
        &self,
        equation: Equation<'_>,
        values: &[f64],
        residuals: &mut Vec<f64>,
    ) -> Result<(), ModelError> {
        match equation {
            Equation::Constraint(constraint) => {
                self.constraint_residuals(constraint, values, residuals)
            }
            Equation::Arc(arc) => {
                let center = self.point(&arc.center, values);
                let residual = line_length((center, self.point(&arc.start, values)))
                    - line_length((center, self.point(&arc.end, values)));
                if !residual.is_finite() {
                    return Err(ModelError::new("sketch arc residual is not finite"));
                }
                residuals.push(residual);
                Ok(())
            }
        }
    }

    fn tangent(&self, id: &str, contact: &str, values: &[f64]) -> (SketchPoint2, SketchPoint2) {
        let entity = self.entities[id];
        let (first, second) = entity.tangent_points(contact);
        let (a, b) = (self.point(first, values), self.point(second, values));
        let direction = if matches!(entity, Entity::Line(_)) {
            SketchPoint2 {
                x: b.x - a.x,
                y: b.y - a.y,
            }
        } else {
            SketchPoint2 {
                x: a.y - b.y,
                y: b.x - a.x,
            }
        };
        (SketchPoint2 { x: 0.0, y: 0.0 }, direction)
    }

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
        let line = self.lines[id];
        (
            self.point(&line.start, values),
            self.point(&line.end, values),
        )
    }

    fn residuals(&self, values: &[f64]) -> Result<Vec<f64>, ModelError> {
        let mut residuals = Vec::new();
        for equation in self.equations() {
            self.equation_residuals(equation, values, &mut residuals)?;
        }
        Ok(residuals)
    }

    fn constraint_residuals(
        &self,
        constraint: &SketchConstraint,
        values: &[f64],
        residuals: &mut Vec<f64>,
    ) -> Result<(), ModelError> {
        let start = residuals.len();
        match constraint {
            SketchConstraint::Tangent {
                first,
                second,
                point,
            } => {
                residuals.push(normalized_cross(
                    self.tangent(first, point, values),
                    self.tangent(second, point, values),
                )?);
            }
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
                    line_length(self.line(first, values)) - line_length(self.line(second, values)),
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
        if residuals[start..].iter().any(|value| !value.is_finite()) {
            return Err(ModelError::new("sketch residual is not finite"));
        }
        Ok(())
    }

    fn columns(&self, constraint: &SketchConstraint) -> Vec<usize> {
        let points = match constraint {
            SketchConstraint::Tangent {
                first,
                second,
                point,
            } => {
                let (a, b) = self.entities[first.as_str()].tangent_points(point);
                let (c, d) = self.entities[second.as_str()].tangent_points(point);
                vec![a, b, c, d]
            }
            SketchConstraint::Coincident { first, second }
            | SketchConstraint::Distance { first, second, .. } => {
                vec![first.as_str(), second.as_str()]
            }
            SketchConstraint::Horizontal { line } | SketchConstraint::Vertical { line } => {
                let line = self.lines[line.as_str()];
                vec![line.start.as_str(), line.end.as_str()]
            }
            SketchConstraint::Parallel { first, second }
            | SketchConstraint::Perpendicular { first, second }
            | SketchConstraint::EqualLength { first, second } => {
                let (a, b) = (self.lines[first.as_str()], self.lines[second.as_str()]);
                vec![
                    a.start.as_str(),
                    a.end.as_str(),
                    b.start.as_str(),
                    b.end.as_str(),
                ]
            }
        };
        self.point_columns(points)
    }

    fn point_columns(&self, points: Vec<&str>) -> Vec<usize> {
        let mut columns = points
            .into_iter()
            .filter_map(|point| self.variables.get(point))
            .flat_map(|&index| [index, index + 1])
            .collect::<Vec<_>>();
        columns.sort_unstable();
        columns.dedup();
        columns
    }

    /// Differentiate only the points named by each constraint, with one
    /// shared scratch vector. Duplicate endpoints contribute only once.
    fn jacobian(&self, values: &[f64]) -> Result<SparseJacobian, ModelError> {
        let mut rows = Vec::new();
        let mut shifted = values.to_vec();
        for equation in self.equations() {
            let mut base = Vec::new();
            self.equation_residuals(equation, values, &mut base)?;
            let mut term_rows = vec![Vec::new(); base.len()];
            let columns = match equation {
                Equation::Constraint(constraint) => self.columns(constraint),
                Equation::Arc(arc) => self.point_columns(vec![&arc.center, &arc.start, &arc.end]),
            };
            for column in columns {
                let step = DIFFERENCE_STEP * values[column].abs().max(1.0);
                shifted[column] = values[column] + step;
                let mut forward = Vec::new();
                self.equation_residuals(equation, &shifted, &mut forward)?;
                shifted[column] = values[column] - step;
                let mut backward = Vec::new();
                self.equation_residuals(equation, &shifted, &mut backward)?;
                shifted[column] = values[column];
                for (row, (a, b)) in term_rows.iter_mut().zip(forward.iter().zip(backward)) {
                    let derivative = (a - b) / (2.0 * step);
                    if !derivative.is_finite() {
                        return Err(ModelError::new("sketch derivative is not finite"));
                    }
                    if derivative != 0.0 {
                        row.push((column, derivative));
                    }
                }
            }
            rows.extend(term_rows);
        }
        Ok(SparseJacobian {
            rows,
            columns: values.len(),
        })
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

fn least_squares_step(jacobian: &SparseJacobian, residual: &[f64]) -> Option<Vec<f64>> {
    let rhs = jacobian
        .transpose_times(residual)
        .into_iter()
        .map(|value| -value)
        .collect();
    jacobian
        .normal_matrix()
        .solve_dropping_null(rhs, PIVOT_TOLERANCE)
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
            datum_plane: None,
            circles: Vec::new(),
            arcs: Vec::new(),
            profile: Vec::new(),
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
    fn sketch_wires_preserve_closed_line_arc_and_circle_geometry() {
        let session = Session::new().unwrap();
        let mut circle = arc_profile(false);
        circle.lines.clear();
        circle.arcs.clear();
        circle.profile.clear();
        circle.circles.push(SketchCircle {
            id: "circle".into(),
            center: "c".into(),
            rim: "a".into(),
        });
        for (sketch, perimeter) in [
            (rectangle(), 30.0),
            (arc_profile(false), std::f64::consts::PI + 8.0_f64.sqrt()),
            (circle, 4.0 * std::f64::consts::PI),
        ] {
            let wire = sketch.wire(&session, &HashMap::new(), None).unwrap();
            assert_eq!(session.shape_type(&wire).unwrap(), ShapeType::Wire);
            assert!(session.is_valid(&wire).unwrap());
            let mut length = 0.0;
            for index in 0..session.subshape_count(&wire, ShapeType::Edge).unwrap() {
                let edge = session.subshape(&wire, ShapeType::Edge, index).unwrap();
                length += session.edge_length(&edge).unwrap();
            }
            assert!((length - perimeter).abs() < 1e-8);
            let face = session.create_face_from_wire(&wire).unwrap();
            assert!(session.is_valid(&face).unwrap());
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    fn linked_family() -> FamilyDefinition {
        let mut sketch = rectangle();
        sketch.datum_plane = Some("mount".into());
        // Unused inline origin/y expressions must not become dependencies.
        sketch.origin = VectorExpr::Parameter("unused_origin".into());
        sketch.y_axis = VectorExpr::Parameter("unused_y_axis".into());
        FamilyDefinition {
            id: "LinkedSketch".into(),
            version: 1,
            parameters: vec![ParameterDefinition {
                id: "height".into(),
                parameter_type: ParameterType::Scalar(Dimension::Length),
                default: ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
                minimum: None,
                maximum: None,
            }],
            derived_parameters: Vec::new(),
            derived_vector_parameters: Vec::new(),
            constraints: Vec::new(),
            requirements: Vec::new(),
            datums: vec![DatumDefinition {
                id: "mount".into(),
                kind: DatumKind::Plane {
                    origin: VectorExpr::Components {
                        x: length(10.0),
                        y: length(20.0),
                        z: ScalarExpr::Parameter("height".into()),
                    },
                    normal: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                },
            }],
            features: vec![
                FeatureDefinition {
                    id: "wire".into(),
                    operation: FeatureOperation::SketchWire {
                        sketch: Box::new(sketch.clone()),
                    },
                },
                FeatureDefinition {
                    id: "face".into(),
                    operation: FeatureOperation::SketchFace {
                        sketch: Box::new(sketch),
                    },
                },
                FeatureDefinition {
                    id: "placed_wire".into(),
                    operation: FeatureOperation::Translate {
                        input: "wire".into(),
                        offset: VectorExpr::Literal(VectorQuantity::lengths(
                            0.0,
                            0.0,
                            2.0,
                            LengthUnit::Millimeter,
                        )),
                    },
                },
                FeatureDefinition {
                    id: "unrelated".into(),
                    operation: FeatureOperation::Box {
                        origin: VectorExpr::Literal(VectorQuantity::lengths(
                            0.0,
                            0.0,
                            0.0,
                            LengthUnit::Millimeter,
                        )),
                        size: VectorExpr::Literal(VectorQuantity::lengths(
                            1.0,
                            1.0,
                            1.0,
                            LengthUnit::Millimeter,
                        )),
                    },
                },
            ],
        }
    }

    #[test]
    fn linked_sketches_follow_datum_parameters_and_definition_edits_incrementally() {
        let session = Session::new().unwrap();
        let mut family = linked_family();
        fn generate<'session>(
            session: &'session Session,
            definition: &FamilyDefinition,
            height: f64,
            previous: Option<&GeneratedResult<'session>>,
        ) -> GeneratedResult<'session> {
            let part = PartInstance {
                id: "part".into(),
                definition,
                overrides: HashMap::from([(
                    "height".into(),
                    ParameterValue::Scalar(Quantity::length(height, LengthUnit::Millimeter)),
                )]),
                provenance: "test".into(),
            };
            match previous {
                Some(previous) => part.regenerate_incremental(session, previous),
                None => part.regenerate(session),
            }
            .unwrap()
        }
        let original = generate(&session, &family, 3.0, None);
        assert_eq!(
            session.shape_type(original.shape("wire").unwrap()).unwrap(),
            ShapeType::Wire
        );
        assert!(
            (session
                .bounds(original.shape("wire").unwrap())
                .unwrap()
                .min
                .z
                - 3.0)
                .abs()
                < 1e-6
        );
        let unchanged = generate(&session, &family, 3.0, Some(&original));
        assert!(unchanged.regeneration.rebuilt.is_empty());
        assert_eq!(unchanged.regeneration.reused.len(), 4);
        let moved = generate(&session, &family, 8.0, Some(&unchanged));
        assert_eq!(moved.regeneration.rebuilt, ["wire", "face", "placed_wire"]);
        assert_eq!(moved.regeneration.reused, ["unrelated"]);
        assert!(
            (session
                .bounds(moved.shape("placed_wire").unwrap())
                .unwrap()
                .min
                .z
                - 10.0)
                .abs()
                < 1e-6
        );
        assert!((session.surface_area(moved.shape("face").unwrap()).unwrap() - 50.0).abs() < 1e-8);
        if let DatumKind::Plane { normal, .. } = &mut family.datums[0].kind {
            *normal = VectorExpr::Literal(VectorQuantity::scalars(0.0, -1.0, 0.0));
        }
        let rotated = generate(&session, &family, 8.0, Some(&moved));
        assert_eq!(
            rotated.regeneration.rebuilt,
            ["wire", "face", "placed_wire"]
        );
        let bounds = session.bounds(rotated.shape("face").unwrap()).unwrap();
        assert!((bounds.min.y - 20.0).abs() < 1e-6);
        assert!((bounds.max.z - 13.0).abs() < 1e-6);
        drop((original, unchanged, moved, rotated));
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn linked_sketches_reject_missing_nonplane_and_incompatible_datums() {
        let session = Session::new().unwrap();
        for case in 0..4 {
            let mut definition = linked_family();
            match case {
                0 => definition.datums.clear(),
                1 => {
                    definition.datums[0].kind = DatumKind::Point {
                        origin: VectorExpr::Literal(VectorQuantity::lengths(
                            0.0,
                            0.0,
                            0.0,
                            LengthUnit::Millimeter,
                        )),
                    }
                }
                2 => {
                    definition.datums[0].kind = DatumKind::Plane {
                        origin: VectorExpr::Literal(VectorQuantity::lengths(
                            0.0,
                            0.0,
                            0.0,
                            LengthUnit::Millimeter,
                        )),
                        normal: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                    }
                }
                _ => {
                    definition.datums[0].kind = DatumKind::Plane {
                        origin: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0)),
                        normal: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    }
                }
            }
            let error = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "test".into(),
            }
            .regenerate(&session)
            .err()
            .expect("invalid linked sketch must fail");
            assert!(
                error.message.contains(if case == 0 {
                    "unknown sketch plane"
                } else if case == 1 {
                    "must be a plane"
                } else if case == 2 {
                    "x axis must lie"
                } else {
                    "datum"
                }),
                "{error}"
            );
            assert_eq!(session.shape_count().unwrap(), 0);
        }
    }

    #[test]
    fn schema_twenty_seven_preserves_datum_links_and_wire_outputs() {
        let definition = linked_family();
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(loaded, document);
        let session = Session::new().unwrap();
        let generated = loaded
            .instance_graph()
            .unwrap()
            .regenerate_all(&session)
            .unwrap();
        assert!(session.shape_count().unwrap() > 0);
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
        let mut inline = rectangle();
        inline.datum_plane = None;
        let mut legacy = serde_json::to_value(&inline).unwrap();
        legacy.as_object_mut().unwrap().remove("datum_plane");
        assert_eq!(
            serde_json::from_value::<SketchDefinition>(legacy).unwrap(),
            inline
        );
    }

    fn sweep_family(
        sketch: SketchDefinition,
        wire: bool,
        operation: FeatureOperation,
    ) -> FamilyDefinition {
        FamilyDefinition {
            id: "Sweep".into(),
            version: 1,
            parameters: Vec::new(),
            derived_parameters: Vec::new(),
            derived_vector_parameters: Vec::new(),
            constraints: Vec::new(),
            requirements: Vec::new(),
            datums: Vec::new(),
            // Deliberately declared before its profile to exercise dependencies.
            features: vec![
                FeatureDefinition {
                    id: "solid".into(),
                    operation,
                },
                FeatureDefinition {
                    id: "profile".into(),
                    operation: if wire {
                        FeatureOperation::SketchWire {
                            sketch: Box::new(sketch),
                        }
                    } else {
                        FeatureOperation::SketchFace {
                            sketch: Box::new(sketch),
                        }
                    },
                },
            ],
        }
    }

    fn circular_profile() -> SketchDefinition {
        let mut sketch = rectangle();
        sketch.points = vec![
            point("center", 0.0, 0.0, true),
            point("rim", 2.0, 0.0, true),
        ];
        sketch.lines.clear();
        sketch.constraints.clear();
        sketch.circles = vec![SketchCircle {
            id: "circle".into(),
            center: "center".into(),
            rim: "rim".into(),
        }];
        sketch
    }

    #[test]
    fn extrudes_line_arc_and_circle_faces_and_wires_to_exact_solids() {
        let session = Session::new().unwrap();
        for wire in [false, true] {
            for (sketch, area) in [
                (rectangle(), 50.0),
                (arc_profile(false), std::f64::consts::PI - 2.0),
                (circular_profile(), 4.0 * std::f64::consts::PI),
            ] {
                for height in [3.0, -3.0] {
                    let definition = sweep_family(
                        sketch.clone(),
                        wire,
                        FeatureOperation::Extrude {
                            input: "profile".into(),
                            direction: VectorExpr::Literal(VectorQuantity::lengths(
                                1.0,
                                0.0,
                                height,
                                LengthUnit::Millimeter,
                            )),
                        },
                    );
                    let generated = PartInstance {
                        id: "part".into(),
                        definition: &definition,
                        overrides: HashMap::new(),
                        provenance: "test".into(),
                    }
                    .regenerate(&session)
                    .unwrap();
                    let solid = generated.shape("solid").unwrap();
                    assert_eq!(session.shape_type(solid).unwrap(), ShapeType::Solid);
                    assert!(session.is_valid(solid).unwrap());
                    assert!((session.volume(solid).unwrap() - area * height.abs()).abs() < 1e-7);
                    let edge = session
                        .subshape(generated.shape("profile").unwrap(), ShapeType::Edge, 0)
                        .unwrap();
                    assert!(
                        session
                            .history_count(solid, &edge, occt_bridge::HistoryRelation::Generated)
                            .unwrap()
                            > 0
                    );
                }
            }
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn revolves_offset_circle_profiles_into_full_and_signed_partial_tori() {
        let session = Session::new().unwrap();
        let mut sketch = circular_profile();
        sketch.points = vec![
            point("center", 3.0, 0.0, true),
            point("rim", 4.0, 0.0, true),
        ];
        sketch.origin = VectorExpr::Literal(VectorQuantity::lengths(
            10.0,
            20.0,
            30.0,
            LengthUnit::Millimeter,
        ));
        sketch.y_axis = VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0));
        for wire in [false, true] {
            for angle in [
                std::f64::consts::TAU,
                std::f64::consts::PI,
                -std::f64::consts::PI / 2.0,
            ] {
                let definition = sweep_family(
                    sketch.clone(),
                    wire,
                    FeatureOperation::Revolve {
                        input: "profile".into(),
                        origin: sketch.origin.clone(),
                        axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 2.0)),
                        angle_radians: ScalarExpr::Literal(Quantity::scalar(angle)),
                    },
                );
                let generated = PartInstance {
                    id: "part".into(),
                    definition: &definition,
                    overrides: HashMap::new(),
                    provenance: "test".into(),
                }
                .regenerate(&session)
                .unwrap();
                let solid = generated.shape("solid").unwrap();
                assert!(session.is_valid(solid).unwrap());
                assert!(
                    (session.volume(solid).unwrap() - 3.0 * std::f64::consts::PI * angle.abs())
                        .abs()
                        < 1e-7
                );
                let edge = session
                    .subshape(generated.shape("profile").unwrap(), ShapeType::Edge, 0)
                    .unwrap();
                assert!(
                    session
                        .history_count(solid, &edge, occt_bridge::HistoryRelation::Generated)
                        .unwrap()
                        > 0
                );
            }
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn sweep_parameters_rebuild_solids_and_reuse_unchanged_profiles() {
        let session = Session::new().unwrap();
        for revolve in [false, true] {
            let mut sketch = circular_profile();
            let operation = if revolve {
                sketch.points[0].x = length(3.0);
                sketch.points[1].x = length(4.0);
                sketch.y_axis = VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0));
                FeatureOperation::Revolve {
                    input: "profile".into(),
                    origin: sketch.origin.clone(),
                    axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    angle_radians: ScalarExpr::Parameter("amount".into()),
                }
            } else {
                FeatureOperation::Extrude {
                    input: "profile".into(),
                    direction: VectorExpr::Components {
                        x: length(0.0),
                        y: length(0.0),
                        z: ScalarExpr::Parameter("amount".into()),
                    },
                }
            };
            let mut definition = sweep_family(sketch, true, operation);
            let value = |amount| {
                if revolve {
                    Quantity::scalar(amount)
                } else {
                    Quantity::length(amount, LengthUnit::Millimeter)
                }
            };
            definition.parameters.push(ParameterDefinition {
                id: "amount".into(),
                parameter_type: ParameterType::Scalar(if revolve {
                    Dimension::Scalar
                } else {
                    Dimension::Length
                }),
                default: ParameterValue::Scalar(value(1.0)),
                minimum: None,
                maximum: None,
            });
            let part = |amount| PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::from([(
                    "amount".into(),
                    ParameterValue::Scalar(value(amount)),
                )]),
                provenance: "test".into(),
            };
            let first = part(1.0).regenerate(&session).unwrap();
            let second = part(2.0).regenerate_incremental(&session, &first).unwrap();
            assert_eq!(second.regeneration.rebuilt, ["solid"]);
            assert_eq!(second.regeneration.reused, ["profile"]);
            assert!(
                (session.volume(second.shape("solid").unwrap()).unwrap()
                    - 2.0 * session.volume(first.shape("solid").unwrap()).unwrap())
                .abs()
                    < 1e-7
            );
            let count = session.shape_count().unwrap();
            assert!(part(0.0).regenerate_incremental(&session, &second).is_err());
            assert_eq!(session.shape_count().unwrap(), count);
            assert!(session.is_valid(second.shape("solid").unwrap()).unwrap());
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn invalid_sweep_values_and_inputs_fail_without_leaking_profiles() {
        let session = Session::new().unwrap();
        for operation in [
            FeatureOperation::Extrude {
                input: "profile".into(),
                direction: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
            FeatureOperation::Extrude {
                input: "profile".into(),
                direction: VectorExpr::Literal(VectorQuantity::lengths(
                    1.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
            FeatureOperation::Extrude {
                input: "profile".into(),
                direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            },
            FeatureOperation::Revolve {
                input: "profile".into(),
                origin: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0)),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(1.0)),
            },
            FeatureOperation::Revolve {
                input: "profile".into(),
                origin: rectangle().origin,
                axis: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    1.0,
                    LengthUnit::Millimeter,
                )),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(1.0)),
            },
            FeatureOperation::Revolve {
                input: "profile".into(),
                origin: rectangle().origin,
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                angle_radians: length(1.0),
            },
            FeatureOperation::Revolve {
                input: "missing".into(),
                origin: rectangle().origin,
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(1.0)),
            },
        ] {
            let definition = sweep_family(rectangle(), true, operation);
            assert!(
                PartInstance {
                    id: "part".into(),
                    definition: &definition,
                    overrides: HashMap::new(),
                    provenance: "test".into()
                }
                .regenerate(&session)
                .is_err()
            );
            assert_eq!(session.shape_count().unwrap(), 0);
        }
        let mut definition = sweep_family(
            rectangle(),
            true,
            FeatureOperation::Extrude {
                input: "profile".into(),
                direction: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    1.0,
                    LengthUnit::Millimeter,
                )),
            },
        );
        definition.features[1].operation = FeatureOperation::Box {
            origin: rectangle().origin,
            size: VectorExpr::Literal(VectorQuantity::lengths(
                1.0,
                1.0,
                1.0,
                LengthUnit::Millimeter,
            )),
        };
        let error = PartInstance {
            id: "part".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        }
        .regenerate(&session)
        .err()
        .unwrap();
        assert!(error.message.contains("profile 'profile'"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn schema_twenty_eight_round_trips_sweeps_and_migrates_sketch_documents() {
        for operation in [
            FeatureOperation::Extrude {
                input: "profile".into(),
                direction: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    2.0,
                    LengthUnit::Millimeter,
                )),
            },
            FeatureOperation::Revolve {
                input: "profile".into(),
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    -3.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::TAU)),
            },
        ] {
            let definition = sweep_family(circular_profile(), true, operation);
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "test").unwrap();
            let document = ModelDocument::from_graph(&graph);
            let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
            assert_eq!(loaded.schema_version, 28);
            assert_eq!(loaded, document);
            let session = Session::new().unwrap();
            let generated = loaded
                .instance_graph()
                .unwrap()
                .regenerate_all(&session)
                .unwrap();
            drop(generated);
            assert_eq!(session.shape_count().unwrap(), 0);
            let mut old = serde_json::to_value(&document).unwrap();
            old["schema_version"] = serde_json::json!(27);
            old["family"]["features"].as_array_mut().unwrap().remove(0);
            let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
            assert_eq!(migrated.schema_version, 28);
            assert_eq!(migrated.family.features.len(), 1);
        }
    }

    #[test]
    fn circle_radius_parameters_drive_exact_faces_in_a_rotated_plane() {
        let mut sketch = rectangle();
        sketch.lines.clear();
        sketch.points = vec![
            point("center", 0.0, 0.0, true),
            point("rim", 3.0, 0.0, false),
        ];
        sketch.circles = vec![SketchCircle {
            id: "circle".into(),
            center: "center".into(),
            rim: "rim".into(),
        }];
        sketch.constraints = vec![SketchConstraint::Distance {
            first: "center".into(),
            second: "rim".into(),
            value: ScalarExpr::Parameter("radius".into()),
        }];
        sketch.origin = VectorExpr::Literal(VectorQuantity::lengths(
            10.0,
            20.0,
            30.0,
            LengthUnit::Millimeter,
        ));
        sketch.x_axis = VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0));
        sketch.y_axis = VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0));
        let mut names = HashSet::new();
        sketch.collect_parameters(&mut names);
        assert!(names.contains("radius"));
        let session = Session::new().unwrap();
        for radius in [2.0, 5.0] {
            let parameters = HashMap::from([(
                "radius".into(),
                ParameterValue::Scalar(Quantity::length(radius, LengthUnit::Millimeter)),
            )]);
            let solution = sketch.solve(&parameters).unwrap();
            assert!(solution.solved);
            assert_eq!(solution.free_degrees, 1); // the rim can rotate
            let face = sketch.face(&session, &parameters).unwrap();
            assert!(
                (session.surface_area(&face).unwrap() - std::f64::consts::PI * radius * radius)
                    .abs()
                    < 1e-7
            );
            let bounds = session.bounds(&face).unwrap();
            assert!((bounds.min.x - 10.0).abs() < 1e-6);
            assert!((bounds.max.z - (30.0 + radius)).abs() < 1e-6);
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    fn arc_profile(clockwise: bool) -> SketchDefinition {
        let mut sketch = rectangle();
        sketch.points = vec![
            point("c", 0.0, 0.0, true),
            point("a", 2.0, 0.0, true),
            point("b", 0.0, 2.0, true),
        ];
        sketch.lines = vec![SketchLine {
            id: "chord".into(),
            start: "b".into(),
            end: "a".into(),
        }];
        sketch.arcs = vec![SketchArc {
            id: "arc".into(),
            center: "c".into(),
            start: "a".into(),
            end: "b".into(),
            clockwise,
        }];
        sketch.constraints.clear();
        sketch.profile = vec!["arc".into(), "chord".into()];
        sketch
    }

    #[test]
    fn mixed_profiles_keep_minor_and_major_arcs_exact() {
        let session = Session::new().unwrap();
        for clockwise in [false, true] {
            let mut sketch = arc_profile(clockwise);
            // Explicit profile excludes construction geometry.
            sketch.lines.push(SketchLine {
                id: "construction".into(),
                start: "c".into(),
                end: "a".into(),
            });
            let face = sketch.face(&session, &HashMap::new()).unwrap();
            let expected = if clockwise {
                3.0 * std::f64::consts::PI + 2.0
            } else {
                std::f64::consts::PI - 2.0
            };
            assert!((session.surface_area(&face).unwrap() - expected).abs() < 1e-8);
            assert!(session.is_valid(&face).unwrap());
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn implicit_arc_radius_equation_solves_and_reports_conflicts() {
        let mut sketch = arc_profile(false);
        sketch.points[2] = point("b", 0.0, 3.0, false);
        let solved = sketch.solve(&HashMap::new()).unwrap();
        assert!(solved.solved);
        assert_eq!(solved.free_degrees, 1);
        assert!((solved.points["b"].y - 2.0).abs() < 1e-8);
        sketch.points[2].fixed = true;
        let conflict = sketch.solve(&HashMap::new()).unwrap();
        assert!(!conflict.solved);
        assert!((conflict.max_residual - 1.0).abs() < 1e-9);
        let session = Session::new().unwrap();
        assert!(sketch.face(&session, &HashMap::new()).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn solves_line_arc_and_line_circle_tangency_at_contact() {
        for use_circle in [false, true] {
            let mut sketch = arc_profile(false);
            sketch.points.push(point("tip", 2.5, 3.0, false));
            sketch.lines = vec![SketchLine {
                id: "tangent".into(),
                start: "a".into(),
                end: "tip".into(),
            }];
            sketch.profile.clear();
            if use_circle {
                sketch.arcs.clear();
                sketch.circles.push(SketchCircle {
                    id: "curve".into(),
                    center: "c".into(),
                    rim: "a".into(),
                });
            } else {
                sketch.arcs[0].id = "curve".into();
            }
            sketch.constraints = vec![
                SketchConstraint::Tangent {
                    first: "curve".into(),
                    second: "tangent".into(),
                    point: "a".into(),
                },
                SketchConstraint::Distance {
                    first: "a".into(),
                    second: "tip".into(),
                    value: length(3.0),
                },
            ];
            let solved = sketch.solve(&HashMap::new()).unwrap();
            assert!(solved.solved, "{solved:?}");
            assert_eq!(solved.free_degrees, 0);
            assert!((solved.points["tip"].x - 2.0).abs() < 1e-8);
            assert!((solved.points["tip"].y - 3.0).abs() < 1e-8);
            sketch.points.last_mut().unwrap().fixed = true;
            assert!(!sketch.solve(&HashMap::new()).unwrap().solved);
        }
    }

    #[test]
    fn solves_circle_circle_and_arc_arc_tangency() {
        for circles in [false, true] {
            let mut sketch = arc_profile(false);
            sketch.lines.clear();
            sketch.profile.clear();
            sketch
                .points
                .extend([point("c2", 4.0, 0.3, false), point("b2", 4.0, 2.0, false)]);
            if circles {
                sketch.arcs.clear();
                sketch.circles = vec![
                    SketchCircle {
                        id: "first".into(),
                        center: "c".into(),
                        rim: "a".into(),
                    },
                    SketchCircle {
                        id: "second".into(),
                        center: "c2".into(),
                        rim: "a".into(),
                    },
                ];
            } else {
                sketch.arcs[0].id = "first".into();
                sketch.arcs.push(SketchArc {
                    id: "second".into(),
                    center: "c2".into(),
                    start: "a".into(),
                    end: "b2".into(),
                    clockwise: true,
                });
            }
            sketch.constraints = vec![
                SketchConstraint::Tangent {
                    first: "first".into(),
                    second: "second".into(),
                    point: "a".into(),
                },
                SketchConstraint::Distance {
                    first: "c2".into(),
                    second: "a".into(),
                    value: length(2.0),
                },
            ];
            let solved = sketch.solve(&HashMap::new()).unwrap();
            assert!(solved.solved, "{solved:?}");
            assert!((solved.points["c2"].x - 4.0).abs() < 1e-8);
            assert!(solved.points["c2"].y.abs() < 1e-8);
        }
    }

    #[test]
    fn invalid_curves_contacts_and_profiles_are_rejected_without_handles() {
        let session = Session::new().unwrap();
        let base = arc_profile(false);
        let mut cases = Vec::new();
        let mut crossing = rectangle();
        crossing.constraints.clear();
        crossing.points = vec![
            point("p0", 0.0, 0.0, true),
            point("p1", 2.0, 2.0, true),
            point("p2", 0.0, 2.0, true),
            point("p3", 2.0, 0.0, true),
        ];
        cases.push(crossing);
        let mut circle = base.clone();
        circle.circles.push(SketchCircle {
            id: "circle".into(),
            center: "c".into(),
            rim: "a".into(),
        });
        circle.profile = vec!["circle".into(), "arc".into()];
        cases.push(circle.clone());
        circle.profile = vec!["circle".into()];
        circle.arcs.clear();
        circle.points[1] = point("a", 0.0, 0.0, true);
        cases.push(circle);
        let mut sketch = base.clone();
        sketch.arcs[0].center = "missing".into();
        cases.push(sketch);
        let mut sketch = base.clone();
        sketch.arcs[0].end = "a".into();
        cases.push(sketch);
        let mut sketch = base.clone();
        sketch.arcs[0].id = "chord".into();
        cases.push(sketch);
        let mut sketch = base.clone();
        sketch.points[0] = point("c", 2.0, 0.0, true);
        cases.push(sketch);
        for profile in [vec![], vec!["missing"], vec!["arc", "arc"], vec!["arc"]] {
            let mut sketch = base.clone();
            sketch.profile = profile.into_iter().map(str::to_owned).collect();
            cases.push(sketch);
        }
        for (first, second, point) in [
            ("arc", "chord", "c"),
            ("arc", "arc", "a"),
            ("arc", "missing", "a"),
            ("arc", "chord", "missing"),
        ] {
            let mut sketch = base.clone();
            sketch.constraints.push(SketchConstraint::Tangent {
                first: first.into(),
                second: second.into(),
                point: point.into(),
            });
            cases.push(sketch);
        }
        for sketch in cases {
            assert!(
                sketch.face(&session, &HashMap::new()).is_err(),
                "{sketch:?}"
            );
            assert_eq!(session.shape_count().unwrap(), 0);
        }
    }

    #[test]
    fn schema_twenty_six_round_trips_curves_and_migrates_line_sketches() {
        let mut curved = arc_profile(true);
        curved.circles.push(SketchCircle {
            id: "circle".into(),
            center: "c".into(),
            rim: "a".into(),
        });
        curved.constraints.push(SketchConstraint::Tangent {
            first: "arc".into(),
            second: "circle".into(),
            point: "a".into(),
        });
        let family = |sketch| FamilyDefinition {
            id: "SketchPart".into(),
            version: 1,
            parameters: Vec::new(),
            derived_parameters: Vec::new(),
            derived_vector_parameters: Vec::new(),
            constraints: Vec::new(),
            features: vec![FeatureDefinition {
                id: "profile".into(),
                operation: FeatureOperation::SketchFace {
                    sketch: Box::new(sketch),
                },
            }],
            requirements: Vec::new(),
            datums: Vec::new(),
        };
        for sketch in [rectangle(), curved] {
            let definition = family(sketch.clone());
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "test").unwrap();
            let document = ModelDocument::from_graph(&graph);
            let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
            assert_eq!(loaded, document);
            let session = Session::new().unwrap();
            let generated = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "test".into(),
            }
            .regenerate(&session)
            .unwrap();
            assert!(
                session
                    .is_valid(generated.shape("profile").unwrap())
                    .unwrap()
            );
            if sketch.arcs.is_empty() {
                let mut old = serde_json::to_value(&document).unwrap();
                old["schema_version"] = serde_json::json!(25);
                fn remove_new_fields(value: &mut serde_json::Value) {
                    match value {
                        serde_json::Value::Object(fields) => {
                            if fields.contains_key("points") && fields.contains_key("lines") {
                                for field in ["circles", "arcs", "profile"] {
                                    fields.remove(field);
                                }
                            }
                            for value in fields.values_mut() {
                                remove_new_fields(value);
                            }
                        }
                        serde_json::Value::Array(values) => {
                            for value in values {
                                remove_new_fields(value);
                            }
                        }
                        _ => {}
                    }
                }
                remove_new_fields(&mut old);
                assert_eq!(
                    ModelDocument::from_json(&old.to_string()).unwrap(),
                    document
                );
            }
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
                operation: FeatureOperation::SketchFace {
                    sketch: Box::new(sketch),
                },
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

    #[test]
    fn sparse_solver_keeps_disconnected_and_unconstrained_points_independent() {
        let mut sketch = rectangle();
        sketch.points = vec![
            point("a", 0.0, 0.0, true),
            point("b", 9.0, 2.0, false),
            point("c", 100.0, 50.0, true),
            point("d", 110.0, 53.0, false),
            point("unused", 7.0, 8.0, false),
        ];
        sketch.lines = vec![
            SketchLine {
                id: "ab".into(),
                start: "a".into(),
                end: "b".into(),
            },
            SketchLine {
                id: "cd".into(),
                start: "c".into(),
                end: "d".into(),
            },
        ];
        sketch.constraints = vec![
            SketchConstraint::Horizontal { line: "ab".into() },
            SketchConstraint::Horizontal { line: "cd".into() },
            SketchConstraint::Distance {
                first: "a".into(),
                second: "b".into(),
                value: length(10.0),
            },
            SketchConstraint::Parallel {
                first: "ab".into(),
                second: "ab".into(),
            },
        ];
        let solution = sketch.solve(&HashMap::new()).unwrap();
        assert!(solution.solved, "{solution:?}");
        assert_eq!(solution.free_degrees, 3);
        assert_eq!(solution.redundant_equations, 1);
        assert_eq!(solution.points["unused"], SketchPoint2 { x: 7.0, y: 8.0 });
        assert_eq!(solution.points["d"].x, 110.0);
        assert!((solution.points["d"].y - 50.0).abs() < 1e-9);
        assert!((solution.points["b"].x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn nonfinite_residuals_are_errors_instead_of_successful_solves() {
        let mut sketch = rectangle();
        sketch.points = vec![point("a", -1e308, 0.0, true), point("b", 1e308, 0.0, true)];
        sketch.lines.clear();
        sketch.constraints = vec![SketchConstraint::Coincident {
            first: "a".into(),
            second: "b".into(),
        }];
        assert!(
            sketch
                .solve(&HashMap::new())
                .unwrap_err()
                .message
                .contains("not finite")
        );
    }
}
