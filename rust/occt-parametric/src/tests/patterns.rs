//! Linear, circular, and driven patterns and their members.

use super::*;

fn integer_parameter(id: &str, default: i64) -> ParameterDefinition {
    ParameterDefinition {
        id: id.into(),
        parameter_type: ParameterType::Integer,
        default: ParameterValue::Integer(default),
        minimum: None,
        maximum: None,
    }
}

#[test]
fn linear_pattern_creates_linked_clones_with_independent_placements() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    let members = graph
        .add_linear_pattern(
            "row",
            "member",
            "source",
            3,
            VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
            "pattern",
        )
        .unwrap();

    assert_eq!(members, ["member[0]", "member[1]", "member[2]"]);
    assert_eq!(graph.patterns().len(), 1);
    assert_eq!(graph.patterns()[0].source, "source");
    assert!(matches!(
        graph.node("member[2]"),
        Some(InstanceNode::Clone { source, .. }) if source == "source"
    ));

    graph
        .set_override(
            "source",
            "width",
            ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    let resolved = graph.resolve_with_placement("member[2]").unwrap();
    assert_eq!(
        resolved.instance.overrides["width"],
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter))
    );
    let session = Session::new().unwrap();
    let result = resolved.regenerate(&session).unwrap();
    let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x - 100.0).abs() < 1e-6);
    assert!((bounds.max.x - 112.0).abs() < 1e-6);
    assert_eq!(session.shape_count().unwrap(), 2);
}

#[test]
fn circular_pattern_rotates_linked_clones_about_a_typed_axis() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    let members = graph
        .add_pattern(
            "ring",
            "spoke",
            "source",
            4,
            PatternRule::Circular {
                origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                angle_step_radians: std::f64::consts::FRAC_PI_2,
            },
            "pattern",
        )
        .unwrap();

    assert_eq!(members.len(), 4);
    assert!(matches!(
        graph.patterns()[0].rule,
        PatternRule::Circular { angle_step_radians, .. }
            if angle_step_radians == std::f64::consts::FRAC_PI_2
    ));
    let session = Session::new().unwrap();
    let quarter = graph.resolve_with_placement("spoke[1]").unwrap();
    let result = quarter.regenerate(&session).unwrap();
    let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x + 20.0).abs() < 1e-6);
    assert!(bounds.max.x.abs() < 1e-6);
    assert!(bounds.min.y.abs() < 1e-6);
    assert!((bounds.max.y - 10.0).abs() < 1e-6);
    drop(result);

    let half = graph.resolve_with_placement("spoke[2]").unwrap();
    let result = half.regenerate(&session).unwrap();
    let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x + 10.0).abs() < 1e-6);
    assert!((bounds.min.y + 20.0).abs() < 1e-6);

    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
}

#[test]
fn detaching_a_pattern_member_removes_it_from_the_pattern() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame("row", None, Placement::identity(), "layout")
        .unwrap();
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "pews",
            "pew",
            "source",
            2,
            VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
            "pattern",
        )
        .unwrap();
    graph.set_pattern_frame("pews", Some("row")).unwrap();

    graph.detach("pew[1]").unwrap();
    assert_eq!(
        graph.patterns()[0].member_ids().collect::<Vec<_>>(),
        ["pew[0]"]
    );
    assert!(matches!(
        graph.node("pew[1]"),
        Some(InstanceNode::Base { .. })
    ));
    assert_eq!(graph.node("pew[1]").unwrap().frame(), Some("row"));
    // Now independent, so it may leave the pattern frame on its own.
    graph.set_instance_frame("pew[1]", None).unwrap();
    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);

    graph.detach("pew[0]").unwrap();
    assert!(graph.patterns().is_empty());
    ModelDocument::from_graph(&graph).to_json_pretty().unwrap();
}

fn along_x(millimeters: f64) -> Placement {
    Placement::translated(VectorQuantity::lengths(
        millimeters,
        0.0,
        0.0,
        LengthUnit::Millimeter,
    ))
}

fn step_x(millimeters: f64) -> PatternRule {
    PatternRule::Linear {
        step: VectorQuantity::lengths(millimeters, 0.0, 0.0, LengthUnit::Millimeter),
    }
}

