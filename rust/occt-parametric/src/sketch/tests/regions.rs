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
