use super::*;

#[test]
fn ellipse_tangent_pad_native_placement_tracks_axes_rotation_and_height_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/ellipse-tangent-pad.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
    let session = Session::new().unwrap();
    let mut part = PartInstance {
        id: "part".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let check = |built: &GeneratedResult<'_>, a: f64, b: f64, theta: f64, height: f64| {
        let body = built.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!(
            (session.volume(body).unwrap() - std::f64::consts::PI * a * b * height).abs() < 1e-5
        );
        let bounds = session.exact_bounds(body).unwrap();
        let sx = (a * theta.cos()).hypot(b * theta.sin());
        let sy = (a * theta.sin()).hypot(b * theta.cos());
        assert!(
            (bounds.min.x + sx).abs() < 2e-7 && (bounds.max.x - sx).abs() < 2e-7,
            "{bounds:?}"
        );
        assert!(
            bounds.min.y.abs() < 2e-7 && (bounds.max.y - 2. * sy).abs() < 2e-7,
            "{bounds:?}"
        );
        assert!((bounds.max.z - height).abs() < 2e-7);
    };
    let first = part.regenerate(&session).unwrap();
    check(&first, 5., 3., std::f64::consts::PI / 6., 8.);
    part.overrides.insert(
        "semi_minor".into(),
        ParameterValue::Scalar(Quantity::length(4., LengthUnit::Millimeter)),
    );
    let edited = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, vec!["profile", "body"]);
    check(&edited, 5., 4., std::f64::consts::PI / 6., 8.);
    part.overrides.insert(
        "tilt".into(),
        ParameterValue::Scalar(Quantity::scalar(std::f64::consts::PI / 3.)),
    );
    let rotated = part.regenerate_incremental(&session, &edited).unwrap();
    assert_eq!(rotated.regeneration.rebuilt, vec!["profile", "body"]);
    check(&rotated, 5., 4., std::f64::consts::PI / 3., 8.);
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &rotated).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    assert_eq!(taller.regeneration.rebuilt, vec!["body"]);
    check(&taller, 5., 4., std::f64::consts::PI / 3., 12.);
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "semi_minor".into(),
        ParameterValue::Scalar(Quantity::length(0., LengthUnit::Millimeter)),
    );
    assert!(part.regenerate_incremental(&session, &taller).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
