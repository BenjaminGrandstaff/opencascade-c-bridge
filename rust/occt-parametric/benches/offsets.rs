//! Joined skin offsets and geometry sharing at scale.
use occt_bridge::{HistoryRelation, Session, ShapeType};
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/offset-part.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for i in 0..500 {
        let mut overrides = HashMap::new();
        let distance = if i % 2 == 0 { 1.0 } else { -2.0 };
        overrides.insert(
            "allowance".into(),
            ParameterValue::Scalar(Quantity::length(distance, LengthUnit::Millimeter)),
        );
        let generated = PartInstance {
            id: "part".into(),
            definition: &document.family,
            overrides,
            provenance: "benchmark".into(),
        }
        .regenerate(&session)
        .unwrap();
        let body = generated.shape("body").unwrap();
        let radius = 10.0 + distance;
        assert!(session.is_valid(body).unwrap());
        assert!(
            (session.volume(body).unwrap() - 4.0 / 3.0 * std::f64::consts::PI * radius.powi(3))
                .abs()
                < 1e-5
        );
        assert!((session.exact_bounds(body).unwrap().max.x - radius).abs() < 1e-7);
        let face = session
            .subshape(generated.shape("source").unwrap(), ShapeType::Face, 0)
            .unwrap();
        assert!(
            session
                .history_count(body, &face, HistoryRelation::Modified)
                .unwrap()
                + session
                    .history_count(body, &face, HistoryRelation::Generated)
                    .unwrap()
                > 0
        );
        drop((face, generated));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(10));
    println!(
        "PASS 500 signed skin offsets, volume, bounds, history and cleanup: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
    let start = Instant::now();
    let mut graph = InstanceGraph::new(&document.family);
    graph.add_base("part", HashMap::new(), "benchmark").unwrap();
    for i in 1..10000 {
        graph
            .add_clone(format!("copy-{i}"), "part", HashMap::new(), "benchmark")
            .unwrap();
    }
    let generated = graph.regenerate_all(&session).unwrap();
    assert_eq!(generated.generated_variants(), 1);
    assert_eq!(generated.instances().count(), 10000);
    assert!(
        session
            .is_valid(
                generated
                    .result("copy-9999")
                    .unwrap()
                    .shape("body")
                    .unwrap()
            )
            .unwrap()
    );
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(10));
    println!(
        "PASS 10000 linked offset parts, one geometry variant and cleanup: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
