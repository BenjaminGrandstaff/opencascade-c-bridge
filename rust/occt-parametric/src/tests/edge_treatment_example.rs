use super::*;
#[test]
fn ai_edge_treatments_match_corner_volumes_and_reuse_the_other_variant_after_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/edge-treatments.request.json"
    ))
    .unwrap();
    let d = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let mut p = PartInstance {
        id: "part".into(),
        definition: &d.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let first = p.regenerate(&session).unwrap();
    assert!(
        (session.volume(first.shape("rounded").unwrap()).unwrap()
            - (6000.0 - (4.0 - std::f64::consts::PI) * 4.0 * 10.0))
            .abs()
            < 1e-5
    );
    assert!((session.volume(first.shape("beveled").unwrap()).unwrap() - 5955.0).abs() < 1e-5);
    p.overrides.insert(
        "fillet_radius".into(),
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
    );
    let edited = p.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, vec!["rounded"]);
    assert_eq!(edited.regeneration.reused, vec!["block", "beveled"]);
    assert!(session.is_valid(edited.shape("rounded").unwrap()).unwrap());
    assert!(
        (session.volume(edited.shape("rounded").unwrap()).unwrap()
            - (6000.0 - (4.0 - std::f64::consts::PI) * 9.0 * 10.0))
            .abs()
            < 1e-5
    );
    let count = session.shape_count().unwrap();
    p.overrides.insert(
        "fillet_radius".into(),
        ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
    );
    assert!(p.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn ai_variable_fillet_station_edits_rebuild_only_the_blend_and_release_handles() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/variable-fillet.request.json"
    ))
    .unwrap();
    let d = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let mut p = PartInstance {
        id: "part".into(),
        definition: &d.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let first = p.regenerate(&session).unwrap();
    let original_volume = session.volume(first.shape("blend").unwrap()).unwrap();
    assert!(original_volume > 5900.0 && original_volume < 6000.0);
    assert!(session.is_valid(first.shape("blend").unwrap()).unwrap());
    for (name, value) in [
        (
            "middle_radius",
            Quantity::length(2.0, LengthUnit::Millimeter),
        ),
        ("station_position", Quantity::scalar(0.5)),
    ] {
        p.overrides.clear();
        p.overrides
            .insert(name.into(), ParameterValue::Scalar(value));
        let edited = p.regenerate_incremental(&session, &first).unwrap();
        assert_eq!(edited.regeneration.rebuilt, vec!["blend"]);
        assert_eq!(edited.regeneration.reused, vec!["block"]);
        assert!(session.is_valid(edited.shape("blend").unwrap()).unwrap());
        assert!(
            (session.volume(edited.shape("blend").unwrap()).unwrap() - original_volume).abs() > 0.1
        );
    }
    let count = session.shape_count().unwrap();
    p.overrides.insert(
        "station_position".into(),
        ParameterValue::Scalar(Quantity::scalar(0.0)),
    );
    assert!(p.regenerate_incremental(&session, &first).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
}
