use super::*;
use std::f64::consts::FRAC_PI_2;

fn length_parameter(id: &str, default: f64) -> ParameterDefinition {
    ParameterDefinition {
        id: id.into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: ParameterValue::Scalar(Quantity::length(default, LengthUnit::Millimeter)),
        minimum: Some(Quantity::length(1.0, LengthUnit::Millimeter)),
        maximum: None,
    }
}

fn parameter(id: &str) -> ScalarExpr {
    ScalarExpr::Parameter(id.into())
}

fn half(id: &str) -> ScalarExpr {
    ScalarExpr::Multiply(
        Box::new(parameter(id)),
        Box::new(ScalarExpr::Literal(Quantity::scalar(0.5))),
    )
}

fn zero() -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter))
}

fn point(x: ScalarExpr, y: ScalarExpr, z: ScalarExpr) -> VectorExpr {
    VectorExpr::Components { x, y, z }
}

fn direction(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::scalars(x, y, z))
}

/// A width x depth x height block with datums on its top, bottom, right
/// face, top center, and vertical center axis.
pub(crate) fn block() -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        id: "Block".into(),
        version: 1,
        parameters: vec![
            length_parameter("width", 10.0),
            length_parameter("depth", 20.0),
            length_parameter("height", 30.0),
        ],
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        features: vec![FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                size: point(parameter("width"), parameter("depth"), parameter("height")),
            },
        }],
        requirements: Vec::new(),
        datums: vec![
            DatumDefinition {
                id: "top_center".into(),
                kind: DatumKind::Point {
                    origin: point(half("width"), half("depth"), parameter("height")),
                },
            },
            DatumDefinition {
                id: "axis".into(),
                kind: DatumKind::Axis {
                    origin: point(half("width"), half("depth"), zero()),
                    direction: direction(0.0, 0.0, 1.0),
                },
            },
            DatumDefinition {
                id: "top".into(),
                kind: DatumKind::Plane {
                    origin: point(zero(), zero(), parameter("height")),
                    normal: direction(0.0, 0.0, 1.0),
                },
            },
            DatumDefinition {
                id: "bottom".into(),
                kind: DatumKind::Plane {
                    origin: point(zero(), zero(), zero()),
                    normal: direction(0.0, 0.0, -1.0),
                },
            },
            DatumDefinition {
                id: "right".into(),
                kind: DatumKind::Plane {
                    origin: point(parameter("width"), zero(), zero()),
                    normal: direction(1.0, 0.0, 0.0),
                },
            },
        ],
    }
}

fn width(millimeters: f64) -> ParameterValue {
    ParameterValue::Scalar(Quantity::length(millimeters, LengthUnit::Millimeter))
}

pub(crate) fn translated(x: f64, y: f64, z: f64) -> Placement {
    Placement::translated(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

pub(crate) fn assert_point(datum: ResolvedDatum, expected: (f64, f64, f64)) {
    let ResolvedDatum::Point { origin } = datum else {
        panic!("expected a point, got {datum:?}");
    };
    let error = length(subtract(
        origin,
        Vec3::new(expected.0, expected.1, expected.2),
    ));
    assert!(error < 1e-9, "{origin:?} != {expected:?}");
}

/// Block `a` at the origin and its clone `b` stacked on top of it.
pub(crate) fn stacked(definition: &FamilyDefinition) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("a", HashMap::new(), "test").unwrap();
    graph.add_clone("b", "a", HashMap::new(), "test").unwrap();
    graph
        .set_placement("b", translated(0.0, 0.0, 30.0))
        .unwrap();
    graph
}

pub(crate) fn relationship(
    id: &str,
    kind: RelationKind,
    first: (&str, &str),
    second: (&str, &str),
) -> AssemblyRelationship {
    AssemblyRelationship {
        id: id.into(),
        kind,
        first: DatumRef::new(first.0, first.1),
        second: DatumRef::new(second.0, second.1),
    }
}

#[test]
fn datums_follow_parameters_placement_and_frames() {
    let definition = block();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_base("a", HashMap::from([("width".into(), width(40.0))]), "test")
        .unwrap();
    assert_point(graph.datum("a", "top_center").unwrap(), (20.0, 10.0, 30.0));

    // A quarter turn about Z in a frame, after a local translation.
    graph
        .add_frame(
            "turned",
            None,
            Placement {
                translation: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: FRAC_PI_2,
                }),
            },
            "layout",
        )
        .unwrap();
    graph
        .set_placement("a", translated(100.0, 0.0, 0.0))
        .unwrap();
    graph.set_instance_frame("a", Some("turned")).unwrap();
    assert_point(
        graph.datum("a", "top_center").unwrap(),
        (-10.0, 120.0, 30.0),
    );
    let ResolvedDatum::Plane { normal, .. } = graph.datum("a", "right").unwrap() else {
        panic!("right is a plane");
    };
    assert!(length(subtract(normal, Vec3::new(0.0, 1.0, 0.0))) < 1e-12);

    assert!(
        graph
            .datum("a", "missing")
            .unwrap_err()
            .message
            .contains("unknown datum")
    );

    let mut flat = block();
    flat.datums[1].kind = DatumKind::Axis {
        origin: point(zero(), zero(), zero()),
        direction: direction(0.0, 0.0, 0.0),
    };
    assert!(
        validate_datums(&flat)
            .unwrap_err()
            .message
            .contains("nonzero")
    );
    let mut untyped = block();
    untyped.datums[0].kind = DatumKind::Point {
        origin: direction(1.0, 0.0, 0.0),
    };
    assert!(validate_datums(&untyped).is_err());
    let mut duplicated = block();
    duplicated.datums.push(duplicated.datums[0].clone());
    assert!(validate_datums(&duplicated).is_err());
}