#[test]
fn rule_edits_move_members_except_placement_overrides() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = pew_row(&definition);
    let placement = |graph: &InstanceGraph<'_>, id: &str| graph.node(id).unwrap().placement();

    graph.set_placement("pew[1]", along_x(500.0)).unwrap();
    assert_eq!(
        graph.patterns()[0]
            .member("pew[1]")
            .unwrap()
            .placement_override,
        Some(along_x(500.0))
    );
    graph.set_pattern_rule("pews", step_x(100.0)).unwrap();
    assert_eq!(placement(&graph, "pew[0]"), along_x(0.0));
    assert_eq!(placement(&graph, "pew[1]"), along_x(500.0));
    assert_eq!(placement(&graph, "pew[2]"), along_x(200.0));

    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);

    assert_eq!(
        graph.clear_placement_override("pew[1]").unwrap(),
        Some(along_x(500.0))
    );
    assert_eq!(placement(&graph, "pew[1]"), along_x(100.0));
    assert_eq!(graph.clear_placement_override("pew[1]").unwrap(), None);
    assert!(graph.clear_placement_override("source").is_err());
    assert!(graph.set_pattern_rule("missing", step_x(1.0)).is_err());
    assert!(
        graph
            .set_pattern_rule(
                "pews",
                PatternRule::Circular {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 0.0),
                    angle_step_radians: 1.0,
                },
            )
            .is_err()
    );
    assert_eq!(placement(&graph, "pew[2]"), along_x(200.0));

    // Moving the base instance is not a pattern override.
    graph.set_placement("source", along_x(7.0)).unwrap();
    assert!(
        graph.patterns()[0]
            .members
            .iter()
            .all(|member| member.placement_override.is_none())
    );
}

#[test]
fn member_slots_survive_detaching_a_neighbor() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = pew_row(&definition);
    graph.detach("pew[1]").unwrap();
    graph.set_pattern_rule("pews", step_x(100.0)).unwrap();
    assert_eq!(graph.node("pew[2]").unwrap().placement(), along_x(200.0));
    assert_eq!(graph.patterns()[0].member("pew[2]").unwrap().index, 2);
    // The detached instance keeps its last placement.
    assert_eq!(graph.node("pew[1]").unwrap().placement(), along_x(50.0));
    ModelDocument::from_graph(&graph).to_json_pretty().unwrap();
}

#[test]
fn suppressed_members_stay_linked_but_skip_regeneration() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = pew_row(&definition);
    graph.set_member_suppressed("pew[1]", true).unwrap();
    assert!(graph.is_suppressed("pew[1]"));
    assert!(!graph.is_suppressed("pew[0]"));
    assert!(graph.set_member_suppressed("source", true).is_err());

    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    assert!(generation.result("pew[1]").is_none());
    assert!(generation.result("pew[2]").is_some());
    assert_eq!(generation.shared_from("pew[2]"), Some("pew[0]"));
    drop(generation);
    let error = graph
        .regenerate_instances(&session, &["pew[1]"])
        .err()
        .unwrap();
    assert!(error.message.contains("suppressed"), "{error}");

    // Inheritance still reaches a suppressed member.
    graph
        .set_override(
            "source",
            "width",
            ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    assert_eq!(
        graph.resolve("pew[1]").unwrap().overrides["width"],
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter))
    );

    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    assert!(loaded.instance_graph().unwrap().is_suppressed("pew[1]"));

    graph.set_member_suppressed("pew[1]", false).unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    assert!(generation.result("pew[1]").is_some());
}

#[test]
fn schema_sixteen_member_placements_become_overrides() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = pew_row(&definition);
    graph.set_placement("pew[2]", along_x(900.0)).unwrap();
    let current = ModelDocument::from_graph(&graph);

    let mut legacy = serde_json::to_value(&current).unwrap();
    legacy["schema_version"] = serde_json::json!(16);
    legacy["patterns"][0]["members"] = serde_json::json!(["pew[0]", "pew[1]", "pew[2]"]);
    let migrated = ModelDocument::from_json(&legacy.to_string()).unwrap();
    assert_eq!(migrated, current);
    let members = &migrated.patterns[0].members;
    assert_eq!(members[1].index, 1);
    assert_eq!(members[1].placement_override, None);
    assert_eq!(members[2].placement_override, Some(along_x(900.0)));
}

