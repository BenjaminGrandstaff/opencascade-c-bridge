//! Repeated three-way merging of valid 10,000-instance documents.
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

fn main() {
    let family = FamilyDefinition {
        references: Vec::new(),
        id: "part".into(),
        version: 1,
        parameters: ["enabled", "visible"]
            .into_iter()
            .map(|id| ParameterDefinition {
                id: id.into(),
                parameter_type: ParameterType::Boolean,
                default: ParameterValue::Boolean(false),
                minimum: None,
                maximum: None,
            })
            .collect(),
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        features: vec![],
        requirements: vec![],
        datums: vec![],
    };
    let mut base = ModelDocument::from_graph(&InstanceGraph::new(&family));
    base.instances = (0..10_000)
        .map(|index| InstanceNode::Base {
            id: format!("part-{index:05}"),
            family: None,
            overrides: HashMap::new(),
            placement: Placement::default(),
            frame: None,
            provenance: "benchmark".into(),
        })
        .collect();
    let mut left = base.clone();
    let mut right = base.clone();
    // Same instance, independent fields. Reorder both branch declaration lists.
    if let InstanceNode::Base { overrides, .. } = &mut left.instances[9_999] {
        overrides.insert("enabled".into(), ParameterValue::Boolean(true));
    }
    if let InstanceNode::Base { overrides, .. } = &mut right.instances[9_999] {
        overrides.insert("visible".into(), ParameterValue::Boolean(true));
    }
    left.instances.reverse();
    right.instances.rotate_left(5_000);
    let start = Instant::now();
    for _ in 0..10 {
        let DocumentMerge::Merged(result) =
            base.three_way_merge(&left, &right).expect("valid merge")
        else {
            panic!("independent edits must combine")
        };
        assert_eq!(result.instances.len(), 10_000);
        assert_eq!(result.instances[9_999].id(), "part-09999");
        assert_eq!(result.instances[9_999].overrides().len(), 2);
        assert!(
            result.instances[9_999]
                .overrides()
                .values()
                .all(|value| *value == ParameterValue::Boolean(true))
        );
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed <= Duration::from_secs(8),
        "merge exceeded 8s budget: {elapsed:?}"
    );
    println!(
        "PASS: ten validated merges of 10,000 reordered instances with independent overrides: {elapsed:.3?} (8s budget); no kernel handles"
    );
}
