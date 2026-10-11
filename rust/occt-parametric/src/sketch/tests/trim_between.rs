//! Trims between named points, curve ends and native crossings, including
//! closed curves that wrap through their seam (schema 126 / ABI 64).

use super::*;
use serde_json::{Value, json};
use std::f64::consts::PI;

fn mm(value: f64) -> Value {
    json!({"literal": {"value": value, "dimension": "length", "unit": "millimeter"}})
}

fn sketch(rest: Value) -> SketchDefinition {
    let mut value = json!({
        "id": "s",
        "origin": {"literal": {"x": {"value": 0, "dimension": "length", "unit": "millimeter"},
            "y": {"value": 0, "dimension": "length", "unit": "millimeter"},
            "z": {"value": 0, "dimension": "length", "unit": "millimeter"}}},
        "x_axis": {"literal": {"x": {"value": 1, "dimension": "scalar", "unit": null},
            "y": {"value": 0, "dimension": "scalar", "unit": null},
            "z": {"value": 0, "dimension": "scalar", "unit": null}}},
        "y_axis": {"literal": {"x": {"value": 0, "dimension": "scalar", "unit": null},
            "y": {"value": 1, "dimension": "scalar", "unit": null},
            "z": {"value": 0, "dimension": "scalar", "unit": null}}},
        "points": [],
        "lines": [],
    });
    for (key, item) in rest.as_object().unwrap() {
        value[key] = item.clone();
    }
    serde_json::from_value(value).unwrap()
}

fn p(id: &str, x: f64, y: f64, fixed: bool) -> Value {
    json!({"id": id, "x": mm(x), "y": mm(y), "fixed": fixed})
}

fn face(sketch: &SketchDefinition) -> Result<f64, ModelError> {
    let session = Session::new().unwrap();
    let face = sketch.face(&session, &HashMap::new())?;
    assert!(session.is_valid(&face).unwrap());
    Ok(session.surface_area(&face).unwrap())
}

/// Keyhole: a radius-6 circle at (0, 10) cut back to the arc between two
/// legs x = ±2, which are themselves trimmed where they cross it.
fn keyhole(operations: Value) -> SketchDefinition {
    sketch(json!({
        "points": [p("c", 0., 10., true), p("r", 6., 10., true),
            p("rb", 2., -10., true), p("rt", 2., 12., true),
            p("lt", -2., 12., true), p("lb", -2., -10., true),
            p("hint_r", 2., 4., true), p("hint_l", -2., 4., true)],
        "lines": [{"id": "right", "start": "rb", "end": "rt"},
            {"id": "left", "start": "lt", "end": "lb"},
            {"id": "bottom", "start": "lb", "end": "rb"}],
        "circles": [{"id": "head", "center": "c", "rim": "r"}],
        "profile": ["right", "head", "left", "bottom"],
        "profile_operations": operations,
    }))
}

fn keyhole_operations() -> Value {
    json!([
        {"trim_between": {"entity": "right", "from": "start",
            "to": {"crossing": {"curve": "head", "near": "hint_r"}}}},
        {"trim_between": {"entity": "head",
            "from": {"crossing": {"curve": "right", "near": "hint_r"}},
            "to": {"crossing": {"curve": "left", "near": "hint_l"}}}},
        {"trim_between": {"entity": "left",
            "from": {"crossing": {"curve": "head", "near": "hint_l"}}, "to": "end"}},
    ])
}

#[test]
fn keyholes_trim_a_circle_and_its_legs_at_native_crossings() {
    let area = face(&keyhole(keyhole_operations())).unwrap();
    // Legs meet the circle at y = 10 − √32; the head is the circle minus
    // the small segment below that chord.
    let d = 32f64.sqrt();
    let segment = 36. * (d / 6.).acos() - d * 2.;
    let expected = 4. * (20. - d) + PI * 36. - segment;
    assert!((area - expected).abs() < 1e-9, "{area} vs {expected}");
    // Fillets may follow the trims: they act on the joined profile.
    let mut filleted = keyhole_operations();
    filleted
        .as_array_mut()
        .unwrap()
        .push(json!({"fillet": {"point": "rb", "radius": mm(1.)}}));
    assert!(face(&keyhole(filleted)).unwrap() < expected);
}

