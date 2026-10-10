use super::*;

#[test]
fn spline_pair_native_cap_tracks_reference_weight_dimensions_and_height_reuse() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/spline-tangent-pair.request.json"
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
    let check = |built: &GeneratedResult<'_>, peak: f64, span: f64, rise: f64, height: f64| {
        let body = built.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!((session.volume(body).unwrap() - span * rise / 3. * height).abs() < 1e-5);
        let b = session.exact_bounds(body).unwrap();
        assert!(
            (b.min.y - peak).abs() < 2e-7 && (b.max.y - peak - rise / 2.).abs() < 2e-7,
            "{b:?}"
        );
        assert!((b.min.x + span / 2.).abs() < 2e-7 && (b.max.x - span / 2.).abs() < 2e-7);
    };
    let first = part.regenerate(&session).unwrap();
    check(&first, 5., 20., 8., 8.);
    part.overrides.insert(
        "middle_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(2.)),
    );
    let weighted = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(weighted.regeneration.rebuilt, vec!["profile", "body"]);
    check(&weighted, 20. / 3., 20., 8., 8.);
    part.overrides.insert(
        "span".into(),
        ParameterValue::Scalar(Quantity::length(24., LengthUnit::Millimeter)),
    );
    part.overrides.insert(
        "upper_rise".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let wider = part.regenerate_incremental(&session, &weighted).unwrap();
    assert_eq!(wider.regeneration.rebuilt, vec!["profile", "body"]);
    check(&wider, 20. / 3., 24., 12., 8.);
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &wider).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    assert_eq!(taller.regeneration.rebuilt, vec!["body"]);
    check(&taller, 20. / 3., 24., 12., 12.);
    // Thin/wide limits use parameter-dependent seeds but still solve the native contact.
    part.overrides.insert(
        "span".into(),
        ParameterValue::Scalar(Quantity::length(10., LengthUnit::Millimeter)),
    );
    part.overrides.insert(
        "lower_rise".into(),
        ParameterValue::Scalar(Quantity::length(20., LengthUnit::Millimeter)),
    );
    let narrow = part.regenerate_incremental(&session, &taller).unwrap();
    check(&narrow, 40. / 3., 10., 12., 12.);
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "middle_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(0.)),
    );
    assert!(part.regenerate_incremental(&session, &narrow).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
