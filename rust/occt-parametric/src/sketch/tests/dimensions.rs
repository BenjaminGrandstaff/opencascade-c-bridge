//! Axis-aligned distances, line and arc angles, curve lengths, collinear and
//! coradial relations, and driven (reference) dimensions (schema 122).

use super::*;
use serde_json::{Value, json};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_3, PI, TAU};

fn mm(value: f64) -> Value {
    json!({"literal": {"value": value, "dimension": "length", "unit": "millimeter"}})
}

fn rad(value: f64) -> Value {
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

fn solve(sketch: &SketchDefinition) -> SketchSolution {
    let solution = sketch.solve(&HashMap::new()).unwrap();
    assert!(solution.solved, "{}", solution.max_residual);
    solution
}

fn at(solution: &SketchSolution, id: &str) -> (f64, f64) {
    let p = solution.points[id];
    (p.x, p.y)
}

fn close(actual: (f64, f64), x: f64, y: f64) -> bool {
    (actual.0 - x).hypot(actual.1 - y) < 1e-8
}

fn measured(sketch: &SketchDefinition, solution: &SketchSolution) -> HashMap<String, f64> {
    sketch
        .measure(&HashMap::new(), solution)
        .unwrap()
        .into_iter()
        .collect()
}

#[test]
fn axis_distances_and_line_angles_place_free_points_exactly() {
    let s = sketch(json!({
        "points": [p("o", 0., 0., true), p("a", 5., 5., false), p("b", 40., 5., false)],
        "lines": [{"id": "run", "start": "o", "end": "a"}, {"id": "rise", "start": "a", "end": "b"}],
        "constraints": [
            {"horizontal_distance": {"first": "o", "second": "a", "value": mm(30.)}},
            {"vertical_distance": {"first": "o", "second": "a", "value": mm(-10.)}},
            {"line_angle": {"line": "rise", "value": rad(FRAC_PI_3)}},
            {"distance": {"first": "a", "second": "b", "value": mm(20.)}},
        ],
    }));
    let solution = solve(&s);
    assert_eq!(solution.free_degrees, 0);
    assert!(close(at(&solution, "a"), 30., -10.));
    assert!(close(at(&solution, "b"), 40., -10. + 20. * FRAC_PI_3.sin()));
    // A negative line angle points the line below the axis.
    let mut down = s.clone();
    down.constraints[2] = serde_json::from_value(
        json!({"line_angle": {"line": "rise", "value": rad(-3. * PI / 4.)}}),
    )
    .unwrap();
    let solution = solve(&down);
    let b = at(&solution, "b");
    assert!(
        close(b, 30. - 20. * 0.5f64.sqrt(), -10. - 20. * 0.5f64.sqrt()),
        "{b:?}"
    );
}

#[test]
fn arc_angles_and_curve_lengths_drive_arcs_circles_and_lines() {
    for (clockwise, end) in [(false, (0., 10.)), (true, (0., -10.))] {
        let s = sketch(json!({
            "points": [p("c", 0., 0., true), p("s", 10., 0., true), p("e", 3., 9., false)],
            "arcs": [{"id": "arc", "center": "c", "start": "s", "end": "e", "clockwise": clockwise}],
            "constraints": [{"arc_angle": {"arc": "arc", "value": rad(FRAC_PI_2)}}],
        }));
        let s = SketchDefinition {
            points: if clockwise {
                let mut points = s.points.clone();
                points[2].y = serde_json::from_value(mm(-9.)).unwrap();
                points
            } else {
                s.points.clone()
            },
            ..s
        };
        let solution = solve(&s);
        assert!(close(at(&solution, "e"), end.0, end.1), "{clockwise}");
    }
    // Circle circumference and a line equal in length to a quarter arc.
    let s = sketch(json!({
        "points": [p("c", 0., 0., true), p("r", 5., 0.5, false),
            p("ac", 0., 0., true), p("as", 10., 0., true), p("ae", 0., 10., true),
            p("l0", 0., -20., true), p("l1", 10., -20., false)],
        "lines": [{"id": "line", "start": "l0", "end": "l1"}],
        "circles": [{"id": "ring", "center": "c", "rim": "r"}],
        "arcs": [{"id": "quarter", "center": "ac", "start": "as", "end": "ae", "clockwise": false}],
        "constraints": [
            {"vertical_distance": {"first": "c", "second": "r", "value": mm(0.)}},
            {"curve_length": {"curve": "ring", "value": mm(TAU * 7.)}},
            {"horizontal": {"line": "line"}},
            {"equal_curve_length": {"first": "line", "second": "quarter"}},
        ],
    }));
    let solution = solve(&s);
    assert!(close(at(&solution, "r"), 7., 0.));
    assert!(close(at(&solution, "l1"), 5. * PI, -20.));
}

#[test]
fn native_spline_and_ellipse_lengths_are_driven_and_measured() {
    // Raise a spline's middle point until the curve is 60 mm long.
    let s = sketch(json!({
        "points": [p("a", 0., 0., true), p("m", 25., 3., false), p("b", 50., 0., true),
            p("c", 0., -30., true), p("j", 12., -30., true), p("n", 0., -24., false)],
        "splines": [{"id": "crest", "points": ["a", "m", "b"]}],
        "ellipses": [{"id": "oval", "center": "c", "major": "j", "minor": "n"}],
        "constraints": [
            {"horizontal_distance": {"first": "a", "second": "m", "value": mm(25.)}},
            {"curve_length": {"curve": "crest", "value": mm(60.)}},
            {"horizontal_distance": {"first": "c", "second": "n", "value": mm(0.)}},
            {"curve_length": {"curve": "oval", "value": mm(60.)}},
        ],
        "measurements": [
            {"id": "crest_length", "measure": {"curve_length": {"curve": "crest"}}},
            {"id": "oval_length", "measure": {"curve_length": {"curve": "oval"}}},
        ],
    }));
    let solution = solve(&s);
    let m = at(&solution, "m");
    assert!((m.0 - 25.).abs() < 1e-9 && m.1 > 10., "{m:?}");
    let values = measured(&s, &solution);
    assert!((values["crest_length"] - 60.).abs() < 1e-7, "{values:?}");
    assert!((values["oval_length"] - 60.).abs() < 1e-7, "{values:?}");
    // Ramanujan's perimeter for the solved semi-axes agrees to 1e-6.
    let b = (at(&solution, "n").1 + 30.).abs();
    let (a, h) = (12., ((12. - b) / (12. + b)).powi(2));
    let perimeter = PI * (a + b) * (1. + 3. * h / (10. + (4. - 3. * h).sqrt()));
    assert!((perimeter - 60.).abs() < 1e-4, "{b} {perimeter}");
}

#[test]
fn collinear_lines_and_coradial_curves_share_their_supports() {
    let s = sketch(json!({
        "points": [p("a", 0., 0., true), p("b", 10., 5., true), p("c", 30., 12., false),
            p("d", 50., 30., false), p("o", 0., 40., true), p("r", 8., 40., true),
            p("oc", 3., 44., false), p("q", 9., 46., false), p("s", 0., 50., false),
            p("e", -10., 40., false)],
        "lines": [{"id": "first", "start": "a", "end": "b"}, {"id": "second", "start": "c", "end": "d"}],
        "circles": [{"id": "ring", "center": "o", "rim": "r"}],
        "arcs": [{"id": "cap", "center": "oc", "start": "s", "end": "e", "clockwise": false}],
        "constraints": [
            {"collinear": {"first": "first", "second": "second"}},
            {"horizontal_distance": {"first": "a", "second": "c", "value": mm(30.)}},
            {"horizontal_distance": {"first": "a", "second": "d", "value": mm(50.)}},
            {"coradial": {"first": "ring", "second": "cap"}},
            {"horizontal_distance": {"first": "o", "second": "s", "value": mm(4.8)}},
            {"vertical_distance": {"first": "o", "second": "e", "value": mm(0.)}},
        ],
    }));
    let solution = solve(&s);
    assert!(close(at(&solution, "c"), 30., 15.));
    assert!(close(at(&solution, "d"), 50., 25.));
    assert!(close(at(&solution, "oc"), 0., 40.));
    assert!(close(at(&solution, "s"), 4.8, 46.4));
    assert!(close(at(&solution, "e"), -8., 40.));
}

#[test]
fn reference_dimensions_measure_without_constraining() {
    let base = sketch(json!({
        "points": [p("o", 0., 0., true), p("x", 30., 0., true), p("y", 30., 20., false),
            p("c", 0., 0., true), p("s", 0., 10., true), p("e", -10., 0., true)],
        "lines": [{"id": "bottom", "start": "o", "end": "x"}, {"id": "side", "start": "x", "end": "y"}],
        "arcs": [{"id": "arc", "center": "c", "start": "s", "end": "e", "clockwise": false}],
        "constraints": [{"distance": {"first": "x", "second": "y", "value": mm(20.)}}],
    }));
    let mut with = base.clone();
    with.measurements = serde_json::from_value(json!([
        {"id": "span", "measure": {"distance": {"first": "o", "second": "y"}}},
        {"id": "dx", "measure": {"horizontal_distance": {"first": "y", "second": "o"}}},
        {"id": "dy", "measure": {"vertical_distance": {"first": "o", "second": "y"}}},
        {"id": "offset", "measure": {"point_line_distance": {"point": "y", "line": "bottom"}}},
        {"id": "corner", "measure": {"angle": {"first": "bottom", "second": "side"}}},
        {"id": "slope", "measure": {"line_angle": {"line": "side"}}},
        {"id": "sweep", "measure": {"arc_angle": {"arc": "arc"}}},
        {"id": "r", "measure": {"radius": {"curve": "arc"}}},
        {"id": "d", "measure": {"diameter": {"curve": "arc"}}},
        {"id": "arc_length", "measure": {"curve_length": {"curve": "arc"}}},
    ]))
    .unwrap();
    // Measurements add no equations: the free point keeps one free degree
    // and the solution is unchanged.
    let (plain, solution) = (solve(&base), solve(&with));
    assert_eq!(plain.free_degrees, 1);
    assert_eq!(solution.free_degrees, 1);
    assert_eq!(plain.points, solution.points);
    let (yx, yy) = at(&solution, "y");
    let values = measured(&with, &solution);
    let expect = [
        ("span", yx.hypot(yy)),
        ("dx", -yx),
        ("dy", yy),
        ("offset", yy),
        ("corner", (yy).atan2(yx - 30.)),
        ("slope", (yy).atan2(yx - 30.)),
        ("sweep", FRAC_PI_2),
        ("r", 10.),
        ("d", 20.),
        ("arc_length", 5. * PI),
    ];
    for (id, value) in expect {
        assert!(
            (values[id] - value).abs() < 1e-9,
            "{id}: {} vs {value}",
            values[id]
        );
    }
}

#[test]
fn invalid_dimensions_and_references_are_rejected() {
    let base = json!({
        "points": [p("a", 0., 0., true), p("b", 10., 0., true), p("c", 0., 0., true),
            p("r", 5., 0., true)],
        "lines": [{"id": "l", "start": "a", "end": "b"}],
        "circles": [{"id": "ring", "center": "c", "rim": "r"}],
    });
    let reject = |extra: Value, message: &str| {
        let mut value = base.clone();
        for (key, item) in extra.as_object().unwrap() {
            value[key] = item.clone();
        }
        let error = sketch(value).solve(&HashMap::new()).unwrap_err();
        assert!(
            error.message.contains(message),
            "{message}: {}",
            error.message
        );
    };
    reject(
        json!({"constraints": [{"arc_angle": {"arc": "ring", "value": rad(1.)}}]}),
        "requires an arc",
    );
    reject(
        json!({"constraints": [{"curve_length": {"curve": "nope", "value": mm(1.)}}]}),
        "unknown curve",
    );
    reject(
        json!({"constraints": [{"curve_length": {"curve": "l", "value": mm(0.)}}]}),
        "must be positive",
    );
    reject(
        json!({"constraints": [{"equal_curve_length": {"first": "l", "second": "l"}}]}),
        "two distinct",
    );
    reject(
        json!({"constraints": [{"coradial": {"first": "ring", "second": "l"}}]}),
        "circles or arcs",
    );
    reject(
        json!({"constraints": [{"collinear": {"first": "l", "second": "l"}}]}),
        "two distinct lines",
    );
    reject(
        json!({"constraints": [{"line_angle": {"line": "l", "value": rad(4.)}}]}),
        "[-pi, pi]",
    );
    reject(
        json!({"constraints": [{"line_angle": {"line": "ring", "value": rad(1.)}}]}),
        "unknown sketch line",
    );
    reject(
        json!({"measurements": [
        {"id": "m", "measure": {"line_angle": {"line": "l"}}},
        {"id": "m", "measure": {"radius": {"curve": "ring"}}}]}),
        "unique",
    );
    reject(
        json!({"measurements": [{"id": "m", "measure": {"radius": {"curve": "l"}}}]}),
        "circle or arc",
    );
}
