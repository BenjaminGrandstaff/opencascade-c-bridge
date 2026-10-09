use occt_bridge::{Session, ShapeType};
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/bolt-circle.request.json"
    ))
    .unwrap();
    let d = ModelDocument::from_json(&r["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for i in 0..100 {
        let count = if i % 2 == 0 { 6.0 } else { 4.0 };
        let mut overrides = HashMap::new();
        overrides.insert(
            "count".into(),
            ParameterValue::Scalar(Quantity::scalar(count)),
        );
        let result = PartInstance {
            id: "plate".into(),
            definition: &d.family,
            overrides,
            provenance: "benchmark".into(),
        }
        .regenerate(&session)
        .unwrap();
        let body = result.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!(
            (session.volume(body).unwrap() - std::f64::consts::PI * (400.0 - count * 4.0) * 4.0)
                .abs()
                < 1e-5
        );
        assert_eq!(
            session
                .subshape_count(result.shape("tools").unwrap(), ShapeType::Solid)
                .unwrap(),
            count as usize
        );
        drop(result);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(15));
    println!(
        "PASS 100 bolt-circle cuts, counts, volume and cleanup: {:.3}s / 15s",
        elapsed.as_secs_f64()
    );
    let mut family = d.family.clone();
    family.constraints.clear();
    family.requirements.clear();
    family.features = vec![family.features[1].clone()];
    family.features.push(FeatureDefinition {
        id: "pattern".into(),
        operation: FeatureOperation::CircularPattern {
            input: "cutter".into(),
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            count: ScalarExpr::Literal(Quantity::scalar(10000.0)),
            angle_step_radians: ScalarExpr::Literal(Quantity::scalar(
                std::f64::consts::TAU / 10000.0,
            )),
        },
    });
    let start = Instant::now();
    let result = PartInstance {
        id: "radial".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "benchmark".into(),
    }
    .regenerate(&session)
    .unwrap();
    let group = result.shape("pattern").unwrap();
    assert_eq!(
        session.subshape_count(group, ShapeType::Solid).unwrap(),
        10000
    );
    let bounds = session.exact_bounds(group).unwrap();
    assert!((bounds.max.x - 14.0).abs() < 1e-6);
    assert!((bounds.min.y + 14.0).abs() < 1e-6);
    assert_eq!(session.shape_count().unwrap(), 2);
    drop(result);
    assert_eq!(session.shape_count().unwrap(), 0);
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(15));
    println!(
        "PASS 10000 radial copies, bounds, shared geometry and cleanup: {:.3}s / 15s",
        elapsed.as_secs_f64()
    );
}
