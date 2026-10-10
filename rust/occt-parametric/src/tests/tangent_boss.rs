use super::*;

#[test]
fn independent_tangencies_drive_native_boss_placement_radius_and_incremental_height() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/tangent-boss.request.json"
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
    let bounds = session.exact_bounds(first.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x - (60_f64.sqrt() - 3.)).abs() < 1e-7);
    assert!((bounds.min.y + 5.).abs() < 1e-7);
    assert!(
        (session.volume(first.shape("body").unwrap()).unwrap() - std::f64::consts::PI * 9. * 8.)
            .abs()
            < 1e-5
    );
    part.overrides.insert(
        "boss_radius".into(),
        ParameterValue::Scalar(Quantity::length(4., LengthUnit::Millimeter)),
    );
    let edited = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, vec!["profile", "body"]);
    let bounds = session.exact_bounds(edited.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x - (80_f64.sqrt() - 4.)).abs() < 1e-7);
    assert!((bounds.min.y + 5.).abs() < 1e-7);
    assert!((bounds.max.y - 3.).abs() < 1e-7);
    assert!(
        (session.volume(edited.shape("body").unwrap()).unwrap() - std::f64::consts::PI * 16. * 8.)
            .abs()
            < 1e-5
    );
    part.overrides.insert(
        "reference_radius".into(),
        ParameterValue::Scalar(Quantity::length(7., LengthUnit::Millimeter)),
    );
    let moved = part.regenerate_incremental(&session, &edited).unwrap();
    assert_eq!(moved.regeneration.rebuilt, vec!["profile", "body"]);
    let bounds = session.exact_bounds(moved.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x - (112_f64.sqrt() - 4.)).abs() < 1e-7);
    assert!((bounds.min.y + 7.).abs() < 1e-7);
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &moved).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    assert_eq!(taller.regeneration.rebuilt, vec!["body"]);
    assert!(session.is_valid(taller.shape("body").unwrap()).unwrap());
    assert!(
        (session.volume(taller.shape("body").unwrap()).unwrap() - std::f64::consts::PI * 16. * 12.)
            .abs()
            < 1e-5
    );
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "boss_radius".into(),
        ParameterValue::Scalar(Quantity::length(0., LengthUnit::Millimeter)),
    );
    assert!(part.regenerate_incremental(&session, &taller).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
