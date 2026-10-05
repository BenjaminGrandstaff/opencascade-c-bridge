//! Identity-based comparisons of persisted model intent, without kernel work.

use crate::{ModelDocument, ModelError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

mod merge;
pub use merge::*;

/// A field or stable entity identity; IDs containing punctuation stay unambiguous.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentPathSegment {
    Field(String),
    Entity(String),
}

/// One addition, removal, or replacement. Missing values differ from JSON null.
/// Added/removed entities contain the whole canonical entity (nested declaration
/// lists are ID maps); edits contain changed fields. These are review records,
/// not JSON Patch operations against the original document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentChange {
    pub path: Vec<DocumentPathSegment>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub before: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub after: Option<Value>,
}

fn present_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

impl ModelDocument {
    /// Compare persisted intent by ID, with deterministic field/ID ordering.
    /// Declaration order is ignored for families, parameters, features, datums,
    /// constraints, requirements, instances, frames, patterns, relationships,
    /// configurations, and materials. Other arrays (including sketch profiles,
    /// pattern members, expression operands and generation audit records) retain
    /// their order and are reported as a whole when changed.
    ///
    /// Numeric values and units are compared exactly as serialized, without
    /// geometric equivalence or tolerance inference. Both documents should have
    /// been migrated with `from_json`; this also works on unaccepted edits,
    /// rejecting ambiguous duplicate IDs without generating or validating shapes.
    /// No schema change or OCCT session is required.
    ///
    /// For S serialized bytes and at most N entries per object or collection, time is
    /// O(S log N + P + C), memory O(S + P + C), where P is the output path size
    /// and C is the copied changed payload. Recursion uses O(document depth).
    pub fn semantic_diff(&self, other: &Self) -> Result<Vec<DocumentChange>, ModelError> {
        let before = canonical_document(self)?;
        let after = canonical_document(other)?;
        let mut changes = Vec::new();
        compare(Some(&before), Some(&after), &mut Vec::new(), &mut changes);
        Ok(changes)
    }
}

fn field(name: &str) -> DocumentPathSegment {
    DocumentPathSegment::Field(name.into())
}

fn family_path(path: &[DocumentPathSegment]) -> bool {
    path == [field("family")]
        || matches!(path, [DocumentPathSegment::Field(name), DocumentPathSegment::Entity(_)]
            if name == "additional_families")
}

fn collections(path: &[DocumentPathSegment]) -> &'static [&'static str] {
    if path.is_empty() {
        &[
            "additional_families",
            "instances",
            "frames",
            "patterns",
            "drawings",
            "mesh_exports",
        ]
    } else if matches!(path, [DocumentPathSegment::Field(name), DocumentPathSegment::Entity(_)] if name == "drawings")
    {
        &[
            "views",
            "dimensions",
            "notes",
            "guides",
            "datum_features",
            "feature_control_frames",
            "datum_reference_frames",
        ]
    } else if matches!(path, [DocumentPathSegment::Field(name), DocumentPathSegment::Entity(_)] if name == "mesh_exports")
    {
        &["face_tags"]
    } else if family_path(path) {
        &[
            "parameters",
            "derived_parameters",
            "derived_vector_parameters",
            "features",
            "datums",
            "constraints",
            "requirements",
        ]
    } else if path == [field("assembly")] {
        &[
            "requirements",
            "relationships",
            "configurations",
            "materials",
        ]
    } else {
        &[]
    }
}

fn keyed(path: &[DocumentPathSegment]) -> bool {
    let Some((DocumentPathSegment::Field(name), parent)) = path.split_last() else {
        return false;
    };
    collections(parent).contains(&name.as_str())
}

fn canonical_document(document: &ModelDocument) -> Result<Value, ModelError> {
    let value = serde_json::to_value(document)
        .map_err(|error| ModelError::new(format!("serialize document diff: {error}")))?;
    canonicalize(value, &mut Vec::new())
}

fn identity(value: &Value) -> Option<&str> {
    value.get("id").and_then(Value::as_str).or_else(|| {
        // InstanceNode uses externally tagged base/clone variants.
        let object = value.as_object()?;
        if object.len() != 1 {
            return None;
        }
        object.values().next()?.get("id")?.as_str()
    })
}

fn canonicalize(
    mut value: Value,
    path: &mut Vec<DocumentPathSegment>,
) -> Result<Value, ModelError> {
    if keyed(path) {
        let values = value
            .as_array_mut()
            .ok_or_else(|| ModelError::new("expected ID collection"))?;
        let mut indexed = BTreeMap::new();
        for item in values.drain(..) {
            let id = identity(&item)
                .ok_or_else(|| ModelError::new("document diff entity has no ID"))?
                .to_owned();
            if indexed.contains_key(&id) {
                return Err(ModelError::new(format!(
                    "duplicate document diff ID '{id}' at {path:?}"
                )));
            }
            path.push(DocumentPathSegment::Entity(id.clone()));
            let item = canonicalize(item, path)?;
            path.pop();
            indexed.insert(id, item);
        }
        return Ok(Value::Object(indexed.into_iter().collect()));
    }
    if let Value::Object(object) = &mut value {
        // Some empty ID collections are omitted by serde. Make them explicit.
        for name in collections(path) {
            object
                .entry(*name)
                .or_insert_with(|| Value::Array(Vec::new()));
        }
        for (name, item) in object.iter_mut() {
            path.push(field(name));
            *item = canonicalize(item.take(), path)?;
            path.pop();
        }
    }
    Ok(value)
}

fn compare(
    before: Option<&Value>,
    after: Option<&Value>,
    path: &mut Vec<DocumentPathSegment>,
    changes: &mut Vec<DocumentChange>,
) {
    if let (Some(Value::Object(left)), Some(Value::Object(right))) = (before, after) {
        let keys: std::collections::BTreeSet<_> = left.keys().chain(right.keys()).collect();
        for name in keys {
            path.push(if keyed(path) {
                DocumentPathSegment::Entity(name.clone())
            } else {
                field(name)
            });
            compare(left.get(name), right.get(name), path, changes);
            path.pop();
        }
    } else if before != after {
        changes.push(DocumentChange {
            path: path.clone(),
            before: before.cloned(),
            after: after.cloned(),
        });
    }
}

#[cfg(test)]
mod tests;
