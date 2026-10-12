//! Session-local history stores documents, never meshes or native handles.
use super::{Studio, build};
use occt_parametric::ModelDocument;
use serde_json::Value;

const MAX_ENTRIES: usize = 50;
const MAX_BYTES: usize = 32 * 1024 * 1024;

pub(super) struct Snapshot {
    text: String,
    output: Option<String>,
}

pub(super) struct History {
    pub saved: String,
    pub undo: Vec<Snapshot>,
    pub redo: Vec<Snapshot>,
}

impl History {
    pub fn new(document: &ModelDocument) -> Result<Self, String> {
        Ok(Self {
            saved: document.to_json_pretty().map_err(|e| e.to_string())?,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    fn trim(&mut self) {
        while self.undo.len() + self.redo.len() > MAX_ENTRIES
            || self
                .undo
                .iter()
                .chain(&self.redo)
                .map(|s| s.text.len() + s.output.as_ref().map_or(0, String::len))
                .sum::<usize>()
                > MAX_BYTES
        {
            if !self.undo.is_empty() {
                self.undo.remove(0);
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
            } else {
                break;
            }
        }
    }
}

impl Studio {
    /// Called only after an edit has passed all its validation and generation.
    pub(super) fn accept(
        &mut self,
        document: ModelDocument,
        gltf: String,
        requirements: Value,
        output: Option<String>,
    ) -> Result<(), String> {
        let text = document.to_json_pretty().map_err(|e| e.to_string())?;
        let previous = self.document.to_json_pretty().map_err(|e| e.to_string())?;
        if text == previous && output == self.output {
            return Ok(());
        }
        self.history.undo.push(Snapshot {
            text: previous,
            output: self.output.clone(),
        });
        self.history.redo.clear();
        self.history.trim();
        self.dirty = text != self.history.saved;
        self.document = document;
        self.gltf = gltf;
        self.requirements = requirements;
        self.output = output;
        self.version += 1;
        self.error = None;
        self.annotations = None;
        self.rejected_annotations = None;
        Ok(())
    }

    pub(super) fn navigate_history(&mut self, body: &[u8], redo: bool) -> Result<(), String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            version: u64,
        }
        let request: Request =
            serde_json::from_slice(body).map_err(|e| format!("invalid history request: {e}"))?;
        if request.version != self.version {
            return Err("Model changed; refresh before undo or redo".into());
        }
        let source = if redo {
            &self.history.redo
        } else {
            &self.history.undo
        };
        let target = source.last().ok_or("No edit to undo or redo")?;
        let document = ModelDocument::from_json(&target.text).map_err(|e| e.to_string())?;
        // Regenerate before changing either stack; failure retains accepted state.
        let (gltf, requirements) = build(&document, target.output.as_deref())?;
        let dirty = target.text != self.history.saved;
        let current = Snapshot {
            text: self.document.to_json_pretty().map_err(|e| e.to_string())?,
            output: self.output.clone(),
        };
        let target = if redo {
            self.history.undo.push(current);
            self.history.redo.pop().unwrap()
        } else {
            self.history.redo.push(current);
            self.history.undo.pop().unwrap()
        };
        self.history.trim();
        self.document = document;
        self.gltf = gltf;
        self.requirements = requirements;
        self.output = target.output;
        self.dirty = dirty;
        self.version += 1;
        self.error = None;
        self.annotations = None;
        self.rejected_annotations = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_bounds_documents_by_count_and_bytes() {
        let mut history = History {
            saved: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
        };
        for i in 0..60 {
            history.undo.push(Snapshot {
                text: i.to_string(),
                output: None,
            });
        }
        history.trim();
        assert_eq!(history.undo.len(), MAX_ENTRIES);
        assert_eq!(history.undo[0].text, "10");
        history.undo.clear();
        history.redo.push(Snapshot {
            text: "large".into(),
            output: Some("x".repeat(MAX_BYTES)),
        });
        history.trim();
        assert!(history.redo.is_empty());
    }
}
