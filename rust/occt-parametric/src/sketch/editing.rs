//! Named native curve edits and seed-selected material boundary edits.
use super::*;

pub(super) struct CurveLibrary<'a> {
    pub curves: HashMap<String, Shape<'a>>,
    pub active: Vec<String>,
}

fn distance(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).hypot(a.y - b.y).hypot(a.z - b.z)
}

impl SketchDefinition {
    /// Named active pieces and extended curves in sketch-local XY. Source
    /// entities remain available separately as solver geometry.
    pub fn edited_curve_previews(
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
    pub(super) fn validate_edit_references<'a>(
        &'a self,
        originals: &HashSet<&'a str>,
        points: &HashSet<&str>,
    ) -> Result<HashSet<&'a str>, ModelError> {
        if self.curve_operations.len() > 1024
            || self.region_edits.len() > 1000
            || self
                .region_edits
                .iter()
                .map(|edit| edit.operations.len())
                .sum::<usize>()
                > 1024
        {
            return Err(ModelError::new(
                "sketch edits exceed 1024 operations or 1000 region selections",
            ));
        }
        let mut known = originals.clone();
        let mut active = originals.clone();
        for operation in &self.curve_operations {
            match operation {
                SketchCurveOperation::Split {
                    entity,
                    at,
                    before,
                    after,
                } => {
                    if !active.remove(entity.as_str()) {
                        return Err(ModelError::new("split needs an active known curve"));
                    }
                    match at {
                        SketchTrimBound::Point { point } if points.contains(point.as_str()) => {}
                        SketchTrimBound::Crossing { curve, near }
                            if known.contains(curve.as_str())
                                && curve != entity
                                && points.contains(near.as_str()) => {}
                        _ => {
                            return Err(ModelError::new(
                                "split needs an interior known point or another known curve and a hint point",
                            ));
                        }
                    }
                    for id in [before, after] {
                        if id.is_empty() || !known.insert(id.as_str()) {
                            return Err(ModelError::new(
                                "split piece names must be nonempty, unique and unused",
                            ));
                        }
                        active.insert(id.as_str());
                    }
                }
                SketchCurveOperation::ExtendTo {
                    entity,
                    target,
                    near,
                    ..
                } => {
                    if !active.contains(entity.as_str())
                        || !known.contains(target.as_str())
                        || entity == target
                        || near
                            .as_ref()
                            .is_some_and(|id| !points.contains(id.as_str()))
                    {
                        return Err(ModelError::new(
                            "extend-to needs an active curve, a different known target and a known optional hint",
                        ));
                    }
                }
            }
        }
        for edit in &self.region_edits {
            if !points.contains(edit.seed.as_str()) || edit.operations.is_empty() {
                return Err(ModelError::new(
                    "region edit needs a known seed point and nonempty operations",
                ));
            }
            let mut offset = false;
            let mut corners = HashSet::new();
            for operation in &edit.operations {
                match operation {
                    SketchProfileOperation::Offset { .. } => offset = true,
                    SketchProfileOperation::Fillet { point, .. }
                    | SketchProfileOperation::Chamfer { point, .. } => {
                        if offset || !points.contains(point.as_str()) || !corners.insert(point) {
                            return Err(ModelError::new(
                                "region corner edits need unique known points and must precede offsets",
                            ));
                        }
                    }
                    _ => {
                        return Err(ModelError::new(
                            "region edits support fillet, chamfer and offset only",
                        ));
                    }
                }
            }
        }
        Ok(known)
    }
    pub(super) fn curve_library<'a>(
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
        let (origin, x, y) = frame;
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
                    let hit=candidates.into_iter().min_by(|a,b|distance(*a,hint).total_cmp(&distance(*b,hint)))
                        .ok_or_else(||ModelError::new(format!("extend-to of '{entity}' finds no crossing beyond that end within its search limit")))?;
                    let result = session.trim_curve_between(
                        &extended,
                        if at_start { hit } else { a },
                        if at_start { b } else { hit },
                    )?;
                    curves.insert(entity.clone(), result);
                }
            }
        }
        Ok(CurveLibrary { curves, active })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn bound_point(
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
                session
                    .planar_curve_intersections(wire, &curves[curve], 1000)?
                    .into_iter()
                    .min_by(|a, b| distance(*a, hint).total_cmp(&distance(*b, hint)))
                    .ok_or_else(|| {
                        ModelError::new(format!("trim/split curve and '{curve}' do not cross"))
                    })?
            }
        })
    }

    pub(super) fn edited_regions<'a>(
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
