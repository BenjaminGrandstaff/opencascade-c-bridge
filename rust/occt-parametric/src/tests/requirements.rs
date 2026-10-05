//! Connectivity, interference, and clearance requirements with exact answers.

use super::*;

fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn cube(id: &str, origin: (f64, f64, f64), size: f64) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::Box {
            origin: point(origin.0, origin.1, origin.2),
            size: point(size, size, size),
        },
    }
}

fn requirement(id: &str, priority: RequirementPriority, rule: VerificationRule) -> Requirement {
    Requirement {
        id: id.into(),
        version: 1,
        kind: RequirementKind::Topological,
        priority,
        statement: format!("{id} holds"),
        rule,
        provenance: "test".into(),
    }
}

fn connectivity(output: &str, solids: u32, allow_voids: bool) -> VerificationRule {
    VerificationRule::Connectivity {
        output: output.into(),
        solids,
        allow_voids,
    }
}

/// `pair` is two disjoint cubes, `hollow` a cube with a sealed internal void,
/// and `skin` the sewn faces of a cube without a solid.
fn topology_family() -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        id: "Topology".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        datums: Vec::new(),
        requirements: Vec::new(),
        features: vec![
            cube("a", (0.0, 0.0, 0.0), 10.0),
            cube("b", (20.0, 0.0, 0.0), 10.0),
            FeatureDefinition {
                id: "pair".into(),
                operation: FeatureOperation::Fuse {
                    left: "a".into(),
                    right: "b".into(),
                },
            },
            cube("outer", (0.0, 0.0, 0.0), 30.0),
            cube("inner", (10.0, 10.0, 10.0), 10.0),
            FeatureDefinition {
                id: "hollow".into(),
                operation: FeatureOperation::Cut {
                    object: "outer".into(),
                    tool: "inner".into(),
                },
            },
            FeatureDefinition {
                id: "skin".into(),
                operation: FeatureOperation::Sew {
                    inputs: vec!["a".into()],
                    tolerance: ScalarExpr::Literal(Quantity::length(1e-6, LengthUnit::Millimeter)),
                },
            },
        ],
    }
}

fn verify(
    definition: &FamilyDefinition,
    rules: Vec<(&str, VerificationRule)>,
) -> Result<Vec<VerificationResult>, ModelError> {
    let mut definition = definition.clone();
    definition.requirements = rules
        .into_iter()
        .map(|(id, rule)| requirement(id, RequirementPriority::Advisory, rule))
        .collect();
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let result = part
        .regenerate(&session)
        .map(|result| result.verification.clone());
    assert_eq!(session.shape_count().unwrap(), 0, "handles released");
    result
}

#[test]
fn connectivity_counts_solids_voids_and_loose_topology_exactly() {
    let family = topology_family();
    let results = verify(
        &family,
        vec![
            ("cube", connectivity("a", 1, false)),
            ("pair.one", connectivity("pair", 1, false)),
            ("pair.two", connectivity("pair", 2, false)),
            ("hollow.sealed", connectivity("hollow", 1, false)),
            ("hollow.voids", connectivity("hollow", 1, true)),
            ("skin", connectivity("skin", 1, false)),
            (
                "volume",
                VerificationRule::VolumeRange {
                    output: "a".into(),
                    minimum: Volume {
                        value: 0.5,
                        unit: LengthUnit::Centimeter,
                    },
                    maximum: Volume {
                        value: 2.0,
                        unit: LengthUnit::Centimeter,
                    },
                },
            ),
        ],
    )
    .unwrap();
    let status = |id: &str| {
        let result = results
            .iter()
            .find(|result| result.requirement_id == id)
            .unwrap();
        assert_eq!(result.evidence, Evidence::Exact);
        (
            result.status,
            result.measured.unwrap().value,
            &result.message,
        )
    };
    assert_eq!(status("cube").0, VerificationStatus::Passed);
    let (pair_one, solids, _) = status("pair.one");
    assert_eq!((pair_one, solids), (VerificationStatus::Failed, 2.0));
    assert_eq!(status("pair.two").0, VerificationStatus::Passed);
    let (sealed, _, message) = status("hollow.sealed");
    assert_eq!(sealed, VerificationStatus::Failed);
    assert!(message.contains("up to 2 shell(s) per solid"), "{message}");
    assert_eq!(status("hollow.voids").0, VerificationStatus::Passed);
    let (skin, solids, message) = status("skin");
    assert_eq!((skin, solids), (VerificationStatus::Failed, 0.0));
    assert!(message.contains("6 loose face(s)"), "{message}");
    // Existing rules report normalized measurements too.
    let volume = results
        .iter()
        .find(|result| result.requirement_id == "volume")
        .unwrap();
    let measured = volume.measured.unwrap();
    assert!((measured.value - 1000.0).abs() < 1e-9);
    assert_eq!(measured.unit, MeasurementUnit::CubicMillimeter);
    assert_eq!(
        (measured.minimum, measured.maximum),
        (Some(500.0), Some(2000.0))
    );
}

