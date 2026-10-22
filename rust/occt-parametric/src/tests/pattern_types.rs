//! Curve, fill and table patterns, and skipped copies (schema 184), checked
//! against positions computed independently from the exact geometry.

use super::*;
use occt_bridge::ShapeType;
use serde_json::{Value, json};
use std::f64::consts::PI;

fn mm(v: f64) -> Value {
    json!({"literal": {"value": v, "dimension": "length", "unit": "millimeter"}})
}
fn lengths(x: f64, y: f64, z: f64) -> Value {
    let q = |v: f64| json!({"value": v, "dimension": "length", "unit": "millimeter"});
    json!({"literal": {"x": q(x), "y": q(y), "z": q(z)}})
}
fn scalars(x: f64, y: f64, z: f64) -> Value {
    let q = |v: f64| json!({"value": v, "dimension": "scalar", "unit": null});
    json!({"literal": {"x": q(x), "y": q(y), "z": q(z)}})
}
fn count(n: f64) -> Value {
    json!({"literal": {"value": n, "dimension": "scalar", "unit": null}})
}
fn sketch(points: Value, extra: Value) -> Value {
    let mut sketch = json!({"id": "s", "origin": lengths(0.0, 0.0, 0.0), "x_axis": scalars(1.0, 0.0, 0.0),
        "y_axis": scalars(0.0, 1.0, 0.0), "points": points, "lines": [], "constraints": []});
    sketch
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    sketch
}
fn point(id: &str, x: f64, y: f64) -> Value {
    json!({"id": id, "x": mm(x), "y": mm(y), "fixed": true})
}
fn family(features: Value) -> FamilyDefinition {
    serde_json::from_value(json!({"id": "patterns", "version": 1, "parameters": [], "derived_parameters": [],
        "derived_vector_parameters": [], "constraints": [], "requirements": [], "datums": [], "features": features}))
        .unwrap()
}

/// A copy's center and bounding-box extent.
type Placed = ([f64; 3], [f64; 3]);

