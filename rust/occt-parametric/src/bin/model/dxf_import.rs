//! `occt-model --import-dxf REQUEST.json NEW_RESULT.json`: a DXF file's 2D
//! outlines as a sketch feature, with the import report and a check that the
//! face builds. Nothing is built or published beyond the result file.
use super::*;
use occt_parametric::dxf::{DxfImportOptions, MAX_DXF_BYTES, import_dxf};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write as _;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    /// The DXF file's text.
    dxf: String,
    /// Id of the returned `sketch_face` feature; default `outline`.
    #[serde(default = "default_feature")]
    feature_id: String,
    #[serde(default)]
    options: Option<DxfImportOptions>,
}

fn default_feature() -> String {
    "outline".into()
}

pub fn run(path: &OsString, destination: &OsString) -> Result<Value, Failure> {
    let size = fs::metadata(path).map_err(|e| failure("request", e))?.len();
    // The JSON-escaped text may be larger than the file it came from.
    if size > (2 * MAX_DXF_BYTES) as u64 {
        return Err(failure("request", "DXF import request is too large"));
    }
    let request: Request =
        serde_json::from_str(&fs::read_to_string(path).map_err(|e| failure("request", e))?)
            .map_err(|e| failure("request", e))?;
    if request.schema != "occb-dxf-import-v1" {
        return Err(failure("request", "expected occb-dxf-import-v1"));
    }
    if request.feature_id.is_empty() {
        return Err(failure("request", "feature_id must not be empty"));
    }
    let options = request.options.unwrap_or_default();
    let import = import_dxf(&request.dxf, &options).map_err(|e| model_failure("import", e))?;
    let feature = FeatureDefinition {
        id: request.feature_id,
        operation: FeatureOperation::SketchFace {
            sketch: Box::new(import.sketch),
        },
    };
    let face = check_face(&feature);
    let result = json!({
        "schema": "occb-dxf-import-report-v1",
        "status": "imported",
        "feature": feature,
        "report": import.report,
        "face": face,
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| failure("publication", e))?;
    file.write_all(
        serde_json::to_string(&result)
            .map_err(|e| failure("publication", e))?
            .as_bytes(),
    )
    .map_err(|e| failure("publication", e))?;
    Ok(result)
}

/// Builds the sketch face alone: its area and region count, or why it
/// failed (an open outline, for example). Kernel work is one sketch face.
fn check_face(feature: &FeatureDefinition) -> Value {
    let definition = FamilyDefinition {
        id: "dxf-import".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        features: vec![feature.clone()],
        requirements: vec![],
        datums: vec![],
        references: vec![],
        feature_colors: Default::default(),
        assumptions: vec![],
        history: Default::default(),
    };
    let result = (|| -> Result<Value, Box<dyn std::error::Error>> {
        let session = Session::new()?;
        let part = PartInstance {
            id: "import".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "dxf import".into(),
        };
        let generated = part.regenerate(&session)?;
        let shape = generated
            .shape(&feature.id)
            .ok_or("the sketch made no face")?;
        Ok(json!({
            "built": true,
            "area_mm2": session.surface_area(shape)?,
            "faces": session.subshape_count(shape, occt_bridge::ShapeType::Face)?,
        }))
    })();
    result.unwrap_or_else(|error| json!({"built": false, "error": error.to_string()}))
}