#[test]
fn required_connectivity_failures_reject_generation_and_rules_persist() {
    let mut family = topology_family();
    family.requirements = vec![requirement(
        "pair.single",
        RequirementPriority::Required,
        connectivity("pair", 1, false),
    )];
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let error = part.regenerate(&session).err().unwrap();
    assert!(
        error
            .message
            .contains("required verification failed: pair.single")
    );
    assert_eq!(session.shape_count().unwrap(), 0);

    let error = verify(&family, vec![("none", connectivity("a", 0, false))]).unwrap_err();
    assert!(error.message.contains("at least one solid"));

    family.requirements[0].rule = connectivity("hollow", 1, true);
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"allow_voids\": true"));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    family.requirements[0].rule = connectivity("hollow", 1, false);
    let json = ModelDocument::from_graph(&InstanceGraph::new(&family))
        .to_json_pretty()
        .unwrap();
    assert!(!json.contains("allow_voids"), "default stays implicit");
}

fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}

fn at(x: f64) -> Placement {
    Placement::translated(VectorQuantity::lengths(x, 0.0, 0.0, LengthUnit::Millimeter))
}

fn body(instance: &str) -> InstanceOutputRef {
    InstanceOutputRef {
        instance: instance.into(),
        output: "body".into(),
    }
}

fn assembly_requirement(
    id: &str,
    priority: RequirementPriority,
    rule: AssemblyVerificationRule,
) -> AssemblyRequirement {
    AssemblyRequirement {
        id: id.into(),
        version: 1,
        kind: RequirementKind::Assembly,
        priority,
        statement: format!("{id} holds"),
        rule,
        provenance: "test".into(),
    }
}

fn block_family() -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition
}

/// 10 mm wide blocks: `a` spans x 0..10, `b` 10..20 (touching `a`), and `c`
/// 25..35 (5 mm from `b`, 15 mm from `a`).
fn row(definition: &FamilyDefinition) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(definition);
    for (id, x) in [("a", 0.0), ("b", 10.0), ("c", 25.0)] {
        graph.add_base(id, HashMap::new(), "test").unwrap();
        graph.set_placement(id, at(x)).unwrap();
    }
    graph
}

fn assembly_results(graph: &mut InstanceGraph<'_>) -> Result<Vec<VerificationResult>, ModelError> {
    let session = Session::new().unwrap();
    let result = graph
        .regenerate_all(&session)
        .map(|generation| generation.verification().to_vec());
    assert_eq!(session.shape_count().unwrap(), 0, "handles released");
    result
}

fn find<'a>(results: &'a [VerificationResult], id: &str) -> &'a VerificationResult {
    results
        .iter()
        .find(|result| result.requirement_id == id)
        .unwrap()
}

#[test]
fn no_interference_allows_contact_and_reports_overlap_with_witnesses() {
    let definition = block_family();
    let mut graph = row(&definition);
    graph
        .add_assembly_requirement(assembly_requirement(
            "apart",
            RequirementPriority::Advisory,
            AssemblyVerificationRule::NoInterference {
                outputs: OutputSet::AllWithOutput("body".into()),
            },
        ))
        .unwrap();
    let results = assembly_results(&mut graph).unwrap();
    let apart = find(&results, "apart");
    assert_eq!(
        apart.status,
        VerificationStatus::Passed,
        "{}",
        apart.message
    );
    assert!(apart.message.contains("among 3 outputs"));

    // Moving b to x 5..15 overlaps a by 5 x 20 x 30 mm.
    graph.set_placement("b", at(5.0)).unwrap();
    let results = assembly_results(&mut graph).unwrap();
    let apart = find(&results, "apart");
    assert_eq!(apart.status, VerificationStatus::Failed);
    assert!((apart.measured.unwrap().value - 3000.0).abs() < 1e-6);
    let witness = apart.witness.as_ref().unwrap();
    assert_eq!(witness.subjects, ["a:body", "b:body"]);
    assert_eq!(witness.points_mm.len(), 2);

    graph.assembly.requirements[0].priority = RequirementPriority::Required;
    let error = assembly_results(&mut graph).unwrap_err();
    assert!(
        error
            .message
            .contains("required assembly verification failed: apart")
    );
}

