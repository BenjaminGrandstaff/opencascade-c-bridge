//! Rectangles, slots and regular polygons expanded into ordinary sketch
//! geometry (schema 123).

use super::*;
use serde_json::{Value, json};
use std::f64::consts::{FRAC_PI_6, PI, TAU};

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

fn solved(sketch: &SketchDefinition) -> SketchSolution {
    let solution = sketch.solve(&HashMap::new()).unwrap();
    assert!(solution.solved, "{}", solution.max_residual);
    assert_eq!(solution.free_degrees, 0);
    assert_eq!(solution.redundant_equations, 0);
    solution
}

fn close(solution: &SketchSolution, id: &str, x: f64, y: f64) -> bool {
    let p = solution.points[id];
    (p.x - x).hypot(p.y - y) < 1e-8
}

fn area(sketch: &SketchDefinition) -> f64 {
    let session = Session::new().unwrap();
    let face = sketch.face(&session, &HashMap::new()).unwrap();
    session.surface_area(&face).unwrap()
}

#[test]
fn axis_and_rotated_rectangles_close_on_their_corners() {
    let plain = sketch(json!({
        "points": [p("a", 2., 3., true), p("c", 12., 9., true)],
        "shapes": [{"rectangle": {"id": "r", "first": "a", "opposite": "c"}}],
        "profile": ["r"],
    }));
    let solution = solved(&plain);
    assert!(close(&solution, "r:corner1", 12., 3.));
    assert!(close(&solution, "r:corner3", 2., 9.));
    assert!((area(&plain) - 60.).abs() < 1e-9);
    // Side0 at 30°: the corners are the projections of the diagonal.
    let tilted = sketch(json!({
        "points": [p("a", 0., 0., true), p("c", 10., 10., true)],
        "shapes": [{"rectangle": {"id": "r", "first": "a", "opposite": "c",
            "angle": {"literal": {"value": FRAC_PI_6, "dimension": "scalar", "unit": null}}}}],
        "profile": ["r"],
    }));
    let solution = solved(&tilted);
    let (u, v) = (
        (FRAC_PI_6.cos(), FRAC_PI_6.sin()),
        (-FRAC_PI_6.sin(), FRAC_PI_6.cos()),
    );
    let (along, across) = (10. * (u.0 + u.1), 10. * (v.0 + v.1));
    assert!(close(&solution, "r:corner1", along * u.0, along * u.1));
    assert!(close(&solution, "r:corner3", across * v.0, across * v.1));
    assert!((area(&tilted) - along * across).abs() < 1e-9);
}

#[test]
fn slots_are_tangent_obrounds_at_any_direction() {
    for (end, radius) in [((20., 0.), 4.), ((12., 16.), 3.)] {
        let s = sketch(json!({
            "points": [p("a", 0., 0., true), p("b", end.0, end.1, true)],
            "shapes": [{"slot": {"id": "slot", "start": "a", "end": "b", "radius": mm(radius)}}],
            "profile": ["slot"],
        }));
        let solution = solved(&s);
        let length = end.0.hypot(end.1);
        let n = (-end.1 / length, end.0 / length);
        assert!(close(
            &solution,
            "slot:end_left",
            end.0 + radius * n.0,
            end.1 + radius * n.1
        ));
        assert!(close(
            &solution,
            "slot:start_right",
            -radius * n.0,
            -radius * n.1
        ));
        let expected = 2. * radius * length + PI * radius * radius;
        assert!((area(&s) - expected).abs() < 1e-9, "{end:?}");
    }
}

#[test]
fn polygons_are_regular_for_every_side_count() {
    for sides in [3u32, 6, 13, 200] {
        let s = sketch(json!({
            "points": [p("c", 5., 5., true), p("v", 15., 5., true)],
            "shapes": [{"polygon": {"id": "poly", "center": "c", "vertex": "v", "sides": sides}}],
            "profile": ["poly"],
        }));
        let solution = solved(&s);
        let step = TAU / f64::from(sides);
        for k in 1..sides {
            let t = step * f64::from(k);
            assert!(close(
                &solution,
                &format!("poly:vertex{k}"),
                5. + 10. * t.cos(),
                5. + 10. * t.sin()
            ));
        }
        let n = f64::from(sides);
        let expected = 0.5 * n * 100. * step.sin();
        assert!((area(&s) - expected).abs() < 1e-9 * expected, "{sides}");
    }
}

