//! Fastened joints at 1,000 screws (schema 186): measuring, hardware and
//! both parts' holes.
use super::*;
use serde_json::{Value, json};

fn lengths(x: f64, y: f64, z: f64) -> Value {
    let q = |v: f64| json!({"value": v, "dimension": "length", "unit": "millimeter"});
    json!({"literal": {"x": q(x), "y": q(y), "z": q(z)}})
}

pub(super) fn fastened_joint_case() -> Outcome {
    // 1,000 M5 screws on a 24 mm grid through a 6 mm lid, tapped into a
    // 20 mm base, with washers under the heads.
    let positions: Vec<Value> = (0..1000)
        .map(|i| {
            lengths(
                12.0 + 24.0 * (i % 40) as f64,
                12.0 + 24.0 * (i / 40) as f64,
                0.0,
            )
        })
        .collect();
    let definition: FamilyDefinition = serde_json::from_value(json!({"id": "joint", "version": 1,
        "parameters": [], "derived_parameters": [], "derived_vector_parameters": [], "constraints": [],
        "requirements": [], "datums": [], "features": [
        {"id": "base", "operation": {"box": {"origin": lengths(0.0, 0.0, 0.0), "size": lengths(960.0, 600.0, 20.0)}}},
        {"id": "lid", "operation": {"box": {"origin": lengths(0.0, 0.0, 20.0), "size": lengths(960.0, 600.0, 6.0)}}},
        {"id": "joint", "operation": {"fastened_joint": {"stack": ["lid", "base"], "positions": positions,
            "axis": {"literal": {"x": {"value": 0, "dimension": "scalar", "unit": null},
                "y": {"value": 0, "dimension": "scalar", "unit": null},
                "z": {"value": -1, "dimension": "scalar", "unit": null}}},
            "nominal_diameter": {"literal": {"value": 5, "dimension": "length", "unit": "millimeter"}},
            "fastening": "tapped"}}},
        {"id": "lid_drilled", "operation": {"joint_holes": {"joint": "joint", "input": "lid"}}},
        {"id": "base_drilled", "operation": {"joint_holes": {"joint": "joint", "input": "base"}}}]}))
    .unwrap();
    timed(
        "fastened joint: 1,000 M5 screws, hardware and both parts drilled".into(),
        ms(20_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let started = Instant::now();
            let part = PartInstance {
                id: "p".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let generated = part.regenerate(&session)?;
            let hardware =
                session.subshape_count(generated.shape("joint").unwrap(), ShapeType::Solid)?;
            // A screw and its head washer at each position.
            if hardware != 2000 {
                return Err(failure(format!(
                    "joint made {hardware} solids, expected 2000"
                )));
            }
            // 5.5 mm medium clearance through the 6 mm lid.
            let lid = session.volume(generated.shape("lid_drilled").unwrap())?;
            let expected = 960.0 * 600.0 * 6.0 - 1000.0 * std::f64::consts::PI * 2.75 * 2.75 * 6.0;
            if (lid / expected - 1.0).abs() > 1e-7 {
                return Err(failure(format!("lid volume {lid}, expected {expected}")));
            }
            for id in ["lid_drilled", "base_drilled"] {
                if !session.is_valid(generated.shape(id).unwrap())? {
                    return Err(failure(format!("{id} is invalid")));
                }
            }
            let elapsed = started.elapsed().as_secs_f64();
            drop(generated);
            if session.shape_count()? != 0 {
                return Err(failure("joint handles retained".into()));
            }
            Ok(format!("1,000 screws placed and drilled in {elapsed:.2} s"))
        },
    )
}