#[test]
fn minimum_clearance_within_and_between_sets() {
    let definition = block_family();
    let mut graph = row(&definition);
    let within = |id: &str, minimum: f64| {
        assembly_requirement(
            id,
            RequirementPriority::Advisory,
            AssemblyVerificationRule::MinimumClearance {
                first: OutputSet::AllWithOutput("body".into()),
                second: None,
                minimum: mm(minimum),
            },
        )
    };
    let between = |id: &str, minimum: f64| {
        assembly_requirement(
            id,
            RequirementPriority::Advisory,
            AssemblyVerificationRule::MinimumClearance {
                first: OutputSet::Explicit(vec![body("a")]),
                second: Some(OutputSet::Explicit(vec![body("c"), body("a")])),
                minimum: mm(minimum),
            },
        )
    };
    graph
        .add_assembly_requirements([
            within("within.zero", 0.0),
            within("within.three", 3.0),
            between("between.ten", 10.0),
            between("between.twenty", 20.0),
        ])
        .unwrap();
    let results = assembly_results(&mut graph).unwrap();
    // Contact satisfies a zero clearance but not a positive one.
    assert_eq!(
        find(&results, "within.zero").status,
        VerificationStatus::Passed
    );
    let three = find(&results, "within.three");
    assert_eq!(three.status, VerificationStatus::Failed);
    let measured = three.measured.unwrap();
    assert!(measured.value.abs() < 1e-6, "a and b touch");
    assert_eq!(measured.minimum, Some(3.0));
    assert!(three.message.contains("1 pair(s)"), "{}", three.message);
    // Only a-c is inspected: the touching a-b pair and a with itself are not.
    let ten = find(&results, "between.ten");
    assert_eq!(ten.status, VerificationStatus::Passed, "{}", ten.message);
    assert!(ten.message.contains("1 x 2 outputs"));
    let twenty = find(&results, "between.twenty");
    assert_eq!(twenty.status, VerificationStatus::Failed);
    assert!((twenty.measured.unwrap().value - 15.0).abs() < 1e-6);
    assert_eq!(
        twenty.witness.as_ref().unwrap().subjects,
        ["a:body", "c:body"]
    );
}

#[test]
fn output_sets_validate_skip_suppressed_members_and_persist() {
    let definition = block_family();
    let mut graph = row(&definition);
    let rule = |outputs| AssemblyVerificationRule::NoInterference { outputs };
    let invalid = [
        (rule(OutputSet::Explicit(Vec::new())), "must not be empty"),
        (
            rule(OutputSet::Explicit(vec![body("a"), body("a")])),
            "more than once",
        ),
        (
            rule(OutputSet::Explicit(vec![InstanceOutputRef {
                instance: "a".into(),
                output: "missing".into(),
            }])),
            "unknown output 'missing'",
        ),
        (rule(OutputSet::Explicit(vec![body("nobody")])), "nobody"),
        (rule(OutputSet::AllWithOutput(String::new())), "nonempty"),
        (
            AssemblyVerificationRule::MinimumClearance {
                first: OutputSet::AllWithOutput("body".into()),
                second: None,
                minimum: Quantity::scalar(1.0),
            },
            "must be a length",
        ),
        (
            AssemblyVerificationRule::MinimumClearance {
                first: OutputSet::AllWithOutput("body".into()),
                second: None,
                minimum: mm(-1.0),
            },
            "nonnegative",
        ),
    ];
    for (rule, message) in invalid {
        let error = graph
            .add_assembly_requirement(assembly_requirement(
                "bad",
                RequirementPriority::Advisory,
                rule,
            ))
            .unwrap_err();
        assert!(error.message.contains(message), "{}", error.message);
    }
    assert!(graph.assembly.requirements.is_empty());

    // An output name no instance generates cannot pass vacuously.
    graph
        .add_assembly_requirement(assembly_requirement(
            "typo",
            RequirementPriority::Advisory,
            rule(OutputSet::AllWithOutput("bodyy".into())),
        ))
        .unwrap();
    let error = assembly_results(&mut graph).unwrap_err();
    assert!(error.message.contains("matches no generated output"));
    graph.remove_assembly_requirement("typo").unwrap();

    // Suppressed instances drop out of explicit sets: b overlaps a, but is
    // suppressed in the active configuration.
    graph.set_placement("b", at(5.0)).unwrap();
    graph
        .add_assembly_requirement(assembly_requirement(
            "apart",
            RequirementPriority::Required,
            rule(OutputSet::Explicit(vec![body("a"), body("b"), body("c")])),
        ))
        .unwrap();
    graph.add_configuration("without-b").unwrap();
    graph
        .set_configuration_suppressed("without-b", "b", true)
        .unwrap();
    graph.set_active_configuration(Some("without-b")).unwrap();
    let results = assembly_results(&mut graph).unwrap();
    let apart = find(&results, "apart");
    assert_eq!(apart.status, VerificationStatus::Passed);
    assert!(apart.message.contains("among 2 outputs"));

    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
}

