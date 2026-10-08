//! Native cutter/containment cost depends on body topology, not global graph size.
//! Repeated independent regeneration keeps all native handles bounded.
use occt_bridge::Session;
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/drill-point.request.json"
    ))
    .unwrap();
    let model = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..500 {
        let result = PartInstance {
            id: "scale".into(),
            definition: &model.family,
            overrides: HashMap::new(),
            provenance: "benchmark".into(),
        }
        .regenerate(&session)
        .unwrap();
        let shape = result.shape("body").unwrap();
        let expected =
            15000.0 - 72.0 * std::f64::consts::PI - 3.0 * std::f64::consts::PI * 3.0_f64.sqrt();
        assert!(session.is_valid(shape).unwrap());
        assert!((session.volume(shape).unwrap() - expected).abs() < 1e-7);
        drop(result);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "drill-point scale budget exceeded: {elapsed:?}"
    );
    println!(
        "PASS 500 drill-point regenerations: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
