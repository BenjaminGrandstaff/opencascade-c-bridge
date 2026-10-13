//! Dataset identification and authority (schema 151): which product a
//! drawing's model-based definition describes, and whether the model, the
//! drawing or both govern it, as ASME Y14.41 asks a dataset to state.
use super::*;

/// Which part of the dataset governs the product definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DatasetAuthority {
    /// The model and its annotations are the definition; a drawing, if any,
    /// is for reference.
    Model,
    /// The model and a reduced-content drawing define the product together.
    ModelAndDrawing,
    /// The drawing is the definition; the model is for reference.
    Drawing,
}

impl DatasetAuthority {
    /// The statement written to exchanged files.
    pub fn statement(self) -> &'static str {
        match self {
            Self::Model => "authority: model",
            Self::ModelAndDrawing => "authority: model and drawing",
            Self::Drawing => "authority: drawing",
        }
    }

    /// Whether the model governs, alone or with the drawing.
    pub fn model_governs(self) -> bool {
        self != Self::Drawing
    }
}

/// The product a drawing's dataset defines.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingDataset {
    /// Part or dataset number, 1–64 printable ASCII characters.
    pub identifier: String,
    /// The organization responsible for the dataset and its approvals.
    pub organization: String,
    pub authority: DatasetAuthority,
}

fn printable(value: &str, what: &str) -> Result<(), ModelError> {
    if value.trim().is_empty()
        || value.len() > 64
        || !value.bytes().all(|b| (0x20..0x7f).contains(&b))
    {
        return Err(ModelError::new(format!(
            "dataset {what} must be 1-64 printable ASCII characters"
        )));
    }
    Ok(())
}

/// Validates the dataset and its revision rule: when the model governs,
/// every release names the model revision it released, so each revision of
/// the dataset traces to the model content it covers. O(releases).
pub fn validate(drawing: &DrawingDefinition) -> Result<(), ModelError> {
    let Some(dataset) = &drawing.dataset else {
        return Ok(());
    };
    printable(&dataset.identifier, "identifier")?;
    printable(&dataset.organization, "organization")?;
    if dataset.authority.model_governs()
        && let Some(release) = drawing.releases.iter().find(|r| r.model_revision.is_none())
    {
        return Err(ModelError::new(format!(
            "release '{}' must name the model revision it releases, since the model governs the dataset",
            release.revision
        )));
    }
    Ok(())
}
