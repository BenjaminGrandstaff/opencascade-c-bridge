//! Instance graphs: clones, families, placement, assembly frames, and shared generation.

use super::*;

#[test]
fn clone_graph_inherits_sparse_overrides_and_detaches_explicitly() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_base(
            "source",
            HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
            )]),
            "user",
        )
        .unwrap();
    graph
        .add_clone(
            "middle",
            "source",
            HashMap::from([(
                "depth".into(),
                ParameterValue::Scalar(Quantity::length(40.0, LengthUnit::Millimeter)),
            )]),
            "clone",
        )
        .unwrap();
    graph
        .add_clone(
            "leaf",
            "middle",
            HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(15.0, LengthUnit::Millimeter)),
            )]),
            "clone",
        )
        .unwrap();

    graph
        .set_override(
            "source",
            "width",
            ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    let leaf = graph.resolve("leaf").unwrap();
    assert_eq!(
        leaf.overrides["width"],
        ParameterValue::Scalar(Quantity::length(15.0, LengthUnit::Millimeter))
    );
    assert_eq!(
        leaf.overrides["depth"],
        ParameterValue::Scalar(Quantity::length(40.0, LengthUnit::Millimeter))
    );

    graph.remove_override("leaf", "width").unwrap();
    assert_eq!(
        graph.resolve("leaf").unwrap().overrides["width"],
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter))
    );
    graph.detach("leaf").unwrap();
    graph
        .set_override(
            "source",
            "width",
            ParameterValue::Scalar(Quantity::length(20.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    graph
        .set_override(
            "middle",
            "depth",
            ParameterValue::Scalar(Quantity::length(50.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    let detached = graph.resolve("leaf").unwrap();
    assert!(matches!(
        graph.node("leaf"),
        Some(InstanceNode::Base { .. })
    ));
    assert_eq!(
        detached.overrides["width"],
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter))
    );
    assert_eq!(
        detached.overrides["depth"],
        ParameterValue::Scalar(Quantity::length(40.0, LengthUnit::Millimeter))
    );

    let session = Session::new().unwrap();
    let result = detached.regenerate(&session).unwrap();
    assert!((session.volume(result.shape("body").unwrap()).unwrap() - 14_400.0).abs() < 1e-6);
}

#[test]
fn clone_graph_reports_inheritance_cycles() {
    let definition = family(RequirementPriority::Required, 7_000.0);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_clone("a", "b", HashMap::new(), "test").unwrap();
    graph.add_clone("b", "a", HashMap::new(), "test").unwrap();

    let error = graph.resolve("a").err().unwrap();
    assert!(error.message.contains("a -> b -> a"));
}

#[test]
fn one_graph_regenerates_and_round_trips_multiple_families() {
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
    graph.add_base("plain", HashMap::new(), "test").unwrap();
    graph
        .add_base_from_family("marked", "MarkedBlockFamily", HashMap::new(), "test")
        .unwrap();
    graph
        .add_clone("marked-copy", "marked", HashMap::new(), "test")
        .unwrap();

    assert_eq!(graph.resolve("plain").unwrap().definition.id, "BlockFamily");
    assert_eq!(
        graph.resolve("marked-copy").unwrap().definition.id,
        "MarkedBlockFamily"
    );

    let session = Session::new().unwrap();
    let generated = graph.regenerate_all(&session).unwrap();
    // Identical parameter maps do not cause distinct families to share a
    // feature graph result.
    assert_eq!(generated.generated_variants(), 2);
    assert!(generated.result("plain").unwrap().shape("marker").is_none());
    assert!(
        generated
            .result("marked")
            .unwrap()
            .shape("marker")
            .is_some()
    );
    assert!(
        generated
            .result("marked-copy")
            .unwrap()
            .shape("marker")
            .is_some()
    );
    assert_eq!(generated.shared_from("marked-copy"), Some("marked"));
    drop(generated);

    graph.detach("marked-copy").unwrap();
    assert!(matches!(
        graph.node("marked-copy"),
        Some(InstanceNode::Base {
            family: Some(family),
            ..
        }) if family == "MarkedBlockFamily"
    ));

    let document = ModelDocument::from_graph(&graph);
    assert_eq!(document.additional_families, [secondary.clone()]);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    let loaded_graph = loaded.instance_graph().unwrap();
    assert_eq!(
        loaded_graph.resolve("marked-copy").unwrap().definition.id,
        "MarkedBlockFamily"
    );
}

#[test]
fn multi_family_graphs_reject_unknown_and_duplicate_family_ids() {
    let mut primary = family(RequirementPriority::Required, 100_000.0);
    primary.requirements.clear();
    let mut graph = InstanceGraph::new(&primary);
    assert!(graph.add_family(&primary).is_err());
    assert!(
        graph
            .add_base_from_family("unknown", "MissingFamily", HashMap::new(), "test")
            .is_err()
    );
    assert!(graph.node("unknown").is_none());

    graph.add_base("plain", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let mut unknown = document.clone();
    if let InstanceNode::Base { family, .. } = &mut unknown.instances[0] {
        *family = Some("MissingFamily".into());
    }
    let error = unknown.to_json_pretty().unwrap_err();
    assert!(
        error.message.contains("unknown family definition"),
        "{error}"
    );

    let mut duplicate = document;
    duplicate.additional_families.push(primary.clone());
    let error = duplicate.to_json_pretty().unwrap_err();
    assert!(error.message.contains("family ids"), "{error}");
}

#[test]
fn placed_instance_rotates_then_translates_every_named_output() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_base("placed-instance", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "placed-instance",
            Placement {
                translation: VectorQuantity::lengths(100.0, 0.0, 0.0, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: std::f64::consts::FRAC_PI_2,
                }),
            },
        )
        .unwrap();
    let session = Session::new().unwrap();
    let result = graph
        .resolve_with_placement("placed-instance")
        .unwrap()
        .regenerate(&session)
        .unwrap();

    let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x - 80.0).abs() < 1e-6);
    assert!((bounds.max.x - 100.0).abs() < 1e-6);
    assert!(bounds.min.y.abs() < 1e-6);
    assert!((bounds.max.y - 10.0).abs() < 1e-6);
    assert_eq!(session.shape_count().unwrap(), 2);
}

