use super::*;
use serde_json::{Value, json};
use std::f64::consts::PI;

fn mm(value: f64) -> Value {
    json!({"literal": {"value": value, "dimension": "length", "unit": "millimeter"}})
}

fn sketch(points: Value, rest: Value) -> SketchDefinition {
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
        "points": points,
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

fn line(id: &str, start: &str, end: &str) -> Value {
    json!({"id": id, "start": start, "end": end})
}

fn params(pairs: &[(&str, f64)]) -> HashMap<String, ParameterValue> {
    pairs
        .iter()
        .map(|(name, value)| {
            (
                (*name).into(),
                ParameterValue::Scalar(Quantity::length(*value, LengthUnit::Millimeter)),
            )
        })
        .collect()
}

/// Within the solver's residual tolerance, which bounds source positions too.
fn near(a: SketchPoint2, x: f64, y: f64) -> bool {
    (a.x - x).hypot(a.y - y) < 1e-8
}

/// Half outline right of the axis A-T: a free base point, a shoulder and a
/// spline crown meeting the axis at T.
fn half_outline(patterned: bool) -> SketchDefinition {
    let mut lines = vec![
        line("axis", "A", "T"),
        line("AB", "A", "B"),
        line("BC", "B", "C"),
        line("CD", "C", "D"),
    ];
    let mut profile = vec!["AB", "BC", "CD", "crown"];
    if !patterned {
        lines.push(line("TA", "T", "A"));
        profile.push("TA");
    } else {
        profile.extend(["m:1:crown", "m:1:CD", "m:1:BC", "m:1:AB"]);
    }
    let patterns = if patterned {
        json!([{"mirror": {"id": "m", "entities": ["AB", "BC", "CD", "crown"],
            "axis": "axis", "on_axis": ["A", "T"]}}])
    } else {
        json!([])
    };
    sketch(
        json!([
            p("A", 0., 0., true),
            p("B", 12., 1., false),
            p("C", 15., 8., true),
            p("D", 6., 14., true),
            p("E", 3., 19., true),
            p("T", 0., 20., true)
        ]),
        json!({
            "lines": lines,
            "splines": [{"id": "crown", "points": ["D", "E", "T"]}],
            "profile": profile,
            "patterns": patterns,
            "constraints": [
                {"horizontal": {"line": "AB"}},
                {"distance": {"first": "A", "second": "B", "value": {"parameter": "half"}}},
            ],
        }),
    )
}

#[test]
fn mirror_closes_half_outline_into_exactly_double_area() {
    let session = Session::new().unwrap();
    for half in [15., 18.] {
        let parameters = params(&[("half", half)]);
        let full = half_outline(true);
        let solved = full.solve(&parameters).unwrap();
        assert!(solved.solved && solved.free_degrees == 0, "{solved:?}");
        for (id, x, y) in [
            ("m:1:B", -half, 0.),
            ("m:1:C", -15., 8.),
            ("m:1:D", -6., 14.),
            ("m:1:E", -3., 19.),
        ] {
            assert!(
                near(solved.points[id], x, y),
                "{id}: {:?}",
                solved.points[id]
            );
        }
        assert!(!solved.points.contains_key("m:1:A") && !solved.points.contains_key("m:1:T"));
        let face = full.face(&session, &parameters).unwrap();
        let reference = half_outline(false).face(&session, &parameters).unwrap();
        let (area, half_area) = (
            session.surface_area(&face).unwrap(),
            session.surface_area(&reference).unwrap(),
        );
        assert!(
            (area - 2. * half_area).abs() < 1e-7 * area,
            "{area} vs {half_area}"
        );
        let bounds = session.exact_bounds(&face).unwrap();
        assert!((bounds.min.x + bounds.max.x).abs() < 1e-7);
    }
}

#[test]
fn mirrored_arcs_and_explicit_splines_reverse_traversal() {
    let s = sketch(
        json!([
            p("O", 0., 0., true),
            p("U", 0., 10., true),
            p("c", 5., 0., true),
            p("a", 8., 0., true),
            p("b", 5., 3., true),
            p("q0", 1., 1., true),
            p("q1", 2., 3., true),
            p("q2", 4., 2., true)
        ]),
        json!({
            "lines": [line("axis", "O", "U")],
            "arcs": [{"id": "arc", "center": "c", "start": "a", "end": "b", "clockwise": false}],
            "splines": [{"id": "s", "points": ["q0", "q1", "q2"], "basis": {"degree": 2,
                "knots": [0, 1], "multiplicities": [3, 3], "weights": [
                {"literal": {"value": 1, "dimension": "scalar", "unit": null}},
                {"literal": {"value": 2, "dimension": "scalar", "unit": null}},
                {"literal": {"value": 3, "dimension": "scalar", "unit": null}}]}}],
            "patterns": [{"mirror": {"id": "m", "entities": ["arc", "s"], "axis": "axis"}}],
        }),
    );
    let expanded = s.with_pattern_copies(&HashMap::new()).unwrap();
    let arc = expanded.arcs.iter().find(|a| a.id == "m:1:arc").unwrap();
    assert_eq!((arc.start.as_str(), arc.end.as_str()), ("m:1:b", "m:1:a"));
    assert!(!arc.clockwise);
    let spline = expanded.splines.iter().find(|s| s.id == "m:1:s").unwrap();
    assert_eq!(spline.points, ["m:1:q2", "m:1:q1", "m:1:q0"]);
    let weights = &spline.basis.as_ref().unwrap().weights;
    assert_eq!(weights[0], s.splines[0].basis.as_ref().unwrap().weights[2]);
    let solved = s.solve(&HashMap::new()).unwrap();
    assert!(near(solved.points["m:1:a"], -8., 0.) && near(solved.points["m:1:q1"], -2., 3.));
}

/// 60 x 20 plate with one radius-3 hole and a linear row of copies.
fn plate(patterns: Value, holes: Value) -> SketchDefinition {
    sketch(
        json!([
            p("A", 0., 0., true),
            p("B", 60., 0., true),
            p("C", 60., 20., true),
            p("D", 0., 20., true),
            p("h", 10., 10., true),
            p("r", 12., 10., false)
        ]),
        json!({
            "lines": [line("AB", "A", "B"), line("BC", "B", "C"), line("CD", "C", "D"),
                line("DA", "D", "A")],
            "circles": [{"id": "hole", "center": "h", "rim": "r"}],
            "profile": ["AB", "BC", "CD", "DA"],
            "holes": holes,
            "patterns": patterns,
            "constraints": [{"radius": {"curve": "hole", "value": {"parameter": "radius"}}}],
        }),
    )
}

#[test]
fn linear_hole_row_follows_spacing_and_source_radius() {
    let session = Session::new().unwrap();
    let row = plate(
        json!([{"linear": {"id": "row", "entities": ["hole"], "direction": "AB",
            "spacing": {"parameter": "pitch"}, "count": 4}}]),
        json!([["hole"], ["row:1:hole"], ["row:2:hole"], ["row:3:hole"]]),
    );
    let mut names = HashSet::new();
    row.collect_parameters(&mut names);
    assert!(names.contains("pitch") && names.contains("radius"));
    for (pitch, radius) in [(13., 3.), (14., 2.5)] {
        let parameters = params(&[("pitch", pitch), ("radius", radius)]);
        let solved = row.solve(&parameters).unwrap();
        assert!(solved.solved);
        // The free rim angle of the source is the only free degree; copies
        // are fully determined by their links.
        assert_eq!(solved.free_degrees, 1);
        for k in 1..4 {
            let c = solved.points[&format!("row:{k}:h")];
            assert!(near(c, 10. + pitch * f64::from(k), 10.), "{c:?}");
        }
        let face = row.face(&session, &parameters).unwrap();
        let expected = 1200. - 4. * PI * radius * radius;
        assert!((session.surface_area(&face).unwrap() - expected).abs() < 1e-6);
        assert_eq!(session.subshape_count(&face, ShapeType::Wire).unwrap(), 5);
    }
}

#[test]
fn circular_bolt_pattern_spaces_equally_or_by_explicit_step() {
    let session = Session::new().unwrap();
    let flange = |count: u32, angle: Option<f64>| {
        let mut pattern = json!({"id": "bolts", "entities": ["hole"], "center": "O",
            "count": count});
        if let Some(angle) = angle {
            pattern["angle"] =
                json!({"literal": {"value": angle, "dimension": "scalar", "unit": null}});
        }
        let holes = std::iter::once(json!(["hole"]))
            .chain((1..count).map(|k| json!([format!("bolts:{k}:hole")])))
            .collect::<Vec<_>>();
        sketch(
            json!([
                p("O", 0., 0., true),
                p("R", 30., 0., true),
                p("h", 20., 0., true),
                p("r", 23., 0., true)
            ]),
            json!({
                "circles": [{"id": "rim", "center": "O", "rim": "R"},
                    {"id": "hole", "center": "h", "rim": "r"}],
                "profile": ["rim"],
                "holes": holes,
                "patterns": [{"circular": pattern}],
            }),
        )
    };
    let six = flange(6, None);
    let solved = six.solve(&HashMap::new()).unwrap();
    for k in 1..6 {
        let angle = PI / 3. * f64::from(k);
        assert!(near(
            solved.points[&format!("bolts:{k}:h")],
            20. * angle.cos(),
            20. * angle.sin()
        ));
    }
    let area = session
        .surface_area(&six.face(&session, &HashMap::new()).unwrap())
        .unwrap();
    assert!((area - (900. - 54.) * PI).abs() < 1e-6);
    let partial = flange(3, Some(0.5)).solve(&HashMap::new()).unwrap();
    assert!(near(
        partial.points["bolts:2:h"],
        20. * 1f64.cos(),
        20. * 1f64.sin()
    ));
    // Four instances a third of a turn apart put the fourth on the source.
    assert!(
        flange(4, Some(2. * PI / 3.))
            .solve(&HashMap::new())
            .is_err()
    );
}

#[test]
fn later_patterns_copy_earlier_copies() {
    let chained = plate(
        json!([
            {"linear": {"id": "row", "entities": ["hole"], "direction": "AB",
                "spacing": {"parameter": "pitch"}, "count": 2}},
            {"linear": {"id": "column", "entities": ["hole", "row:1:hole"], "direction": "AB",
                "spacing": {"parameter": "pitch"}, "count": 2}},
        ]),
        json!([]),
    );
    let solved = chained
        .solve(&params(&[("pitch", 20.), ("radius", 2.)]))
        .unwrap();
    assert!(near(solved.points["column:1:row:1:h"], 50., 10.));
}

#[test]
fn invalid_patterns_and_holes_fail_with_specific_errors() {
    let parameters = params(&[("pitch", 13.), ("radius", 3.)]);
    let linear = |entities: Value, count: u32, spacing: f64| {
        json!([{"linear": {"id": "row", "entities": entities, "direction": "AB",
            "spacing": mm(spacing), "count": count}}])
    };
    let cases = [
        (linear(json!(["nope"]), 2, 5.), json!([]), "unknown entity"),
        (
            linear(json!(["hole"]), 1, 5.),
            json!([]),
            "count must be 2-1000",
        ),
        (linear(json!(["hole"]), 2, 0.), json!([]), "nonzero length"),
        (
            linear(json!(["AB"]), 2, 5.),
            json!([]),
            "own reference line",
        ),
        (
            json!([{"circular": {"id": "c", "entities": ["hole"], "center": "h", "count": 2}}]),
            json!([]),
            "own center",
        ),
        (
            json!([{"mirror": {"id": "a:b", "entities": ["hole"], "axis": "DA"}}]),
            json!([]),
            "free of ':'",
        ),
        (
            json!([{"mirror": {"id": "m", "entities": ["hole"], "axis": "DA", "on_axis": ["A"]}}]),
            json!([]),
            "on-axis point",
        ),
        (
            json!([{"mirror": {"id": "m", "entities": ["AB"], "axis": "DA"}}]),
            json!([]),
            "listed on_axis",
        ),
        (json!([]), json!([["AB"]]), "reuse profile"),
        (json!([]), json!([[]]), "nonempty"),
    ];
    for (patterns, holes, message) in cases {
        let result = plate(patterns, holes).solve(&parameters);
        let error = result.expect_err(message);
        assert!(
            error.message.contains(message),
            "{message}: {}",
            error.message
        );
    }
    let mut collide = plate(
        json!([{"linear": {"id": "row", "entities": ["hole", "AB"], "direction": "CD",
            "spacing": mm(5.), "count": 2}}]),
        json!([]),
    );
    collide.lines.push(SketchLine {
        id: "row:1:hole".into(),
        start: "A".into(),
        end: "B".into(),
    });
    let error = collide.solve(&parameters).unwrap_err();
    assert!(error.message.contains("collide"), "{}", error.message);
    let session = Session::new().unwrap();
    let wire = plate(json!([]), json!([["hole"]])).wire(&session, &parameters, None);
    assert!(wire.unwrap_err().message.contains("only to face outputs"));
}
