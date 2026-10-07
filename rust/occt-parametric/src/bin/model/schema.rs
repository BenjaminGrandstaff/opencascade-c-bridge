//! Authoring schemas generated from the engine's serde-compatible Rust types.
use super::*;

pub fn document(name: &str) -> Option<Value> {
    let mut schema = match name {
        "request" => serde_json::to_value(schemars::schema_for!(Request)),
        "model" => serde_json::to_value(schemars::schema_for!(ModelDocument)),
        "feature" => serde_json::to_value(schemars::schema_for!(FeatureOperation)),
        "parameter" => serde_json::to_value(schemars::schema_for!(ParameterDefinition)),
        "sketch" => serde_json::to_value(schemars::schema_for!(SketchDefinition)),
        "requirement" => serde_json::to_value(schemars::schema_for!(Requirement)),
        "inspection" => serde_json::to_value(schemars::schema_for!(inspect::Inspection)),
        "face_selector" => serde_json::to_value(schemars::schema_for!(FaceSelector)),
        "edge_selector" => serde_json::to_value(schemars::schema_for!(EdgeSelector)),
        "edit" => serde_json::to_value(schemars::schema_for!(patch::PatchRequest)),
        "change" => serde_json::to_value(schemars::schema_for!(patch::Change)),
        "view" => serde_json::to_value(schemars::schema_for!(visualize::ViewRequest)),
        _ => return None,
    }
    .expect("JSON Schema is serializable");
    if name == "request" {
        schema["properties"]["schema"] = json!({"type":"string","const":"occb-model-request-v1"});
        schema["properties"]["outputs"]["minItems"] = json!(1);
        schema["properties"]["outputs"]["maxItems"] = json!(10_000);
        schema["properties"]["edits"]["maxItems"] = json!(10_000);
        schema["$defs"]["ModelDocument"]["properties"]["schema_version"] =
            json!({"type":"integer","const":CURRENT_SCHEMA_VERSION});
    } else if name == "view" {
        schema["properties"]["schema"] = json!({"type":"string","const":"occb-model-view-v1"});
        schema["$defs"]["ModelDocument"]["properties"]["schema_version"] =
            json!({"type":"integer","const":CURRENT_SCHEMA_VERSION});
    } else if name == "edit" {
        schema["properties"]["schema"] = json!({"type":"string","const":"occb-model-edit-v1"});
        schema["properties"]["changes"]["maxItems"] = json!(10000);
        schema["properties"]["edits"]["maxItems"] = json!(10000);
        schema["properties"]["outputs"]["minItems"] = json!(1);
        schema["properties"]["outputs"]["maxItems"] = json!(10000);
        schema["$defs"]["ModelDocument"]["properties"]["schema_version"] =
            json!({"type":"integer","const":CURRENT_SCHEMA_VERSION});
    } else if name == "inspection" {
        schema["properties"]["schema"] =
            json!({"type":"string","const":"occb-model-inspection-v1"});
        schema["properties"]["limit"]["minimum"] = json!(1);
        schema["properties"]["limit"]["maximum"] = json!(1000);
        schema["$defs"]["ModelDocument"]["properties"]["schema_version"] =
            json!({"type":"integer","const":CURRENT_SCHEMA_VERSION});
    } else if name == "model" {
        schema["properties"]["schema_version"] =
            json!({"type":"integer","const":CURRENT_SCHEMA_VERSION});
    }
    Some(schema)
}