#[test]
fn relationships_report_satisfied_and_violated_intent() {
    let definition = block();
    let mut graph = stacked(&definition);
    let distance =
        |millimeters| RelationKind::Distance(Quantity::length(millimeters, LengthUnit::Millimeter));
    let cases = [
        relationship(
            "seated",
            RelationKind::Coincident,
            ("a", "top"),
            ("b", "bottom"),
        ),
        relationship(
            "aligned",
            RelationKind::Coincident,
            ("a", "axis"),
            ("b", "axis"),
        ),
        relationship(
            "centered",
            RelationKind::Coincident,
            ("a", "top_center"),
            ("b", "axis"),
        ),
        relationship("level", RelationKind::Parallel, ("a", "top"), ("b", "top")),
        relationship(
            "along_face",
            RelationKind::Parallel,
            ("a", "axis"),
            ("a", "right"),
        ),
        relationship(
            "upright",
            RelationKind::Perpendicular,
            ("a", "axis"),
            ("a", "top"),
        ),
        relationship(
            "square",
            RelationKind::Perpendicular,
            ("a", "right"),
            ("b", "top"),
        ),
        relationship(
            "stack",
            distance(30.0),
            ("a", "top_center"),
            ("b", "top_center"),
        ),
        relationship("gap", distance(30.0), ("a", "top"), ("b", "top")),
        relationship(
            "in_top",
            RelationKind::Coincident,
            ("b", "axis"),
            ("a", "right"),
        ),
    ];
    for case in &cases[..9] {
        graph.add_relationship(case.clone()).unwrap();
    }
    let checks = graph.check_relationships().unwrap();
    assert!(checks.iter().all(|check| check.satisfied), "{checks:?}");

    // An axis coincident with a plane must lie in it; the vertical axis
    // does not lie in the right face.
    graph.add_relationship(cases[9].clone()).unwrap();
    let in_top = graph.check_relationships().unwrap().pop().unwrap();
    assert!(!in_top.satisfied);
    assert!((in_top.linear_residual.unwrap() - 5.0).abs() < 1e-9);

    graph
        .set_placement("b", translated(0.0, 0.0, 31.0))
        .unwrap();
    let checks = graph.check_relationships().unwrap();
    let seated = checks.iter().find(|check| check.id == "seated").unwrap();
    assert!(!seated.satisfied);
    assert!((seated.linear_residual.unwrap() - 1.0).abs() < 1e-9);
    assert_eq!(seated.angular_residual, Some(0.0));
    assert!(
        checks
            .iter()
            .find(|check| check.id == "aligned")
            .unwrap()
            .satisfied
    );

    let rejected = [
        relationship(
            "seated",
            RelationKind::Coincident,
            ("a", "top"),
            ("b", "bottom"),
        ),
        relationship(
            "points",
            RelationKind::Parallel,
            ("a", "top_center"),
            ("b", "top"),
        ),
        relationship("mixed", distance(1.0), ("a", "axis"), ("b", "top")),
        relationship(
            "scalar",
            RelationKind::Distance(Quantity::scalar(1.0)),
            ("a", "top"),
            ("b", "top"),
        ),
        relationship(
            "unknown",
            RelationKind::Coincident,
            ("a", "missing"),
            ("b", "top"),
        ),
        relationship(
            "nobody",
            RelationKind::Coincident,
            ("z", "top"),
            ("b", "top"),
        ),
    ];
    for case in rejected {
        assert!(graph.add_relationship(case.clone()).is_err(), "{case:?}");
    }
    assert_eq!(graph.remove_relationship("in_top").unwrap().id, "in_top");
    assert!(graph.remove_relationship("in_top").is_err());
}

