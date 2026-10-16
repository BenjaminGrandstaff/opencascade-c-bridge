//! Feature-graph regeneration, incremental reuse, verification, and
//! result placement.

use super::*;

pub(crate) mod parameters;
pub mod validation;

pub use parameters::*;
pub use validation::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegenerationReport {
    pub rebuilt: Vec<String>,
    pub reused: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegenerationState {
    NeverGenerated,
    Current,
    Frozen,
    Stale,
    Failed,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct FeatureSignature<'a> {
    pub feature: &'a FeatureDefinition,
    pub parameters: Vec<(&'a str, &'a ParameterValue)>,
    /// The sketch's datum and the datums it is built on, in order.
    pub datum: Vec<&'a DatumDefinition>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<&'a NamedReference>,
}

pub fn sketch_datum<'a>(
    datums: &HashMap<&str, &'a DatumDefinition>,
    operation: &FeatureOperation,
) -> Result<Option<&'a DatumDefinition>, ModelError> {
    let sketch = match operation {
        FeatureOperation::SketchFace { sketch }
        | FeatureOperation::SketchWire { sketch }
        | FeatureOperation::SketchOpenWire { sketch } => sketch,
        _ => return Ok(None),
    };
    let Some(id) = &sketch.datum_plane else {
        return Ok(None);
    };
    let datum = datums
        .get(id.as_str())
        .copied()
        .ok_or_else(|| ModelError::new(format!("unknown sketch plane datum '{id}'")))?;
    if !datum.kind.is_plane() {
        return Err(ModelError::new(format!(
            "sketch datum '{id}' must be a plane"
        )));
    }
    Ok(Some(datum))
}

/// The feature, the parameter values it reads, its sketch datum, and the
/// named references it uses: any change to these rebuilds it.
pub fn feature_signature(
    datums: &HashMap<&str, &DatumDefinition>,
    feature: &FeatureDefinition,
    references: &References<'_>,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<Vec<u8>, ModelError> {
    let mut names = HashSet::new();
    collect_operation_parameters(&feature.operation, &mut names);
    let mut used = feature
        .operation
        .reference_names()
        .into_iter()
        .filter_map(|(name, _)| references.get(name).copied())
        .collect::<Vec<_>>();
    let chain = match sketch_datum(datums, &feature.operation)? {
        Some(datum) => datum_chain(datums, &datum.id),
        None => Vec::new(),
    };
    for datum in &chain {
        datum.kind.collect_parameters(&mut names);
        for selector in datum.kind.faces().map(|(_, s)| s).unwrap_or_default() {
            let mut selector_names = Vec::new();
            selector.names(&mut selector_names);
            used.extend(
                selector_names
                    .into_iter()
                    .filter_map(|(name, _)| references.get(name).copied()),
            );
        }
    }
    used.sort_by(|a, b| a.name.cmp(&b.name));
    used.dedup_by(|a, b| a.name == b.name);
    for reference in &used {
        match &reference.target {
            ReferenceTarget::Faces(selector) => {
                collect_face_selector_parameters(selector, &mut names)
            }
            ReferenceTarget::Edges(selector) => {
                collect_edge_selector_parameters(selector, &mut names)
            }
        }
    }
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort_unstable();
    let values = names
        .into_iter()
        .map(|name| {
            parameters
                .get(name)
                .map(|value| (name, value))
                .ok_or_else(|| ModelError::new(format!("unknown parameter '{name}'")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_vec(&FeatureSignature {
        feature,
        parameters: values,
        datum: chain,
        references: used,
    })
    .map_err(|error| ModelError::new(format!("create feature signature: {error}")))
}

pub fn instance_error(id: &str, error: ModelError) -> ModelError {
    error.context(&format!("instance '{id}'"))
}
