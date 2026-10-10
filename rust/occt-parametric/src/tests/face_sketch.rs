use super::*;
fn document() -> ModelDocument {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/face-pocket.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&r["model"].to_string()).unwrap()
}
fn part(f: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition: f,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
fn sketch(f: &mut FamilyDefinition) -> &mut SketchDefinition {
    let FeatureOperation::SketchFace { sketch } = &mut f
        .features
        .iter_mut()
        .find(|x| x.id == "profile")
        .unwrap()
        .operation
    else {
        panic!()
    };
    sketch
}
fn mm(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn vec(x: f64, y: f64, z: f64, dim: Dimension) -> VectorExpr {
    VectorExpr::Literal(if dim == Dimension::Length {
        VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter)
    } else {
        VectorQuantity::scalars(x, y, z)
    })
}
fn top() -> FaceSelector {
    FaceSelector::NormalAligned {
        direction: vec(0., 0., 1., Dimension::Scalar),
        minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999999)),
    }
}

#[test]
fn face_attached_pocket_follows_height_and_retains_accepted_geometry_after_rejected_edits() {
    let d = document();
    let session = Session::new().unwrap();
    let mut p = part(&d.family);
    let first = p.regenerate(&session).unwrap();
    let b = session
        .exact_bounds(first.shape("profile").unwrap())
        .unwrap();
    assert!(
        (b.min.x - 20.).abs() < 1e-7
            && (b.max.x - 40.).abs() < 1e-7
            && (b.min.z - 20.).abs() < 1e-7
    );
    assert!((session.volume(first.shape("body").unwrap()).unwrap() - 46800.).abs() < 1e-5);
    p.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(30., LengthUnit::Millimeter)),
    );
    let edited = p.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(
        edited.regeneration.rebuilt,
        vec!["block", "profile", "tool", "body"]
    );
    assert!(
        (session
            .exact_bounds(edited.shape("profile").unwrap())
            .unwrap()
            .min
            .z
            - 30.)
            .abs()
            < 1e-7
    );
    assert!((session.volume(edited.shape("body").unwrap()).unwrap() - 70800.).abs() < 1e-5);
    assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    let count = session.shape_count().unwrap();
    let planes = p.sketch_support_planes(&session, &edited).unwrap();
    let ResolvedDatum::Plane { origin, normal } = planes["profile"].as_ref().unwrap() else {
        panic!()
    };
    assert!((origin.z - 30.).abs() < 1e-7 && (normal.z - 1.).abs() < 1e-7);
    assert_eq!(session.shape_count().unwrap(), count);
    p.overrides.insert(
        "cut_depth".into(),
        ParameterValue::Scalar(Quantity::length(100., LengthUnit::Millimeter)),
    );
    assert!(p.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    let mut bad = d.clone();
    sketch(&mut bad.family).face_support.as_mut().unwrap().face = FaceSelector::Union(vec![
        top(),
        FaceSelector::NormalAligned {
            direction: vec(0., 0., -1., Dimension::Scalar),
            minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999999)),
        },
    ]);
    p.overrides.remove("cut_depth");
    p.definition = &bad.family;
    assert!(p.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    assert!((session.volume(edited.shape("body").unwrap()).unwrap() - 70800.).abs() < 1e-5);
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn support_offset_rebuilds_only_the_attached_branch_and_checks_units() {
    let mut d = document();
    let session = Session::new().unwrap();
    let mut p = part(&d.family);
    let first = p.regenerate(&session).unwrap();
    p.overrides.insert(
        "support_offset".into(),
        ParameterValue::Scalar(Quantity::length(1., LengthUnit::Millimeter)),
    );
    let edited = p.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.reused, vec!["block"]);
    assert_eq!(edited.regeneration.rebuilt, vec!["profile", "tool", "body"]);
    assert!(
        (session
            .exact_bounds(edited.shape("profile").unwrap())
            .unwrap()
            .min
            .z
            - 21.)
            .abs()
            < 1e-7
    );
    assert!((session.volume(edited.shape("body").unwrap()).unwrap() - 47040.).abs() < 1e-5);
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
    sketch(&mut d.family).face_support.as_mut().unwrap().offset =
        ScalarExpr::Literal(Quantity::scalar(1.));
    assert!(part(&d.family).regenerate(&session).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn invalid_ambiguous_and_nonplanar_supports_fail_without_leaking_shapes() {
    let session = Session::new().unwrap();
    for case in 0..6 {
        let mut d = document();
        match case {
            0 => sketch(&mut d.family).face_support.as_mut().unwrap().input = "missing".into(),
            1 => sketch(&mut d.family).face_support.as_mut().unwrap().input = "profile".into(),
            2 => sketch(&mut d.family).datum_plane = Some("plane".into()),
            3 => {
                let bottom = FaceSelector::NormalAligned {
                    direction: vec(0., 0., -1., Dimension::Scalar),
                    minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999999)),
                };
                sketch(&mut d.family).face_support.as_mut().unwrap().face =
                    FaceSelector::Union(vec![top(), bottom]);
            }
            4 => {
                d.family.features[0].operation = FeatureOperation::Cylinder {
                    origin: vec(0., 0., 0., Dimension::Length),
                    axis: vec(0., 0., 1., Dimension::Scalar),
                    radius: mm(8.),
                    height: mm(20.),
                };
                sketch(&mut d.family).face_support.as_mut().unwrap().face =
                    FaceSelector::LargestArea {
                        planar_only: false,
                        allow_ties: false,
                        relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-9)),
                    };
            }
            _ => {
                sketch(&mut d.family).face_support.as_mut().unwrap().face =
                    FaceSelector::NormalAligned {
                        direction: vec(1., 1., 1., Dimension::Scalar),
                        minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999999)),
                    }
            }
        }
        let error = part(&d.family).regenerate(&session).err().unwrap();
        if case == 3 {
            assert!(error.message.contains("exactly one face"));
        }
        if case == 4 {
            assert!(error.message.contains("must be planar"));
        }
        assert_eq!(
            session.shape_count().unwrap(),
            0,
            "failed support case {case}"
        );
    }
}

