//! Bounded face selection/cutters scale with input topology. Single-output
//! witness queries also build a definition index; no handles persist afterward.
use occt_bridge::Session;
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/hole-limits.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..100 {
        let instance = PartInstance {
            id: "scale".into(),
            definition: &document.family,
            overrides: HashMap::new(),
            provenance: "benchmark".into(),
        };
        let result = instance.regenerate(&session).unwrap();
        let count = session.shape_count().unwrap();
        let original = session.volume(result.shape("body").unwrap()).unwrap();
        let expected = 450.0 * std::f64::consts::PI
            - 2.0 * std::f64::consts::PI / 3.0 * (27000.0 - 891_f64.powf(1.5));
        for output in ["bored", "selected-hole"] {
            let shape = result.shape(output).unwrap();
            assert!(session.is_valid(shape).unwrap());
            assert!((original - session.volume(shape).unwrap() - expected).abs() < original * 1e-7);
            assert!(
                (instance
                    .hole_limit_measurement(&session, &result, output)
                    .unwrap()
                    .unwrap()
                    .distance
                    - 20.0)
                    .abs()
                    < 1e-6
            );
            assert_eq!(session.shape_count().unwrap(), count);
        }
        drop(result);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "hole-limit scale budget exceeded: {elapsed:?}"
    );
    println!(
        "PASS 100 two-hole curved-limit regenerations and witnesses: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
