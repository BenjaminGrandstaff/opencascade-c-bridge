//! Native wires and faces built from solved sketch profiles, including offset and edited profiles.

use super::validation::{validate_curve_entities, validate_open_endpoints};
use super::*;

impl SketchDefinition {
    /// O(expression size); resolves the same orthonormal frame for projection
    /// and native profile construction. Attachment/datum planes are caller-resolved.
    pub(crate) fn resolved_frame(
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
        if let Some((start, end)) = ellipse.arc_ends() {
            let at = |id: &str| transform(solution.points[id]);
            return Ok(session.create_curve_wire(
                &[ellipse.native_arc(
                    [
                        at(&ellipse.center),
                        at(&ellipse.major),
                        at(&ellipse.minor),
                        at(start),
                        at(end),
                    ],
                    normal,
                )],
                false,
            )?);
        }
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

    /// The profile face, with any `holes` added natively as inner boundaries:
    /// each strictly inside the outer loop, clear of the others and not nested.
    /// With `regions`, a compound of that face and one face per region; the
    /// regions' faces must not overlap each other or the profile's.
    pub(crate) fn face_on_plane<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
        datum: Option<ResolvedDatum>,
    ) -> Result<Shape<'session>, ModelError> {
        if let std::borrow::Cow::Owned(expanded) = self.with_patterns(Some(parameters))? {
            return expanded.face_on_plane(session, parameters, datum);
        }
        let solution = self.solved(parameters)?;
        let (origin, x_axis, y_axis) = self.resolved_frame(parameters, datum)?;
        let transform =
            |point: SketchPoint2| add(origin, add(scale(x_axis, point.x), scale(y_axis, point.y)));
        let direction = |d: SketchPoint2| add(scale(x_axis, d.x), scale(y_axis, d.y));
        let normal = cross(x_axis, y_axis);
        let library = if self.curve_operations.is_empty() {
            None
        } else {
            Some(self.curve_library(session, parameters, &solution, (origin, x_axis, y_axis))?)
        };
        let face = |outer: &Shape<'session>, holes: &[Vec<String>], name: &str| {
            if holes.is_empty() {
                let face = session.create_face_from_wire(outer)?;
                if !session.is_valid(&face)? {
                    return Err(ModelError::new(format!(
                        "sketch {name} produced an invalid face"
                    )));
                }
                return Ok(face);
            }
            let loops = holes
                .iter()
                .map(|ids| {
                    self.loop_wire(
                        session,
                        ids,
                        &solution,
                        &transform,
                        &direction,
                        normal,
                        library.as_ref(),
                    )
                })
                .collect::<Result<Vec<_>, ModelError>>()?;
            session
                .create_face_with_holes(outer, &loops.iter().collect::<Vec<_>>())
                .map_err(|e| {
                    ModelError::from(e).context(&format!("sketch '{}' {name} holes", self.id))
                })
        };
        // A sketch of only texts has no profile face.
        let text_only = self.profile.is_empty()
            && self.regions.is_empty()
            && !self.texts.is_empty()
            && self.lines.is_empty()
            && self.circles.is_empty()
            && self.arcs.is_empty()
            && self.ellipses.is_empty()
            && self.splines.is_empty();
        let mut faces = Vec::new();
        let mut names = Vec::new();
        if let Some(discovery) = &self.region_discovery {
            faces.push(self.discovered_regions(
                session,
                parameters,
                &solution,
                discovery,
                (origin, x_axis, y_axis),
            )?);
            names.push("discovered regions".to_owned());
        } else if !text_only {
            let wire = self.solved_profile_wire(session, parameters, datum, true, &solution)?;
            let main = face(&wire, &self.holes, "profile")?;
            if self.regions.is_empty() && self.texts.is_empty() && self.region_edits.is_empty() {
                return Ok(main);
            }
            faces.push(main);
            names.push("profile".to_owned());
        }
        for (index, region) in self.regions.iter().enumerate() {
            let name = format!("region {index}");
            let outer = self.loop_wire(
                session,
                &region.profile,
                &solution,
                &transform,
                &direction,
                normal,
                library.as_ref(),
            )?;
            faces.push(face(&outer, &region.holes, &name)?);
            names.push(name);
        }
        if !self.region_edits.is_empty() {
            let mut flattened = Vec::new();
            for face in &faces {
                flattened.extend(session.subshapes(face, ShapeType::Face)?);
            }
            faces = self.edited_regions(
                session,
                parameters,
                &solution,
                flattened,
                (origin, x_axis, y_axis),
            )?;
            names = (0..faces.len()).map(|i| format!("region {i}")).collect();
        }
        for text in &self.texts {
            faces.push(
                self.text_faces(session, parameters, &solution, text, &transform, &direction)?,
            );
            names.push(format!("text '{}'", text.id));
        }
        if faces.len() == 1 {
            return Ok(faces.pop().unwrap());
        }
        disjoint_faces(session, &faces, &names)?;
        Ok(session.create_compound(&faces.iter().collect::<Vec<_>>())?)
    }

    /// The glyph faces of one text, placed at its solved anchor along its
    /// baseline angle in the sketch plane.
    pub(crate) fn text_faces<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        text: &SketchText,
        transform: &impl Fn(SketchPoint2) -> Vec3,
        direction: &impl Fn(SketchPoint2) -> Vec3,
    ) -> Result<Shape<'session>, ModelError> {
        let file = font_file(session, &text.font, text.style)?;
        let angle = match &text.angle {
            Some(angle) => scalar(angle, parameters, Dimension::Scalar)?,
            None => 0.0,
        };
        let (sin, cos) = angle.sin_cos();
        let align = match text.align {
            SketchTextAlign::Start => occt_bridge::TextAlign::Start,
            SketchTextAlign::Center => occt_bridge::TextAlign::Center,
            SketchTextAlign::End => occt_bridge::TextAlign::End,
        };
        session
            .create_text(
                &text.text,
                &file,
                scalar(&text.height, parameters, Dimension::Length)?,
                transform(solution.points[&text.anchor]),
                direction(SketchPoint2 { x: cos, y: sin }),
                direction(SketchPoint2 { x: -sin, y: cos }),
                align,
            )
            .map_err(|error| ModelError::from(error).context(&format!("sketch text '{}'", text.id)))
    }

    /// Sampled glyph edges of every text in sketch-local XY, with the
    /// resolved font file, for previews: `samples` points per edge and at
    /// most 20,000 edges in all.
    pub fn text_outlines(
        &self,
        session: &Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        samples: usize,
    ) -> Result<Vec<SketchTextOutlines>, ModelError> {
        let local = |p: SketchPoint2| Vec3::new(p.x, p.y, 0.0);
        let mut edges_left = 20_000usize;
        self.texts
            .iter()
            .map(|text| {
                let faces = self.text_faces(session, parameters, solution, text, &local, &local)?;
                let edges = session.subshapes(&faces, ShapeType::Edge)?;
                edges_left = edges_left
                    .checked_sub(edges.len())
                    .ok_or_else(|| ModelError::new("text previews exceed 20,000 glyph edges"))?;
                let outlines = edges
                    .iter()
                    .map(|edge| {
                        Ok(session
                            .edge_sample_points(edge, samples)?
                            .into_iter()
                            .map(|p| SketchPoint2 { x: p.x, y: p.y })
                            .collect())
                    })
                    .collect::<Result<Vec<_>, ModelError>>()?;
                Ok(SketchTextOutlines {
                    id: text.id.clone(),
                    font_file: font_file(session, &text.font, text.style)?,
                    outlines,
                })
            })
            .collect()
    }

    /// A closed boundary wire through the named entities: an exact circle or
    /// full ellipse alone, or a connected chain of segments.
    #[allow(clippy::too_many_arguments)]
    fn loop_wire<'session>(
        &self,
        session: &'session Session,
        ids: &[String],
        solution: &SketchSolution,
        transform: &impl Fn(SketchPoint2) -> Vec3,
        direction: &impl Fn(SketchPoint2) -> Vec3,
        normal: Vec3,
        library: Option<&super::editing::CurveLibrary<'session>>,
    ) -> Result<Shape<'session>, ModelError> {
        if let Some(library) = library {
            let wires: Vec<_> = ids.iter().map(|id| &library.curves[id]).collect();
            return Ok(session.join_wires(&wires, true)?);
        }
        let entities = self.entities();
        let members = ids
            .iter()
            .map(|id| entities[id.as_str()])
            .collect::<Vec<_>>();
        match members.as_slice() {
            // Exact circles, as in outer profiles: an equal-radius ellipse
            // would force numeric extrema in hole checks.
            [Entity::Circle(circle)] => {
                let center = solution.points[&circle.center];
                let radius = line_length((center, solution.points[&circle.rim]));
                Ok(session.create_circle_wire(transform(center), normal, radius)?)
            }
            [entity @ Entity::Ellipse(e)] if e.arc_ends().is_none() => {
                self.entity_wire(session, *entity, solution, transform, direction, normal)
            }
            _ => {
                let segments =
                    self.profile_segments(&members, solution, transform, direction, true)?;
                Ok(session.create_curve_wire(&segments, true)?)
            }
        }
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

    fn solved(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<SketchSolution, ModelError> {
        let solution = self.solve(parameters)?;
        self.check_acceptance(&solution)?;
        Ok(solution)
    }

    /// Check a solution produced by this sketch against its acceptance policy.
    /// This uses the solver's local numerical rank, not a global rigidity proof.
    pub fn check_acceptance(&self, solution: &SketchSolution) -> Result<(), ModelError> {
        if !solution.solved {
            return Err(ModelError::new(format!(
                "sketch '{}' constraints conflict; maximum residual {}",
                self.id, solution.max_residual
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

    fn profile_wire<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
        datum: Option<ResolvedDatum>,
        closed: bool,
    ) -> Result<Shape<'session>, ModelError> {
        if let std::borrow::Cow::Owned(expanded) = self.with_patterns(Some(parameters))? {
            return expanded.profile_wire(session, parameters, datum, closed);
        }
        if !self.texts.is_empty() {
            return Err(ModelError::new(
                "sketch texts apply only to face outputs, not wires",
            ));
        }
        if !self.region_edits.is_empty() {
            return Err(ModelError::new(
                "region edits apply only to face outputs, not wires",
            ));
        }
        if self.region_discovery.is_some() {
            return Err(ModelError::new(
                "sketch region discovery applies only to face outputs, not wires",
            ));
        }
        if !self.holes.is_empty() || !self.regions.is_empty() {
            return Err(ModelError::new(
                "sketch holes and regions apply only to face outputs, not wires",
            ));
        }
        let solution = self.solved(parameters)?;
        self.solved_profile_wire(session, parameters, datum, closed, &solution)
    }

    fn solved_profile_wire<'session>(
        &self,
        session: &'session Session,
        parameters: &HashMap<String, ParameterValue>,
        datum: Option<ResolvedDatum>,
        closed: bool,
        solution: &SketchSolution,
    ) -> Result<Shape<'session>, ModelError> {
        let (origin, x_axis, y_axis) = self.resolved_frame(parameters, datum)?;
        let transform =
            |point: SketchPoint2| add(origin, add(scale(x_axis, point.x), scale(y_axis, point.y)));
        if !self.curve_operations.is_empty() {
            let library =
                self.curve_library(session, parameters, solution, (origin, x_axis, y_axis))?;
            let ids = if self.profile.is_empty() {
                &library.active
            } else {
                &self.profile
            };
            return self.edited_named_profile(
                session,
                parameters,
                solution,
                ids,
                &library,
                &transform,
                cross(x_axis, y_axis),
                closed,
            );
        }
        let profile = self.profile_entities()?;
        let wire = if !self.profile_operations.is_empty() {
            let direction = |d: SketchPoint2| add(scale(x_axis, d.x), scale(y_axis, d.y));
            self.edited_profile(
                session,
                parameters,
                solution,
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
        } else if let [Entity::Ellipse(ellipse)] = profile.as_slice()
            && ellipse.arc_ends().is_none()
        {
            if !closed && self.profile_operations.is_empty() {
                return Err(ModelError::new(
                    "open sketch profile cannot be a full ellipse",
                ));
            }
            let direction = |d: SketchPoint2| add(scale(x_axis, d.x), scale(y_axis, d.y));
            self.ellipse_wire(
                session,
                ellipse,
                solution,
                &transform,
                &direction,
                cross(x_axis, y_axis),
            )?
        } else {
            let direction = |d: SketchPoint2| add(scale(x_axis, d.x), scale(y_axis, d.y));
            let segments =
                self.profile_segments(&profile, solution, &transform, &direction, closed)?;
            session.create_curve_wire(&segments, closed)?
        };
        if !session.is_valid(&wire)? {
            return Err(ModelError::new("sketch profile produced an invalid wire"));
        }
        Ok(wire)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn entity_wire<'a>(
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
        let frame = (
            transform(SketchPoint2 { x: 0., y: 0. }),
            direction(SketchPoint2 { x: 1., y: 0. }),
            direction(SketchPoint2 { x: 0., y: 1. }),
        );
        let library = self.curve_library(session, parameters, solution, frame)?;
        let ids: Vec<_> = profile.iter().map(|e| e.id().to_owned()).collect();
        self.edited_named_profile(
            session, parameters, solution, &ids, &library, transform, normal, closed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn edited_named_profile<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        ids: &[String],
        library: &super::editing::CurveLibrary<'a>,
        transform: &impl Fn(SketchPoint2) -> Vec3,
        normal: Vec3,
        closed: bool,
    ) -> Result<Shape<'a>, ModelError> {
        let index = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect::<HashMap<_, _>>();
        let mut wires = ids
            .iter()
            .map(|id| session.duplicate(&library.curves[id]))
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
                SketchProfileOperation::TrimBetween { entity, from, to } => {
                    let i = *index.get(entity.as_str()).ok_or_else(|| {
                        ModelError::new("trim entity must belong to the selected profile")
                    })?;
                    let a = self.bound_point(
                        session,
                        solution,
                        &library.curves,
                        &wires[i],
                        from,
                        transform,
                    )?;
                    let b = self.bound_point(
                        session,
                        solution,
                        &library.curves,
                        &wires[i],
                        to,
                        transform,
                    )?;
                    wires[i] = session
                        .trim_curve_between(&wires[i], a, b)
                        .map_err(|error| {
                            ModelError::from(error).context(&format!("trim of '{entity}'"))
                        })?;
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
                SketchProfileOperation::Fillet { point, radius } => {
                    let source = match joined.take() {
                        Some(s) => s,
                        None => session.join_wires(&wires.iter().collect::<Vec<_>>(), closed)?,
                    };
                    joined = Some(session.fillet_wire_corner(
                        &source,
                        transform(solution.points[point]),
                        scalar(radius, parameters, Dimension::Length)?,
                    )?);
                }
                SketchProfileOperation::Chamfer {
                    point,
                    incoming,
                    outgoing,
                } => {
                    let source = match joined.take() {
                        Some(s) => s,
                        None => session.join_wires(&wires.iter().collect::<Vec<_>>(), closed)?,
                    };
                    joined = Some(session.chamfer_wire_corner(
                        &source,
                        transform(solution.points[point]),
                        scalar(incoming, parameters, Dimension::Length)?,
                        scalar(outgoing, parameters, Dimension::Length)?,
                    )?);
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
        if let std::borrow::Cow::Owned(expanded) = self.with_patterns(Some(parameters))? {
            return expanded.preview_edited_profile(session, parameters, solution, samples, closed);
        }
        if !self.projections.is_empty() {
            return Err(ModelError::new(
                "projected sketch requires source-geometry resolution before preview",
            ));
        }
        if !(2..=256).contains(&samples) {
            return Err(ModelError::new("profile preview samples must be in 2..256"));
        }
        if !self.region_edits.is_empty() || !self.curve_operations.is_empty() {
            let mut local = self.clone();
            local.origin =
                VectorExpr::Literal(VectorQuantity::lengths(0., 0., 0., LengthUnit::Millimeter));
            local.x_axis = VectorExpr::Literal(VectorQuantity::scalars(1., 0., 0.));
            local.y_axis = VectorExpr::Literal(VectorQuantity::scalars(0., 1., 0.));
            local.datum_plane = None;
            local.face_support = None;
            local.texts.clear();
            let shape = if closed {
                local.face_on_plane(session, parameters, None)?
            } else {
                local.open_wire(session, parameters, None)?
            };
            return session
                .subshapes(&shape, ShapeType::Edge)?
                .iter()
                .map(|edge| {
                    Ok(session
                        .edge_sample_points(edge, samples)?
                        .into_iter()
                        .map(|p| SketchPoint2 { x: p.x, y: p.y })
                        .collect())
                })
                .collect();
        }
        if self.profile_operations.is_empty() && self.region_discovery.is_none() {
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
        let wire = if let Some(discovery) = &self.region_discovery {
            self.discovered_regions(
                session,
                parameters,
                solution,
                discovery,
                (
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(0., 1., 0.),
                ),
            )?
        } else {
            self.edited_profile(
                session,
                parameters,
                solution,
                &self.profile_entities()?,
                &transform,
                &transform,
                Vec3::new(0.0, 0.0, 1.0),
                closed,
            )?
        };
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

/// Rejects overlapping faces of a sketch output: candidate pairs come from
/// a sweep over their bounds along x, then a native common must have no
/// area. O(n log n) plus booleans for pairs whose bounds overlap.
pub(super) fn disjoint_faces(
    session: &Session,
    faces: &[Shape<'_>],
    names: &[String],
) -> Result<(), ModelError> {
    let mut bounds = faces
        .iter()
        .enumerate()
        .map(|(i, face)| Ok((i, session.exact_bounds(face)?)))
        .collect::<Result<Vec<_>, ModelError>>()?;
    bounds.sort_by(|a, b| a.1.min.x.total_cmp(&b.1.min.x));
    let gap = 1e-9;
    for (k, (i, a)) in bounds.iter().enumerate() {
        for (j, b) in &bounds[k + 1..] {
            if b.min.x > a.max.x + gap {
                break;
            }
            let apart = b.min.y > a.max.y + gap
                || a.min.y > b.max.y + gap
                || b.min.z > a.max.z + gap
                || a.min.z > b.max.z + gap;
            if apart {
                continue;
            }
            let common = session.common(&faces[*i], &faces[*j])?;
            let area = session.surface_area(&common).unwrap_or(0.0);
            if area > 1e-9 {
                let (first, second) = ((*i).min(*j), (*i).max(*j));
                return Err(ModelError::new(format!(
                    "sketch {} and {} overlap",
                    names[first], names[second]
                )));
            }
        }
    }
    Ok(())
}

/// Installed font files by family and style, resolved once per process.
pub(crate) fn font_file(
    session: &Session,
    family: &str,
    style: SketchFontStyle,
) -> Result<String, ModelError> {
    use std::sync::{Mutex, OnceLock};
    static FILES: OnceLock<Mutex<HashMap<(String, SketchFontStyle), String>>> = OnceLock::new();
    let files = FILES.get_or_init(Default::default);
    let key = (family.to_owned(), style);
    if let Some(file) = files.lock().unwrap().get(&key) {
        return Ok(file.clone());
    }
    let native = match style {
        SketchFontStyle::Regular => occt_bridge::FontStyle::Regular,
        SketchFontStyle::Bold => occt_bridge::FontStyle::Bold,
        SketchFontStyle::Italic => occt_bridge::FontStyle::Italic,
        SketchFontStyle::BoldItalic => occt_bridge::FontStyle::BoldItalic,
    };
    let file = session.font_file(family, native)?;
    files.lock().unwrap().insert(key, file.clone());
    Ok(file)
}
