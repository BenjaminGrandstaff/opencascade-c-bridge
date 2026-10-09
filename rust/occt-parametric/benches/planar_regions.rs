//! Holed-face construction, bounded multiple-hole indexing and native cleanup.
use occt_bridge::{Session, ShapeType};
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
        "../../../tools/model/hollow-profile.request.json"
    ))
    .unwrap();
    let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..500 {
        let generated = part(&document).regenerate(&session).unwrap();
        let region = generated.shape("region").unwrap();
        let body = generated.shape("body").unwrap();
        assert!(session.is_valid(region).unwrap() && session.is_valid(body).unwrap());
        assert_eq!(session.subshape_count(region, ShapeType::Wire).unwrap(), 2);
        assert!((session.volume(body).unwrap() - 960.0 * std::f64::consts::PI).abs() < 1e-6);
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "annular region gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 500 annular profiles and hollow extrusions: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
    for parameter in &mut document.family.parameters {
        if parameter.id == "outer_radius" {
            parameter.default =
                ParameterValue::Scalar(Quantity::length(20.0, LengthUnit::Millimeter));
        }
        if parameter.id == "inner_radius" {
            parameter.default =
                ParameterValue::Scalar(Quantity::length(0.4, LengthUnit::Millimeter));
        }
    }
    let mut holes = Vec::new();
    for index in 0..100 {
        let id = format!("hole-{index}");
        holes.push(id.clone());
        document.family.features.push(FeatureDefinition {
            id,
            operation: FeatureOperation::Translate {
                input: "inner".into(),
                offset: VectorExpr::Literal(VectorQuantity::lengths(
                    (index % 10) as f64 * 2.0 - 9.0,
                    (index / 10) as f64 * 2.0 - 9.0,
                    0.0,
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
    let region = generated.shape("region").unwrap();
    let body = generated.shape("body").unwrap();
    assert_eq!(
        session.subshape_count(region, ShapeType::Wire).unwrap(),
        101
    );
    assert!(session.is_valid(region).unwrap() && session.is_valid(body).unwrap());
    assert!((session.surface_area(region).unwrap() - 384.0 * std::f64::consts::PI).abs() < 1e-6);
    assert!((session.volume(body).unwrap() - 7680.0 * std::f64::consts::PI).abs() < 1e-5);
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "100-hole region gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 100-hole planar region, exact area/volume and native cleanup: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
