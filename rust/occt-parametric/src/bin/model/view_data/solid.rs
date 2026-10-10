//! Annotated solid scenes: native mesh, outlines, extents, and linked parameter, constraint, and requirement annotations.

use super::dimensions::helix_dimensions;
use super::features::{SolidScene, feature_dimensions};
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn solid_scene(
    session: &Session,
    part: &PartInstance<'_>,
    diagnostic: &DiagnosticGeneration<'_>,
    output: &str,
    controls: &BTreeMap<String, Value>,
    parameters: &HashMap<String, ParameterValue>,
    budget: &mut Budget,
) -> Result<Value, Failure> {
    let generated = &diagnostic.generated;
    let helix = helix_route(part, output);
    let edge_samples = helix
        .map(|(_, op)| helix_samples(op, parameters))
        .transpose()?
        .unwrap_or(32);
    let shape = generated.shape(output).ok_or_else(|| {
        failure(
            "selection",
            format!("unknown visualization output '{output}'"),
        )
    })?;
    let bounds = session
        .exact_bounds(shape)
        .map_err(|e| failure("visualization", e))?;
    let min = point(bounds.min);
    let max = point(bounds.max);
    let span = (0..3).map(|i| max[i] - min[i]).fold(0.0, f64::max);
    if !span.is_finite() || span <= 0.0 {
        return Err(failure(
            "visualization",
            "shape has no finite visual extent",
        ));
    }
    // Wires and edges have no faces to triangulate, but retain native outlines.
    let mesh = if session
        .subshape_count(shape, ShapeType::Face)
        .map_err(|e| failure("visualization", e))?
        == 0
    {
        Vec::new()
    } else {
        session
            .surface_mesh(
                shape,
                MeshOptions {
                    linear_deflection: 0.1_f64.max(span * 1e-5),
                    angular_deflection_radians: 0.3,
                    maximum_triangles: budget.triangles.max(1),
                },
            )
            .map_err(|e| failure("visualization", e))?
    };
    check_budget(&mut budget.triangles, mesh.len(), "triangle")?;
    check_budget(&mut budget.vertices, mesh.len() * 3, "vertex")?;
    let mesh = mesh
        .iter()
        .map(|triangle| json!({"face":triangle.face_index,"points":triangle.points.map(point)}))
        .collect::<Vec<_>>();
    let mut lines = Vec::new();
    for edge in session
        .subshapes(shape, ShapeType::Edge)
        .map_err(|e| failure("visualization", e))?
    {
        check_budget(&mut budget.vertices, edge_samples, "vertex")?;
        let points = session
            .edge_sample_points(&edge, edge_samples)
            .map_err(|e| failure("visualization", e))?;
        lines.push(points.into_iter().map(point).collect::<Vec<_>>());
    }
    let mut annotations = Vec::new();
    for axis in 0..3 {
        let mut end = min;
        end[axis] = max[axis];
        annotations.push(annotation(format!("extent-{axis}"),format!("{} span {} mm",["X","Y","Z"][axis],length_label(max[axis]-min[axis])),"dimension","measured",vec![output.into()],vec![],json!([min,end]),json!({"measured_mm":max[axis]-min[axis],"axis":(["X","Y","Z"][axis]),"driving":false,"description":"Exact geometry bounding extent along the family axis; independent of view rotation"})));
    }
    let center = [
        min[0] * 0.5 + max[0] * 0.5,
        min[1] * 0.5 + max[1] * 0.5,
        min[2] * 0.5 + max[2] * 0.5,
    ];
    let input_map = part
        .definition
        .feature_inputs()
        .map_err(|e| model_failure("validation", e))?;
    let mut ancestors = BTreeSet::new();
    let mut pending = vec![output];
    while let Some(id) = pending.pop() {
        if ancestors.insert(id)
            && let Some(inputs) = input_map.get(id)
        {
            pending.extend(inputs.iter().copied());
        }
    }
    let mut used = BTreeSet::new();
    let mut bindings = BTreeMap::<String, Vec<String>>::new();
    for feature in &part.definition.features {
        if ancestors.contains(feature.id.as_str()) {
            let expression = serde_json::to_value(&feature.operation)
                .map_err(|e| failure("visualization", e))?;
            for name in names(&expression) {
                used.insert(name.clone());
                bindings.entry(name).or_default().push(feature.id.clone());
            }
        }
    }
    for name in used {
        if let Some(control) = controls.get(&name) {
            annotations.push(annotation(format!("parameter-{name}"),name.clone(),"parameter","driving",vec![output.into()],vec![name.clone()],json!([center]),json!({"parameter":name,"control":control,"features":bindings[&name],"description":"Driving parameter used by this output's feature inputs; highlight indicates the related output, not a fitted dimension"})));
        }
    }
    for constraint in &part.definition.constraints {
        let mut control_names = constraint
            .left
            .parameter_names()
            .into_iter()
            .chain(constraint.right.parameter_names())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        control_names.sort();
        control_names.dedup();
        annotations.push(annotation(format!("constraint-{}",constraint.id),constraint.id.clone(),"constraint","passed",vec![output.into()],control_names,json!([center]),json!({"constraint":constraint,"left":constraint.left.evaluate(parameters).map_err(|e|model_failure("visualization",e))?,"right":constraint.right.evaluate(parameters).map_err(|e|model_failure("visualization",e))?})));
    }
    for requirement in part
        .definition
        .requirements
        .iter()
        .filter(|r| r.rule.output() == output)
    {
        let (status, verification, anchors) = match generated
            .verification
            .iter()
            .find(|r| r.requirement_id == requirement.id)
        {
            Some(result) => {
                let status = if result.status == VerificationStatus::Passed {
                    "passed"
                } else {
                    "failed"
                };
                let anchors = result
                    .witness
                    .as_ref()
                    .map(|w| w.points_mm.iter().map(|p| point(*p)).collect::<Vec<_>>())
                    .filter(|p| !p.is_empty())
                    .unwrap_or_else(|| vec![center]);
                (
                    status,
                    verification_result(&part.id, result),
                    json!(anchors),
                )
            }
            None => {
                let message = diagnostic
                    .verification_errors
                    .iter()
                    .find(|(id, _)| id == &requirement.id)
                    .map(|(_, e)| e.message.clone());
                ("unverified", json!({"message":message}), json!([center]))
            }
        };
        annotations.push(annotation(
            format!("requirement-{}", requirement.id),
            requirement.statement.clone(),
            "requirement",
            status,
            vec![output.into()],
            vec![],
            anchors,
            json!({"requirement":requirement,"verification":verification}),
        ));
    }
    if let Some((path, operation)) = helix {
        let route = generated
            .shape(path)
            .ok_or_else(|| failure("visualization", "helix route unavailable"))?;
        helix_dimensions(
            session,
            route,
            path,
            output,
            operation,
            parameters,
            &mut annotations,
        )?;
    }
    if let Some(feature) = part.definition.features.iter().find(|f| f.id == output) {
        let scene = SolidScene {
            session,
            part,
            generated,
            output,
            parameters,
            input_map: &input_map,
            shape,
            min,
            max,
            center,
            edge_samples,
        };
        feature_dimensions(&scene, feature, budget, &mut annotations)?;
    }
    check_budget(&mut budget.annotations, annotations.len(), "annotation")?;
    Ok(
        json!({"kind":"solid","instance":part.id,"feature":output,"title":format!("{}/{}",part.id,output),"bounds":[min,max],"mesh":mesh,"lines":lines,"parameters":controls,"annotations":annotations,"coordinate_system":"family-local mm","valid":session.is_valid(shape).map_err(|e|failure("visualization",e))?}),
    )
}
