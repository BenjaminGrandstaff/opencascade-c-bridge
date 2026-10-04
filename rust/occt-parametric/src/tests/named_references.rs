//! Named references: face and edge selections declared once per family and
//! used by name in any feature.

use super::*;

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

/// The box's front face (normal -y), aligned to within the `alignment`
/// parameter's cosine.
fn front_target() -> FaceSelector {
    FaceSelector::Persistent {
        feature: "box".into(),
        select: Box::new(FaceSelector::NormalAligned {
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, -1.0, 0.0)),
            minimum_dot: ScalarExpr::Parameter("alignment".into()),
        }),
    }
}

fn shell(id: &str, front: FaceSelector, thickness: f64) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::Hollow {
            input: "box".into(),
            faces: vec![front],
            thickness: length(-thickness),
            tolerance: length(1e-4),
        },
    }
}

/// A 40 x 20 x 10 box shelled open through its front face twice, and filleted
/// along its top edges. Features are declared before the box they use.
fn named_family() -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.parameters = vec![ParameterDefinition {
        id: "alignment".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(0.999)),
        minimum: None,
        maximum: None,
    }];
    family.references = vec![
        NamedReference {
            name: "front".into(),
            target: ReferenceTarget::Faces(front_target()),
        },
        NamedReference {
            name: "top edges".into(),
            target: ReferenceTarget::Edges(EdgeSelector::AtExtreme {
                axis: CoordinateAxis::Z,
                extremum: Extremum::Maximum,
                tolerance: length(1e-3),
            }),
        },
    ];
    family.features = vec![
        shell("thin", FaceSelector::Named("front".into()), 1.0),
        shell("thick", FaceSelector::Named("front".into()), 2.0),
        FeatureDefinition {
            id: "rounded".into(),
            operation: FeatureOperation::Fillet {
                input: "box".into(),
                edges: vec![EdgeSelector::Named("top edges".into())],
                radius: length(1.0),
            },
        },
        FeatureDefinition {
            id: "box".into(),
            operation: FeatureOperation::Box {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                size: VectorExpr::Literal(VectorQuantity::lengths(
                    40.0,
                    20.0,
                    10.0,
                    LengthUnit::Millimeter,
                )),
            },
        },
    ];
    family
}

fn part(family: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition: family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

fn regeneration_error(family: &FamilyDefinition) -> String {
    let session = Session::new().unwrap();
    part(family).regenerate(&session).err().unwrap().message
}

#[test]
fn features_share_a_named_reference_and_rebuild_when_it_changes() {
    let session = Session::new().unwrap();
    let family = named_family();
    let mut instance = part(&family);
    let first = instance.regenerate(&session).unwrap();
    for id in ["thin", "thick", "rounded"] {
        assert!(session.is_valid(first.shape(id).unwrap()).unwrap(), "{id}");
    }

    // A named reference resolves exactly like the selector it names.
    let mut inline = family.clone();
    inline.references.clear();
    inline.features = vec![
        shell("thin", front_target(), 1.0),
        inline.features[3].clone(),
    ];
    let expected = part(&inline).regenerate(&session).unwrap();
    let named = session.volume(first.shape("thin").unwrap()).unwrap();
    let direct = session.volume(expected.shape("thin").unwrap()).unwrap();
    assert!((named - direct).abs() < 1e-9 * direct, "{named} {direct}");
    // Open-fronted shells: the thicker one keeps more material.
    let thick = session.volume(first.shape("thick").unwrap()).unwrap();
    assert!(named < thick && thick < 40.0 * 20.0 * 10.0);

    // Editing a parameter of the reference rebuilds exactly its users.
    instance.overrides.insert(
        "alignment".into(),
        ParameterValue::Scalar(Quantity::scalar(0.99)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.rebuilt, vec!["thin", "thick"]);
    let mut reused = second.regeneration.reused.clone();
    reused.sort();
    assert_eq!(reused, vec!["box", "rounded"]);
    let again = session.volume(second.shape("thin").unwrap()).unwrap();
    assert!((again - named).abs() < 1e-9 * named);
    drop((first, second, expected));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn named_references_validate_names_kinds_and_targets() {
    let mut unknown = named_family();
    unknown.features[0] = shell("thin", FaceSelector::Named("back".into()), 1.0);
    let message = regeneration_error(&unknown);
    assert!(
        message.contains("feature 'thin' uses unknown named reference 'back'"),
        "{message}"
    );

    let mut wrong_kind = named_family();
    wrong_kind.features[0] = shell("thin", FaceSelector::Named("top edges".into()), 1.0);
    let message = regeneration_error(&wrong_kind);
    assert!(message.contains("as the wrong kind"), "{message}");

    let mut duplicate = named_family();
    duplicate.references[1].name = "front".into();
    let message = regeneration_error(&duplicate);
    assert!(message.contains("nonempty and unique"), "{message}");

    let mut empty = named_family();
    empty.references[1].name.clear();
    let message = regeneration_error(&empty);
    assert!(message.contains("nonempty and unique"), "{message}");

    let mut nested = named_family();
    nested.references.push(NamedReference {
        name: "alias".into(),
        target: ReferenceTarget::Faces(FaceSelector::Named("front".into())),
    });
    let message = regeneration_error(&nested);
    assert!(
        message.contains("'alias' cannot use other named references"),
        "{message}"
    );

    let mut missing = named_family();
    missing.references[0].target = ReferenceTarget::Faces(FaceSelector::Persistent {
        feature: "nowhere".into(),
        select: Box::new(front_target()),
    });
    let message = regeneration_error(&missing);
    assert!(message.contains("unknown output 'nowhere'"), "{message}");

    // A reference whose origin is downstream of its user is a cycle.
    let mut cycle = named_family();
    cycle.references[0].target = ReferenceTarget::Faces(FaceSelector::Persistent {
        feature: "thick".into(),
        select: Box::new(front_target()),
    });
    let message = regeneration_error(&cycle);
    assert!(message.contains("cycle"), "{message}");
}

#[test]
fn documents_keep_named_references_and_omit_an_empty_list() {
    let family = named_family();
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"references\"") && json.contains("\"named\""));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);

    let mut plain = named_family();
    plain.references.clear();
    plain.features.clear();
    let json = serde_json::to_string(&plain).unwrap();
    assert!(!json.contains("\"references\""));
    let back: FamilyDefinition = serde_json::from_str(&json).unwrap();
    assert!(back.references.is_empty());
}