/// Each copy of the last feature.
fn copies(definition: &FamilyDefinition) -> Result<Vec<Placed>, ModelError> {
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "p".into(),
        definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let last = &definition.features.last().unwrap().id;
    let result = part.regenerate(&session).map(|generated| {
        session
            .subshapes(generated.shape(last).unwrap(), ShapeType::Solid)
            .unwrap()
            .iter()
            .map(|solid| {
                let c = session.center_of_mass(solid).unwrap();
                let b = session.exact_bounds(solid).unwrap();
                (
                    [c.x, c.y, c.z],
                    [b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z],
                )
            })
            .collect()
    });
    assert_eq!(session.shape_count().unwrap(), 0, "handles released");
    result
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn linear_and_circular_patterns_skip_copies() {
    let block = json!({"id": "block", "operation": {"box": {"origin": lengths(0.0, 0.0, 0.0), "size": lengths(2.0, 2.0, 2.0)}}});
    let linear = |skip: Value| {
        family(
            json!([block, {"id": "row", "operation": {"linear_pattern": {
        "input": "block", "step": lengths(10.0, 0.0, 0.0), "count": count(5.0), "skip": skip}}}]),
        )
    };
    let kept = copies(&linear(json!([1, 3]))).unwrap();
    let xs: Vec<f64> = kept.iter().map(|(c, _)| c[0]).collect();
    assert_eq!(xs, [1.0, 21.0, 41.0]);
    assert!(
        copies(&linear(json!([5])))
            .unwrap_err()
            .message
            .contains("copies run 0 to 4")
    );
    assert!(
        copies(&linear(json!([0, 1, 2, 3, 4])))
            .unwrap_err()
            .message
            .contains("every copy")
    );
    let ring = family(
        json!([block, {"id": "ring", "operation": {"circular_pattern": {
        "input": "block", "origin": lengths(-20.0, 1.0, 0.0), "axis": scalars(0.0, 0.0, 1.0), "count": count(4.0),
        "angle_step_radians": {"literal": {"value": PI / 2.0, "dimension": "scalar", "unit": null}}, "skip": [2]}}}]),
    );
    assert_eq!(copies(&ring).unwrap().len(), 3);
}

#[test]
fn curve_patterns_follow_open_and_closed_paths() {
    // A semicircle of radius 50 from (50, 0) over the top to (-50, 0), and a
    // bar 6 mm long along the path's starting tangent (+Y), centered there.
    let arc = json!({"id": "path", "operation": {"sketch_open_wire": {"sketch": sketch(
        json!([point("c", 0.0, 0.0), point("s", 50.0, 0.0), point("e", -50.0, 0.0)]),
        json!({"arcs": [{"id": "a", "center": "c", "start": "s", "end": "e"}], "profile": ["a"]}))}}});
    let bar = json!({"id": "bar", "operation": {"box": {"origin": lengths(49.0, -3.0, -1.0), "size": lengths(2.0, 6.0, 2.0)}}});
    let along = |orientation: &str, skip: Value| {
        family(
            json!([arc, bar, {"id": "copies", "operation": {"curve_pattern": {
            "input": "bar", "path": "path", "count": count(5.0), "orientation": orientation, "skip": skip}}}]),
        )
    };
    let fixed = copies(&along("fixed", json!([]))).unwrap();
    assert_eq!(fixed.len(), 5);
    for (i, (center, extent)) in fixed.iter().enumerate() {
        let angle = PI * i as f64 / 4.0;
        assert!(
            near(center[0], 50.0 * angle.cos()) && near(center[1], 50.0 * angle.sin()),
            "{i}: {center:?}"
        );
        assert!(near(extent[1], 6.0), "fixed copies keep their orientation");
    }
    // Turning with the path: at the top the bar lies along X.
    let turned = copies(&along("tangent", json!([0, 1, 3, 4]))).unwrap();
    let (center, extent) = turned[0];
    assert!(near(center[0], 0.0) && near(center[1], 50.0), "{center:?}");
    assert!(near(extent[0], 6.0) && near(extent[1], 2.0), "{extent:?}");
    // A closed circle: six copies, a sixth of a turn apart, none doubled.
    let circle = json!({"id": "loop", "operation": {"sketch_wire": {"sketch": sketch(
        json!([point("c", 0.0, 0.0), point("r", 30.0, 0.0)]),
        json!({"circles": [{"id": "o", "center": "c", "rim": "r"}], "profile": ["o"]}))}}});
    let dot = json!({"id": "dot", "operation": {"sphere": {"center": lengths(30.0, 0.0, 0.0), "radius": mm(1.0)}}});
    let around = family(
        json!([circle, dot, {"id": "copies", "operation": {"curve_pattern": {
        "input": "dot", "path": "loop", "count": count(6.0)}}}]),
    );
    let mut angles: Vec<f64> = copies(&around)
        .unwrap()
        .iter()
        .map(|(c, _)| {
            let angle = c[1].atan2(c[0]).rem_euclid(2.0 * PI);
            // The copy at 0° may sit a rounding error below it.
            if angle > 2.0 * PI - 1e-9 { 0.0 } else { angle }
        })
        .collect();
    angles.sort_by(f64::total_cmp);
    for (i, angle) in angles.iter().enumerate() {
        assert!(near(*angle, PI / 3.0 * i as f64), "{angles:?}");
    }
    // A solid is not a path.
    let bad = family(
        json!([bar, {"id": "copies", "operation": {"curve_pattern": {
        "input": "bar", "path": "bar", "count": count(3.0)}}}]),
    );
    assert!(
        copies(&bad)
            .unwrap_err()
            .message
            .contains("one edge or wire")
    );
}

/// Sites of a grid through (5, 5) at 10 mm, inside the 100 × 60 rectangle
/// less the Ø20 hole at (50, 30), at least `margin` from both.
fn expected_sites(staggered: bool, margin: f64) -> Vec<(i64, i64)> {
    let mut sites = Vec::new();
    for j in -10..20 {
        for i in -10..20 {
            let x = 5.0 + 10.0 * i as f64 + if staggered && j % 2 != 0 { 5.0 } else { 0.0 };
            let y = 5.0 + 10.0 * j as f64;
            let edge = x.min(100.0 - x).min(y).min(60.0 - y);
            let hole = (x - 50.0).hypot(y - 30.0) - 10.0;
            if edge >= margin - 1e-9 && hole >= margin - 1e-9 && edge > 0.0 && hole > 0.0 {
                sites.push(((x * 1000.0).round() as i64, (y * 1000.0).round() as i64));
            }
        }
    }
    sites.sort();
    sites
}

fn fill_family(layout: &str, margin: f64) -> FamilyDefinition {
    let region = json!({"id": "region", "operation": {"sketch_face": {"sketch": sketch(
        json!([point("a", 0.0, 0.0), point("b", 100.0, 0.0), point("c", 100.0, 60.0), point("d", 0.0, 60.0),
               point("o", 50.0, 30.0), point("r", 60.0, 30.0)]),
        json!({"lines": [{"id": "ab", "start": "a", "end": "b"}, {"id": "bc", "start": "b", "end": "c"},
                         {"id": "cd", "start": "c", "end": "d"}, {"id": "da", "start": "d", "end": "a"}],
               "circles": [{"id": "hole", "center": "o", "rim": "r"}],
               "profile": ["ab", "bc", "cd", "da"], "holes": [["hole"]]}))}}});
    let pin = json!({"id": "pin", "operation": {"cylinder": {"origin": lengths(5.0, 5.0, -1.0), "axis": scalars(0.0, 0.0, 1.0),
        "radius": mm(1.0), "height": mm(2.0)}}});
    family(
        json!([region, pin, {"id": "pins", "operation": {"fill_pattern": {
        "input": "pin", "region": "region", "anchor": lengths(5.0, 5.0, 0.0), "direction": scalars(1.0, 0.0, 0.0),
        "spacing": mm(10.0), "row_spacing": mm(10.0), "layout": layout, "margin": mm(margin)}}}]),
    )
}

fn sites(definition: &FamilyDefinition) -> Vec<(i64, i64)> {
    let mut found: Vec<(i64, i64)> = copies(definition)
        .unwrap()
        .iter()
        .map(|(c, _)| {
            (
                (c[0] * 1000.0).round() as i64,
                (c[1] * 1000.0).round() as i64,
            )
        })
        .collect();
    found.sort();
    found
}

#[test]
fn fill_patterns_keep_sites_inside_the_region_and_clear_of_its_edges() {
    for (layout, margin) in [
        ("grid", 4.0),
        ("grid", 5.0),
        ("staggered", 3.0),
        ("grid", 0.0),
        ("staggered", 7.5),
    ] {
        let expected = expected_sites(layout == "staggered", margin);
        assert_eq!(
            sites(&fill_family(layout, margin)),
            expected,
            "{layout} {margin}"
        );
    }
    // 4 mm clear of everything: the four sites by the hole go.
    assert_eq!(expected_sites(false, 4.0).len(), 56);
    // A margin exactly reaching the outer sites keeps them (5 mm from x = 0).
    assert!(expected_sites(false, 5.0).contains(&(5000, 5000)));
}

#[test]
fn fill_patterns_use_a_selected_face_of_a_solid() {
    let plate = json!({"id": "plate", "operation": {"box": {"origin": lengths(0.0, 0.0, 0.0), "size": lengths(100.0, 60.0, 5.0)}}});
    let pin = json!({"id": "pin", "operation": {"cylinder": {"origin": lengths(5.0, 5.0, 4.0), "axis": scalars(0.0, 0.0, 1.0),
        "radius": mm(1.0), "height": mm(2.0)}}});
    let top = json!({"at_extreme": {"axis": "z", "extremum": "maximum", "tolerance": mm(1e-6)}});
    let definition = family(
        json!([plate, pin, {"id": "pins", "operation": {"fill_pattern": {
        "input": "pin", "region": "plate", "face": top, "anchor": lengths(5.0, 5.0, 5.0), "direction": scalars(0.0, 1.0, 0.0),
        "spacing": mm(10.0), "row_spacing": mm(20.0), "margin": mm(5.0)}}}]),
    );
    // Sites every 10 mm along Y (y = 5..55); the grid's second axis is
    // normal × direction, -X, with rows 20 mm apart through x = 5, so the
    // rows that fit the 5 mm margin are x = 5, 25, 45, 65 and 85.
    let found = sites(&definition);
    let xs: std::collections::BTreeSet<i64> = found.iter().map(|s| s.0).collect();
    let ys: std::collections::BTreeSet<i64> = found.iter().map(|s| s.1).collect();
    assert_eq!(
        xs.into_iter().collect::<Vec<_>>(),
        [5000, 25000, 45000, 65000, 85000]
    );
    assert_eq!(
        ys.into_iter().collect::<Vec<_>>(),
        [5000, 15000, 25000, 35000, 45000, 55000]
    );
    assert_eq!(found.len(), 30);
    // Without a selector a solid has too many faces.
    let mut unselected = definition.clone();
    if let FeatureOperation::FillPattern { face, .. } = &mut unselected.features[2].operation {
        *face = None;
    }
    assert!(copies(&unselected).unwrap_err().message.contains("6 faces"));
}

#[test]
fn table_patterns_place_listed_offsets_and_bad_grids_are_refused() {
    let block = json!({"id": "block", "operation": {"box": {"origin": lengths(0.0, 0.0, 0.0), "size": lengths(2.0, 2.0, 2.0)}}});
    let table = family(
        json!([block, {"id": "copies", "operation": {"table_pattern": {
        "input": "block", "offsets": [lengths(0.0, 0.0, 0.0), lengths(7.0, 0.0, 0.0), lengths(19.0, 3.0, 0.0)]}}}]),
    );
    let centers: Vec<[f64; 3]> = copies(&table)
        .unwrap()
        .iter()
        .map(|(c, _)| c.to_owned())
        .collect();
    assert_eq!(
        centers,
        [[1.0, 1.0, 1.0], [8.0, 1.0, 1.0], [20.0, 4.0, 1.0]]
    );
    // A grid far finer than the region is refused before any copy is made.
    let mut fine = fill_family("grid", 0.0);
    if let FeatureOperation::FillPattern {
        spacing,
        row_spacing,
        ..
    } = &mut fine.features[2].operation
    {
        *spacing = ScalarExpr::Literal(Quantity::length(0.01, LengthUnit::Millimeter));
        *row_spacing = spacing.clone();
    }
    assert!(
        copies(&fine)
            .unwrap_err()
            .message
            .contains("increase the spacing")
    );
}
