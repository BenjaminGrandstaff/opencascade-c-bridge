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
    let shapes = sketch.shapes.clone();
    let expanded = sketch.with_pattern_copies(parameters).stage("sketch")?;
    let sketch = expanded.as_ref();
    let solution = sketch.solve(parameters).stage("sketch")?;
    let acceptance_error = sketch
        .check_acceptance(&solution)
        .err()
        .map(|e| e.to_string());
    let checks = sketch
        .constraint_checks(parameters, &solution)
        .stage("sketch")?;
    let ellipse_pair_contacts = sketch
        .ellipse_ellipse_tangent_contacts(parameters, &solution)
        .stage("sketch")?;
    let spline_pair_contacts = sketch
        .spline_spline_tangent_contacts(parameters, &solution)
        .stage("sketch")?;
    let spline_ellipse_contacts = sketch
        .spline_ellipse_tangent_contacts(parameters, &solution)
        .stage("sketch")?;
    let ellipse_circle_contacts = sketch
        .ellipse_circle_tangent_contacts(parameters, &solution)
        .stage("sketch")?;
    let spline_circle_contacts = sketch
        .spline_circle_tangent_contacts(parameters, &solution)
        .stage("sketch")?;
    let spline_contacts = sketch
        .line_spline_tangent_contacts(parameters, &solution)
        .stage("sketch")?;
    let ellipse_contacts = sketch
        .line_ellipse_tangent_contacts(&solution)
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
    let explicit: HashMap<_, _> = sketch
        .splines
        .iter()
        .filter_map(|s| s.basis.as_ref().map(|b| (s.id.as_str(), (s, b))))
        .collect();
    for entity in &mut entities {
        if let Some((s, b)) = explicit.get(entity["id"].as_str().unwrap()) {
            entity["bspline"] = json!({"basis":b,"control_points":s.points,"endpoints":s.endpoints,"evaluated_weights":solution.spline_weights[&s.id]});
        }
    }
    for entity in &mut entities {
        if let Some((s, _)) = explicit.get(entity["id"].as_str().unwrap())
            && let Some(e) = &s.endpoints
        {
            entity["point_ids"]
                .as_array_mut()
                .unwrap()
                .extend([json!(e.start), json!(e.end)]);
        }
    }
    let native_spline_ends: HashMap<_, _> = entities
        .iter()
        .filter(|e| !e["bspline"].is_null())
        .map(|e| {
            let samples = e["points"].as_array().unwrap();
            let point = |p: &Value| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap(), 0.];
            (
                e["id"].as_str().unwrap().to_owned(),
                [point(&samples[0]), point(samples.last().unwrap())],
            )
        })
        .collect();
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
    let spline_defs: HashMap<_, _> = sketch.splines.iter().map(|s| (s.id.as_str(), s)).collect();
    let lines: HashMap<_, _> = sketch.lines.iter().map(|l| (l.id.as_str(), l)).collect();
    let ellipses: HashMap<_, _> = sketch.ellipses.iter().map(|e| (e.id.as_str(), e)).collect();
    let point_controls: HashMap<_, _> = sketch
        .points
        .iter()
        .map(|p| (p.id.as_str(), names(&json!([p.x, p.y]))))
        .collect();
    // Free ellipse axes can be driven by distance and angle dimensions rather
    // than coordinate expressions. Index those direct controls once.
    let mut dimension_point_controls: HashMap<&str, BTreeSet<String>> = HashMap::new();
    for constraint in &sketch.constraints {
        let (ids, value) = match constraint {
            SketchConstraint::Radius { curve, value }
            | SketchConstraint::Diameter { curve, value } => {
                let (c, r) = circular_refs[curve.as_str()];
                (vec![c, r], value)
            }
            SketchConstraint::PointLineDistance { point, line, value } => {
                let l = lines[line.as_str()];
                (
                    vec![point.as_str(), l.start.as_str(), l.end.as_str()],
                    value,
                )
            }
            SketchConstraint::Distance {
                first,
                second,
                value,
            }
            | SketchConstraint::HorizontalDistance {
                first,
                second,
                value,
            }
            | SketchConstraint::VerticalDistance {
                first,
                second,
                value,
            } => (vec![first.as_str(), second.as_str()], value),
            SketchConstraint::Angle {
                first,
                second,
                value,
            } => {
                let a = lines[first.as_str()];
                let b = lines[second.as_str()];
                (
                    vec![
                        a.start.as_str(),
                        a.end.as_str(),
                        b.start.as_str(),
                        b.end.as_str(),
                    ],
                    value,
                )
            }
            _ => continue,
        };
        for id in ids {
            dimension_point_controls
                .entry(id)
                .or_default()
                .extend(value.parameter_names().into_iter().map(str::to_owned));
        }
    }
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
    let mut junction_point_ids = BTreeSet::new();
    for projection in &projections {
        let Some(id) = projection["definition"]["id"].as_str() else {
            continue;
        };
        for junction in projection["definition"]["junctions"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let name = junction["id"].as_str().expect("junction ID");
            let point_id = format!("{id}:junction:{name}");
            junction_point_ids.insert(point_id.clone());
            if let Some(p) = point_map.get(&point_id) {
                annotations.push(Annotation {
                    id:format!("junction-{id}-{name}"),label:format!("Junction {name}"),kind:AnnotationKind::Group,status:AnnotationStatus::Constructed,
                    targets:vec![id.into(),point_id.clone()],parameters:projection["source_parameters"].as_array().into_iter().flatten().filter_map(|v|v.as_str().map(str::to_owned)).collect(),
                    anchors:json!([p]),detail:json!({"junction":junction,"point":point_id,"source":projection["definition"]["input"],"coordinates_mm":p,"fixed":true,"match":"unique source-space junction","topology":"fixed reference point; composite spline topology is unchanged"}),
                }.into());
            }
        }
    }
    for projection in &projections {
        let id = projection["definition"]["id"]
            .as_str()
            .expect("projection ID");
        for reference in projection["definition"]["intersections"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let name = reference["id"].as_str().expect("intersection ID");
            let other = reference["other"].as_str().expect("other projection");
            let point_id = format!("{id}:intersection:{name}");
            junction_point_ids.insert(point_id.clone());
            let mut controls: BTreeSet<String> = projection["source_parameters"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            if let Some(other) = projections.iter().find(|p| p["definition"]["id"] == other) {
                controls.extend(
                    other["source_parameters"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str().map(str::to_owned)),
                );
            }
            if let Some(p) = point_map.get(&point_id) {
                annotations.push(Annotation {
                    id:format!("intersection-{id}-{name}"),label:format!("Intersection {name}"),kind:AnnotationKind::Group,status:AnnotationStatus::Constructed,
                    targets:vec![id.into(),other.into(),point_id.clone()],parameters:controls.into_iter().collect(),
                    anchors:json!([p]),detail:json!({"intersection":reference,"point":point_id,"coordinates_mm":p,"fixed":true,"match":"unique finite projected crossing","topology":"fixed reference point; curve topology is unchanged"}),
                }.into());
            }
        }
    }
    for spline in &sketch.splines {
        if let Some(basis) = &spline.basis {
            let mut parameters: BTreeSet<String> = names(&json!(basis)).into_iter().collect();
            for id in &spline.points {
                parameters.extend(point_controls[id.as_str()].iter().cloned());
            }
            annotations.push(Annotation {
                id: format!("bspline-{}", spline.id), label: format!("{}B-spline degree {}", if basis.periodic { "Periodic " } else if basis.non_clamped { "Non-clamped " } else { "" }, basis.degree),
                kind: AnnotationKind::Group, status: AnnotationStatus::Constructed,
                targets: vec![spline.id.clone()], parameters: parameters.into_iter().collect(),
                anchors: json!([point_map[&spline.points[0]]]),
                detail: json!({"basis":basis,"control_points":spline.points,"evaluated_weights":solution.spline_weights[&spline.id],"periodic":basis.periodic}),
            }.into());
        }
    }
    for spline in &sketch.splines {
        if let Some(e) = &spline.endpoints {
            let samples = native_spline_ends[spline.id.as_str()];
            let mut residual = 0_f64;
            let mut anchors = Vec::new();
            for (id, actual) in [(&e.start, &samples[0]), (&e.end, &samples[1])] {
                let p = point_map[id];
                let q = *actual;
                residual = residual.max((p[0] - q[0]).abs()).max((p[1] - q[1]).abs());
                anchors.extend([p, q]);
            }
            annotations.push(Annotation {
                id: format!("bspline-endpoints-{}",spline.id), label:"Native spline endpoints".into(),
                kind:AnnotationKind::Constraint, status:if residual<=1e-9 {AnnotationStatus::Passed} else {AnnotationStatus::Failed},
                targets:vec![spline.id.clone(),e.start.clone(),e.end.clone()],
                parameters:vec![], anchors:json!(anchors),
                detail:json!({"endpoints":e,"max_residual":residual,"residual_unit":"mm","by_construction":false,"tolerance":1e-9}),
            }.into());
        }
    }
    // Label anchor for a curve dimension: the centre of a circle, arc or
    // ellipse, a spline's first point; lines use their midpoint.
    let curve_anchor = |id: &str| -> Option<String> {
        centers
            .get(id)
            .map(|c| (*c).to_owned())
            .or_else(|| spline_defs.get(id).map(|s| s.points[0].clone()))
    };
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
            SketchConstraint::SplineSplineTangent {
                first,
                second,
                point,
            } => (
                "T SPLINES",
                vec![first.clone(), second.clone(), point.clone()],
                vec![point.clone()],
                None,
            ),
            SketchConstraint::EllipseEllipseTangent {
                first,
                second,
                point,
            } => (
                "T ELLIPSES",
                vec![first.clone(), second.clone(), point.clone()],
                vec![point.clone()],
                None,
            ),
            SketchConstraint::SplineEllipseTangent {
                spline,
                ellipse,
                point,
            } => (
                "T SPLINE ELLIPSE",
                vec![spline.clone(), ellipse.clone(), point.clone()],
                vec![point.clone()],
                None,
            ),
            SketchConstraint::SplineCircleTangent {
                spline,
                circle,
                point,
            } => (
                "T SPLINE CIRCLE",
                vec![spline.clone(), circle.clone(), point.clone()],
                vec![point.clone()],
                None,
            ),
            SketchConstraint::EllipseCircleTangent {
                ellipse,
                circle,
                point,
            } => (
                "T ELLIPSE CIRCLE",
                vec![ellipse.clone(), circle.clone(), point.clone()],
                vec![point.clone()],
                None,
            ),
            SketchConstraint::LineSplineTangent {
                line,
                spline,
                point,
            } => (
                "T SPLINE",
                vec![line.clone(), spline.clone(), point.clone()],
                vec![point.clone()],
                None,
            ),
            SketchConstraint::LineEllipseTangent {
                line,
                ellipse,
                side,
            } => (
                match side {
                    SketchLineSide::Left => "T LEFT",
                    SketchLineSide::Right => "T RIGHT",
                },
                vec![line.clone(), ellipse.clone()],
                vec![centers[ellipse.as_str()].to_owned()],
                None,
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
            SketchConstraint::HorizontalDistance {
                first,
                second,
                value,
            } => (
                "↔",
                vec![first.clone(), second.clone()],
                vec![first.clone(), second.clone()],
                Some(value),
            ),
            SketchConstraint::VerticalDistance {
                first,
                second,
                value,
            } => (
                "↕",
                vec![first.clone(), second.clone()],
                vec![first.clone(), second.clone()],
                Some(value),
            ),
            SketchConstraint::LineAngle { line, value } => {
                ("∠x", vec![line.clone()], vec![], Some(value))
            }
            SketchConstraint::ArcAngle { arc, value } => (
                "∠arc",
                vec![arc.clone()],
                curve_anchor(arc).into_iter().collect(),
                Some(value),
            ),
            SketchConstraint::CurveLength { curve, value } => (
                "length",
                vec![curve.clone()],
                curve_anchor(curve).into_iter().collect(),
                Some(value),
            ),
            SketchConstraint::EqualCurveLength { first, second } => (
                "= length",
                vec![first.clone(), second.clone()],
                [first, second]
                    .into_iter()
                    .filter_map(|c| curve_anchor(c))
                    .collect(),
                None,
            ),
            SketchConstraint::Collinear { first, second } => (
                "collinear",
                vec![first.clone(), second.clone()],
                vec![],
                None,
            ),
            SketchConstraint::Coradial { first, second } => (
                "◎=",
                vec![first.clone(), second.clone()],
                vec![centers[first.as_str()].to_owned()],
                None,
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
            SketchConstraint::SplineSplineTangent { .. } => {
                anchors = spline_pair_contacts[&index]
                    .0
                    .into_iter()
                    .map(|p| [p.x, p.y, 0.])
                    .collect();
            }
            SketchConstraint::EllipseEllipseTangent { .. } => {
                anchors = ellipse_pair_contacts[&index]
                    .0
                    .into_iter()
                    .map(|p| [p.x, p.y, 0.])
                    .collect();
            }
            SketchConstraint::SplineEllipseTangent { .. } => {
                anchors = spline_ellipse_contacts[&index]
                    .0
                    .into_iter()
                    .map(|p| [p.x, p.y, 0.])
                    .collect();
            }
            SketchConstraint::SplineCircleTangent { .. } => {
                anchors = spline_circle_contacts[&index]
                    .points
                    .into_iter()
                    .map(|p| [p.x, p.y, 0.])
                    .collect();
            }
            SketchConstraint::EllipseCircleTangent { .. } => {
                anchors = ellipse_circle_contacts[&index]
                    .points
                    .into_iter()
                    .map(|p| [p.x, p.y, 0.])
                    .collect();
            }
            SketchConstraint::LineSplineTangent { .. } => {
                anchors = spline_contacts[&index]
                    .0
                    .into_iter()
                    .map(|p| [p.x, p.y, 0.])
                    .collect();
            }
            SketchConstraint::LineEllipseTangent { .. } => {
                anchors = ellipse_contacts[&index]
                    .into_iter()
                    .map(|p| [p.x, p.y, 0.])
                    .collect();
            }
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
            } else if matches!(
                constraint,
                SketchConstraint::LineAngle { .. } | SketchConstraint::ArcAngle { .. }
            ) {
                format!("{symbol} {} rad", length_label(target))
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
            SketchConstraint::SplineSplineTangent {
                first,
                second,
                point,
            } => {
                let mut ids = vec![point.as_str()];
                for id in [first, second] {
                    let s = spline_defs[id.as_str()];
                    ids.extend(s.points.iter().map(String::as_str));
                    control_names.extend(names(&json!(s.basis)));
                }
                for id in ids {
                    control_names.extend(point_controls[id].iter().cloned());
                    if let Some(names) = dimension_point_controls.get(id) {
                        control_names.extend(names.iter().cloned());
                    }
                }
            }
            SketchConstraint::EllipseEllipseTangent {
                first,
                second,
                point,
            } => {
                let mut ids = vec![point.as_str()];
                for id in [first, second] {
                    let e = ellipses[id.as_str()];
                    ids.extend([e.center.as_str(), e.major.as_str(), e.minor.as_str()]);
                }
                for id in ids {
                    control_names.extend(point_controls[id].iter().cloned());
                    if let Some(names) = dimension_point_controls.get(id) {
                        control_names.extend(names.iter().cloned());
                    }
                }
            }
            SketchConstraint::SplineEllipseTangent {
                spline,
                ellipse,
                point,
            } => {
                let s = spline_defs[spline.as_str()];
                let e = ellipses[ellipse.as_str()];
                let mut ids = vec![
                    point.as_str(),
                    e.center.as_str(),
                    e.major.as_str(),
                    e.minor.as_str(),
                ];
                ids.extend(s.points.iter().map(String::as_str));
                control_names.extend(names(&json!(s.basis)));
                for id in ids {
                    control_names.extend(point_controls[id].iter().cloned());
                    if let Some(names) = dimension_point_controls.get(id) {
                        control_names.extend(names.iter().cloned());
                    }
                }
            }
            SketchConstraint::SplineCircleTangent {
                spline,
                circle,
                point,
            } => {
                let s = spline_defs[spline.as_str()];
                let (c, r) = circular_refs[circle.as_str()];
                let mut ids = vec![c, r, point.as_str()];
                if let Some(end) = arc_ends.get(circle.as_str()) {
                    ids.push(end);
                }
                ids.extend(s.points.iter().map(String::as_str));
                control_names = ids
                    .iter()
                    .flat_map(|id| point_controls[id].iter().cloned())
                    .collect();
                for id in ids {
                    if let Some(names) = dimension_point_controls.get(id) {
                        control_names.extend(names.iter().cloned());
                    }
                }
                control_names.extend(names(&json!(s.basis)));
            }
            SketchConstraint::EllipseCircleTangent {
                ellipse,
                circle,
                point,
            } => {
                let e = ellipses[ellipse.as_str()];
                let (c, r) = circular_refs[circle.as_str()];
                let mut ids = vec![c, r, point.as_str()];
                if let Some(end) = arc_ends.get(circle.as_str()) {
                    ids.push(end);
                }
                ids.extend([e.center.as_str(), e.major.as_str(), e.minor.as_str()]);
                control_names = ids
                    .iter()
                    .flat_map(|id| point_controls[id].iter().cloned())
                    .collect();
                for id in ids {
                    if let Some(names) = dimension_point_controls.get(id) {
                        control_names.extend(names.iter().cloned());
                    }
                }
            }
            SketchConstraint::LineSplineTangent {
                line,
                spline,
                point,
            } => {
                let l = lines[line.as_str()];
                let s = spline_defs[spline.as_str()];
                let mut ids = vec![l.start.as_str(), l.end.as_str(), point.as_str()];
                ids.extend(s.points.iter().map(String::as_str));
                control_names = ids
                    .into_iter()
                    .flat_map(|id| point_controls[id].iter().cloned())
                    .collect();
                control_names.extend(names(&json!(s.basis)));
            }
            SketchConstraint::LineEllipseTangent { line, ellipse, .. } => {
                let l = lines[line.as_str()];
                let e = ellipses[ellipse.as_str()];
                control_names = [
                    l.start.as_str(),
                    l.end.as_str(),
                    e.center.as_str(),
                    e.major.as_str(),
                    e.minor.as_str(),
                ]
                .into_iter()
                .flat_map(|id| point_controls[id].iter().cloned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
                for id in [
                    l.start.as_str(),
                    l.end.as_str(),
                    e.center.as_str(),
                    e.major.as_str(),
                    e.minor.as_str(),
                ] {
                    if let Some(names) = dimension_point_controls.get(id) {
                        control_names.extend(names.iter().cloned());
                    }
                }
            }
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
            SketchConstraint::LineCircleTangent { circle, .. }
            | SketchConstraint::SplineCircleTangent { circle, .. } => vec![circle.as_str()],
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
            SketchConstraint::Angle { .. }
            | SketchConstraint::LineAngle { .. }
            | SketchConstraint::ArcAngle { .. } => "rad",
            SketchConstraint::LineSplineTangent { .. }
            | SketchConstraint::SplineCircleTangent { .. }
            | SketchConstraint::SplineSplineTangent { .. }
            | SketchConstraint::EllipseEllipseTangent { .. }
            | SketchConstraint::SplineEllipseTangent { .. }
            | SketchConstraint::EllipseCircleTangent { .. } => "mixed",
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
                    "contact_residuals": spline_pair_contacts.get(&index).or_else(||ellipse_pair_contacts.get(&index)).or_else(||spline_ellipse_contacts.get(&index)).map(|(p,sine)|json!({
                        "position_residual_mm":(p[0].x-p[1].x).abs().max((p[0].y-p[1].y).abs()).max((p[0].x-p[2].x).abs()).max((p[0].y-p[2].y).abs()),
                        "tangent_sine":sine,"residual_units":["mm","mm","mm","mm","dimensionless"]
                    })).or_else(||spline_circle_contacts.get(&index).or_else(||ellipse_circle_contacts.get(&index)).map(|c|json!({
                        "position_residual_mm": (c.points[0].x-c.points[1].x).abs().max((c.points[0].y-c.points[1].y).abs()).max((c.points[0].x-c.points[2].x).hypot(c.points[0].y-c.points[2].y)),
                        "tangent_sine": c.tangent_sine,"arc_span_residual_mm":c.arc_span_residual_mm,
                        "residual_units":if c.arc_span_residual_mm.is_some() {vec!["mm","mm","mm","dimensionless","mm"]} else {vec!["mm","mm","mm","dimensionless"]}
                    })).or_else(||spline_contacts.get(&index).map(|(points,sine)| json!({
                        "position_residual_mm": (points[0].x-points[1].x).abs().max((points[0].y-points[1].y).abs()).max((points[0].x-points[2].x).hypot(points[0].y-points[2].y)),
                        "tangent_sine":sine,
                        "residual_units":["mm","mm","mm","dimensionless"]
                    })))),
                }),
            }
            .into(),
        );
    }
    // Authored shapes: one group per rectangle, slot or polygon.
    for shape in &shapes {
        let (label, anchor, expression) = match shape {
            SketchShape::Rectangle {
                first,
                opposite,
                angle,
                ..
            } => (
                if angle.is_some() {
                    "rotated rectangle".to_owned()
                } else {
                    "rectangle".to_owned()
                },
                vec![point_map[first], point_map[opposite]],
                json!(angle),
            ),
            SketchShape::Slot {
                start, end, radius, ..
            } => (
                format!(
                    "slot R {} mm",
                    length_label(radius.evaluate(parameters).stage("sketch")?.value)
                ),
                vec![point_map[start], point_map[end]],
                json!(radius),
            ),
            SketchShape::Conic {
                start,
                control,
                end,
                rho,
                ..
            } => {
                let value = match rho {
                    Some(rho) => rho.evaluate(parameters).stage("sketch")?.value,
                    None => 0.5,
                };
                let kind = if (value - 0.5).abs() <= 1e-12 {
                    "parabola"
                } else if value < 0.5 {
                    "elliptic"
                } else {
                    "hyperbolic"
                };
                (
                    format!("conic ρ {value:.3} ({kind})"),
                    vec![point_map[start], point_map[control], point_map[end]],
                    json!(rho),
                )
            }
            SketchShape::Polygon { center, sides, .. } => (
                format!("regular {sides}-gon"),
                vec![point_map[center]],
                json!(null),
            ),
        };
        annotations.push(
            Annotation {
                id: format!("shape-{}", shape.id()),
                label,
                kind: AnnotationKind::Group,
                status: AnnotationStatus::Constructed,
                targets: shape.boundary(),
                parameters: names(&expression),
                anchors: json!(anchor),
                detail: json!({"shape": shape, "boundary": shape.boundary()}),
            }
            .into(),
        );
    }
    // Driven (reference) dimensions: measured, never constraining.
    for ((id, value), measurement) in sketch
        .measure(parameters, &solution)
        .stage("sketch")?
        .into_iter()
        .zip(&sketch.measurements)
    {
        let (targets, anchor_ids): (Vec<&str>, Vec<String>) = match &measurement.measure {
            SketchMeasure::Distance { first, second }
            | SketchMeasure::HorizontalDistance { first, second }
            | SketchMeasure::VerticalDistance { first, second } => {
                (vec![first, second], vec![first.clone(), second.clone()])
            }
            SketchMeasure::PointLineDistance { point, line } => {
                (vec![point, line], vec![point.clone()])
            }
            SketchMeasure::Angle { first, second } => (vec![first, second], vec![]),
            SketchMeasure::LineAngle { line } => (vec![line], vec![]),
            SketchMeasure::ArcAngle { arc: curve }
            | SketchMeasure::Radius { curve }
            | SketchMeasure::Diameter { curve }
            | SketchMeasure::CurveLength { curve } => {
                (vec![curve], curve_anchor(curve).into_iter().collect())
            }
        };
        let anchors = if anchor_ids.is_empty() {
            targets
                .iter()
                .filter_map(|id| lines.get(id))
                .map(|l| {
                    let (a, b) = (point_map[&l.start], point_map[&l.end]);
                    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, 0.0]
                })
                .collect::<Vec<_>>()
        } else {
            anchor_ids.iter().map(|id| point_map[id]).collect()
        };
        let unit = if measurement.measure.is_angle() {
            "rad"
        } else {
            "mm"
        };
        annotations.push(
            Annotation {
                id: format!("measurement-{id}"),
                label: format!("({} {unit})", length_label(value)),
                kind: AnnotationKind::Dimension,
                status: AnnotationStatus::Measured,
                targets: targets.into_iter().map(str::to_owned).collect(),
                parameters: vec![],
                anchors: json!(anchors),
                detail: json!({
                    "measurement": measurement,
                    "value": value,
                    "unit": unit,
                    "driving": false,
                    "description": "Reference dimension measured on the solved sketch; \
                        it does not constrain geometry.",
                }),
            }
            .into(),
        );
    }
    let diagnosis = &solution.diagnosis;
    let mut memberships = HashMap::<usize, Vec<usize>>::new();
    for (group_index, group) in diagnosis.conflict_groups.iter().enumerate() {
        for source in &group.sources {
            if let SketchEquationSource::Constraint { index } = source {
                memberships.entry(*index).or_default().push(group_index);
            }
        }
    }
    let redundant = diagnosis
        .locally_redundant_constraints
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let fixed = diagnosis
        .fixed_satisfied_constraints
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for annotation in &mut annotations {
        if let Some(index) = annotation["detail"]["constraint_index"]
            .as_u64()
            .map(|i| i as usize)
        {
            annotation["detail"]["diagnosis"] = json!({
                "local_role":if annotation["detail"]["by_construction"]==true {"constructed"} else if redundant.contains(&index) {"locally_redundant"} else if fixed.contains(&index) {"fixed_geometry"} else {"measured_equation"},
                "conflict_groups":memberships.get(&index).cloned().unwrap_or_default(),
                "local_analysis_only":true,
            });
        }
    }
    for (index, motion) in diagnosis.free_motions.iter().enumerate() {
        let origin = point_map[&motion.coordinate.point];
        let direction = motion
            .points
            .iter()
            .find(|p| p.point == motion.coordinate.point)
            .unwrap();
        let targets = motion
            .points
            .iter()
            .map(|p| p.point.clone())
            .collect::<Vec<_>>();
        annotations.push(Annotation {
            id:format!("free-motion-{index}"),label:format!("Free motion {} · {} {:?} (local)",index+1,motion.coordinate.point,motion.coordinate.axis),
            kind:AnnotationKind::Constraint,status:AnnotationStatus::Unverified,targets,parameters:Vec::new(),
            anchors:json!([origin,[origin[0]+direction.dx,origin[1]+direction.dy,0.]]),
            detail:json!({"free_motion":motion,"description":"Representative direction in the final Jacobian nullspace. A local direction is not a finite edit or a global degree-of-freedom proof."}),
        }.into());
    }
    let constraint_annotations = annotations
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            a["detail"]["constraint_index"]
                .as_u64()
                .map(|c| (c as usize, i))
        })
        .collect::<HashMap<_, _>>();
    for (index, group) in diagnosis.conflict_groups.iter().enumerate() {
        let mut targets = BTreeSet::new();
        let mut anchors = Vec::new();
        let mut parameters = BTreeSet::new();
        for source in &group.sources {
            match source {
                SketchEquationSource::Constraint { index } => {
                    if let Some(annotation) = constraint_annotations
                        .get(index)
                        .and_then(|i| annotations.get(*i))
                    {
                        // Source constraint annotations precede diagnostics.
                        targets.extend(
                            annotation["targets"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .map(str::to_owned),
                        );
                        parameters.extend(
                            annotation["parameters"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .map(str::to_owned),
                        );
                        anchors.extend(
                            annotation["anchors"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .take(3)
                                .cloned(),
                        );
                    }
                }
                SketchEquationSource::ArcRadii { entity }
                | SketchEquationSource::EllipseAxes { entity }
                | SketchEquationSource::SplineEndpoints { entity } => {
                    targets.insert(entity.clone());
                }
                SketchEquationSource::PatternLink { point, .. } => {
                    targets.insert(point.clone());
                    if let Some(p) = point_map.get(point) {
                        anchors.push(json!(p));
                    }
                }
            }
        }
        anchors.truncate(8);
        annotations.push(Annotation {
            id:format!("conflict-group-{index}"),label:format!("Conflict group {} · {}",index+1,group.evidence.replace('_'," ")),
            kind:AnnotationKind::Constraint,status:AnnotationStatus::Failed,targets:targets.into_iter().collect(),parameters:parameters.into_iter().collect(),anchors:json!(anchors),
            detail:json!({"conflict_group":group,"description":"Related equations disagree at the current iterate. Linearized groups are local evidence, not minimal conflict sets or a proof that the nonlinear sketch has no solution."}),
        }.into());
    }
    if sketch.acceptance == SketchAcceptance::FullyConstrained {
        let targets = solution
            .diagnosis
            .free_coordinates
            .iter()
            .map(|c| c.point.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .take(128)
            .collect::<Vec<_>>();
        let anchor = targets
            .first()
            .and_then(|id| point_map.get(id))
            .or_else(|| sketch.points.first().and_then(|p| point_map.get(&p.id)));
        annotations.push(Annotation {
            id: "acceptance-policy".into(), label: format!("Strict sketch · {} free", solution.free_degrees),
            kind: AnnotationKind::Constraint,
            status: if acceptance_error.is_some() { AnnotationStatus::Failed } else { AnnotationStatus::Passed },
            targets, parameters: Vec::new(), anchors: json!(anchor.into_iter().collect::<Vec<_>>()),
            detail: json!({"acceptance":sketch.acceptance,"accepted":acceptance_error.is_none(),"error":acceptance_error,"free_degrees":solution.free_degrees,"description":"Requires zero local free degrees across all sketch points, including construction geometry."}),
        }.into());
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
        if p.fixed && !junction_point_ids.contains(&p.id) {
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
        let mut control_names = names(&detail);
        let (label, targets) = match operation {
            SketchProfileOperation::Trim { entity, .. } => {
                (format!("Trim {entity}"), vec![entity.clone()])
            }
            SketchProfileOperation::TrimBetween { entity, from, to } => {
                let mut targets = vec![entity.clone()];
                for bound in [from, to] {
                    match bound {
                        SketchTrimBound::Point { point } => targets.push(point.clone()),
                        SketchTrimBound::Crossing { curve, near } => {
                            targets.extend([curve.clone(), near.clone()])
                        }
                        _ => {}
                    }
                }
                (format!("Trim {entity} between bounds"), targets)
            }
            SketchProfileOperation::Extend { entity, .. } => {
                (format!("Extend {entity}"), vec![entity.clone()])
            }
            SketchProfileOperation::Fillet { point, radius } => (
                format!(
                    "Fillet R {} mm",
                    length_label(radius.evaluate(parameters).stage("sketch")?.value)
                ),
                vec![point.clone()],
            ),
            SketchProfileOperation::Chamfer {
                point,
                incoming,
                outgoing,
            } => (
                format!(
                    "Chamfer {} × {} mm",
                    length_label(incoming.evaluate(parameters).stage("sketch")?.value),
                    length_label(outgoing.evaluate(parameters).stage("sketch")?.value)
                ),
                vec![point.clone()],
            ),
            SketchProfileOperation::Offset { distance, .. } => (
                format!(
                    "Offset {} mm",
                    length_label(distance.evaluate(parameters).stage("sketch")?.value)
                ),
                vec![],
            ),
        };
        let anchors = match operation {
            SketchProfileOperation::Fillet { point, .. }
            | SketchProfileOperation::Chamfer { point, .. } => {
                control_names.extend(point_controls[point.as_str()].iter().cloned());
                if let Some(names) = dimension_point_controls.get(point.as_str()) {
                    control_names.extend(names.iter().cloned());
                }
                vec![point_map[point.as_str()]]
            }
            _ => Vec::new(),
        };
        control_names.sort();
        control_names.dedup();
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
                anchors: json!(anchors),
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
    for pattern in &sketch.patterns {
        let detail = serde_json::to_value(pattern).stage("sketch")?;
        let instances = pattern.instances();
        let copies = (1..=instances)
            .flat_map(|k| {
                pattern
                    .entities()
                    .iter()
                    .map(move |e| pattern.copy_id(k, e))
            })
            .collect::<Vec<_>>();
        let mid = |line: &str| {
            let l = sketch.lines.iter().find(|l| l.id == line)?;
            let (a, b) = (point_map[l.start.as_str()], point_map[l.end.as_str()]);
            Some([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, 0.0])
        };
        let (label, anchor) = match pattern {
            SketchPattern::Mirror { id, axis, .. } => {
                (format!("Mirror {id} about {axis}"), mid(axis))
            }
            SketchPattern::Linear {
                id,
                direction,
                spacing,
                count,
                ..
            } => (
                format!(
                    "Linear {id} × {count} @ {} mm",
                    length_label(spacing.evaluate(parameters).stage("sketch")?.value)
                ),
                mid(direction),
            ),
            SketchPattern::Circular {
                id, center, count, ..
            } => (
                format!(
                    "Circular {id} × {count} @ {:.3}°",
                    pattern
                        .circular_step(parameters)
                        .stage("sketch")?
                        .to_degrees()
                ),
                Some(point_map[center.as_str()]),
            ),
        };
        let mut targets = pattern.entities().to_vec();
        targets.extend(copies.iter().cloned());
        annotations.push(
            Annotation {
                id: format!("pattern-{}", pattern.id()),
                label,
                kind: AnnotationKind::Group,
                status: AnnotationStatus::Driving,
                targets,
                parameters: names(&detail),
                anchors: json!(anchor.into_iter().collect::<Vec<_>>()),
                detail: json!({
                    "pattern": detail,
                    "instances": instances + 1,
                    "copies": copies,
                    "description": "Linked copies: source edits move every \
                        instance; copies carry no constraints of their own",
                }),
            }
            .into(),
        );
    }
    let vertices = entities
        .iter()
        .map(|e| {
            e["points"].as_array().map_or(0, Vec::len)
                + e["bspline"]["control_points"]
                    .as_array()
                    .map_or(0, Vec::len)
        })
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
        "acceptance": sketch.acceptance,
        "acceptance_error": acceptance_error,
        "profile_operations": sketch.profile_operations,
        "holes": sketch.holes,
        "patterns": sketch.patterns,
        "points": point_map,
        "parameters": controls,
        "annotations": annotations,
        "solver": {
            "solved": solution.solved,
            "accepted": acceptance_error.is_none(),
            "iterations": solution.iterations,
            "max_residual": solution.max_residual,
            "free_degrees": solution.free_degrees,
            "diagnosis": solution.diagnosis,
            "redundant_equations": solution.redundant_equations,
        },
        "coordinate_system": "sketch-local XY millimeters",
    }))
}
