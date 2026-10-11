//! Authored elliptical arcs: named ends held on the ellipse, exact native
//! trimmed curves in profiles, tangency, spans, mirrors and fillets
//! (schema 124 / ABI 63).

use super::*;
use occt_bridge::Bounds;
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

fn face_area(sketch: &SketchDefinition) -> (f64, Bounds) {
    let session = Session::new().unwrap();
    let face = sketch.face(&session, &HashMap::new()).unwrap();
    assert!(session.is_valid(&face).unwrap());
    (
        session.surface_area(&face).unwrap(),
        session.exact_bounds(&face).unwrap(),
    )
}

/// Half ellipse (12 x 5) closed by its major diameter: the arc runs from the
/// major vertex to the free point `w`, which the solver puts on the far end.
fn d_shape(clockwise: bool) -> SketchDefinition {
    let below = if clockwise { -1.0 } else { 1.0 };
    sketch(json!({
        "points": [p("c", 0., 0., true), p("a", 12., 0., true), p("m", 0.5, 4.0 * below, false),
            p("w", -11.5, 0.5 * below, false)],
        "lines": [{"id": "chord", "start": "w", "end": "a"}],
        "ellipses": [{"id": "e", "center": "c", "major": "a", "minor": "m",
            "start": "a", "end": "w", "clockwise": clockwise}],
        "profile": ["e", "chord"],
        "constraints": [
            {"horizontal_distance": {"first": "c", "second": "m", "value": mm(0.)}},
            {"vertical_distance": {"first": "c", "second": "m", "value": mm(5. * below)}},
            {"vertical_distance": {"first": "c", "second": "w", "value": mm(0.)}},
        ],
    }))
}

#[test]
fn half_ellipse_profiles_are_exact_in_both_directions() {
    for clockwise in [false, true] {
        let s = d_shape(clockwise);
        let solution = s.solve(&HashMap::new()).unwrap();
        assert!(solution.solved && solution.free_degrees == 0);
        let w = solution.points["w"];
        assert!((w.x + 12.).abs() < 1e-9 && w.y.abs() < 1e-9, "{w:?}");
        let (area, bounds) = face_area(&s);
        assert!((area - PI * 30.).abs() < 1e-9, "{area}");
        if clockwise {
            assert!((bounds.min.y + 5.).abs() < 1e-9 && bounds.max.y.abs() < 1e-9);
        } else {
            assert!((bounds.max.y - 5.).abs() < 1e-9 && bounds.min.y.abs() < 1e-9);
        }
    }
}

#[test]
fn lines_are_exactly_tangent_to_elliptical_arcs_at_shared_ends() {
    // From (0, 20), the tangents to x²/144 + y²/25 = 1 touch at y = 1.25.
    let s = sketch(json!({
        "points": [p("c", 0., 0., true), p("a", 12., 0., true), p("m", 0., 5., true),
            p("q", 11., 2., false), p("l", 0., 20., true)],
        "lines": [{"id": "guy", "start": "q", "end": "l"}],
        "ellipses": [{"id": "e", "center": "c", "major": "a", "minor": "m", "start": "a", "end": "q"}],
        "constraints": [{"tangent": {"first": "e", "second": "guy", "point": "q"}}],
        "measurements": [{"id": "arc", "measure": {"curve_length": {"curve": "e"}}}],
    }));
    let solution = s.solve(&HashMap::new()).unwrap();
    assert!(solution.solved && solution.free_degrees == 0);
    let q = solution.points["q"];
    let x = 12. * (1f64 - 1.25 * 1.25 / 25.).sqrt();
    assert!((q.x - x).abs() < 1e-8 && (q.y - 1.25).abs() < 1e-8, "{q:?}");
    // Arc length from the major vertex by midpoint integration.
    let end = (1.25f64 / 5.).atan2(x / 12.);
    let n = 200_000;
    let reference = (0..n)
        .map(|i| {
            let t = (i as f64 + 0.5) * end / n as f64;
            (144. * t.sin().powi(2) + 25. * t.cos().powi(2)).sqrt()
        })
        .sum::<f64>()
        * end
        / n as f64;
    let measured = s.measure(&HashMap::new(), &solution).unwrap()[0].1;
    assert!(
        (measured - reference).abs() < 1e-9,
        "{measured} {reference}"
    );
}

