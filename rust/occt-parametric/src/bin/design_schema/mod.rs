//! Complete MCP requests generated from the same setup types the commands read.
use occt_parametric::{CURRENT_SCHEMA_VERSION, ModelDocument};
use schemars::JsonSchema;
use serde_json::json;

#[derive(JsonSchema)]
#[schemars(deny_unknown_fields)]
#[allow(dead_code)]
struct Request<T: JsonSchema> {
    schema: String,
    model: ModelDocument,
    setup: T,
}

pub fn print<T: JsonSchema>(command: &str) -> bool {
    if std::env::args_os().skip(1).collect::<Vec<_>>() != ["--schema"] {
        return false;
    }
    let mut schema = serde_json::to_value(schemars::schema_for!(Request<T>)).unwrap();
    schema["properties"]["schema"] =
        json!({"type":"string","const":format!("occb-{command}-request-v1")});
    schema["$defs"]["ModelDocument"]["properties"]["schema_version"] =
        json!({"type":"integer","const":CURRENT_SCHEMA_VERSION});
    // Setup discriminators are checked by the command as well as discovery.
    let setup = schema["properties"]["setup"]["$ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let setup = setup.strip_prefix("#/$defs/").unwrap();
    let tag = if command == "inspection-report" {
        "occb-inspection-setup-v1".to_owned()
    } else {
        format!("occb-{command}-v1")
    };
    schema["$defs"][setup]["properties"]["schema"] = json!({"type":"string","const":tag});
    // MCP accepts an inline chord only; wing_project would read a caller path.
    if command == "balance-report" {
        schema["$defs"]["Reference"]["oneOf"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v["properties"]["kind"]["const"] == "chord");
    }
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    true
}
