use super::*;
#[test]
fn native_open_corner_pipe_tracks_path_sizes_reuses_section_and_preserves_failures() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/open-corner-pipe.request.json"
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
    let check = |built: &GeneratedResult<'_>, fillet: f64, radius: f64| {
        let wire = built.shape("path").unwrap();
        assert!(!session.wire_is_closed(wire).unwrap());
        let length = session
            .subshapes(wire, ShapeType::Edge)
            .unwrap()
            .iter()
            .map(|e| session.edge_length(e).unwrap())
            .sum::<f64>();
        let expected = 95. - 2. * fillet + std::f64::consts::FRAC_PI_2 * fillet - 8. + 34f64.sqrt();
        assert!((length - expected).abs() < 1e-6);
        let body = built.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!(
            (session.volume(body).unwrap() - std::f64::consts::PI * radius * radius * expected)
                .abs()
                < 1e-4
        );
    };
    let first = part.regenerate(&session).unwrap();
    check(&first, 4., 1.);
    part.overrides.insert(
        "fillet_radius".into(),
        ParameterValue::Scalar(Quantity::length(6., LengthUnit::Millimeter)),
    );
    let edited = part.regenerate_incremental(&session, &first).unwrap();
    assert!(edited.regeneration.reused.contains(&"profile".into()));
    assert!(
        edited.regeneration.rebuilt.contains(&"path".into())
            && edited.regeneration.rebuilt.contains(&"body".into())
    );
    check(&edited, 6., 1.);
    part.overrides.insert(
        "radius".into(),
        ParameterValue::Scalar(Quantity::length(1.2, LengthUnit::Millimeter)),
    );
    let wider = part.regenerate_incremental(&session, &edited).unwrap();
    assert!(wider.regeneration.reused.contains(&"path".into()));
    check(&wider, 6., 1.2);
    let count = session.shape_count().unwrap();
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(15., LengthUnit::Millimeter)),
    );
    part.overrides.insert(
        "fillet_radius".into(),
        ParameterValue::Scalar(Quantity::length(8., LengthUnit::Millimeter)),
    );
    part.overrides.insert(
        "chamfer_incoming".into(),
        ParameterValue::Scalar(Quantity::length(8., LengthUnit::Millimeter)),
    );
    assert!(part.regenerate_incremental(&session, &wider).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    check(&wider, 6., 1.2);
}
