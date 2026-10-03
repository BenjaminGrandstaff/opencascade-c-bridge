//! Three-way merge of canonical document trees, followed by model validation.

use super::*;
use std::collections::BTreeSet;

/// Incompatible concurrent edits at one typed path. Payloads use the same
/// canonical representation as `DocumentChange`; omitted fields mean absence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentConflict {
    pub path: Vec<DocumentPathSegment>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub base: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub left: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub right: Option<Value>,
}

/// A validated merged document, or deterministic conflicts requiring resolution.
/// A conflicting merge never returns a partially combined document.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentMerge {
    Merged(Box<ModelDocument>),
    Conflicts(Vec<DocumentConflict>),
}

impl ModelDocument {
    /// Merge two edited documents sharing `self` as their base.
    ///
    /// Independent fields and IDs combine; identical concurrent edits coalesce.
    /// Delete/edit, different additions of the same ID, incompatible variant
    /// replacements, and different edits to an ordered array produce conflicts.
    /// Declarations in a successful result are sorted by ID. Inputs are unchanged.
    ///
    /// All inputs and the final result undergo ordinary document validation.
    /// Invalid input or a combined dependency/reference/constraint failure returns
    /// `ModelError`, even when no individual field conflicts. This checks model
    /// intent without allocating kernel handles or promising valid geometry.
    ///
    /// Tree merging costs O(S log N + S D + P + C) time and O(S + P + C) memory,
    /// with S serialized bytes, N maximum object/collection size, D document
    /// nesting depth, P conflict path bytes, and C conflict payload bytes. The
    /// S D term accounts for subtree equivalence checks that short-circuit
    /// unchanged branches; it does not scan instance graphs inside per-node loops.
    /// Ordinary document validation adds its existing graph/parameter cost.
    pub fn three_way_merge(&self, left: &Self, right: &Self) -> Result<DocumentMerge, ModelError> {
        for (name, document) in [("base", self), ("left", left), ("right", right)] {
            document
                .validate()
                .map_err(|error| error.context(&format!("merge {name}")))?;
        }
        let base = canonical_document(self)?;
        let left = canonical_document(left)?;
        let right = canonical_document(right)?;
        let mut conflicts = Vec::new();
        let merged = merge_values(
            Some(&base),
            Some(&left),
            Some(&right),
            &mut Vec::new(),
            &mut conflicts,
        );
        if !conflicts.is_empty() {
            return Ok(DocumentMerge::Conflicts(conflicts));
        }
        let value = restore(merged.expect("document roots are present"), &mut Vec::new());
        let document: ModelDocument = serde_json::from_value(value)
            .map_err(|error| ModelError::new(format!("decode merged document: {error}")))?;
        document
            .validate()
            .map_err(|error| error.context("merged document"))?;
        Ok(DocumentMerge::Merged(Box::new(document)))
    }
}

fn dictionary(path: &[DocumentPathSegment]) -> bool {
    keyed(path)
        || matches!(path, [DocumentPathSegment::Field(assembly), DocumentPathSegment::Field(name)]
            if assembly == "assembly" && matches!(name.as_str(), "material_assignments" | "material_appearances" | "joints"))
        || matches!(path, [DocumentPathSegment::Field(instances), DocumentPathSegment::Entity(_), DocumentPathSegment::Field(variant), DocumentPathSegment::Field(overrides)]
            if instances == "instances" && matches!(variant.as_str(), "base" | "clone") && overrides == "overrides")
        || matches!(path, [DocumentPathSegment::Field(assembly), DocumentPathSegment::Field(configurations), DocumentPathSegment::Entity(_), DocumentPathSegment::Field(overrides)]
            if assembly == "assembly" && configurations == "configurations" && overrides == "overrides")
        || matches!(path, [DocumentPathSegment::Field(assembly), DocumentPathSegment::Field(configurations), DocumentPathSegment::Entity(_), DocumentPathSegment::Field(overrides), DocumentPathSegment::Field(_)]
            if assembly == "assembly" && configurations == "configurations" && overrides == "overrides")
}

fn variant_replaced(
    base: &Value,
    left: &Value,
    right: &Value,
    path: &[DocumentPathSegment],
) -> bool {
    if dictionary(path) {
        return false;
    }
    let (Some(base), Some(left), Some(right)) =
        (base.as_object(), left.as_object(), right.as_object())
    else {
        return false;
    };
    // Externally tagged enums are singleton objects. Keep replacement of their
    // discriminant atomic so combining branches cannot create two variants.
    base.len() == 1
        && [left, right]
            .iter()
            .any(|side| side.len() == 1 && side.keys().next() != base.keys().next())
}

fn merge_values(
    base: Option<&Value>,
    left: Option<&Value>,
    right: Option<&Value>,
    path: &mut Vec<DocumentPathSegment>,
    conflicts: &mut Vec<DocumentConflict>,
) -> Option<Value> {
    // Serde omits some empty dictionaries. Treat all three missing maps as
    // empty so deleting their last entry can combine with an unrelated addition.
    let empty = Value::Object(serde_json::Map::new());
    let normalize_maps = dictionary(path)
        && [base, left, right]
            .iter()
            .all(|value| value.is_none_or(Value::is_object));
    let [base, left, right] = [base, left, right].map(|value| {
        if normalize_maps {
            Some(value.unwrap_or(&empty))
        } else {
            value
        }
    });
    if left == right {
        return left.cloned();
    }
    if left == base {
        return right.cloned();
    }
    if right == base {
        return left.cloned();
    }
    if let (Some(base_value), Some(left_value), Some(right_value)) = (base, left, right)
        && !variant_replaced(base_value, left_value, right_value, path)
        && let (Value::Object(base), Value::Object(left), Value::Object(right)) =
            (base_value, left_value, right_value)
    {
        return Some(merge_objects(base, left, right, path, conflicts));
    }
    conflicts.push(DocumentConflict {
        path: path.clone(),
        base: base.cloned(),
        left: left.cloned(),
        right: right.cloned(),
    });
    None
}

fn merge_objects(
    base: &serde_json::Map<String, Value>,
    left: &serde_json::Map<String, Value>,
    right: &serde_json::Map<String, Value>,
    path: &mut Vec<DocumentPathSegment>,
    conflicts: &mut Vec<DocumentConflict>,
) -> Value {
    let keys: BTreeSet<_> = base.keys().chain(left.keys()).chain(right.keys()).collect();
    let mut merged = serde_json::Map::new();
    for name in keys {
        path.push(if keyed(path) {
            DocumentPathSegment::Entity(name.clone())
        } else {
            field(name)
        });
        if let Some(value) = merge_values(
            base.get(name),
            left.get(name),
            right.get(name),
            path,
            conflicts,
        ) {
            merged.insert(name.clone(), value);
        }
        path.pop();
    }
    Value::Object(merged)
}

fn restore(mut value: Value, path: &mut Vec<DocumentPathSegment>) -> Value {
    if keyed(path) {
        let Value::Object(indexed) = value else {
            unreachable!("canonical ID collection")
        };
        return Value::Array(
            indexed
                .into_iter()
                .map(|(id, item)| {
                    path.push(DocumentPathSegment::Entity(id));
                    let item = restore(item, path);
                    path.pop();
                    item
                })
                .collect(),
        );
    }
    if let Value::Object(object) = &mut value {
        for (name, item) in object.iter_mut() {
            path.push(field(name));
            *item = restore(item.take(), path);
            path.pop();
        }
    }
    value
}