#[test]
fn explicitly_named_pattern_members_cannot_be_removed() {
    let definition = block_family();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "row",
            "member",
            "source",
            3,
            VectorQuantity::lengths(20.0, 0.0, 0.0, LengthUnit::Millimeter),
            "pattern",
        )
        .unwrap();
    graph
        .add_assembly_requirement(assembly_requirement(
            "gap",
            RequirementPriority::Advisory,
            AssemblyVerificationRule::MinimumClearance {
                first: OutputSet::Explicit(vec![body("member[2]")]),
                second: None,
                minimum: mm(1.0),
            },
        ))
        .unwrap();
    let error = graph.set_pattern_count("row", 2).unwrap_err();
    assert!(
        error
            .message
            .contains("named by assembly requirement 'gap'")
    );
}

fn radius_rule(
    output: &str,
    minimum: f64,
    side: RadiusSide,
    sharp_edges: SharpEdges,
) -> VerificationRule {
    VerificationRule::MinimumRadius {
        output: output.into(),
        minimum: mm(minimum),
        side,
        sharp_edges,
        samples_per_direction: DEFAULT_RADIUS_SAMPLES,
    }
}

/// A 40 x 40 x 10 plate with a 6 mm blind hole 5 mm deep (concave radius 3,
/// sharp concave bottom edge) and a separate 4 mm-radius cylinder.
fn radius_family() -> FamilyDefinition {
    let mut family = topology_family();
    family.features = vec![
        FeatureDefinition {
            id: "plate".into(),
            operation: FeatureOperation::Box {
                origin: point(0.0, 0.0, 0.0),
                size: point(40.0, 40.0, 10.0),
            },
        },
        FeatureDefinition {
            id: "drilled".into(),
            operation: FeatureOperation::Hole {
                input: "plate".into(),
                position: point(20.0, 20.0, 10.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
                diameter: ScalarExpr::Literal(mm(6.0)),
                extent: HoleExtent::Blind {
                    depth: ScalarExpr::Literal(mm(5.0)),
                },
                finish: HoleFinish::Plain,
                thread: None,
            },
        },
        FeatureDefinition {
            id: "pin".into(),
            operation: FeatureOperation::Cylinder {
                origin: point(100.0, 0.0, 0.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: ScalarExpr::Literal(mm(4.0)),
                height: ScalarExpr::Literal(mm(10.0)),
            },
        },
    ];
    family
}

#[test]
fn minimum_radius_measures_signed_faces_and_optional_sharp_edges() {
    let family = radius_family();
    let sharp = SharpEdges::ZeroRadius {
        tangency_radians: 1e-6,
    };
    let results = verify(
        &family,
        vec![
            (
                "bore.ok",
                radius_rule("drilled", 2.5, RadiusSide::Concave, SharpEdges::Ignore),
            ),
            (
                "bore.small",
                radius_rule("drilled", 3.5, RadiusSide::Concave, SharpEdges::Ignore),
            ),
            (
                "bore.corner",
                radius_rule("drilled", 1.0, RadiusSide::Concave, sharp),
            ),
            (
                "plate.convex",
                radius_rule("drilled", 1.0, RadiusSide::Convex, SharpEdges::Ignore),
            ),
            (
                "plate.edges",
                radius_rule("drilled", 1.0, RadiusSide::Convex, sharp),
            ),
            (
                "pin.exact",
                radius_rule("pin", 4.0, RadiusSide::Both, SharpEdges::Ignore),
            ),
            (
                "pin.small",
                radius_rule("pin", 4.5, RadiusSide::Convex, SharpEdges::Ignore),
            ),
        ],
    )
    .unwrap();
    let get = |id: &str| find(&results, id);
    for id in ["bore.ok", "plate.convex", "pin.exact"] {
        assert_eq!(
            get(id).status,
            VerificationStatus::Passed,
            "{}",
            get(id).message
        );
        assert_eq!(get(id).evidence, Evidence::Exact);
    }
    assert!(get("plate.convex").message.contains("no convex curvature"));
    assert!(get("plate.convex").measured.is_none());

    let small = get("bore.small");
    assert_eq!(small.status, VerificationStatus::Failed);
    let measured = small.measured.unwrap();
    assert!((measured.value - 3.0).abs() < 1e-9);
    assert_eq!(measured.minimum, Some(3.5));
    let witness = small.witness.as_ref().unwrap();
    assert!(witness.subjects[0].starts_with("face "));
    let point = witness.points_mm[0];
    assert!(
        ((point.x - 20.0).hypot(point.y - 20.0) - 3.0).abs() < 1e-9,
        "on the bore"
    );

    // The blind hole's floor meets its wall in a sharp inside corner.
    let corner = get("bore.corner");
    assert_eq!(corner.status, VerificationStatus::Failed);
    assert_eq!(corner.measured.unwrap().value, 0.0);
    assert!(
        corner.message.contains("1 sharp concave edge(s)"),
        "{}",
        corner.message
    );
    let witness = corner.witness.as_ref().unwrap();
    assert!(witness.subjects[0].starts_with("edge "));
    assert!(
        (witness.points_mm[0].z - 5.0).abs() < 1e-9,
        "on the hole floor"
    );

    // The plate's 12 outside edges and the hole rim are sharp convex corners.
    let edges = get("plate.edges");
    assert_eq!(edges.status, VerificationStatus::Failed);
    assert!(
        edges.message.contains("13 sharp convex edge(s)"),
        "{}",
        edges.message
    );

    assert_eq!(get("pin.small").status, VerificationStatus::Failed);
}

#[test]
fn freeform_radii_are_sampled_and_rules_validate_and_persist() {
    let mut family = super::variable_fillet::definition();
    let rule = |samples| VerificationRule::MinimumRadius {
        output: "blend".into(),
        minimum: mm(0.9),
        side: RadiusSide::Convex,
        sharp_edges: SharpEdges::Ignore,
        samples_per_direction: samples,
    };
    let results = verify(&family, vec![("blend", rule(DEFAULT_RADIUS_SAMPLES))]).unwrap();
    let blend = find(&results, "blend");
    assert_eq!(
        blend.status,
        VerificationStatus::Passed,
        "{}",
        blend.message
    );
    assert!((blend.measured.unwrap().value - 1.0).abs() < 1e-2);
    let Evidence::Sampled {
        samples,
        unresolved,
    } = blend.evidence
    else {
        panic!("a variable blend is freeform");
    };
    assert!(samples > 0 && samples <= 17 * 17);
    assert_eq!(unresolved, 0);

    for (rule, message) in [
        (rule(1), "samples_per_direction"),
        (
            VerificationRule::MinimumRadius {
                output: "blend".into(),
                minimum: Quantity::scalar(1.0),
                side: RadiusSide::Both,
                sharp_edges: SharpEdges::Ignore,
                samples_per_direction: DEFAULT_RADIUS_SAMPLES,
            },
            "must be a length",
        ),
        (
            VerificationRule::MinimumRadius {
                output: "blend".into(),
                minimum: mm(0.0),
                side: RadiusSide::Both,
                sharp_edges: SharpEdges::Ignore,
                samples_per_direction: DEFAULT_RADIUS_SAMPLES,
            },
            "finite and positive",
        ),
        (
            VerificationRule::MinimumRadius {
                output: "blend".into(),
                minimum: mm(1.0),
                side: RadiusSide::Both,
                sharp_edges: SharpEdges::ZeroRadius {
                    tangency_radians: 2.0,
                },
                samples_per_direction: DEFAULT_RADIUS_SAMPLES,
            },
            "tangency_radians",
        ),
    ] {
        let error = verify(&family, vec![("bad", rule)]).unwrap_err();
        assert!(error.message.contains(message), "{}", error.message);
    }

    // The default sample count stays implicit in documents.
    family.requirements = vec![requirement(
        "blend",
        RequirementPriority::Advisory,
        rule(DEFAULT_RADIUS_SAMPLES),
    )];
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"side\": \"convex\""));
    assert!(!json.contains("samples_per_direction"));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
}