#[test]
fn relationship_checks_use_validated_model_tolerances() {
    let definition = block();
    let mut graph = stacked(&definition);
    graph
        .add_relationship(relationship(
            "seated",
            RelationKind::Coincident,
            ("a", "top"),
            ("b", "bottom"),
        ))
        .unwrap();
    graph
        .set_placement("b", translated(0.0, 0.0, 30.000_5))
        .unwrap();
    assert!(!graph.check_relationships().unwrap()[0].satisfied);

    let tolerances = RelationshipTolerances {
        linear_millimeters: 0.001,
        angular_radians: 0.000_02,
    };
    graph.set_relationship_tolerances(tolerances).unwrap();
    assert_eq!(graph.relationship_tolerances(), tolerances);
    assert!(graph.check_relationships().unwrap()[0].satisfied);

    graph.remove_relationship("seated").unwrap();
    graph
        .set_placement(
            "b",
            Placement {
                translation: VectorQuantity::lengths(0.0, 0.0, 30.0, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                    angle_radians: 0.000_01,
                }),
            },
        )
        .unwrap();
    graph
        .add_relationship(relationship(
            "level",
            RelationKind::Parallel,
            ("a", "top"),
            ("b", "top"),
        ))
        .unwrap();
    assert!(graph.check_relationships().unwrap()[0].satisfied);

    for invalid in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        let error = graph
            .set_relationship_tolerances(RelationshipTolerances {
                linear_millimeters: invalid,
                angular_radians: RELATIONSHIP_ANGULAR_TOLERANCE,
            })
            .unwrap_err();
        assert!(error.message.contains("linear tolerance"));

        let error = graph
            .set_relationship_tolerances(RelationshipTolerances {
                linear_millimeters: RELATIONSHIP_LINEAR_TOLERANCE,
                angular_radians: invalid,
            })
            .unwrap_err();
        assert!(error.message.contains("angular tolerance"));
    }
    assert_eq!(graph.relationship_tolerances(), tolerances);
}

#[test]
fn configurations_layer_overrides_and_suppression_over_the_base_graph() {
    let definition = block();
    let mut graph = stacked(&definition);
    graph.add_configuration("wide").unwrap();
    graph.add_configuration("single").unwrap();
    assert!(graph.add_configuration("wide").is_err());
    graph
        .set_configuration_override("wide", "a", "width", width(40.0))
        .unwrap();
    graph
        .set_configuration_suppressed("single", "b", true)
        .unwrap();

    // Invalid overrides are rejected without changing the configuration.
    assert!(
        graph
            .set_configuration_override("wide", "a", "width", width(0.5))
            .is_err()
    );
    assert!(
        graph
            .set_configuration_override("wide", "missing", "width", width(5.0))
            .is_err()
    );
    assert!(graph.set_active_configuration(Some("missing")).is_err());

    assert_point(graph.datum("b", "top_center").unwrap(), (5.0, 10.0, 60.0));
    graph.set_active_configuration(Some("wide")).unwrap();
    assert_eq!(graph.active_configuration(), Some("wide"));
    // The clone inherits the configured width of its source.
    assert_point(graph.datum("b", "top_center").unwrap(), (20.0, 10.0, 60.0));

    let session = Session::new().unwrap();
    graph.set_active_configuration(Some("single")).unwrap();
    assert!(graph.is_suppressed("b"));
    let generation = graph.regenerate_all(&session).unwrap();
    assert!(generation.result("a").is_some() && generation.result("b").is_none());
    drop(generation);

    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    let reloaded = loaded.instance_graph().unwrap();
    assert_eq!(reloaded.active_configuration(), None);
    assert!(!reloaded.is_suppressed("b"));

    graph.set_active_configuration(None).unwrap();
    assert_point(graph.datum("b", "top_center").unwrap(), (5.0, 10.0, 60.0));
    assert_eq!(
        graph
            .remove_configuration_override("wide", "a", "width")
            .unwrap(),
        Some(width(40.0))
    );
    assert!(graph.assembly().configurations[0].overrides.is_empty());
}