#[test]
fn named_persistent_support_follows_placed_faces_and_rejects_an_invalid_x_axis() {
    let mut d = document();
    d.family.requirements.clear();
    d.family
        .features
        .retain(|f| f.id == "block" || f.id == "profile");
    d.family.features.insert(
        1,
        FeatureDefinition {
            id: "placed".into(),
            operation: FeatureOperation::Translate {
                input: "block".into(),
                offset: vec(100., 0., 5., Dimension::Length),
            },
        },
    );
    d.family.references.push(NamedReference {
        name: "top".into(),
        target: ReferenceTarget::Faces(FaceSelector::Persistent {
            feature: "block".into(),
            select: Box::new(top()),
        }),
    });
    let support = sketch(&mut d.family).face_support.as_mut().unwrap();
    support.input = "placed".into();
    support.face = FaceSelector::Named("top".into());
    let session = Session::new().unwrap();
    let first = part(&d.family).regenerate(&session).unwrap();
    let bounds = session
        .exact_bounds(first.shape("profile").unwrap())
        .unwrap();
    assert!((bounds.min.x - 120.).abs() < 1e-7 && (bounds.min.z - 25.).abs() < 1e-7);
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
    d.family.features[1].operation = FeatureOperation::Rotate {
        input: "block".into(),
        origin: vec(0., 0., 0., Dimension::Length),
        axis: vec(0., 1., 0., Dimension::Scalar),
        angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
    };
    assert!(part(&d.family).regenerate(&session).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
    sketch(&mut d.family).x_axis = vec(0., 1., 0., Dimension::Scalar);
    let rotated = part(&d.family).regenerate(&session).unwrap();
    let bounds = session
        .exact_bounds(rotated.shape("profile").unwrap())
        .unwrap();
    assert!((bounds.min.x - 20.).abs() < 1e-7 && (bounds.max.x - 20.).abs() < 1e-7);
    drop(rotated);
    assert_eq!(session.shape_count().unwrap(), 0);
}