fn block(id: &str, origin: (f64, f64, f64), size: (f64, f64, f64)) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::Box {
            origin: point(origin.0, origin.1, origin.2),
            size: point(size.0, size.1, size.2),
        },
    }
}

fn binary(
    id: &str,
    operation: fn(String, String) -> FeatureOperation,
    a: &str,
    b: &str,
) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation: operation(a.into(), b.into()),
    }
}

/// `cup`: a 20 x 20 x 10 box with a pocket leaving 2 mm walls and floor.
/// `tilted`: a 10 mm cube turned 5 degrees about x, so its y walls lean 5
/// degrees from the +z pull. `tee`: a 4 x 4 stem under a 14 x 14 x 2 top whose
/// underside faces straight down above the build plate.
fn manufacturing_family() -> FamilyDefinition {
    let mut family = topology_family();
    let cut = |object, tool| FeatureOperation::Cut { object, tool };
    let fuse = |left, right| FeatureOperation::Fuse { left, right };
    family.features = vec![
        block("shell", (0.0, 0.0, 0.0), (20.0, 20.0, 10.0)),
        block("pocket", (2.0, 2.0, 2.0), (16.0, 16.0, 9.0)),
        binary("cup", cut, "shell", "pocket"),
        block("cube", (0.0, 0.0, 0.0), (10.0, 10.0, 10.0)),
        FeatureDefinition {
            id: "tilted".into(),
            operation: FeatureOperation::Rotate {
                input: "cube".into(),
                origin: point(0.0, 0.0, 0.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(5f64.to_radians())),
            },
        },
        block("stem", (0.0, 0.0, 0.0), (4.0, 4.0, 10.0)),
        block("top", (-5.0, -5.0, 10.0), (14.0, 14.0, 2.0)),
        binary("tee", fuse, "stem", "top"),
    ];
    family
}