#[test]
fn materials_are_inherited_and_give_mass() {
    let definition = block();
    let mut graph = stacked(&definition);
    let steel = Material {
        id: "steel".into(),
        name: "Structural steel".into(),
        density_kg_per_cubic_meter: 7850.0,
    };
    graph.add_material(steel.clone()).unwrap();
    assert!(graph.add_material(steel.clone()).is_err());
    assert!(
        graph
            .add_material(Material {
                density_kg_per_cubic_meter: 0.0,
                id: "void".into(),
                ..steel.clone()
            })
            .is_err()
    );
    assert!(graph.assign_material("a", Some("unobtainium")).is_err());

    graph.assign_material("a", Some("steel")).unwrap();
    assert_eq!(graph.material_of("b").unwrap(), Some(&steel));
    let session = Session::new().unwrap();
    // 10 x 20 x 30 mm is 6e-6 m^3.
    let mass = graph.mass(&session, "b", "body").unwrap();
    assert!((mass - 6e-6 * 7850.0).abs() < 1e-12);
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(graph.mass(&session, "b", "missing").is_err());

    // Detaching keeps the material the clone used to inherit.
    graph.detach("b").unwrap();
    graph.assign_material("a", None).unwrap();
    assert_eq!(graph.material_of("a").unwrap(), None);
    assert_eq!(graph.material_of("b").unwrap(), Some(&steel));
    assert!(
        graph
            .mass(&session, "a", "body")
            .unwrap_err()
            .message
            .contains("no material")
    );

    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
}

#[test]
fn assembly_requirements_verify_mass_datum_clearance_and_relationships() {
    let definition = block();
    let mut graph = stacked(&definition);
    graph
        .add_material(Material {
            id: "steel".into(),
            name: "Structural steel".into(),
            density_kg_per_cubic_meter: 7850.0,
        })
        .unwrap();
    graph.assign_material("a", Some("steel")).unwrap();
    graph
        .add_relationship(relationship(
            "seated",
            RelationKind::Coincident,
            ("a", "top"),
            ("b", "bottom"),
        ))
        .unwrap();

    let requirement = |id: &str, priority, rule| AssemblyRequirement {
        id: id.into(),
        version: 1,
        kind: RequirementKind::Validation,
        priority,
        statement: format!("verify {id}"),
        rule,
        provenance: "test".into(),
    };
    graph
        .add_assembly_requirement(requirement(
            "mass",
            RequirementPriority::Required,
            AssemblyVerificationRule::MassRange {
                instance: "b".into(),
                output: "body".into(),
                minimum_kilograms: 0.047,
                maximum_kilograms: 0.048,
            },
        ))
        .unwrap();
    graph
        .add_assembly_requirement(requirement(
            "clearance",
            RequirementPriority::Preferred,
            AssemblyVerificationRule::DatumClearance {
                first: DatumRef::new("a", "top_center"),
                second: DatumRef::new("b", "top_center"),
                minimum: Quantity::length(29.0, LengthUnit::Millimeter),
                maximum: Some(Quantity::length(30.5, LengthUnit::Millimeter)),
            },
        ))
        .unwrap();
    graph
        .add_assembly_requirement(requirement(
            "relationship",
            RequirementPriority::Advisory,
            AssemblyVerificationRule::RelationshipSatisfied {
                relationship: "seated".into(),
            },
        ))
        .unwrap();

    let session = Session::new().unwrap();
    let partial = graph.regenerate_instances(&session, &["a"]).unwrap();
    assert!(partial.verification().is_empty());
    drop(partial);
    let generation = graph.regenerate_all(&session).unwrap();
    assert_eq!(generation.verification().len(), 3);
    assert!(
        generation
            .verification()
            .iter()
            .all(|result| result.status == VerificationStatus::Passed)
    );
    drop(generation);
    assert_eq!(session.shape_count().unwrap(), 0);

    graph
        .set_placement("b", translated(0.0, 0.0, 31.0))
        .unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    assert_eq!(
        generation.verification()[0].status,
        VerificationStatus::Passed
    );
    assert_eq!(
        generation.verification()[1].status,
        VerificationStatus::Failed
    );
    assert_eq!(
        generation.verification()[2].status,
        VerificationStatus::Failed
    );
    assert!(generation.verification()[1].message.contains("31"));
    drop(generation);

    let advisory = graph.remove_assembly_requirement("relationship").unwrap();
    graph
        .add_assembly_requirement(AssemblyRequirement {
            priority: RequirementPriority::Required,
            ..advisory
        })
        .unwrap();
    let error = graph.regenerate_all(&session).err().unwrap();
    assert!(
        error
            .message
            .contains("required assembly verification failed")
    );
    assert!(error.message.contains("relationship"));
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(graph.remove_relationship("seated").is_err());
    assert!(graph.assign_material("a", None).is_err());

    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
}

