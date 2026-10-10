use super::*;
#[test]
fn rational_spline_cap_native_volume_and_weight_edits_track_exact_basis_dependencies() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/rational-spline-cap.request.json"
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
    let first = part.regenerate(&session).unwrap();
    let expected = (std::f64::consts::PI / 4. - 0.5) * 100. * 8.;
    assert!(session.is_valid(first.shape("body").unwrap()).unwrap());
    let volume = session.volume(first.shape("body").unwrap()).unwrap();
    assert!((volume - expected).abs() < 1e-5, "volume {volume}");
    part.overrides.insert(
        "middle_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(1.)),
    );
    let edited = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, vec!["profile", "body"]);
    assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    // Unit weights produce a polynomial quadratic with cap area R²/3.
    assert!((session.volume(edited.shape("body").unwrap()).unwrap() - 100. / 3. * 8.).abs() < 1e-5);
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &edited).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    assert_eq!(taller.regeneration.rebuilt, vec!["body"]);
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "middle_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(0.)),
    );
    assert!(part.regenerate_incremental(&session, &taller).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
