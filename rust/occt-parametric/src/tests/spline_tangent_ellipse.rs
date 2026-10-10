use super::*;

#[test]
fn native_spline_tangent_ellipse_edits_axes_weight_rotation_and_reuses_profile() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/spline-tangent-ellipse.request.json"
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
    check(&first, 5., 3., 8.);
    let bounds = session.exact_bounds(first.shape("body").unwrap()).unwrap();
    assert!((bounds.min.y - 5.).abs() < 2e-7 && (bounds.max.y - 11.).abs() < 2e-7);
    for (id, value, dimension) in [
        ("middle_weight", 2., Dimension::Scalar),
        ("semi_major", 8., Dimension::Length),
        ("semi_minor", 4., Dimension::Length),
        ("tilt", 0.7, Dimension::Scalar),
    ] {
        let q = if dimension == Dimension::Scalar {
            Quantity::scalar(value)
        } else {
            Quantity::length(value, LengthUnit::Millimeter)
        };
        part.overrides.insert(id.into(), ParameterValue::Scalar(q));
    }
    let edited = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, vec!["profile", "body"]);
    check(&edited, 8., 4., 8.);
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &edited).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    check(&taller, 8., 4., 12.);
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "semi_minor".into(),
        ParameterValue::Scalar(Quantity::length(0., LengthUnit::Millimeter)),
    );
    assert!(part.regenerate_incremental(&session, &taller).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
