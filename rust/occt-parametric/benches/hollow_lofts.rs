//! Explicit bore tracks, station containment and composed native wall history.
use occt_bridge::{HistoryRelation, Session, ShapeType};
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn part(document: &ModelDocument) -> PartInstance<'_> {
    PartInstance {
        id: "scale".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "benchmark".into(),
    }
}
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/hollow-loft.request.json"
    ))
    .unwrap();
    let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..500 {
        let generated = part(&document).regenerate(&session).unwrap();
        let body = generated.shape("body").unwrap();
        assert_eq!(session.shape_type(body).unwrap(), ShapeType::Solid);
        assert!(session.is_valid(body).unwrap());
        assert!((session.volume(body).unwrap() - 560.0 * std::f64::consts::PI).abs() < 1e-6);
        for id in ["lower", "lower-bore"] {
            let edge = session
                .subshape(generated.shape(id).unwrap(), ShapeType::Edge, 0)
                .unwrap();
            assert!(
                session
                    .history_count(body, &edge, HistoryRelation::Generated)
                    .unwrap()
                    > 0
            );
        }
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "hollow loft gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 500 tapered hollow lofts, exact volume and wall ancestry: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
    for parameter in &mut document.family.parameters {
        if parameter.id == "lower_radius" || parameter.id == "upper_radius" {
            parameter.default =
                ParameterValue::Scalar(Quantity::length(20.0, LengthUnit::Millimeter));
        }
        if parameter.id == "lower_bore_radius" || parameter.id == "upper_bore_radius" {
            parameter.default =
                ParameterValue::Scalar(Quantity::length(0.4, LengthUnit::Millimeter));
        }
    }
    let mut holes = Vec::new();
    for index in 0..100 {
        let mut track = Vec::new();
        for (end, input) in [("lower", "lower-bore"), ("upper", "upper-bore")] {
            let id = format!("bore-{index}-{end}");
            track.push(id.clone());
            document.family.features.push(FeatureDefinition {
                id,
                operation: FeatureOperation::Translate {
                    input: input.into(),
                    offset: VectorExpr::Literal(VectorQuantity::lengths(
                        (index % 10) as f64 * 2.0 - 9.0,
                        (index / 10) as f64 * 2.0 - 9.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                },
            });
        }
        holes.push(track);
    }
    let FeatureOperation::ProfileLoft { holes: tracks, .. } =
        &mut document.family.features[0].operation
    else {
        panic!()
    };
    *tracks = holes;
    let start = Instant::now();
    let generated = part(&document).regenerate(&session).unwrap();
    let body = generated.shape("body").unwrap();
    assert_eq!(session.shape_type(body).unwrap(), ShapeType::Solid);
    assert!(session.is_valid(body).unwrap());
    assert!((session.volume(body).unwrap() - 7680.0 * std::f64::consts::PI).abs() < 1e-5);
    for index in 0..100 {
        let edge = session
            .subshape(
                generated.shape(&format!("bore-{index}-lower")).unwrap(),
                ShapeType::Edge,
                0,
            )
            .unwrap();
        assert!(
            session
                .history_count(body, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
    }
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "100-bore loft gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 100-bore profile loft, exact volume and every bore's wall history: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
