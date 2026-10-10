use super::*;

#[test]
fn arc_tangent_boss_builds_exact_segment_and_tracks_radius_and_reference_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/arc-tangent-boss.request.json"
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
    let mut generated = part.regenerate(&session).unwrap();
    for (radius, reference) in [(3., 5.), (4., 5.), (4., 7.)] {
        part.overrides.insert(
            "boss_radius".into(),
            ParameterValue::Scalar(Quantity::length(radius, LengthUnit::Millimeter)),
        );
        part.overrides.insert(
            "reference_radius".into(),
            ParameterValue::Scalar(Quantity::length(reference, LengthUnit::Millimeter)),
        );
        generated = part.regenerate_incremental(&session, &generated).unwrap();
        assert!(session.is_valid(generated.shape("body").unwrap()).unwrap());
        let expected = (3. * std::f64::consts::PI / 4. + 0.5) * radius * radius * 8.;
        assert!(
            (session.volume(generated.shape("body").unwrap()).unwrap() - expected).abs() < 1e-5
        );
        let bounds = session
            .exact_bounds(generated.shape("body").unwrap())
            .unwrap();
        let center_x = (4. * radius * reference).sqrt();
        assert!((bounds.min.x - (center_x - radius)).abs() < 1e-7);
        assert!((bounds.min.y + reference).abs() < 1e-7);
        let edges = session
            .subshapes(
                generated.shape("profile").unwrap(),
                occt_bridge::ShapeType::Edge,
            )
            .unwrap();
        assert_eq!(edges.len(), 2);
        let radii: Vec<_> = edges
            .iter()
            .filter_map(|edge| session.edge_circle_radius(edge).unwrap())
            .collect();
        assert_eq!(radii.len(), 1);
        assert!((radii[0] - radius).abs() < 1e-7);
    }
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &generated).unwrap();
    assert_eq!(taller.regeneration.reused, vec!["profile"]);
    assert_eq!(taller.regeneration.rebuilt, vec!["body"]);
    // Keep the same supporting-circle tangency, but exclude its contact from the reference arc.
    let mut bad = document.clone();
    let FeatureOperation::SketchFace { sketch } = &mut bad.family.features[0].operation else {
        panic!()
    };
    sketch.arcs[0].clockwise = false;
    let bad_part = PartInstance {
        id: "part".into(),
        definition: &bad.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let count = session.shape_count().unwrap();
    assert!(bad_part.regenerate(&session).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
}