#[test]
fn documents_reject_inconsistent_member_slots() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let graph = pew_row(&definition);
    let document = ModelDocument::from_graph(&graph);

    let mut moved = document.clone();
    let node = moved
        .instances
        .iter_mut()
        .find(|node| node.id() == "pew[1]")
        .unwrap();
    if let InstanceNode::Clone { placement, .. } = node {
        *placement = along_x(3.0);
    }
    let error = moved.to_json_pretty().unwrap_err();
    assert!(error.message.contains("does not match"), "{error}");

    let mut duplicated = document;
    duplicated.patterns[0].members[2].index = 0;
    let error = duplicated.to_json_pretty().unwrap_err();
    assert!(error.message.contains("slot 0"), "{error}");
}

fn member_ids(graph: &InstanceGraph<'_>) -> Vec<String> {
    let mut ids = graph.patterns()[0]
        .member_ids()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

#[test]
fn pattern_count_edits_add_and_remove_rule_slots() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = pew_row(&definition);
    graph
        .add_frame("aisle", None, along_x(1000.0), "layout")
        .unwrap();
    graph.set_pattern_frame("pews", Some("aisle")).unwrap();

    graph.set_pattern_count("pews", 5).unwrap();
    assert_eq!(graph.patterns()[0].slot_count, 5);
    assert_eq!(graph.node("pew[4]").unwrap().placement(), along_x(200.0));
    assert_eq!(graph.node("pew[4]").unwrap().frame(), Some("aisle"));
    assert!(matches!(
        graph.node("pew[3]"),
        Some(InstanceNode::Clone { source, .. }) if source == "source"
    ));

    graph.set_pattern_count("pews", 2).unwrap();
    assert_eq!(member_ids(&graph), ["pew[0]", "pew[1]"]);
    assert!(graph.node("pew[2]").is_none() && graph.node("pew[4]").is_none());

    // A detached slot stays empty when the pattern grows again.
    graph.detach("pew[1]").unwrap();
    graph.set_pattern_count("pews", 3).unwrap();
    assert_eq!(member_ids(&graph), ["pew[0]", "pew[2]"]);
    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);

    graph
        .add_clone("fan", "pew[2]", HashMap::new(), "test")
        .unwrap();
    let error = graph.set_pattern_count("pews", 1).unwrap_err();
    assert!(error.message.contains("'fan' is cloned from it"), "{error}");
    graph.add_base("pew[3]", HashMap::new(), "test").unwrap();
    let error = graph.set_pattern_count("pews", 4).unwrap_err();
    assert!(error.message.contains("'pew[3]' already exists"), "{error}");
    assert!(graph.set_pattern_count("pews", 0).is_err());
    assert!(graph.set_pattern_count("missing", 2).is_err());
    assert_eq!(graph.patterns()[0].slot_count, 3);
}

fn linear_fit(span_millimeters: f64, spacing: LinearSpacing) -> PatternRule {
    PatternRule::LinearFit {
        span: VectorQuantity::lengths(span_millimeters, 0.0, 0.0, LengthUnit::Millimeter),
        spacing,
    }
}

