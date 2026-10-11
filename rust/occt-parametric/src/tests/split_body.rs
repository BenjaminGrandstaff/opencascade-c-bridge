use super::*;
use serde_json::{Value, json};

fn example() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tools/model/split-housing.request.json"
    ))
    .unwrap()
}

fn family(request: &Value) -> FamilyDefinition {
    ModelDocument::from_json(&request["model"].to_string())
        .unwrap()
        .family
}

fn part(definition: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "housing".into(),
        definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

fn length(value: f64) -> ParameterValue {
    ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter))
}

fn direction(x: f64, y: f64, z: f64) -> Value {
    json!({"literal": {"x": {"value": x, "dimension": "scalar", "unit": null},
        "y": {"value": y, "dimension": "scalar", "unit": null},
        "z": {"value": z, "dimension": "scalar", "unit": null}}})
}

#[test]
fn cap_and_base_conserve_volume_and_follow_split_height_edits() {
    let session = Session::new().unwrap();
    let definition = family(&example());
    let mut housing = part(&definition);
    let first = housing.regenerate(&session).unwrap();
    let mut previous = None;
    for height in [15.0, 21.0, 6.5] {
        housing
            .overrides
            .insert("split_height".into(), length(height));
        let built = housing
            .regenerate_incremental(&session, previous.as_ref().unwrap_or(&first))
            .unwrap();
        if height != 15.0 {
            assert!(built.regeneration.reused.contains(&"housing".into()));
            assert!(built.regeneration.rebuilt.contains(&"cap".into()));
        }
        let (whole, cap, base) = (
            built.shape("housing").unwrap(),
            built.shape("cap").unwrap(),
            built.shape("base").unwrap(),
        );
        let (v, c, b) = (
            session.volume(whole).unwrap(),
            session.volume(cap).unwrap(),
            session.volume(base).unwrap(),
        );
        assert!((c + b - v).abs() < 1e-7 * v, "{height}: {c} + {b} vs {v}");
        for piece in [cap, base] {
            assert!(session.is_valid(piece).unwrap());
            assert_eq!(session.subshape_count(piece, ShapeType::Solid).unwrap(), 1);
        }
        let (cb, bb) = (
            session.exact_bounds(cap).unwrap(),
            session.exact_bounds(base).unwrap(),
        );
        assert!((cb.min.z - height).abs() < 1e-6 && (cb.max.z - 30.).abs() < 1e-6);
        assert!((bb.max.z - height).abs() < 1e-6 && bb.min.z.abs() < 1e-6);
        previous = Some(built);
    }
}

#[test]
fn both_sides_tilted_planes_and_u_shapes_give_every_piece() {
    let session = Session::new().unwrap();
    // Keep both sides of a tilted plane through the housing centre.
    let mut request = example();
    let cap = &mut request["model"]["family"]["features"][4]["operation"]["split"];
    cap["keep"] = json!("both");
    cap["normal"] = direction(0., 1., 1.);
    cap["origin"] = json!({"literal": {"x": {"value": 30, "dimension": "length", "unit": "millimeter"},
        "y": {"value": 20, "dimension": "length", "unit": "millimeter"},
        "z": {"value": 15, "dimension": "length", "unit": "millimeter"}}});
    let definition = family(&request);
    let built = part(&definition).regenerate(&session).unwrap();
    let both = built.shape("cap").unwrap();
    assert_eq!(session.subshape_count(both, ShapeType::Solid).unwrap(), 2);
    let v = session.volume(built.shape("housing").unwrap()).unwrap();
    assert!((session.volume(both).unwrap() - v).abs() < 1e-7 * v);
    // A U shape split above its floor keeps two separate prongs.
    let mut u = example();
    let features = u["model"]["family"]["features"].as_array_mut().unwrap();
    features.insert(
        4,
        json!({"id": "notch", "operation": {"box": {
            "origin": {"literal": {"x": {"value": 20, "dimension": "length", "unit": "millimeter"},
                "y": {"value": -1, "dimension": "length", "unit": "millimeter"},
                "z": {"value": 10, "dimension": "length", "unit": "millimeter"}}},
            "size": {"literal": {"x": {"value": 20, "dimension": "length", "unit": "millimeter"},
                "y": {"value": 42, "dimension": "length", "unit": "millimeter"},
                "z": {"value": 21, "dimension": "length", "unit": "millimeter"}}}}}}),
    );
    features.insert(
        5,
        json!({"id": "u", "operation": {"cut": {"object": "housing", "tool": "notch"}}}),
    );
    features[6]["operation"]["split"]["input"] = json!("u");
    features[7]["operation"]["split"]["input"] = json!("u");
    let definition = family(&u);
    let mut prongs = part(&definition);
    prongs.overrides.insert("split_height".into(), length(20.));
    let built = prongs.regenerate(&session).unwrap();
    let cap = built.shape("cap").unwrap();
    assert_eq!(session.subshape_count(cap, ShapeType::Solid).unwrap(), 2);
    assert!(session.is_valid(cap).unwrap());
}

#[test]
fn planes_that_miss_and_zero_normals_fail_and_keep_accepted_pieces() {
    let session = Session::new().unwrap();
    let definition = family(&example());
    let first = part(&definition).regenerate(&session).unwrap();
    let count = session.shape_count().unwrap();
    // Planes at or beyond the block's faces leave one side empty.
    for height in [30.0, 45.0, 0.0, -5.0] {
        let mut request = example();
        request["model"]["family"]["features"][4]["operation"]["split"]["origin"] = json!(
            {"literal": {"x": {"value": 0, "dimension": "length", "unit": "millimeter"},
                "y": {"value": 0, "dimension": "length", "unit": "millimeter"},
                "z": {"value": height, "dimension": "length", "unit": "millimeter"}}});
        let missed = family(&request);
        let Err(error) = part(&missed).regenerate_incremental(&session, &first) else {
            panic!("split at {height} should fail");
        };
        assert!(
            error.message.contains("must cross the input"),
            "{height}: {}",
            error.message
        );
        assert_eq!(session.shape_count().unwrap(), count);
    }
    assert!(session.is_valid(first.shape("cap").unwrap()).unwrap());
    // Tilted planes: one clear of the block, one clipping only its top edge
    // at y = 40, z = 30 (the plane y + z = 69 leaves a sliver there).
    let tilted = |y: f64, z: f64| {
        let mut request = example();
        let split = &mut request["model"]["family"]["features"][4]["operation"]["split"];
        split["normal"] = direction(0., 1., 1.);
        split["origin"] = json!({"literal": {"x": {"value": 30, "dimension": "length", "unit": "millimeter"},
            "y": {"value": y, "dimension": "length", "unit": "millimeter"},
            "z": {"value": z, "dimension": "length", "unit": "millimeter"}}});
        family(&request)
    };
    let clear = tilted(40., 31.);
    assert!(part(&clear).regenerate(&session).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    let clip = tilted(40., 29.);
    let built = part(&clip).regenerate(&session).unwrap();
    let sliver = session.volume(built.shape("cap").unwrap()).unwrap();
    // A 1 x 1 mm right-triangle prism along the 60 mm edge.
    assert!((sliver - 30.).abs() < 1e-6, "{sliver}");
    drop(built);
    let mut flat = example();
    flat["model"]["family"]["features"][4]["operation"]["split"]["normal"] = direction(0., 0., 0.);
    let definition = family(&flat);
    let Err(error) = part(&definition).regenerate(&session) else {
        panic!("zero normal should fail");
    };
    assert!(
        error.message.contains("normal must be nonzero"),
        "{}",
        error.message
    );
}
