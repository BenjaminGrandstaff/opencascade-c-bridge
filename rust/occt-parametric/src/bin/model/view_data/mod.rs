//! Shared native dimension and constraint scenes for live and standalone viewers.
use occt_bridge::{BridgeError, MeshOptions, Session, Shape, ShapeType, Vec3};
use occt_parametric::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

mod dimensions;
mod features;
mod sketch;
mod solid;

use dimensions::helix_route;
pub(crate) use dimensions::helix_samples;
use sketch::sketch_scene;
use solid::solid_scene;

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
    let diagnostics = error
        .diagnostics
        .iter()
        .map(|d| {
            json!({
                "feature": d.feature,
                "kind": format!("{:?}",d.kind),
                "code": d.code,
                "name": d.name,
                "selection": d.selection,
                "selector": d.selector,
                "input": d.input,
            })
        })
        .collect::<Vec<_>>();
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
/// One annotation shown on a scene: a dimension, constraint, control, or
/// check, with the geometry it targets and the parameters that drive it.
#[derive(Serialize)]
struct Annotation {
    id: String,
    label: String,
    kind: AnnotationKind,
    status: AnnotationStatus,
    /// Output or sketch entity ids the annotation highlights.
    targets: Vec<String>,
    /// Parameter names whose controls are linked to the annotation.
    parameters: Vec<String>,
    /// Points in scene coordinates: a leader anchor, a measured segment, or
    /// an angular arc's ends.
    anchors: Value,
    detail: Value,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum AnnotationKind {
    Dimension,
    Constraint,
    Parameter,
    Requirement,
    Group,
    ProfileOperation,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum AnnotationStatus {
    /// Set by a parameter or definition value.
    Driving,
    /// Measured on generated geometry.
    Measured,
    /// Computed from other driving values.
    Derived,
    Fixed,
    Initial,
    Constructed,
    Passed,
    Failed,
    Unverified,
}

impl From<Annotation> for Value {
    fn from(annotation: Annotation) -> Self {
        serde_json::to_value(annotation).expect("annotations serialize to JSON")
    }
}

/// Converts a kernel, model, or JSON error into a [`Failure`] at a stage.
/// Model errors keep their feature diagnostics.
trait StageResult<T> {
    fn stage(self, stage: &'static str) -> Result<T, Failure>;
}

impl<T> StageResult<T> for Result<T, BridgeError> {
    fn stage(self, stage: &'static str) -> Result<T, Failure> {
        self.map_err(|e| failure(stage, e))
    }
}

impl<T> StageResult<T> for Result<T, serde_json::Error> {
    fn stage(self, stage: &'static str) -> Result<T, Failure> {
        self.map_err(|e| failure(stage, e))
    }
}

impl<T> StageResult<T> for Result<T, ModelError> {
    fn stage(self, stage: &'static str) -> Result<T, Failure> {
        self.map_err(|e| model_failure(stage, e))
    }
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
    let graph = model.instance_graph().stage("validation")?;
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
    let session = Session::new().stage("kernel")?;
    let mut budget = Budget {
        triangles: options.maximum_triangles,
        vertices: options.maximum_vertices,
        annotations: options.maximum_annotations,
    };
    let mut scenes = Vec::new();
    let mut scene_ids = BTreeSet::new();
    for id in &ids {
        let part = graph.resolve(id).stage("resolution")?;
        let parameters = part.resolved_parameters().stage("resolution")?;
        let controls = parameters
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    json!({
                        "value": value,
                        "definition": part.definition.parameters.iter().find(|p|p.id==*name),
                    }),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let selected = outputs
            .iter()
            .filter(|o| o.instance == *id)
            .collect::<Vec<_>>();
        let needs_linked_sketches = sketches && part.definition.features.iter().any(|f| matches!(
            &f.operation, FeatureOperation::SketchFace { sketch } | FeatureOperation::SketchWire { sketch }
                | FeatureOperation::SketchOpenWire { sketch } if sketch.face_support.is_some() || !sketch.projections.is_empty()));
        let diagnostic = if !selected.is_empty() || needs_linked_sketches {
            Some(part.diagnostic_geometry(&session))
        } else {
            None
        };
        let resolved_sketches = match diagnostic.as_ref() {
            Some(Ok(diagnostic)) if needs_linked_sketches => part
                .resolved_sketches(&session, &diagnostic.generated)
                .stage("visualization")?,
            _ => HashMap::new(),
        };
        if !selected.is_empty() {
            match diagnostic.as_ref().expect("selected outputs need geometry") {
                Ok(diagnostic) => {
                    for output in selected {
                        if !scene_ids.insert((id.clone(), output.output.clone())) {
                            return Err(failure("request", "duplicate visualization output"));
                        }
                        let scene = solid_scene(
                            &session,
                            &part,
                            diagnostic,
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
                        scenes.push(json!({
                            "kind": "solid",
                            "instance": id,
                            "feature": output.output,
                            "title": format!("{id}/{}",output.output),
                            "error": error.message,
                            "annotations": [],
                            "parameters": controls,
                        }));
                    }
                }
            }
        }
        if sketches {
            let source_controls: HashMap<_, _> = if needs_linked_sketches {
                part.definition
                    .features
                    .iter()
                    .map(|f| (f.id.as_str(), names(&json!(f.operation))))
                    .collect()
            } else {
                HashMap::new()
            };
            for feature in &part.definition.features {
                let sketch = match &feature.operation {
                    FeatureOperation::SketchFace { sketch }
                    | FeatureOperation::SketchWire { sketch }
                    | FeatureOperation::SketchOpenWire { sketch } => sketch,
                    _ => continue,
                };
                let support = sketch.face_support.as_ref().map(|definition| match resolved_sketches.get(&feature.id) {
                    Some(Ok(ResolvedSketch { plane:Some(ResolvedDatum::Plane { origin, normal }), .. })) => json!({"definition":definition,"origin_mm":point(*origin),"normal":point(*normal),"status":"resolved","coordinate_system":"family-local millimeters"}),
                    Some(Err(error)) => json!({"definition":definition,"status":"failed","error":error.message}),
                    _ => json!({"definition":definition,"status":"unavailable","error":diagnostic.as_ref().and_then(|d| d.as_ref().err()).map(|e| e.message.as_str())}),
                });
                let projected=sketch.projections.iter().map(|projection| {
                    let mut linked:BTreeSet<String>=names(&json!(projection)).into_iter().collect();
                    linked.extend(source_controls.get(projection.input.as_str()).into_iter().flatten().cloned());
                    json!({"definition":projection,"source_parameters":linked,"status":"resolved"})
                }).collect();
                let resolved = if sketch.projections.is_empty() {
                    sketch.as_ref()
                } else {
                    match resolved_sketches.get(&feature.id) {
                        Some(Ok(resolved)) => &resolved.sketch,
                        resolution => {
                            let error = match resolution {
                                Some(Err(error)) => error.message.clone(),
                                _ => diagnostic
                                    .as_ref()
                                    .and_then(|d| d.as_ref().err())
                                    .map(|e| e.message.clone())
                                    .unwrap_or_else(|| {
                                        "projected source geometry unavailable".into()
                                    }),
                            };
                            scenes.push(json!({"kind":"sketch","instance":id,"feature":feature.id,"title":format!("{id}/{}",feature.id),"error":error,"projections":sketch.projections,"face_support":support,"annotations":[],"parameters":controls}));
                            continue;
                        }
                    }
                };
                match sketch_scene(
                    &session,
                    id,
                    &feature.id,
                    resolved,
                    !matches!(feature.operation, FeatureOperation::SketchOpenWire { .. }),
                    &parameters,
                    &controls,
                    support,
                    projected,
                    &mut budget,
                ) {
                    Ok(scene) => scenes.push(scene),
                    Err(error) if error.stage == "sketch" => scenes.push(json!({
                        "kind": "sketch",
                        "instance": id,
                        "feature": feature.id,
                        "title": format!("{id}/{}",feature.id),
                        "error": error.message,
                        "annotations": [],
                        "parameters": controls,
                    })),
                    Err(error) => return Err(error),
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
    Ok(json!({
        "schema": "occb-annotated-view-v1",
        "diagnostic": true,
        "coordinate_system": "family-local millimeters",
        "scenes": scenes,
    }))
}
pub fn verification_result(scope: &str, r: &VerificationResult) -> Value {
    let measured = r.measured.map(|m| {
        json!({
            "value": m.value,
            "unit": format!("{:?}",m.unit),
            "minimum": m.minimum,
            "maximum": m.maximum,
        })
    });
    let evidence = match r.evidence {
        Evidence::Exact => json!({"kind":"exact"}),
        Evidence::Sampled {
            samples,
            unresolved,
        } => json!({
            "kind": "sampled",
            "samples": samples,
            "unresolved": unresolved,
        }),
    };
    let witness = r.witness.as_ref().map(|w| {
        json!({
            "subjects": w.subjects,
            "points_mm": w.points_mm.iter().map(|p| [p.x,p.y,p.z]).collect::<Vec<_>>(),
        })
    });
    json!({
        "scope": scope,
        "requirement": r.requirement_id,
        "status": if r.status == VerificationStatus::Passed {"passed"} else {"failed"},
        "message": r.message,
        "measured": measured,
        "evidence": evidence,
        "witness": witness,
    })
}
