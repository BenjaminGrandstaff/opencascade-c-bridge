//! Standalone document-diff scale case: no kernel session or shared scale driver.
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

fn main() {
    let family = FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
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
    let mut before = ModelDocument::from_graph(&InstanceGraph::new(&family));
    before.instances = (0..10_000)
        .map(|index| InstanceNode::Base {
            id: format!("part-{index:05}"),
            family: None,
            overrides: HashMap::from([("enabled".into(), ParameterValue::Boolean(false))]),
            placement: Placement::default(),
            frame: None,
            provenance: "benchmark".into(),
        })
        .collect();
    let mut after = before.clone();
    after.instances.reverse();
    if let InstanceNode::Base { overrides, .. } = &mut after.instances[0] {
        overrides.insert("enabled".into(), ParameterValue::Boolean(true));
    }
    let start = Instant::now();
    for _ in 0..10 {
        let changes = before.semantic_diff(&after).expect("document diff");
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].path,
            vec![
                DocumentPathSegment::Field("instances".into()),
                DocumentPathSegment::Entity("part-09999".into()),
                DocumentPathSegment::Field("base".into()),
                DocumentPathSegment::Field("overrides".into()),
                DocumentPathSegment::Field("enabled".into()),
                DocumentPathSegment::Field("boolean".into()),
            ]
        );
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed <= Duration::from_secs(5),
        "diff exceeded 5s budget: {elapsed:?}"
    );
    println!(
        "PASS: ten diffs of 10,000 reordered instances with one exact edit: {elapsed:.3?} (5s budget); no kernel handles"
    );
}
