use super::*;
#[test]
fn non_clamped_cap_builds_from_solved_endpoints_and_tracks_size_weight_and_height_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/non-clamped-spline-cap.request.json"
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
        (session.volume(first.shape("body").unwrap()).unwrap() - 20. * 10. / 12. * 8.).abs() < 1e-5
    );
    // OCCT AddOptimal retains a 1e-7 numerical margin for spline surfaces.
    let bounds = session.exact_bounds(first.shape("body").unwrap()).unwrap();
    assert!(
        (bounds.min.x - 5.).abs() < 2e-7 && (bounds.max.x - 15.).abs() < 2e-7,
        "{bounds:?}"
    );
    assert!((bounds.min.y - 5.).abs() < 2e-7 && (bounds.max.y - 7.5).abs() < 2e-7);
    part.overrides.insert(
        "span".into(),
        ParameterValue::Scalar(Quantity::length(24., LengthUnit::Millimeter)),
    );
    let wider = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(wider.regeneration.rebuilt, vec!["profile", "body"]);
    assert!(
        (session.volume(wider.shape("body").unwrap()).unwrap() - 24. * 10. / 12. * 8.).abs() < 1e-5
    );
    part.overrides.insert(
        "middle_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(2.)),
    );
    let weighted = part.regenerate_incremental(&session, &wider).unwrap();
    assert!(session.is_valid(weighted.shape("body").unwrap()).unwrap());
    assert_eq!(weighted.regeneration.rebuilt, vec!["profile", "body"]);
    let bounds = session
        .exact_bounds(weighted.shape("body").unwrap())
        .unwrap();
    assert!((bounds.min.x - 8.).abs() < 2e-7 && (bounds.max.x - 16.).abs() < 2e-7);
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &weighted).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    assert_eq!(taller.regeneration.rebuilt, vec!["body"]);
    assert!(
        (session.volume(taller.shape("body").unwrap()).unwrap()
            - 1.5 * session.volume(weighted.shape("body").unwrap()).unwrap())
        .abs()
            < 1e-5
    );
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "middle_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(0.)),
    );
    assert!(part.regenerate_incremental(&session, &taller).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