fn up() -> VectorQuantity {
    VectorQuantity::scalars(0.0, 0.0, 1.0)
}

fn wall(output: &str, minimum: f64) -> VerificationRule {
    VerificationRule::MinimumWall {
        output: output.into(),
        minimum: mm(minimum),
        mesh: MeshSettings::default(),
        maximum_samples: DEFAULT_WALL_SAMPLES,
    }
}

fn draft(output: &str, minimum_radians: f64) -> VerificationRule {
    VerificationRule::DraftAngle {
        output: output.into(),
        pull_direction: up(),
        minimum_radians,
        mesh: MeshSettings::default(),
    }
}

fn overhang(output: &str, maximum_radians: f64) -> VerificationRule {
    VerificationRule::Overhang {
        output: output.into(),
        build_direction: up(),
        maximum_radians,
        mesh: MeshSettings::default(),
    }
}

#[test]
fn manufacturing_rules_screen_walls_draft_and_overhang() {
    let family = manufacturing_family();
    let degree = 1f64.to_radians();
    let results = verify(
        &family,
        vec![
            ("cup.ok", wall("cup", 1.5)),
            ("cup.thin", wall("cup", 2.5)),
            ("cube.zero", draft("cube", 0.0)),
            ("cube.degree", draft("cube", degree)),
            ("tilted.two", draft("tilted", 2f64.to_radians())),
            ("tilted.six", draft("tilted", 6f64.to_radians())),
            (
                "cube.overhang",
                overhang("cube", std::f64::consts::FRAC_PI_4),
            ),
            ("tee.overhang", overhang("tee", std::f64::consts::FRAC_PI_4)),
        ],
    )
    .unwrap();
    let get = |id: &str| find(&results, id);
    for id in ["cup.ok", "cube.zero", "cube.overhang"] {
        assert_eq!(
            get(id).status,
            VerificationStatus::Passed,
            "{}",
            get(id).message
        );
    }
    // Draft on these planar parts is exact; walls and overhang are sampled.
    for result in &results {
        if result.requirement_id.contains("overhang") || result.requirement_id.starts_with("cup") {
            assert!(
                matches!(result.evidence, Evidence::Sampled { .. }),
                "{result:?}"
            );
        } else {
            assert_eq!(result.evidence, Evidence::Exact, "{result:?}");
        }
    }

    // Rays from the outer walls cross 2 mm of material.
    let thin = get("cup.thin");
    assert_eq!(thin.status, VerificationStatus::Failed);
    assert!(
        (thin.measured.unwrap().value - 2.0).abs() < 1e-6,
        "{}",
        thin.message
    );
    let Evidence::Sampled { samples, .. } = thin.evidence else {
        unreachable!()
    };
    assert!(samples > 0 && samples <= DEFAULT_WALL_SAMPLES);
    let witness = thin.witness.as_ref().unwrap();
    let [entry, exit] = witness.points_mm[..] else {
        panic!("entry and exit points")
    };
    let gap =
        ((entry.x - exit.x).powi(2) + (entry.y - exit.y).powi(2) + (entry.z - exit.z).powi(2))
            .sqrt();
    assert!((gap - 2.0).abs() < 1e-6);

    // Vertical walls have zero draft. Tilting 5 degrees about x gives the y
    // walls 5 degrees either way, which both release, while the x walls stay
    // vertical. Down-facing faces such as the base are not draft failures.
    let zero = get("cube.zero").measured.unwrap();
    assert!(zero.value.abs() < 1e-9);
    assert_eq!(zero.unit, MeasurementUnit::Radian);
    assert_eq!(get("cube.degree").status, VerificationStatus::Failed);
    assert!(
        get("cube.degree").message.contains("4 of 4 face(s)"),
        "{}",
        get("cube.degree").message
    );
    let two = get("tilted.two");
    assert_eq!(two.status, VerificationStatus::Failed);
    assert!(two.message.contains("2 of 6 face(s)"), "{}", two.message);
    assert!(
        two.measured.unwrap().value.abs() < 1e-9,
        "x walls stay vertical"
    );
    let six = get("tilted.six");
    assert!(six.message.contains("4 of 6 face(s)"), "{}", six.message);
    assert!(six.witness.as_ref().unwrap().subjects[0].starts_with("face "));

    // The cube's underside rests on the build plate; the tee's top does not.
    let tee = get("tee.overhang");
    assert_eq!(tee.status, VerificationStatus::Failed);
    assert!(tee.measured.unwrap().value >= 2.0);
    let point = tee.witness.as_ref().unwrap().points_mm[0];
    assert!((point.z - 10.0).abs() < 1e-9, "on the underside of the top");
}

