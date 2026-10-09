//! Reflected geometry creation and sharing across linked part instances.
use occt_bridge::{HistoryRelation, Session, ShapeType};
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/mirrored-part.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..500 {
        let generated = PartInstance {
            id: "scale".into(),
            definition: &document.family,
            overrides: HashMap::new(),
            provenance: "benchmark".into(),
        }
        .regenerate(&session)
        .unwrap();
        let body = generated.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!((session.volume(body).unwrap() - 1920.0).abs() < 1e-6);
        let b = session.exact_bounds(body).unwrap();
        assert!((b.min.x + 25.0).abs() < 1e-7 && (b.max.x + 5.0).abs() < 1e-7);
        let source = session
            .subshape(generated.shape("source").unwrap(), ShapeType::Face, 0)
            .unwrap();
        assert!(
            session
                .history_count(body, &source, HistoryRelation::Modified)
                .unwrap()
                > 0
        );
        drop((source, generated));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "mirror regeneration gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 500 mirrored handed parts, volume, bounds and source history: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
    let start = Instant::now();
    let mut graph = InstanceGraph::new(&document.family);
    graph
        .add_base("bracket", HashMap::new(), "benchmark")
        .unwrap();
    for index in 1..10000 {
        graph
            .add_clone(
                format!("copy-{index}"),
                "bracket",
                HashMap::new(),
                "benchmark",
            )
            .unwrap();
    }
    let generated = graph.regenerate_all(&session).unwrap();
    assert_eq!(generated.generated_variants(), 1);
    assert_eq!(generated.instances().count(), 10000);
    let body = generated
        .result("copy-9999")
        .unwrap()
        .shape("body")
        .unwrap();
    assert!((session.volume(body).unwrap() - 1920.0).abs() < 1e-6);
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "mirror sharing gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 10000 linked mirrored parts, one geometry variant and cleanup: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
