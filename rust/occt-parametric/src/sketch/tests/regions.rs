//! Several disjoint face regions in one sketch (schema 127).

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

fn p(id: &str, x: f64, y: f64) -> Value {
    json!({"id": id, "x": mm(x), "y": mm(y), "fixed": true})
}

/// A 20 × 10 rectangle, a disc of radius 4 with a 1.5 hole beside it, and a
/// slot region; `offset` moves the disc toward the rectangle.
fn three_regions(offset: f64) -> SketchDefinition {
    sketch(json!({
        "points": [p("a", 0., 0.), p("b", 20., 10.), p("c", 30. - offset, 5.), p("r", 34. - offset, 5.),
            p("hr", 31.5 - offset, 5.), p("s0", 45., 5.), p("s1", 55., 5.)],
        "circles": [{"id": "disc", "center": "c", "rim": "r"}, {"id": "bore", "center": "c", "rim": "hr"}],
        "shapes": [{"rectangle": {"id": "plate", "first": "a", "opposite": "b"}},
            {"slot": {"id": "slot", "start": "s0", "end": "s1", "radius": mm(2.)}}],
        "profile": ["plate"],
        "regions": [{"profile": ["disc"], "holes": [["bore"]]}, {"profile": ["slot"]}],
    }))
}

fn faces(sketch: &SketchDefinition) -> Result<(usize, f64), ModelError> {
    let session = Session::new().unwrap();
    let face = sketch.face(&session, &HashMap::new())?;
    assert!(session.is_valid(&face).unwrap());
    Ok((
        session.subshape_count(&face, ShapeType::Face).unwrap(),
        session.surface_area(&face).unwrap(),
    ))
}

#[test]
fn automatic_regions_split_intersections_and_select_named_seed_cells() {
    let s = sketch(json!({
        "points": [p("c", 0., 0.), p("r", 10., 0.), p("l", -12., 0.), p("h", 12., 0.), p("pick", 0., 5.)],
        "circles": [{"id": "disc", "center": "c", "rim": "r"}],
        "lines": [{"id": "divider", "start": "l", "end": "h"}],
        "region_discovery": {"seeds": ["pick", "pick"]}
    }));
    let (count, area) = faces(&s).unwrap();
    assert_eq!(count, 1);
    assert!((area - 50. * PI).abs() < 1e-7);
    let session = Session::new().unwrap();
    let solution = s.solve(&HashMap::new()).unwrap();
    let preview = s
        .preview_edited_profile(&session, &HashMap::new(), &solution, 32, true)
        .unwrap();
    assert!(!preview.is_empty());
    assert!(preview.iter().flatten().all(|p| p.y >= -1e-7));
    let mut all = s;
    all.region_discovery.as_mut().unwrap().seeds.clear();
    let (count, area) = faces(&all).unwrap();
    assert_eq!(count, 2);
    assert!((area - 100. * PI).abs() < 1e-7);
}

#[test]
fn automatic_regions_handle_nested_loops_and_overlapping_circles() {
    let nested = sketch(json!({
        "points": [p("c", 0., 0.), p("r", 10., 0.), p("b", 3., 0.), p("pick", 5., 0.)],
        "circles": [{"id": "disc", "center": "c", "rim": "r"}, {"id": "hole", "center": "c", "rim": "b"}],
        "region_discovery": {"seeds": ["pick"]}
    }));
    let (count, area) = faces(&nested).unwrap();
    assert_eq!(count, 1);
    assert!((area - 91. * PI).abs() < 1e-7);
    let lens = sketch(json!({
        "points": [p("c0", 0., 0.), p("r0", 0., 10.), p("c1", 10., 0.), p("r1", 10., 10.), p("pick", 5., 0.)],
        "circles": [{"id": "left", "center": "c0", "rim": "r0"}, {"id": "right", "center": "c1", "rim": "r1"}],
        "region_discovery": {"seeds": ["pick"]}
    }));
    let (_, area) = faces(&lens).unwrap();
    assert!((area - (200. * PI / 3. - 50. * 3_f64.sqrt())).abs() < 1e-7);
}

#[test]
fn automatic_regions_expand_shapes_and_use_native_ellipse_and_spline_edges() {
    let rectangles = sketch(json!({
        "points": [p("a", 0., 0.), p("b", 20., 10.), p("c", 30., 0.), p("d", 40., 10.)],
        "shapes": [{"rectangle": {"id": "first", "first": "a", "opposite": "b"}},
            {"rectangle": {"id": "second", "first": "c", "opposite": "d"}}],
        "region_discovery": {"entities": ["first", "second"]}
    }));
    let (count, area) = faces(&rectangles).unwrap();
    assert_eq!(count, 2);
    assert!((area - 300.).abs() < 1e-7);
    let ellipse = sketch(json!({
        "points": [p("c", 0., 0.), p("a", 10., 0.), p("b", 0., 4.), p("l", -12., 0.), p("r", 12., 0.), p("pick", 0., 2.)],
        "ellipses": [{"id": "e", "center": "c", "major": "a", "minor": "b"}],
        "lines": [{"id": "line", "start": "l", "end": "r"}],
        "region_discovery": {"seeds": ["pick"]}
    }));
    assert!((faces(&ellipse).unwrap().1 - 20. * PI).abs() < 1e-7);
    let spline = sketch(json!({
        "points": [p("a", 0., 0.), p("b", 5., 4.), p("c", 10., 0.), p("l", -2., 0.), p("r", 12., 0.)],
        "splines": [{"id": "arch", "points": ["a", "b", "c"]}],
        "lines": [{"id": "line", "start": "l", "end": "r"}],
        "region_discovery": {}
    }));
    let (count, area) = faces(&spline).unwrap();
    assert_eq!(count, 1);
    assert!(area > 20. && area < 40.);
}

