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
    budget: &mut Budget,
) -> Result<Value, Failure> {
    let solution = sketch
        .solve(parameters)
        .map_err(|e| model_failure("sketch", e))?;
    let checks = sketch
        .constraint_checks(parameters, &solution)
        .map_err(|e| model_failure("sketch", e))?;
    let curves = sketch
        .preview_curves(session, &solution, 32)
        .map_err(|e| model_failure("sketch", e))?;
    let entities=curves.into_iter().map(|(id,points)|{
        let point_ids=if let Some(e)=sketch.lines.iter().find(|e|e.id==id){vec![e.start.clone(),e.end.clone()]}else if let Some(e)=sketch.arcs.iter().find(|e|e.id==id){vec![e.center.clone(),e.start.clone(),e.end.clone()]}else if let Some(e)=sketch.circles.iter().find(|e|e.id==id){vec![e.center.clone(),e.rim.clone()]}else if let Some(e)=sketch.ellipses.iter().find(|e|e.id==id){vec![e.center.clone(),e.major.clone(),e.minor.clone()]}else{sketch.splines.iter().find(|e|e.id==id).map(|e|e.points.clone()).unwrap_or_default()};
        json!({"id":id,"points":points.iter().map(|p|[p.x,p.y,0.0]).collect::<Vec<_>>(),"point_ids":point_ids})
    }).collect::<Vec<_>>();
    let point_map = solution
        .points
        .iter()
        .map(|(id, p)| (id.clone(), [p.x, p.y, 0.0]))
        .collect::<BTreeMap<_, _>>();
    let mut annotations = Vec::new();
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
                let refs = sketch
                    .circles
                    .iter()
                    .find(|c| c.id == *curve)
                    .map(|c| vec![c.center.clone(), c.rim.clone()])
                    .or_else(|| {
                        sketch
                            .arcs
                            .iter()
                            .find(|a| a.id == *curve)
                            .map(|a| vec![a.center.clone(), a.start.clone()])
                    })
                    .unwrap_or_default();
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
            "constructed"
        } else if check.satisfied {
            "passed"
        } else {
            "failed"
        };
        let label = if let Some(expression) = value {
            let target = expression
                .evaluate(parameters)
                .map_err(|e| model_failure("sketch", e))?
                .value;
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
        let control_names = value
            .map(|v| v.parameter_names().into_iter().map(str::to_owned).collect())
            .unwrap_or_default();
        annotations.push(annotation(format!("constraint-{index}"),label,if value.is_some(){"dimension"}else{"constraint"},status,targets,control_names,json!(anchors),json!({"constraint_index":index,"constraint":constraint,"angular_arc":angular_arc,"max_residual":check.max_residual,"by_construction":check.by_construction,"tolerance":1e-9,"residual_unit":if matches!(constraint,SketchConstraint::Angle {..}){"rad"}else if matches!(constraint,SketchConstraint::Parallel {..}|SketchConstraint::Perpendicular {..}|SketchConstraint::Tangent {..}){"dimensionless"}else{"mm"}})));
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
            annotations.push(annotation(
                format!("fixed-{}", p.id),
                format!("fixed {}", p.id),
                "constraint",
                "fixed",
                vec![p.id.clone()],
                control_names.clone(),
                json!([point_map[&p.id]]),
                json!({"point":p.id,"fixed":true}),
            ));
        }
        for name in control_names {
            annotations.push(annotation(
                format!("parameter-{}-{name}", p.id),
                name.clone(),
                "parameter",
                if p.fixed { "driving" } else { "initial" },
                vec![p.id.clone()],
                vec![name.clone()],
                json!([point_map[&p.id]]),
                json!({"point":p.id,"parameter":name,"control":controls.get(&name),"coordinate_role":if p.fixed {"fixed driving coordinate"}else{"initial guess; constraints may move this point"}}),
            ));
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
        let detail = serde_json::to_value(operation).map_err(|e| failure("sketch", e))?;
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
                    length_label(
                        distance
                            .evaluate(parameters)
                            .map_err(|e| model_failure("sketch", e))?
                            .value
                    )
                ),
                vec![],
            ),
        };
        annotations.push(annotation(format!("profile-operation-{i}"),label,"profile_operation",if profile_error.is_some(){"unverified"}else{"driving"},targets,control_names,json!([]),json!({"operation":detail,"error":profile_error,"description":"Derived profile edit; source constraints remain on their original entities"})));
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
    Ok(
        json!({"kind":"sketch","instance":instance,"feature":feature,"title":format!("{instance}/{feature} — {}",sketch.id),"sketch":sketch.id,"bounds":[min,max],"entities":entities,"edited_profile":edited_profile,"profile_error":profile_error,"profile_operations":sketch.profile_operations,"points":point_map,"parameters":controls,"annotations":annotations,"solver":{"solved":solution.solved,"iterations":solution.iterations,"max_residual":solution.max_residual,"free_degrees":solution.free_degrees,"redundant_equations":solution.redundant_equations},"coordinate_system":"sketch-local XY millimeters"}),
    )
}
