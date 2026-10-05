//! `--serve`: a localhost page that renders the model with WebGL, edits
//! family parameter defaults, saves them, and follows saves made elsewhere.
use super::{POLL, stamp};
use occt_bridge::Session;
use occt_parametric::{InstanceNode, MeshSettings, ModelDocument, ParameterValue};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};

const INDEX: &str = include_str!("web/index.html");
const VIEWER: &str = include_str!("web/viewer.mjs");
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;

pub(super) struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    fn json(status: u16, value: &Value) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: value.to_string().into_bytes(),
        }
    }
    fn text(status: u16, content_type: &'static str, body: &str) -> Self {
        Self {
            status,
            content_type,
            body: body.as_bytes().to_vec(),
        }
    }
    fn error(status: u16, message: &str) -> Self {
        Self::json(status, &json!({ "error": message }))
    }
}

/// The served model: the document as last loaded or edited, and its glTF.
pub(super) struct Studio {
    model: PathBuf,
    output: Option<String>,
    document: ModelDocument,
    gltf: String,
    /// Incremented whenever `gltf` changes, so the page knows to refetch it.
    version: u64,
    /// Unsaved parameter edits since the last load or save.
    dirty: bool,
    error: Option<String>,
    seen: Option<(SystemTime, u64)>,
    pending: Option<(SystemTime, u64)>,
    last_check: Option<Instant>,
}

impl Studio {
    pub(super) fn load(model: &Path, output: Option<String>) -> Result<Self, Box<dyn Error>> {
        let seen = stamp(model);
        let document = ModelDocument::from_json(&fs::read_to_string(model)?)?;
        let gltf = gltf(&document, output.as_deref())?;
        Ok(Self {
            model: model.to_owned(),
            output,
            document,
            gltf,
            version: 1,
            dirty: false,
            error: None,
            seen,
            pending: None,
            last_check: None,
        })
    }

    /// Reloads after a settled change on disk; failures keep the last model.
    /// Unsaved edits are replaced by the newer file. At most one check per poll.
    fn follow_disk(&mut self) {
        if self
            .last_check
            .is_some_and(|checked| checked.elapsed() < POLL)
        {
            return;
        }
        self.last_check = Some(Instant::now());
        let now = stamp(&self.model);
        if now.is_none() || now == self.seen {
            self.pending = None;
            return;
        }
        if self.pending != now {
            self.pending = now;
            return;
        }
        self.seen = now;
        self.pending = None;
        let loaded = fs::read_to_string(&self.model)
            .map_err(|e| e.to_string())
            .and_then(|text| ModelDocument::from_json(&text).map_err(|e| e.to_string()))
            .and_then(|document| {
                gltf(&document, self.output.as_deref()).map(|gltf| (document, gltf))
            });
        match loaded {
            Ok((document, gltf)) => {
                self.document = document;
                self.gltf = gltf;
                self.version += 1;
                self.dirty = false;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("file change not loaded: {error}")),
        }
    }

    fn state(&self) -> Value {
        let parameters: Vec<Value> = self
            .document
            .family
            .parameters
            .iter()
            .map(|p| {
                let (kind, unit) = match &p.default {
                    ParameterValue::Scalar(q) => ("scalar", json!(q.unit)),
                    ParameterValue::Integer(_) => ("integer", Value::Null),
                    ParameterValue::Boolean(_) => ("boolean", Value::Null),
                    ParameterValue::Choice(_) => ("choice", Value::Null),
                    ParameterValue::Vector(_) => ("vector", Value::Null),
                };
                let value = value_json(&p.default);
                let choices = match &p.parameter_type {
                    occt_parametric::ParameterType::Choice(choices) => json!(choices),
                    _ => Value::Null,
                };
                json!({
                    "id": p.id, "kind": kind, "value": value, "unit": unit,
                    "minimum": p.minimum.map(|q| q.value),
                    "maximum": p.maximum.map(|q| q.value),
                    "choices": choices,
                })
            })
            .collect();
        json!({
            "model": self.model.display().to_string(),
            "family": self.document.family.id,
            "version": self.version,
            "dirty": self.dirty,
            "error": self.error,
            "parameters": parameters,
        })
    }

