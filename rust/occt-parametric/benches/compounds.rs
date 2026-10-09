//! A multi-tool cut and the bounded 10,000-member grouping case.
use occt_bridge::{HistoryRelation, Session, ShapeType};
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/multi-hole-plate.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..100 {
        let generated = PartInstance {
            id: "plate".into(),
            definition: &document.family,
            overrides: HashMap::new(),
            provenance: "benchmark".into(),
        }
        .regenerate(&session)
        .unwrap();
        let body = generated.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert!(
            (session.volume(body).unwrap() - (3600.0 - 144.0 * std::f64::consts::PI)).abs() < 1e-5
        );
        assert_eq!(
            session
                .subshape_count(generated.shape("tools").unwrap(), ShapeType::Solid)
                .unwrap(),
            9
        );
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(15));
    println!(
        "PASS 100 nine-tool plate cuts, volume and cleanup: {:.3}s / 15s",
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
    let mut inputs = Vec::new();
    for i in 0..10000 {
        let id = format!("member-{i}");
        inputs.push(id.clone());
        family.features.push(FeatureDefinition {
            id,
            operation: FeatureOperation::Translate {
                input: "source".into(),
                offset: VectorExpr::Literal(VectorQuantity::lengths(
                    i as f64 * 2.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
        });
    }
    family.features.push(FeatureDefinition {
        id: "group".into(),
        operation: FeatureOperation::Compound { inputs },
    });
    let start = Instant::now();
    let generated = PartInstance {
        id: "grouped".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "benchmark".into(),
    }
    .regenerate(&session)
    .unwrap();
    let group = generated.shape("group").unwrap();
    let source_face = session
        .subshape(generated.shape("source").unwrap(), ShapeType::Face, 0)
        .unwrap();
    assert_eq!(
        session
            .history_count(group, &source_face, HistoryRelation::Modified)
            .unwrap(),
        10000
    );
    drop(source_face);
    assert_eq!(
        session.subshape_count(group, ShapeType::Solid).unwrap(),
        10000
    );
    assert!((session.volume(group).unwrap() - 10000.0).abs() < 1e-5);
    assert!((session.exact_bounds(group).unwrap().max.x - 19999.0).abs() < 1e-7);
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(15));
    println!(
        "PASS 10000 located compound members, shared source, bounds and cleanup: {:.3}s / 15s",
        elapsed.as_secs_f64()
    );
}
