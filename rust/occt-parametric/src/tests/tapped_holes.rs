use super::*;
use serde_json::{Value, json};
use std::f64::consts::PI;

fn example() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tools/model/tapped-block.request.json"
    ))
    .unwrap()
}

/// The example with only one hole's thread modeled, for tests about that
/// hole: each threaded turn costs tens of milliseconds of booleans.
fn only_threaded(feature: usize) -> Value {
    let mut request = example();
    request["model"]["family"]["features"][3 - feature]["operation"]["hole"]["thread"]["modeled"] =
        Value::Null;
    request
}

fn family(request: &Value) -> FamilyDefinition {
    ModelDocument::from_json(&request["model"].to_string())
        .unwrap()
        .family
}

fn mm(value: f64) -> Value {
    json!({"literal": {"value": value, "dimension": "length", "unit": "millimeter"}})
}

fn part(definition: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "block".into(),
        definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

/// Removed volume of one turn of the internal groove, by Pappus over the
/// in-material part of the basic profile: width 3P/4 at the basic minor
/// radius narrowing linearly to P/8 at the major radius, clipped at the bore.
fn groove_turn_volume(bore_radius: f64, major_radius: f64, pitch: f64) -> f64 {
    let depth = 5. / 8. * 3f64.sqrt() / 2. * pitch;
    let minor = major_radius - depth;
    let width = |r: f64| 0.75 * pitch - (r - minor) / depth * (0.75 - 0.125) * pitch;
    // Simpson's rule is exact for the quadratic integrand r·w(r).
    let (a, b) = (bore_radius.max(minor), major_radius);
    let f = |r: f64| r * width(r);
    let integral = (b - a) / 6. * (f(a) + 4. * f((a + b) / 2.) + f(b));
    2. * PI * integral
}

#[test]
fn tapped_holes_match_drilling_then_threading_and_pappus_volume() {
    let session = Session::new().unwrap();
    let request = example();
    let tapped = family(&request);
    let built = part(&tapped).regenerate(&session).unwrap();
    let body = built.shape("body").unwrap();
    assert!(session.is_valid(body).unwrap());
    assert_eq!(session.subshape_count(body, ShapeType::Solid).unwrap(), 1);
    // The same holes drilled plain, with the existing thread feature cut
    // into each afterwards, remove exactly the same material.
    let mut plain = request.clone();
    let features = plain["model"]["family"]["features"].as_array_mut().unwrap();
    for f in features.iter_mut() {
        if let Some(hole) = f["operation"].get_mut("hole") {
            hole["thread"]["modeled"] = Value::Null;
        }
    }
    let thread = |id: &str, input: &str, z: f64, x: f64, major: f64, pitch: f64, length: f64| {
        json!({"id": id, "operation": {"thread": {
            "input": input,
            "origin": {"literal": {"x": {"value": x, "dimension": "length", "unit": "millimeter"},
                "y": {"value": 15, "dimension": "length", "unit": "millimeter"},
                "z": {"value": z, "dimension": "length", "unit": "millimeter"}}},
            "axis": {"literal": {"x": {"value": 0, "dimension": "scalar", "unit": null},
                "y": {"value": 0, "dimension": "scalar", "unit": null},
                "z": {"value": -1, "dimension": "scalar", "unit": null}}},
            "major_diameter": mm(major), "pitch": mm(pitch), "length": mm(length),
            "internal": true}}})
    };
    features.push(thread("through-thread", "body", 20., 12., 10., 1.5, 20.));
    let mut blind = thread("threaded", "through-thread", 20., 30., 6., 1., 9.);
    blind["operation"]["thread"]["runout"] = mm(2.);
    features.push(blind);
    let plain = family(&plain);
    let reference = part(&plain).regenerate(&session).unwrap();
    let expected = session
        .volume(reference.shape("threaded").unwrap())
        .unwrap();
    let volume = session.volume(body).unwrap();
    // Boolean volumes of helical sweeps agree to ~1e-8 relative.
    assert!(
        (volume - expected).abs() < 1e-7 * expected,
        "{volume} vs {expected}"
    );
    // Against the drilled-only block, the grooves remove 20/1.5 and 9 turns
    // of Pappus volume, less partial turns where they leave the material.
    let drilled = session.volume(reference.shape("body").unwrap()).unwrap();
    let removed = drilled - volume;
    let (through, blind) = (
        groove_turn_volume(4.25, 5., 1.5),
        groove_turn_volume(2.5, 3., 1.),
    );
    // The 2 mm runout adds less than two more full turns of the blind groove.
    let full = through * 20. / 1.5 + blind * 9.;
    assert!(
        removed < full + 2. * blind && removed > full - through - blind,
        "{removed} of {full}"
    );
}

#[test]
fn blind_threads_stay_above_the_bore_floor_and_edits_rebuild() {
    let session = Session::new().unwrap();
    let tapped = family(&only_threaded(2));
    let mut block = part(&tapped);
    let first = block.regenerate(&session).unwrap();
    let mut names = HashSet::new();
    crate::regeneration::collect_operation_parameters(&tapped.features[2].operation, &mut names);
    assert!(names.contains("blind_thread") && names.contains("blind_depth"));
    let mut unthreaded = only_threaded(2);
    unthreaded["model"]["family"]["features"][2]["operation"]["hole"]["thread"]["modeled"] =
        Value::Null;
    let unthreaded = family(&unthreaded);
    for (depth, run) in [(14., 9.), (12., 9.5), (18., 4.)] {
        block.overrides.insert(
            "blind_depth".into(),
            ParameterValue::Scalar(Quantity::length(depth, LengthUnit::Millimeter)),
        );
        block.overrides.insert(
            "blind_thread".into(),
            ParameterValue::Scalar(Quantity::length(run, LengthUnit::Millimeter)),
        );
        let built = block.regenerate_incremental(&session, &first).unwrap();
        assert!((depth, run) == (14., 9.) || built.regeneration.rebuilt.contains(&"body".into()));
        let mut reference = part(&unthreaded);
        reference.overrides = block.overrides.clone();
        let reference = reference.regenerate(&session).unwrap();
        // The material the blind thread removed lies between the entry and
        // the bore floor, within the nominal radius.
        let groove = session
            .cut(
                reference.shape("body").unwrap(),
                built.shape("body").unwrap(),
            )
            .unwrap();
        let bounds = session.exact_bounds(&groove).unwrap();
        assert!(
            bounds.min.z >= 20. - depth - 1e-7,
            "{depth}/{run}: {bounds:?}"
        );
        assert!(bounds.min.z <= 20. - run + 0.5, "{depth}/{run}: {bounds:?}");
        // Bounding boxes of swept surfaces are accurate to ~1e-5 mm.
        assert!(
            (bounds.max.x - 33.).abs() < 1e-4 && (bounds.min.x - 27.).abs() < 1e-4,
            "{bounds:?}"
        );
    }
}

#[test]
fn left_handed_threads_mirror_right_handed_volume() {
    let session = Session::new().unwrap();
    let mut left = only_threaded(1);
    left["model"]["family"]["features"][1]["operation"]["hole"]["thread"]["handedness"] =
        json!("left");
    let (right, left) = (family(&only_threaded(1)), family(&left));
    let (r, l) = (
        part(&right).regenerate(&session).unwrap(),
        part(&left).regenerate(&session).unwrap(),
    );
    let (r, l) = (r.shape("through").unwrap(), l.shape("through").unwrap());
    let (vr, vl) = (session.volume(r).unwrap(), session.volume(l).unwrap());
    assert!((vr - vl).abs() < 1e-7 * vr, "{vr} vs {vl}");
    // Where each groove breaks the entry face it leaves vertices. The block
    // is symmetric about y = 15, the plane through the axis and the helix
    // start, so the left-handed set mirrors the right-handed one and differs
    // from it. Vertex queries avoid slow booleans between helical solids.
    let entry = |body: &Shape<'_>| {
        let mut points = session
            .subshapes(body, ShapeType::Vertex)
            .unwrap()
            .iter()
            .map(|v| session.exact_bounds(v).unwrap().min)
            .filter(|p| (p.z - 20.).abs() < 1e-6 && (p.x - 12.).hypot(p.y - 15.) < 5.5)
            .map(|p| [(p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64])
            .collect::<Vec<_>>();
        points.sort_unstable();
        points
    };
    let (rh, lh) = (entry(r), entry(l));
    let mut mirrored = rh
        .iter()
        .map(|[x, y]| [*x, 300_000 - y])
        .collect::<Vec<_>>();
    mirrored.sort_unstable();
    assert!(!rh.is_empty() && rh != lh, "{rh:?}");
    assert_eq!(mirrored, lh);
}

#[test]
fn invalid_modeled_threads_fail_without_leaking_handles() {
    let session = Session::new().unwrap();
    let cases: [(&str, Value, &str); 5] = [
        (
            "tap_drill",
            json!({"literal": {"value": 8.2, "dimension": "length", "unit": "millimeter"}}),
            "basic minor diameter",
        ),
        ("modeled", json!({"length": mm(13.6)}), "at most 13.500 mm"),
        ("modeled", json!({"length": mm(0.)}), "must be positive"),
        ("extent", json!("up_to_next"), "blind or through-all"),
        (
            "nominal",
            mm(4.),
            "nominal diameter must exceed bore diameter",
        ),
    ];
    for (field, value, message) in cases {
        let mut request = example();
        let hole = if field == "tap_drill" {
            &mut request["model"]["family"]["features"][1]["operation"]["hole"]
        } else {
            &mut request["model"]["family"]["features"][2]["operation"]["hole"]
        };
        match field {
            "tap_drill" => hole["diameter"] = value,
            "modeled" => hole["thread"]["modeled"] = value,
            "extent" => {
                hole["extent"] = value;
                hole["bottom"] = json!("flat");
            }
            _ => hole["thread"]["nominal_diameter"] = value,
        }
        let definition = family(&request);
        let Err(error) = part(&definition).regenerate(&session) else {
            panic!("{field} should fail");
        };
        assert!(
            error.message.contains(message),
            "{field}: {}",
            error.message
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
