//! Design templates in the studio: the `occb-model-template-v1` files of
//! `tools/model/templates`, built into the binary, listed and instantiated
//! exactly as `tools/model/templates.py` does. Starting from a template
//! replaces the studio's document as one undoable edit; nothing is written
//! until Save.
use super::*;

/// The template files, by id.
pub(super) const TEMPLATES: [(&str, &str); 4] = [
    (
        "l-bracket",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/model/templates/l-bracket.template.json"
        )),
    ),
    (
        "printed-enclosure",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/model/templates/printed-enclosure.template.json"
        )),
    ),
    (
        "shaft",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/model/templates/shaft.template.json"
        )),
    ),
    (
        "sheet-metal-channel",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/model/templates/sheet-metal-channel.template.json"
        )),
    ),
];

fn template(id: &str) -> Result<Value, String> {
    let (_, text) = TEMPLATES
        .iter()
        .find(|(name, _)| *name == id)
        .ok_or_else(|| format!("Unknown template '{id}'."))?;
    serde_json::from_str(text).map_err(|e| format!("template '{id}': {e}"))
}

/// What a client needs to choose a template and fill its inputs.
pub(super) fn summaries() -> Result<Value, String> {
    TEMPLATES
        .iter()
        .map(|(id, _)| {
            let t = template(id)?;
            let keys = [
                "id", "version", "title", "summary", "use_when", "not_for", "inputs", "checks",
            ];
            Ok(Value::Object(
                keys.iter()
                    .filter_map(|key| t.get(*key).map(|v| ((*key).to_owned(), v.clone())))
                    .collect(),
            ))
        })
        .collect::<Result<Vec<_>, String>>()
        .map(Value::Array)
}

/// A number as Python keeps it: integers stay integers until a float or a
/// `$sum` (which uses `math.fsum`) makes them floats.
#[derive(Clone, Copy)]
enum Number {
    Int(i64),
    Float(f64),
}
impl Number {
    fn of(value: &Value) -> Option<Self> {
        match value {
            Value::Number(n) if n.is_i64() => n.as_i64().map(Self::Int),
            Value::Number(n) => n.as_f64().map(Self::Float),
            _ => None,
        }
    }
    fn float(self) -> f64 {
        match self {
            Self::Int(v) => v as f64,
            Self::Float(v) => v,
        }
    }
    fn times(self, other: Self) -> Self {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a
                .checked_mul(b)
                .map_or(Self::Float(a as f64 * b as f64), Self::Int),
            _ => Self::Float(self.float() * other.float()),
        }
    }
    fn value(self) -> Value {
        match self {
            Self::Int(v) => json!(v),
            Self::Float(v) => json!(v),
        }
    }
}

struct Inputs {
    values: serde_json::Map<String, Value>,
    kinds: HashMap<String, String>,
}

/// Every input's value (defaults for those not given), checked against its
/// declaration as `resolve_inputs` does.
fn resolve(template: &Value, given: &Value) -> Result<Inputs, String> {
    let given = given
        .as_object()
        .ok_or("inputs must be an object of input id to value")?;
    let declared = template["inputs"]
        .as_array()
        .ok_or("template has no inputs")?;
    let ids: Vec<&str> = declared.iter().filter_map(|i| i["id"].as_str()).collect();
    let mut unknown: Vec<&str> = given
        .keys()
        .map(String::as_str)
        .filter(|k| !ids.contains(k))
        .collect();
    unknown.sort();
    if !unknown.is_empty() {
        return Err(format!(
            "unknown input(s) {}; declared: {}",
            unknown.join(", "),
            ids.join(", ")
        ));
    }
    let mut inputs = Inputs {
        values: serde_json::Map::new(),
        kinds: HashMap::new(),
    };
    for item in declared {
        let name = item["id"].as_str().unwrap_or_default();
        let kind = item["kind"].as_str().unwrap_or_default();
        let value = given.get(name).unwrap_or(&item["default"]).clone();
        if kind == "choice" {
            let choices = item["choices"].as_array().cloned().unwrap_or_default();
            if !choices.contains(&value) {
                let names: Vec<String> = choices
                    .iter()
                    .map(|c| c.as_str().map_or_else(|| c.to_string(), str::to_owned))
                    .collect();
                return Err(format!("{name} must be one of {}", names.join(", ")));
            }
        } else {
            let number = Number::of(&value)
                .map(Number::float)
                .filter(|v| v.is_finite())
                .ok_or_else(|| {
                    format!(
                        "{name} must be a finite number{}",
                        if kind == "length" {
                            " in millimeters"
                        } else {
                            ""
                        }
                    )
                })?;
            let (low, high) = (item["minimum"].as_f64(), item["maximum"].as_f64());
            if low.is_some_and(|low| number < low) || high.is_some_and(|high| number > high) {
                let show = |v: &Value| {
                    if v.is_null() {
                        "None".to_owned()
                    } else {
                        v.to_string()
                    }
                };
                return Err(format!(
                    "{name} must be in {}..{}, got {value}",
                    show(&item["minimum"]),
                    show(&item["maximum"])
                ));
            }
            if let Some(allowed) = item["allowed"].as_array()
                && !allowed.iter().any(|a| a.as_f64() == Some(number))
            {
                let names: Vec<String> = allowed.iter().map(Value::to_string).collect();
                return Err(format!("{name} must be one of {}", names.join(", ")));
            }
        }
        inputs.values.insert(name.to_owned(), value);
        inputs.kinds.insert(name.to_owned(), kind.to_owned());
    }
    Ok(inputs)
}

