//! Per-constraint residual checks and curve previews for solved or diagnostic sketches.

use super::solver::SketchProblem;
use super::*;

/// Per-constraint diagnostics from the same residual equations as the solver.
#[derive(Clone, Debug, PartialEq)]
pub struct SketchConstraintCheck {
    pub index: usize,
    pub max_residual: Option<f64>,
    pub satisfied: bool,
    /// Spline end tangency is imposed during curve construction, not solved
    /// by a residual equation. It must not be reported as a measured zero.
    pub by_construction: bool,
}
/// Measured seed, native spline contact and circular contact, with any arc
/// domain violation kept separate from tangent alignment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SketchSplineCircleContact {
    pub points: [SketchPoint2; 3],
    pub tangent_sine: f64,
    pub arc_span_residual_mm: Option<f64>,
}

impl SketchDefinition {
    /// Exact ellipse support contacts and their supporting-line projections,
    /// indexed by constraint number. Distinct positions reveal a diagnostic
    /// residual. Indexing the curves keeps this O(lines + ellipses + constraints).
    pub fn line_ellipse_tangent_contacts(
        &self,
        solution: &SketchSolution,
    ) -> Result<HashMap<usize, [SketchPoint2; 2]>, ModelError> {
        let lines: HashMap<_, _> = self.lines.iter().map(|l| (l.id.as_str(), l)).collect();
        let ellipses: HashMap<_, _> = self.ellipses.iter().map(|e| (e.id.as_str(), e)).collect();
        let p = |id: &str| {
            solution
                .points
                .get(id)
                .copied()
                .ok_or_else(|| ModelError::new("tangency solution is missing a point"))
        };
        let mut result = HashMap::new();
        for (index, constraint) in self.constraints.iter().enumerate() {
            let SketchConstraint::LineEllipseTangent {
                line,
                ellipse,
                side,
            } = constraint
            else {
                continue;
            };
            let l = lines
                .get(line.as_str())
                .ok_or_else(|| ModelError::new("unknown tangency line"))?;
            let e = ellipses
                .get(ellipse.as_str())
                .ok_or_else(|| ModelError::new("unknown tangency ellipse"))?;
            let (_, contacts) = super::geometry::line_ellipse_contact(
                (p(&l.start)?, p(&l.end)?),
                p(&e.center)?,
                p(&e.major)?,
                p(&e.minor)?,
                *side,
            )?;
            result.insert(index, contacts);
        }
        Ok(result)
    }

