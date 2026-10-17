//! Curve edits on a sketch plane other than XY: crossings are found on
//! sketch-local curves, which are then placed in the plane, so a turned
//! sketch costs a few rigid transforms per curve over an XY one.

use super::*;
use serde_json::{Value, json};

const LINES: usize = 10_000;
const EXTENSIONS: usize = 1_000;

/// A 100 × (LINES / 10) mm rectangle outline with `LINES` short spurs
/// inside it, the first `EXTENSIONS` of them extended to the left edge, in
/// the frame (`x`, `y`) through `origin`.
fn family(origin: [f64; 3], x: [f64; 3], y: [f64; 3]) -> Result<FamilyDefinition, ModelError> {
    let mm = |v: f64| json!({"literal": {"value": v, "dimension": "length", "unit": "millimeter"}});
    let point =
        |id: String, x: f64, y: f64| json!({"id": id, "x": mm(x), "y": mm(y), "fixed": true});
    let vector = |v: [f64; 3], dimension: &str, unit: Value| {
        json!({"literal": {"x": {"value": v[0], "dimension": dimension, "unit": unit},
            "y": {"value": v[1], "dimension": dimension, "unit": unit},
            "z": {"value": v[2], "dimension": dimension, "unit": unit}}})
    };
    let height = (LINES / 10) as f64;
    let mut points = vec![
        point("a".into(), 0., 0.),
        point("b".into(), 100., 0.),
        point("c".into(), 100., height),
        point("d".into(), 0., height),
    ];
    let mut lines = vec![
        json!({"id": "bottom", "start": "a", "end": "b"}),
        json!({"id": "right", "start": "b", "end": "c"}),
        json!({"id": "top", "start": "c", "end": "d"}),
        json!({"id": "left", "start": "d", "end": "a"}),
    ];
    let mut operations = Vec::new();
    for i in 0..LINES {
        let (column, row) = ((i % 10) as f64, (i / 10) as f64);
        let (x0, y0) = (5. + 9. * column, 0.5 + row);
        points.push(point(format!("s{i}"), x0 + 4., y0));
        points.push(point(format!("t{i}"), x0, y0));
        lines.push(
            json!({"id": format!("spur{i}"), "start": format!("s{i}"), "end": format!("t{i}")}),
        );
        if i < EXTENSIONS {
            operations.push(
                json!({"extend_to": {"entity": format!("spur{i}"), "end": "end",
                "target": "left", "limit": mm(100.)}}),
            );
        }
    }
    let sketch: SketchDefinition = serde_json::from_value(json!({
        "id": "edits", "origin": vector(origin, "length", json!("millimeter")),
        "x_axis": vector(x, "scalar", Value::Null), "y_axis": vector(y, "scalar", Value::Null),
        "points": points, "lines": lines, "profile": ["bottom", "right", "top", "left"],
        "curve_operations": operations,
    }))
    .map_err(|error| failure(error.to_string()))?;
    let mut definition = block();
    definition.features = vec![FeatureDefinition {
        id: "outline".into(),
        operation: FeatureOperation::SketchFace {
            sketch: Box::new(sketch),
        },
    }];
    definition.requirements.clear();
    Ok(definition)
}

fn regenerate(definition: &FamilyDefinition) -> Result<(f64, Duration), ModelError> {
    let session = Session::new()?;
    let part = PartInstance {
        id: "part".into(),
        definition,
        overrides: HashMap::new(),
        provenance: "bench".into(),
    };
    let start = Instant::now();
    let generated = part.regenerate(&session)?;
    let face = generated
        .shape("outline")
        .ok_or_else(|| failure("outline face missing".into()))?;
    let area = session.surface_area(face)?;
    Ok((area, start.elapsed()))
}

/// 10,000 curves with 1,000 extend-to edits, on a tilted plane and on XY:
/// the same face, with placement adding O(curves) rigid transforms (about
/// 0.1 s for XY and 0.13 s tilted; 0.5 s for the case with setup).
pub(crate) fn turned_plane_case() -> Outcome {
    timed(
        format!("curve edits: {LINES} curves, {EXTENSIONS} extend-to edits on a tilted plane"),
        Duration::from_secs(2),
        Expectation::Required,
        || {
            let s = 0.5f64.sqrt();
            let (flat, flat_time) = regenerate(&family([0.; 3], [1., 0., 0.], [0., 1., 0.])?)?;
            let (turned, turned_time) =
                regenerate(&family([4., -2., 7.], [s, 0., -s], [0.5, s, 0.5])?)?;
            let expected = 100. * (LINES / 10) as f64;
            if (flat - expected).abs() > 1e-6 || (turned - expected).abs() > 1e-6 {
                return Err(failure(format!(
                    "outline areas {flat} and {turned}, expected {expected}"
                )));
            }
            Ok(format!(
                "same {expected:.0} mm² face; XY {:.2} s, tilted {:.2} s",
                flat_time.as_secs_f64(),
                turned_time.as_secs_f64()
            ))
        },
    )
}