#[test]
fn linear_fit_solves_member_count_from_spacing_constraints() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    let minimum = LinearSpacing::Minimum(Quantity::length(900.0, LengthUnit::Millimeter));
    let members = graph
        .add_fitted_pattern(
            "pews",
            "pew",
            "source",
            linear_fit(10_000.0, minimum),
            "fit",
        )
        .unwrap();
    // floor(10000 / 900) = 11 gaps of 909.09 mm.
    assert_eq!(members.len(), 12);
    let last = graph.node("pew[11]").unwrap().placement();
    assert_eq!(last, along_x(10_000.0));
    let gap = graph
        .node("pew[1]")
        .unwrap()
        .placement()
        .translation
        .x
        .value;
    assert!((gap - 10_000.0 / 11.0).abs() < 1e-9);

    // A 9 m span at 3 m maximum spacing is exactly 3 gaps, not 4.
    let maximum = LinearSpacing::Maximum(Quantity::length(3.0, LengthUnit::Meter));
    graph
        .set_pattern_rule("pews", linear_fit(9_000.0, maximum))
        .unwrap();
    assert_eq!(member_ids(&graph).len(), 4);
    assert_eq!(graph.node("pew[3]").unwrap().placement(), along_x(9_000.0));
    assert!(graph.node("pew[4]").is_none());

    graph
        .set_pattern_rule("pews", linear_fit(9_000.0, LinearSpacing::Count(1)))
        .unwrap();
    assert_eq!(member_ids(&graph), ["pew[0]"]);
    let error = graph.set_pattern_count("pews", 3).unwrap_err();
    assert!(error.message.contains("driven by its constraints"));

    let too_many = LinearSpacing::Minimum(Quantity::length(0.01, LengthUnit::Millimeter));
    assert!(
        graph
            .set_pattern_rule("pews", linear_fit(10_000.0, too_many))
            .is_err()
    );
    let wrong_unit = LinearSpacing::Minimum(Quantity::scalar(1.0));
    assert!(
        graph
            .set_pattern_rule("pews", linear_fit(1_000.0, wrong_unit))
            .is_err()
    );
    assert!(
        graph
            .set_pattern_rule("pews", linear_fit(0.0, LinearSpacing::Count(2)))
            .is_err()
    );
    assert!(
        graph
            .add_pattern(
                "free",
                "free",
                "source",
                2,
                linear_fit(10.0, LinearSpacing::Count(2)),
                "x"
            )
            .is_err()
    );
    assert!(
        graph
            .add_fitted_pattern("free", "free", "source", step_x(10.0), "x")
            .is_err()
    );

    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
}

#[test]
fn integer_parameters_drive_pattern_counts() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition
        .parameters
        .push(integer_parameter("bolt_count", 5));
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "bolts",
            "bolt",
            "source",
            2,
            VectorQuantity::lengths(25.0, 0.0, 0.0, LengthUnit::Millimeter),
            "parameter-driven",
        )
        .unwrap();
    graph
        .set_pattern_count_driver(
            "bolts",
            Some(PatternCountDriver::Parameter {
                instance: "source".into(),
                parameter: "bolt_count".into(),
            }),
        )
        .unwrap();

    let session = Session::new().unwrap();
    let generated = graph.regenerate_all(&session).unwrap();
    assert_eq!(graph.patterns()[0].slot_count, 5);
    assert!(generated.result("bolt[4]").is_some());
    assert_eq!(graph.node("bolt[4]").unwrap().placement(), along_x(100.0));
    drop(generated);

    graph
        .set_override("source", "bolt_count", ParameterValue::Integer(3))
        .unwrap();
    let generated = graph.regenerate_all(&session).unwrap();
    assert_eq!(graph.patterns()[0].slot_count, 3);
    assert!(generated.result("bolt[2]").is_some());
    assert!(graph.node("bolt[4]").is_none());
    drop(generated);

    assert!(graph.set_pattern_count("bolts", 7).is_err());
    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);

    graph
        .set_override(
            "source",
            "width",
            ParameterValue::Scalar(Quantity::length(110.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    graph
        .set_pattern_count_driver(
            "bolts",
            Some(PatternCountDriver::BoundsExtent {
                instance: "source".into(),
                output: "body".into(),
                axis: CoordinateAxis::X,
                maximum_spacing: Quantity::length(30.0, LengthUnit::Millimeter),
            }),
        )
        .unwrap();
    let measurement_session = Session::new().unwrap();
    graph.refresh_driven_patterns(&measurement_session).unwrap();
    assert_eq!(graph.patterns()[0].slot_count, 5);
    assert_eq!(measurement_session.shape_count().unwrap(), 0);
    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
}

#[test]
fn measured_drivers_use_exact_extents_of_the_measured_family() {
    let mut primary = family(RequirementPriority::Required, 100_000.0);
    primary.requirements.clear();
    let mut secondary = primary.clone();
    secondary.id = "MarkedBlockFamily".into();
    secondary.features.push(FeatureDefinition {
        id: "marker".into(),
        operation: FeatureOperation::Cylinder {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                50.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            radius: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
            height: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
        },
    });
    let mut graph = InstanceGraph::new(&primary);
    graph.add_family(&secondary).unwrap();
    graph
        .add_base(
            "source",
            HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(120.0, LengthUnit::Millimeter)),
            )]),
            "test",
        )
        .unwrap();
    graph
        .add_base_from_family("marked", "MarkedBlockFamily", HashMap::new(), "test")
        .unwrap();
    graph
        .add_linear_pattern(
            "bolts",
            "bolt",
            "source",
            2,
            VectorQuantity::lengths(10.0, 0.0, 0.0, LengthUnit::Millimeter),
            "test",
        )
        .unwrap();
    let session = Session::new().unwrap();

    // 120 mm at 30 mm maximum spacing is exactly 4 gaps. Tolerance-padded
    // bounds measured 120.0000002 mm and produced a sixth member.
    graph
        .set_pattern_count_driver(
            "bolts",
            Some(PatternCountDriver::BoundsExtent {
                instance: "source".into(),
                output: "body".into(),
                axis: CoordinateAxis::X,
                maximum_spacing: Quantity::length(30.0, LengthUnit::Millimeter),
            }),
        )
        .unwrap();
    graph.refresh_driven_patterns(&session).unwrap();
    assert_eq!(graph.patterns()[0].slot_count, 5);

    // `marker` exists only in the secondary family of the measured
    // instance; its 5 mm curved height at 1 mm spacing is 5 gaps.
    graph
        .set_pattern_count_driver(
            "bolts",
            Some(PatternCountDriver::BoundsExtent {
                instance: "marked".into(),
                output: "marker".into(),
                axis: CoordinateAxis::Z,
                maximum_spacing: Quantity::length(1.0, LengthUnit::Millimeter),
            }),
        )
        .unwrap();
    graph.refresh_driven_patterns(&session).unwrap();
    assert_eq!(graph.patterns()[0].slot_count, 6);
    assert_eq!(session.shape_count().unwrap(), 0);

    let missing = graph
        .set_pattern_count_driver(
            "bolts",
            Some(PatternCountDriver::BoundsExtent {
                instance: "source".into(),
                output: "marker".into(),
                axis: CoordinateAxis::Z,
                maximum_spacing: Quantity::length(1.0, LengthUnit::Millimeter),
            }),
        )
        .unwrap_err();
    assert!(
        missing
            .message
            .contains("unknown pattern measurement output")
    );
}

