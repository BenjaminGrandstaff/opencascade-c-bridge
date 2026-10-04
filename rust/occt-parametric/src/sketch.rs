//! Constraint-solved two-dimensional sketches with exact circular geometry.

use super::*;
use crate::assembly::{add, cross, dot, scale, unit};
use crate::sparse::SparseJacobian;
use occt_bridge::CurveSegment;
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

/// A smooth curve interpolated through `points` in order. Its ends are the
/// first and last points. Repeating the first point at the end makes a smooth
/// closed loop with no corner, usable alone as a profile. A `Tangent`
/// constraint at an end with a line or arc sets the spline's end direction so
/// it continues smoothly from that entity; spline-to-spline tangency is not
/// supported. Interior points move with the solver like any other point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchSpline {
    pub id: String,
    pub points: Vec<String>,
}

impl SketchSpline {
    fn closed(&self) -> bool {
        self.points.len() > 1 && self.points.first() == self.points.last()
    }
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub splines: Vec<SketchSpline>,
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
    Spline(&'a SketchSpline),
}

impl<'a> Entity<'a> {
    fn endpoints(self) -> (&'a str, &'a str) {
        match self {
            Self::Line(line) => (&line.start, &line.end),
            Self::Circle(circle) => (&circle.rim, &circle.rim),
            Self::Arc(arc) => (&arc.start, &arc.end),
            Self::Spline(spline) => (&spline.points[0], &spline.points[spline.points.len() - 1]),
        }
    }

    fn tangent_points(self, contact: &'a str) -> (&'a str, &'a str) {
        match self {
            Self::Line(line) => (&line.start, &line.end),
            Self::Circle(circle) => (&circle.center, contact),
            Self::Arc(arc) => (&arc.center, contact),
            // Spline tangency shapes the spline; it is never a solver equation.
            Self::Spline(_) => unreachable!("spline tangency is applied geometrically"),
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
            .chain(
                self.splines
                    .iter()
                    .map(|spline| (spline.id.as_str(), Entity::Spline(spline))),
            )
            .collect()
    }

    /// True when a tangency involves a spline and so shapes it instead of
    /// adding a solver equation.
    fn spline_tangency(&self, constraint: &SketchConstraint) -> bool {
        matches!(constraint, SketchConstraint::Tangent { first, second, .. }
            if self.splines.iter().any(|spline| spline.id == *first || spline.id == *second))
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
        for spline in &self.splines {
            for pair in spline.points.windows(2) {
                let distance = line_length((points[&pair[0]], points[&pair[1]]));
                if !distance.is_finite() || distance <= RESIDUAL_TOLERANCE {
                    return Err(ModelError::new(format!(
                        "sketch spline '{}' has coincident consecutive points",
                        spline.id
                    )));
                }
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
        self.profile_wire(session, parameters, datum, true)
    }

    pub(crate) fn open_wire<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
        datum: Option<ResolvedDatum>,
    ) -> Result<Shape<'session>, ModelError> {
        self.profile_wire(session, parameters, datum, false)
    }

    fn profile_wire<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
        datum: Option<ResolvedDatum>,
        closed: bool,
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
        let profile = self.profile_entities()?;
        let wire = if let [Entity::Circle(circle)] = profile.as_slice() {
            if !closed {
                return Err(ModelError::new("open sketch profile cannot be a circle"));
            }
            let center = solution.points[&circle.center];
            let radius = line_length((center, solution.points[&circle.rim]));
            session.create_circle_wire(transform(center), cross(x_axis, y_axis), radius)?
        } else {
            let direction = |d: SketchPoint2| add(scale(x_axis, d.x), scale(y_axis, d.y));
            let segments =
                self.profile_segments(&profile, &solution, &transform, &direction, closed)?;
            session.create_curve_wire(&segments, closed)?
        };
        if !session.is_valid(&wire)? {
            return Err(ModelError::new("sketch profile produced an invalid wire"));
        }
        Ok(wire)
    }

