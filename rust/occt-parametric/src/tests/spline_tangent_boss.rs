use super::*;

#[test]
fn spline_tangent_boss_native_circle_tracks_contact_weights_radius_and_height() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/spline-tangent-boss.request.json"
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
    let check = |built: &GeneratedResult<'_>, contact: f64, radius: f64, height: f64| {
        let body = built.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!(
            (session.volume(body).unwrap() - std::f64::consts::PI * radius * radius * height).abs()
                < 1e-5
        );
        let b = session.exact_bounds(body).unwrap();
        assert!(
            (b.min.y - contact).abs() < 2e-7 && (b.max.y - contact - 2. * radius).abs() < 2e-7,
            "{b:?}"
        );
        assert!((b.min.x + radius).abs() < 2e-7 && (b.max.x - radius).abs() < 2e-7);
    };
    let first = part.regenerate(&session).unwrap();
    check(&first, 5., 3., 8.);
    part.overrides.insert(
        "middle_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(2.)),
    );
    let weighted = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(weighted.regeneration.rebuilt, vec!["profile", "body"]);
    check(&weighted, 20. / 3., 3., 8.);
    part.overrides.insert(
        "boss_radius".into(),
        ParameterValue::Scalar(Quantity::length(4., LengthUnit::Millimeter)),
    );
    let wider = part.regenerate_incremental(&session, &weighted).unwrap();
    assert_eq!(wider.regeneration.rebuilt, vec!["profile", "body"]);
    check(&wider, 20. / 3., 4., 8.);
    part.overrides.insert(
        "rise".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let moved = part.regenerate_incremental(&session, &wider).unwrap();
    assert_eq!(moved.regeneration.rebuilt, vec!["profile", "body"]);
    check(&moved, 8., 4., 8.);
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &moved).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    assert_eq!(taller.regeneration.rebuilt, vec!["body"]);
    check(&taller, 8., 4., 12.);
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "boss_radius".into(),
        ParameterValue::Scalar(Quantity::length(0., LengthUnit::Millimeter)),
    );
    assert!(part.regenerate_incremental(&session, &taller).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
