use super::*;
#[test]
fn native_spline_corner_pad_edits_rebuild_profile_and_preserve_failures() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/spline-corner-pad.request.json"
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
    // Returns the profile face area after checking the edited boundary: one
    // fillet arc of `fillet`, the chamfered side cut to height - 4 mm, and a
    // prism of the face extruded 6 mm.
    let check = |built: &GeneratedResult<'_>, fillet: f64, height: f64, incoming: f64| {
        let face = built.shape("profile").unwrap();
        let edges = session.subshapes(face, ShapeType::Edge).unwrap();
        assert_eq!(edges.len(), 6);
        let radii = edges
            .iter()
            .filter_map(|e| session.edge_circle_radius(e).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(radii.len(), 1);
        assert!((radii[0] - fillet).abs() < 1e-9);
        let lengths = edges
            .iter()
            .map(|e| session.edge_length(e).unwrap())
            .collect::<Vec<_>>();
        assert!(lengths.iter().any(|l| (l - (height - 4.)).abs() < 1e-9));
        // The chamfer chord spans `incoming` of spline and 4 mm of side across
        // an obtuse corner: longer than either leg, shorter than their sum.
        assert!(
            edges.iter().zip(&lengths).any(|(e, l)| {
                session.edge_circle_radius(e).unwrap().is_none()
                    && *l > incoming.max(4.) + 1e-6
                    && *l < incoming + 4.
                    && (l - (height - 4.)).abs() > 1e-6
            }),
            "{lengths:?}"
        );
        let area = session.surface_area(face).unwrap();
        let body = built.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!((session.volume(body).unwrap() - 6. * area).abs() < 1e-6);
        area
    };
    let first = part.regenerate(&session).unwrap();
    let area = check(&first, 3., 20., 3.);
    // Rectangle plus a crown under 8 mm tall, minus the two corner edits.
    assert!(area > 800. && area < 800. + 40. * 8.);
    part.overrides.insert(
        "fillet_radius".into(),
        ParameterValue::Scalar(Quantity::length(5., LengthUnit::Millimeter)),
    );
    let rounder = part.regenerate_incremental(&session, &first).unwrap();
    assert!(
        rounder.regeneration.rebuilt.contains(&"profile".into())
            && rounder.regeneration.rebuilt.contains(&"body".into())
    );
    assert!(check(&rounder, 5., 20., 3.) < area);
    part.overrides.insert(
        "crown".into(),
        ParameterValue::Scalar(Quantity::length(14., LengthUnit::Millimeter)),
    );
    part.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(30., LengthUnit::Millimeter)),
    );
    let taller = part.regenerate_incremental(&session, &rounder).unwrap();
    assert!(check(&taller, 5., 30., 3.) > area + 200.);
    // Parameter extremes still fit: the flattest, narrowest crown with the
    // largest corner edits.
    for (name, value) in [
        ("crown", 2.),
        ("width", 24.),
        ("fillet_radius", 6.),
        ("chamfer_incoming", 6.),
    ] {
        part.overrides.insert(
            name.into(),
            ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
        );
    }
    let flat = part.regenerate_incremental(&session, &taller).unwrap();
    check(&flat, 6., 30., 6.);
    // A fillet that cannot fit the spline fails natively without leaking
    // shapes, and the accepted result stays usable.
    let mut oversized = r.clone();
    oversized["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]["profile_operations"]
        [0]["fillet"]["radius"] =
        serde_json::json!({"literal":{"value":100,"dimension":"length","unit":"millimeter"}});
    let oversized = ModelDocument::from_json(&oversized["model"].to_string()).unwrap();
    let broken = PartInstance {
        id: "part".into(),
        definition: &oversized.family,
        overrides: part.overrides.clone(),
        provenance: "test".into(),
    };
    let count = session.shape_count().unwrap();
    assert!(broken.regenerate_incremental(&session, &flat).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    check(&flat, 6., 30., 6.);
}
