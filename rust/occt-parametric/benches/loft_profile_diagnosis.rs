//! Isolated failed-loft diagnosis: source regeneration, exact alignment and release.
use occt_bridge::Session;
use occt_parametric::*;
use std::time::{Duration, Instant};
fn main() {
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/partitioned-profile-loft.request.json"
    ))
    .unwrap();
    request["model"]["family"]["features"][0]["operation"]["profile_loft"]["correspondence"] =
        serde_json::json!({});
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let graph = document.instance_graph().unwrap();
    let part = graph.resolve("block").unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..1000 {
        let report =
            diagnose_loft_profiles(&part, &session, &document.family.features[0].operation)
                .unwrap();
        assert!(!report.checks_passed && !report.solid_checked);
        assert_eq!(report.profiles.len(), 2);
        assert_eq!(
            (
                report.profiles[0].source_edges,
                report.profiles[0].aligned_edges
            ),
            (Some(1), Some(1))
        );
        assert_eq!(report.profiles[1].aligned_edges, Some(4));
        assert!(
            report
                .profiles
                .iter()
                .all(|p| p.issues.iter().any(|i| i.code == "edge_count_mismatch"))
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    println!("1000 isolated failed-loft profile diagnoses: {elapsed:?} (budget 10s)");
    assert!(
        elapsed < Duration::from_secs(10),
        "Loft profile diagnosis exceeded its scale budget."
    );
}