fn placeholder(node: &serde_json::Map<String, Value>, inputs: &Inputs) -> Result<Value, String> {
    let (value, kind) = if let Some(name) = node.get("$input") {
        let name = name.as_str().unwrap_or_default();
        let value = inputs
            .values
            .get(name)
            .ok_or_else(|| format!("placeholder names undeclared input '{name}'"))?;
        let value = match Number::of(value) {
            Some(n) => n
                .times(
                    node.get("scale")
                        .and_then(Number::of)
                        .unwrap_or(Number::Int(1)),
                )
                .value(),
            None => value.clone(),
        };
        (value, inputs.kinds[name].clone())
    } else {
        let product = node.contains_key("$product");
        let items = node[if product { "$product" } else { "$sum" }]
            .as_array()
            .ok_or("arithmetic placeholder needs a list")?
            .iter()
            .map(|item| number(item, inputs))
            .collect::<Result<Vec<_>, _>>()?;
        let value = if product {
            items.into_iter().fold(Number::Int(1), Number::times)
        } else {
            Number::Float(items.iter().map(|n| n.float()).sum())
        };
        (
            value.value(),
            node.get("kind")
                .and_then(Value::as_str)
                .unwrap_or("number")
                .to_owned(),
        )
    };
    Ok(match node.get("as").and_then(Value::as_str) {
        Some("quantity") if kind == "length" => {
            json!({"value": value, "dimension": "length", "unit": "millimeter"})
        }
        Some("quantity") => json!({"value": value, "dimension": "scalar", "unit": null}),
        _ => value,
    })
}

fn number(item: &Value, inputs: &Inputs) -> Result<Number, String> {
    if let Some(n) = Number::of(item) {
        return Ok(n);
    }
    if let Value::Object(node) = item {
        let mut node = node.clone();
        node.insert("as".into(), json!("number"));
        if let Some(n) = Number::of(&placeholder(&node, inputs)?) {
            return Ok(n);
        }
    }
    Err(format!("arithmetic placeholder needs numbers, got {item}"))
}

fn substitute(node: &Value, inputs: &Inputs) -> Result<Value, String> {
    Ok(match node {
        Value::Object(map)
            if map.contains_key("$input")
                || map.contains_key("$product")
                || map.contains_key("$sum") =>
        {
            placeholder(map, inputs)?
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), substitute(v, inputs)?)))
                .collect::<Result<_, String>>()?,
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| substitute(v, inputs))
                .collect::<Result<_, String>>()?,
        ),
        other => other.clone(),
    })
}

/// Python's `json.dumps(values, sort_keys=True)`: ", " and ": " separators.
fn python_json(values: &serde_json::Map<String, Value>) -> String {
    let mut keys: Vec<&String> = values.keys().collect();
    keys.sort();
    let items: Vec<String> = keys
        .into_iter()
        .map(|k| format!("{}: {}", json!(k), values[k]))
        .collect();
    format!("{{{}}}", items.join(", "))
}

