//! Shared editing sessions: an AI client (through MCP) and the person at the
//! studio work on one in-memory model. The client reads the model with its
//! SHA-256, verifies a change elsewhere (an accepted `occt-model` build),
//! and applies it only while that hash is still current, so neither side
//! overwrites an edit it has not seen. An applied change enters the studio's
//! undo history like any page edit, leaves the model unsaved, and is named
//! in `/api/state` so the page can say who changed what.
use super::*;
use occt_parametric::digest::sha256_hex;

/// Longest author and message in an applied change, in characters.
const MAX_LABEL: usize = 200;

/// Who applied the current version, and why.
#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct SessionChange {
    pub author: String,
    pub message: String,
    pub version: u64,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyRequest {
    expected_model_sha256: String,
    model: Value,
    author: String,
    message: String,
}

impl Studio {
    /// SHA-256 of the model as the studio would save it. O(model size).
    fn model_sha256(&self) -> Result<String, String> {
        let text = self.document.to_json_pretty().map_err(|e| e.to_string())?;
        Ok(sha256_hex(text.as_bytes()))
    }

    /// The session's status, and with `model` the current document.
    pub(super) fn session(&self, model: bool) -> Result<Value, String> {
        let mut value = json!({
            "schema": "occb-studio-session-v1",
            "family": self.document.family.id,
            "model_path": self.model.display().to_string(),
            "version": self.version,
            "dirty": self.dirty,
            "model_sha256": self.model_sha256()?,
            "last_change": self.last_change,
            "undo_count": self.history.undo.len(),
        });
        if model {
            value["model"] = serde_json::to_value(&self.document).map_err(|e| e.to_string())?;
        }
        Ok(value)
    }

    /// Replaces the model with `model` when `expected_model_sha256` is still
    /// current: 409 when it is not, 422 when the model does not validate or
    /// regenerate, leaving the session unchanged either way.
    pub(super) fn session_apply(&mut self, body: &[u8]) -> Result<Value, (u16, String)> {
        let invalid = |message: String| (422, message);
        let request: ApplyRequest = serde_json::from_slice(body)
            .map_err(|e| (400, format!("invalid session change: {e}")))?;
        for (name, text) in [("author", &request.author), ("message", &request.message)] {
            if text.trim().is_empty() || text.chars().count() > MAX_LABEL {
                return Err((400, format!("{name} needs 1 to {MAX_LABEL} characters")));
            }
        }
        let current = self.model_sha256().map_err(invalid)?;
        if request.expected_model_sha256 != current {
            return Err((
                409,
                format!(
                    "the studio model changed since it was read (now {current}, version {}); read the session again and rebase the change",
                    self.version
                ),
            ));
        }
        let document = ModelDocument::from_json(&request.model.to_string())
            .map_err(|e| invalid(format!("model: {e}")))?;
        let before = self.version;
        self.commit(document).map_err(invalid)?;
        let changed = self.version != before;
        if changed {
            self.last_change = Some(SessionChange {
                author: request.author,
                message: request.message,
                version: self.version,
            });
        }
        let mut status = self.session(false).map_err(invalid)?;
        status["changed"] = json!(changed);
        Ok(status)
    }
}
