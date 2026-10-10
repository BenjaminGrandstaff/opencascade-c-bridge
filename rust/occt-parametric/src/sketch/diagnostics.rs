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
impl SketchDefinition {
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
                    session.create_curve_wire(&segments, closed)?
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
