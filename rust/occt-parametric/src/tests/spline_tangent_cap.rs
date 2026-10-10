use super::*;

#[test]
fn spline_tangent_cap_native_solid_tracks_weight_size_and_height_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/spline-tangent-cap.request.json"
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
    assert!(session.is_valid(first.shape("body").unwrap()).unwrap());
    assert!(
        (session.volume(first.shape("body").unwrap()).unwrap() - 20. * 10. / 3. * 8.).abs() < 1e-5
    );
    let bounds = session.exact_bounds(first.shape("body").unwrap()).unwrap();
    assert!((bounds.max.y - 5.).abs() < 2e-7 && (bounds.min.x + 10.).abs() < 2e-7);
    part.overrides.insert(
        "middle_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(2.)),
    );
    let weighted = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(weighted.regeneration.rebuilt, vec!["profile", "body"]);
    let bounds = session
        .exact_bounds(weighted.shape("body").unwrap())
        .unwrap();
    assert!((bounds.max.y - 20. / 3.).abs() < 2e-7);
    assert!(session.is_valid(weighted.shape("body").unwrap()).unwrap());
    part.overrides.insert(
        "span".into(),
        ParameterValue::Scalar(Quantity::length(24., LengthUnit::Millimeter)),
    );
    let wider = part.regenerate_incremental(&session, &weighted).unwrap();
    assert_eq!(wider.regeneration.rebuilt, vec!["profile", "body"]);
    let bounds = session.exact_bounds(wider.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x + 12.).abs() < 2e-7 && (bounds.max.x - 12.).abs() < 2e-7);
    assert!(
        (session.volume(wider.shape("body").unwrap()).unwrap()
            / session.volume(weighted.shape("body").unwrap()).unwrap()
            - 1.2)
            .abs()
            < 1e-7
    );
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &wider).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    assert_eq!(taller.regeneration.rebuilt, vec!["body"]);
    assert!(
        (session.volume(taller.shape("body").unwrap()).unwrap()
            / session.volume(wider.shape("body").unwrap()).unwrap()
            - 1.5)
            .abs()
            < 1e-7
    );
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "middle_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(0.)),
    );
    assert!(part.regenerate_incremental(&session, &taller).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
