//! Native section compatibility/fitting with bounded reusable source profiles.
use occt_bridge::Session;
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn run(source: &str, expected: f64, label: &str) {
    let request: serde_json::Value = serde_json::from_str(source).unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let parameters = PartInstance {
        id: "inspect".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "benchmark".into(),
    }
    .resolved_parameters()
    .unwrap();
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
        assert!(session.is_valid(solid).unwrap());
        assert!((session.volume(solid).unwrap() - expected).abs() < 1e-5);
        assert_eq!(
            session
                .subshape_count(result.shape("upper").unwrap(), occt_bridge::ShapeType::Edge)
                .unwrap(),
            1
        );
        let handles = session.shape_count().unwrap();
        let tangents = result
            .loft_end_tangents(
                &session,
                &document.family.features[0].operation,
                &parameters,
            )
            .unwrap();
        if label != "saved-profile" {
            assert_eq!(tangents.len(), 2);
            assert!(!tangents[0].end && tangents[1].end);
        }
        assert_eq!(session.shape_count().unwrap(), handles);
        drop(result);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "profile-loft scale budget exceeded: {elapsed:?}"
    );
    println!(
        "PASS 1000 {label} loft regenerations: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}

fn main() {
    run(
        include_str!("../../../tools/model/profile-loft.request.json"),
        20. * std::f64::consts::PI * (64. + 32. + 16.) / 3.,
        "saved-profile",
    );
    run(
        include_str!("../../../tools/model/constrained-loft.request.json"),
        20. * std::f64::consts::PI * (64. - 32. + 16. * 13. / 35.),
        "constrained-profile",
    );
    run(
        include_str!("../../../tools/model/normal-profile-loft.request.json"),
        20. * std::f64::consts::PI * (64. - 32. + 16. * 13. / 35.),
        "normal-profile",
    );
    run(
        include_str!("../../../tools/model/aligned-profile-loft.request.json"),
        20. * std::f64::consts::PI * (64. - 32. + 16. * 13. / 35.),
        "aligned-profile",
    );
}
