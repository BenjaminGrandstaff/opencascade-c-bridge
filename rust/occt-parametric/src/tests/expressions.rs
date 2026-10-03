//! Derived scalar and vector expressions and constraints.

use super::*;

#[test]
fn derived_parameters_drive_geometry_and_constraints_precede_generation() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition.parameters.push(length_parameter("margin", 5.0));
    definition
        .derived_parameters
        .push(DerivedParameterDefinition {
            id: "overall_width".into(),
            dimension: Dimension::Length,
            expression: ScalarExpr::Add(
                Box::new(ScalarExpr::Parameter("width".into())),
                Box::new(ScalarExpr::Multiply(
                    Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
                    Box::new(ScalarExpr::Parameter("margin".into())),
                )),
            ),
        });
    if let FeatureOperation::Box {
        size: VectorExpr::Components { x, .. },
        ..
    } = &mut definition.features[1].operation
    {
        *x = ScalarExpr::Parameter("overall_width".into());
    }
    definition.constraints.push(ParameterConstraint {
        id: "overall-width.limit".into(),
        statement: "overall width must not exceed 40 mm".into(),
        left: ScalarExpr::Parameter("overall_width".into()),
        relation: ConstraintRelation::LessOrEqual,
        right: ScalarExpr::Literal(Quantity::length(40.0, LengthUnit::Millimeter)),
        provenance: "test".into(),
    });

    let valid = PartInstance {
        id: "derived-valid".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let valid_session = Session::new().unwrap();
    let result = valid.regenerate(&valid_session).unwrap();
    let bounds = valid_session.bounds(result.shape("body").unwrap()).unwrap();
    assert!((bounds.max.x - 20.0).abs() < 1e-6);

    let invalid = PartInstance {
        id: "derived-invalid".into(),
        definition: &definition,
        overrides: HashMap::from([(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(35.0, LengthUnit::Millimeter)),
        )]),
        provenance: "test".into(),
    };
    let invalid_session = Session::new().unwrap();
    let error = invalid.regenerate(&invalid_session).err().unwrap();
    assert!(error.message.contains("overall-width.limit"));
    assert_eq!(invalid_session.shape_count().unwrap(), 0);
}

#[test]
fn richer_scalar_functions_are_dimension_safe_and_drive_geometry() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition.derived_parameters = vec![
        DerivedParameterDefinition {
            id: "negated_width".into(),
            dimension: Dimension::Length,
            expression: ScalarExpr::Negate(Box::new(ScalarExpr::Parameter("width".into()))),
        },
        DerivedParameterDefinition {
            id: "absolute_width".into(),
            dimension: Dimension::Length,
            expression: ScalarExpr::Absolute(Box::new(ScalarExpr::Parameter(
                "negated_width".into(),
            ))),
        },
        DerivedParameterDefinition {
            id: "at_least_twelve".into(),
            dimension: Dimension::Length,
            expression: ScalarExpr::Maximum(
                Box::new(ScalarExpr::Parameter("absolute_width".into())),
                Box::new(ScalarExpr::Literal(Quantity::length(
                    12.0,
                    LengthUnit::Millimeter,
                ))),
            ),
        },
        DerivedParameterDefinition {
            id: "at_most_fifteen".into(),
            dimension: Dimension::Length,
            expression: ScalarExpr::Minimum(
                Box::new(ScalarExpr::Parameter("at_least_twelve".into())),
                Box::new(ScalarExpr::Literal(Quantity::length(
                    15.0,
                    LengthUnit::Millimeter,
                ))),
            ),
        },
        DerivedParameterDefinition {
            id: "bounded_depth".into(),
            dimension: Dimension::Length,
            expression: ScalarExpr::Clamp {
                value: Box::new(ScalarExpr::Parameter("depth".into())),
                minimum: Box::new(ScalarExpr::Parameter("at_most_fifteen".into())),
                maximum: Box::new(ScalarExpr::Literal(Quantity::length(
                    18.0,
                    LengthUnit::Millimeter,
                ))),
            },
        },
    ];
    let body = definition
        .features
        .iter_mut()
        .find(|feature| feature.id == "body")
        .unwrap();
    if let FeatureOperation::Box { size, .. } = &mut body.operation {
        *size = VectorExpr::Components {
            x: ScalarExpr::Parameter("at_most_fifteen".into()),
            y: ScalarExpr::Parameter("bounded_depth".into()),
            z: ScalarExpr::Parameter("height".into()),
        };
    } else {
        panic!("body feature must be a box");
    }

    let instance = PartInstance {
        id: "richer-expressions".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();
    let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
    assert!((bounds.max.x - 12.0).abs() < 1e-6);
    assert!((bounds.max.y - 18.0).abs() < 1e-6);
}

