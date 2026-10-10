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
