//! Native section compatibility/fitting with bounded reusable source profiles.
use occt_bridge::Session;
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/profile-loft.request.json"
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
        let solid = result.shape("body").unwrap();
        let expected = 20.0 * std::f64::consts::PI * (64.0 + 32.0 + 16.0) / 3.0;
        assert!(session.is_valid(solid).unwrap());
        assert!((session.volume(solid).unwrap() - expected).abs() < 1e-5);
        assert_eq!(
            session
                .subshape_count(result.shape("upper").unwrap(), occt_bridge::ShapeType::Edge)
                .unwrap(),
            1
        );
        drop(result);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "profile-loft scale budget exceeded: {elapsed:?}"
    );
    println!(
        "PASS 1000 saved-profile loft regenerations: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