#[test]
fn closed_curves_wrap_between_named_points() {
    // A full 10 × 4 ellipse trimmed counterclockwise from (−10, 0) over
    // the bottom to (10, 0) and closed by the diameter: the lower half.
    let s = sketch(json!({
        "points": [p("c", 0., 0., true), p("a", 10., 0., true), p("m", 0., 4., true),
            p("w", -10., 0., true), p("e", 10., 0., true)],
        "lines": [{"id": "chord", "start": "e", "end": "w"}],
        "ellipses": [{"id": "oval", "center": "c", "major": "a", "minor": "m"}],
        "profile": ["oval", "chord"],
        "profile_operations": [{"trim_between": {"entity": "oval",
            "from": {"point": {"point": "w"}}, "to": {"point": {"point": "e"}}}}],
    }));
    assert!((face(&s).unwrap() - PI * 20.).abs() < 1e-9);
    // The upper half needs the opposite order: from e to w through the top.
    let mut upper = s.clone();
    upper.lines[0] =
        serde_json::from_value(json!({"id": "chord", "start": "w", "end": "e"})).unwrap();
    upper.profile_operations = serde_json::from_value(json!([{"trim_between": {"entity": "oval",
        "from": {"point": {"point": "e"}}, "to": {"point": {"point": "w"}}}}]))
    .unwrap();
    let session = Session::new().unwrap();
    let face = upper.face(&session, &HashMap::new()).unwrap();
    let bounds = session.exact_bounds(&face).unwrap();
    assert!((bounds.max.y - 4.).abs() < 1e-9 && bounds.min.y.abs() < 1e-9);
}

#[test]
fn spline_ends_trim_to_a_line_crossing() {
    // A spline arch cut by a horizontal line, keeping the part from its
    // start to the first crossing; a line closes the remaining piece.
    let s = sketch(json!({
        "points": [p("a", 0., 0., true), p("t", 10., 12., true), p("b", 20., 0., true),
            p("g0", -5., 6., true), p("g1", 25., 6., true), p("near", 3., 6., true)],
        "lines": [{"id": "guide", "start": "g0", "end": "g1"}],
        "splines": [{"id": "arch", "points": ["a", "t", "b"]}],
        "profile": ["arch"],
        "profile_operations": [{"trim_between": {"entity": "arch", "from": "start",
            "to": {"crossing": {"curve": "guide", "near": "near"}}}}],
    }));
    let session = Session::new().unwrap();
    let solution = s.solve(&HashMap::new()).unwrap();
    let preview = s
        .preview_edited_profile(&session, &HashMap::new(), &solution, 16, false)
        .unwrap();
    let last = *preview.last().unwrap().last().unwrap();
    assert!((last.y - 6.).abs() < 1e-9 && last.x < 10., "{last:?}");
    let first = preview[0][0];
    assert!(first.x.abs() < 1e-12 && first.y.abs() < 1e-12);
}

#[test]
fn invalid_trim_bounds_are_rejected() {
    let reject = |operations: Value, message: &str| {
        let error = face(&keyhole(operations)).unwrap_err();
        assert!(
            error.message.contains(message),
            "{message}: {}",
            error.message
        );
    };
    reject(
        json!([{"trim_between": {"entity": "head", "from": "start", "to": "start"}}]),
        "two different bounds",
    );
    reject(
        json!([{"trim_between": {"entity": "right", "from": "start",
            "to": {"crossing": {"curve": "right", "near": "hint_r"}}}}]),
        "another known curve",
    );
    reject(
        json!([{"trim_between": {"entity": "right", "from": "start",
            "to": {"point": {"point": "nope"}}}}]),
        "unknown point 'nope'",
    );
    reject(
        json!([{"trim_between": {"entity": "bottom", "from": "start",
            "to": {"crossing": {"curve": "head", "near": "hint_r"}}}}]),
        "do not cross",
    );
    reject(
        json!([{"trim_between": {"entity": "right", "from": "start",
            "to": {"point": {"point": "c"}}}}]),
        "not on the curve",
    );
    reject(
        json!([{"fillet": {"point": "rb", "radius": mm(1.)}},
            {"trim_between": {"entity": "right", "from": "start", "to": "end"}}]),
        "must precede",
    );
}
