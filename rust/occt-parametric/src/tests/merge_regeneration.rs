//! Regenerating merged documents; the merge itself is tested in the core crate.

use super::*;

fn document() -> ModelDocument {
    let parameter = |id: &str| ParameterDefinition {
        id: id.into(),
        parameter_type: ParameterType::Boolean,
        default: ParameterValue::Boolean(false),
        minimum: None,
        maximum: None,
    };
    let family = FamilyDefinition {
        history: Default::default(),
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

fn merged(base: &ModelDocument, left: &ModelDocument, right: &ModelDocument) -> ModelDocument {
    match base.three_way_merge(left, right).unwrap() {
        DocumentMerge::Merged(document) => *document,
        DocumentMerge::Conflicts(conflicts) => panic!("unexpected conflicts: {conflicts:?}"),
    }
}

#[test]
fn merged_geometry_regenerates_with_both_dimension_edits_and_bounded_handles() {
    let mut base = document();
    base.family.parameters = ["width", "height"]
        .into_iter()
        .map(|id| ParameterDefinition {
            id: id.into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
            minimum: None,
            maximum: None,
        })
        .collect();
    let zero = || ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter));
    base.family.features.push(FeatureDefinition {
        id: "body".into(),
        operation: FeatureOperation::Box {
            origin: VectorExpr::Components {
                x: zero(),
                y: zero(),
                z: zero(),
            },
            size: VectorExpr::Components {
                x: ScalarExpr::Parameter("width".into()),
                y: ScalarExpr::Parameter("height".into()),
                z: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
            },
        },
    });
    let mut left = base.clone();
    let mut right = base.clone();
    left.family.parameters[0].default =
        ParameterValue::Scalar(Quantity::length(20.0, LengthUnit::Millimeter));
    right.family.parameters[1].default =
        ParameterValue::Scalar(Quantity::length(30.0, LengthUnit::Millimeter));
    let result = merged(&base, &left, &right);
    let session = Session::new().unwrap();
    let starting = session.shape_count().unwrap();
    for _ in 0..3 {
        let graph = result.instance_graph().unwrap();
        let generated = graph.resolve("one").unwrap().regenerate(&session).unwrap();
        let body = generated.shape("body").unwrap();
        assert!((session.volume(body).unwrap() - 3000.0).abs() < 1e-8);
        assert!(session.is_valid(body).unwrap());
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), starting);
    }
}