    /// Applies `{"instance"?, "set": {id: value}, "clear": [id]}`. Without
    /// an instance, `set` changes family defaults (keeping each scalar's
    /// unit); with one, it sets that instance's own overrides and `clear`
    /// removes them, so it inherits again. All changes apply or none: one
    /// that fails validation or regeneration leaves the model unchanged.
    fn edit(&mut self, body: &[u8]) -> Result<(), String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Edit {
            #[serde(default)]
            instance: Option<String>,
            #[serde(default)]
            set: serde_json::Map<String, Value>,
            #[serde(default)]
            clear: Vec<String>,
        }
        let edit: Edit = serde_json::from_slice(body).map_err(|e| format!("invalid edit: {e}"))?;
        let mut document = self.document.clone();
        let defaults: HashMap<String, ParameterValue> = document
            .family
            .parameters
            .iter()
            .map(|p| (p.id.clone(), p.default.clone()))
            .collect();
        let default = |id: &str| {
            defaults
                .get(id)
                .ok_or_else(|| format!("unknown parameter '{id}'"))
        };
        match &edit.instance {
            None => {
                if !edit.clear.is_empty() {
                    return Err("family defaults cannot be cleared".into());
                }
                for (id, value) in &edit.set {
                    let parameter = document
                        .family
                        .parameters
                        .iter_mut()
                        .find(|p| &p.id == id)
                        .ok_or_else(|| format!("unknown parameter '{id}'"))?;
                    parameter.default = value_from(id, &parameter.default, value)?;
                }
            }
            Some(instance) => {
                if !primary_instances(&document)?.contains(instance) {
                    return Err(format!(
                        "'{instance}' is not an instance of family '{}'",
                        document.family.id
                    ));
                }
                let node = document
                    .instances
                    .iter_mut()
                    .find(|node| node.id() == instance)
                    .ok_or_else(|| format!("unknown instance '{instance}'"))?;
                let (InstanceNode::Base { overrides, .. } | InstanceNode::Clone { overrides, .. }) =
                    node;
                for id in &edit.clear {
                    default(id)?;
                    overrides.remove(id);
                }
                for (id, value) in &edit.set {
                    // An existing override keeps its own unit.
                    let like = overrides.get(id).unwrap_or(default(id)?).clone();
                    overrides.insert(id.clone(), value_from(id, &like, value)?);
                }
            }
        }
        // Round-trip through JSON so the edit is validated like a saved file.
        let document = document
            .to_json_pretty()
            .and_then(|text| ModelDocument::from_json(&text))
            .map_err(|e| e.to_string())?;
        let gltf = gltf(&document, self.output.as_deref())?;
        self.document = document;
        self.gltf = gltf;
        self.version += 1;
        self.dirty = true;
        self.error = None;
        Ok(())
    }

    /// Instances of the primary family, by id, with their own overrides.
    /// O(instances log instances); the page fetches it once per version.
    fn instances(&self) -> Result<Value, String> {
        let primary = primary_instances(&self.document)?;
        let mut list: Vec<Value> = self
            .document
            .instances
            .iter()
            .filter(|node| primary.contains(node.id()))
            .map(|node| {
                let mut own: Vec<&String> = node.overrides().keys().collect();
                own.sort();
                json!({ "id": node.id(), "overrides": own })
            })
            .collect();
        list.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        Ok(json!({ "version": self.version, "instances": list }))
    }

    /// One instance's effective parameter values, each marked as its own
    /// override, inherited from a clone source, or the family default.
    fn instance(&self, id: &str) -> Result<Value, String> {
        if !primary_instances(&self.document)?.contains(id) {
            return Err(format!("unknown instance '{id}'"));
        }
        let node = self
            .document
            .instances
            .iter()
            .find(|node| node.id() == id)
            .ok_or_else(|| format!("unknown instance '{id}'"))?;
        let graph = self.document.instance_graph().map_err(|e| e.to_string())?;
        let merged = graph.resolve(id).map_err(|e| e.to_string())?.overrides;
        let parameters: Vec<Value> = self
            .document
            .family
            .parameters
            .iter()
            .map(|p| {
                let source = if node.overrides().contains_key(&p.id) {
                    "own"
                } else if merged.contains_key(&p.id) {
                    "inherited"
                } else {
                    "default"
                };
                json!({
                    "id": p.id,
                    "value": value_json(merged.get(&p.id).unwrap_or(&p.default)),
                    "source": source,
                })
            })
            .collect();
        Ok(json!({ "id": id, "version": self.version, "parameters": parameters }))
    }

    /// Writes the edited document beside the model, then renames it over the
    /// model so readers never see a partial file.
    fn save(&mut self) -> Result<(), String> {
        let text = self.document.to_json_pretty().map_err(|e| e.to_string())?;
        let mut temporary = self.model.clone().into_os_string();
        temporary.push(".occt-view-save");
        let temporary = PathBuf::from(temporary);
        fs::write(&temporary, text + "\n").map_err(|e| e.to_string())?;
        fs::rename(&temporary, &self.model).map_err(|e| {
            let _ = fs::remove_file(&temporary);
            e.to_string()
        })?;
        self.seen = stamp(&self.model);
        self.dirty = false;
        Ok(())
    }

    pub(super) fn handle(&mut self, method: &str, target: &str, body: &[u8]) -> Response {
        self.follow_disk();
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        match (method, path) {
            ("GET", "/") => Response::text(200, "text/html; charset=utf-8", INDEX),
            ("GET", "/viewer.mjs") => Response::text(200, "text/javascript", VIEWER),
            ("GET", "/api/state") => Response::json(200, &self.state()),
            ("GET", "/api/model.gltf") => Response::text(200, "model/gltf+json", &self.gltf),
            ("GET", "/api/instances") => match self.instances() {
                Ok(list) => Response::json(200, &list),
                Err(error) => Response::error(500, &error),
            },
            ("GET", "/api/instance") => {
                let id = query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("id="))
                    .and_then(percent_decode);
                match id.map(|id| self.instance(&id)) {
                    Some(Ok(view)) => Response::json(200, &view),
                    Some(Err(error)) => Response::error(404, &error),
                    None => Response::error(400, "instance view needs ?id="),
                }
            }
            ("POST", "/api/parameters") => match self.edit(body) {
                Ok(()) => Response::json(200, &self.state()),
                Err(error) => Response::error(422, &error),
            },
            ("POST", "/api/save") => match self.save() {
                Ok(()) => Response::json(200, &self.state()),
                Err(error) => Response::error(500, &error),
            },
            ("GET" | "POST", _) => Response::error(404, "not found"),
            _ => Response::error(405, "method not allowed"),
        }
    }
}

