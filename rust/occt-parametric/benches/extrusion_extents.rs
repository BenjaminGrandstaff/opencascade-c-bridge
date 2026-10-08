//! O(target topology) candidate scan with exact coplanar coverage intersections.
//! Each iteration releases all native handles; no cross-regeneration cache grows.
use occt_bridge::Session;
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/extrusion-limits.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..1000 {
        let result = PartInstance {
            id: "scale".into(),
            definition: &document.family,
            overrides: HashMap::new(),
            provenance: "benchmark".into(),
        }
        .regenerate(&session)
        .unwrap();
        for (output, depth) in [("body", 12.0), ("selected", 14.0), ("symmetric", 12.0)] {
            let solid = result.shape(output).unwrap();
            assert!(session.is_valid(solid).unwrap());
            assert!((session.volume(solid).unwrap() - 1000.0 * depth).abs() < 1e-6);
        }
        drop(result);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "extent scale budget exceeded: {elapsed:?}"
    );
    println!(
        "PASS 1000 three-extent regenerations: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
