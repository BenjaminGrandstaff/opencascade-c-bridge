//! Feature-graph regeneration, verification, managed generation, and incremental reuse.

use super::*;

#[test]
fn regenerates_out_of_order_features_with_units_and_named_results() {
    let definition = family(RequirementPriority::Required, 7_000.0);
    let instance = PartInstance {
        id: "block-01".into(),
        definition: &definition,
        overrides: HashMap::from([(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(1.0, LengthUnit::Centimeter)),
        )]),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();

    let placed = result.shape("placed").unwrap();
    let bounds = session.bounds(placed).unwrap();
    assert!((bounds.min.x - 10.0).abs() < 1e-6);
    assert!((bounds.min.y - 20.0).abs() < 1e-6);
    assert!((bounds.min.z - 30.0).abs() < 1e-6);
    assert!(
        result
            .verification
            .iter()
            .all(|item| item.status == VerificationStatus::Passed)
    );
    assert_eq!(session.shape_count().unwrap(), 2);
}

#[test]
fn sewing_and_multi_shell_solids_are_feature_graph_operations() {
    let millimeters =
        |x, y, z| VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter));
    let definition = FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "VoidBlock".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        features: vec![
            FeatureDefinition {
                id: "void-solid".into(),
                operation: FeatureOperation::MakeSolid {
                    shells: vec!["inner".into(), "outer".into()],
                },
            },
            FeatureDefinition {
                id: "outer-sewn".into(),
                operation: FeatureOperation::Sew {
                    inputs: vec!["outer".into()],
                    tolerance: ScalarExpr::Literal(Quantity::length(
                        1.0e-6,
                        LengthUnit::Millimeter,
                    )),
                },
            },
            FeatureDefinition {
                id: "inner".into(),
                operation: FeatureOperation::Box {
                    origin: millimeters(2.0, 2.0, 2.0),
                    size: millimeters(2.0, 2.0, 2.0),
                },
            },
            FeatureDefinition {
                id: "outer".into(),
                operation: FeatureOperation::Box {
                    origin: millimeters(0.0, 0.0, 0.0),
                    size: millimeters(10.0, 10.0, 10.0),
                },
            },
        ],
        datums: Vec::new(),
        requirements: vec![Requirement {
            id: "void.valid".into(),
            version: 1,
            kind: RequirementKind::Validation,
            priority: RequirementPriority::Required,
            statement: "The multi-shell result must be valid.".into(),
            rule: VerificationRule::ShapeValid {
                output: "void-solid".into(),
            },
            provenance: "test".into(),
            traces: Vec::new(),
        }],
    };
    let instance = PartInstance {
        id: "void-block-01".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();

    assert!(result.shape("outer-sewn").is_some());
    let solid = result.shape("void-solid").unwrap();
    assert_eq!(session.subshape_count(solid, ShapeType::Shell).unwrap(), 2);
    assert!((session.volume(solid).unwrap() - 992.0).abs() < 1e-9);

    let document = ModelDocument::from_graph(&InstanceGraph::new(&definition));
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"make_solid\""));
    assert!(json.contains("\"sew\""));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
}

