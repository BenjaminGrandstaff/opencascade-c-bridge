use super::*;
use occt_bridge::ShapeType;
fn document() -> ModelDocument {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/bolt-circle.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&r["model"].to_string()).unwrap()
}
fn part(d: &ModelDocument) -> PartInstance<'_> {
    PartInstance {
        id: "plate".into(),
        definition: &d.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
#[test]
fn circular_tool_patterns_cut_a_bolt_circle_and_reuse_the_source_after_count_edits() {
    let d = document();
    let session = Session::new().unwrap();
    let mut p = part(&d);
    let first = p.regenerate(&session).unwrap();
    assert_eq!(
        session
            .subshape_count(first.shape("tools").unwrap(), ShapeType::Solid)
            .unwrap(),
        6
    );
    let expected = std::f64::consts::PI * (400.0 - 6.0 * 4.0) * 4.0;
    assert!((session.volume(first.shape("body").unwrap()).unwrap() - expected).abs() < 1e-5);
    p.overrides.insert(
        "count".into(),
        ParameterValue::Scalar(Quantity::scalar(4.0)),
    );
    let edited = p.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.reused, vec!["plate", "cutter"]);
    assert_eq!(
        session
            .subshape_count(edited.shape("tools").unwrap(), ShapeType::Solid)
            .unwrap(),
        4
    );
    let expected = std::f64::consts::PI * (400.0 - 4.0 * 4.0) * 4.0;
    assert!((session.volume(edited.shape("body").unwrap()).unwrap() - expected).abs() < 1e-5);
    let count = session.shape_count().unwrap();
    p.overrides.insert(
        "count".into(),
        ParameterValue::Scalar(Quantity::scalar(4.5)),
    );
    assert!(p.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    assert_eq!(
        ModelDocument::from_json(&d.to_json_pretty().unwrap()).unwrap(),
        d
    );
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn circular_patterns_support_signed_partial_turns_and_reject_invalid_axes_counts_and_wraps() {
    let session = Session::new().unwrap();
    let mut d = document();
    d.family.requirements.clear();
    d.family.constraints.clear();
    d.family.features.truncate(3);
    for (count, angle, axis) in [
        (
            Quantity::scalar(0.0),
            Quantity::scalar(1.0),
            VectorQuantity::scalars(0.0, 0.0, 1.0),
        ),
        (
            Quantity::scalar(2.5),
            Quantity::scalar(1.0),
            VectorQuantity::scalars(0.0, 0.0, 1.0),
        ),
        (
            Quantity::scalar(10001.0),
            Quantity::scalar(0.0001),
            VectorQuantity::scalars(0.0, 0.0, 1.0),
        ),
        (
            Quantity::scalar(3.0),
            Quantity::scalar(0.0),
            VectorQuantity::scalars(0.0, 0.0, 1.0),
        ),
        (
            Quantity::scalar(3.0),
            Quantity::scalar(std::f64::consts::PI),
            VectorQuantity::scalars(0.0, 0.0, 1.0),
        ),
        (
            Quantity::length(3.0, LengthUnit::Millimeter),
            Quantity::scalar(1.0),
            VectorQuantity::scalars(0.0, 0.0, 1.0),
        ),
        (
            Quantity::scalar(3.0),
            Quantity::length(1.0, LengthUnit::Millimeter),
            VectorQuantity::scalars(0.0, 0.0, 1.0),
        ),
        (
            Quantity::scalar(3.0),
            Quantity::scalar(1.0),
            VectorQuantity::scalars(0.0, 0.0, 0.0),
        ),
    ] {
        if let FeatureOperation::CircularPattern {
            count: c,
            angle_step_radians: a,
            axis: n,
            ..
        } = &mut d.family.features[2].operation
        {
            *c = ScalarExpr::Literal(count);
            *a = ScalarExpr::Literal(angle);
            *n = VectorExpr::Literal(axis);
        }
        assert!(part(&d).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    if let FeatureOperation::CircularPattern {
        count,
        angle_step_radians,
        axis,
        ..
    } = &mut d.family.features[2].operation
    {
        *count = ScalarExpr::Literal(Quantity::scalar(3.0));
        *angle_step_radians = ScalarExpr::Literal(Quantity::scalar(-std::f64::consts::FRAC_PI_2));
        *axis = VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0));
    }
    let result = part(&d).regenerate(&session).unwrap();
    let bounds = session
        .exact_bounds(result.shape("tools").unwrap())
        .unwrap();
    assert!((bounds.min.y + 14.0).abs() < 1e-6);
    assert!((bounds.max.y - 2.0).abs() < 1e-6);
    drop(result);
    assert_eq!(session.shape_count().unwrap(), 0);
}
