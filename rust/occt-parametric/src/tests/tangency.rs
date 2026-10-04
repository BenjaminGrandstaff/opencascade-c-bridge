//! Face tangency measured where booleans record no continuity.

use super::*;

fn mm(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

/// The block's part of the top: the larger face looking up (the bottom,
/// equally large, looks down; the 20 x 5 sides are smaller).
fn block_top() -> FaceSelector {
    FaceSelector::Intersection(vec![
        FaceSelector::NormalAligned {
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999)),
        },
        FaceSelector::LargestArea {
            planar_only: true,
            allow_ties: true,
            relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
        },
    ])
}

/// A radius-5 cylinder fused flush with a 20 x 10 x 5 block: a stadium
/// whose top is split into coplanar faces (the block's face notched around
/// the cylinder's disc, and that disc in halves) along edges the fuse records
/// no continuity for. The tray feature would shell it open through the
/// block's top and the faces tangent to it, measured to within the
/// `tolerance` parameter or recorded only; OCCT's offset cannot shell this
/// stadium without a `Unify` feature, so tests select on it directly.
fn stadium_family(measured: bool) -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.parameters = vec![ParameterDefinition {
        id: "tolerance".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(1e-3)),
        minimum: None,
        maximum: None,
    }];
    let tangent = FaceSelector::TangentTo {
        faces: Box::new(block_top()),
        minimum_count: 1,
        angular_tolerance: measured.then(|| ScalarExpr::Parameter("tolerance".into())),
    };
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
            id: "tray".into(),
            operation: FeatureOperation::Hollow {
                input: "stadium".into(),
                faces: vec![FaceSelector::Union(vec![block_top(), tangent])],
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

/// Number and total area of the faces `selector` picks on the stadium.
fn select(
    session: &Session,
    family: &FamilyDefinition,
    generated: &GeneratedResult<'_>,
    overrides: &HashMap<String, ParameterValue>,
    selector: &FaceSelector,
) -> Result<(usize, f64), ModelError> {
    let parameters = resolve_parameters(family, overrides)?;
    let faces = resolve_face_selector(
        session,
        generated.shape("stadium").unwrap(),
        selector,
        &parameters,
        &generated.shapes,
        &Features::new(family),
    )?;
    let area = faces
        .iter()
        .map(|face| session.surface_area(face).unwrap())
        .sum();
    let count = faces.len();
    cleanup_shapes(session, faces);
    Ok((count, area))
}

fn tangent(family: &FamilyDefinition) -> FaceSelector {
    let FeatureOperation::Hollow { faces, .. } = &family.features[3].operation else {
        unreachable!()
    };
    let FaceSelector::Union(parts) = &faces[0] else {
        unreachable!()
    };
    parts[1].clone()
}

#[test]
fn measured_tangency_finds_junctions_that_booleans_leave_unrecorded() {
    let session = Session::new().unwrap();
    let mut family = stadium_family(true);
    // OCCT's offset cannot shell this un-unified stadium; select directly.
    family.features.truncate(3);
    let generated = part(&family).regenerate(&session).unwrap();
    let none = HashMap::new();

    // Recorded continuity alone finds nothing tangent to the block's top.
    let error = select(
        &session,
        &family,
        &generated,
        &none,
        &tangent(&stadium_family(false)),
    )
    .unwrap_err();
    assert!(
        error
            .message
            .contains("face tangency selector found no matches"),
        "{}",
        error.message
    );

    // Measured, it finds the coplanar half-disc the fuse split off inside
    // the block's footprint, so the pair covers the 20 x 10 rectangle.
    let top = FaceSelector::Union(vec![block_top(), tangent(&stadium_family(true))]);
    let (count, area) = select(&session, &family, &generated, &none, &top).unwrap();
    let expected = 200.0;
    assert_eq!(count, 2);
    assert!((area - expected).abs() < 1e-9 * expected, "{area}");

    for tolerance in [0.0, 2.0] {
        let overrides = HashMap::from([(
            "tolerance".to_string(),
            ParameterValue::Scalar(Quantity::scalar(tolerance)),
        )]);
        let error = select(&session, &family, &generated, &overrides, &top).unwrap_err();
        assert!(error.message.contains("(0, pi/2)"), "{}", error.message);
    }
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn the_tolerance_parameter_is_part_of_the_consuming_feature_signature() {
    let family = stadium_family(true);
    let datums = HashMap::new();
    let references = reference_map(&family);
    let signature = |tolerance: f64| {
        let overrides = HashMap::from([(
            "tolerance".to_string(),
            ParameterValue::Scalar(Quantity::scalar(tolerance)),
        )]);
        let parameters = resolve_parameters(&family, &overrides).unwrap();
        family
            .features
            .iter()
            .map(|feature| feature_signature(&datums, feature, &references, &parameters).unwrap())
            .collect::<Vec<_>>()
    };
    let (before, after) = (signature(1e-3), signature(1e-2));
    let changed = family
        .features
        .iter()
        .zip(before.iter().zip(&after))
        .filter(|(_, (before, after))| before != after)
        .map(|(feature, _)| feature.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(changed, vec!["tray"]);
}

#[test]
fn documents_omit_an_absent_tolerance() {
    let recorded = serde_json::to_string(&stadium_family(false)).unwrap();
    assert!(!recorded.contains("angular_tolerance"));
    let measured = stadium_family(true);
    let json = serde_json::to_string(&measured).unwrap();
    assert!(json.contains("angular_tolerance"));
    let back: FamilyDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(back, measured);
}
