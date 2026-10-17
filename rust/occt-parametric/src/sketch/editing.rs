//! Named native curve edits and seed-selected material boundary edits.
use super::*;
#[allow(unused_imports)]
pub use occt_parametric_core::sketch::editing::*;

pub(crate) struct CurveLibrary<'a> {
    pub curves: HashMap<String, Shape<'a>>,
    pub active: Vec<String>,
}

/// `shape`, built in sketch-local XY, moved rigidly into the orthonormal
/// sketch `frame` (origin, x axis, y axis): Z turns onto the frame normal,
/// then about it until X lies along the frame's x axis, then it moves to
/// the origin. Identity steps are skipped.
fn place_in_frame<'a>(
    session: &'a Session,
    mut shape: Shape<'a>,
    (origin, x, y): (Vec3, Vec3, Vec3),
) -> Result<Shape<'a>, ModelError> {
    let zero = Vec3::new(0., 0., 0.);
    let z = Vec3::new(0., 0., 1.);
    let normal = cross(x, y);
    let length = |v: Vec3| dot(v, v).sqrt();
    // Rodrigues rotation of `v` about unit `axis` by `angle`.
    let turn = |v: Vec3, axis: Vec3, angle: f64| {
        let (sin, cos) = angle.sin_cos();
        add(
            add(scale(v, cos), scale(cross(axis, v), sin)),
            scale(axis, dot(axis, v) * (1. - cos)),
        )
    };
    let mut local_x = Vec3::new(1., 0., 0.);
    let tilt = cross(z, normal);
    if length(tilt) > 1e-12 {
        let axis = scale(tilt, 1. / length(tilt));
        let angle = length(tilt).atan2(dot(z, normal));
        shape = session.rotate(&shape, zero, axis, angle)?;
        local_x = turn(local_x, axis, angle);
    } else if dot(z, normal) < 0. {
        let axis = Vec3::new(1., 0., 0.);
        shape = session.rotate(&shape, zero, axis, std::f64::consts::PI)?;
        local_x = turn(local_x, axis, std::f64::consts::PI);
    }
    let spin = dot(normal, cross(local_x, x)).atan2(dot(local_x, x));
    if spin.abs() > 1e-15 {
        shape = session.rotate(&shape, zero, normal, spin)?;
    }
    if length(origin) > 0. {
        shape = session.translate(&shape, origin)?;
    }
    Ok(shape)
}

fn distance(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).hypot(a.y - b.y).hypot(a.z - b.z)
}

/// A hint is a selection rule, not a native enumeration index. Ties must
/// fail, otherwise a topology reorder can silently switch the chosen branch.
fn nearest_crossing(
    mut candidates: Vec<Vec3>,
    hint: Vec3,
    context: &str,
) -> Result<Vec3, ModelError> {
    candidates.sort_by(|a, b| distance(*a, hint).total_cmp(&distance(*b, hint)));
    let first = candidates.first().copied().ok_or_else(|| {
        ModelError::new(format!("{context}: lost crossing reference (no crossing)"))
    })?;
    if candidates.iter().skip(1).any(|p| {
        distance(*p, first) > 1e-7 && (distance(*p, hint) - distance(first, hint)).abs() <= 1e-7
    }) {
        return Err(ModelError::new(format!(
            "{context}: ambiguous crossing reference; move the hint point"
        )));
    }
    Ok(first)
}
pub trait SketchDefinitionEditing {
    /// Named active pieces and extended curves in sketch-local XY. Source
    /// entities remain available separately as solver geometry.
    fn edited_curve_previews(
        &self,
        session: &Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        samples: usize,
    ) -> Result<Vec<SketchEditedCurve>, ModelError>;
}

