use super::*;
use crate::*;

fn document() -> ModelDocument {
    let family = FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "part".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        features: vec![],
        requirements: vec![],
        datums: vec![],
    };
    ModelDocument::from_graph(&InstanceGraph::new(&family))
}
fn metadata(id: &str) -> RevisionMetadata {
    RevisionMetadata {
        id: id.into(),
        author: "Ben".into(),
        recorded_at: "2026-10-03T20:00:00Z".into(),
        message: "Edit part".into(),
    }
}

#[test]
fn records_linear_semantic_history_without_recursive_payloads() {
    let base = document();
    let mut first = base.clone();
    first.family.version = 2;
    let revision = first.record_revision(&base, metadata("r1")).unwrap();
    assert_eq!(revision.parent, None);
    assert_eq!(revision.changes.len(), 1);
    assert_eq!(revision.changes[0].after, Some(serde_json::json!(2)));
    let mut second = first.clone();
    second.family.version = 3;
    let revision = second.record_revision(&first, metadata("r2")).unwrap();
    assert_eq!(revision.parent.as_deref(), Some("r1"));
    assert_eq!(revision.changes.len(), 1);
    assert_eq!(
        ModelDocument::from_json(&second.to_json_pretty().unwrap()).unwrap(),
        second
    );
    let mut old = serde_json::to_value(&base).unwrap();
    old["schema_version"] = serde_json::json!(42);
    old.as_object_mut().unwrap().remove("revisions");
    assert!(
        ModelDocument::from_json(&old.to_string())
            .unwrap()
            .revisions
            .is_empty()
    );
    assert!(base.revisions.is_empty());
    let mut no_op = second.clone();
    assert!(no_op.record_revision(&second, metadata("r3")).is_err());
    assert_eq!(no_op, second);
}

#[test]
fn invalid_edits_metadata_history_and_duplicate_ids_roll_back() {
    let base = document();
    let mut edited = base.clone();
    edited.family.version = 2;
    edited.record_revision(&base, metadata("r1")).unwrap();
    let mut next = edited.clone();
    next.family.version = 3;
    let before = next.clone();
    assert!(next.record_revision(&edited, metadata("r1")).is_err());
    assert_eq!(next, before);
    let mut bad_metadata = metadata("r2");
    bad_metadata.author = " ".into();
    assert!(next.record_revision(&edited, bad_metadata).is_err());
    assert_eq!(next, before);
    assert!(next.record_revision(&base, metadata("r2")).is_err());
    assert_eq!(next, before);
    next.family.id.clear();
    let invalid_before = next.clone();
    assert!(next.record_revision(&edited, metadata("r2")).is_err());
    assert_eq!(next, invalid_before);
    next = before;
    next.record_revision(&edited, metadata("r2")).unwrap();
    for corruption in 0..6 {
        let mut bad = next.clone();
        match corruption {
            0 => bad.revisions[1].metadata.id = "r1".into(),
            1 => bad.revisions[1].parent = None,
            2 => bad.revisions[1].changes.clear(),
            3 => bad.revisions[1].changes[0].path.clear(),
            4 => {
                let duplicate = bad.revisions[0].changes[0].clone();
                bad.revisions[1].changes.push(duplicate);
            }
            _ => {
                bad.revisions[1].changes[0].path =
                    vec![DocumentPathSegment::Field("revisions".into())]
            }
        }
        assert!(bad.to_json_pretty().is_err(), "corruption {corruption}");
    }
    let mut bad = next.clone();
    bad.revisions[0].changes[0].before = bad.revisions[0].changes[0].after.clone();
    assert!(bad.to_json_pretty().is_err());
}

#[test]
fn concurrent_history_append_conflicts_instead_of_discarding_a_branch() {
    let base = document();
    let mut left = base.clone();
    let mut right = base.clone();
    left.family.version = 2;
    right.family.version = 3;
    left.record_revision(&base, metadata("left")).unwrap();
    right.record_revision(&base, metadata("right")).unwrap();
    let DocumentMerge::Conflicts(conflicts) = base.three_way_merge(&left, &right).unwrap() else {
        panic!("histories diverge")
    };
    assert!(
        conflicts
            .iter()
            .any(|conflict| conflict.path == vec![DocumentPathSegment::Field("revisions".into())])
    );
    assert_eq!(left.revisions.len(), 1);
    assert_eq!(right.revisions.len(), 1);
}