#[test]
fn richer_scalar_functions_reject_invalid_dimensions_and_bounds() {
    let parameters = HashMap::new();
    let mismatched = ScalarExpr::Minimum(
        Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
        Box::new(ScalarExpr::Literal(Quantity::length(
            1.0,
            LengthUnit::Millimeter,
        ))),
    );
    assert!(
        evaluate_resolved_expression(&mismatched, &parameters)
            .err()
            .unwrap()
            .message
            .contains("matching dimensions")
    );

    let reversed = ScalarExpr::Clamp {
        value: Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
        minimum: Box::new(ScalarExpr::Literal(Quantity::scalar(3.0))),
        maximum: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
    };
    assert!(
        evaluate_resolved_expression(&reversed, &parameters)
            .err()
            .unwrap()
            .message
            .contains("minimum")
    );
}

#[test]
fn conditional_scalar_expressions_choose_branches_and_drive_geometry() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition
        .derived_parameters
        .push(DerivedParameterDefinition {
            id: "selected_width".into(),
            dimension: Dimension::Length,
            expression: ScalarExpr::Conditional {
                left: Box::new(ScalarExpr::Parameter("width".into())),
                relation: ConstraintRelation::LessOrEqual,
                right: Box::new(ScalarExpr::Literal(Quantity::length(
                    15.0,
                    LengthUnit::Millimeter,
                ))),
                when_true: Box::new(ScalarExpr::Parameter("depth".into())),
                when_false: Box::new(ScalarExpr::Parameter("height".into())),
            },
        });
    let body = definition
        .features
        .iter_mut()
        .find(|feature| feature.id == "body")
        .unwrap();
    if let FeatureOperation::Box { size, .. } = &mut body.operation {
        *size = VectorExpr::Components {
            x: ScalarExpr::Parameter("selected_width".into()),
            y: ScalarExpr::Parameter("depth".into()),
            z: ScalarExpr::Parameter("height".into()),
        };
    }

    for (width, expected) in [(10.0, 20.0), (20.0, 30.0)] {
        let instance = PartInstance {
            id: "conditional".into(),
            definition: &definition,
            overrides: HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(width, LengthUnit::Millimeter)),
            )]),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();
        let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
        assert!((bounds.max.x - expected).abs() < 1e-6);
    }
}

#[test]
fn conditional_scalar_expressions_validate_comparisons_and_both_branches() {
    let mismatched_branches = ScalarExpr::Conditional {
        left: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
        relation: ConstraintRelation::GreaterOrEqual,
        right: Box::new(ScalarExpr::Literal(Quantity::scalar(0.0))),
        when_true: Box::new(ScalarExpr::Literal(Quantity::length(
            1.0,
            LengthUnit::Millimeter,
        ))),
        when_false: Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
    };
    let error = evaluate_resolved_expression(&mismatched_branches, &HashMap::new())
        .err()
        .unwrap();
    assert!(
        error
            .message
            .contains("branches require matching dimensions")
    );

    let invalid_comparison = ScalarExpr::Conditional {
        left: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
        relation: ConstraintRelation::Equal {
            tolerance: Quantity::length(0.1, LengthUnit::Millimeter),
        },
        right: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
        when_true: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
        when_false: Box::new(ScalarExpr::Literal(Quantity::scalar(0.0))),
    };
    let error = evaluate_resolved_expression(&invalid_comparison, &HashMap::new())
        .err()
        .unwrap();
    assert!(error.message.contains("tolerance has the wrong dimension"));
}

