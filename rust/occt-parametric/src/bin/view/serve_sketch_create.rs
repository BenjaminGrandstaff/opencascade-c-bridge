//! New profiles use existing schema geometry and native attachment validation.
use super::*;
use occt_parametric::{FeatureDefinition, FeatureOperation, SketchDefinition};

impl Studio {
    pub(super) fn sketch_create(&mut self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Create {
            version: u64,
            instance: String,
            expected: Vec<FeatureDefinition>,
            feature: String,
            definition: SketchDefinition,
            apply: bool,
        }
        let edit: Create =
            serde_json::from_slice(body).map_err(|e| format!("invalid new sketch: {e}"))?;
        if edit.version != self.version || edit.expected != self.document.family.features {
            return Err("The feature graph changed. Reload New sketch before applying.".into());
        }
        if !primary_instances(&self.document)?.contains(&edit.instance) {
            return Err("Unknown editable instance.".into());
        }
        if edit.feature.trim().is_empty() || edit.feature != edit.feature.trim() {
            return Err("Enter a nonempty feature name without surrounding spaces.".into());
        }
        if self
            .document
            .family
            .features
            .iter()
            .any(|f| f.id == edit.feature)
        {
            return Err(format!("Feature '{}' already exists.", edit.feature));
        }
        // Keep the current solid visible while a new profile is authored.
        let output = self
            .output
            .clone()
            .or_else(|| self.document.family.features.last().map(|f| f.id.clone()));
        let mut document = self.document.clone();
        document.family.features.push(FeatureDefinition {
            id: edit.feature.clone(),
            operation: FeatureOperation::SketchFace {
                sketch: Box::new(edit.definition),
            },
        });
        document.family.version = document
            .family
            .version
            .checked_add(1)
            .ok_or("family version overflow")?;
        let document =
            ModelDocument::from_json(&document.to_json_pretty().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let generated = build_profile(&document, &edit.feature, output.as_deref());
        if edit.apply {
            let (gltf, requirements) = generated?;
            self.accept(document, gltf, requirements, output)?;
            Ok(json!({"applied":true,"feature":edit.feature,"state":self.state()}))
        } else {
            let outputs = [occt_parametric::InstanceOutputRef {
                instance: edit.instance,
                output: edit.feature,
            }];
            let annotations = view_data::collect(
                &document,
                &outputs,
                true,
                &view_data::ViewOptions::default(),
            )
            .map(|mut data| {
                data["version"] = json!(self.version);
                data["accepted"] = json!(false);
                data["preview"] = json!(true);
                data
            })
            .ok();
            match generated {
                Ok(_) => Ok(json!({"applied":false,"can_apply":true,"annotations":annotations})),
                Err(error) => Ok(
                    json!({"applied":false,"can_apply":false,"error":error,"annotations":annotations}),
                ),
            }
        }
    }
}

/// Validate the authored face itself as well as the previously viewed output.
fn build_profile(
    document: &ModelDocument,
    profile: &str,
    output: Option<&str>,
) -> Result<(String, Value), String> {
    let primary = primary_instances(document)?;
    let mut graph = document.instance_graph().map_err(|e| e.to_string())?;
    let session = Session::new().map_err(|e| e.to_string())?;
    let generation = graph.regenerate_all(&session).map_err(|e| e.to_string())?;
    let mut checked = HashSet::new();
    for (id, result) in generation
        .instances()
        .filter(|(id, _)| primary.contains(*id))
    {
        if !checked.insert(generation.shared_from(id).unwrap_or(id)) {
            continue;
        }
        let shape = result
            .shape(profile)
            .ok_or("New profile output is unavailable.")?;
        let faces = session
            .subshape_count(shape, occt_bridge::ShapeType::Face)
            .map_err(|e| e.to_string())?;
        let area = session.surface_area(shape).map_err(|e| e.to_string())?;
        if faces == 0
            || !area.is_finite()
            || area <= 0.0
            || !session.is_valid(shape).map_err(|e| e.to_string())?
        {
            return Err(format!(
                "New sketch '{profile}' on '{id}' must have valid, nonempty face geometry."
            ));
        }
    }
    if checked.is_empty() {
        return Err("No primary-family profile was generated.".into());
    }
    let gltf = graph
        .export_gltf_generated(
            &session,
            &generation,
            output.unwrap_or(profile),
            MeshSettings::default(),
        )
        .map_err(|e| e.to_string())?;
    Ok((gltf, requirement_report(document, &generation)))
}