#[test]
fn invalid_pattern_drivers_are_rejected_before_mutation() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition
        .parameters
        .push(integer_parameter("bolt_count", 4));
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "free",
            "free_member",
            "source",
            2,
            VectorQuantity::lengths(10.0, 0.0, 0.0, LengthUnit::Millimeter),
            "test",
        )
        .unwrap();

    let wrong_type = graph
        .set_pattern_count_driver(
            "free",
            Some(PatternCountDriver::Parameter {
                instance: "source".into(),
                parameter: "width".into(),
            }),
        )
        .unwrap_err();
    assert!(wrong_type.message.contains("must be an integer"));
    assert!(graph.patterns()[0].count_driver.is_none());

    let missing_output = graph
        .set_pattern_count_driver(
            "free",
            Some(PatternCountDriver::BoundsExtent {
                instance: "source".into(),
                output: "missing".into(),
                axis: CoordinateAxis::X,
                maximum_spacing: Quantity::length(10.0, LengthUnit::Millimeter),
            }),
        )
        .unwrap_err();
    assert!(
        missing_output
            .message
            .contains("unknown pattern measurement output")
    );
    assert!(graph.patterns()[0].count_driver.is_none());

    let incompatible_span = graph
        .set_pattern_span_driver(
            "free",
            Some(PatternSpanDriver::Parameter {
                instance: "source".into(),
                parameter: "width".into(),
                direction: VectorQuantity::scalars(1.0, 0.0, 0.0),
            }),
        )
        .unwrap_err();
    assert!(incompatible_span.message.contains("requires a linear_fit"));

    graph
        .add_fitted_pattern(
            "fit",
            "fit_member",
            "source",
            linear_fit(100.0, LinearSpacing::Count(3)),
            "test",
        )
        .unwrap();
    let incompatible_count = graph
        .set_pattern_count_driver(
            "fit",
            Some(PatternCountDriver::Parameter {
                instance: "source".into(),
                parameter: "bolt_count".into(),
            }),
        )
        .unwrap_err();
    assert!(incompatible_count.message.contains("freely counted"));
    let zero_direction = graph
        .set_pattern_span_driver(
            "fit",
            Some(PatternSpanDriver::Parameter {
                instance: "source".into(),
                parameter: "width".into(),
                direction: VectorQuantity::scalars(0.0, 0.0, 0.0),
            }),
        )
        .unwrap_err();
    assert!(zero_direction.message.contains("direction is zero"));
    assert!(graph.patterns()[1].span_driver.is_none());
}

