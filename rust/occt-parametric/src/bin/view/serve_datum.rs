//! Constructed datum planes from the sketch studio: preview, add, replace or
//! remove one plane, with any parameters that drive it, validated and
//! regenerated like a saved file before it becomes current.
use super::*;
use occt_parametric::{DatumDefinition, ParameterDefinition};

impl Studio {
    pub(super) fn datum_edit(&mut self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Edit {
            version: u64,
            instance: String,
            /// The family's datums as the client saw them.
            expected: Vec<DatumDefinition>,
            /// The plane to add, or to replace the datum with its id.
            #[serde(default)]
            datum: Option<DatumDefinition>,
            /// The id of a datum to remove instead.
            #[serde(default)]
            remove: Option<String>,
            /// New parameters the plane reads, added with it.
            #[serde(default)]
            parameters: Vec<ParameterDefinition>,
            apply: bool,
        }
        let edit: Edit =
            serde_json::from_slice(body).map_err(|e| format!("invalid datum edit: {e}"))?;
        if edit.version != self.version || edit.expected != self.document.family.datums {
            return Err("The model's datums changed. Reopen New plane before applying.".into());
        }
        if !primary_instances(&self.document)?.contains(&edit.instance) {
            return Err("Unknown editable instance.".into());
        }
        let mut document = self.document.clone();
        let family = &mut document.family;
        match (edit.datum, edit.remove) {
            (Some(datum), None) => {
                if datum.id.trim().is_empty() || datum.id != datum.id.trim() {
                    return Err("Enter a nonempty plane name without surrounding spaces.".into());
                }
                if !datum.kind.is_plane() {
                    return Err("Only planes can be made here.".into());
                }
                match family.datums.iter_mut().find(|d| d.id == datum.id) {
                    Some(existing) => *existing = datum,
                    None => family.datums.push(datum),
                }
            }
            (None, Some(id)) => {
                let before = family.datums.len();
                family.datums.retain(|d| d.id != id);
                if family.datums.len() == before {
                    return Err(format!("Unknown datum '{id}'."));
                }
            }
            _ => return Err("Give either a datum or a datum to remove.".into()),
        }
        for parameter in edit.parameters {
            let taken = family.parameters.iter().any(|p| p.id == parameter.id)
                || family
                    .derived_parameters
                    .iter()
                    .any(|p| p.id == parameter.id)
                || family
                    .derived_vector_parameters
                    .iter()
                    .any(|p| p.id == parameter.id);
            if taken {
                return Err(format!("Parameter '{}' already exists.", parameter.id));
            }
            family.parameters.push(parameter);
        }
        family.version = family
            .version
            .checked_add(1)
            .ok_or("family version overflow")?;
        if edit.apply {
            self.commit(document)?;
            return Ok(json!({"applied": true, "state": self.state()}));
        }
        let document = document
            .to_json_pretty()
            .and_then(|text| ModelDocument::from_json(&text))
            .map_err(|e| e.to_string())?;
        let output = self
            .output
            .clone()
            .or_else(|| document.family.features.last().map(|f| f.id.clone()))
            .ok_or("model family has no features")?;
        let outputs = [occt_parametric::InstanceOutputRef {
            instance: edit.instance,
            output,
        }];
        let data = view_data::collect(
            &document,
            &outputs,
            true,
            &view_data::ViewOptions::default(),
        )
        .map_err(|e| e.to_string())?;
        Ok(
            json!({"applied": false, "datum_planes": data.get("datum_planes").cloned().unwrap_or(json!([]))}),
        )
    }
}