impl SketchDefinitionEditing for SketchDefinition {
    fn edited_curve_previews(
        &self,
        session: &Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        samples: usize,
    ) -> Result<Vec<SketchEditedCurve>, ModelError> {
        if self.curve_operations.is_empty() {
            return Ok(Vec::new());
        }
        if !(2..=256).contains(&samples) {
            return Err(ModelError::new("curve preview samples must be in 2..256"));
        }
        self.validate(parameters)?;
        let library = self.curve_library(
            session,
            parameters,
            solution,
            (
                Vec3::new(0., 0., 0.),
                Vec3::new(1., 0., 0.),
                Vec3::new(0., 1., 0.),
            ),
        )?;
        let mut roots: HashMap<String, String> = HashMap::new();
        for op in &self.curve_operations {
            match op {
                SketchCurveOperation::Split {
                    entity,
                    before,
                    after,
                    ..
                } => {
                    let root = roots.get(entity).cloned().unwrap_or_else(|| entity.clone());
                    roots.insert(before.clone(), root.clone());
                    roots.insert(after.clone(), root);
                }
                SketchCurveOperation::ExtendTo { entity, .. } => {
                    roots
                        .entry(entity.clone())
                        .or_insert_with(|| entity.clone());
                }
            }
        }
        library
            .active
            .iter()
            .filter(|id| roots.contains_key(*id))
            .map(|id| {
                Ok(SketchEditedCurve {
                    id: id.clone(),
                    source: roots[id].clone(),
                    points: session
                        .edge_sample_points(
                            &session.subshape(&library.curves[id], ShapeType::Edge, 0)?,
                            samples,
                        )?
                        .into_iter()
                        .map(|p| SketchPoint2 { x: p.x, y: p.y })
                        .collect(),
                })
            })
            .collect()
    }
}

pub(crate) trait SketchDefinitionEditingInternal {
    fn curve_library<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        frame: (Vec3, Vec3, Vec3),
    ) -> Result<CurveLibrary<'a>, ModelError>;

    fn bound_point(
        &self,
        session: &Session,
        solution: &SketchSolution,
        curves: &HashMap<String, Shape<'_>>,
        wire: &Shape<'_>,
        bound: &SketchTrimBound,
        place: &impl Fn(SketchPoint2) -> Vec3,
    ) -> Result<Vec3, ModelError>;

    fn edited_regions<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        faces: Vec<Shape<'a>>,
        frame: (Vec3, Vec3, Vec3),
    ) -> Result<Vec<Shape<'a>>, ModelError>;
}