#[test]
fn automatic_region_errors_reject_ambiguous_requests_and_bad_seeds() {
    let base = |seed: (f64, f64)| {
        sketch(json!({
            "points": [p("c", 0., 0.), p("r", 10., 0.), p("pick", seed.0, seed.1)],
            "circles": [{"id": "disc", "center": "c", "rim": "r"}],
            "region_discovery": {"seeds": ["pick"]}
        }))
    };
    assert!(
        faces(&base((10., 0.)))
            .unwrap_err()
            .message
            .contains("boundary")
    );
    assert!(
        faces(&base((20., 0.)))
            .unwrap_err()
            .message
            .contains("outside")
    );
    let mut s = base((0., 0.));
    s.profile = vec!["disc".into()];
    assert!(
        faces(&s)
            .unwrap_err()
            .message
            .contains("mutually exclusive")
    );
    s.profile.clear();
    s.region_discovery.as_mut().unwrap().entities = vec!["missing".into()];
    assert!(
        faces(&s)
            .unwrap_err()
            .message
            .contains("unknown or repeated")
    );
    s.region_discovery.as_mut().unwrap().entities.clear();
    s.region_discovery.as_mut().unwrap().seeds = vec!["missing".into()];
    assert!(faces(&s).unwrap_err().message.contains("unknown seed"));
    s.region_discovery.as_mut().unwrap().seeds.clear();
    s.region_discovery.as_mut().unwrap().seeds = vec!["pick".into(); 1001];
    assert!(faces(&s).unwrap_err().message.contains("exceeds 1000"));
    s.region_discovery.as_mut().unwrap().seeds.clear();
    let session = Session::new().unwrap();
    assert!(
        s.wire(&session, &HashMap::new(), None)
            .unwrap_err()
            .message
            .contains("only to face outputs")
    );
    let open = sketch(json!({"points": [p("a", 0., 0.), p("b", 10., 0.)],
        "lines": [{"id": "line", "start": "a", "end": "b"}], "region_discovery": {}}));
    assert!(
        faces(&open)
            .unwrap_err()
            .message
            .contains("no bounded cells")
    );
}

#[test]
fn disjoint_regions_build_one_compound_face_with_their_own_holes() {
    let (count, area) = faces(&three_regions(0.)).unwrap();
    assert_eq!(count, 3);
    let expected = 200. + PI * (16. - 2.25) + (2. * 2. * 10. + PI * 4.);
    assert!((area - expected).abs() < 1e-9, "{area} vs {expected}");
}

#[test]
fn overlapping_regions_and_misused_loops_are_rejected() {
    // The disc (radius 4 at x = 30 − 7) overlaps the rectangle ending at 20.
    let error = faces(&three_regions(7.)).unwrap_err();
    assert!(
        error.message.contains("profile and region 0 overlap"),
        "{}",
        error.message
    );
    // Touching at a single point is not an overlap.
    let (count, _) = faces(&three_regions(6.)).unwrap();
    assert_eq!(count, 3);
    // Wire outputs reject regions.
    let session = Session::new().unwrap();
    let error = three_regions(0.)
        .wire(&session, &HashMap::new(), None)
        .unwrap_err();
    assert!(error.message.contains("regions apply only to face outputs"));
    // An entity may belong to one loop only.
    let mut reused = three_regions(0.);
    reused.regions[1].profile = vec!["disc".into()];
    let error = reused.solve(&HashMap::new()).unwrap_err();
    assert!(
        error.message.contains("reuse entities"),
        "{}",
        error.message
    );
    let mut empty = three_regions(0.);
    empty.regions[0].profile.clear();
    let error = empty.solve(&HashMap::new()).unwrap_err();
    assert!(error.message.contains("nonempty"), "{}", error.message);
}

#[test]
fn profile_operations_apply_to_the_profile_beside_regions() {
    let mut s = three_regions(0.);
    s.profile_operations =
        serde_json::from_value(json!([{"fillet": {"point": "a", "radius": mm(2.)}}])).unwrap();
    let (count, area) = faces(&s).unwrap();
    assert_eq!(count, 3);
    // One rounded corner removes (1 − π/4) r² from the rectangle.
    let expected = 200. - (1. - PI / 4.) * 4. + PI * (16. - 2.25) + (40. + PI * 4.);
    assert!((area - expected).abs() < 1e-9, "{area} vs {expected}");
}
