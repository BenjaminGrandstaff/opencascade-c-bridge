//! Parameter-driven tool grids and 10,000 shared-geometry part copies.
use occt_bridge::{Session, ShapeType};
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/patterned-plate.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for i in 0..100 {
        let columns = if i % 2 == 0 { 3.0 } else { 4.0 };
        let mut overrides = HashMap::new();
        overrides.insert(
            "columns".into(),
            ParameterValue::Scalar(Quantity::scalar(columns)),
        );
        let result = PartInstance {
            id: "plate".into(),
            definition: &document.family,
            overrides,
            provenance: "benchmark".into(),
        }
        .regenerate(&session)
        .unwrap();
        let body = result.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        let width = (columns - 1.0) * 10.0 + 10.0;
        let expected = width * 30.0 * 4.0 - columns * 3.0 * std::f64::consts::PI * 4.0 * 4.0;
        assert!((session.volume(body).unwrap() - expected).abs() < 1e-5);
        assert_eq!(
            session
                .subshape_count(result.shape("tools").unwrap(), ShapeType::Solid)
                .unwrap(),
            columns as usize * 3
        );
        drop(result);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(15));
    println!(
        "PASS 100 patterned tool-grid cuts, counts, volume and cleanup: {:.3}s / 15s",
        elapsed.as_secs_f64()
    );
    let mut family = document.family.clone();
    family.requirements.clear();
    family.features.truncate(1);
    family.features[0] = FeatureDefinition {
        id: "source".into(),
        operation: FeatureOperation::Box {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            size: VectorExpr::Literal(VectorQuantity::lengths(
                1.0,
                1.0,
                1.0,
                LengthUnit::Millimeter,
            )),
        },
    };
    family.features.push(FeatureDefinition {
        id: "pattern".into(),
        operation: FeatureOperation::LinearPattern {
            input: "source".into(),
            step: VectorExpr::Literal(VectorQuantity::lengths(
                2.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            count: ScalarExpr::Literal(Quantity::scalar(10000.0)),
        },
    });
    let start = Instant::now();
    let result = PartInstance {
        id: "copies".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "benchmark".into(),
    }
    .regenerate(&session)
    .unwrap();
    let pattern = result.shape("pattern").unwrap();
    assert_eq!(
        session.subshape_count(pattern, ShapeType::Solid).unwrap(),
        10000
    );
    assert!((session.volume(pattern).unwrap() - 10000.0).abs() < 1e-5);
    assert!((session.exact_bounds(pattern).unwrap().max.x - 19999.0).abs() < 1e-7);
    assert_eq!(session.shape_count().unwrap(), 2);
    drop(result);
    assert_eq!(session.shape_count().unwrap(), 0);
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(15));
    println!(
        "PASS 10000 location-only part copies, bounds, bounded handles and cleanup: {:.3}s / 15s",
        elapsed.as_secs_f64()
    );
}