#[test]
fn assembly_requirements_reject_invalid_ranges_units_and_references() {
    let definition = block();
    let mut graph = stacked(&definition);
    let base = AssemblyRequirement {
        id: "invalid".into(),
        version: 1,
        kind: RequirementKind::Validation,
        priority: RequirementPriority::Required,
        statement: "invalid test".into(),
        rule: AssemblyVerificationRule::DatumClearance {
            first: DatumRef::new("a", "top_center"),
            second: DatumRef::new("b", "top_center"),
            minimum: Quantity::scalar(1.0),
            maximum: None,
        },
        provenance: "test".into(),
    };
    assert!(graph.add_assembly_requirement(base.clone()).is_err());

    let mut valid = base.clone();
    valid.rule = AssemblyVerificationRule::DatumClearance {
        first: DatumRef::new("a", "top_center"),
        second: DatumRef::new("b", "top_center"),
        minimum: Quantity::length(0.0, LengthUnit::Millimeter),
        maximum: None,
    };
    assert!(
        graph
            .add_assembly_requirements([valid.clone(), valid])
            .is_err()
    );
    assert!(graph.assembly().requirements.is_empty());

    let mut unknown = base.clone();
    unknown.rule = AssemblyVerificationRule::RelationshipSatisfied {
        relationship: "missing".into(),
    };
    assert!(graph.add_assembly_requirement(unknown).is_err());

    let mut mass = base;
    mass.rule = AssemblyVerificationRule::MassRange {
        instance: "a".into(),
        output: "body".into(),
        minimum_kilograms: 2.0,
        maximum_kilograms: 1.0,
    };
    assert!(graph.add_assembly_requirement(mass).is_err());

    let mut no_material = AssemblyRequirement {
        id: "mass".into(),
        version: 1,
        kind: RequirementKind::Validation,
        priority: RequirementPriority::Required,
        statement: "mass test".into(),
        rule: AssemblyVerificationRule::MassRange {
            instance: "a".into(),
            output: "body".into(),
            minimum_kilograms: 0.0,
            maximum_kilograms: 1.0,
        },
        provenance: "test".into(),
    };
    assert!(
        graph
            .add_assembly_requirement(no_material.clone())
            .unwrap_err()
            .message
            .contains("needs a material")
    );
    graph
        .add_material(Material {
            id: "steel".into(),
            name: "Steel".into(),
            density_kg_per_cubic_meter: 7850.0,
        })
        .unwrap();
    graph.assign_material("a", Some("steel")).unwrap();
    no_material.rule = AssemblyVerificationRule::MassRange {
        instance: "a".into(),
        output: "missing".into(),
        minimum_kilograms: 0.0,
        maximum_kilograms: 1.0,
    };
    assert!(
        graph
            .add_assembly_requirement(no_material)
            .unwrap_err()
            .message
            .contains("unknown output")
    );

    let reversed_clearance = AssemblyRequirement {
        id: "clearance".into(),
        version: 1,
        kind: RequirementKind::Assembly,
        priority: RequirementPriority::Required,
        statement: "clearance test".into(),
        rule: AssemblyVerificationRule::DatumClearance {
            first: DatumRef::new("a", "top_center"),
            second: DatumRef::new("b", "top_center"),
            minimum: Quantity::length(2.0, LengthUnit::Millimeter),
            maximum: Some(Quantity::length(1.0, LengthUnit::Millimeter)),
        },
        provenance: "test".into(),
    };
    assert!(
        graph
            .add_assembly_requirement(reversed_clearance)
            .unwrap_err()
            .message
            .contains("exceeds maximum")
    );
    assert!(graph.remove_assembly_requirement("missing").is_err());
}

#[test]
fn referenced_pattern_members_are_not_deleted_and_documents_reject_dangling_references() {
    let definition = block();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("a", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "row",
            "seat",
            "a",
            3,
            VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
            "test",
        )
        .unwrap();
    graph
        .add_relationship(relationship(
            "spacing",
            RelationKind::Distance(Quantity::length(100.0, LengthUnit::Millimeter)),
            ("seat[0]", "top_center"),
            ("seat[2]", "top_center"),
        ))
        .unwrap();
    let error = graph.set_pattern_count("row", 2).unwrap_err();
    assert!(error.message.contains("relationship 'spacing'"), "{error}");
    assert_eq!(graph.patterns()[0].slot_count, 3);

    let document = ModelDocument::from_graph(&graph);
    let mut dangling = document.clone();
    dangling.assembly.relationships[0].second.instance = "gone".into();
    assert!(dangling.to_json_pretty().is_err());
    let mut unknown_material = document;
    unknown_material
        .assembly
        .material_assignments
        .insert("a".into(), "missing".into());
    assert!(unknown_material.to_json_pretty().is_err());
}