impl SketchDefinitionEditingInternal for SketchDefinition {
    fn curve_library<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        frame: (Vec3, Vec3, Vec3),
    ) -> Result<CurveLibrary<'a>, ModelError> {
        if !self.projections.is_empty() {
            return Err(ModelError::new(
                "projected sketch requires source-geometry resolution before native edits",
            ));
        }
        // Native crossings are found in the XY plane, so curve edits are
        // made on sketch-local curves, then placed rigidly in the frame.
        let placed = !self.curve_operations.is_empty();
        let (origin, x, y) = if placed {
            (
                Vec3::new(0., 0., 0.),
                Vec3::new(1., 0., 0.),
                Vec3::new(0., 1., 0.),
            )
        } else {
            frame
        };
        let place = |p: SketchPoint2| add(origin, add(scale(x, p.x), scale(y, p.y)));
        let direction = |p: SketchPoint2| add(scale(x, p.x), scale(y, p.y));
        let mut curves = HashMap::new();
        for (id, entity) in self.entities() {
            curves.insert(
                id.to_owned(),
                self.entity_wire(session, entity, solution, &place, &direction, cross(x, y))?,
            );
        }
        let mut active: Vec<_> = self
            .lines
            .iter()
            .map(|e| e.id.clone())
            .chain(self.circles.iter().map(|e| e.id.clone()))
            .chain(self.arcs.iter().map(|e| e.id.clone()))
            .chain(self.ellipses.iter().map(|e| e.id.clone()))
            .chain(self.splines.iter().map(|e| e.id.clone()))
            .collect();
        for op in &self.curve_operations {
            match op {
                SketchCurveOperation::Split {
                    entity,
                    at,
                    before,
                    after,
                } => {
                    let source = &curves[entity];
                    let (start, end, _, _) = session.curve_end_data(source)?;
                    let cut = self.bound_point(session, solution, &curves, source, at, &place)?;
                    if distance(cut, start) <= 1e-6 || distance(cut, end) <= 1e-6 {
                        return Err(ModelError::new(format!(
                            "split of '{entity}' must be inside its parameter interval"
                        )));
                    }
                    let first = session.trim_curve_between(source, start, cut)?;
                    let second = session.trim_curve_between(source, cut, end)?;
                    curves.insert(before.clone(), first);
                    curves.insert(after.clone(), second);
                    let index = active
                        .iter()
                        .position(|id| id == entity)
                        .ok_or_else(|| ModelError::new("split needs an active curve"))?;
                    active.splice(index..index + 1, [before.clone(), after.clone()]);
                }
                SketchCurveOperation::ExtendTo {
                    entity,
                    end,
                    target,
                    limit,
                    near,
                } => {
                    let source = &curves[entity];
                    if session.wire_is_closed(source)? {
                        return Err(ModelError::new("extend-to needs an open curve"));
                    }
                    let (a, b, _, _) = session.curve_end_data(source)?;
                    let at_start = *end == SketchCurveEnd::Start;
                    let limit = scalar(limit, parameters, Dimension::Length)?;
                    let extended = session.extend_curve(
                        source,
                        if at_start { limit } else { 0. },
                        if at_start { 0. } else { limit },
                    )?;
                    let hint = near.as_ref().map_or(if at_start { a } else { b }, |id| {
                        place(solution.points[id])
                    });
                    let mut candidates = Vec::new();
                    for p in session.planar_curve_intersections(&extended, &curves[target], 1000)? {
                        if distance(p, session.curve_closest_point(source, p)?) > 1e-6 {
                            candidates.push(p);
                        }
                    }
                    let hit = nearest_crossing(
                        candidates,
                        hint,
                        &format!("extend-to of '{entity}' beyond that end within its search limit"),
                    )?;
                    let result = session.trim_curve_between(
                        &extended,
                        if at_start { hit } else { a },
                        if at_start { b } else { hit },
                    )?;
                    curves.insert(entity.clone(), result);
                }
            }
        }
        if placed {
            curves = curves
                .into_iter()
                .map(|(id, curve)| Ok((id, place_in_frame(session, curve, frame)?)))
                .collect::<Result<_, ModelError>>()?;
        }
        Ok(CurveLibrary { curves, active })
    }

    #[allow(clippy::too_many_arguments)]
    fn bound_point(
        &self,
        session: &Session,
        solution: &SketchSolution,
        curves: &HashMap<String, Shape<'_>>,
        wire: &Shape<'_>,
        bound: &SketchTrimBound,
        place: &impl Fn(SketchPoint2) -> Vec3,
    ) -> Result<Vec3, ModelError> {
        let (start, end, _, _) = session.curve_end_data(wire)?;
        Ok(match bound {
            SketchTrimBound::Start => start,
            SketchTrimBound::End => end,
            SketchTrimBound::Point { point } => place(solution.points[point]),
            SketchTrimBound::Crossing { curve, near } => {
                let hint = place(solution.points[near]);
                nearest_crossing(
                    session.planar_curve_intersections(wire, &curves[curve], 1000)?,
                    hint,
                    &format!("trim/split curve and '{curve}' do not cross unambiguously"),
                )?
            }
        })
    }

    fn edited_regions<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        faces: Vec<Shape<'a>>,
        frame: (Vec3, Vec3, Vec3),
    ) -> Result<Vec<Shape<'a>>, ModelError> {
        let (origin, x, y) = frame;
        let place = |p: SketchPoint2| add(origin, add(scale(x, p.x), scale(y, p.y)));
        let mut faces = faces;
        for edit in &self.region_edits {
            let seed = place(solution.points[&edit.seed]);
            let mut target = None;
            for (i, face) in faces.iter().enumerate() {
                match session.planar_face_point_state(face, seed)? {
                    occt_bridge::FacePointState::Boundary => {
                        return Err(ModelError::new(format!(
                            "region edit seed '{}' lies on a boundary",
                            edit.seed
                        )));
                    }
                    occt_bridge::FacePointState::Inside if target.replace(i).is_some() => {
                        return Err(ModelError::new(
                            "region edit seed selects overlapping regions",
                        ));
                    }
                    _ => {}
                }
            }
            let index = target.ok_or_else(|| {
                ModelError::new(format!(
                    "region edit seed '{}' is outside the material",
                    edit.seed
                ))
            })?;
            let count = session.subshape_count(&faces[index], ShapeType::Wire)?;
            let mut wires = (0..count)
                .map(|i| session.face_boundary(&faces[index], i))
                .collect::<Result<Vec<_>, _>>()?;
            for operation in &edit.operations {
                match operation {
                    SketchProfileOperation::Offset { distance, join } => {
                        let d = scalar(distance, parameters, Dimension::Length)?;
                        for (i, wire) in wires.iter_mut().enumerate() {
                            *wire = session.offset_wire(
                                wire,
                                cross(x, y),
                                if i == 0 { d } else { -d },
                                *join == SketchOffsetJoin::Intersection,
                            )?;
                            if !session.wire_is_closed(wire)? {
                                return Err(ModelError::new(
                                    "region offset must retain closed boundaries",
                                ));
                            }
                        }
                    }
                    SketchProfileOperation::Fillet { point, .. }
                    | SketchProfileOperation::Chamfer { point, .. } => {
                        let corner = place(solution.points[point]);
                        let mut found = None;
                        for (i, wire) in wires.iter().enumerate() {
                            let mut has_corner = false;
                            for edge in session.subshapes(wire, ShapeType::Edge)? {
                                let (a, b, _, _) = session.curve_end_data(&edge)?;
                                has_corner |=
                                    distance(a, corner) <= 1e-6 || distance(b, corner) <= 1e-6;
                            }
                            if has_corner && found.replace(i).is_some() {
                                return Err(ModelError::new(
                                    "region corner belongs to multiple boundaries",
                                ));
                            }
                        }
                        let i = found.ok_or_else(|| {
                            ModelError::new(format!(
                                "region corner '{point}' is not a remaining boundary vertex"
                            ))
                        })?;
                        wires[i] = match operation {
                            SketchProfileOperation::Fillet { radius, .. } => session
                                .fillet_wire_corner(
                                    &wires[i],
                                    corner,
                                    scalar(radius, parameters, Dimension::Length)?,
                                )?,
                            SketchProfileOperation::Chamfer {
                                incoming, outgoing, ..
                            } => session.chamfer_wire_corner(
                                &wires[i],
                                corner,
                                scalar(incoming, parameters, Dimension::Length)?,
                                scalar(outgoing, parameters, Dimension::Length)?,
                            )?,
                            _ => unreachable!(),
                        };
                    }
                    _ => {
                        return Err(ModelError::new(
                            "region edits support fillet, chamfer and offset only",
                        ));
                    }
                }
            }
            faces[index] = if wires.len() == 1 {
                session.create_face_from_wire(&wires[0])?
            } else {
                session.create_face_with_holes(&wires[0], &wires[1..].iter().collect::<Vec<_>>())?
            };
            if !session.is_valid(&faces[index])? {
                return Err(ModelError::new("region edit produced an invalid face"));
            }
        }
        let names: Vec<_> = (0..faces.len())
            .map(|i| format!("edited region {i}"))
            .collect();
        super::profile::disjoint_faces(session, &faces, &names)?;
        Ok(faces)
    }
}
