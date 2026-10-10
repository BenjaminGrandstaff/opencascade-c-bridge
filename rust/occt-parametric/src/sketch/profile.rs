//! Native wires and faces built from solved sketch profiles, including offset and edited profiles.

use super::validation::{validate_curve_entities, validate_open_endpoints};
use super::*;

impl SketchDefinition {
    #[cfg(test)]
    pub(crate) fn face<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Shape<'session>, ModelError> {
        self.face_on_plane(session, parameters, None)
    }

    pub(super) fn ellipse_wire<'a>(
        &self,
        session: &'a Session,
        ellipse: &SketchEllipse,
        solution: &SketchSolution,
        transform: &impl Fn(SketchPoint2) -> Vec3,
        direction: &impl Fn(SketchPoint2) -> Vec3,
        normal: Vec3,
    ) -> Result<Shape<'a>, ModelError> {
        let c = solution.points[&ellipse.center];
        let a = solution.points[&ellipse.major];
        let b = solution.points[&ellipse.minor];
        Ok(session.create_ellipse_wire_axes(
            transform(c),
            normal,
            direction(SketchPoint2 {
                x: a.x - c.x,
                y: a.y - c.y,
            }),
            line_length((c, a)),
            line_length((c, b)),
        )?)
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
        let wire = if !self.profile_operations.is_empty() {
            let direction = |d: SketchPoint2| add(scale(x_axis, d.x), scale(y_axis, d.y));
            self.edited_profile(
                session,
                parameters,
                &solution,
                &profile,
                &transform,
                &direction,
                cross(x_axis, y_axis),
                closed,
            )?
        } else if let [Entity::Circle(circle)] = profile.as_slice() {
            if !closed {
                return Err(ModelError::new("open sketch profile cannot be a circle"));
            }
            let center = solution.points[&circle.center];
            let radius = line_length((center, solution.points[&circle.rim]));
            session.create_circle_wire(transform(center), cross(x_axis, y_axis), radius)?
        } else if let [Entity::Ellipse(ellipse)] = profile.as_slice() {
            if !closed && self.profile_operations.is_empty() {
                return Err(ModelError::new(
                    "open sketch profile cannot be a full ellipse",
                ));
            }
            let direction = |d: SketchPoint2| add(scale(x_axis, d.x), scale(y_axis, d.y));
            self.ellipse_wire(
                session,
                ellipse,
                &solution,
                &transform,
                &direction,
                cross(x_axis, y_axis),
            )?
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

    #[allow(clippy::too_many_arguments)]
    fn entity_wire<'a>(
        &self,
        session: &'a Session,
        entity: Entity<'_>,
        solution: &SketchSolution,
        transform: &impl Fn(SketchPoint2) -> Vec3,
        direction: &impl Fn(SketchPoint2) -> Vec3,
        normal: Vec3,
    ) -> Result<Shape<'a>, ModelError> {
        match entity {
            Entity::Circle(c) => {
                let center = solution.points[&c.center];
                let rim = solution.points[&c.rim];
                let radius = line_length((center, rim));
                Ok(session.create_ellipse_wire_axes(
                    transform(center),
                    normal,
                    direction(SketchPoint2 {
                        x: rim.x - center.x,
                        y: rim.y - center.y,
                    }),
                    radius,
                    radius,
                )?)
            }
            Entity::Ellipse(e) => {
                self.ellipse_wire(session, e, solution, transform, direction, normal)
            }
            _ => {
                let closed = matches!(entity,Entity::Spline(s) if s.closed());
                let segments =
                    self.profile_segments(&[entity], solution, transform, direction, closed)?;
                Ok(session.create_curve_wire(&segments, closed)?)
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn edited_profile<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        profile: &[Entity<'_>],
        transform: &impl Fn(SketchPoint2) -> Vec3,
        direction: &impl Fn(SketchPoint2) -> Vec3,
        normal: Vec3,
        closed: bool,
    ) -> Result<Shape<'a>, ModelError> {
        let index = profile
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id(), i))
            .collect::<HashMap<_, _>>();
        let mut wires = profile
            .iter()
            .map(|e| self.entity_wire(session, *e, solution, transform, direction, normal))
            .collect::<Result<Vec<_>, _>>()?;
        let mut joined = None;
        for operation in &self.profile_operations {
            match operation {
                SketchProfileOperation::Trim {
                    entity,
                    first,
                    last,
                } => {
                    let i = *index.get(entity.as_str()).ok_or_else(|| {
                        ModelError::new("trim entity must belong to the selected profile")
                    })?;
                    wires[i] = session.trim_curve(
                        &wires[i],
                        scalar(first, parameters, Dimension::Scalar)?,
                        scalar(last, parameters, Dimension::Scalar)?,
                    )?;
                }
                SketchProfileOperation::Extend { entity, start, end } => {
                    let i = *index.get(entity.as_str()).ok_or_else(|| {
                        ModelError::new("extend entity must belong to the selected profile")
                    })?;
                    wires[i] = session.extend_curve(
                        &wires[i],
                        scalar(start, parameters, Dimension::Length)?,
                        scalar(end, parameters, Dimension::Length)?,
                    )?;
                }
                SketchProfileOperation::Offset { distance, join } => {
                    let source = match joined.take() {
                        Some(shape) => shape,
                        None => session.join_wires(&wires.iter().collect::<Vec<_>>(), false)?,
                    };
                    joined = Some(session.offset_wire(
                        &source,
                        normal,
                        scalar(distance, parameters, Dimension::Length)?,
                        *join == SketchOffsetJoin::Intersection,
                    )?);
                }
            }
        }
        let wire = match joined {
            Some(shape) => shape,
            None => session.join_wires(&wires.iter().collect::<Vec<_>>(), closed)?,
        };
        if session.wire_is_closed(&wire)? != closed {
            return Err(ModelError::new(if closed {
                "edited sketch profile must be closed"
            } else {
                "edited sketch profile must be open"
            }));
        }
        Ok(wire)
    }

    /// Native sampled edited profile in sketch-local XY, for diagnostics.
    /// Source constraints stay attached to source entities; these curves show
    /// the derived boundary actually used by solid/wire features.
    pub fn preview_edited_profile(
        &self,
        session: &Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        samples: usize,
        closed: bool,
    ) -> Result<Vec<Vec<SketchPoint2>>, ModelError> {
        if !(2..=256).contains(&samples) {
            return Err(ModelError::new("profile preview samples must be in 2..256"));
        }
        if self.profile_operations.is_empty() {
            return Ok(Vec::new());
        }
        self.validate(parameters)?;
        for p in &self.points {
            if !solution
                .points
                .get(&p.id)
                .is_some_and(|p| p.x.is_finite() && p.y.is_finite())
            {
                return Err(ModelError::new("profile preview is missing a finite point"));
            }
        }
        self.validate_curve_geometry(&solution.points)?;
        let transform = |p: SketchPoint2| Vec3::new(p.x, p.y, 0.0);
        let wire = self.edited_profile(
            session,
            parameters,
            solution,
            &self.profile_entities()?,
            &transform,
            &transform,
            Vec3::new(0.0, 0.0, 1.0),
            closed,
        )?;
        session
            .subshapes(&wire, ShapeType::Edge)?
            .iter()
            .map(|edge| {
                Ok(session
                    .edge_sample_points(edge, samples)?
                    .into_iter()
                    .map(|p| SketchPoint2 { x: p.x, y: p.y })
                    .collect())
            })
            .collect()
    }

    /// Wire segments for the profile in order. Spline ends tangent to a line
    /// or arc continue that entity's direction away from the shared point.
    pub(super) fn profile_segments(
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

    fn profile_entities(&self) -> Result<Vec<Entity<'_>>, ModelError> {
        let entities = self.entities();
        let only_lines =
            self.arcs.is_empty() && self.splines.is_empty() && self.ellipses.is_empty();
        let profile = if !self.profile.is_empty() {
            self.profile
                .iter()
                .map(|id| entities[id.as_str()])
                .collect::<Vec<_>>()
        } else if self.circles.len() == 1 && self.lines.is_empty() && only_lines {
            vec![Entity::Circle(&self.circles[0])]
        } else if self.ellipses.len() == 1
            && self.lines.is_empty()
            && self.arcs.is_empty()
            && self.circles.is_empty()
            && self.splines.is_empty()
        {
            vec![Entity::Ellipse(&self.ellipses[0])]
        } else if self.splines.len() == 1
            && self.lines.is_empty()
            && self.arcs.is_empty()
            && self.circles.is_empty()
            && self.ellipses.is_empty()
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
}