/// Ids of instances that resolve to the primary family, whose parameters
/// the page edits. O(instances) resolutions sharing one cache.
fn primary_instances(document: &ModelDocument) -> Result<HashSet<String>, String> {
    let graph = document.instance_graph().map_err(|e| e.to_string())?;
    let mut ids = HashSet::new();
    for node in &document.instances {
        let resolved = graph.resolve(node.id()).map_err(|e| e.to_string())?;
        if resolved.definition.id == document.family.id {
            ids.insert(node.id().to_owned());
        }
    }
    Ok(ids)
}

/// A parameter value as JSON: number, boolean or string; vectors are null.
fn value_json(value: &ParameterValue) -> Value {
    match value {
        ParameterValue::Scalar(q) => json!(q.value),
        ParameterValue::Integer(i) => json!(i),
        ParameterValue::Boolean(b) => json!(b),
        ParameterValue::Choice(c) => json!(c),
        ParameterValue::Vector(_) => Value::Null,
    }
}

/// `value` as a parameter value of the same kind (and scalar unit) as `like`.
fn value_from(id: &str, like: &ParameterValue, value: &Value) -> Result<ParameterValue, String> {
    Ok(match (like, value) {
        (ParameterValue::Scalar(q), Value::Number(n)) => {
            let mut q = *q;
            q.value = n.as_f64().ok_or("invalid number")?;
            ParameterValue::Scalar(q)
        }
        (ParameterValue::Integer(_), Value::Number(n)) => {
            ParameterValue::Integer(n.as_i64().ok_or("integer parameter needs an integer")?)
        }
        (ParameterValue::Boolean(_), Value::Bool(b)) => ParameterValue::Boolean(*b),
        (ParameterValue::Choice(_), Value::String(s)) => ParameterValue::Choice(s.clone()),
        _ => return Err(format!("parameter '{id}' cannot take {value}")),
    })
}

