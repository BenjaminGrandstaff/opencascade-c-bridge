//! Explicit, append-only revision records for persisted model intent.
use crate::{DocumentChange, ModelDocument, ModelError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Caller-supplied revision identity and review metadata. `recorded_at` is an
/// opaque nonempty timestamp string; the engine does not read the wall clock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionMetadata {
    pub id: String,
    pub author: String,
    pub recorded_at: String,
    pub message: String,
}

/// A linear history entry containing semantic changes from the previous state.
/// Payloads exclude the revision ledger, so records never recursively copy it.
/// These are review records, not restorable snapshots or kernel generations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentRevision {
    pub metadata: RevisionMetadata,
    pub parent: Option<String>,
    pub changes: Vec<DocumentChange>,
}

impl ModelDocument {
    /// Append one revision after validating both documents and their equal
    /// existing histories. A no-op, duplicate ID, or invalid document leaves
    /// `self` unchanged. Declaration reordering alone does not create a revision.
    /// The first entry records changes against the supplied unrecorded baseline.
    ///
    /// Cost is ordinary validation plus semantic diff over both documents and
    /// linear history scanning/copying. No kernel session or handles are needed.
    pub fn record_revision(
        &mut self,
        previous: &Self,
        metadata: RevisionMetadata,
    ) -> Result<&DocumentRevision, ModelError> {
        previous.validate()?;
        self.validate()?;
        validate_metadata(&metadata)?;
        if self.revisions != previous.revisions {
            return Err(ModelError::new(
                "revision recording requires equal existing histories",
            ));
        }
        if self
            .revisions
            .iter()
            .any(|revision| revision.metadata.id == metadata.id)
        {
            return Err(ModelError::new("revision id already exists"));
        }
        let mut before = previous.clone();
        let mut after = self.clone();
        before.revisions.clear();
        after.revisions.clear();
        let changes = before.semantic_diff(&after)?;
        if changes.is_empty() {
            return Err(ModelError::new(
                "cannot record a revision without semantic changes",
            ));
        }
        let parent = self
            .revisions
            .last()
            .map(|revision| revision.metadata.id.clone());
        self.revisions.push(DocumentRevision {
            metadata,
            parent,
            changes,
        });
        Ok(self.revisions.last().expect("revision was appended"))
    }

    pub(crate) fn validate_revisions(&self) -> Result<(), ModelError> {
        let mut ids = HashSet::new();
        let mut parent = None;
        for revision in &self.revisions {
            validate_metadata(&revision.metadata)?;
            if !ids.insert(revision.metadata.id.as_str()) || revision.parent.as_deref() != parent {
                return Err(ModelError::new(
                    "revision ids must be unique and parents must follow ledger order",
                ));
            }
            validate_changes(&revision.changes)?;
            parent = Some(revision.metadata.id.as_str());
        }
        Ok(())
    }
}

fn validate_changes(changes: &[DocumentChange]) -> Result<(), ModelError> {
    if changes.is_empty() {
        return Err(ModelError::new("recorded revision must contain changes"));
    }
    let mut paths = HashSet::new();
    for change in changes {
        if change.path.is_empty() || change.before == change.after {
            return Err(ModelError::new(
                "recorded revision contains an empty change",
            ));
        }
        // Serialized typed paths preserve punctuation without ambiguous
        // string joining; uniqueness is checked once per entry.
        let path = serde_json::to_string(&change.path)
            .map_err(|error| ModelError::new(format!("revision path: {error}")))?;
        if !paths.insert(path) {
            return Err(ModelError::new(
                "recorded revision contains duplicate change paths",
            ));
        }
        if matches!(change.path.first(), Some(crate::DocumentPathSegment::Field(name)) if name == "revisions")
        {
            return Err(ModelError::new(
                "revision changes must exclude the revision ledger",
            ));
        }
    }
    Ok(())
}

fn validate_metadata(metadata: &RevisionMetadata) -> Result<(), ModelError> {
    if [
        &metadata.id,
        &metadata.author,
        &metadata.recorded_at,
        &metadata.message,
    ]
    .into_iter()
    .any(|value| value.trim().is_empty())
    {
        return Err(ModelError::new(
            "revision id, author, recorded_at, and message must be nonempty",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
