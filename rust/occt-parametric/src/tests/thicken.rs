use super::*;
use serde_json::{Value, json};
use std::f64::consts::PI;

fn example() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tools/model/thickened-strap.request.json"
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
        id: "strap".into(),
        definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

fn length(value: f64) -> ParameterValue {
    ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter))
}

/// The strap request with the thicken feature's `side` (and optionally a
/// literal thickness) replaced.
fn strap_with(side: &str, thickness: Option<f64>) -> FamilyDefinition {
    let mut request = example();
    let shell = &mut request["model"]["family"]["features"][3]["operation"]["thicken"];
    shell["side"] = json!(side);
    if let Some(t) = thickness {
        shell["thickness"] =
            json!({"literal": {"value": t, "dimension": "length", "unit": "millimeter"}});
    }
    family(&request)
}

/// ∫₀ᵗ √(r² − z²) dz.
fn quarter_strip(r: f64, t: f64) -> f64 {
    t / 2.0 * (r * r - t * t).sqrt() + r * r / 2.0 * (t / r).asin()
}

/// Exact volume of the strap wall between offset levels `a <= 0 <= b` of
/// the sheet (normal side positive): feet from x = ±(radius + 14) and a half
/// ring, with mitred corners where the extended foot planes meet the
/// extended cylinders. 20 mm wide, before the bolt holes.
fn strap_volume(radius: f64, a: f64, b: f64) -> f64 {
    let (reach, inner, outer) = (radius + 14.0, radius + a, radius + b);
    let half = reach * (b - a) - quarter_strip(inner, -a)
        + PI / 4.0 * (outer * outer - inner * inner)
        - quarter_strip(outer, b);
    2.0 * 20.0 * half
}

#[test]
fn strap_thickens_outward_and_follows_thickness_and_radius_edits() {
    let session = Session::new().unwrap();
    let definition = family(&example());
    let mut strap = part(&definition);
    let first = strap.regenerate(&session).unwrap();
    let mut previous: Option<GeneratedResult<'_>> = None;
    for (radius, t) in [(12.0, 2.0), (12.0, 3.5), (20.0, 3.5), (20.0, 0.8)] {
        strap.overrides.insert("pipe_radius".into(), length(radius));
        strap.overrides.insert("thickness".into(), length(t));
        let built = strap
            .regenerate_incremental(&session, previous.as_ref().unwrap_or(&first))
            .unwrap();
        let shell = built.shape("shell").unwrap();
        assert!(session.is_valid(shell).unwrap());
        assert_eq!(session.subshape_count(shell, ShapeType::Solid).unwrap(), 1);
        let bounds = session.exact_bounds(shell).unwrap();
        // The normal side grows outward: the bore stays at the pipe radius.
        assert!((bounds.max.z - (radius + t)).abs() < 1e-6, "{bounds:?}");
        assert!(bounds.min.z.abs() < 1e-6);
        assert!((bounds.max.x - (radius + 14.0)).abs() < 1e-6);
        let volume = session.volume(shell).unwrap();
        let expected = strap_volume(radius, 0.0, t);
        assert!(
            (volume - expected).abs() < 1e-9 * volume,
            "{radius} {t}: {volume} vs {expected}"
        );
        // Bolt holes cut through both feet of the thickened strap.
        let holes = session.volume(built.shape("strap").unwrap()).unwrap();
        let bore = PI * (5.5f64 / 2.0).powi(2) * t * 2.0;
        assert!(
            (volume - holes - bore).abs() < 1e-6 * volume,
            "{volume} {holes} {bore}"
        );
        previous = Some(built);
    }
}

#[test]
fn thickness_only_edits_reuse_the_swept_sheet() {
    let session = Session::new().unwrap();
    let definition = family(&example());
    let mut strap = part(&definition);
    let first = strap.regenerate(&session).unwrap();
    strap.overrides.insert("thickness".into(), length(3.0));
    let built = strap.regenerate_incremental(&session, &first).unwrap();
    assert!(built.regeneration.reused.contains(&"sheet".into()));
    assert!(built.regeneration.rebuilt.contains(&"shell".into()));
    let bounds = session.exact_bounds(built.shape("shell").unwrap()).unwrap();
    assert!((bounds.max.z - 15.0).abs() < 1e-6);
}

#[test]
fn reverse_and_symmetric_sides_place_the_wall() {
    let session = Session::new().unwrap();
    for (side, a, b) in [("reverse", -2.0, 0.0), ("symmetric", -1.0, 1.0)] {
        let definition = strap_with(side, None);
        let built = part(&definition).regenerate(&session).unwrap();
        let shell = built.shape("shell").unwrap();
        assert!(session.is_valid(shell).unwrap(), "{side}");
        let bounds = session.exact_bounds(shell).unwrap();
        assert!(
            (bounds.max.z - (12.0 + b)).abs() < 1e-6,
            "{side}: {bounds:?}"
        );
        assert!((bounds.min.z - a).abs() < 1e-6, "{side}: {bounds:?}");
        let volume = session.volume(shell).unwrap();
        let expected = strap_volume(12.0, a, b);
        assert!(
            (volume - expected).abs() < 1e-9 * volume,
            "{side}: {volume} vs {expected}"
        );
    }
}

