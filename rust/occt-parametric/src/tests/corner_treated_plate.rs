use super::*;
#[test]
fn native_corner_plate_regenerates_sizes_reuses_profile_and_preserves_failed_edits() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/corner-treated-plate.request.json"
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
    let check =
        |built: &GeneratedResult<'_>, radius: f64, incoming: f64, outgoing: f64, depth: f64| {
            let shape = built.shape("body").unwrap();
            assert!(session.is_valid(shape).unwrap());
            let area = 1000.
                - radius * radius * (1. - std::f64::consts::FRAC_PI_4)
                - incoming * outgoing / 2.;
            assert!((session.volume(shape).unwrap() - area * depth).abs() < 1e-5);
        };
    let first = part.regenerate(&session).unwrap();
    check(&first, 4., 3., 5., 10.);
    for (id, value) in [
        ("fillet_radius", 6.),
        ("chamfer_incoming", 4.),
        ("chamfer_outgoing", 7.),
    ] {
        part.overrides.insert(
            id.into(),
            ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
        );
    }
    let edited = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, vec!["profile", "body"]);
    check(&edited, 6., 4., 7., 10.);
    part.overrides.insert(
        "depth".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let deeper = part.regenerate_incremental(&session, &edited).unwrap();
    assert_eq!(deeper.regeneration.reused, vec!["profile"]);
    check(&deeper, 6., 4., 7., 12.);
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(3., LengthUnit::Millimeter)),
    );
    assert!(part.regenerate_incremental(&session, &deeper).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    check(&deeper, 6., 4., 7., 12.);
}
