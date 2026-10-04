//! Shared feature impact and nonrecursive revision history at document scale.
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

fn document() -> ModelDocument {
    let family = FamilyDefinition {
        references: Vec::new(),
        id: "part".into(),
        version: 1,
        parameters: vec![ParameterDefinition {
            id: "width".into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
            minimum: None,
            maximum: None,
        }],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        requirements: vec![],
        datums: vec![],
        features: (0..100)
            .map(|index| FeatureDefinition {
                id: format!("feature-{index:03}"),
                operation: if index == 0 {
                    FeatureOperation::Box {
                        origin: VectorExpr::Literal(VectorQuantity::lengths(
                            0.0,
                            0.0,
                            0.0,
                            LengthUnit::Millimeter,
                        )),
                        size: VectorExpr::Components {
                            x: ScalarExpr::Parameter("width".into()),
                            y: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
                            z: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
                        },
                    }
                } else {
                    FeatureOperation::Translate {
                        input: format!("feature-{:03}", index - 1),
                        offset: VectorExpr::Literal(VectorQuantity::lengths(
                            1.0,
                            0.0,
                            0.0,
                            LengthUnit::Millimeter,
                        )),
                    }
                },
            })
            .collect(),
    };
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("source", HashMap::new(), "bench").unwrap();
    for index in 0..9_999 {
        graph
            .add_clone(
                format!("clone-{index:05}"),
                "source",
                HashMap::new(),
                "bench",
            )
            .unwrap();
    }
    ModelDocument::from_graph(&graph)
}
fn main() {
    let base = document();
    let mut after = base.clone();
    if let InstanceNode::Base { overrides, .. } = after
        .instances
        .iter_mut()
        .find(|node| node.id() == "source")
        .unwrap()
    {
        overrides.insert(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(15.0, LengthUnit::Millimeter)),
        );
    }
    let start = Instant::now();
    let report = base.change_impact(&after).unwrap();
    assert_eq!(report.instances.len(), 10_000);
    assert!(
        report
            .instances
            .iter()
            .all(|impact| impact.features.len() == 100 && impact.parameters == ["width"])
    );
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "impact exceeded 10s budget: {elapsed:?}"
    );
    println!(
        "PASS: 10,000 inherited instances, 100 dependent features, shared impact walk: {elapsed:.3?} (10s budget); no kernel handles"
    );

    let mut history_base = base;
    let change = DocumentChange {
        path: vec![
            DocumentPathSegment::Field("family".into()),
            DocumentPathSegment::Field("version".into()),
        ],
        before: Some(serde_json::json!(1)),
        after: Some(serde_json::json!(2)),
    };
    history_base.revisions = (0..10_000)
        .map(|index| DocumentRevision {
            metadata: RevisionMetadata {
                id: format!("r-{index:05}"),
                author: "bench".into(),
                recorded_at: "2026-10-03".into(),
                message: "Edit".into(),
            },
            parent: (index != 0).then(|| format!("r-{:05}", index - 1)),
            changes: vec![change.clone()],
        })
        .collect();
    let mut history_after = history_base.clone();
    history_after.family.version += 1;
    let start = Instant::now();
    let revision = history_after
        .record_revision(
            &history_base,
            RevisionMetadata {
                id: "new".into(),
                author: "bench".into(),
                recorded_at: "2026-10-03".into(),
                message: "New edit".into(),
            },
        )
        .unwrap();
    assert_eq!(revision.parent.as_deref(), Some("r-09999"));
    assert_eq!(revision.changes.len(), 1);
    assert_eq!(history_after.revisions.len(), 10_001);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "revision exceeded 10s budget: {elapsed:?}"
    );
    println!(
        "PASS: append after 10,000 revisions in a 10,000-instance document: {elapsed:.3?} (10s budget); history excluded from change payload"
    );
}
