//! Native resizing and geometry sharing for linked scaled parts.
use occt_bridge::{HistoryRelation, Session, ShapeType};
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/scaled-part.request.json"
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
        assert!((session.volume(body).unwrap() - 1920.0 * 1.2_f64.powi(3)).abs() < 1e-6);
        assert!((session.exact_bounds(body).unwrap().max.x - 30.0).abs() < 1e-7);
        let face = session
            .subshape(generated.shape("source").unwrap(), ShapeType::Face, 0)
            .unwrap();
        assert!(
            session
                .history_count(body, &face, HistoryRelation::Modified)
                .unwrap()
                > 0
        );
        drop((face, generated));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "scaled part gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 500 scaled parts, cubed volume factor, bounds and source history: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
    let start = Instant::now();
    let mut graph = InstanceGraph::new(&document.family);
    graph
        .add_base("bracket", HashMap::new(), "benchmark")
        .unwrap();
    for i in 1..10000 {
        graph
            .add_clone(format!("copy-{i}"), "bracket", HashMap::new(), "benchmark")
            .unwrap();
    }
    let generated = graph.regenerate_all(&session).unwrap();
    assert_eq!(generated.generated_variants(), 1);
    assert_eq!(generated.instances().count(), 10000);
    assert!(
        (session
            .volume(
                generated
                    .result("copy-9999")
                    .unwrap()
                    .shape("body")
                    .unwrap()
            )
            .unwrap()
            - 3317.76)
            .abs()
            < 1e-6
    );
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "scaled sharing gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 10000 linked scaled parts, one geometry variant and cleanup: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