#[test]
fn derived_vector_arithmetic_and_normalization_drive_features() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition.derived_vector_parameters = vec![
        DerivedVectorParameterDefinition {
            id: "base_offset".into(),
            dimension: Dimension::Length,
            expression: VectorExpr::Components {
                x: ScalarExpr::Parameter("width".into()),
                y: ScalarExpr::Parameter("depth".into()),
                z: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
            },
        },
        DerivedVectorParameterDefinition {
            id: "shifted_offset".into(),
            dimension: Dimension::Length,
            expression: VectorExpr::Add(
                Box::new(VectorExpr::Parameter("base_offset".into())),
                Box::new(VectorExpr::Literal(VectorQuantity::lengths(
                    1.0,
                    2.0,
                    3.0,
                    LengthUnit::Millimeter,
                ))),
            ),
        },
        DerivedVectorParameterDefinition {
            id: "delta".into(),
            dimension: Dimension::Length,
            expression: VectorExpr::Subtract(
                Box::new(VectorExpr::Parameter("shifted_offset".into())),
                Box::new(VectorExpr::Parameter("base_offset".into())),
            ),
        },
        DerivedVectorParameterDefinition {
            id: "scaled_delta".into(),
            dimension: Dimension::Length,
            expression: VectorExpr::Scale {
                vector: Box::new(VectorExpr::Parameter("delta".into())),
                factor: ScalarExpr::Literal(Quantity::scalar(2.0)),
            },
        },
        DerivedVectorParameterDefinition {
            id: "axis".into(),
            dimension: Dimension::Scalar,
            expression: VectorExpr::Normalize(Box::new(VectorExpr::Parameter(
                "scaled_delta".into(),
            ))),
        },
    ];
    definition.features.push(FeatureDefinition {
        id: "vector-placed".into(),
        operation: FeatureOperation::Translate {
            input: "body".into(),
            offset: VectorExpr::Add(
                Box::new(VectorExpr::Parameter("base_offset".into())),
                Box::new(VectorExpr::Parameter("scaled_delta".into())),
            ),
        },
    });
    definition.features.push(FeatureDefinition {
        id: "vector-cylinder".into(),
        operation: FeatureOperation::Cylinder {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                50.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Parameter("axis".into()),
            radius: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
            height: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
        },
    });
    let instance = PartInstance {
        id: "derived-vectors".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();
    let bounds = session
        .bounds(result.shape("vector-placed").unwrap())
        .unwrap();
    assert!((bounds.min.x - 12.0).abs() < 1e-6);
    assert!((bounds.min.y - 24.0).abs() < 1e-6);
    assert!((bounds.min.z - 6.0).abs() < 1e-6);
    assert!(
        session
            .is_valid(result.shape("vector-cylinder").unwrap())
            .unwrap()
    );
}

#[test]
fn derived_vectors_report_cycles_and_invalid_operations() {
    let mut cycle = family(RequirementPriority::Required, 100_000.0);
    cycle.derived_vector_parameters = vec![
        DerivedVectorParameterDefinition {
            id: "a".into(),
            dimension: Dimension::Length,
            expression: VectorExpr::Parameter("b".into()),
        },
        DerivedVectorParameterDefinition {
            id: "b".into(),
            dimension: Dimension::Length,
            expression: VectorExpr::Parameter("a".into()),
        },
    ];
    let error = resolve_parameters(&cycle, &HashMap::new()).err().unwrap();
    assert!(error.message.contains("a -> b -> a"));

    let mut invalid = family(RequirementPriority::Required, 100_000.0);
    invalid
        .derived_vector_parameters
        .push(DerivedVectorParameterDefinition {
            id: "bad".into(),
            dimension: Dimension::Length,
            expression: VectorExpr::Scale {
                vector: Box::new(VectorExpr::Literal(VectorQuantity::lengths(
                    1.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                ))),
                factor: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
            },
        });
    let error = resolve_parameters(&invalid, &HashMap::new()).err().unwrap();
    assert!(error.message.contains("scale factor"));
}

#[test]
fn derived_parameter_cycles_report_the_dependency_path() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.derived_parameters = vec![
        DerivedParameterDefinition {
            id: "a".into(),
            dimension: Dimension::Length,
            expression: ScalarExpr::Parameter("b".into()),
        },
        DerivedParameterDefinition {
            id: "b".into(),
            dimension: Dimension::Length,
            expression: ScalarExpr::Parameter("a".into()),
        },
    ];
    let instance = PartInstance {
        id: "derived-cycle".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();

    let error = instance.regenerate(&session).err().unwrap();
    assert!(error.message.contains("a -> b -> a"));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn derived_expression_rejects_invalid_dimension_arithmetic() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition
        .derived_parameters
        .push(DerivedParameterDefinition {
            id: "invalid".into(),
            dimension: Dimension::Length,
            expression: ScalarExpr::Add(
                Box::new(ScalarExpr::Parameter("width".into())),
                Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
            ),
        });
    let instance = PartInstance {
        id: "dimension-error".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();

    let error = instance.regenerate(&session).err().unwrap();
    assert!(error.message.contains("matching dimensions"));
    assert_eq!(session.shape_count().unwrap(), 0);
}
