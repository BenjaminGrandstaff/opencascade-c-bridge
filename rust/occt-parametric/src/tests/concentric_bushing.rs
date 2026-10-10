use super::*;

#[test]
fn concentric_bushing_drives_native_bore_and_rebuilds_after_center_radius_and_height_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/concentric-bushing.request.json"
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
    let original = part.regenerate(&session).unwrap();
    let expected = |r: f64, h: f64| std::f64::consts::PI * (144. - r * r) * h;
    assert!(
        (session.volume(original.shape("body").unwrap()).unwrap() - expected(5., 8.)).abs() < 1e-5
    );
    for (name, value) in [("center_x", 20.), ("center_y", -10.), ("inner_radius", 6.)] {
        part.overrides.insert(
            name.into(),
            ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
        );
    }
    let moved = part.regenerate_incremental(&session, &original).unwrap();
    assert_eq!(
        moved.regeneration.rebuilt,
        vec!["outer", "inner", "region", "body"]
    );
    assert!(session.is_valid(moved.shape("body").unwrap()).unwrap());
    assert!(
        (session.volume(moved.shape("body").unwrap()).unwrap() - expected(6., 8.)).abs() < 1e-5
    );
    let edge = session
        .subshapes(moved.shape("inner").unwrap(), occt_bridge::ShapeType::Edge)
        .unwrap()
        .pop()
        .unwrap();
    assert!((session.edge_circle_radius(&edge).unwrap().unwrap() - 6.).abs() < 1e-7);
    let bounds = session.exact_bounds(moved.shape("inner").unwrap()).unwrap();
    assert!((bounds.min.x - 14.).abs() < 1e-6);
    assert!((bounds.max.y + 4.).abs() < 1e-6);
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &moved).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["outer", "inner", "region"]);
    assert_eq!(taller.regeneration.rebuilt, vec!["body"]);
    assert!(
        (session.volume(taller.shape("body").unwrap()).unwrap() - expected(6., 12.)).abs() < 1e-5
    );
}
