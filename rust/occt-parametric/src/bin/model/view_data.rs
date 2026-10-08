//! Shared native dimension and constraint scenes for live and standalone viewers.
use occt_bridge::{MeshOptions, Session, ShapeType, Vec3};
use occt_parametric::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug)]
pub struct Failure {
    pub stage: &'static str,
    pub message: String,
    pub diagnostics: Value,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.stage, self.message)?;
        if self
            .diagnostics
            .as_array()
            .is_some_and(|items| !items.is_empty())
        {
            write!(f, "; feature diagnostics: {}", self.diagnostics)?;
        }
        Ok(())
    }
}
impl std::error::Error for Failure {}
fn failure(stage: &'static str, error: impl std::fmt::Display) -> Failure {
    Failure {
        stage,
        message: error.to_string(),
        diagnostics: json!([]),
    }
}
fn model_failure(stage: &'static str, error: ModelError) -> Failure {
    // Retain the public feature diagnostics for command clients.
    let diagnostics = error.diagnostics.iter().map(|d| json!({"feature":d.feature,"kind":format!("{:?}",d.kind),"code":d.code,"name":d.name,"selection":d.selection,"selector":d.selector,"input":d.input})).collect::<Vec<_>>();
    Failure {
        stage,
        message: error.message,
        diagnostics: json!(diagnostics),
    }
}
#[derive(Clone, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ViewOptions {
    maximum_triangles: usize,
    maximum_vertices: usize,
    maximum_annotations: usize,
    maximum_scenes: usize,
}
impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            maximum_triangles: 100_000,
            maximum_vertices: 1_000_000,
            maximum_annotations: 10_000,
            maximum_scenes: 1000,
        }
    }
}
struct Budget {
    triangles: usize,
    vertices: usize,
    annotations: usize,
}
fn point(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}
fn names(value: &Value) -> Vec<String> {
    let mut found = BTreeSet::new();
    let mut pending = vec![value];
    while let Some(v) = pending.pop() {
        match v {
            Value::Object(map) => {
                if let Some(Value::String(name)) = map.get("parameter") {
                    found.insert(name.clone());
                }
                pending.extend(map.values());
            }
            Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    found.into_iter().collect()
}
#[allow(clippy::too_many_arguments)]
fn annotation(
    id: String,
    label: String,
    kind: &str,
    status: &str,
    targets: Vec<String>,
    parameters: Vec<String>,
    anchors: Value,
    detail: Value,
) -> Value {
    json!({"id":id,"label":label,"kind":kind,"status":status,"targets":targets,"parameters":parameters,"anchors":anchors,"detail":detail})
}
fn length_label(value: f64) -> String {
    if value != 0.0 && (value.abs() < 1e-3 || value.abs() >= 1e6) {
        format!("{value:.4e}")
    } else {
        format!("{value:.3}")
    }
}
fn check_budget(budget: &mut usize, count: usize, kind: &str) -> Result<(), Failure> {
    *budget = budget
        .checked_sub(count)
        .ok_or_else(|| failure("visualization", format!("{kind} budget exceeded")))?;
    Ok(())
}
pub fn collect(
    model: &ModelDocument,
    outputs: &[InstanceOutputRef],
    sketches: bool,
    options: &ViewOptions,
) -> Result<Value, Failure> {
    if options.maximum_triangles == 0
        || options.maximum_triangles > 1_000_000
        || options.maximum_vertices == 0
        || options.maximum_vertices > 1_000_000
        || options.maximum_annotations == 0
        || options.maximum_annotations > 100_000
        || options.maximum_scenes == 0
        || options.maximum_scenes > 10_000
        || outputs.len() > 10_000
    {
        return Err(failure(
            "request",
            "visualization options exceed supported limits",
        ));
    }
    let graph = model
        .instance_graph()
        .map_err(|e| model_failure("validation", e))?;
    let ids = if outputs.is_empty() {
        model
            .instances
            .iter()
            .map(|n| n.id().to_owned())
            .collect::<BTreeSet<_>>()
    } else {
        outputs.iter().map(|o| o.instance.clone()).collect()
    };
    if ids.len() > options.maximum_scenes {
        return Err(failure("visualization", "scene budget exceeded"));
    }
    let session = Session::new().map_err(|e| failure("kernel", e))?;
    let mut budget = Budget {
        triangles: options.maximum_triangles,
        vertices: options.maximum_vertices,
        annotations: options.maximum_annotations,
    };
    let mut scenes = Vec::new();
    let mut scene_ids = BTreeSet::new();
    for id in &ids {
        let part = graph
            .resolve(id)
            .map_err(|e| model_failure("resolution", e))?;
        let parameters = part
            .resolved_parameters()
            .map_err(|e| model_failure("resolution", e))?;
        let controls=parameters.iter().map(|(name,value)|(name.clone(),json!({"value":value,"definition":part.definition.parameters.iter().find(|p|p.id==*name)}))).collect::<BTreeMap<_,_>>();
        let selected = outputs
            .iter()
            .filter(|o| o.instance == *id)
            .collect::<Vec<_>>();
        if !selected.is_empty() {
            match part.diagnostic_geometry(&session) {
                Ok(diagnostic) => {
                    for output in selected {
                        if !scene_ids.insert((id.clone(), output.output.clone())) {
                            return Err(failure("request", "duplicate visualization output"));
                        }
                        let scene = solid_scene(
                            &session,
                            &part,
                            &diagnostic,
                            &output.output,
                            &controls,
                            &parameters,
                            &mut budget,
                        )?;
                        scenes.push(scene);
                    }
                }
                Err(error) => {
                    for output in selected {
                        scenes.push(json!({"kind":"solid","instance":id,"feature":output.output,"title":format!("{id}/{}",output.output),"error":error.message,"annotations":[],"parameters":controls}));
                    }
                }
            }
        }
        if sketches {
            for feature in &part.definition.features {
                let sketch = match &feature.operation {
                    FeatureOperation::SketchFace { sketch }
                    | FeatureOperation::SketchWire { sketch }
                    | FeatureOperation::SketchOpenWire { sketch } => sketch,
                    _ => continue,
                };
                match sketch_scene(&session,id,&feature.id,sketch,!matches!(feature.operation,FeatureOperation::SketchOpenWire {..}),&parameters,&controls,&mut budget) {
                    Ok(scene)=>scenes.push(scene),
                    Err(error) if error.stage=="sketch"=>scenes.push(json!({"kind":"sketch","instance":id,"feature":feature.id,"title":format!("{id}/{}",feature.id),"error":error.message,"annotations":[],"parameters":controls})),
                    Err(error)=>return Err(error),
                }
            }
        }
        if scenes.len() > options.maximum_scenes {
            return Err(failure("visualization", "scene budget exceeded"));
        }
    }
    if scenes.is_empty() {
        return Err(failure(
            "visualization",
            "select a shape output or provide a sketch feature",
        ));
    }
    Ok(
        json!({"schema":"occb-annotated-view-v1","diagnostic":true,"coordinate_system":"family-local millimeters","scenes":scenes}),
    )
}
pub fn verification_result(scope: &str, r: &VerificationResult) -> Value {
    let measured = r.measured.map(|m| json!({"value":m.value,"unit":format!("{:?}",m.unit),"minimum":m.minimum,"maximum":m.maximum}));
    let evidence = match r.evidence {
        Evidence::Exact => json!({"kind":"exact"}),
        Evidence::Sampled {
            samples,
            unresolved,
        } => json!({"kind":"sampled","samples":samples,"unresolved":unresolved}),
    };
    let witness = r.witness.as_ref().map(|w| json!({"subjects":w.subjects,"points_mm":w.points_mm.iter().map(|p| [p.x,p.y,p.z]).collect::<Vec<_>>()}));
    json!({"scope":scope,"requirement":r.requirement_id,
        "status":if r.status == VerificationStatus::Passed {"passed"} else {"failed"},
        "message":r.message,"measured":measured,"evidence":evidence,"witness":witness})
}
#[allow(clippy::too_many_arguments)]
fn solid_scene(
    session: &Session,
    part: &PartInstance<'_>,
    diagnostic: &DiagnosticGeneration<'_>,
    output: &str,
    controls: &BTreeMap<String, Value>,
    parameters: &HashMap<String, ParameterValue>,
    budget: &mut Budget,
) -> Result<Value, Failure> {
    let generated = &diagnostic.generated;
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
    let mesh = session
        .surface_mesh(
            shape,
            MeshOptions {
                linear_deflection: 0.1_f64.max(span * 1e-5),
                angular_deflection_radians: 0.3,
                maximum_triangles: budget.triangles.max(1),
            },
        )
        .map_err(|e| failure("visualization", e))?;
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
        let points = session
            .edge_sample_points(&edge, 32)
            .map_err(|e| failure("visualization", e))?;
        check_budget(&mut budget.vertices, points.len(), "vertex")?;
        lines.push(points.into_iter().map(point).collect::<Vec<_>>());
    }
    let mut annotations = Vec::new();
    for axis in 0..3 {
        let mut end = min;
        end[axis] = max[axis];
        annotations.push(annotation(format!("extent-{axis}"),format!("{} span {} mm",["X","Y","Z"][axis],max[axis]-min[axis]),"dimension","measured",vec![output.into()],vec![],json!([min,end]),json!({"measured_mm":max[axis]-min[axis],"axis":(["X","Y","Z"][axis]),"driving":false,"description":"Exact geometry bounding extent along the family axis; independent of view rotation"})));
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
    // Direct primitive dimensions have their true feature-frame anchors. For
    // downstream booleans/transforms, controls stay in the linked side panel.
    if let Some(feature) = part.definition.features.iter().find(|f| f.id == output) {
        primitive_dimensions(feature, parameters, &mut annotations)?;
    }
    if let Some(FeatureDefinition {
        operation: FeatureOperation::Extrude { input, direction },
        ..
    }) = part.definition.features.iter().find(|f| f.id == output)
        && let Some(profile) = generated.shape(input)
    {
        let a = point(
            session
                .center_of_mass(profile)
                .map_err(|e| failure("visualization", e))?,
        );
        let d = direction
            .evaluate(parameters)
            .map_err(|e| model_failure("visualization", e))?;
        let d = [d.x.value, d.y.value, d.z.value];
        let length = d[0].hypot(d[1].hypot(d[2]));
        annotations.push(annotation(
            "driving-extrusion".into(),
            format!("extrusion {} mm", length_label(length)),
            "dimension",
            "driving",
            vec![output.into()],
            names(&serde_json::to_value(direction).map_err(|e| failure("visualization", e))?),
            json!([a, std::array::from_fn::<_, 3, _>(|i| a[i] + d[i])]),
            json!({"feature":output,"expression":direction,"value_mm":length,"driving":true}),
        ));
    }
    check_budget(&mut budget.annotations, annotations.len(), "annotation")?;
    Ok(
        json!({"kind":"solid","instance":part.id,"feature":output,"title":format!("{}/{}",part.id,output),"bounds":[min,max],"mesh":mesh,"lines":lines,"parameters":controls,"annotations":annotations,"coordinate_system":"family-local mm","valid":session.is_valid(shape).map_err(|e|failure("visualization",e))?}),
    )
}
fn primitive_dimensions(
    feature: &FeatureDefinition,
    parameters: &HashMap<String, ParameterValue>,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let vec = |expression: &VectorExpr| {
        expression
            .evaluate(parameters)
            .map(|v| [v.x.value, v.y.value, v.z.value])
            .map_err(|e| model_failure("visualization", e))
    };
    let mut add =
        |label: &str, expression: &ScalarExpr, a: [f64; 3], b: [f64; 3]| -> Result<(), Failure> {
            let q = expression
                .evaluate(parameters)
                .map_err(|e| model_failure("visualization", e))?;
            annotations.push(annotation(
                format!("driving-{label}"),
                format!("{label} {} mm", length_label(q.value)),
                "dimension",
                "driving",
                vec![feature.id.clone()],
                expression
                    .parameter_names()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                json!([a, b]),
                json!({"feature":feature.id,"expression":expression,"value":q,"driving":true}),
            ));
            Ok(())
        };
    match &feature.operation {
        FeatureOperation::Box { origin, size } => {
            let a = vec(origin)?;
            let lengths = vec(size)?;
            let parameter_names =
                names(&serde_json::to_value(size).map_err(|e| failure("visualization", e))?);
            for axis in 0..3 {
                let mut b = a;
                b[axis] += lengths[axis];
                annotations.push(annotation(format!("driving-box-{axis}"),format!("{} {} mm",["width","depth","height"][axis],lengths[axis]),"dimension","driving",vec![feature.id.clone()],parameter_names.clone(),json!([a,b]),json!({"feature":feature.id,"expression":size,"component":axis,"value_mm":lengths[axis],"driving":true})));
            }
        }
        FeatureOperation::Sphere { center, radius } => {
            let a = vec(center)?;
            let r = radius
                .evaluate(parameters)
                .map_err(|e| model_failure("visualization", e))?
                .value;
            add("R", radius, a, [a[0] + r, a[1], a[2]])?;
        }
        FeatureOperation::Cylinder {
            origin,
            axis,
            radius,
            height,
        }
        | FeatureOperation::Cone {
            origin,
            axis,
            base_radius: radius,
            height,
            ..
        } => {
            let a = vec(origin)?;
            let axis = vec(axis)?;
            let scale = axis.into_iter().map(f64::abs).fold(0.0, f64::max);
            let n = axis.map(|x| x / scale);
            let length = n[0].hypot(n[1].hypot(n[2]));
            let n = n.map(|x| x / length);
            let h = height
                .evaluate(parameters)
                .map_err(|e| model_failure("visualization", e))?
                .value;
            let r = radius
                .evaluate(parameters)
                .map_err(|e| model_failure("visualization", e))?
                .value;
            let u = if n[0].abs() < 0.9 {
                [0.0, -n[2], n[1]]
            } else {
                [-n[1], n[0], 0.0]
            };
            let len = u[0].hypot(u[1].hypot(u[2]));
            let u = u.map(|x| x / len);
            add(
                "height",
                height,
                a,
                std::array::from_fn(|i| a[i] + n[i] * h),
            )?;
            if r > 0.0 {
                add("R", radius, a, std::array::from_fn(|i| a[i] + u[i] * r))?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn sketch_scene(
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