#[test]
fn shapes_combine_as_holes_take_constraints_and_feed_patterns() {
    // A plate whose height is driven through a generated side, with a slot
    // hole copied twice by a linear pattern and a hexagonal hole.
    let s = sketch(json!({
        "points": [p("a", 0., 0., true), p("c", 60., 25., false), p("s0", 10., 10., true),
            p("s1", 18., 10., true), p("h", 50., 12., true), p("hv", 54., 12., true),
            p("d0", 0., -10., true), p("d1", 1., -10., true)],
        "lines": [{"id": "dir", "start": "d0", "end": "d1"}],
        "shapes": [{"rectangle": {"id": "plate", "first": "a", "opposite": "c"}},
            {"slot": {"id": "slot", "start": "s0", "end": "s1", "radius": mm(3.)}},
            {"polygon": {"id": "hex", "center": "h", "vertex": "hv", "sides": 6}}],
        "patterns": [{"linear": {"id": "row", "entities": ["slot:right", "slot:end_cap", "slot:left", "slot:start_cap"],
            "direction": "dir", "spacing": mm(20.), "count": 2}}],
        "profile": ["plate"],
        "holes": [["slot"], ["row:1:slot:right", "row:1:slot:end_cap", "row:1:slot:left", "row:1:slot:start_cap"], ["hex"]],
        "constraints": [
            {"curve_length": {"curve": "plate:side1", "value": mm(24.)}},
            {"horizontal_distance": {"first": "a", "second": "c", "value": mm(64.)}},
        ],
    }));
    // Pattern copies of the slot's two arcs repeat their equal-radius
    // equations, already implied by the rigid copy links.
    let solution = s.solve(&HashMap::new()).unwrap();
    assert!(solution.solved);
    assert_eq!(
        (solution.free_degrees, solution.redundant_equations),
        (0, 2)
    );
    assert!(close(&solution, "c", 64., 24.));
    let slot = 2. * 3. * 8. + PI * 9.;
    let hex = 3. * 3f64.sqrt() / 2. * 16.;
    assert!((area(&s) - (64. * 24. - 2. * slot - hex)).abs() < 1e-8);
}

#[test]
fn invalid_shapes_are_rejected() {
    let reject = |shapes: Value, message: &str| {
        let s = sketch(json!({
            "points": [p("a", 0., 0., true), p("b", 5., 5., true)],
            "lines": [{"id": "r:side0", "start": "a", "end": "b"}],
            "shapes": shapes,
        }));
        let error = s.solve(&HashMap::new()).unwrap_err();
        assert!(
            error.message.contains(message),
            "{message}: {}",
            error.message
        );
    };
    reject(
        json!([{"slot": {"id": "x", "start": "a", "end": "b", "radius": mm(1.)}},
            {"slot": {"id": "x", "start": "a", "end": "b", "radius": mm(1.)}}]),
        "unique",
    );
    reject(
        json!([{"slot": {"id": "x:y", "start": "a", "end": "b", "radius": mm(1.)}}]),
        "free of ':'",
    );
    reject(
        json!([{"rectangle": {"id": "q", "first": "a", "opposite": "nope"}}]),
        "unknown point 'nope'",
    );
    reject(
        json!([{"polygon": {"id": "q", "center": "a", "vertex": "b", "sides": 2}}]),
        "3 to 1000 sides",
    );
    reject(
        json!([{"slot": {"id": "q", "start": "a", "end": "b", "radius": mm(0.)}}]),
        "positive",
    );
    // A generated ID that collides with an authored entity.
    reject(
        json!([{"rectangle": {"id": "r", "first": "a", "opposite": "b"}}]),
        "unique",
    );
}
