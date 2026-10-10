use super::*;

#[test]
fn equal_radius_sketches_drive_both_holes_and_incrementally_rebuild_the_plate() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/equal-radius-plate.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    assert_eq!(document.schema_version, 92);
    let session = Session::new().unwrap();
    let mut part = PartInstance {
        id: "part".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let original = part.regenerate(&session).unwrap();
    let expected = |radius: f64| (60.0 * 30.0 - 2.0 * std::f64::consts::PI * radius.powi(2)) * 8.0;
    assert!(
        (session.volume(original.shape("body").unwrap()).unwrap() - expected(3.0)).abs() < 1e-5
    );
    part.overrides.insert(
        "hole_radius".into(),
        ParameterValue::Scalar(Quantity::length(4.0, LengthUnit::Millimeter)),
    );
    let edited = part.regenerate_incremental(&session, &original).unwrap();
    assert_eq!(edited.regeneration.reused, vec!["outer"]);
    assert_eq!(
        edited.regeneration.rebuilt,
        vec!["hole-left", "hole-right", "region", "body"]
    );
    assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    assert!((session.volume(edited.shape("body").unwrap()).unwrap() - expected(4.0)).abs() < 1e-5);
    for output in ["hole-left", "hole-right"] {
        let edge = session
            .subshapes(edited.shape(output).unwrap(), occt_bridge::ShapeType::Edge)
            .unwrap()
            .pop()
            .unwrap();
        assert!((session.edge_circle_radius(&edge).unwrap().unwrap() - 4.0).abs() < 1e-7);
    }
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "hole_radius".into(),
        ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
    );
    assert!(part.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    drop((original, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
    // Older definitions without the new constraint migrate without changing their entities.
    let mut old: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/sketch-block.request.json"
    ))
    .unwrap();
    old["model"]["schema_version"] = serde_json::json!(91);
    let migrated = ModelDocument::from_json(&old["model"].to_string()).unwrap();
    assert_eq!(migrated.schema_version, 92);
    let original: Vec<FeatureDefinition> =
        serde_json::from_value(old["model"]["family"]["features"].clone()).unwrap();
    assert_eq!(migrated.family.features, original);
}
