use super::*;

#[test]
fn ellipse_tangent_boss_regenerates_native_placement_radius_and_height() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/ellipse-tangent-boss.request.json"
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
    let check = |built: &GeneratedResult<'_>, r: f64, h: f64| {
        let body = built.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!((session.volume(body).unwrap() - std::f64::consts::PI * r * r * h).abs() < 1e-5);
    };
    let first = part.regenerate(&session).unwrap();
    check(&first, 2., 8.);
    let bounds = session.exact_bounds(first.shape("body").unwrap()).unwrap();
    assert!((bounds.min.y - 3.).abs() < 2e-7 && (bounds.max.y - 7.).abs() < 2e-7);
    for (id, value) in [("semi_major", 8.), ("semi_minor", 4.), ("boss_radius", 3.)] {
        part.overrides.insert(
            id.into(),
            ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
        );
    }
    part.overrides
        .insert("tilt".into(), ParameterValue::Scalar(Quantity::scalar(0.7)));
    let edited = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, vec!["profile", "body"]);
    check(&edited, 3., 8.);
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &edited).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    check(&taller, 3., 12.);
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "boss_radius".into(),
        ParameterValue::Scalar(Quantity::length(0., LengthUnit::Millimeter)),
    );
    assert!(part.regenerate_incremental(&session, &taller).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
