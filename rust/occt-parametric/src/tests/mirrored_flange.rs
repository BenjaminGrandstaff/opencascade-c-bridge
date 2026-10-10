use super::*;
use std::f64::consts::PI;

fn length(value: f64) -> ParameterValue {
    ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter))
}

#[test]
fn mirrored_flange_area_follows_mirror_and_bolt_pattern_edits() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/mirrored-flange.request.json"
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
    // Rectangle plus half disc (the mirrored outline), minus the bore and six
    // bolt holes; the body is that face extruded by `depth`.
    let check = |built: &GeneratedResult<'_>, p: [f64; 6]| {
        let [width, height, bore, _circle, bolt, depth] = p;
        let face = built.shape("profile").unwrap();
        assert_eq!(session.subshape_count(face, ShapeType::Wire).unwrap(), 8);
        let area = width * height + PI * width * width / 8. - PI * (bore * bore + 6. * bolt * bolt);
        assert!((session.surface_area(face).unwrap() - area).abs() < 1e-6 * area);
        let body = built.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!((session.volume(body).unwrap() - area * depth).abs() < 1e-6 * area * depth);
        let bounds = session.exact_bounds(body).unwrap();
        assert!(
            (bounds.min.x + width / 2.).abs() < 1e-7 && (bounds.max.x - width / 2.).abs() < 1e-7
        );
        assert!((bounds.max.y - (height + width / 2.)).abs() < 1e-7);
    };
    let first = part.regenerate(&session).unwrap();
    check(&first, [60., 30., 8., 18., 3., 8.]);
    part.overrides.insert("width".into(), length(80.));
    let wider = part.regenerate_incremental(&session, &first).unwrap();
    assert!(wider.regeneration.rebuilt.contains(&"profile".into()));
    check(&wider, [80., 30., 8., 18., 3., 8.]);
    part.overrides.insert("bolt_circle".into(), length(15.));
    part.overrides.insert("bolt_radius".into(), length(4.));
    part.overrides.insert("bore_radius".into(), length(10.));
    let crowded = part.regenerate_incremental(&session, &wider).unwrap();
    check(&crowded, [80., 30., 10., 15., 4., 8.]);
    // Smallest outline with the largest bolt circle still clears the edge.
    for (name, value) in [("width", 50.), ("height", 24.), ("bolt_circle", 19.)] {
        part.overrides.insert(name.into(), length(value));
    }
    let tight = part.regenerate_incremental(&session, &crowded).unwrap();
    check(&tight, [50., 24., 10., 19., 4., 8.]);
    // Bolt holes beyond the outline fail natively without leaking shapes.
    let mut outside = r.clone();
    let constraints = &mut outside["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]
        ["constraints"];
    for c in constraints.as_array_mut().unwrap() {
        if c["distance"]["second"] == "H" {
            c["distance"]["value"] = serde_json::json!({"literal":{"value":40,"dimension":"length","unit":"millimeter"}});
        }
    }
    let outside = ModelDocument::from_json(&outside["model"].to_string()).unwrap();
    let broken = PartInstance {
        id: "part".into(),
        definition: &outside.family,
        overrides: part.overrides.clone(),
        provenance: "test".into(),
    };
    let count = session.shape_count().unwrap();
    let Err(error) = broken.regenerate_incremental(&session, &tight) else {
        panic!("bolt holes outside the outline must fail");
    };
    assert!(error.message.contains("holes"), "{}", error.message);
    assert_eq!(session.shape_count().unwrap(), count);
    check(&tight, [50., 24., 10., 19., 4., 8.]);
}
