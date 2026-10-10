//! Newton least-squares solve of sketch constraints with a sparse Jacobian.

use super::*;

impl SketchDefinition {
    /// Solves using the assembly solver's sparse normal-matrix algebra.
    /// Analytic constraints touch at most eight coordinates. Point-on-spline
    /// also touches its interpolation and tangent-neighbor points: native
    /// projection/interpolation and local finite differences scale with that
    /// curve, not the full sketch. Normal assembly is O(sum(row widths²));
    /// elimination depends on fill-in, small for independent components.
    /// Source constraints are solved before saved profile edits are applied.
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
        let problem = SketchProblem::new(self, parameters, variables, fixed)?;
        let mut residual = problem.residuals(&values)?;
        let mut iterations = 0;
        while iterations < MAX_ITERATIONS && max_abs(&residual) > RESIDUAL_TOLERANCE {
            iterations += 1;
            let jacobian = problem.jacobian(&values)?;
            let Some(step) = least_squares_step(
                &jacobian,
                &residual,
                self.constraints.iter().any(|c| {
                    matches!(
                        c,
                        SketchConstraint::Angle { .. }
                            | SketchConstraint::Radius { .. }
                            | SketchConstraint::Diameter { .. }
                            | SketchConstraint::EqualRadius { .. }
                            | SketchConstraint::Symmetric { .. }
                            | SketchConstraint::PointOnCurve { .. }
                    )
                }),
            ) else {
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
}

pub(super) struct SketchProblem<'a> {
    sketch: &'a SketchDefinition,
    parameters: &'a HashMap<String, ParameterValue>,
    variables: HashMap<&'a str, usize>,
    fixed: HashMap<&'a str, SketchPoint2>,
    lines: HashMap<&'a str, &'a SketchLine>,
    entities: HashMap<&'a str, Entity<'a>>,
    native: Session,
    spline_tangents: HashMap<(&'a str, &'a str), &'a str>,
}

#[derive(Clone, Copy)]
enum Equation<'a> {
    Constraint(&'a SketchConstraint),
    Arc(&'a SketchArc),
    Ellipse(&'a SketchEllipse),
}

impl<'a> SketchProblem<'a> {
    pub(super) fn new(
        sketch: &'a SketchDefinition,
        parameters: &'a HashMap<String, ParameterValue>,
        variables: HashMap<&'a str, usize>,
        fixed: HashMap<&'a str, SketchPoint2>,
    ) -> Result<Self, ModelError> {
        Ok(Self {
            sketch,
            parameters,
            variables,
            fixed,
            lines: sketch
                .lines
                .iter()
                .map(|line| (line.id.as_str(), line))
                .collect(),
            entities: sketch.entities(),
            native: Session::new()?,
            spline_tangents: sketch.spline_tangent_index()?,
        })
    }
}

impl SketchProblem<'_> {
    fn equations(&self) -> impl Iterator<Item = Equation<'_>> {
        self.sketch
            .constraints
            .iter()
            .filter(|constraint| !self.sketch.spline_tangency(constraint))
            .map(Equation::Constraint)
            .chain(self.sketch.arcs.iter().map(Equation::Arc))
            .chain(self.sketch.ellipses.iter().map(Equation::Ellipse))
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
            Equation::Ellipse(e) => {
                let c = self.point(&e.center, values);
                residuals.push(normalized_dot(
                    (c, self.point(&e.major, values)),
                    (c, self.point(&e.minor, values)),
                )?);
                Ok(())
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

    fn spline_neighbor_refs<'a>(&'a self, spline: &'a SketchSpline) -> Vec<&'a str> {
        let mut refs = spline.points.iter().map(String::as_str).collect::<Vec<_>>();
        for at in [
            spline.points.first().unwrap(),
            spline.points.last().unwrap(),
        ] {
            if let Some(neighbor) = self.spline_tangents.get(&(spline.id.as_str(), at.as_str())) {
                match self.entities[neighbor] {
                    Entity::Line(l) => refs.extend([l.start.as_str(), l.end.as_str()]),
                    Entity::Arc(a) => {
                        refs.extend([a.center.as_str(), a.start.as_str(), a.end.as_str()])
                    }
                    _ => {}
                }
            }
        }
        refs
    }
    fn spline_curve<'a>(
        &'a self,
        spline: &SketchSpline,
        values: &[f64],
    ) -> Result<Shape<'a>, ModelError> {
        let away = |at: &str| -> Option<Vec3> {
            let neighbor = self.spline_tangents.get(&(spline.id.as_str(), at))?;
            let p = self.point(at, values);
            let d = match self.entities[neighbor] {
                Entity::Line(l) => {
                    let other = self.point(if l.start == at { &l.end } else { &l.start }, values);
                    SketchPoint2 {
                        x: other.x - p.x,
                        y: other.y - p.y,
                    }
                }
                Entity::Arc(a) => {
                    let c = self.point(&a.center, values);
                    let k = (if a.clockwise { -1.0 } else { 1.0 })
                        * (if a.start == at { 1.0 } else { -1.0 });
                    SketchPoint2 {
                        x: -(p.y - c.y) * k,
                        y: (p.x - c.x) * k,
                    }
                }
                _ => unreachable!("validated spline tangency"),
            };
            Some(Vec3::new(d.x, d.y, 0.0))
        };
        let periodic = spline.closed();
        let through = if periodic {
            &spline.points[..spline.points.len() - 1]
        } else {
            &spline.points[..]
        };
        let segment = CurveSegment::Spline {
            points: through
                .iter()
                .map(|id| {
                    let p = self.point(id, values);
                    Vec3::new(p.x, p.y, 0.0)
                })
                .collect(),
            start_tangent: away(&spline.points[0]).map(|d| Vec3::new(-d.x, -d.y, -d.z)),
            end_tangent: away(spline.points.last().unwrap()),
            periodic,
        };
        Ok(self.native.create_curve_wire(&[segment], periodic)?)
    }

    fn radius_points<'a>(&'a self, id: &str) -> (&'a str, &'a str) {
        match self.entities[id] {
            Entity::Circle(c) => (&c.center, &c.rim),
            Entity::Arc(a) => (&a.center, &a.start),
            _ => unreachable!("validated radius curve"),
        }
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

    pub(super) fn constraint_residuals(
        &self,
        constraint: &SketchConstraint,
        values: &[f64],
        residuals: &mut Vec<f64>,
    ) -> Result<(), ModelError> {
        let start = residuals.len();
        match constraint {
            SketchConstraint::Angle {
                first,
                second,
                value,
            } => {
                let angle =
                    normalized_cross(self.line(first, values), self.line(second, values))?.atan2(
                        normalized_dot(self.line(first, values), self.line(second, values))?,
                    );
                let target = scalar(value, self.parameters, Dimension::Scalar)?;
                let delta = angle - target;
                residuals.push(delta.sin().atan2(delta.cos()));
            }
            SketchConstraint::Radius { curve, value }
            | SketchConstraint::Diameter { curve, value } => {
                let (center, rim) = self.radius_points(curve);
                let radius = line_length((self.point(center, values), self.point(rim, values)));
                let factor = if matches!(constraint, SketchConstraint::Diameter { .. }) {
                    2.0
                } else {
                    1.0
                };
                residuals
                    .push(factor * radius - scalar(value, self.parameters, Dimension::Length)?);
            }
            SketchConstraint::Symmetric {
                first,
                second,
                axis,
            } => {
                let (a, b) = (self.point(first, values), self.point(second, values));
                let (u, v) = self.line(axis, values);
                let (dx, dy) = (v.x - u.x, v.y - u.y);
                let length = dx.hypot(dy);
                if length <= f64::EPSILON {
                    return Err(ModelError::new("symmetry axis has zero length"));
                }
                residuals.push(
                    ((a.x + b.x) * 0.5 - u.x) * dy / length
                        - ((a.y + b.y) * 0.5 - u.y) * dx / length,
                );
                residuals.push((b.x - a.x) * dx / length + (b.y - a.y) * dy / length);
            }
            SketchConstraint::PointOnCurve { point, curve } => {
                let p = self.point(point, values);
                match self.entities[curve.as_str()] {
                    Entity::Line(line) => {
                        let (a, b) = (
                            self.point(&line.start, values),
                            self.point(&line.end, values),
                        );
                        let length = line_length((a, b));
                        if length <= f64::EPSILON {
                            return Err(ModelError::new("point-on-line has a zero length line"));
                        }
                        residuals
                            .push(((p.x - a.x) * (b.y - a.y) - (p.y - a.y) * (b.x - a.x)) / length);
                    }
                    Entity::Circle(_) | Entity::Arc(_) => {
                        let (center, rim) = self.radius_points(curve);
                        let c = self.point(center, values);
                        residuals
                            .push(line_length((c, p)) - line_length((c, self.point(rim, values))));
                        if let Entity::Arc(arc) = self.entities[curve.as_str()] {
                            let a = self.point(&arc.start, values);
                            let b = self.point(&arc.end, values);
                            let sign = if arc.clockwise { -1.0 } else { 1.0 };
                            let start = (a.y - c.y).atan2(a.x - c.x);
                            let end = (b.y - c.y).atan2(b.x - c.x);
                            let at = (p.y - c.y).atan2(p.x - c.x);
                            let span = (sign * (end - start)).rem_euclid(std::f64::consts::TAU);
                            let along = (sign * (at - start)).rem_euclid(std::f64::consts::TAU);
                            let violation = if along <= span {
                                0.0
                            } else {
                                (along - span).min(std::f64::consts::TAU - along)
                            };
                            residuals.push(violation * line_length((c, a)));
                        }
                    }
                    Entity::Ellipse(e) => {
                        let c = self.point(&e.center, values);
                        let major = self.point(&e.major, values);
                        let minor = self.point(&e.minor, values);
                        let a = line_length((c, major));
                        let b = line_length((c, minor));
                        if a <= f64::EPSILON || b <= f64::EPSILON {
                            return Err(ModelError::new("point-on-ellipse needs positive axes"));
                        }
                        let x = ((p.x - c.x) * (major.x - c.x) + (p.y - c.y) * (major.y - c.y)) / a;
                        let y =
                            (-(p.x - c.x) * (major.y - c.y) + (p.y - c.y) * (major.x - c.x)) / a;
                        residuals.push(((x / a).hypot(y / b) - 1.0) * b);
                    }
                    Entity::Spline(spline) => {
                        let curve = self.spline_curve(spline, values)?;
                        let nearest = self
                            .native
                            .curve_closest_point(&curve, Vec3::new(p.x, p.y, 0.0))?;
                        residuals.extend([p.x - nearest.x, p.y - nearest.y]);
                    }
                }
            }
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
            SketchConstraint::EqualRadius { first, second } => {
                let (a, b) = self.radius_points(first);
                let (c, d) = self.radius_points(second);
                residuals.push(
                    line_length((self.point(a, values), self.point(b, values)))
                        - line_length((self.point(c, values), self.point(d, values))),
                );
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
            SketchConstraint::EqualRadius { first, second } => {
                let (a, b) = self.radius_points(first);
                let (c, d) = self.radius_points(second);
                vec![a, b, c, d]
            }
            SketchConstraint::Radius { curve, .. } | SketchConstraint::Diameter { curve, .. } => {
                let (a, b) = self.radius_points(curve);
                vec![a, b]
            }
            SketchConstraint::Symmetric {
                first,
                second,
                axis,
            } => {
                let line = self.lines[axis.as_str()];
                vec![first.as_str(), second.as_str(), &line.start, &line.end]
            }
            SketchConstraint::PointOnCurve { point, curve } => {
                let mut refs = vec![point.as_str()];
                match self.entities[curve.as_str()] {
                    Entity::Line(l) => refs.extend([l.start.as_str(), l.end.as_str()]),
                    Entity::Circle(c) => refs.extend([c.center.as_str(), c.rim.as_str()]),
                    Entity::Arc(a) => {
                        refs.extend([a.center.as_str(), a.start.as_str(), a.end.as_str()])
                    }
                    Entity::Ellipse(e) => {
                        refs.extend([e.center.as_str(), e.major.as_str(), e.minor.as_str()])
                    }
                    Entity::Spline(spline) => refs.extend(self.spline_neighbor_refs(spline)),
                }
                refs
            }
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
            SketchConstraint::Angle { first, second, .. }
            | SketchConstraint::Parallel { first, second }
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
                Equation::Ellipse(e) => self.point_columns(vec![&e.center, &e.major, &e.minor]),
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

fn least_squares_step(
    jacobian: &SparseJacobian,
    residual: &[f64],
    minimum_change: bool,
) -> Option<Vec<f64>> {
    let rhs = jacobian
        .transpose_times(residual)
        .into_iter()
        .map(|value| -value)
        .collect();
    let mut normal = jacobian.normal_matrix();
    // Curved underconstrained equations must move all constrained coordinates:
    // dropping a dependent pivot can otherwise freeze the coordinate needed to
    // reach a circle/ellipse. Small relative LM damping leaves untouched
    // columns untouched; rank/free-degree reporting uses the undamped matrix.
    if minimum_change {
        for i in 0..normal.size() {
            let damping = normal.diagonal(i) * 1e-8;
            normal.add_diagonal(i, damping);
        }
    }
    normal.solve_dropping_null(rhs, PIVOT_TOLERANCE)
}

fn squared_norm(values: &[f64]) -> f64 {
    values.iter().map(|value| value * value).sum()
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