#[test]
fn manufacturing_rules_validate_settings_and_persist_defaults_implicitly() {
    let mut family = manufacturing_family();
    let invalid = [
        (draft("cube", std::f64::consts::FRAC_PI_2), "minimum draft"),
        (overhang("cube", -0.1), "invalid manufacturing angles"),
        (
            VerificationRule::MinimumWall {
                output: "cup".into(),
                minimum: Quantity::scalar(1.0),
                mesh: MeshSettings::default(),
                maximum_samples: DEFAULT_WALL_SAMPLES,
            },
            "invalid manufacturing angles, wall units",
        ),
        (
            VerificationRule::MinimumWall {
                output: "cup".into(),
                minimum: mm(1.0),
                mesh: MeshSettings::default(),
                maximum_samples: 0,
            },
            "sample budget",
        ),
        (
            VerificationRule::DraftAngle {
                output: "cube".into(),
                pull_direction: VectorQuantity::scalars(0.0, 0.0, 0.0),
                minimum_radians: 0.0,
                mesh: MeshSettings::default(),
            },
            "nonzero",
        ),
        (
            VerificationRule::Overhang {
                output: "cube".into(),
                build_direction: up(),
                maximum_radians: 0.5,
                mesh: MeshSettings {
                    maximum_triangles: 0,
                    ..MeshSettings::default()
                },
            },
            "triangle budget",
        ),
    ];
    for (rule, message) in invalid {
        let error = verify(&family, vec![("bad", rule)]).unwrap_err();
        assert!(error.message.contains(message), "{}", error.message);
    }

    family.requirements = vec![
        requirement("wall", RequirementPriority::Advisory, wall("cup", 1.0)),
        requirement("draft", RequirementPriority::Advisory, draft("cube", 0.0)),
    ];
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(!json.contains("maximum_samples") && !json.contains("\"mesh\""));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
}

fn fits(output: &str, x: f64, y: f64, z: f64) -> VerificationRule {
    VerificationRule::FitsWithin {
        output: output.into(),
        envelope: VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter),
    }
}

#[test]
fn fits_within_compares_exact_extents_in_any_axis_aligned_orientation() {
    let family = manufacturing_family();
    // The cup is 20 x 20 x 10; a 21 x 11 x 21 bed holds it on its side.
    let results = verify(
        &family,
        vec![
            ("cup.on-side", fits("cup", 21.0, 11.0, 21.0)),
            ("cup.too-narrow", fits("cup", 25.0, 15.0, 12.0)),
            ("cup.exact", fits("cup", 10.0, 20.0, 20.0)),
        ],
    )
    .unwrap();
    let on_side = find(&results, "cup.on-side");
    assert_eq!(
        on_side.status,
        VerificationStatus::Passed,
        "{}",
        on_side.message
    );
    assert_eq!(on_side.evidence, Evidence::Exact);
    let measured = on_side.measured.unwrap();
    assert!((measured.value - 20.0).abs() < 1e-9);
    assert_eq!(measured.maximum, Some(21.0));
    assert_eq!(
        find(&results, "cup.exact").status,
        VerificationStatus::Passed
    );
    let narrow = find(&results, "cup.too-narrow");
    assert_eq!(narrow.status, VerificationStatus::Failed);
    assert_eq!(narrow.witness.as_ref().unwrap().points_mm.len(), 2);

    let error = verify(
        &family,
        vec![(
            "bad",
            VerificationRule::FitsWithin {
                output: "cup".into(),
                envelope: VectorQuantity::scalars(1.0, 1.0, 1.0),
            },
        )],
    )
    .unwrap_err();
    assert!(error.message.contains("dimension"), "{}", error.message);
    let error = verify(&family, vec![("zero", fits("cup", 0.0, 10.0, 10.0))]).unwrap_err();
    assert!(
        error.message.contains("finite and positive"),
        "{}",
        error.message
    );
}

