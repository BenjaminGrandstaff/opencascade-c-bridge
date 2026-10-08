//! Symmetric rotation, exact native revolution and composed source history.
use occt_bridge::{HistoryRelation, Session, ShapeType};
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/symmetric-revolve.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for index in 0..1000 {
        let angle = if index % 2 == 0 {
            std::f64::consts::FRAC_PI_2
        } else {
            -std::f64::consts::FRAC_PI_2
        };
        let mut overrides = HashMap::new();
        overrides.insert(
            "angle".into(),
            ParameterValue::Scalar(Quantity::scalar(angle)),
        );
        let generated = PartInstance {
            id: "scale".into(),
            definition: &document.family,
            overrides,
            provenance: "benchmark".into(),
        }
        .regenerate(&session)
        .unwrap();
        let body = generated.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!((session.volume(body).unwrap() - 140.0 * angle.abs()).abs() < 1e-6);
        let bounds = session.exact_bounds(body).unwrap();
        assert!((bounds.min.y + bounds.max.y).abs() < 1e-7);
        let profile = generated.shape("profile").unwrap();
        let edge = session.subshape(profile, ShapeType::Edge, 0).unwrap();
        assert!(
            session
                .history_count(body, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
        assert_eq!(session.subshape_count(profile, ShapeType::Edge).unwrap(), 4);
        drop((edge, generated));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "symmetric revolve scale budget exceeded: {elapsed:?}"
    );
    println!(
        "PASS 1000 signed symmetric revolutions, volumes, centering and source history: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
