//! Merging the faces booleans leave split, so fused shapes can be shelled.

use super::*;

fn mm(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

/// The block's top face, chosen on the bare block and followed forward.
fn block_top() -> FaceSelector {
    FaceSelector::Persistent {
        feature: "block".into(),
        select: Box::new(FaceSelector::NormalAligned {
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999)),
        }),
    }
}

/// A radius-5 cylinder fused flush with a 20 x 10 x 5 block (a stadium
/// whose top the fuse splits into three faces), unified, then shelled open
/// through the top. `unified` false shells the fused stadium directly.
fn tray_family(unified: bool) -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.parameters = vec![ParameterDefinition {
        id: "angle".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(1e-9)),
        minimum: None,
        maximum: None,
    }];
    family.features = vec![
        FeatureDefinition {
            id: "round".into(),
            operation: FeatureOperation::Cylinder {
                origin: mm(0.0, 0.0, 0.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: length(5.0),
                height: length(5.0),
            },
        },
        FeatureDefinition {
            id: "block".into(),
            operation: FeatureOperation::Box {
                origin: mm(0.0, -5.0, 0.0),
                size: mm(20.0, 10.0, 5.0),
            },
        },
        FeatureDefinition {
            id: "stadium".into(),
            operation: FeatureOperation::Fuse {
                left: "round".into(),
                right: "block".into(),
            },
        },
        FeatureDefinition {
            id: "merged".into(),
            operation: FeatureOperation::Unify {
                input: "stadium".into(),
                linear_tolerance: length(1e-7),
                angular_tolerance: ScalarExpr::Parameter("angle".into()),
            },
        },
        FeatureDefinition {
            id: "tray".into(),
            operation: FeatureOperation::Hollow {
                input: if unified { "merged" } else { "stadium" }.into(),
                faces: vec![block_top()],
                thickness: length(-1.0),
                tolerance: length(1e-4),
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

#[test]
fn unified_fused_shapes_shell_through_a_persistent_face() {
    let session = Session::new().unwrap();
    // The split top cannot be shelled; the reference finds only the notched
    // block piece of it.
    assert!(part(&tray_family(false)).regenerate(&session).is_err());

    let family = tray_family(true);
    let mut instance = part(&family);
    let first = instance.regenerate(&session).unwrap();
    let stadium = first.shape("stadium").unwrap();
    let merged = first.shape("merged").unwrap();
    assert_eq!(
        session.subshape_count(stadium, ShapeType::Face).unwrap(),
        10
    );
    assert_eq!(session.subshape_count(merged, ShapeType::Face).unwrap(), 6);
    let solid = session.volume(stadium).unwrap();
    assert!((session.volume(merged).unwrap() - solid).abs() < 1e-9 * solid);
    // The block's top, followed through the split and the merge, opens the
    // whole top: the stadium less a 1 mm-inset stadium 4 mm deep.
    let tray = first.shape("tray").unwrap();
    assert!(session.is_valid(tray).unwrap());
    let pi = std::f64::consts::PI;
    let expected = 5.0 * (200.0 + pi * 12.5) - 4.0 * (152.0 + pi * 8.0);
    let volume = session.volume(tray).unwrap();
    assert!((volume - expected).abs() < 1e-6 * expected, "{volume}");

    // The tolerance is the unify feature's own parameter.
    instance.overrides.insert(
        "angle".into(),
        ParameterValue::Scalar(Quantity::scalar(1e-6)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.rebuilt, vec!["merged", "tray"]);
    let again = session.volume(second.shape("tray").unwrap()).unwrap();
    assert!((again - volume).abs() < 1e-9 * volume);

    instance.overrides.insert(
        "angle".into(),
        ParameterValue::Scalar(Quantity::scalar(2.0)),
    );
    let error = instance
        .regenerate_incremental(&session, &second)
        .err()
        .unwrap();
    assert!(
        error.message.contains("feature 'merged'"),
        "{}",
        error.message
    );
    drop((first, second));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn unify_features_persist() {
    let family = tray_family(true);
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"unify\""));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
}