fn quarter_turn_about_z() -> Placement {
    Placement {
        translation: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        rotation: Some(AxisAngle {
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            angle_radians: std::f64::consts::FRAC_PI_2,
        }),
    }
}

/// Regenerates in a fresh session so the shape count proves that every
/// intermediate placement shape was released.
fn body_bounds(graph: &InstanceGraph<'_>, id: &str) -> occt_bridge::Bounds {
    let session = Session::new().unwrap();
    let result = graph
        .resolve_with_placement(id)
        .unwrap()
        .regenerate(&session)
        .unwrap();
    let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
    assert_eq!(session.shape_count().unwrap(), 2);
    bounds
}

fn assert_bounds(bounds: occt_bridge::Bounds, min: (f64, f64), max: (f64, f64)) {
    assert!((bounds.min.x - min.0).abs() < 1e-6, "{bounds:?}");
    assert!((bounds.min.y - min.1).abs() < 1e-6, "{bounds:?}");
    assert!((bounds.max.x - max.0).abs() < 1e-6, "{bounds:?}");
    assert!((bounds.max.y - max.1).abs() < 1e-6, "{bounds:?}");
}

#[test]
fn nested_assembly_frames_compose_placements_and_move_their_contents() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame(
            "building",
            None,
            Placement::translated(VectorQuantity::lengths(1.0, 0.0, 0.0, LengthUnit::Meter)),
            "layout",
        )
        .unwrap();
    graph
        .add_frame("row", Some("building"), quarter_turn_about_z(), "layout")
        .unwrap();
    assert!(
        graph
            .add_frame("orphan", Some("missing"), Placement::identity(), "layout")
            .unwrap_err()
            .message
            .contains("unknown assembly frame 'missing'")
    );
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_clone("pew", "source", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "pew",
            Placement::translated(VectorQuantity::lengths(
                50.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph.set_instance_frame("pew", Some("row")).unwrap();

    // Local x 50..60 turns into y 50..60, then the building shifts x by 1 m.
    assert_bounds(body_bounds(&graph, "pew"), (980.0, 50.0), (1000.0, 60.0));
    assert_bounds(body_bounds(&graph, "source"), (0.0, 0.0), (10.0, 20.0));

    graph
        .set_frame_placement(
            "building",
            Placement::translated(VectorQuantity::lengths(
                0.0,
                500.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    assert_bounds(body_bounds(&graph, "pew"), (-20.0, 550.0), (0.0, 560.0));

    graph.detach("pew").unwrap();
    assert_eq!(graph.node("pew").unwrap().frame(), Some("row"));
    graph.set_instance_frame("pew", None).unwrap();
    assert_bounds(body_bounds(&graph, "pew"), (50.0, 0.0), (60.0, 20.0));
}

#[test]
fn patterns_follow_their_assembly_frame() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame(
            "flange",
            None,
            Placement::translated(VectorQuantity::lengths(
                100.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            "layout",
        )
        .unwrap();
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_pattern(
            "bolts",
            "bolt",
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
    graph.set_pattern_frame("bolts", Some("flange")).unwrap();
    assert_eq!(graph.patterns()[0].frame.as_deref(), Some("flange"));
    assert_eq!(graph.node("bolt[3]").unwrap().frame(), Some("flange"));
    let error = graph.set_instance_frame("bolt[1]", None).unwrap_err();
    assert!(error.message.contains("set the pattern frame instead"));
    assert!(graph.set_pattern_frame("bolts", Some("missing")).is_err());

    assert_bounds(body_bounds(&graph, "bolt[1]"), (80.0, 0.0), (100.0, 10.0));

    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    assert_bounds(
        body_bounds(&loaded.instance_graph().unwrap(), "bolt[1]"),
        (80.0, 0.0),
        (100.0, 10.0),
    );
}

#[test]
fn documents_reject_invalid_assembly_frames() {
    let definition = family(RequirementPriority::Required, 100_000.0);
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame("a", None, Placement::identity(), "layout")
        .unwrap();
    graph
        .add_frame("b", Some("a"), Placement::identity(), "layout")
        .unwrap();
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "row",
            "member",
            "source",
            2,
            VectorQuantity::lengths(5.0, 0.0, 0.0, LengthUnit::Millimeter),
            "pattern",
        )
        .unwrap();
    graph.set_pattern_frame("row", Some("b")).unwrap();
    let document = ModelDocument::from_graph(&graph);
    document.to_json_pretty().unwrap();

    let mut cyclic = document.clone();
    cyclic.frames[0].parent = Some("b".into());
    let error = cyclic.to_json_pretty().unwrap_err();
    assert!(error.message.contains("assembly frame cycle: a -> b -> a"));

    let mut unknown = document.clone();
    *unknown
        .instances
        .iter_mut()
        .find(|node| node.id() == "source")
        .unwrap()
        .frame_mut() = Some("missing".into());
    let error = unknown.to_json_pretty().unwrap_err();
    assert!(error.message.contains("instance 'source'"));
    assert!(error.message.contains("unknown assembly frame 'missing'"));

    let mut mismatched = document.clone();
    *mismatched
        .instances
        .iter_mut()
        .find(|node| node.id() == "member[1]")
        .unwrap()
        .frame_mut() = Some("a".into());
    let error = mismatched.to_json_pretty().unwrap_err();
    assert!(error.message.contains("in the pattern frame"));

    let mut duplicate = document;
    duplicate.frames.push(duplicate.frames[0].clone());
    assert!(duplicate.to_json_pretty().is_err());
}

#[test]
fn clones_differing_only_in_placement_share_one_generation() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame(
            "row",
            None,
            Placement::translated(VectorQuantity::lengths(
                0.0,
                100.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            "layout",
        )
        .unwrap();
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "pews",
            "pew",
            "source",
            3,
            VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
            "pattern",
        )
        .unwrap();
    graph.set_pattern_frame("pews", Some("row")).unwrap();
    graph
        .add_clone(
            "wide",
            "source",
            HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
            )]),
            "test",
        )
        .unwrap();
    graph
        .add_clone(
            "explicit_default",
            "source",
            HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
            )]),
            "test",
        )
        .unwrap();

    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    assert_eq!(generation.generated_variants(), 2);
    assert_eq!(session.shape_count().unwrap(), 6 * 2);
    for id in ["explicit_default", "pew[0]", "pew[1]", "pew[2]", "source"] {
        assert_eq!(generation.shared_from(id), Some("explicit_default"), "{id}");
    }
    assert_eq!(generation.shared_from("wide"), Some("wide"));
    assert!(
        !generation
            .result("explicit_default")
            .unwrap()
            .regeneration
            .rebuilt
            .is_empty()
    );
    let shared = &generation.result("pew[2]").unwrap().regeneration;
    assert!(shared.rebuilt.is_empty());
    assert_eq!(shared.reused.len(), definition.features.len());

    let bounds = |id: &str| {
        session
            .bounds(generation.result(id).unwrap().shape("body").unwrap())
            .unwrap()
    };
    assert_bounds(bounds("pew[2]"), (100.0, 100.0), (110.0, 120.0));
    assert_bounds(bounds("pew[0]"), (0.0, 100.0), (10.0, 120.0));
    assert_bounds(bounds("source"), (0.0, 0.0), (10.0, 20.0));
    assert_bounds(bounds("wide"), (0.0, 0.0), (12.0, 20.0));

    let pews = graph
        .regenerate_instances(&session, &["pew[1]", "pew[0]"])
        .unwrap();
    assert_eq!(pews.generated_variants(), 1);
    assert_eq!(pews.shared_from("pew[0]"), Some("pew[1]"));
    assert!(
        graph
            .regenerate_instances(&session, &["pew[0]", "pew[0]"])
            .is_err()
    );
}

