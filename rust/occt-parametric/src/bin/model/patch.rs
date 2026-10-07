//! Guarded edits by family/entity identity, followed by verified generation.
use super::*;
use std::collections::HashMap;

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
}
impl Change {
    fn family(&self) -> &str {
        match self {
            Self::AddFeature { family, .. }
            | Self::ReplaceFeature { family, .. }
            | Self::RemoveFeature { family, .. }
            | Self::AddParameter { family, .. }
            | Self::AddRequirement { family, .. }
            | Self::AddReference { family, .. } => family,
        }
    }
    fn identity(&self) -> (&str, &str) {
        match self {
            Self::AddFeature { feature, .. } | Self::ReplaceFeature { feature, .. } => {
                ("feature", &feature.id)
            }
            Self::RemoveFeature { expected, .. } => ("feature", &expected.id),
            Self::AddParameter { parameter, .. } => ("parameter", &parameter.id),
            Self::AddRequirement { requirement, .. } => ("requirement", &requirement.id),
            Self::AddReference { reference, .. } => ("reference", &reference.name),
        }
    }
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
    let request: PatchRequest = serde_json::from_value(raw).map_err(|e| failure("request", e))?;
    edit(request, Path::new(destination))
}
fn edit(request: PatchRequest, destination: &Path) -> Result<Value, Failure> {
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
    for feature in family.features.drain(..) {
        match modifications.remove(feature.id.as_str()) {
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