    /// Wire segments for the profile in order. Spline ends tangent to a line
    /// or arc continue that entity's direction away from the shared point.
    fn profile_segments(
        &self,
        profile: &[Entity<'_>],
        solution: &SketchSolution,
        transform: &impl Fn(SketchPoint2) -> Vec3,
        direction: &impl Fn(SketchPoint2) -> Vec3,
        closed: bool,
    ) -> Result<Vec<CurveSegment>, ModelError> {
        validate_curve_entities(profile)?;
        if !closed {
            validate_open_endpoints(profile, solution)?;
        }
        let mut segments = Vec::with_capacity(profile.len());
        for (index, entity) in profile.iter().enumerate() {
            let (start, end) = entity.endpoints();
            if (closed || index + 1 < profile.len())
                && end != profile[(index + 1) % profile.len()].endpoints().0
            {
                return Err(ModelError::new(
                    "sketch profile is not a continuous boundary",
                ));
            }
            let start_point = solution.points[start];
            let end_point = solution.points[end];
            let segment = match entity {
                Entity::Line(_) => CurveSegment::Line {
                    start: transform(start_point),
                    end: transform(end_point),
                },
                Entity::Arc(arc) => {
                    let middle = arc_middle(arc, solution);
                    CurveSegment::Arc {
                        start: transform(start_point),
                        middle: transform(middle),
                        end: transform(end_point),
                    }
                }
                Entity::Spline(spline) => {
                    let periodic = spline.closed();
                    let through = if periodic {
                        &spline.points[..spline.points.len() - 1]
                    } else {
                        &spline.points[..]
                    };
                    let away = |point: &str| self.tangent_away(&spline.id, point, solution);
                    CurveSegment::Spline {
                        points: through
                            .iter()
                            .map(|id| transform(solution.points[id]))
                            .collect(),
                        // Leave the start opposite to where the neighbor
                        // extends; arrive at the end heading into it.
                        start_tangent: away(start)?
                            .map(|d| direction(SketchPoint2 { x: -d.x, y: -d.y })),
                        end_tangent: away(end)?.map(direction),
                        periodic,
                    }
                }
                Entity::Circle(_) => unreachable!("circles handled above"),
            };
            segments.push(segment);
        }
        Ok(segments)
    }

    fn tangent_neighbor<'a>(
        &'a self,
        spline: &str,
        point: &str,
    ) -> Result<Option<&'a str>, ModelError> {
        let neighbors = self
            .constraints
            .iter()
            .filter_map(|constraint| match constraint {
                SketchConstraint::Tangent {
                    first,
                    second,
                    point: at,
                } if at == point && (first == spline || second == spline) => {
                    Some(if first == spline {
                        second.as_str()
                    } else {
                        first.as_str()
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let Some(neighbor) = neighbors.first() else {
            return Ok(None);
        };
        if neighbors.len() > 1 {
            return Err(ModelError::new(format!(
                "spline '{spline}' has more than one tangency at '{point}'"
            )));
        }
        Ok(Some(*neighbor))
    }

    /// The direction in which the line or arc tangent to `spline` at `point`
    /// extends away from that point, if such a tangency exists.
    fn tangent_away(
        &self,
        spline: &str,
        point: &str,
        solution: &SketchSolution,
    ) -> Result<Option<SketchPoint2>, ModelError> {
        let Some(neighbor) = self.tangent_neighbor(spline, point)? else {
            return Ok(None);
        };
        let entities = self.entities();
        let at = solution.points[point];
        let away = match entities[neighbor] {
            Entity::Line(line) => {
                let other = if line.start == point {
                    &line.end
                } else {
                    &line.start
                };
                let other = solution.points[other];
                SketchPoint2 {
                    x: other.x - at.x,
                    y: other.y - at.y,
                }
            }
            Entity::Arc(arc) => {
                let center = solution.points[&arc.center];
                let (rx, ry) = (at.x - center.x, at.y - center.y);
                // Travel direction at `point`; the arc extends forward from
                // its start and backward from its end.
                let sense = if arc.clockwise { -1.0 } else { 1.0 };
                let forward = if arc.start == point { 1.0 } else { -1.0 };
                SketchPoint2 {
                    x: -ry * sense * forward,
                    y: rx * sense * forward,
                }
            }
            _ => {
                return Err(ModelError::new(
                    "a spline can be tangent only to a line or an arc",
                ));
            }
        };
        Ok(Some(away))
    }

    fn profile_entities(&self) -> Result<Vec<Entity<'_>>, ModelError> {
        let entities = self.entities();
        let only_lines = self.arcs.is_empty() && self.splines.is_empty();
        let profile = if !self.profile.is_empty() {
            self.profile
                .iter()
                .map(|id| entities[id.as_str()])
                .collect::<Vec<_>>()
        } else if self.circles.len() == 1 && self.lines.is_empty() && only_lines {
            vec![Entity::Circle(&self.circles[0])]
        } else if self.splines.len() == 1
            && self.lines.is_empty()
            && self.arcs.is_empty()
            && self.circles.is_empty()
        {
            vec![Entity::Spline(&self.splines[0])]
        } else if only_lines && self.circles.is_empty() {
            self.lines.iter().map(Entity::Line).collect()
        } else {
            return Err(ModelError::new(
                "mixed sketch geometry requires an explicit profile",
            ));
        };
        Ok(profile)
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
        let point_ids = self.validate_points()?;
        let line_ids = self.validate_lines(&point_ids)?;
        let entity_ids = self.validate_curves(&point_ids, &line_ids)?;
        self.validate_profile(&entity_ids)?;
        let entities = self.entities();
        for constraint in &self.constraints {
            constraint.validate_references(&point_ids, &line_ids)?;
            validate_tangency(constraint, &entities)?;
        }
        Ok((point_ids, line_ids))
    }
    fn validate_points(&self) -> Result<HashSet<&str>, ModelError> {
        let mut point_ids = HashSet::new();
        for point in &self.points {
            if point.id.is_empty() || !point_ids.insert(point.id.as_str()) {
                return Err(ModelError::new(
                    "sketch point ids must be nonempty and unique",
                ));
            }
        }
        Ok(point_ids)
    }
    fn validate_lines<'a>(
        &'a self,
        point_ids: &HashSet<&str>,
    ) -> Result<HashSet<&'a str>, ModelError> {
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
        Ok(line_ids)
    }
    fn validate_curves<'a>(
        &'a self,
        point_ids: &HashSet<&str>,
        line_ids: &HashSet<&'a str>,
    ) -> Result<HashSet<&'a str>, ModelError> {
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
        for spline in &self.splines {
            if spline.id.is_empty() || !entity_ids.insert(spline.id.as_str()) {
                return Err(ModelError::new(
                    "sketch entity ids must be nonempty and unique",
                ));
            }
            validate_spline_points(spline, point_ids)?;
        }
        Ok(entity_ids)
    }
    fn validate_profile(&self, entity_ids: &HashSet<&str>) -> Result<(), ModelError> {
        let mut profile_ids = HashSet::new();
        for id in &self.profile {
            if !entity_ids.contains(id.as_str()) || !profile_ids.insert(id.as_str()) {
                return Err(ModelError::new(
                    "sketch profile has unknown or repeated entities",
                ));
            }
        }
        Ok(())
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
            .filter(|constraint| !self.sketch.spline_tangency(constraint))
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
                    push_derivative(row, column, (a - b) / (2.0 * step))?;
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

fn arc_middle(arc: &SketchArc, solution: &SketchSolution) -> SketchPoint2 {
    let center = solution.points[&arc.center];
    let (start, end) = (solution.points[&arc.start], solution.points[&arc.end]);
    let angle = (start.y - center.y).atan2(start.x - center.x);
    let end_angle = (end.y - center.y).atan2(end.x - center.x);
    let sweep = if arc.clockwise {
        -((angle - end_angle).rem_euclid(std::f64::consts::TAU))
    } else {
        (end_angle - angle).rem_euclid(std::f64::consts::TAU)
    };
    let radius = line_length((center, start));
    let middle_angle = angle + sweep / 2.0;
    SketchPoint2 {
        x: center.x + radius * middle_angle.cos(),
        y: center.y + radius * middle_angle.sin(),
    }
}

fn validate_tangency(
    constraint: &SketchConstraint,
    entities: &HashMap<&str, Entity<'_>>,
) -> Result<(), ModelError> {
    if let SketchConstraint::Tangent {
        first,
        second,
        point,
    } = constraint
    {
        if first == second {
            return Err(ModelError::new("tangency requires two distinct entities"));
        }
        let kinds = [first, second].map(|id| entities.get(id.as_str()).copied());
        validate_spline_tangency(kinds)?;
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
    Ok(())
}

fn push_derivative(
    row: &mut Vec<(usize, f64)>,
    column: usize,
    derivative: f64,
) -> Result<(), ModelError> {
    if !derivative.is_finite() {
        return Err(ModelError::new("sketch derivative is not finite"));
    }
    if derivative != 0.0 {
        row.push((column, derivative));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

fn validate_open_endpoints(
    profile: &[Entity<'_>],
    solution: &SketchSolution,
) -> Result<(), ModelError> {
    let start = solution.points[profile[0].endpoints().0];
    let end = solution.points[profile[profile.len() - 1].endpoints().1];
    if line_length((start, end)) <= 1e-7 {
        return Err(ModelError::new(
            "open sketch profile must have distinct endpoints",
        ));
    }
    Ok(())
}

fn validate_curve_entities(profile: &[Entity<'_>]) -> Result<(), ModelError> {
    if profile.is_empty()
        || profile
            .iter()
            .any(|entity| matches!(entity, Entity::Circle(_)))
    {
        return Err(ModelError::new(
            "profile must contain connected lines, arcs, and splines, or one circle",
        ));
    }
    if profile.len() > 1
        && profile
            .iter()
            .any(|entity| matches!(entity, Entity::Spline(s) if s.closed()))
    {
        return Err(ModelError::new(
            "a closed spline must be the only entity in its profile",
        ));
    }
    Ok(())
}

fn validate_spline_points(
    spline: &SketchSpline,
    point_ids: &HashSet<&str>,
) -> Result<(), ModelError> {
    // A closed spline repeats only its first point, at the end.
    let open = if spline.closed() {
        &spline.points[..spline.points.len() - 1]
    } else {
        &spline.points[..]
    };
    let unique = open.iter().map(String::as_str).collect::<HashSet<_>>();
    let minimum = if spline.closed() { 3 } else { 2 };
    if open.len() < minimum
        || unique.len() != open.len()
        || open.iter().any(|id| !point_ids.contains(id.as_str()))
    {
        return Err(ModelError::new(format!(
            "sketch spline '{}' needs {minimum} or more distinct known points",
            spline.id
        )));
    }
    Ok(())
}

fn validate_spline_tangency(kinds: [Option<Entity<'_>>; 2]) -> Result<(), ModelError> {
    let spline = |entity: Option<Entity<'_>>| matches!(entity, Some(Entity::Spline(_)));
    if kinds.iter().any(|entity| spline(*entity)) {
        let other = if spline(kinds[0]) { kinds[1] } else { kinds[0] };
        if !matches!(other, Some(Entity::Line(_) | Entity::Arc(_))) {
            return Err(ModelError::new(
                "a spline can be tangent only to a line or an arc",
            ));
        }
        if kinds
            .iter()
            .any(|entity| matches!(entity, Some(Entity::Spline(s)) if s.closed()))
        {
            return Err(ModelError::new(
                "a closed spline has no free ends for tangency",
            ));
        }
    }
    Ok(())
}
