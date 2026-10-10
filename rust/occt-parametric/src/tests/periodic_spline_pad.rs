use super::*;
#[test]
fn periodic_spline_pad_native_volume_tracks_size_weight_and_height_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/periodic-spline-pad.request.json"
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
        (session.volume(first.shape("body").unwrap()).unwrap() - 122. / 45. * 10. * 8. * 8.).abs()
            < 1e-5
    );
    let bounds = session.exact_bounds(first.shape("body").unwrap()).unwrap();
    assert!((bounds.max.x - 110. / 12.).abs() < 1e-7);
    assert!((bounds.max.y - 88. / 12.).abs() < 1e-7);
    let edges = session
        .subshapes(
            first.shape("profile").unwrap(),
            occt_bridge::ShapeType::Edge,
        )
        .unwrap();
    assert_eq!(edges.len(), 1);
    part.overrides.insert(
        "half_width".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let wider = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(wider.regeneration.rebuilt, vec!["profile", "body"]);
    assert!(
        (session.volume(wider.shape("body").unwrap()).unwrap() - 122. / 45. * 12. * 8. * 8.).abs()
            < 1e-5
    );
    part.overrides.insert(
        "corner_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(2.)),
    );
    let weighted = part.regenerate_incremental(&session, &wider).unwrap();
    assert_eq!(weighted.regeneration.rebuilt, vec!["profile", "body"]);
    assert!(session.is_valid(weighted.shape("body").unwrap()).unwrap());
    assert!(
        (session.volume(weighted.shape("body").unwrap()).unwrap()
            - session.volume(wider.shape("body").unwrap()).unwrap())
        .abs()
            > 1.
    );
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
        "corner_weight".into(),
        ParameterValue::Scalar(Quantity::scalar(0.)),
    );
    assert!(part.regenerate_incremental(&session, &taller).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    // Schema-99 explicit bases retain clamped behavior when periodic is absent.
    let mut old: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/rational-spline-cap.request.json"
    ))
    .unwrap();
    old["model"]["schema_version"] = serde_json::json!(99);
    let migrated = ModelDocument::from_json(&old["model"].to_string()).unwrap();
    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    let FeatureOperation::SketchFace { sketch } = &migrated.family.features[0].operation else {
        panic!()
    };
    assert!(!sketch.splines[0].basis.as_ref().unwrap().periodic);
}
