//! Model documents: round trips, validation, and schema migration.

use super::*;

#[test]
fn model_document_round_trips_intent_and_regenerates_loaded_instances() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_base(
            "source",
            HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
            )]),
            "user",
        )
        .unwrap();
    graph
        .add_linear_pattern(
            "row",
            "member",
            "source",
            2,
            VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
            "pattern",
        )
        .unwrap();
    graph
        .set_override(
            "member[1]",
            "depth",
            ParameterValue::Scalar(Quantity::length(25.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    graph
        .set_relationship_tolerances(RelationshipTolerances {
            linear_millimeters: 0.002,
            angular_radians: 0.000_003,
        })
        .unwrap();
    let mut document = ModelDocument::from_graph(&graph);
    document.generation_records.push(GenerationRecord {
        instance_id: "source".into(),
        attempted_revision: 3,
        accepted_revision: Some(2),
        state: RegenerationState::Stale,
        last_error: Some("new parameters failed verification".into()),
    });

    let json = document.to_json_pretty().unwrap();
    let loaded = ModelDocument::from_json(&json).unwrap();
    assert_eq!(loaded, document);
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded.assembly.tolerances, graph.relationship_tolerances());
    assert_eq!(loaded.generation_records[0].accepted_revision, Some(2));

    let loaded_graph = loaded.instance_graph().unwrap();
    let resolved = loaded_graph.resolve_with_placement("member[1]").unwrap();
    assert_eq!(
        resolved.instance.overrides["width"],
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter))
    );
    assert_eq!(
        resolved.instance.overrides["depth"],
        ParameterValue::Scalar(Quantity::length(25.0, LengthUnit::Millimeter))
    );
    let session = Session::new().unwrap();
    let result = resolved.regenerate(&session).unwrap();
    let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x - 50.0).abs() < 1e-6);
    assert!((bounds.max.x - 62.0).abs() < 1e-6);
}

#[test]
fn schema_one_documents_migrate_missing_fields_to_current_defaults() {
    let definition = family(RequirementPriority::Required, 100_000.0);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("legacy", HashMap::new(), "import").unwrap();
    let current = ModelDocument::from_graph(&graph);
    let mut value = serde_json::to_value(current).unwrap();
    let object = value.as_object_mut().unwrap();
    object.insert("schema_version".into(), serde_json::json!(1));
    object.remove("patterns");
    object.remove("frames");
    object.remove("generation_records");
    let family = object.get_mut("family").unwrap().as_object_mut().unwrap();
    family.remove("derived_parameters");
    family.remove("derived_vector_parameters");
    family.remove("constraints");
    for instance in object.get_mut("instances").unwrap().as_array_mut().unwrap() {
        let variant = instance
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        variant.as_object_mut().unwrap().remove("placement");
        variant.as_object_mut().unwrap().remove("frame");
    }

    let migrated = ModelDocument::from_json(&serde_json::to_string(&value).unwrap()).unwrap();
    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    assert!(migrated.family.derived_parameters.is_empty());
    assert!(migrated.family.derived_vector_parameters.is_empty());
    assert!(migrated.family.constraints.is_empty());
    assert!(migrated.patterns.is_empty());
    assert!(migrated.frames.is_empty());
    assert_eq!(migrated.instances[0].frame(), None);
    assert!(migrated.generation_records.is_empty());
    assert_eq!(
        migrated.assembly.tolerances,
        RelationshipTolerances::default()
    );
    assert_eq!(migrated.instances[0].placement(), Placement::identity());
    assert!(migrated.instance_graph().unwrap().resolve("legacy").is_ok());

    for version in 2..CURRENT_SCHEMA_VERSION {
        let mut previous = serde_json::to_value(ModelDocument::from_graph(&graph)).unwrap();
        previous["schema_version"] = serde_json::json!(version);
        let migrated =
            ModelDocument::from_json(&serde_json::to_string(&previous).unwrap()).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    }
}

#[test]
fn schema_thirteen_linear_patterns_migrate_to_tagged_rules() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    let step = VectorQuantity::lengths(5.0, 0.0, 0.0, LengthUnit::Centimeter);
    graph
        .add_linear_pattern("row", "member", "source", 2, step, "pattern")
        .unwrap();
    let current = ModelDocument::from_graph(&graph);
    let mut legacy = serde_json::to_value(&current).unwrap();
    legacy["schema_version"] = serde_json::json!(13);
    let pattern = legacy["patterns"][0].as_object_mut().unwrap();
    pattern.remove("rule");
    pattern.insert("step".into(), serde_json::to_value(step).unwrap());

    let migrated = ModelDocument::from_json(&legacy.to_string()).unwrap();
    assert_eq!(migrated, current);
    assert_eq!(migrated.patterns[0].rule, PatternRule::Linear { step });

    legacy["patterns"][0]
        .as_object_mut()
        .unwrap()
        .remove("step");
    let error = ModelDocument::from_json(&legacy.to_string()).unwrap_err();
    assert!(error.message.contains("requires a step"));
}