#[test]
fn planar_sketch_faces_thicken_exactly() {
    let session = Session::new().unwrap();
    // Replace the swept sheet with a 30 x 20 sketch face.
    let mut request = example();
    let features = request["model"]["family"]["features"]
        .as_array_mut()
        .unwrap();
    let mut plate = features[0].clone();
    let sketch = &mut plate["operation"]["sketch_open_wire"]["sketch"];
    sketch["y_axis"] = json!({"literal": {"x": {"value": 0, "dimension": "scalar", "unit": null},
        "y": {"value": 1, "dimension": "scalar", "unit": null},
        "z": {"value": 0, "dimension": "scalar", "unit": null}}});
    let mm = |v: f64| json!({"literal": {"value": v, "dimension": "length", "unit": "millimeter"}});
    sketch["points"] = json!([
        {"id": "p0", "x": mm(0.), "y": mm(0.), "fixed": true},
        {"id": "p1", "x": mm(30.), "y": mm(0.), "fixed": true},
        {"id": "p2", "x": mm(30.), "y": mm(20.), "fixed": true},
        {"id": "p3", "x": mm(0.), "y": mm(20.), "fixed": true}]);
    sketch["lines"] = json!([{"id": "l0", "start": "p0", "end": "p1"},
        {"id": "l1", "start": "p1", "end": "p2"}, {"id": "l2", "start": "p2", "end": "p3"},
        {"id": "l3", "start": "p3", "end": "p0"}]);
    sketch["arcs"] = json!([]);
    sketch["profile"] = json!(["l0", "l1", "l2", "l3"]);
    plate["operation"] = json!({"sketch_face": {"sketch": sketch.clone()}});
    features[2] = json!({"id": "sheet", "operation": plate["operation"].clone()});
    features.truncate(4);
    request["model"]["family"]["requirements"] = json!([]);
    request["outputs"] = json!([{"instance": "strap", "output": "shell"}]);
    let definition = family(&request);
    let built = part(&definition).regenerate(&session).unwrap();
    let shell = built.shape("shell").unwrap();
    assert!((session.volume(shell).unwrap() - 1200.0).abs() < 1e-9);
    let bounds = session.exact_bounds(shell).unwrap();
    assert!(bounds.min.z.abs() < 1e-9 && (bounds.max.z - 2.0).abs() < 1e-9);
    assert_eq!(session.subshape_count(shell, ShapeType::Face).unwrap(), 6);
}

#[test]
fn solid_inputs_and_collapsing_walls_fail_and_keep_accepted_geometry() {
    let session = Session::new().unwrap();
    let definition = family(&example());
    let first = part(&definition).regenerate(&session).unwrap();
    let count = session.shape_count().unwrap();
    // Wires and solids are not sheets.
    let mut request = example();
    request["model"]["family"]["features"][3]["operation"]["thicken"]["input"] = json!("path");
    let wire = family(&request);
    let Err(error) = part(&wire).regenerate_incremental(&session, &first) else {
        panic!("a wire input must fail");
    };
    assert!(
        error.message.contains("must be a sheet"),
        "{}",
        error.message
    );
    let mut request = example();
    let features = request["model"]["family"]["features"]
        .as_array_mut()
        .unwrap();
    features.push(
        json!({"id": "again", "operation": {"thicken": {"input": "strap",
        "thickness": {"parameter": "thickness"}}}}),
    );
    let again = family(&request);
    let Err(error) = part(&again).regenerate_incremental(&session, &first) else {
        panic!("a solid input must fail");
    };
    assert!(
        error.message.contains("must be a sheet"),
        "{}",
        error.message
    );
    assert_eq!(session.shape_count().unwrap(), count);
    // A reversed wall thicker than the 12 mm loop radius collapses it.
    let collapse = strap_with("reverse", Some(13.0));
    let Err(error) = part(&collapse).regenerate_incremental(&session, &first) else {
        panic!("a collapsing wall must fail");
    };
    assert!(error.message.contains("thicken"), "{}", error.message);
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(first.shape("strap").unwrap()).unwrap());
    for t in [0.0, -1.0] {
        let flat = strap_with("normal", Some(t));
        let Err(error) = part(&flat).regenerate_incremental(&session, &first) else {
            panic!("thickness {t} must fail");
        };
        assert!(error.message.contains("positive"), "{}", error.message);
    }
    assert_eq!(session.shape_count().unwrap(), count);
}
