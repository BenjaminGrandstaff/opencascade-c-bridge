//! One-shot, machine-readable model build, parameter edit, and export workflow.
#[path = "model/inspect.rs"]
mod inspect;
#[path = "model/patch.rs"]
mod patch;
#[path = "model/preview.rs"]
mod preview;
#[path = "model/report.rs"]
mod report;
#[path = "model/schema.rs"]
mod schema;
#[path = "model/view_data.rs"]
mod view_data;
#[path = "model/view_svg.rs"]
mod view_svg;
#[path = "model/visualize.rs"]
mod visualize;
use occt_bridge::{Session, StlOptions};
use occt_parametric::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashSet, error::Error, ffi::OsString, fs, path::Path, process::ExitCode};

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    model: ModelDocument,
    outputs: Vec<InstanceOutputRef>,
    #[serde(default)]
    edits: Vec<Edit>,
    #[serde(default = "yes")]
    step: bool,
    #[serde(default)]
    stl: bool,
    #[serde(default = "yes")]
    preview: bool,
}
fn yes() -> bool {
    true
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Edit {
    instance: String,
    parameter: String,
    value: ParameterValue,
}

#[derive(Debug)]
struct Failure {
    stage: &'static str,
    message: String,
    diagnostics: Value,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.stage, self.message)
    }
}
impl Error for Failure {}
fn failure(stage: &'static str, error: impl std::fmt::Display) -> Failure {
    Failure {
        stage,
        message: error.to_string(),
        diagnostics: json!([]),
    }
}
fn model_failure(stage: &'static str, error: ModelError) -> Failure {
    Failure {
        stage,
        message: error.message,
        diagnostics: report::diagnostics(&error.diagnostics),
    }
}
fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if let [flag, name] = args.as_slice()
        && flag == "--schema"
    {
        return match name.to_str().and_then(schema::document) {
            Some(document) => {
                println!("{document}");
                ExitCode::SUCCESS
            }
            None => {
                eprintln!(
                    "unknown schema; choose request, model, feature, parameter, sketch, requirement, inspection, face_selector, edge_selector, edit, change, or view"
                );
                ExitCode::FAILURE
            }
        };
    }
    let outcome = match args.as_slice() {
        [flag, request, destination] if flag == "--inspect" => inspect::run(request, destination),
        [flag, request, destination] if flag == "--edit" => patch::run(request, destination),
        [flag, request, destination] if flag == "--visualize" => {
            visualize::run(request, destination)
        }
        _ => run(&args),
    };
    match outcome {
        Ok(value) => {
            eprintln!("{value}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "{}",
                json!({"schema":"occb-model-report-v1","status":"failed","stage":error.stage,"message":error.message,"diagnostics":error.diagnostics})
            );
            ExitCode::FAILURE
        }
    }
}
fn run(args: &[OsString]) -> Result<Value, Failure> {
    let [request, destination] = args else {
        return Err(failure(
            "request",
            "usage: occt-model REQUEST.json NEW_OUTPUT_DIRECTORY",
        ));
    };
    let destination = Path::new(destination);
    if destination
        .try_exists()
        .map_err(|e| failure("publication", e))?
    {
        return Err(failure(
            "publication",
            "output directory already exists; choose a new directory",
        ));
    }
    let source = fs::read_to_string(request).map_err(|e| failure("request", e))?;
    // Decode the nested model through the migration/validation API, rather than
    // accepting an unchecked current/future schema via plain serde.
    let mut raw: Value = serde_json::from_str(&source).map_err(|e| failure("request", e))?;
    let model = ModelDocument::from_json(
        &raw.get("model")
            .ok_or_else(|| failure("request", "model is required"))?
            .to_string(),
    )
    .map_err(|e| model_failure("validation", e))?;
    raw["model"] = serde_json::to_value(model).map_err(|e| failure("request", e))?;
    let request: Request = serde_json::from_value(raw).map_err(|e| failure("request", e))?;
    build(request, destination)
}
fn build(request: Request, destination: &Path) -> Result<Value, Failure> {
    build_internal(request, destination, true)
}
// Edits defer their success marker until the revision and source metadata are saved.
fn build_internal(
    request: Request,
    destination: &Path,
    write_report: bool,
) -> Result<Value, Failure> {
    if request.schema != "occb-model-request-v1" {
        return Err(failure("request", "expected occb-model-request-v1"));
    }
    if request.outputs.is_empty() || request.outputs.len() > 10_000 || request.edits.len() > 10_000
    {
        return Err(failure(
            "request",
            "select 1..10000 outputs and at most 10000 parameter edits",
        ));
    }
    let mut selected = HashSet::new();
    for output in &request.outputs {
        if !selected.insert(&output.instance) {
            return Err(failure("request", "select at most one output per instance"));
        }
    }
    let document = request.model;
    let mut graph = document
        .instance_graph()
        .map_err(|e| model_failure("validation", e))?;
    let mut edits = HashSet::new();
    for edit in &request.edits {
        if !edits.insert((&edit.instance, &edit.parameter)) {
            return Err(failure("edits", "duplicate instance/parameter edit"));
        }
        graph
            .set_override(&edit.instance, &edit.parameter, edit.value.clone())
            .map_err(|e| model_failure("edits", e))?;
    }
    let session = Session::new().map_err(|e| failure("kernel", e))?;
    let generation = graph
        .regenerate_all(&session)
        .map_err(|e| model_failure("regeneration", e))?;
    let mut outputs = Vec::new();
    for output in &request.outputs {
        let shape = generation
            .result(&output.instance)
            .and_then(|r| r.shape(&output.output))
            .ok_or_else(|| {
                failure(
                    "selection",
                    format!(
                        "missing generated output '{}/{}'",
                        output.instance, output.output
                    ),
                )
            })?;
        let bounds = session
            .exact_bounds(shape)
            .map_err(|e| failure("inspection", e))?;
        outputs.push(json!({"instance":output.instance,"output":output.output,
            "valid":session.is_valid(shape).map_err(|e| failure("inspection", e))?,
            "volume_mm3":session.volume(shape).map_err(|e| failure("inspection", e))?,
            "bounds_mm":{"min":[bounds.min.x,bounds.min.y,bounds.min.z],"max":[bounds.max.x,bounds.max.y,bounds.max.z]}}));
    }
    // Preserve authoring metadata, drawings, and revision history while saving
    // edited overrides and refreshed driven patterns/instances from the graph.
    let refreshed = ModelDocument::from_graph(&graph);
    let mut edited_document = document.clone();
    edited_document.instances = refreshed.instances;
    edited_document.patterns = refreshed.patterns;
    edited_document.generation_records.clear();
    let persisted = edited_document
        .to_json_pretty()
        .map_err(|e| model_failure("validation", e))?;
    let mut value = json!({"schema":"occb-model-report-v1","status":"built",
        "model_schema":CURRENT_SCHEMA_VERSION,"generated_variants":generation.generated_variants(),
        "outputs":outputs,"verification":report::verification(&generation),"artifacts":{"model":"model.json"},
        "preview_quality":"sampled; visual review only"});
    // Exclusive creation prevents overwriting previous accepted builds. On any
    // write failure remove only the directory this invocation created.
    fs::create_dir(destination).map_err(|e| failure("publication", e))?;
    let published = publish(
        &request.outputs,
        request.step,
        request.stl,
        request.preview,
        destination,
        &session,
        &graph,
        &generation,
        &persisted,
        &mut value,
        write_report && !request.preview,
    );
    if published.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    published?;
    if request.preview {
        let annotated = visualize::write(
            &edited_document,
            &request.outputs,
            true,
            &visualize::ViewOptions::default(),
            destination,
            false,
        );
        match annotated {
            Ok(annotated) => {
                for (key, path) in annotated["artifacts"].as_object().unwrap() {
                    if key != "model" {
                        value["artifacts"][key] = path.clone();
                    }
                }
                value["artifacts"]["viewer"] = json!("viewer.html");
                value["artifacts"]["view_data"] = json!("view.json");
                if write_report {
                    fs::write(
                        destination.join("report.json"),
                        serde_json::to_string_pretty(&value)
                            .map_err(|e| failure("publication", e))?,
                    )
                    .map_err(|e| failure("publication", e))?;
                }
            }
            Err(error) => {
                let _ = fs::remove_dir_all(destination);
                return Err(error);
            }
        }
    }
    Ok(value)
}
#[allow(clippy::too_many_arguments)]
fn publish(
    outputs: &[InstanceOutputRef],
    step: bool,
    stl: bool,
    previews: bool,
    destination: &Path,
    session: &Session,
    graph: &InstanceGraph<'_>,
    generation: &GraphRegeneration<'_>,
    persisted: &str,
    value: &mut Value,
    write_report: bool,
) -> Result<(), Failure> {
    if step {
        graph
            .export_step(
                session,
                generation,
                destination.join("parts.step"),
                "AI model",
                &OutputSet::Explicit(outputs.to_vec()),
            )
            .map_err(|e| model_failure("export", e))?;
        value["artifacts"]["step"] = json!("parts.step");
    }
    for (index, output) in outputs.iter().enumerate() {
        let shape = generation
            .result(&output.instance)
            .unwrap()
            .shape(&output.output)
            .unwrap();
        if stl {
            let filename = format!("{:04}.stl", index + 1);
            session
                .save_stl(shape, destination.join(&filename), StlOptions::default())
                .map_err(|e| failure("export", e))?;
            value["outputs"][index]["stl"] = json!(filename);
        }
        if previews {
            let filename = format!("{:04}.svg", index + 1);
            let svg = preview::svg(session, shape).map_err(|e| failure("preview", e))?;
            fs::write(destination.join(&filename), svg).map_err(|e| failure("publication", e))?;
            value["outputs"][index]["preview"] = json!(filename);
        }
    }
    fs::write(destination.join("model.json"), persisted).map_err(|e| failure("publication", e))?;
    if write_report {
        fs::write(
            destination.join("report.json"),
            serde_json::to_string_pretty(value).map_err(|e| failure("publication", e))?,
        )
        .map_err(|e| failure("publication", e))?;
    }
    Ok(())
}
#[cfg(test)]
#[path = "model/tests.rs"]
mod tests;