/// A complete build request from the template `id` and its inputs, with
/// the inputs recorded as the family assumption `template-inputs`.
pub(super) fn instantiate(id: &str, given: &Value) -> Result<Value, String> {
    let template = template(id)?;
    let inputs = resolve(&template, given)?;
    let mut request = substitute(&template["request"], &inputs)?;
    let assumptions = request["model"]["family"]
        .as_object_mut()
        .ok_or("template request has no family")?
        .entry("assumptions")
        .or_insert_with(|| json!([]));
    assumptions
        .as_array_mut()
        .ok_or("template assumptions must be a list")?
        .push(json!({
            "id": "template-inputs",
            "statement": format!(
                "Instantiated from template {} v{} with inputs {}",
                template["id"].as_str().unwrap_or(id),
                template["version"],
                python_json(&inputs.values)
            ),
            "provenance": "template inputs; values are the requester's design assumptions",
        }));
    Ok(request)
}

impl Studio {
    /// Previews or starts the studio's document from a template.
    pub(super) fn start_template(&mut self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            version: u64,
            template: String,
            #[serde(default = "empty")]
            inputs: Value,
            apply: bool,
        }
        fn empty() -> Value {
            json!({})
        }
        let request: Request =
            serde_json::from_slice(body).map_err(|e| format!("invalid template request: {e}"))?;
        if request.version != self.version {
            return Err("The model changed. Reload before starting from a template.".into());
        }
        let built = instantiate(&request.template, &request.inputs)?;
        let output = built["outputs"][0]["output"]
            .as_str()
            .ok_or("template request selects no output")?
            .to_owned();
        let document = ModelDocument::from_json(&built["model"].to_string())
            .map_err(|e| format!("template model: {e}"))?;
        match build(&document, Some(&output)) {
            Ok((gltf, requirements)) if request.apply => {
                self.accept(document, gltf, requirements, Some(output.clone()))?;
                Ok(json!({"applied": true, "output": output, "state": self.state()}))
            }
            Ok((gltf, requirements)) => {
                let gltf: Value = serde_json::from_str(&gltf).map_err(|e| e.to_string())?;
                Ok(
                    json!({"applied": false, "can_apply": true, "version": self.version,
                    "template": request.template, "output": output, "requirements": requirements, "gltf": gltf}),
                )
            }
            Err(error) if request.apply => Err(error),
            Err(error) => Ok(
                json!({"applied": false, "can_apply": false, "version": self.version,
                "template": request.template, "output": output, "error": error}),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/model/templates");

    /// The Python reference's output for the same template and inputs.
    fn python(id: &str, inputs: &Value) -> Value {
        let path =
            std::env::temp_dir().join(format!("occb-template-{}-{id}.json", std::process::id()));
        fs::write(&path, inputs.to_string()).unwrap();
        let output = std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tools/model/templates.py"
            ))
            .args(["instantiate", id])
            .arg(&path)
            .output()
            .expect("python3 runs the reference template engine");
        let _ = fs::remove_file(&path);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    #[test]
    fn every_template_file_is_built_in() {
        let mut files: Vec<String> = fs::read_dir(DIRECTORY)
            .unwrap()
            .filter_map(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_str()?
                    .strip_suffix(".template.json")
                    .map(str::to_owned)
            })
            .collect();
        files.sort();
        let built: Vec<&str> = TEMPLATES.iter().map(|(id, _)| *id).collect();
        assert_eq!(files, built);
        assert_eq!(
            summaries().unwrap().as_array().unwrap().len(),
            TEMPLATES.len()
        );
    }

    #[test]
    fn instantiation_matches_the_python_reference() {
        let cases = [
            ("l-bracket", json!({})),
            ("printed-enclosure", json!({})),
            ("shaft", json!({})),
            (
                "shaft",
                json!({"diameter": 60, "load_n": 50000.5, "material": "aluminum_6061_t6"}),
            ),
            ("sheet-metal-channel", json!({})),
        ];
        for (id, inputs) in cases {
            assert_eq!(
                instantiate(id, &inputs).unwrap(),
                python(id, &inputs),
                "{id} {inputs}"
            );
        }
    }

    #[test]
    fn bad_inputs_are_refused_like_the_reference() {
        let message = |inputs: Value| instantiate("shaft", &inputs).unwrap_err();
        assert!(
            message(json!({"girth": 3})).starts_with("unknown input(s) girth; declared: diameter")
        );
        assert_eq!(
            message(json!({"diameter": 1})),
            "diameter must be in 2..200, got 1"
        );
        assert!(
            message(json!({"material": "cheese"}))
                .starts_with("material must be one of aluminum_6061_t6")
        );
        assert_eq!(
            message(json!({"diameter": "wide"})),
            "diameter must be a finite number in millimeters"
        );
        assert!(
            instantiate("gizmo", &json!({}))
                .unwrap_err()
                .contains("Unknown template")
        );
    }
}