#[test]
fn required_failure_rejects_generation_and_cleans_shapes() {
    let definition = family(RequirementPriority::Required, 5_500.0);
    let instance = PartInstance {
        id: "block-02".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();

    let error = instance.regenerate(&session).err().unwrap();
    assert!(error.message.contains("block.volume"));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn preferred_failure_publishes_geometry_with_diagnostic() {
    let definition = family(RequirementPriority::Preferred, 5_500.0);
    let instance = PartInstance {
        id: "block-03".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();

    assert!(result.shape("placed").is_some());
    assert_eq!(result.verification[1].status, VerificationStatus::Failed);
    assert_eq!(session.shape_count().unwrap(), 2);
}

#[test]
fn invalid_override_is_rejected_before_geometry_is_created() {
    let definition = family(RequirementPriority::Required, 7_000.0);
    let instance = PartInstance {
        id: "block-04".into(),
        definition: &definition,
        overrides: HashMap::from([(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
        )]),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();

    assert!(instance.regenerate(&session).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn dependency_cycle_is_reported_without_leaking_shapes() {
    let mut definition = family(RequirementPriority::Required, 7_000.0);
    definition.features = vec![
        FeatureDefinition {
            id: "a".into(),
            operation: FeatureOperation::Translate {
                input: "b".into(),
                offset: VectorExpr::Literal(VectorQuantity::lengths(
                    1.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
        },
        FeatureDefinition {
            id: "b".into(),
            operation: FeatureOperation::Translate {
                input: "a".into(),
                offset: VectorExpr::Literal(VectorQuantity::lengths(
                    1.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
        },
    ];
    definition.requirements.clear();
    let instance = PartInstance {
        id: "cycle".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();

    let error = instance.regenerate(&session).err().unwrap();
    assert!(error.message.contains("cycle"));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn invalid_verification_target_cleans_generated_shapes() {
    let mut definition = family(RequirementPriority::Required, 7_000.0);
    definition.requirements[0].rule = VerificationRule::ShapeValid {
        output: "missing".into(),
    };
    let instance = PartInstance {
        id: "invalid-requirement".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();

    let error = instance.regenerate(&session).err().unwrap();
    assert!(error.message.contains("block.valid"));
    assert!(error.message.contains("missing"));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn managed_regeneration_retains_stale_result_then_replaces_it() {
    let definition = family(RequirementPriority::Required, 7_000.0);
    let instance = PartInstance {
        id: "managed".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    {
        let mut managed = ManagedPartInstance::new(&session, instance);
        managed.regenerate().unwrap();
        assert_eq!(managed.state(), RegenerationState::Current);
        assert_eq!(managed.attempted_revision(), 1);
        assert_eq!(managed.accepted_revision(), Some(1));
        assert_eq!(session.shape_count().unwrap(), 2);

        managed.instance_mut().overrides.insert(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
        );
        assert!(managed.regenerate().is_err());
        assert_eq!(managed.state(), RegenerationState::Stale);
        assert_eq!(managed.attempted_revision(), 2);
        assert_eq!(managed.accepted_revision(), Some(1));
        assert!(managed.accepted().is_some());
        assert!(managed.last_error().is_some());
        assert_eq!(session.shape_count().unwrap(), 2);

        managed.instance_mut().overrides.insert(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(11.0, LengthUnit::Millimeter)),
        );
        managed.regenerate().unwrap();
        assert_eq!(managed.state(), RegenerationState::Current);
        assert_eq!(managed.attempted_revision(), 3);
        assert_eq!(managed.accepted_revision(), Some(3));
        assert!(managed.last_error().is_none());
        assert_eq!(session.shape_count().unwrap(), 2);
        assert!(
            (session
                .volume(managed.accepted().unwrap().shape("body").unwrap())
                .unwrap()
                - 6_600.0)
                .abs()
                < 1e-6
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn managed_initial_failure_has_no_accepted_result() {
    let definition = family(RequirementPriority::Required, 7_000.0);
    let instance = PartInstance {
        id: "managed-failure".into(),
        definition: &definition,
        overrides: HashMap::from([(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
        )]),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let mut managed = ManagedPartInstance::new(&session, instance);

    assert!(managed.regenerate().is_err());
    assert_eq!(managed.state(), RegenerationState::Failed);
    assert_eq!(managed.accepted_revision(), None);
    assert!(managed.accepted().is_none());
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn dropped_graph_results_release_every_generated_shape() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = pew_row(&definition);
    let session = Session::new().unwrap();
    for _ in 0..5 {
        let generation = graph.regenerate_all(&session).unwrap();
        assert!(session.shape_count().unwrap() > 0);
        drop(generation);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let kept = graph.regenerate_all(&session).unwrap().into_results();
    assert_eq!(session.shape_count().unwrap(), kept.len() * 2);
    drop(kept);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn managed_generation_can_be_frozen_and_unfrozen() {
    let definition = family(RequirementPriority::Required, 100_000.0);
    let instance = PartInstance {
        id: "frozen".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let mut managed = ManagedPartInstance::new(&session, instance);

    assert!(managed.freeze().is_err());
    managed.regenerate().unwrap();
    assert_eq!(managed.freeze().unwrap(), 1);
    assert_eq!(managed.state(), RegenerationState::Frozen);
    managed.instance_mut().overrides.insert(
        "width".into(),
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
    );
    let error = managed.regenerate().err().unwrap();
    assert!(error.message.contains("frozen"));
    assert_eq!(managed.attempted_revision(), 1);
    assert_eq!(managed.accepted_revision(), Some(1));
    assert_eq!(session.shape_count().unwrap(), 2);

    managed.unfreeze();
    managed.regenerate().unwrap();
    assert_eq!(managed.state(), RegenerationState::Current);
    assert_eq!(managed.attempted_revision(), 2);
    assert_eq!(managed.accepted_revision(), Some(2));
    assert!(
        (session
            .volume(managed.accepted().unwrap().shape("body").unwrap())
            .unwrap()
            - 7_200.0)
            .abs()
            < 1e-6
    );
}

#[test]
fn managed_regeneration_rebuilds_only_dirty_dependency_branches() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition
        .parameters
        .push(length_parameter("pin_radius", 2.0));
    definition.features.push(FeatureDefinition {
        id: "pin".into(),
        operation: FeatureOperation::Cylinder {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                50.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            radius: ScalarExpr::Parameter("pin_radius".into()),
            height: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
        },
    });
    let instance = PartInstance {
        id: "incremental".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let mut managed = ManagedPartInstance::new(&session, instance);
    managed.regenerate().unwrap();
    assert_eq!(managed.accepted().unwrap().regeneration.rebuilt.len(), 3);
    assert!(managed.accepted().unwrap().regeneration.reused.is_empty());

    managed.instance_mut().overrides.insert(
        "pin_radius".into(),
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
    );
    managed.regenerate().unwrap();
    let report = &managed.accepted().unwrap().regeneration;
    assert_eq!(report.rebuilt, ["pin"]);
    let mut reused = report.reused.clone();
    reused.sort();
    assert_eq!(reused, ["body", "placed"]);
    assert_eq!(session.shape_count().unwrap(), 3);
    assert!(
        (session
            .volume(managed.accepted().unwrap().shape("pin").unwrap())
            .unwrap()
            - std::f64::consts::PI * 90.0)
            .abs()
            < 1e-6
    );
}

#[test]
fn incremental_reuse_preserves_history_and_rolls_back_failed_downstream_work() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition
        .parameters
        .push(length_parameter("fillet_radius", 1.0));
    definition.features.push(FeatureDefinition {
        id: "filleted".into(),
        operation: FeatureOperation::Fillet {
            input: "placed".into(),
            edges: vec![EdgeSelector::History {
                source_feature: "body".into(),
                source: Box::new(EdgeSelector::NearestCenter {
                    target: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        15.0,
                        LengthUnit::Millimeter,
                    )),
                    maximum_distance: ScalarExpr::Literal(Quantity::length(
                        0.01,
                        LengthUnit::Millimeter,
                    )),
                }),
                relation: SemanticHistoryRelation::Modified,
            }],
            radius: ScalarExpr::Parameter("fillet_radius".into()),
        },
    });
    let instance = PartInstance {
        id: "incremental-history".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let mut managed = ManagedPartInstance::new(&session, instance);
    managed.regenerate().unwrap();
    assert_eq!(session.shape_count().unwrap(), 3);

    managed.instance_mut().overrides.insert(
        "fillet_radius".into(),
        ParameterValue::Scalar(Quantity::length(1.5, LengthUnit::Millimeter)),
    );
    managed.regenerate().unwrap();
    let report = &managed.accepted().unwrap().regeneration;
    assert_eq!(report.rebuilt, ["filleted"]);
    let mut reused = report.reused.clone();
    reused.sort();
    assert_eq!(reused, ["body", "placed"]);
    assert_eq!(session.shape_count().unwrap(), 3);
    assert_eq!(managed.accepted_revision(), Some(2));

    managed.instance_mut().overrides.insert(
        "fillet_radius".into(),
        ParameterValue::Scalar(Quantity::length(100.0, LengthUnit::Millimeter)),
    );
    assert!(managed.regenerate().is_err());
    assert_eq!(managed.state(), RegenerationState::Stale);
    assert_eq!(managed.accepted_revision(), Some(2));
    assert_eq!(session.shape_count().unwrap(), 3);
    assert!(
        session
            .is_valid(managed.accepted().unwrap().shape("filleted").unwrap())
            .unwrap()
    );
}