#[test]
fn parameters_and_measured_geometry_drive_linear_fit_spans() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition
        .parameters
        .push(length_parameter("aisle_length", 120.0));
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_fitted_pattern(
            "pews",
            "pew",
            "source",
            linear_fit(
                30.0,
                LinearSpacing::Maximum(Quantity::length(30.0, LengthUnit::Millimeter)),
            ),
            "parameter-driven",
        )
        .unwrap();
    graph
        .set_pattern_span_driver(
            "pews",
            Some(PatternSpanDriver::Parameter {
                instance: "source".into(),
                parameter: "aisle_length".into(),
                direction: VectorQuantity::scalars(1.0, 0.0, 0.0),
            }),
        )
        .unwrap();
    let session = Session::new().unwrap();
    graph.refresh_driven_patterns(&session).unwrap();
    assert_eq!(graph.patterns()[0].slot_count, 5);
    assert_eq!(graph.node("pew[4]").unwrap().placement(), along_x(120.0));
    assert_eq!(session.shape_count().unwrap(), 0);

    graph
        .set_override(
            "source",
            "width",
            ParameterValue::Scalar(Quantity::length(110.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    graph
        .set_pattern_span_driver(
            "pews",
            Some(PatternSpanDriver::BoundsExtent {
                instance: "source".into(),
                output: "body".into(),
                axis: CoordinateAxis::X,
                direction: VectorQuantity::scalars(0.0, 1.0, 0.0),
            }),
        )
        .unwrap();
    graph.refresh_driven_patterns(&session).unwrap();
    assert_eq!(graph.patterns()[0].slot_count, 5);
    let measured_placement = graph.node("pew[4]").unwrap().placement();
    assert!(measured_placement.translation.x.value.abs() < 1e-9);
    assert!((measured_placement.translation.y.value - 110.0).abs() < 1e-9);
    assert!(measured_placement.translation.z.value.abs() < 1e-9);
    assert_eq!(session.shape_count().unwrap(), 0);
    for member in &graph.patterns()[0].members {
        assert_eq!(
            graph.node(&member.id).unwrap().placement(),
            graph.patterns()[0].member_placement(member),
            "{}",
            member.id
        );
    }

    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    let loaded = ModelDocument::from_json(&json).unwrap();
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded.patterns[0].slot_count, 5);
    assert_eq!(
        loaded.patterns[0].span_driver,
        document.patterns[0].span_driver
    );
}

fn circular_fit(sweep_radians: f64, spacing: AngularSpacing) -> PatternRule {
    PatternRule::CircularFit {
        origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
        sweep_radians,
        spacing,
    }
}

#[test]
fn circular_fit_divides_closed_and_open_sweeps() {
    use std::f64::consts::{PI, TAU};
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    let angle = |graph: &InstanceGraph<'_>, id: &str| {
        graph
            .node(id)
            .unwrap()
            .placement()
            .rotation
            .unwrap()
            .angle_radians
    };

    graph
        .add_fitted_pattern(
            "bolts",
            "bolt",
            "source",
            circular_fit(TAU, AngularSpacing::Count(6)),
            "fit",
        )
        .unwrap();
    // A closed turn does not repeat the first bolt at 360 degrees.
    assert!((angle(&graph, "bolt[5]") - 5.0 * PI / 3.0).abs() < 1e-12);

    graph
        .set_pattern_rule("bolts", circular_fit(PI, AngularSpacing::Count(5)))
        .unwrap();
    assert!((angle(&graph, "bolt[4]") - PI).abs() < 1e-12);
    assert!(graph.node("bolt[5]").is_none());

    graph
        .set_pattern_rule(
            "bolts",
            circular_fit(TAU, AngularSpacing::MaximumRadians(PI / 4.0)),
        )
        .unwrap();
    assert_eq!(member_ids(&graph).len(), 8);
    graph
        .set_pattern_rule(
            "bolts",
            circular_fit(PI, AngularSpacing::MinimumRadians(PI / 3.0)),
        )
        .unwrap();
    assert_eq!(member_ids(&graph).len(), 4);

    for invalid in [
        circular_fit(TAU, AngularSpacing::MinimumRadians(7.0)),
        circular_fit(0.0, AngularSpacing::Count(2)),
        circular_fit(7.0, AngularSpacing::Count(2)),
        circular_fit(PI, AngularSpacing::MaximumRadians(f64::NAN)),
    ] {
        assert!(
            graph.set_pattern_rule("bolts", invalid).is_err(),
            "{invalid:?}"
        );
    }
    assert_eq!(member_ids(&graph).len(), 4);
}

