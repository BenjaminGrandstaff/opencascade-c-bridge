//! Hollow native transport, composed wall ancestry and bounded bore counts.
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
        "../../../tools/model/hollow-sweep.request.json"
    ))
    .unwrap();
    let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..250 {
        let generated = part(&document).regenerate(&session).unwrap();
        let body = generated.shape("body").unwrap();
        assert_eq!(session.shape_type(body).unwrap(), ShapeType::Solid);
        assert!(session.is_valid(body).unwrap());
        assert!(
            (session.volume(body).unwrap()
                - 3.0 * std::f64::consts::PI * (10.0 + 5.0 * std::f64::consts::PI))
                .abs()
                < 1e-6
        );
        for id in ["outer", "inner"] {
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
        "curved hollow sweep gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 250 curved hollow sweeps, exact volumes and wall ancestry: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
    for parameter in &mut document.family.parameters {
        if parameter.id == "radius" {
            parameter.default =
                ParameterValue::Scalar(Quantity::length(20.0, LengthUnit::Millimeter));
        }
        if parameter.id == "inner_radius" {
            parameter.default =
                ParameterValue::Scalar(Quantity::length(0.4, LengthUnit::Millimeter));
        }
    }
    let FeatureOperation::Sweep { orientation, .. } = &mut document.family.features[0].operation
    else {
        panic!()
    };
    *orientation = SweepOrientation::Fixed;
    let FeatureOperation::SketchOpenWire { sketch } = &mut document.family.features[4].operation
    else {
        panic!()
    };
    sketch.arcs.clear();
    sketch.profile = vec!["run".into()];
    let mut holes = Vec::new();
    for index in 0..100 {
        let id = format!("bore-{index}");
        holes.push(id.clone());
        document.family.features.push(FeatureDefinition {
            id,
            operation: FeatureOperation::Translate {
                input: "inner".into(),
                offset: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    (index % 10) as f64 * 2.0 - 9.0,
                    (index / 10) as f64 * 2.0 - 9.0,
                    LengthUnit::Millimeter,
                )),
            },
        });
    }
    document.family.features[1].operation = FeatureOperation::PlanarRegion {
        outer: "outer".into(),
        holes,
    };
    let start = Instant::now();
    let generated = part(&document).regenerate(&session).unwrap();
    let body = generated.shape("body").unwrap();
    assert_eq!(session.shape_type(body).unwrap(), ShapeType::Solid);
    assert!(session.is_valid(body).unwrap());
    assert!((session.volume(body).unwrap() - 3840.0 * std::f64::consts::PI).abs() < 1e-5);
    for index in 0..100 {
        let edge = session
            .subshape(
                generated.shape(&format!("bore-{index}")).unwrap(),
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
        "100-bore sweep gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 100-bore native sweep, solid topology, volume and wall history: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