#[test]
fn schema_thirteen_round_trips_new_selectors_and_expressions() {
    let edge_selectors = vec![
        EdgeSelector::Longest {
            allow_ties: true,
            relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
        },
        EdgeSelector::CircularRadius {
            minimum: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
            maximum: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
        },
        EdgeSelector::CurvatureRadius {
            minimum: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
            maximum: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
        },
        EdgeSelector::CurvatureRadiusRange {
            minimum: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
            maximum: ScalarExpr::Literal(Quantity::length(8.0, LengthUnit::Millimeter)),
            sample_count: 17,
            require_entire_edge: true,
        },
        EdgeSelector::CurvatureRadiusBounds {
            minimum: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
            maximum: ScalarExpr::Literal(Quantity::length(8.0, LengthUnit::Millimeter)),
            relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
            require_entire_edge: false,
        },
        EdgeSelector::Union(vec![
            EdgeSelector::Intersection(vec![EdgeSelector::Longest {
                allow_ties: true,
                relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
            }]),
            EdgeSelector::Difference {
                base: Box::new(EdgeSelector::Longest {
                    allow_ties: true,
                    relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
                }),
                subtract: Box::new(EdgeSelector::Longest {
                    allow_ties: true,
                    relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-7)),
                }),
            },
        ]),
    ];
    let face_selectors = vec![
        FaceSelector::LargestArea {
            planar_only: true,
            allow_ties: false,
            relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
        },
        FaceSelector::TangentTo {
            faces: Box::new(FaceSelector::LargestArea {
                planar_only: true,
                allow_ties: true,
                relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
            }),
            minimum_count: 1,
            angular_tolerance: Some(ScalarExpr::Literal(Quantity::scalar(1e-3))),
        },
        FaceSelector::Union(vec![FaceSelector::Intersection(vec![
            FaceSelector::Difference {
                base: Box::new(FaceSelector::LargestArea {
                    planar_only: true,
                    allow_ties: true,
                    relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
                }),
                subtract: Box::new(FaceSelector::LargestArea {
                    planar_only: false,
                    allow_ties: true,
                    relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-7)),
                }),
            },
        ])]),
    ];
    let expressions = vec![
        ScalarExpr::Negate(Box::new(ScalarExpr::Literal(Quantity::scalar(1.0)))),
        ScalarExpr::Absolute(Box::new(ScalarExpr::Literal(Quantity::scalar(-1.0)))),
        ScalarExpr::Minimum(
            Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
            Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
        ),
        ScalarExpr::Maximum(
            Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
            Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
        ),
        ScalarExpr::Clamp {
            value: Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
            minimum: Box::new(ScalarExpr::Literal(Quantity::scalar(0.0))),
            maximum: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
        },
        ScalarExpr::Conditional {
            left: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
            relation: ConstraintRelation::LessOrEqual,
            right: Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
            when_true: Box::new(ScalarExpr::Literal(Quantity::scalar(3.0))),
            when_false: Box::new(ScalarExpr::Literal(Quantity::scalar(4.0))),
        },
    ];
    let vector_expressions = vec![
        VectorExpr::Add(
            Box::new(VectorExpr::Literal(VectorQuantity::scalars(1.0, 2.0, 3.0))),
            Box::new(VectorExpr::Literal(VectorQuantity::scalars(4.0, 5.0, 6.0))),
        ),
        VectorExpr::Subtract(
            Box::new(VectorExpr::Literal(VectorQuantity::scalars(4.0, 5.0, 6.0))),
            Box::new(VectorExpr::Literal(VectorQuantity::scalars(1.0, 2.0, 3.0))),
        ),
        VectorExpr::Scale {
            vector: Box::new(VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0))),
            factor: ScalarExpr::Literal(Quantity::scalar(2.0)),
        },
        VectorExpr::Normalize(Box::new(VectorExpr::Literal(VectorQuantity::lengths(
            1.0,
            2.0,
            3.0,
            LengthUnit::Millimeter,
        )))),
    ];

    let edges_json = serde_json::to_string(&edge_selectors).unwrap();
    let face_json = serde_json::to_string(&face_selectors).unwrap();
    let expressions_json = serde_json::to_string(&expressions).unwrap();
    let vector_expressions_json = serde_json::to_string(&vector_expressions).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<EdgeSelector>>(&edges_json).unwrap(),
        edge_selectors
    );
    assert_eq!(
        serde_json::from_str::<Vec<FaceSelector>>(&face_json).unwrap(),
        face_selectors
    );
    assert_eq!(
        serde_json::from_str::<Vec<ScalarExpr>>(&expressions_json).unwrap(),
        expressions
    );
    assert_eq!(
        serde_json::from_str::<Vec<VectorExpr>>(&vector_expressions_json).unwrap(),
        vector_expressions
    );
    assert!(edges_json.contains("longest"));
    assert!(edges_json.contains("circular_radius"));
    assert!(edges_json.contains("curvature_radius"));
    assert!(edges_json.contains("curvature_radius_range"));
    assert!(edges_json.contains("intersection"));
    assert!(edges_json.contains("difference"));
    assert!(face_json.contains("largest_area"));
    assert!(face_json.contains("tangent_to"));
    assert!(face_json.contains("union"));
    assert!(expressions_json.contains("absolute"));
    assert!(expressions_json.contains("clamp"));
    assert!(expressions_json.contains("conditional"));
    assert!(vector_expressions_json.contains("normalize"));
}

#[test]
fn model_document_rejects_future_versions_and_broken_links() {
    let definition = family(RequirementPriority::Required, 100_000.0);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let mut future = serde_json::to_value(&document).unwrap();
    future["schema_version"] = serde_json::json!(CURRENT_SCHEMA_VERSION + 1);
    let error = ModelDocument::from_json(&serde_json::to_string(&future).unwrap())
        .err()
        .unwrap();
    assert!(error.message.contains("unsupported"));

    let mut broken = document;
    broken.instances.push(InstanceNode::Clone {
        id: "orphan".into(),
        source: "missing".into(),
        overrides: HashMap::new(),
        placement: Placement::identity(),
        frame: None,
        provenance: "test".into(),
    });
    let error = broken.to_json_pretty().err().unwrap();
    assert!(error.message.contains("missing"));

    let mut invalid_tolerance = ModelDocument::from_graph(&graph);
    invalid_tolerance.assembly.tolerances.linear_millimeters = f64::NAN;
    let error = invalid_tolerance.to_json_pretty().err().unwrap();
    assert!(error.message.contains("linear tolerance"));
}
