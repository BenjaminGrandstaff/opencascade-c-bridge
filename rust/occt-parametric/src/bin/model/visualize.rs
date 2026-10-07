//! Diagnostic annotated sketch/solid viewer; distinct from accepted generation.
use super::*;
pub use view_data::ViewOptions;
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewRequest {
    schema: String,
    model: ModelDocument,
    #[serde(default)]
    outputs: Vec<InstanceOutputRef>,
    #[serde(default = "yes")]
    sketches: bool,
    #[serde(default)]
    options: ViewOptions,
}
pub fn run(path: &OsString, destination: &OsString) -> Result<Value, Failure> {
    let mut raw: Value =
        serde_json::from_str(&fs::read_to_string(path).map_err(|e| failure("request", e))?)
            .map_err(|e| failure("request", e))?;
    let model = ModelDocument::from_json(
        &raw.get("model")
            .ok_or_else(|| failure("request", "model is required"))?
            .to_string(),
    )
    .map_err(|e| model_failure("validation", e))?;
    raw["model"] = serde_json::to_value(model).map_err(|e| failure("request", e))?;
    let request: ViewRequest = serde_json::from_value(raw).map_err(|e| failure("request", e))?;
    if request.schema != "occb-model-view-v1" {
        return Err(failure("request", "expected occb-model-view-v1"));
    }
    write(
        &request.model,
        &request.outputs,
        request.sketches,
        &request.options,
        Path::new(destination),
        true,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn write(
    model: &ModelDocument,
    outputs: &[InstanceOutputRef],
    sketches: bool,
    options: &ViewOptions,
    destination: &Path,
    new_directory: bool,
) -> Result<Value, Failure> {
    if new_directory
        && destination
            .try_exists()
            .map_err(|e| failure("publication", e))?
    {
        return Err(failure(
            "publication",
            "visualization directory already exists",
        ));
    }
    let data = view_data::collect(model, outputs, sketches, options).map_err(|error| Failure {
        stage: error.stage,
        message: error.message,
        diagnostics: error.diagnostics,
    })?;
    let scenes = data["scenes"].as_array().expect("collected scenes");
    let json = serde_json::to_string(&data).map_err(|e| failure("publication", e))?;
    let safe = json
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    let html = include_str!("../../../../../tools/model/viewer/viewer.html")
        .replace(
            "/*VIEWER_PANEL*/",
            include_str!("../../../../../tools/model/viewer/panel.html"),
        )
        .replace(
            "/*VIEWER_STYLE*/",
            include_str!("../../../../../tools/model/viewer/viewer.css"),
        )
        .replace(
            "/*VIEWER_SCRIPT*/",
            include_str!("../../../../../tools/model/viewer/viewer.js"),
        )
        .replace("VIEWER_DATA", &safe);
    if new_directory {
        fs::create_dir(destination).map_err(|e| failure("publication", e))?;
    }
    let published = (|| {
        fs::write(destination.join("viewer.html"), html).map_err(|e| failure("publication", e))?;
        let mut snapshots = Vec::new();
        for (index, scene) in scenes.iter().enumerate() {
            let name = format!("view-{:04}.svg", index + 1);
            fs::write(destination.join(&name), view_svg::render(scene))
                .map_err(|e| failure("publication", e))?;
            snapshots.push(name);
        }
        fs::write(destination.join("view.json"), &json).map_err(|e| failure("publication", e))?;
        let report = json!({"schema":"occb-model-view-report-v1","status":"visualized","diagnostic":true,"scenes":scenes.len(),"outputs":outputs,"snapshots":snapshots,"artifacts":{"viewer":"viewer.html","view_data":"view.json","model":"model.json"}});
        let mut report = report;
        for (index, name) in snapshots.iter().enumerate() {
            report["artifacts"][format!("snapshot_{}", index + 1)] = json!(name);
        }
        if new_directory {
            fs::write(
                destination.join("model.json"),
                model
                    .to_json_pretty()
                    .map_err(|e| model_failure("validation", e))?,
            )
            .map_err(|e| failure("publication", e))?;
            fs::write(
                destination.join("report.json"),
                serde_json::to_string_pretty(&report).map_err(|e| failure("publication", e))?,
            )
            .map_err(|e| failure("publication", e))?;
        }
        Ok(report)
    })();
    if new_directory && published.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    published
}