#[test]
fn schema_seventeen_patterns_gain_slot_counts_and_prefixes() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = pew_row(&definition);
    graph.detach("pew[2]").unwrap();
    let current = ModelDocument::from_graph(&graph);

    let mut legacy = serde_json::to_value(&current).unwrap();
    legacy["schema_version"] = serde_json::json!(17);
    let pattern = legacy["patterns"][0].as_object_mut().unwrap();
    pattern.remove("slot_count");
    pattern.remove("member_prefix");
    let migrated = ModelDocument::from_json(&legacy.to_string()).unwrap();
    // Slot 2 was detached, so the highest remaining slot sets the count.
    assert_eq!(migrated.patterns[0].slot_count, 2);
    assert_eq!(migrated.patterns[0].member_prefix, "pew");

    let mut renamed = legacy.clone();
    renamed["patterns"][0]["members"][0]["id"] = serde_json::json!("alpha");
    let instances = renamed["instances"].as_array_mut().unwrap();
    for node in instances.iter_mut() {
        let variant = node.as_object_mut().unwrap().values_mut().next().unwrap();
        if variant["id"] == "pew[0]" {
            variant["id"] = serde_json::json!("alpha");
        }
    }
    let migrated = ModelDocument::from_json(&renamed.to_string()).unwrap();
    assert_eq!(migrated.patterns[0].member_prefix, "pews");
}

#[test]
fn documents_reject_slot_counts_that_disagree_with_members_or_constraints() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let graph = pew_row(&definition);
    let document = ModelDocument::from_graph(&graph);

    let mut short = document.clone();
    short.patterns[0].slot_count = 2;
    let error = short.to_json_pretty().unwrap_err();
    assert!(
        error.message.contains("outside the 2 pattern slots"),
        "{error}"
    );

    let mut constrained = document;
    constrained.patterns[0].rule = linear_fit(100.0, LinearSpacing::Count(5));
    let error = constrained.to_json_pretty().unwrap_err();
    assert!(error.message.contains("require 5 slots"), "{error}");
}

#[test]
fn invalid_pattern_rules_are_rejected() {
    let definition = family(RequirementPriority::Required, 100_000.0);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    let origin = VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter);
    let zero_axis = graph.add_pattern(
        "ring",
        "spoke",
        "source",
        3,
        PatternRule::Circular {
            origin,
            axis: VectorQuantity::scalars(0.0, 0.0, 0.0),
            angle_step_radians: 1.0,
        },
        "pattern",
    );
    assert!(zero_axis.unwrap_err().message.contains("axis is zero"));
    let infinite_angle = graph.add_pattern(
        "ring",
        "spoke",
        "source",
        3,
        PatternRule::Circular {
            origin,
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            angle_step_radians: f64::INFINITY,
        },
        "pattern",
    );
    assert!(infinite_angle.unwrap_err().message.contains("not finite"));
    assert!(graph.patterns().is_empty());
    assert!(graph.node("spoke[0]").is_none());

    graph
        .add_pattern(
            "ring",
            "spoke",
            "source",
            2,
            PatternRule::Circular {
                origin,
                axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                angle_step_radians: 1.0,
            },
            "pattern",
        )
        .unwrap();
    let mut value = serde_json::to_value(ModelDocument::from_graph(&graph)).unwrap();
    value["patterns"][0]["rule"]["circular"]["axis"] =
        serde_json::to_value(VectorQuantity::scalars(0.0, 0.0, 0.0)).unwrap();
    let error = ModelDocument::from_json(&value.to_string()).unwrap_err();
    assert!(error.message.contains("pattern 'ring'"));
}
