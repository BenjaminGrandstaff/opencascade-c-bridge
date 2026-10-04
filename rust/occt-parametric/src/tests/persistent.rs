//! Persistent references: topology chosen where it is unambiguous and
//! followed through later features by operation history.

use super::*;

fn mm(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn direction(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::scalars(x, y, z))
}

fn facing(x: f64, y: f64, z: f64) -> FaceSelector {
    FaceSelector::NormalAligned {
        direction: direction(x, y, z),
        minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999)),
    }
}

/// The box's front face (normal -y), named where the box is made.
fn front() -> FaceSelector {
    FaceSelector::Persistent {
        feature: "box".into(),
        select: Box::new(facing(0.0, -1.0, 0.0)),
    }
}

/// A width x 20 x 10 box, turned a quarter turn about z so its front face
/// looks along +x at x = 0, then notched through the middle of that face.
/// `slot` sizes the notch; covering the whole face removes it.
fn turned_family(slot: (f64, f64)) -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.parameters = vec![length_parameter("width", 40.0)];
    family.features = vec![
        FeatureDefinition {
            id: "box".into(),
            operation: FeatureOperation::Box {
                origin: mm(0.0, 0.0, 0.0),
                size: VectorExpr::Components {
                    x: ScalarExpr::Parameter("width".into()),
                    y: ScalarExpr::Literal(Quantity::length(20.0, LengthUnit::Millimeter)),
                    z: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
                },
            },
        },
        FeatureDefinition {
            id: "turned".into(),
            operation: FeatureOperation::Rotate {
                input: "box".into(),
                origin: mm(0.0, 0.0, 0.0),
                axis: direction(0.0, 0.0, 1.0),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
            },
        },
        FeatureDefinition {
            id: "notch".into(),
            operation: FeatureOperation::Box {
                origin: mm(-5.0, slot.0, -1.0),
                size: mm(6.0, slot.1 - slot.0, 12.0),
            },
        },
        FeatureDefinition {
            id: "slotted".into(),
            operation: FeatureOperation::Cut {
                object: "turned".into(),
                tool: "notch".into(),
            },
        },
    ];
    family
}

fn resolve(
    session: &Session,
    family: &FamilyDefinition,
    generated: &GeneratedResult<'_>,
    selector: &FaceSelector,
) -> Result<Vec<(Vec3, f64)>, ModelError> {
    let definitions = family
        .features
        .iter()
        .map(|feature| (feature.id.as_str(), feature))
        .collect::<HashMap<_, _>>();
    let parameters = resolve_parameters(family, &HashMap::new())?;
    let faces = resolve_face_selector(
        session,
        generated.shape("slotted").unwrap(),
        selector,
        &parameters,
        &generated.shapes,
        &definitions,
    )?;
    let measured = faces
        .iter()
        .map(|face| {
            (
                session.face_normal(face).unwrap(),
                session.surface_area(face).unwrap(),
            )
        })
        .collect();
    for face in faces {
        session.remove(face).unwrap();
    }
    Ok(measured)
}

#[test]
fn persistent_faces_survive_placement_splits_and_parameter_edits() {
    let session = Session::new().unwrap();
    let family = turned_family((18.0, 22.0));
    let mut instance = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let generated = instance.regenerate(&session).unwrap();
    // The notch split the original front face into two pieces, both now
    // looking along +x, totalling the face less the 4 mm notch.
    let pieces = resolve(&session, &family, &generated, &front()).unwrap();
    assert_eq!(pieces.len(), 2);
    for (normal, _) in &pieces {
        assert!((normal.x - 1.0).abs() < 1e-9, "{normal:?}");
    }
    let area = pieces.iter().map(|(_, area)| area).sum::<f64>();
    assert!((area - (40.0 - 4.0) * 10.0).abs() < 1e-6, "{area}");
    // Asking the final shape for "the -y face" finds other faces: the box's
    // original -x end (20 x 10) and one side wall of the notch (5 x 10).
    let naive = resolve(&session, &family, &generated, &facing(0.0, -1.0, 0.0)).unwrap();
    let mut areas = naive.iter().map(|(_, area)| *area).collect::<Vec<_>>();
    areas.sort_by(f64::total_cmp);
    assert_eq!(areas.len(), 2);
    assert!(
        (areas[0] - 50.0).abs() < 1e-6 && (areas[1] - 200.0).abs() < 1e-6,
        "{areas:?}"
    );

    // A wider box keeps the reference on the same face.
    instance.overrides.insert(
        "width".into(),
        ParameterValue::Scalar(Quantity::length(60.0, LengthUnit::Millimeter)),
    );
    let wider = instance
        .regenerate_incremental(&session, &generated)
        .unwrap();
    let pieces = resolve(&session, &family, &wider, &front()).unwrap();
    let area = pieces.iter().map(|(_, area)| area).sum::<f64>();
    assert_eq!(pieces.len(), 2);
    assert!((area - (60.0 - 4.0) * 10.0).abs() < 1e-6, "{area}");
    drop((generated, wider));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn removed_or_unrelated_persistent_references_fail_clearly() {
    let session = Session::new().unwrap();
    // A notch across the whole face removes it.
    let family = turned_family((-1.0, 41.0));
    let generated = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
    .regenerate(&session)
    .unwrap();
    let error = resolve(&session, &family, &generated, &front()).unwrap_err();
    assert!(
        error.message.contains("removed by feature 'slotted'"),
        "{}",
        error.message
    );

    // The notch box is not upstream of the slotted result's box faces.
    let unrelated = FaceSelector::Persistent {
        feature: "missing".into(),
        select: Box::new(facing(0.0, -1.0, 0.0)),
    };
    assert!(resolve(&session, &family, &generated, &unrelated).is_err());
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn features_consume_persistent_references_and_documents_keep_them() {
    let session = Session::new().unwrap();
    let mut family = turned_family((18.0, 22.0));
    // Shell the slotted block open through both pieces of the front face.
    family.features.push(FeatureDefinition {
        id: "shell".into(),
        operation: FeatureOperation::Hollow {
            input: "slotted".into(),
            faces: vec![front()],
            thickness: ScalarExpr::Literal(Quantity::length(-1.0, LengthUnit::Millimeter)),
            tolerance: ScalarExpr::Literal(Quantity::length(1e-4, LengthUnit::Millimeter)),
        },
    });
    let generated = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
    .regenerate(&session)
    .unwrap();
    let shell = generated.shape("shell").unwrap();
    assert!(session.is_valid(shell).unwrap());
    let solid = session.volume(generated.shape("slotted").unwrap()).unwrap();
    assert!(
        session.volume(shell).unwrap() < solid / 2.0,
        "the shell is hollow"
    );

    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"persistent\""));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}
