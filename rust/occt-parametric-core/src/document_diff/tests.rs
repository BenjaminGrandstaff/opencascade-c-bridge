use super::*;
use crate::*;
use serde_json::json;
use std::collections::HashMap;

mod merge_tests;

fn document() -> ModelDocument {
    let parameter = |id: &str| ParameterDefinition {
        id: id.into(),
        parameter_type: ParameterType::Boolean,
        default: ParameterValue::Boolean(false),
        minimum: None,
        maximum: None,
    };
    let family = FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "part".into(),
        version: 1,
        parameters: vec![parameter("width/with~punctuation"), parameter("enabled")],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        features: vec![],
        requirements: vec![],
        datums: vec![],
    };
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("one", HashMap::new(), "test").unwrap();
    graph.add_base("two", HashMap::new(), "test").unwrap();
    ModelDocument::from_graph(&graph)
}

#[test]
fn declarations_are_matched_by_id_and_edits_are_located() {
    let before = document();
    let mut after = before.clone();
    after.instances.reverse();
    after.family.parameters.reverse();
    assert!(before.semantic_diff(&after).unwrap().is_empty());
    after.family.parameters[1].default = ParameterValue::Boolean(true);
    let changes = before.semantic_diff(&after).unwrap();
    assert_eq!(
        changes,
        vec![DocumentChange {
            path: vec![
                field("family"),
                field("parameters"),
                DocumentPathSegment::Entity("width/with~punctuation".into()),
                field("default"),
                field("boolean")
            ],
            before: Some(json!(false)),
            after: Some(json!(true)),
        }]
    );
    let encoded = serde_json::to_string(&changes).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<DocumentChange>>(&encoded).unwrap(),
        changes
    );
}

#[test]
fn instance_additions_removals_and_variant_changes_are_deterministic() {
    let before = document();
    let mut after = before.clone();
    after.instances.remove(0);
    after.instances.push(InstanceNode::Clone {
        id: "three".into(),
        source: "two".into(),
        overrides: HashMap::new(),
        placement: Placement::default(),
        frame: None,
        provenance: "test".into(),
    });
    let changes = before.semantic_diff(&after).unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(
        changes[0].path,
        vec![
            field("instances"),
            DocumentPathSegment::Entity("one".into())
        ]
    );
    assert!(changes[0].before.is_some() && changes[0].after.is_none());
    assert_eq!(
        changes[1].path,
        vec![
            field("instances"),
            DocumentPathSegment::Entity("three".into())
        ]
    );
    assert!(changes[1].before.is_none() && changes[1].after.is_some());
    let reverse = after.semantic_diff(&before).unwrap();
    for (forward, backward) in changes.iter().zip(reverse) {
        assert_eq!(forward.path, backward.path);
        assert_eq!(forward.before, backward.after);
        assert_eq!(forward.after, backward.before);
    }
    after.instances[0] = InstanceNode::Clone {
        id: "two".into(),
        source: "one".into(),
        overrides: HashMap::new(),
        placement: Placement::default(),
        frame: None,
        provenance: "test".into(),
    };
    let changes = before.semantic_diff(&after).unwrap();
    assert!(
        changes
            .iter()
            .any(|change| change.path.last() == Some(&field("base")))
    );
    assert!(
        changes
            .iter()
            .any(|change| change.path.last() == Some(&field("clone")))
    );
}

#[test]
fn duplicate_ids_are_rejected_even_for_identical_documents() {
    let mut before = document();
    before.instances.push(before.instances[0].clone());
    assert!(
        before
            .semantic_diff(&before)
            .unwrap_err()
            .message
            .contains("duplicate document diff ID 'one'")
    );
    let before = document();
    let mut after = before.clone();
    after
        .family
        .parameters
        .push(after.family.parameters[0].clone());
    assert!(before.semantic_diff(&after).is_err());
}