#[test]
fn shared_regeneration_failure_releases_every_created_handle() {
    let definition = family(RequirementPriority::Required, 7_000.0);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_clone("copy", "source", HashMap::new(), "test")
        .unwrap();
    graph
        .add_clone(
            "too_wide",
            "source",
            HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
            )]),
            "test",
        )
        .unwrap();

    let session = Session::new().unwrap();
    let error = graph
        .regenerate_instances(&session, &["source", "copy", "too_wide"])
        .err()
        .unwrap();
    assert!(error.message.contains("instance 'too_wide'"), "{error}");
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(
        graph
            .regenerate_instances(&session, &["missing"])
            .err()
            .unwrap()
            .message
            .contains("instance 'missing'")
    );
}

#[test]
fn invalid_placement_is_rejected_before_graph_mutation() {
    let definition = family(RequirementPriority::Required, 100_000.0);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    let invalid = Placement {
        translation: VectorQuantity::lengths(1.0, 0.0, 0.0, LengthUnit::Millimeter),
        rotation: Some(AxisAngle {
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(0.0, 0.0, 0.0),
            angle_radians: 1.0,
        }),
    };

    assert!(graph.set_placement("source", invalid).is_err());
    assert_eq!(
        graph.node("source").unwrap().placement(),
        Placement::identity()
    );
}
