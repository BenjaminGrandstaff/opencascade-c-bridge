//! Native wires and faces built from solved sketch profiles, including offset and edited profiles.

use super::validation::{validate_curve_entities, validate_open_endpoints};
use super::*;

impl SketchDefinition {
    /// O(expression size); resolves the same orthonormal frame for projection
    /// and native profile construction. Attachment/datum planes are caller-resolved.
    pub fn resolved_frame(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        datum: Option<ResolvedDatum>,
    ) -> Result<(Vec3, Vec3, Vec3), ModelError> {
        let x_axis = unit(vector(&self.x_axis, parameters, Dimension::Scalar)?)?;
        let (origin, y_axis) = match (
            self.datum_plane.is_some() || self.face_support.is_some(),
            datum,
        ) {
            (true, Some(ResolvedDatum::Plane { origin, normal })) => {
                if dot(x_axis, normal).abs() > 1e-9 {
                    return Err(ModelError::new("sketch x axis must lie in its datum plane"));
                }
                (origin, unit(cross(normal, x_axis))?)
            }
            (true, _) => return Err(ModelError::new("sketch requires a resolved plane datum")),
            (false, _) => (
                vector(&self.origin, parameters, Dimension::Length)?,
                unit(vector(&self.y_axis, parameters, Dimension::Scalar)?)?,
            ),
        };
        if dot(x_axis, y_axis).abs() > 1e-9 {
            return Err(ModelError::new("sketch plane axes must be perpendicular"));
        }
        Ok((origin, x_axis, y_axis))
    }

    /// Check a solution produced by this sketch against its acceptance policy.
    /// This uses the solver's local numerical rank, not a global rigidity proof.
    pub fn check_acceptance(&self, solution: &SketchSolution) -> Result<(), ModelError> {
        if !solution.solved {
            return Err(ModelError::new(format!(
                "sketch '{}' constraints did not converge ({:?}); maximum residual {}",
                self.id, solution.solver_report.termination, solution.max_residual
            )));
        }
        if self.acceptance == SketchAcceptance::FullyConstrained
            && !solution.solver_report.rank_reliable
        {
            return Err(ModelError::new(format!(
                "sketch '{}' cannot certify fully constrained geometry: local rank is uncertain",
                self.id
            )));
        }
        if self.acceptance == SketchAcceptance::FullyConstrained && solution.free_degrees > 0 {
            return Err(ModelError::new(format!(
                "sketch '{}' requires fully constrained geometry; {} free degrees remain",
                self.id, solution.free_degrees
            )));
        }
        Ok(())
    }

    /// Wire segments for the profile in order. Spline ends tangent to a line
    /// or arc continue that entity's direction away from the shared point.
    pub fn profile_segments(
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
                Entity::Spline(spline) if spline.basis.is_some() => spline.explicit_segment(
                    spline
                        .points
                        .iter()
                        .map(|id| transform(solution.points[id]))
                        .collect(),
                    solution
                        .spline_weights
                        .get(&spline.id)
                        .ok_or_else(|| {
                            ModelError::new("explicit spline solution is missing weights")
                        })?
                        .clone(),
                )?,
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
                Entity::Ellipse(e) if e.arc_ends().is_some() => {
                    let at = |id: &str| transform(solution.points[id]);
                    let normal = cross(
                        direction(SketchPoint2 { x: 1.0, y: 0.0 }),
                        direction(SketchPoint2 { x: 0.0, y: 1.0 }),
                    );
                    e.native_arc(
                        [
                            at(&e.center),
                            at(&e.major),
                            at(&e.minor),
                            at(start),
                            at(end),
                        ],
                        normal,
                    )
                }
                Entity::Circle(_) | Entity::Ellipse(_) => {
                    unreachable!("closed conics handled above")
                }
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
}
