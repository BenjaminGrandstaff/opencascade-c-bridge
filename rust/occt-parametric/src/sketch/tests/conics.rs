//! Conic arcs as exact rational quadratic splines (schema 125).

use super::*;
use serde_json::{Value, json};
use std::f64::consts::{FRAC_1_SQRT_2, PI};

fn mm(value: f64) -> Value {
    json!({"literal": {"value": value, "dimension": "length", "unit": "millimeter"}})
}

fn ratio(value: f64) -> Value {
    json!({"literal": {"value": value, "dimension": "scalar", "unit": null}})
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

fn area(sketch: &SketchDefinition) -> f64 {
    let session = Session::new().unwrap();
    let face = sketch.face(&session, &HashMap::new()).unwrap();
    assert!(session.is_valid(&face).unwrap());
    session.surface_area(&face).unwrap()
}

/// Conic from (−10, 0) to (10, 0) with control (0, 10), closed by its chord.
fn arch(rho: Option<f64>) -> SketchDefinition {
    let mut conic = json!({"id": "k", "start": "a", "control": "t", "end": "b"});
    if let Some(rho) = rho {
        conic["rho"] = ratio(rho);
    }
    sketch(json!({
        "points": [p("a", -10., 0., true), p("t", 0., 10., true), p("b", 10., 0., true)],
        "lines": [{"id": "chord", "start": "b", "end": "a"}],
        "shapes": [{"conic": conic}],
        "profile": ["k", "chord"],
    }))
}

#[test]
fn parabolas_by_default_and_their_shoulder_follow_rho() {
    // A parabola's segment is 2/3 of its bounding base × height (height 5).
    assert!((area(&arch(None)) - 2. / 3. * 20. * 5.).abs() < 1e-9);
    assert!((area(&arch(Some(0.5))) - 2. / 3. * 20. * 5.).abs() < 1e-9);
    // The shoulder (t = 1/2) sits at rho of the way from the chord's middle
    // to the control point, for every conic type.
    for rho in [0.2, 0.5, 0.8] {
        let mut s = arch(Some(rho));
        s.points
            .push(serde_json::from_value(p("shoulder", 0., 10. * rho, true)).unwrap());
        s.constraints = serde_json::from_value(
            json!([{"point_on_curve": {"point": "shoulder", "curve": "k"}}]),
        )
        .unwrap();
        let solution = s.solve(&HashMap::new()).unwrap();
        let checks = s.constraint_checks(&HashMap::new(), &solution).unwrap();
        assert!(checks[0].satisfied, "{rho}: {:?}", checks[0]);
    }
    // Larger rho bulges further toward the control point.
    assert!(area(&arch(Some(0.2))) < area(&arch(Some(0.5))));
    assert!(area(&arch(Some(0.5))) < area(&arch(Some(0.8))));
}

#[test]
fn elliptic_conics_reproduce_circular_arcs_exactly() {
    // w = cos 45° makes a quarter circle; rho = w / (1 + w).
    let rho = FRAC_1_SQRT_2 / (1. + FRAC_1_SQRT_2);
    let s = sketch(json!({
        "points": [p("o", 0., 0., true), p("a", 10., 0., true), p("t", 10., 10., true),
            p("b", 0., 10., true), p("probe", 10. * (0.3f64).cos(), 10. * (0.3f64).sin(), true)],
        "lines": [{"id": "down", "start": "b", "end": "o"}, {"id": "out", "start": "o", "end": "a"}],
        "shapes": [{"conic": {"id": "k", "start": "a", "control": "t", "end": "b", "rho": ratio(rho)}}],
        "profile": ["out", "k", "down"],
        "constraints": [{"point_on_curve": {"point": "probe", "curve": "k"}}],
        "measurements": [{"id": "len", "measure": {"curve_length": {"curve": "k"}}}],
    }));
    assert!((area(&s) - 25. * PI).abs() < 1e-9);
    let solution = s.solve(&HashMap::new()).unwrap();
    assert!(s.constraint_checks(&HashMap::new(), &solution).unwrap()[0].satisfied);
    let length = s.measure(&HashMap::new(), &solution).unwrap()[0].1;
    assert!((length - 5. * PI).abs() < 1e-9, "{length}");
}

#[test]
fn conic_ends_are_tangent_to_lines_through_their_control_point() {
    // Lines along the control legs are tangent at both ends; solving with a
    // free control point makes it the intersection of two fixed tangents.
    let s = sketch(json!({
        "points": [p("a", 0., 0., true), p("ta", -5., -5., true), p("b", 20., 0., true),
            p("tb", 25., -5., true), p("t", 8., 9., false)],
        "lines": [{"id": "lead", "start": "ta", "end": "a"}, {"id": "tail", "start": "b", "end": "tb"}],
        "shapes": [{"conic": {"id": "k", "start": "a", "control": "t", "end": "b", "rho": ratio(0.6)}}],
        "constraints": [{"tangent": {"first": "lead", "second": "k", "point": "a"}},
            {"tangent": {"first": "k", "second": "tail", "point": "b"}}],
    }));
    let solution = s.solve(&HashMap::new()).unwrap();
    assert!(solution.solved && solution.free_degrees == 0);
    let t = solution.points["t"];
    assert!(
        (t.x - 10.).abs() < 1e-8 && (t.y - 10.).abs() < 1e-8,
        "{t:?}"
    );
}

#[test]
fn invalid_conics_are_rejected() {
    let reject = |conic: Value, message: &str| {
        let s = sketch(json!({
            "points": [p("a", 0., 0., true), p("t", 5., 5., true), p("b", 10., 0., true)],
            "shapes": [{"conic": conic}],
        }));
        let error = s.solve(&HashMap::new()).unwrap_err();
        assert!(
            error.message.contains(message),
            "{message}: {}",
            error.message
        );
    };
    for rho in [0.0, 1.0, 1.5] {
        reject(
            json!({"id": "k", "start": "a", "control": "t", "end": "b", "rho": ratio(rho)}),
            "rho must be in (0, 1)",
        );
    }
    reject(
        json!({"id": "k", "start": "a", "control": "a", "end": "b"}),
        "distinct",
    );
    reject(
        json!({"id": "k", "start": "a", "control": "q", "end": "b"}),
        "unknown point 'q'",
    );
}