    /// Seed point, bounded native spline contact, supporting-line projection,
    /// and measured tangent sine, indexed by line-spline constraint number.
    pub fn line_spline_tangent_contacts(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
    ) -> Result<HashMap<usize, ([SketchPoint2; 3], f64)>, ModelError> {
        if !self
            .constraints
            .iter()
            .any(|c| matches!(c, SketchConstraint::LineSplineTangent { .. }))
        {
            return Ok(HashMap::new());
        }
        if !self.projections.is_empty() {
            return Err(ModelError::new(
                "projected sketch requires source-geometry resolution before contact diagnostics",
            ));
        }
        if solution
            .points
            .values()
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err(ModelError::new(
                "spline contact diagnostic point is not finite",
            ));
        }
        self.validate(parameters)?;
        let fixed = self
            .points
            .iter()
            .map(|p| {
                solution
                    .points
                    .get(&p.id)
                    .copied()
                    .map(|v| (p.id.as_str(), v))
                    .ok_or_else(|| ModelError::new("contact solution is missing a point"))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let problem = SketchProblem::new(self, parameters, HashMap::new(), fixed)?;
        let mut result = HashMap::new();
        for (index, c) in self.constraints.iter().enumerate() {
            if let SketchConstraint::LineSplineTangent {
                line,
                spline,
                point,
            } = c
            {
                result.insert(
                    index,
                    problem.line_spline_contact(line, spline, point, &[])?,
                );
            }
        }
        Ok(result)
    }

    /// Seed point, bounded native spline contact, second bounded native spline contact,
    /// and measured tangent sine, indexed by spline-spline constraint number.
    pub fn spline_spline_tangent_contacts(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
    ) -> Result<HashMap<usize, ([SketchPoint2; 3], f64)>, ModelError> {
        if !self
            .constraints
            .iter()
            .any(|c| matches!(c, SketchConstraint::SplineSplineTangent { .. }))
        {
            return Ok(HashMap::new());
        }
        if !self.projections.is_empty() {
            return Err(ModelError::new(
                "projected sketch requires source-geometry resolution before contact diagnostics",
            ));
        }
        if solution
            .points
            .values()
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err(ModelError::new(
                "spline contact diagnostic point is not finite",
            ));
        }
        self.validate(parameters)?;
        let fixed = self
            .points
            .iter()
            .map(|p| {
                solution
                    .points
                    .get(&p.id)
                    .copied()
                    .map(|v| (p.id.as_str(), v))
                    .ok_or_else(|| ModelError::new("contact solution is missing a point"))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let problem = SketchProblem::new(self, parameters, HashMap::new(), fixed)?;
        let mut result = HashMap::new();
        for (index, c) in self.constraints.iter().enumerate() {
            if let SketchConstraint::SplineSplineTangent {
                first,
                second,
                point,
            } = c
            {
                result.insert(
                    index,
                    problem.spline_spline_contact(first, second, point, &[])?,
                );
            }
        }
        Ok(result)
    }

    /// Seed point, bounded native spline contact, native ellipse contact,
    /// and measured tangent sine, indexed by spline-ellipse constraint number.
    pub fn spline_ellipse_tangent_contacts(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
    ) -> Result<HashMap<usize, ([SketchPoint2; 3], f64)>, ModelError> {
        if !self
            .constraints
            .iter()
            .any(|c| matches!(c, SketchConstraint::SplineEllipseTangent { .. }))
        {
            return Ok(HashMap::new());
        }
        if !self.projections.is_empty() {
            return Err(ModelError::new(
                "projected sketch requires source-geometry resolution before contact diagnostics",
            ));
        }
        if solution
            .points
            .values()
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err(ModelError::new(
                "spline contact diagnostic point is not finite",
            ));
        }
        self.validate(parameters)?;
        let fixed = self
            .points
            .iter()
            .map(|p| {
                solution
                    .points
                    .get(&p.id)
                    .copied()
                    .map(|v| (p.id.as_str(), v))
                    .ok_or_else(|| ModelError::new("contact solution is missing a point"))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let problem = SketchProblem::new(self, parameters, HashMap::new(), fixed)?;
        let mut result = HashMap::new();
        for (index, c) in self.constraints.iter().enumerate() {
            if let SketchConstraint::SplineEllipseTangent {
                spline,
                ellipse,
                point,
            } = c
            {
                result.insert(
                    index,
                    problem.spline_ellipse_contact(spline, ellipse, point, &[])?,
                );
            }
        }
        Ok(result)
    }

    /// Seed point, bounded native spline contact, circular contact,
    /// and measured tangent sine, indexed by spline-circle constraint number.
    pub fn spline_circle_tangent_contacts(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
    ) -> Result<HashMap<usize, SketchSplineCircleContact>, ModelError> {
        if !self
            .constraints
            .iter()
            .any(|c| matches!(c, SketchConstraint::SplineCircleTangent { .. }))
        {
            return Ok(HashMap::new());
        }
        if !self.projections.is_empty() {
            return Err(ModelError::new(
                "projected sketch requires source-geometry resolution before contact diagnostics",
            ));
        }
        if solution
            .points
            .values()
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err(ModelError::new(
                "spline contact diagnostic point is not finite",
            ));
        }
        self.validate(parameters)?;
        let fixed = self
            .points
            .iter()
            .map(|p| {
                solution
                    .points
                    .get(&p.id)
                    .copied()
                    .map(|v| (p.id.as_str(), v))
                    .ok_or_else(|| ModelError::new("contact solution is missing a point"))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let problem = SketchProblem::new(self, parameters, HashMap::new(), fixed)?;
        let mut result = HashMap::new();
        for (index, c) in self.constraints.iter().enumerate() {
            if let SketchConstraint::SplineCircleTangent {
                spline,
                circle,
                point,
            } = c
            {
                result.insert(
                    index,
                    problem.spline_circle_contact(spline, circle, point, &[])?,
                );
            }
        }
        Ok(result)
    }

    pub fn constraint_checks(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
    ) -> Result<Vec<SketchConstraintCheck>, ModelError> {
        if !self.projections.is_empty() {
            return Err(ModelError::new(
                "projected sketch requires source-geometry resolution before diagnostics",
            ));
        }
        self.validate(parameters)?;
        let mut fixed = HashMap::new();
        for point in &self.points {
            let value = solution
                .points
                .get(&point.id)
                .ok_or_else(|| ModelError::new("sketch diagnostic solution is missing a point"))?;
            if !value.x.is_finite() || !value.y.is_finite() {
                return Err(ModelError::new("sketch diagnostic point is not finite"));
            }
            fixed.insert(point.id.as_str(), *value);
        }
        let problem = SketchProblem::new(self, parameters, HashMap::new(), fixed)?;
        self.constraints
            .iter()
            .enumerate()
            .map(|(index, constraint)| {
                if self.spline_tangency(constraint) {
                    return Ok(SketchConstraintCheck {
                        index,
                        max_residual: None,
                        satisfied: false,
                        by_construction: true,
                    });
                }
                let mut residuals = Vec::new();
                problem.constraint_residuals(constraint, &[], &mut residuals)?;
                let residual = max_abs(&residuals);
                Ok(SketchConstraintCheck {
                    index,
                    max_residual: Some(residual),
                    satisfied: residual <= RESIDUAL_TOLERANCE,
                    by_construction: false,
                })
            })
            .collect()
    }

    /// Sample each entity in sketch-local XY using native curve construction,
    /// including spline end tangency. Available for unsolved diagnostic sketches.
    pub fn preview_curves(
        &self,
        session: &Session,
        solution: &SketchSolution,
        samples: usize,
    ) -> Result<Vec<(String, Vec<SketchPoint2>)>, ModelError> {
        if !self.projections.is_empty() {
            return Err(ModelError::new(
                "projected sketch requires source-geometry resolution before preview",
            ));
        }
        if !(2..=256).contains(&samples) {
            return Err(ModelError::new("sketch preview samples must be in 2..256"));
        }
        for point in &self.points {
            let p = solution
                .points
                .get(&point.id)
                .ok_or_else(|| ModelError::new("sketch preview solution is missing a point"))?;
            if !p.x.is_finite() || !p.y.is_finite() {
                return Err(ModelError::new("sketch preview point is not finite"));
            }
        }
        self.validate_curve_geometry(&solution.points)?;
        let transform = |p: SketchPoint2| Vec3::new(p.x, p.y, 0.0);
        let mut result = Vec::new();
        for entity in self.entities().values() {
            let id = match entity {
                Entity::Line(e) => &e.id,
                Entity::Arc(e) => &e.id,
                Entity::Circle(e) => &e.id,
                Entity::Ellipse(e) => &e.id,
                Entity::Spline(e) => &e.id,
            };
            let wire = match entity {
                Entity::Ellipse(ellipse) => self.ellipse_wire(
                    session,
                    ellipse,
                    solution,
                    &transform,
                    &transform,
                    Vec3::new(0.0, 0.0, 1.0),
                )?,
                Entity::Circle(circle) => {
                    let center = solution.points[&circle.center];
                    let radius = line_length((center, solution.points[&circle.rim]));
                    session.create_circle_wire(
                        transform(center),
                        Vec3::new(0.0, 0.0, 1.0),
                        radius,
                    )?
                }
                _ => {
                    // A single segment has no connectivity condition to a neighbor.
                    let closed = matches!(entity,Entity::Spline(s) if s.closed());
                    let segments = self.profile_segments(
                        &[*entity],
                        solution,
                        &transform,
                        &transform,
                        closed,
                    )?;
                    // Render unsolved endpoint conflicts too; the accepted profile
                    // still enforces native closure during construction.
                    let native_closed =
                        closed && !matches!(entity, Entity::Spline(s) if s.endpoints.is_some());
                    session.create_curve_wire(&segments, native_closed)?
                }
            };
            let mut points = Vec::new();
            for edge in session.subshapes(&wire, ShapeType::Edge)? {
                points.extend(
                    session
                        .edge_sample_points(&edge, samples)?
                        .into_iter()
                        .map(|p| SketchPoint2 { x: p.x, y: p.y }),
                );
            }
            result.push((id.clone(), points));
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }
}
