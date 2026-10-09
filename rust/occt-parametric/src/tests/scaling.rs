use super::*;
use occt_bridge::{HistoryRelation, ShapeType};
fn document() -> ModelDocument {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/scaled-part.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&r["model"].to_string()).unwrap()
}
fn part(d: &ModelDocument) -> PartInstance<'_> {
    PartInstance {
        id: "bracket".into(),
        definition: &d.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
#[test]
fn scales_resize_geometry_preserve_source_history_and_reuse_inputs_after_edits() {
    let session = Session::new().unwrap();
    let document = document();
    let mut instance = part(&document);
    let first = instance.regenerate(&session).unwrap();
    let body = first.shape("body").unwrap();
    assert!(session.is_valid(body).unwrap());
    assert!((session.volume(body).unwrap() - 1920.0 * 1.2_f64.powi(3)).abs() < 1e-6);
    let bounds = session.exact_bounds(body).unwrap();
    assert!((bounds.min.x - 6.0).abs() < 1e-7 && (bounds.max.x - 30.0).abs() < 1e-7);
    let face = session
        .subshape(first.shape("source").unwrap(), ShapeType::Face, 0)
        .unwrap();
    assert!(
        session
            .history_count(body, &face, HistoryRelation::Modified)
            .unwrap()
            > 0
    );
    instance.overrides.insert(
        "scale_factor".into(),
        ParameterValue::Scalar(Quantity::scalar(2.0)),
    );
    instance.overrides.insert(
        "scale_center_x".into(),
        ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.reused, vec!["base", "tab", "source"]);
    assert_eq!(second.regeneration.rebuilt, vec!["body"]);
    let bounds = session.exact_bounds(second.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x - 5.0).abs() < 1e-7 && (bounds.max.x - 45.0).abs() < 1e-7);
    assert!((session.volume(second.shape("body").unwrap()).unwrap() - 15360.0).abs() < 1e-6);
    assert!(
        (session
            .exact_bounds(first.shape("source").unwrap())
            .unwrap()
            .max
            .x
            - 25.0)
            .abs()
            < 1e-7
    );
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    let count = session.shape_count().unwrap();
    instance.overrides.insert(
        "scale_factor".into(),
        ParameterValue::Scalar(Quantity::scalar(0.0)),
    );
    assert!(instance.regenerate_incremental(&session, &second).is_err());
    assert!(session.is_valid(second.shape("body").unwrap()).unwrap());
    assert_eq!(session.shape_count().unwrap(), count);
    drop((face, first, second));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn scale_requires_a_length_centre_and_a_positive_dimensionless_factor() {
    let session = Session::new().unwrap();
    for (center, factor) in [
        (
            VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0)),
            ScalarExpr::Literal(Quantity::scalar(2.0)),
        ),
        (
            VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
        ),
        (
            VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            ScalarExpr::Literal(Quantity::scalar(-1.0)),
        ),
    ] {
        let mut document = document();
        let FeatureOperation::Scale {
            center: c,
            factor: f,
            ..
        } = &mut document.family.features[0].operation
        else {
            panic!()
        };
        *c = center;
        *f = factor;
        assert!(part(&document).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
