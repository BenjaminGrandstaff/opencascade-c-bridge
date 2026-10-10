use super::*;

#[test]
fn native_ellipse_pair_solid_tracks_rotations_axes_and_height_reuse() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/ellipse-tangent-pair.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&r["model"].to_string()).unwrap();
    assert_eq!(doc.schema_version, CURRENT_SCHEMA_VERSION);
    let session = Session::new().unwrap();
    let mut part = PartInstance {
        id: "part".into(),
        definition: &doc.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let check = |built: &GeneratedResult<'_>, a: f64, b: f64, h: f64| {
        let body = built.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!((session.volume(body).unwrap() - std::f64::consts::PI * a * b * h).abs() < 1e-5);
    };
    let first = part.regenerate(&session).unwrap();
    check(&first, 4., 2., 8.);
    let bounds = session.exact_bounds(first.shape("body").unwrap()).unwrap();
    assert!((bounds.min.y - 3.).abs() < 2e-7 && (bounds.max.y - 7.).abs() < 2e-7);
    for (id, value) in [
        ("semi_major", 8.),
        ("semi_minor", 4.),
        ("upper_major", 5.),
        ("upper_minor", 2.),
    ] {
        part.overrides.insert(
            id.into(),
            ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
        );
    }
    for (id, value) in [("tilt", 0.7), ("upper_tilt", 0.3)] {
        part.overrides
            .insert(id.into(), ParameterValue::Scalar(Quantity::scalar(value)));
    }
    let edited = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, vec!["profile", "body"]);
    check(&edited, 5., 2., 8.);
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &edited).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    check(&taller, 5., 2., 12.);
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "upper_minor".into(),
        ParameterValue::Scalar(Quantity::length(0., LengthUnit::Millimeter)),
    );
    assert!(part.regenerate_incremental(&session, &taller).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
