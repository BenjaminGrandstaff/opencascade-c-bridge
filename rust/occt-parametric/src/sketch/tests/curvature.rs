//! Curvature (G2) continuity at shared ends (schema 128 / ABI 65).

use super::*;
use serde_json::{Value, json};

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

fn hd(a: &str, b: &str, v: f64) -> Value {
    json!({"horizontal_distance": {"first": a, "second": b, "value": mm(v)}})
}

fn vd(a: &str, b: &str, v: f64) -> Value {
    json!({"vertical_distance": {"first": a, "second": b, "value": mm(v)}})
}

fn g2(a: &str, b: &str, at: &str) -> Value {
    json!({"curvature_continuous": {"first": a, "second": b, "point": at}})
}

fn solved(s: &SketchDefinition) -> SketchSolution {
    let solution = s.solve(&HashMap::new()).unwrap();
    assert!(solution.solved, "{}", solution.max_residual);
    assert_eq!(solution.free_degrees, 0);
    solution
}

fn at(solution: &SketchSolution, id: &str, x: f64, y: f64) -> bool {
    let q = solution.points[id];
    (q.x - x).hypot(q.y - y) < 1e-8
}

/// The cubic Bezier `k` with poles a, p1, p2, e as a native wire.
fn bezier<'a>(session: &'a Session, solution: &SketchSolution, poles: [&str; 4]) -> Shape<'a> {
    let v = |id: &str| Vec3::new(solution.points[id].x, solution.points[id].y, 0.0);
    session
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: false,
                poles: poles.map(v).to_vec(),
                degree: 3,
                knots: vec![0., 1.],
                multiplicities: vec![4, 4],
                weights: vec![],
            }],
            false,
        )
        .unwrap()
}

fn cubic(id: &str, poles: [&str; 4]) -> Value {
    json!({"id": id, "points": poles, "basis": {"degree": 3, "knots": [0., 1.], "multiplicities": [4, 4]}})
}

#[test]
fn arcs_continue_onto_the_same_circle() {
    let s = sketch(json!({
        "points": [p("c", 0., 0., true), p("s", 10., 0., true), p("j", 0., 10., true),
            p("cb", 1., 1., false), p("e", -9., 2., false)],
        "arcs": [{"id": "first", "center": "c", "start": "s", "end": "j"},
            {"id": "second", "center": "cb", "start": "j", "end": "e"}],
        "constraints": [g2("first", "second", "j"), vd("cb", "e", 0.)],
    }));
    let solution = solved(&s);
    assert!(at(&solution, "cb", 0., 0.) && at(&solution, "e", -10., 0.));
}

#[test]
fn lines_flatten_a_cubic_and_arcs_bend_it_to_their_curvature() {
    // Leaving a line, zero curvature puts the first three poles in a row.
    let s = sketch(json!({
        "points": [p("l0", -10., 0., true), p("a", 0., 0., true), p("p1", 3., 1., false),
            p("p2", 6., 2., false), p("e", 10., 5., true)],
        "lines": [{"id": "lead", "start": "l0", "end": "a"}],
        "splines": [cubic("k", ["a", "p1", "p2", "e"])],
        "constraints": [g2("lead", "k", "a"), hd("a", "p1", 3.), hd("a", "p2", 6.)],
    }));
    let solution = solved(&s);
    assert!(at(&solution, "p1", 3., 0.) && at(&solution, "p2", 6., 0.));
    // Leaving a radius-10 arc at its top: the tangent leg is horizontal and
    // (2/3)·4·|Δy| / 4³ = 0.1 puts the second pole 2.4 below it.
    let s = sketch(json!({
        "points": [p("c", 0., 0., true), p("s", 10., 0., true), p("a", 0., 10., true),
            p("p1", -4., 11., false), p("p2", -8., 9., false), p("e", -12., 2., true)],
        "arcs": [{"id": "bend", "center": "c", "start": "s", "end": "a"}],
        "splines": [cubic("k", ["a", "p1", "p2", "e"])],
        "constraints": [g2("bend", "k", "a"), hd("a", "p1", -4.), hd("a", "p2", -8.)],
    }));
    let solution = solved(&s);
    assert!(at(&solution, "p1", -4., 10.) && at(&solution, "p2", -8., 7.6));
    let session = Session::new().unwrap();
    let wire = bezier(&session, &solution, ["a", "p1", "p2", "e"]);
    let k = session
        .curve_curvature_at(&wire, Vec3::new(0., 10., 0.))
        .unwrap();
    assert!(k.x.abs() < 1e-9 && (k.y + 0.1).abs() < 1e-9, "{k:?}");
}

#[test]
fn circular_arcs_osculate_ellipses_at_their_vertices() {
    // At the major vertex of a 12 × 5 ellipse the radius of curvature is
    // b²/a = 25/12; the circle continues with that radius and centre.
    let s = sketch(json!({
        "points": [p("c", 0., 0., true), p("a", 12., 0., true), p("m", 0., 5., true),
            p("cb", 9., 0.5, false), p("sb", 9.5, -2.5, false)],
        "ellipses": [{"id": "oval", "center": "c", "major": "a", "minor": "m", "start": "a", "end": "m"}],
        "arcs": [{"id": "tip", "center": "cb", "start": "sb", "end": "a"}],
        "constraints": [g2("tip", "oval", "a"), hd("cb", "sb", 0.)],
    }));
    let solution = solved(&s);
    let r = 25. / 12.;
    assert!(at(&solution, "cb", 12. - r, 0.) && at(&solution, "sb", 12. - r, -r));
}

#[test]
fn impossible_and_invalid_curvature_joints_are_reported() {
    // A line cannot continue a circle's curvature: the solve fails.
    let s = sketch(json!({
        "points": [p("c", 0., 0., true), p("s", 10., 0., true), p("j", 0., 10., true),
            p("e", -5., 11., false)],
        "lines": [{"id": "run", "start": "j", "end": "e"}],
        "arcs": [{"id": "arc", "center": "c", "start": "s", "end": "j"}],
        "constraints": [g2("arc", "run", "j")],
    }));
    assert!(!s.solve(&HashMap::new()).unwrap().solved);
    for (constraint, message) in [
        (g2("arc", "arc", "j"), "two distinct"),
        (g2("arc", "run", "s"), "shared endpoint"),
        (g2("arc", "run", "nope"), "unknown sketch point"),
    ] {
        let mut bad = s.clone();
        bad.constraints = vec![serde_json::from_value(constraint).unwrap()];
        let error = bad.solve(&HashMap::new()).unwrap_err();
        assert!(
            error.message.contains(message),
            "{message}: {}",
            error.message
        );
    }
}
