//! Curve, fill and table patterns at 10,000 copies (schema 184).
use super::*;
use serde_json::{Value, json};

fn lengths(x: f64, y: f64, z: f64) -> Value {
    let q = |v: f64| json!({"value": v, "dimension": "length", "unit": "millimeter"});
    json!({"literal": {"x": q(x), "y": q(y), "z": q(z)}})
}
fn scalars(x: f64, y: f64, z: f64) -> Value {
    let q = |v: f64| json!({"value": v, "dimension": "scalar", "unit": null});
    json!({"literal": {"x": q(x), "y": q(y), "z": q(z)}})
}
fn quantity(v: f64, dimension: &str) -> Value {
    json!({"literal": {"value": v, "dimension": dimension, "unit": if dimension == "length" { json!("millimeter") } else { Value::Null }}})
}

fn family(features: Value) -> FamilyDefinition {
    serde_json::from_value(json!({"id": "patterns", "version": 1, "parameters": [], "derived_parameters": [],
        "derived_vector_parameters": [], "constraints": [], "requirements": [], "datums": [], "features": features}))
        .unwrap()
}

pub(super) fn pattern_types_case() -> Outcome {
    let pin = json!({"id": "pin", "operation": {"box": {"origin": lengths(0.0, 0.0, 0.0), "size": lengths(1.0, 1.0, 1.0)}}});
    // A 1,010 mm square region with a Ø200 hole: 10,000 sites at 10 mm
    // with a 2 mm margin, less those by the hole.
    let point = |id: &str, x: f64, y: f64| json!({"id": id, "x": quantity(x, "length"), "y": quantity(y, "length"), "fixed": true});
    let region = json!({"id": "region", "operation": {"sketch_face": {"sketch": {
        "id": "s", "origin": lengths(0.0, 0.0, 0.0), "x_axis": scalars(1.0, 0.0, 0.0), "y_axis": scalars(0.0, 1.0, 0.0),
        "points": [point("a", 0.0, 0.0), point("b", 1000.0, 0.0), point("c", 1000.0, 1000.0), point("d", 0.0, 1000.0),
                   point("o", 500.0, 500.0), point("r", 600.0, 500.0)],
        "lines": [{"id": "ab", "start": "a", "end": "b"}, {"id": "bc", "start": "b", "end": "c"},
                  {"id": "cd", "start": "c", "end": "d"}, {"id": "da", "start": "d", "end": "a"}],
        "circles": [{"id": "hole", "center": "o", "rim": "r"}],
        "profile": ["ab", "bc", "cd", "da"], "holes": [["hole"]], "constraints": []}}}});
    let fill = family(
        json!([region, pin, {"id": "copies", "operation": {"fill_pattern": {
        "input": "pin", "region": "region", "anchor": lengths(5.0, 5.0, 0.0), "direction": scalars(1.0, 0.0, 0.0),
        "spacing": quantity(10.0, "length"), "row_spacing": quantity(10.0, "length"), "margin": quantity(2.0, "length")}}}]),
    );
    // A full circle of radius 1,000 carrying 1,000 copies that turn with it.
    let circle = json!({"id": "loop", "operation": {"sketch_wire": {"sketch": {
        "id": "s", "origin": lengths(0.0, 0.0, 0.0), "x_axis": scalars(1.0, 0.0, 0.0), "y_axis": scalars(0.0, 1.0, 0.0),
        "points": [point("o", 0.0, 0.0), point("r", 1000.0, 0.0)], "lines": [],
        "circles": [{"id": "c", "center": "o", "rim": "r"}], "profile": ["c"], "constraints": []}}}});
    let ring = json!({"id": "pin", "operation": {"box": {"origin": lengths(999.5, -0.5, 0.0), "size": lengths(1.0, 1.0, 1.0)}}});
    let curve = family(
        json!([circle, ring, {"id": "copies", "operation": {"curve_pattern": {
        "input": "pin", "path": "loop", "count": quantity(1000.0, "scalar"), "orientation": "tangent"}}}]),
    );
    let offsets: Vec<Value> = (0..10_000)
        .map(|i| {
            lengths(
                (i % 100) as f64 * 3.0,
                (i / 100) as f64 * 3.0 + (i % 7) as f64 * 0.1,
                0.0,
            )
        })
        .collect();
    let table = family(
        json!([pin, {"id": "copies", "operation": {"table_pattern": {"input": "pin", "offsets": offsets}}}]),
    );
    let expected_fill = (0..100)
        .flat_map(|j| (0..100).map(move |i| (5.0 + 10.0 * i as f64, 5.0 + 10.0 * j as f64)))
        .filter(|&(x, y)| (x - 500.0f64).hypot(y - 500.0) >= 102.0)
        .count();
    timed(
        "pattern types: 10,000-site fill, 1,000 tangent curve copies, 10,000 table offsets".into(),
        ms(60_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let mut timings = Vec::new();
            for (name, definition, expected) in [
                ("fill", &fill, expected_fill),
                ("curve", &curve, 1000),
                ("table", &table, 10_000),
            ] {
                let started = Instant::now();
                let part = PartInstance {
                    id: "p".into(),
                    definition,
                    overrides: HashMap::new(),
                    provenance: "bench".into(),
                };
                let generated = part.regenerate(&session)?;
                let solids =
                    session.subshape_count(generated.shape("copies").unwrap(), ShapeType::Solid)?;
                if solids != expected {
                    return Err(failure(format!(
                        "{name} pattern made {solids} copies, expected {expected}"
                    )));
                }
                drop(generated);
                timings.push(format!(
                    "{name} {solids} in {:.2} s",
                    started.elapsed().as_secs_f64()
                ));
            }
            if session.shape_count()? != 0 {
                return Err(failure("pattern handles retained".into()));
            }
            Ok(timings.join(", "))
        },
    )
}
