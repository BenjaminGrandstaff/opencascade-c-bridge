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
