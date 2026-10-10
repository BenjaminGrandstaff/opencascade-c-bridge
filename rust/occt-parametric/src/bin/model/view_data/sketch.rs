//! Annotated sketch scenes with solved geometry and constraint diagnostics.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn sketch_scene(
    session: &Session,
    instance: &str,
    feature: &str,
    sketch: &SketchDefinition,
    closed: bool,
    parameters: &HashMap<String, ParameterValue>,
    controls: &BTreeMap<String, Value>,
    support: Option<Value>,
    projections: Vec<Value>,
    budget: &mut Budget,
) -> Result<Value, Failure> {
    let solution = sketch.solve(parameters).stage("sketch")?;
    let checks = sketch
        .constraint_checks(parameters, &solution)
        .stage("sketch")?;
    let curves = sketch
        .preview_curves(session, &solution, 32)
        .stage("sketch")?;
    let mut entities = curves
        .into_iter()
        .map(|(id, points)| {
            let point_ids = if let Some(e) = sketch.lines.iter().find(|e| e.id == id) {
                vec![e.start.clone(), e.end.clone()]
            } else if let Some(e) = sketch.arcs.iter().find(|e| e.id == id) {
                vec![e.center.clone(), e.start.clone(), e.end.clone()]
            } else if let Some(e) = sketch.circles.iter().find(|e| e.id == id) {
                vec![e.center.clone(), e.rim.clone()]
            } else if let Some(e) = sketch.ellipses.iter().find(|e| e.id == id) {
                vec![e.center.clone(), e.major.clone(), e.minor.clone()]
            } else {
                sketch
                    .splines
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.points.clone())
                    .unwrap_or_default()
            };
            json!({
                "id": id,
                "points": points.iter().map(|p|[p.x,p.y,0.0]).collect::<Vec<_>>(),
                "point_ids": point_ids,
            })
        })
        .collect::<Vec<_>>();
    let external_ids: BTreeSet<_> = projections
        .iter()
        .filter_map(|p| p["definition"]["id"].as_str())
        .collect();
    for entity in &mut entities {
        if external_ids.contains(entity["id"].as_str().unwrap_or("")) {
            entity["external"] = json!(true);
        }
    }
    let point_map = solution
        .points
        .iter()
        .map(|(id, p)| (id.clone(), [p.x, p.y, 0.0]))
        .collect::<BTreeMap<_, _>>();
    // Index circular references once: O(points + curves), plus referenced
    // expression names. Equal-radius annotations do not scan all entities.
    let circular_refs: HashMap<_, _> = sketch
        .circles
        .iter()
        .map(|c| (c.id.as_str(), (c.center.as_str(), c.rim.as_str())))
        .chain(
            sketch
                .arcs
                .iter()
                .map(|a| (a.id.as_str(), (a.center.as_str(), a.start.as_str()))),
        )
        .collect();
    let arc_ends: HashMap<_, _> = sketch
        .arcs
        .iter()
        .map(|a| (a.id.as_str(), a.end.as_str()))
        .collect();
    let centers: HashMap<_, _> = sketch
        .circles
        .iter()
        .map(|c| (c.id.as_str(), c.center.as_str()))
        .chain(
            sketch
                .arcs
                .iter()
                .map(|a| (a.id.as_str(), a.center.as_str())),
        )
        .chain(
            sketch
                .ellipses
                .iter()
                .map(|e| (e.id.as_str(), e.center.as_str())),
        )
        .collect();
    let lines: HashMap<_, _> = sketch.lines.iter().map(|l| (l.id.as_str(), l)).collect();
    let point_controls: HashMap<_, _> = sketch
        .points
        .iter()
        .map(|p| (p.id.as_str(), names(&json!([p.x, p.y]))))
        .collect();
    let mut annotations: Vec<Value> = Vec::new();
    if let Some(support) = &support {
        annotations.push(
            Annotation {
                id: "sketch-face-support".into(),
                label: "Face-attached sketch".into(),
                kind: AnnotationKind::Group,
                status: if support["status"] == "resolved" {
                    AnnotationStatus::Constructed
                } else {
                    AnnotationStatus::Unverified
                },
                targets: vec![],
                parameters: names(&support["definition"]),
                anchors: json!([]),
                detail: support.clone(),
            }
            .into(),
        );
    }
    for projection in &projections {
        let id = projection["definition"]["id"]
            .as_str()
            .expect("projection ID");
        let anchor = point_map.get(&format!("{id}:center")).copied().or_else(|| {
            let a = point_map.get(&format!("{id}:start"))?;
            let b = point_map.get(&format!("{id}:end"))?;
            Some([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, 0.0])
        });
        annotations.push(
            Annotation {
                id: format!("projection-{id}"),
                label: format!("Projected {id}"),
                kind: AnnotationKind::Group,
                status: AnnotationStatus::Constructed,
                targets: vec![id.into()],
                parameters: projection["source_parameters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|n| n.as_str().map(str::to_owned))
                    .collect(),
                anchors: json!(anchor.into_iter().collect::<Vec<_>>()),
                detail: projection.clone(),
            }
            .into(),
        );
    }
    for (index, constraint) in sketch.constraints.iter().enumerate() {
        let (symbol, targets, anchor_ids, value) = match constraint {
            SketchConstraint::Angle {
                first,
                second,
                value,
            } => (
                "∠",
                vec![first.clone(), second.clone()],
                vec![],
                Some(value),
            ),
            SketchConstraint::Radius { curve, value }
            | SketchConstraint::Diameter { curve, value } => {
                let (center, rim) = circular_refs[curve.as_str()];
                let refs = vec![center.to_owned(), rim.to_owned()];
                (
                    if matches!(constraint, SketchConstraint::Radius { .. }) {
                        "R"
                    } else {
                        "Ø"
                    },
                    vec![curve.clone()],
                    refs,
                    Some(value),
                )
            }
            SketchConstraint::Symmetric {
                first,
                second,
                axis,
            } => (
                "SYM",
                vec![first.clone(), second.clone(), axis.clone()],
                vec![first.clone(), second.clone()],
                None,
            ),
            SketchConstraint::PointOnCurve { point, curve } => (
                "ON",
                vec![point.clone(), curve.clone()],
                vec![point.clone()],
                None,
            ),
            SketchConstraint::Horizontal { line } => ("H", vec![line.clone()], vec![], None),
            SketchConstraint::Vertical { line } => ("V", vec![line.clone()], vec![], None),
            SketchConstraint::Parallel { first, second } => {
                ("∥", vec![first.clone(), second.clone()], vec![], None)
            }
            SketchConstraint::Perpendicular { first, second } => {
                ("⊥", vec![first.clone(), second.clone()], vec![], None)
            }
            SketchConstraint::EqualRadius { first, second } => (
                "=R",
                vec![first.clone(), second.clone()],
                vec![
                    circular_refs[first.as_str()].0.to_owned(),
                    circular_refs[second.as_str()].0.to_owned(),
                ],
                None,
            ),
            SketchConstraint::Concentric { first, second } => (
                "CONC",
                vec![first.clone(), second.clone()],
                vec![
                    centers[first.as_str()].to_owned(),
                    centers[second.as_str()].to_owned(),
                ],
                None,
            ),
            SketchConstraint::Midpoint { point, line } => (
                "MID",
                vec![point.clone(), line.clone()],
                vec![point.clone()],
                None,
            ),
            SketchConstraint::PointLineDistance { point, line, value } => (
                "⊥",
                vec![point.clone(), line.clone()],
                vec![point.clone()],
                Some(value),
            ),
            SketchConstraint::LineCircleTangent { line, circle, side } => (
                match side {
                    SketchLineSide::Left => "T LEFT",
                    SketchLineSide::Right => "T RIGHT",
                },
                vec![line.clone(), circle.clone()],
                vec![circular_refs[circle.as_str()].0.to_owned()],
                None,
            ),
            SketchConstraint::CircleCircleTangent {
                first,
                second,
                mode,
            } => (
                match mode {
                    SketchCircleTangency::External => "T EXT",
                    SketchCircleTangency::Internal => "T INT",
                },
                vec![first.clone(), second.clone()],
                vec![
                    circular_refs[first.as_str()].0.to_owned(),
                    circular_refs[second.as_str()].0.to_owned(),
                ],
                None,
            ),
            SketchConstraint::EqualLength { first, second } => {
                ("=", vec![first.clone(), second.clone()], vec![], None)
            }
            SketchConstraint::Coincident { first, second } => (
                "≡",
                vec![first.clone(), second.clone()],
                vec![first.clone(), second.clone()],
                None,
            ),
            SketchConstraint::Tangent {
                first,
                second,
                point,
            } => (
                "T",
                vec![first.clone(), second.clone(), point.clone()],
                vec![point.clone()],
                None,
            ),
            SketchConstraint::Distance {
                first,
                second,
                value,
            } => (
                "distance",
                vec![first.clone(), second.clone()],
                vec![first.clone(), second.clone()],
                Some(value),
            ),
        };
        let mut anchors = if !anchor_ids.is_empty() {
            anchor_ids
                .iter()
                .map(|id| point_map[id])
                .collect::<Vec<_>>()
        } else {
            targets
                .iter()
                .filter_map(|id| sketch.lines.iter().find(|l| l.id == *id))
                .map(|l| {
                    let a = point_map[&l.start];
                    let b = point_map[&l.end];
                    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, 0.0]
                })
                .collect()
        };
        if let SketchConstraint::Midpoint { line, .. } = constraint {
            let l = lines[line.as_str()];
            let a = point_map[&l.start];
            let b = point_map[&l.end];
            anchors.push([a[0] * 0.5 + b[0] * 0.5, a[1] * 0.5 + b[1] * 0.5, 0.0]);
        }
        if let SketchConstraint::PointLineDistance { line, .. } = constraint {
            let l = lines[line.as_str()];
            let a = point_map[&l.start];
            let b = point_map[&l.end];
            let p = anchors[0];
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            let length = dx.hypot(dy);
            if length <= f64::EPSILON {
                return Err(failure(
                    "sketch",
                    "point-line distance has a zero length line",
                ));
            }
            let ux = dx / length;
            let uy = dy / length;
            let t = (p[0] - a[0]) * ux + (p[1] - a[1]) * uy;
            anchors = vec![[a[0] + t * ux, a[1] + t * uy, 0.0], p];
        }
        match constraint {
            SketchConstraint::LineCircleTangent { line, circle, side } => {
                let l = lines[line.as_str()];
                let a = point_map[&l.start];
                let b = point_map[&l.end];
                let (c, r) = circular_refs[circle.as_str()];
                let c = point_map[c];
                let r = point_map[r];
                let radius = (r[0] - c[0]).hypot(r[1] - c[1]);
                let length = (b[0] - a[0]).hypot(b[1] - a[1]);
                let ux = (b[0] - a[0]) / length;
                let uy = (b[1] - a[1]) / length;
                let t = (c[0] - a[0]) * ux + (c[1] - a[1]) * uy;
                let sign = match side {
                    SketchLineSide::Left => 1.,
                    SketchLineSide::Right => -1.,
                };
                anchors = vec![
                    [a[0] + t * ux, a[1] + t * uy, 0.0],
                    [c[0] + sign * radius * uy, c[1] - sign * radius * ux, 0.0],
                ];
            }
            SketchConstraint::CircleCircleTangent {
                first,
                second,
                mode,
            } => {
                let (a, ar) = circular_refs[first.as_str()];
                let (b, br) = circular_refs[second.as_str()];
                let a = point_map[a];
                let ar = point_map[ar];
                let b = point_map[b];
                let br = point_map[br];
                let ra = (ar[0] - a[0]).hypot(ar[1] - a[1]);
                let rb = (br[0] - b[0]).hypot(br[1] - b[1]);
                let distance = (b[0] - a[0]).hypot(b[1] - a[1]);
                let ux = (b[0] - a[0]) / distance;
                let uy = (b[1] - a[1]) / distance;
                let sign = match mode {
                    SketchCircleTangency::External => -1.,
                    SketchCircleTangency::Internal => 1.,
                };
                anchors = vec![
                    [a[0] + ra * ux, a[1] + ra * uy, 0.0],
                    [b[0] + sign * rb * ux, b[1] + sign * rb * uy, 0.0],
                ];
            }
            _ => {}
        }
        if matches!(constraint, SketchConstraint::Diameter { .. }) && anchors.len() == 2 {
            let c = anchors[0];
            let r = anchors[1];
            anchors = vec![[2.0 * c[0] - r[0], 2.0 * c[1] - r[1], 0.0], r];
        }
        let angular_arc = if let SketchConstraint::Angle { first, second, .. } = constraint {
            let a = sketch
                .lines
                .iter()
                .find(|l| l.id == *first)
                .expect("validated angle line");
            let b = sketch
                .lines
                .iter()
                .find(|l| l.id == *second)
                .expect("validated angle line");
            let o = point_map[&a.start];
            let u = point_map[&a.end];
            let p = point_map[&b.start];
            let q = point_map[&b.end];
            let theta = (u[1] - o[1]).atan2(u[0] - o[0]);
            let end = (q[1] - p[1]).atan2(q[0] - p[0]);
            let delta = end - theta;
            let sweep = delta.sin().atan2(delta.cos());
            let radius = 0.25
                * (u[0] - o[0])
                    .hypot(u[1] - o[1])
                    .min((q[0] - p[0]).hypot(q[1] - p[1]));
            (0..=16)
                .map(|i| {
                    let t = theta + sweep * i as f64 / 16.0;
                    [o[0] + radius * t.cos(), o[1] + radius * t.sin(), 0.0]
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let check = &checks[index];
        let status = if check.by_construction {
            AnnotationStatus::Constructed
        } else if check.satisfied {
            AnnotationStatus::Passed
        } else {
            AnnotationStatus::Failed
        };
        let label = if let Some(expression) = value {
            let target = expression.evaluate(parameters).stage("sketch")?.value;
            if matches!(constraint, SketchConstraint::Angle { .. }) {
                format!("∠ {} rad", length_label(target))
            } else if symbol == "distance" {
                format!("{} mm", length_label(target))
            } else {
                format!("{symbol} {} mm", length_label(target))
            }
        } else {
            symbol.to_owned()
        };
        let mut control_names: Vec<String> = value
            .map(|v| v.parameter_names().into_iter().map(str::to_owned).collect())
            .unwrap_or_default();
        if let SketchConstraint::EqualRadius { first, second } = constraint {
            let mut linked = BTreeSet::new();
            for curve in [first, second] {
                let (center, rim) = circular_refs[curve.as_str()];
                linked.extend(point_controls[center].iter().cloned());
                linked.extend(point_controls[rim].iter().cloned());
            }
            control_names = linked.into_iter().collect();
        }
        match constraint {
            SketchConstraint::Concentric { first, second } => {
                control_names = [first, second]
                    .into_iter()
                    .flat_map(|id| point_controls[centers[id.as_str()]].iter().cloned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
            }
            SketchConstraint::Midpoint { point, line } => {
                let l = lines[line.as_str()];
                control_names = [point.as_str(), l.start.as_str(), l.end.as_str()]
                    .into_iter()
                    .flat_map(|id| point_controls[id].iter().cloned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
            }
            _ => {}
        }
        match constraint {
            SketchConstraint::LineCircleTangent { line, circle, .. } => {
                let l = lines[line.as_str()];
                let (c, r) = circular_refs[circle.as_str()];
                control_names = [l.start.as_str(), l.end.as_str(), c, r]
                    .into_iter()
                    .flat_map(|id| point_controls[id].iter().cloned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
            }
            SketchConstraint::CircleCircleTangent { first, second, .. } => {
                let (a, b) = circular_refs[first.as_str()];
                let (c, d) = circular_refs[second.as_str()];
                control_names = [a, b, c, d]
                    .into_iter()
                    .flat_map(|id| point_controls[id].iter().cloned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
            }
            _ => {}
        }
        let arc_targets: Vec<&str> = match constraint {
            SketchConstraint::LineCircleTangent { circle, .. } => vec![circle.as_str()],
            SketchConstraint::CircleCircleTangent { first, second, .. } => {
                vec![first.as_str(), second.as_str()]
            }
            _ => vec![],
        };
        for id in arc_targets {
            if let Some(end) = arc_ends.get(id) {
                control_names.extend(point_controls[end].iter().cloned());
            }
        }
        control_names.sort();
        control_names.dedup();
        let kind = if value.is_some() {
            AnnotationKind::Dimension
        } else {
            AnnotationKind::Constraint
        };
        let residual_unit = match constraint {
            SketchConstraint::Angle { .. } => "rad",
            SketchConstraint::Parallel { .. }
            | SketchConstraint::Perpendicular { .. }
            | SketchConstraint::Tangent { .. } => "dimensionless",
            _ => "mm",
        };
        annotations.push(
            Annotation {
                id: format!("constraint-{index}"),
                label,
                kind,
                status,
                targets,
                parameters: control_names,
                anchors: json!(anchors),
                detail: json!({
                    "constraint_index": index,
                    "constraint": constraint,
                    "angular_arc": angular_arc,
                    "max_residual": check.max_residual,
                    "by_construction": check.by_construction,
                    "tolerance": 1e-9,
                    "residual_unit": residual_unit,
                }),
            }
            .into(),
        );
    }
    for p in &sketch.points {
        let control_names =
            p.x.parameter_names()
                .into_iter()
                .chain(p.y.parameter_names())
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
        if p.fixed {
            annotations.push(
                Annotation {
                    id: format!("fixed-{}", p.id),
                    label: format!("fixed {}", p.id),
                    kind: AnnotationKind::Constraint,
                    status: AnnotationStatus::Fixed,
                    targets: vec![p.id.clone()],
                    parameters: control_names.clone(),
                    anchors: json!([point_map[&p.id]]),
                    detail: json!({
                        "point": p.id,
                        "fixed": true,
                    }),
                }
                .into(),
            );
        }
        let (status, coordinate_role) = if p.fixed {
            (AnnotationStatus::Driving, "fixed driving coordinate")
        } else {
            (
                AnnotationStatus::Initial,
                "initial guess; constraints may move this point",
            )
        };
        for name in control_names {
            annotations.push(
                Annotation {
                    id: format!("parameter-{}-{name}", p.id),
                    label: name.clone(),
                    kind: AnnotationKind::Parameter,
                    status,
                    targets: vec![p.id.clone()],
                    parameters: vec![name.clone()],
                    anchors: json!([point_map[&p.id]]),
                    detail: json!({
                        "point": p.id,
                        "parameter": name,
                        "control": controls.get(&name),
                        "coordinate_role": coordinate_role,
                    }),
                }
                .into(),
            );
        }
    }
    let (edited_profile, profile_error) =
        match sketch.preview_edited_profile(session, parameters, &solution, 32, closed) {
            Ok(lines) => (
                lines
                    .into_iter()
                    .map(|line| {
                        line.into_iter()
                            .map(|p| [p.x, p.y, 0.0])
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>(),
                None,
            ),
            Err(error) => (Vec::new(), Some(error.message)),
        };
    for (i, operation) in sketch.profile_operations.iter().enumerate() {
        let detail = serde_json::to_value(operation).stage("sketch")?;
        let control_names = names(&detail);
        let (label, targets) = match operation {
            SketchProfileOperation::Trim { entity, .. } => {
                (format!("Trim {entity}"), vec![entity.clone()])
            }
            SketchProfileOperation::Extend { entity, .. } => {
                (format!("Extend {entity}"), vec![entity.clone()])
            }
            SketchProfileOperation::Offset { distance, .. } => (
                format!(
                    "Offset {} mm",
                    length_label(distance.evaluate(parameters).stage("sketch")?.value)
                ),
                vec![],
            ),
        };
        annotations.push(
            Annotation {
                id: format!("profile-operation-{i}"),
                label,
                kind: AnnotationKind::ProfileOperation,
                status: if profile_error.is_some() {
                    AnnotationStatus::Unverified
                } else {
                    AnnotationStatus::Driving
                },
                targets,
                parameters: control_names,
                anchors: json!([]),
                detail: json!({
                    "operation": detail,
                    "error": profile_error,
                    "description": "Derived profile edit; source \
                        constraints remain on their original entities",
                }),
            }
            .into(),
        );
    }
    let vertices = entities
        .iter()
        .map(|e| e["points"].as_array().map_or(0, Vec::len))
        .sum::<usize>()
        + annotations
            .iter()
            .map(|a| a["detail"]["angular_arc"].as_array().map_or(0, Vec::len))
            .sum::<usize>()
        + point_map.len()
        + edited_profile.iter().map(Vec::len).sum::<usize>();
    check_budget(&mut budget.vertices, vertices, "vertex")?;
    check_budget(&mut budget.annotations, annotations.len(), "annotation")?;
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in point_map.values() {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    for line in &edited_profile {
        for p in line {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
    }
    for e in &entities {
        for p in e["points"].as_array().into_iter().flatten() {
            for i in 0..3 {
                let v = p[i].as_f64().unwrap();
                min[i] = min[i].min(v);
                max[i] = max[i].max(v);
            }
        }
    }
    Ok(json!({
        "kind": "sketch",
        "instance": instance,
        "feature": feature,
        "title": format!("{instance}/{feature} — {}",sketch.id),
        "sketch": sketch.id,
        "face_support": support,
        "projections": projections,
        "bounds": [min,max],
        "entities": entities,
        "edited_profile": edited_profile,
        "profile_error": profile_error,
        "profile_operations": sketch.profile_operations,
        "points": point_map,
        "parameters": controls,
        "annotations": annotations,
        "solver": {
            "solved": solution.solved,
            "iterations": solution.iterations,
            "max_residual": solution.max_residual,
            "free_degrees": solution.free_degrees,
            "redundant_equations": solution.redundant_equations,
        },
        "coordinate_system": "sketch-local XY millimeters",
    }))
}