#[test]
fn nested_family_and_relationship_edits_use_stable_paths() {
    let mut before = document();
    before.additional_families.push(before.family.clone());
    before.assembly.relationships.push(AssemblyRelationship {
        id: "mate".into(),
        kind: RelationKind::Parallel,
        first: DatumRef {
            instance: "one".into(),
            datum: "axis".into(),
        },
        second: DatumRef {
            instance: "two".into(),
            datum: "axis".into(),
        },
    });
    let mut after = before.clone();
    after.additional_families[0].parameters[0].default = ParameterValue::Boolean(true);
    after.assembly.relationships[0].second.datum = "other".into();
    let changes = before.semantic_diff(&after).unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(
        changes[0].path[..3],
        [
            field("additional_families"),
            DocumentPathSegment::Entity("part".into()),
            field("parameters")
        ]
    );
    assert_eq!(
        changes[1].path,
        vec![
            field("assembly"),
            field("relationships"),
            DocumentPathSegment::Entity("mate".into()),
            field("second"),
            field("datum")
        ]
    );
}

#[test]
fn ordered_arrays_and_null_replacements_remain_visible() {
    let before = document();
    let mut after = before.clone();
    after.family.parameters[0].maximum = Some(Quantity::scalar(3.0));
    after.generation_records = vec![GenerationRecord {
        instance_id: "one".into(),
        attempted_revision: 2,
        accepted_revision: Some(1),
        state: RegenerationState::Stale,
        last_error: None,
    }];
    let changes = before.semantic_diff(&after).unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].before, Some(Value::Null));
    let encoded = serde_json::to_string(&changes).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<DocumentChange>>(&encoded).unwrap(),
        changes
    );
    assert_eq!(changes[1].path, vec![field("generation_records")]);
    let mut reordered = after.clone();
    reordered.generation_records.push(GenerationRecord {
        instance_id: "two".into(),
        ..after.generation_records[0].clone()
    });
    let mut reversed = reordered.clone();
    reversed.generation_records.reverse();
    assert_eq!(reordered.semantic_diff(&reversed).unwrap().len(), 1);
}

#[test]
fn omitted_empty_collections_report_added_entities_individually() {
    let before = document();
    let mut after = before.clone();
    after.family.datums.push(DatumDefinition {
        id: "origin".into(),
        kind: DatumKind::Point {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        },
    });
    let changes = before.semantic_diff(&after).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].path,
        vec![
            field("family"),
            field("datums"),
            DocumentPathSegment::Entity("origin".into())
        ]
    );
    assert!(changes[0].before.is_none());
    let encoded = serde_json::to_string(&changes).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<DocumentChange>>(&encoded).unwrap(),
        changes
    );
}

#[test]
fn features_follow_ids_while_expression_operand_order_is_preserved() {
    let mut before = document();
    let vector = || {
        VectorExpr::Literal(VectorQuantity::lengths(
            1.0,
            2.0,
            3.0,
            LengthUnit::Millimeter,
        ))
    };
    before.family.features = vec![
        FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: vector(),
                size: vector(),
            },
        },
        FeatureDefinition {
            id: "placed".into(),
            operation: FeatureOperation::Translate {
                input: "body".into(),
                offset: vector(),
            },
        },
    ];
    before
        .family
        .derived_parameters
        .push(DerivedParameterDefinition {
            id: "difference".into(),
            dimension: Dimension::Scalar,
            expression: ScalarExpr::Subtract(
                Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
                Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
            ),
        });
    let mut after = before.clone();
    after.family.features.reverse();
    assert!(before.semantic_diff(&after).unwrap().is_empty());
    if let FeatureOperation::Translate { input, .. } = &mut after.family.features[0].operation {
        *input = "other".into();
    }
    if let ScalarExpr::Subtract(left, right) = &mut after.family.derived_parameters[0].expression {
        std::mem::swap(left, right);
    }
    let changes = before.semantic_diff(&after).unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].path.last(), Some(&field("subtract")));
    assert_eq!(
        changes[1].path,
        vec![
            field("family"),
            field("features"),
            DocumentPathSegment::Entity("placed".into()),
            field("operation"),
            field("translate"),
            field("input")
        ]
    );
}