#[test]
fn point_membership_respects_the_arc_span_on_rotated_ellipses() {
    // Ellipse rotated 30°: a point on the ellipse but outside the quarter
    // arc fails membership, one inside passes.
    let (u, v) = ((PI / 6.).cos(), (PI / 6.).sin());
    let on = |t: f64| {
        (
            10. * t.cos() * u - 4. * t.sin() * v,
            10. * t.cos() * v + 4. * t.sin() * u,
        )
    };
    let (inside, outside) = (on(0.7), on(2.5));
    let s = sketch(json!({
        "points": [p("c", 0., 0., true), p("a", 10. * u, 10. * v, true), p("m", -4. * v, 4. * u, true),
            p("e1", on(PI / 2.).0, on(PI / 2.).1, true),
            p("in", inside.0, inside.1, true), p("out", outside.0, outside.1, true)],
        "ellipses": [{"id": "e", "center": "c", "major": "a", "minor": "m", "start": "a", "end": "e1"}],
        "constraints": [{"point_on_curve": {"point": "in", "curve": "e"}},
            {"point_on_curve": {"point": "out", "curve": "e"}}],
    }));
    let solution = s.solve(&HashMap::new()).unwrap();
    let checks = s.constraint_checks(&HashMap::new(), &solution).unwrap();
    assert!(checks[0].satisfied, "{:?}", checks[0]);
    assert!(!checks[1].satisfied);
    // The violation is the angular overshoot past the end, scaled by b.
    assert!((checks[1].max_residual.unwrap() - (2.5 - PI / 2.) * 4.).abs() < 1e-9);
}

#[test]
fn mirrored_and_filleted_elliptical_arcs_close_exact_profiles() {
    // Quarter arc mirrored across the minor axis makes the half ellipse.
    let s = sketch(json!({
        "points": [p("c", 0., 0., true), p("a", 12., 0., true), p("m", 0., 5., true),
            p("axis_end", 0., 10., true)],
        "lines": [{"id": "base", "start": "c", "end": "a"}, {"id": "axis", "start": "c", "end": "axis_end"}],
        "ellipses": [{"id": "e", "center": "c", "major": "a", "minor": "m", "start": "a", "end": "m"}],
        "patterns": [{"mirror": {"id": "mir", "entities": ["base", "e"], "axis": "axis", "on_axis": ["c", "m"]}}],
        "profile": ["base", "e", "mir:1:e", "mir:1:base"],
    }));
    let (area, bounds) = face_area(&s);
    assert!((area - PI * 30.).abs() < 1e-9, "{area}");
    assert!((bounds.min.x + 12.).abs() < 1e-9);
    // A native fillet where the arc meets its chord trims both.
    let mut filleted = d_shape(false);
    filleted.profile_operations = serde_json::from_value(json!([
        {"fillet": {"point": "w", "radius": mm(1.5)}},
        {"fillet": {"point": "a", "radius": mm(1.5)}}]))
    .unwrap();
    let session = Session::new().unwrap();
    let face = filleted.face(&session, &HashMap::new()).unwrap();
    assert!(session.is_valid(&face).unwrap());
    // Two fillet arcs join the trimmed ellipse and chord.
    assert_eq!(session.subshape_count(&face, ShapeType::Edge).unwrap(), 4);
    let area = session.surface_area(&face).unwrap();
    assert!(area < PI * 30. && area > 0.9 * PI * 30., "{area}");
}

#[test]
fn invalid_elliptical_arcs_are_rejected() {
    let reject = |ellipse: Value, constraints: Value, message: &str| {
        let s = sketch(json!({
            "points": [p("c", 0., 0., true), p("a", 12., 0., true), p("m", 0., 5., true),
                p("w", -12., 0., true), p("l0", 0., 9., true), p("l1", 5., 9., true)],
            "lines": [{"id": "l", "start": "l0", "end": "l1"}],
            "ellipses": [ellipse],
            "constraints": constraints,
        }));
        let error = s.solve(&HashMap::new()).unwrap_err();
        assert!(
            error.message.contains(message),
            "{message}: {}",
            error.message
        );
    };
    let base = json!({"id": "e", "center": "c", "major": "a", "minor": "m"});
    let with = |extra: Value| {
        let mut e = base.clone();
        for (k, v) in extra.as_object().unwrap() {
            e[k] = v.clone();
        }
        e
    };
    reject(with(json!({"start": "a"})), json!([]), "both start and end");
    reject(
        with(json!({"start": "a", "end": "a"})),
        json!([]),
        "distinct known ends",
    );
    reject(
        with(json!({"start": "c", "end": "w"})),
        json!([]),
        "distinct known ends",
    );
    reject(
        with(json!({"start": "a", "end": "nope"})),
        json!([]),
        "distinct known ends",
    );
    reject(
        with(json!({"start": "a", "end": "w"})),
        json!([{"line_ellipse_tangent": {"line": "l", "ellipse": "e", "side": "left"}}]),
        "full ellipse",
    );
}