/// `tapered`: a radius-5, 10 mm post drafted 3 degrees about its base, so its
/// side is a cone. `horn`: a smooth loft from radius 5 to 3 over 30 mm along
/// y, a freeform surface.
fn curved_draft_family() -> FamilyDefinition {
    let mut family = topology_family();
    let section = |y: f64, radius: f64| LoftSection {
        profile: (0..24)
            .map(|index| {
                let angle = std::f64::consts::TAU * index as f64 / 24.0;
                [angle.cos(), angle.sin()]
            })
            .collect(),
        origin: point(0.0, y, 0.0),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        scale: ScalarExpr::Literal(Quantity::length(radius, LengthUnit::Millimeter)),
        rotation_radians: None,
        pivot: [0.0, 0.0],
    };
    family.features = vec![
        FeatureDefinition {
            id: "post".into(),
            operation: FeatureOperation::Cylinder {
                origin: point(0.0, 0.0, 0.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: ScalarExpr::Literal(mm(5.0)),
                height: ScalarExpr::Literal(mm(10.0)),
            },
        },
        FeatureDefinition {
            id: "tapered".into(),
            operation: FeatureOperation::Draft {
                input: "post".into(),
                faces: vec![FaceSelector::LargestArea {
                    planar_only: false,
                    allow_ties: false,
                    relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
                }],
                neutral_origin: point(0.0, 0.0, 0.0),
                neutral_normal: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                pull_direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(3f64.to_radians())),
            },
        },
        FeatureDefinition {
            id: "horn".into(),
            operation: FeatureOperation::Loft {
                sections: vec![section(0.0, 5.0), section(30.0, 3.0)],
                smooth: true,
                ruled: true,
            },
        },
    ];
    family
}

#[test]
fn draft_is_exact_on_analytic_faces_and_sampled_on_freeform_ones() {
    let family = curved_draft_family();
    let degrees = |value: f64| value.to_radians();
    let along_y = |minimum_radians| VerificationRule::DraftAngle {
        output: "horn".into(),
        pull_direction: VectorQuantity::scalars(0.0, 1.0, 0.0),
        minimum_radians,
        mesh: MeshSettings::default(),
    };
    let results = verify(
        &family,
        vec![
            ("cone.under", draft("tapered", degrees(2.999))),
            ("cone.over", draft("tapered", degrees(3.001))),
            ("horn.under", along_y(degrees(3.0))),
            ("horn.over", along_y(degrees(4.5))),
        ],
    )
    .unwrap();
    let get = |id: &str| find(&results, id);

    // The drafted side is a cone measured exactly: 3 degrees, not a facet
    // approximation of it.
    let under = get("cone.under");
    assert_eq!(
        under.status,
        VerificationStatus::Passed,
        "{}",
        under.message
    );
    assert_eq!(under.evidence, Evidence::Exact);
    let measured = under.measured.unwrap().value;
    assert!((measured - degrees(3.0)).abs() < 1e-9, "{measured}");
    let over = get("cone.over");
    assert_eq!(over.status, VerificationStatus::Failed);
    assert_eq!(over.evidence, Evidence::Exact);
    let witness = over.witness.as_ref().unwrap();
    // The witness lies on the cone: radius 5 at the base, shrinking with z.
    let point = witness.points_mm[0];
    let radius = point.x.hypot(point.y);
    let expected = 5.0 - point.z * degrees(3.0).tan();
    assert!((radius - expected).abs() < 1e-6, "{point:?}");

    // The loft's freeform side tapers by atan(2 / 30), about 3.8 degrees,
    // screened on the tessellation.
    for id in ["horn.under", "horn.over"] {
        let result = get(id);
        assert!(
            matches!(result.evidence, Evidence::Sampled { samples, .. } if samples > 0),
            "{result:?}"
        );
        assert!(
            result.message.contains("sampled on the tessellation"),
            "{}",
            result.message
        );
    }
    assert_eq!(get("horn.under").status, VerificationStatus::Passed);
    assert_eq!(get("horn.over").status, VerificationStatus::Failed);
    let taper = get("horn.under").measured.unwrap().value;
    assert!((taper - (2.0f64 / 30.0).atan()).abs() < 0.01, "{taper}");
}
