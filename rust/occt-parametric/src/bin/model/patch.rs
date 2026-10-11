//! Guarded edits by family/entity identity, followed by verified generation.
use super::*;
use std::collections::HashMap;
#[path = "patch/sketch.rs"]
pub(super) mod sketch;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchRequest {
    schema: String,
    model: ModelDocument,
    changes: Vec<Change>,
    outputs: Vec<InstanceOutputRef>,
    #[serde(default)]
    edits: Vec<Edit>,
    revision: RevisionMetadata,
    #[serde(default)]
    source: Option<Source>,
    #[serde(default = "yes")]
    step: bool,
    #[serde(default)]
    stl: bool,
    #[serde(default = "yes")]
    preview: bool,
}
#[derive(Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Source {
    build_id: String,
    model_sha256: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    EditSketch {
        family: String,
        feature: String,
        expected: Box<SketchDefinition>,
        changes: Vec<sketch::SketchChange>,
    },
    AddFeature {
        family: String,
        feature: Box<FeatureDefinition>,
    },
    ReplaceFeature {
        family: String,
        expected: Box<FeatureDefinition>,
        feature: Box<FeatureDefinition>,
    },
    RemoveFeature {
        family: String,
        expected: Box<FeatureDefinition>,
    },
    AddParameter {
        family: String,
        parameter: Box<ParameterDefinition>,
    },
    AddRequirement {
        family: String,
        requirement: Box<Requirement>,
    },
    AddReference {
        family: String,
        reference: Box<NamedReference>,
    },
    ReplaceReference {
        family: String,
        expected: Box<NamedReference>,
        reference: Box<NamedReference>,
    },
    RemoveReference {
        family: String,
        expected: Box<NamedReference>,
    },
}
impl Change {
    fn family(&self) -> &str {
        match self {
            Self::EditSketch { family, .. }
            | Self::AddFeature { family, .. }
            | Self::ReplaceFeature { family, .. }
            | Self::RemoveFeature { family, .. }
            | Self::AddParameter { family, .. }
            | Self::AddRequirement { family, .. }
            | Self::AddReference { family, .. }
            | Self::ReplaceReference { family, .. }
            | Self::RemoveReference { family, .. } => family,
        }
    }
    fn identity(&self) -> (&str, &str) {
        match self {
            Self::EditSketch { feature, .. } => ("feature", feature),
            Self::AddFeature { feature, .. } | Self::ReplaceFeature { feature, .. } => {
                ("feature", &feature.id)
            }
            Self::RemoveFeature { expected, .. } => ("feature", &expected.id),
            Self::AddParameter { parameter, .. } => ("parameter", &parameter.id),
            Self::AddRequirement { requirement, .. } => ("requirement", &requirement.id),
            Self::AddReference { reference, .. } | Self::ReplaceReference { reference, .. } => {
                ("reference", &reference.name)
            }
            Self::RemoveReference { expected, .. } => ("reference", &expected.name),
        }
    }
}
pub fn run(path: &OsString, destination: &OsString) -> Result<Value, Failure> {
    load(path, destination, false)
}
pub fn preview(path: &OsString, destination: &OsString) -> Result<Value, Failure> {
    load(path, destination, true)
}
fn load(path: &OsString, destination: &OsString, preview: bool) -> Result<Value, Failure> {
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
    let request: PatchRequest = serde_json::from_value(raw).map_err(|e| failure("request", e))?;
    edit(request, Path::new(destination), preview)
}
fn edit(request: PatchRequest, destination: &Path, preview: bool) -> Result<Value, Failure> {
    if request.schema != "occb-model-edit-v1"
        || request.changes.len() > 10000
        || request.edits.len() > 10000
        || (request.changes.is_empty() && request.edits.is_empty())
    {
        return Err(failure(
            "request",
            "expected occb-model-edit-v1 with 1..10000 changes and/or parameter edits",
        ));
    }
    if let Some(source) = &request.source {
        let hex = |s: &str, n| {
            s.len() == n
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if !hex(&source.build_id, 32) || !hex(&source.model_sha256, 64) {
            return Err(failure("request", "invalid source build ID or SHA-256"));
        }
    }
    let original = request.model;
    let mut edited = original.clone();
    apply(&mut edited, &request.changes)?;
    if edited == original && request.edits.is_empty() {
        return Err(failure("revision", "edit makes no semantic changes"));
    }
    // Validate revision identity before kernel/export work; the final record is
    // made from the actual saved result, including parameter/pattern changes.
    if request.revision.id.trim().is_empty()
        || request.revision.author.trim().is_empty()
        || request.revision.recorded_at.trim().is_empty()
        || request.revision.message.trim().is_empty()
        || original
            .revisions
            .iter()
            .any(|r| r.metadata.id == request.revision.id)
    {
        return Err(failure(
            "revision",
            "revision metadata must be nonempty with a new stable ID",
        ));
    }
    edited
        .to_json_pretty()
        .map_err(|e| model_failure("validation", e))?;
    if preview {
        if request.edits.len() > 10000
            || request.outputs.is_empty()
            || request.outputs.len() > 10000
        {
            return Err(failure(
                "request",
                "preview requires 1..10000 outputs and at most 10000 parameter edits",
            ));
        }
        if request
            .outputs
            .iter()
            .map(|o| &o.instance)
            .collect::<HashSet<_>>()
            .len()
            != request.outputs.len()
        {
            return Err(failure("request", "select at most one output per instance"));
        }
        let mut graph = edited
            .instance_graph()
            .map_err(|e| model_failure("validation", e))?;
        let mut seen = HashSet::new();
        for edit in &request.edits {
            if !seen.insert((&edit.instance, &edit.parameter)) {
                return Err(failure("edits", "duplicate instance/parameter edit"));
            }
            graph
                .set_override(&edit.instance, &edit.parameter, edit.value.clone())
                .map_err(|e| model_failure("edits", e))?;
        }
        let refreshed = ModelDocument::from_graph(&graph);
        edited.instances = refreshed.instances;
        edited.patterns = refreshed.patterns;
        let mut report = visualize::write(
            &edited,
            &request.outputs,
            true,
            &visualize::ViewOptions::default(),
            destination,
            true,
        )?;
        report["status"] = json!("previewed");
        report["accepted"] = json!(false);
        report["source"] = json!(request.source);
        let publication = fs::write(
            destination.join("report.json"),
            serde_json::to_string_pretty(&report).map_err(|e| failure("publication", e))?,
        )
        .map_err(|e| failure("publication", e));
        if publication.is_err() {
            let _ = fs::remove_dir_all(destination);
        }
        publication?;
        return Ok(report);
    }
    let mut report = build_internal(
        Request {
            schema: "occb-model-request-v1".into(),
            model: edited,
            outputs: request.outputs,
            edits: request.edits,
            step: request.step,
            stl: request.stl,
            preview: request.preview,
        },
        destination,
        false,
    )?;
    let finish = (|| {
        let mut saved = ModelDocument::from_json(
            &fs::read_to_string(destination.join("model.json"))
                .map_err(|e| failure("publication", e))?,
        )
        .map_err(|e| model_failure("validation", e))?;
        let revision = saved
            .record_revision(&original, request.revision)
            .map_err(|e| model_failure("revision", e))?
            .clone();
        fs::write(
            destination.join("changes.json"),
            serde_json::to_string_pretty(&revision).map_err(|e| failure("publication", e))?,
        )
        .map_err(|e| failure("publication", e))?;
        fs::write(
            destination.join("model.json"),
            saved
                .to_json_pretty()
                .map_err(|e| model_failure("validation", e))?,
        )
        .map_err(|e| failure("publication", e))?;
        report["revision"] = json!(revision.metadata);
        report["change_count"] = json!(revision.changes.len());
        report["source"] = json!(request.source);
        report["artifacts"]["changes"] = json!("changes.json");
        fs::write(
            destination.join("report.json"),
            serde_json::to_string_pretty(&report).map_err(|e| failure("publication", e))?,
        )
        .map_err(|e| failure("publication", e))?;
        Ok(report)
    })();
    if finish.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    finish
}
fn apply(model: &mut ModelDocument, changes: &[Change]) -> Result<(), Failure> {
    let families = std::iter::once(&model.family)
        .chain(&model.additional_families)
        .map(|f| f.id.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut groups = HashMap::<&str, Vec<&Change>>::new();
    for change in changes {
        if !families.contains(change.family()) {
            return Err(failure(
                "patch",
                format!("unknown family '{}'", change.family()),
            ));
        }
        let (kind, id) = change.identity();
        if !seen.insert((change.family(), kind, id)) {
            return Err(failure(
                "patch",
                format!(
                    "duplicate edit for {kind} '{id}' in family '{}'",
                    change.family()
                ),
            ));
        }
        if let Change::ReplaceFeature {
            expected, feature, ..
        } = change
            && expected.id != feature.id
        {
            return Err(failure("patch", "replacement must preserve the feature ID"));
        }
        if let Change::ReplaceReference {
            expected,
            reference,
            ..
        } = change
            && expected.name != reference.name
        {
            return Err(failure(
                "patch",
                "replacement must preserve the reference name",
            ));
        }
        groups.entry(change.family()).or_default().push(change);
    }
    for family in std::iter::once(&mut model.family).chain(&mut model.additional_families) {
        if let Some(changes) = groups.get(family.id.as_str())
            && apply_family(family, changes)?
        {
            family.version = family
                .version
                .checked_add(1)
                .ok_or_else(|| failure("patch", "family version overflow"))?;
        }
    }
    Ok(())
}
fn apply_family(family: &mut FamilyDefinition, changes: &[&Change]) -> Result<bool, Failure> {
    let mut changed = false;
    let mut modifications = changes
        .iter()
        .filter_map(|c| match c {
            Change::EditSketch { feature, .. } => Some((feature.as_str(), *c)),
            Change::ReplaceFeature { expected, .. } | Change::RemoveFeature { expected, .. } => {
                Some((expected.id.as_str(), *c))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut features = Vec::with_capacity(family.features.len() + changes.len());
    let feature_ids = family
        .features
        .iter()
        .map(|f| f.id.clone())
        .collect::<HashSet<_>>();
    for mut feature in family.features.drain(..) {
        match modifications.remove(feature.id.as_str()) {
            Some(Change::EditSketch {
                expected, changes, ..
            }) => {
                let sketch = match &mut feature.operation {
                    FeatureOperation::SketchFace { sketch }
                    | FeatureOperation::SketchWire { sketch }
                    | FeatureOperation::SketchOpenWire { sketch } => sketch,
                    _ => {
                        return Err(failure(
                            "patch",
                            format!("feature '{}' is not a sketch", feature.id),
                        ));
                    }
                };
                if sketch.as_ref() != expected.as_ref() {
                    return Err(failure(
                        "conflict",
                        format!(
                            "sketch feature '{}' differs from expected snapshot",
                            feature.id
                        ),
                    ));
                }
                sketch::apply(sketch, changes)?;
                changed |= sketch.as_ref() != expected.as_ref();
                features.push(feature);
            }
            Some(Change::ReplaceFeature {
                expected,
                feature: replacement,
                ..
            }) => {
                if &feature != expected.as_ref() {
                    return Err(failure(
                        "conflict",
                        format!(
                            "feature '{}' in family '{}' differs from expected snapshot",
                            feature.id, family.id
                        ),
                    ));
                }
                changed |= &feature != replacement.as_ref();
                features.push(replacement.as_ref().clone());
            }
            Some(Change::RemoveFeature { expected, .. }) => {
                changed = true;
                if &feature != expected.as_ref() {
                    return Err(failure(
                        "conflict",
                        format!(
                            "feature '{}' in family '{}' differs from expected snapshot",
                            feature.id, family.id
                        ),
                    ));
                }
            }
            _ => features.push(feature),
        }
    }
    if let Some(id) = modifications.keys().min() {
        return Err(failure(
            "conflict",
            format!(
                "expected feature '{id}' is missing in family '{}'",
                family.id
            ),
        ));
    }
    let mut parameter_ids = family
        .parameters
        .iter()
        .map(|p| p.id.clone())
        .chain(family.derived_parameters.iter().map(|p| p.id.clone()))
        .chain(
            family
                .derived_vector_parameters
                .iter()
                .map(|p| p.id.clone()),
        )
        .collect::<HashSet<_>>();
    let mut requirement_ids = family
        .requirements
        .iter()
        .map(|r| r.id.clone())
        .collect::<HashSet<_>>();
    for change in changes {
        let (expected, replacement) = match change {
            Change::ReplaceReference {
                expected,
                reference,
                ..
            } => (expected.as_ref(), Some(reference.as_ref())),
            Change::RemoveReference { expected, .. } => (expected.as_ref(), None),
            _ => continue,
        };
        let index = family
            .references
            .iter()
            .position(|r| r.name == expected.name)
            .ok_or_else(|| {
                failure(
                    "conflict",
                    format!("reference '{}' is missing", expected.name),
                )
            })?;
        if &family.references[index] != expected {
            return Err(failure(
                "conflict",
                format!(
                    "reference '{}' differs from expected snapshot",
                    expected.name
                ),
            ));
        }
        if let Some(reference) = replacement {
            changed |= reference != expected;
            family.references[index] = reference.clone();
        } else {
            family.references.remove(index);
            changed = true;
        }
    }
    let mut reference_names = family
        .references
        .iter()
        .map(|r| r.name.clone())
        .collect::<HashSet<_>>();
    for change in changes {
        match change {
            Change::AddFeature { feature, .. } => {
                if feature_ids.contains(&feature.id) {
                    return Err(failure(
                        "conflict",
                        format!("feature '{}' already exists", feature.id),
                    ));
                }
                changed = true;
                features.push(feature.as_ref().clone());
            }
            Change::AddParameter { parameter, .. } => {
                if !parameter_ids.insert(parameter.id.clone()) {
                    return Err(failure(
                        "conflict",
                        format!("parameter '{}' already exists", parameter.id),
                    ));
                }
                changed = true;
                family.parameters.push(parameter.as_ref().clone());
            }
            Change::AddRequirement { requirement, .. } => {
                if !requirement_ids.insert(requirement.id.clone()) {
                    return Err(failure(
                        "conflict",
                        format!("requirement '{}' already exists", requirement.id),
                    ));
                }
                changed = true;
                family.requirements.push(requirement.as_ref().clone());
            }
            Change::AddReference { reference, .. } => {
                if !reference_names.insert(reference.name.clone()) {
                    return Err(failure(
                        "conflict",
                        format!("reference '{}' already exists", reference.name),
                    ));
                }
                changed = true;
                family.references.push(reference.as_ref().clone());
            }
            _ => {}
        }
    }
    family.features = features;
    Ok(changed)
}
