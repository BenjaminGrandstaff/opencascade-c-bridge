use super::*;
use occt_bridge::{HistoryRelation, ShapeType};

fn document() -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/offset-part.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}

#[test]
fn signed_offsets_resize_the_skin_retain_ancestry_and_reuse_the_source() {
    let document = document();
    let session = Session::new().unwrap();
    let mut instance = PartInstance {
        id: "part".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let first = instance.regenerate(&session).unwrap();
    let face = session
        .subshape(first.shape("source").unwrap(), ShapeType::Face, 0)
        .unwrap();
    let body = first.shape("body").unwrap();
    let history = session
        .history_count(body, &face, HistoryRelation::Modified)
        .unwrap()
        + session
            .history_count(body, &face, HistoryRelation::Generated)
            .unwrap();
    assert!(history > 0);
    assert!(
        (session.volume(body).unwrap() - 4.0 / 3.0 * std::f64::consts::PI * 11.0_f64.powi(3)).abs()
            < 1e-5
    );
    instance.overrides.insert(
        "allowance".into(),
        ParameterValue::Scalar(Quantity::length(-2.0, LengthUnit::Millimeter)),
    );
    let inward = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(inward.regeneration.reused, vec!["source"]);
    assert_eq!(inward.regeneration.rebuilt, vec!["body"]);
    let body = inward.shape("body").unwrap();
    assert!(session.is_valid(body).unwrap());
    assert!(
        (session.volume(body).unwrap() - 4.0 / 3.0 * std::f64::consts::PI * 8.0_f64.powi(3)).abs()
            < 1e-5
    );
    assert!((session.exact_bounds(body).unwrap().max.x - 8.0).abs() < 1e-7);
    assert!(
        (session
            .exact_bounds(first.shape("source").unwrap())
            .unwrap()
            .max
            .x
            - 10.0)
            .abs()
            < 1e-7
    );
    let count = session.shape_count().unwrap();
    instance.overrides.insert(
        "allowance".into(),
        ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
    );
    assert!(instance.regenerate_incremental(&session, &inward).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(body).unwrap());
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    drop((face, first, inward));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn offsets_reject_wrong_units_zero_distance_and_nonpositive_tolerance_without_leaks() {
    let session = Session::new().unwrap();
    for (distance, tolerance) in [
        (
            Quantity::scalar(1.0),
            Quantity::length(1e-6, LengthUnit::Millimeter),
        ),
        (
            Quantity::length(1.0, LengthUnit::Millimeter),
            Quantity::scalar(1e-6),
        ),
        (
            Quantity::length(0.0, LengthUnit::Millimeter),
            Quantity::length(1e-6, LengthUnit::Millimeter),
        ),
        (
            Quantity::length(1.0, LengthUnit::Millimeter),
            Quantity::length(0.0, LengthUnit::Millimeter),
        ),
        (
            Quantity::length(1.0, LengthUnit::Millimeter),
            Quantity::length(-1e-6, LengthUnit::Millimeter),
        ),
    ] {
        let mut document = document();
        if let FeatureOperation::Offset {
            distance: d,
            tolerance: t,
            ..
        } = &mut document.family.features[1].operation
        {
            *d = ScalarExpr::Literal(distance);
            *t = ScalarExpr::Literal(tolerance);
        }
        assert!(
            PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "test".into()
            }
            .regenerate(&session)
            .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
