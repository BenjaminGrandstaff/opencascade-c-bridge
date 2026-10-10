//! Validation of family definitions: unique ids, references, and traceability.

use super::*;

pub(crate) fn validate_definition(definition: &FamilyDefinition) -> Result<(), ModelError> {
    if definition.id.is_empty() || definition.version == 0 {
        return Err(ModelError::new("family id and version are required"));
    }
    let mut feature_ids = HashSet::new();
    insert_unique_ids(
        &mut feature_ids,
        definition
            .features
            .iter()
            .map(|feature| feature.id.as_str()),
        "feature ids must be nonempty and unique",
    )?;
    let references = validate_references(definition, &feature_ids)?;
    for (feature, color) in &definition.feature_colors {
        if !feature_ids.contains(feature.as_str()) {
            return Err(ModelError::new(format!(
                "feature color names unknown feature '{feature}'"
            )));
        }
        if !color
            .iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
        {
            return Err(ModelError::new(format!(
                "feature '{feature}' color channels must be in [0, 1]"
            )));
        }
    }
    let datums = definition
        .datums
        .iter()
        .map(|datum| (datum.id.as_str(), datum))
        .collect::<HashMap<_, _>>();
    for feature in &definition.features {
        for (name, kind) in feature.operation.reference_names() {
            let found = references.get(name).ok_or_else(|| {
                ModelError::new(format!(
                    "feature '{}' uses unknown named reference '{name}'",
                    feature.id
                ))
            })?;
            if found.target.kind() != kind {
                return Err(ModelError::new(format!(
                    "feature '{}' uses named reference '{name}' as the wrong kind of topology",
                    feature.id
                )));
            }
        }
        match &feature.operation {
            FeatureOperation::SketchFace { sketch }
            | FeatureOperation::SketchWire { sketch }
            | FeatureOperation::SketchOpenWire { sketch } => {
                sketch
                    .validate_structure()
                    .map_err(|error| error.in_feature(&feature.id))?;
                sketch_datum(&datums, &feature.operation)
                    .map_err(|error| error.in_feature(&feature.id))?;
            }
            FeatureOperation::Sew { inputs, .. } if inputs.is_empty() => {
                return Err(ModelError::new(format!(
                    "feature '{}' requires at least one sewing input",
                    feature.id
                )));
            }
            FeatureOperation::MakeSolid { shells } if shells.is_empty() => {
                return Err(ModelError::new(format!(
                    "feature '{}' requires at least one shell input",
                    feature.id
                )));
            }
            _ => {}
        }
        if let Some(dependency) = feature
            .operation
            .dependencies()
            .into_iter()
            .find(|dependency| !feature_ids.contains(dependency))
        {
            return Err(ModelError::new(format!(
                "feature '{}' references unknown output '{dependency}'",
                feature.id
            )));
        }
    }
    let mut parameter_ids = HashSet::new();
    insert_unique_ids(
        &mut parameter_ids,
        definition
            .parameters
            .iter()
            .map(|parameter| parameter.id.as_str()),
        "parameter ids must be nonempty and unique",
    )?;
    insert_unique_ids(
        &mut parameter_ids,
        definition
            .derived_parameters
            .iter()
            .map(|parameter| parameter.id.as_str())
            .chain(
                definition
                    .derived_vector_parameters
                    .iter()
                    .map(|parameter| parameter.id.as_str()),
            ),
        "input and derived parameter ids must be nonempty and unique",
    )?;
    insert_unique_ids(
        &mut HashSet::new(),
        definition
            .constraints
            .iter()
            .map(|constraint| constraint.id.as_str()),
        "constraint ids must be nonempty and unique",
    )?;
    if definition
        .requirements
        .iter()
        .any(|requirement| requirement.version == 0)
    {
        return Err(ModelError::new(
            "requirement ids must be nonempty, versioned, and unique",
        ));
    }
    insert_unique_ids(
        &mut HashSet::new(),
        definition
            .requirements
            .iter()
            .map(|requirement| requirement.id.as_str()),
        "requirement ids must be nonempty, versioned, and unique",
    )?;
    validate_traces(definition, &feature_ids, &parameter_ids)?;
    assembly::validate_datums(definition)
}

/// Checks named references: unique nonempty names, targets that use no other
/// names, and features that exist. O(references + their selectors).
fn validate_references<'a>(
    definition: &'a FamilyDefinition,
    feature_ids: &HashSet<&str>,
) -> Result<References<'a>, ModelError> {
    insert_unique_ids(
        &mut HashSet::new(),
        definition
            .references
            .iter()
            .map(|reference| reference.name.as_str()),
        "named reference names must be nonempty and unique",
    )?;
    for reference in &definition.references {
        let mut names = Vec::new();
        reference.target.names(&mut names);
        if !names.is_empty() {
            return Err(ModelError::new(format!(
                "named reference '{}' cannot use other named references",
                reference.name
            )));
        }
        let mut dependencies = Vec::new();
        reference.target.dependencies(&mut dependencies);
        if let Some(missing) = dependencies
            .into_iter()
            .find(|dependency| !feature_ids.contains(dependency))
        {
            return Err(ModelError::new(format!(
                "named reference '{}' references unknown output '{missing}'",
                reference.name
            )));
        }
    }
    Ok(reference_map(definition))
}

/// Assumptions have unique ids and statements; every requirement trace names
/// an existing feature, parameter, or assumption. O(assumptions + traces).
fn validate_traces(
    definition: &FamilyDefinition,
    feature_ids: &HashSet<&str>,
    parameter_ids: &HashSet<&str>,
) -> Result<(), ModelError> {
    let mut assumptions = HashSet::new();
    insert_unique_ids(
        &mut assumptions,
        definition
            .assumptions
            .iter()
            .map(|assumption| assumption.id.as_str()),
        "assumption ids must be nonempty and unique",
    )?;
    if let Some(assumption) = definition
        .assumptions
        .iter()
        .find(|assumption| assumption.statement.trim().is_empty())
    {
        return Err(ModelError::new(format!(
            "assumption '{}' needs a statement",
            assumption.id
        )));
    }
    for requirement in &definition.requirements {
        for trace in &requirement.traces {
            let (known, kind, id) = match trace {
                TraceTarget::Feature(id) => (feature_ids.contains(id.as_str()), "feature", id),
                TraceTarget::Parameter(id) => {
                    (parameter_ids.contains(id.as_str()), "parameter", id)
                }
                TraceTarget::Assumption(id) => {
                    (assumptions.contains(id.as_str()), "assumption", id)
                }
            };
            if !known {
                return Err(ModelError::new(format!(
                    "requirement '{}' traces to unknown {kind} '{id}'",
                    requirement.id
                )));
            }
        }
    }
    Ok(())
}

/// Adds every id to `seen`, failing on the first empty or repeated id.
pub(crate) fn insert_unique_ids<'a>(
    seen: &mut HashSet<&'a str>,
    ids: impl IntoIterator<Item = &'a str>,
    message: &str,
) -> Result<(), ModelError> {
    for id in ids {
        if id.is_empty() || !seen.insert(id) {
            return Err(ModelError::new(message));
        }
    }
    Ok(())
}