/// Decodes `%XX` escapes (and `+` as space) in a query value.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// The model's glTF, showing `output` or by default the primary family's
/// last feature on every instance that has it.
fn gltf(document: &ModelDocument, output: Option<&str>) -> Result<String, String> {
    let output = match output {
        Some(output) => output.to_owned(),
        None => document
            .family
            .features
            .last()
            .map(|feature| feature.id.clone())
            .ok_or("model family has no features; pass --output")?,
    };
    let graph = document.instance_graph().map_err(|e| e.to_string())?;
    let session = Session::new().map_err(|e| e.to_string())?;
    graph
        .export_gltf_output(&session, &output, MeshSettings::default())
        .map_err(|e| e.to_string())
}

/// Serves requests one at a time until the process ends.
pub(super) fn serve(studio: &mut Studio, listener: &TcpListener) -> Result<(), Box<dyn Error>> {
    let port = listener.local_addr()?.port();
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let response = match read_request(&mut stream, port) {
            Ok((method, path, body)) => studio.handle(&method, &path, &body),
            Err(response) => response,
        };
        let _ = write_response(&mut stream, &response);
    }
    Ok(())
}

/// Parses one HTTP/1.1 request. Rejects other hosts (DNS rebinding) and
/// POSTs without the `X-OCCT-View` header, which a cross-site form cannot set.
fn read_request(stream: &mut TcpStream, port: u16) -> Result<(String, String, Vec<u8>), Response> {
    let bad = |message: &str| Response::error(400, message);
    let mut reader = BufReader::new(stream.try_clone().map_err(|_| bad("connection"))?);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|_| bad("request line"))?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(bad("request line"));
    };
    let path = target.to_owned();
    let method = method.to_owned();
    let mut length = 0usize;
    let mut host_ok = false;
    let mut marked = false;
    let mut header_bytes = line.len();
    loop {
        let mut header = String::new();
        let read = reader.read_line(&mut header).map_err(|_| bad("headers"))?;
        header_bytes += read;
        if read == 0 || header_bytes > MAX_HEADER_BYTES {
            return Err(bad("headers"));
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            return Err(bad("headers"));
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse().map_err(|_| bad("content length"))?,
            "host" => {
                host_ok = [format!("127.0.0.1:{port}"), format!("localhost:{port}")]
                    .iter()
                    .any(|allowed| allowed == value)
            }
            "x-occt-view" => marked = true,
            _ => {}
        }
    }
    if !host_ok {
        return Err(Response::error(403, "unexpected host"));
    }
    if method == "POST" && !marked {
        return Err(Response::error(403, "missing X-OCCT-View header"));
    }
    if length > MAX_BODY_BYTES {
        return Err(Response::error(413, "request too large"));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(|_| bad("body"))?;
    Ok((method, path, body))
}

fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        422 => "Unprocessable Content",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)?;
    stream.flush()
}
