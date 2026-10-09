use super::*;
use occt_bridge::ShapeType;
fn document() -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/patterned-plate.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}
fn part(document: &ModelDocument) -> PartInstance<'_> {
    PartInstance {
        id: "plate".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
#[test]
fn part_pattern_counts_resize_the_grid_reuse_source_cutter_and_preserve_accepted_parts() {
    let document = document();
    let session = Session::new().unwrap();
    let mut instance = part(&document);
    let first = instance.regenerate(&session).unwrap();
    assert_eq!(
        session
            .subshape_count(first.shape("tools").unwrap(), ShapeType::Solid)
            .unwrap(),
        9
    );
    let expected = 3600.0 - 144.0 * std::f64::consts::PI;
    assert!((session.volume(first.shape("body").unwrap()).unwrap() - expected).abs() < 1e-5);
    instance.overrides.insert(
        "columns".into(),
        ParameterValue::Scalar(Quantity::scalar(4.0)),
    );
    instance
        .overrides
        .insert("rows".into(), ParameterValue::Scalar(Quantity::scalar(2.0)));
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.reused, vec!["cutter"]);
    assert_eq!(
        session
            .subshape_count(edited.shape("tools").unwrap(), ShapeType::Solid)
            .unwrap(),
        8
    );
    let expected = 40.0 * 20.0 * 4.0 - 8.0 * std::f64::consts::PI * 4.0 * 4.0;
    assert!((session.volume(edited.shape("body").unwrap()).unwrap() - expected).abs() < 1e-5);
    assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    let bounds = session.exact_bounds(edited.shape("body").unwrap()).unwrap();
    assert!((bounds.max.x - 40.0).abs() < 1e-7 && (bounds.max.y - 20.0).abs() < 1e-7);
    let count = session.shape_count().unwrap();
    instance.overrides.insert(
        "columns".into(),
        ParameterValue::Scalar(Quantity::scalar(2.5)),
    );
    assert!(instance.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn part_patterns_validate_counts_units_steps_and_nested_topology_before_expansion() {
    let session = Session::new().unwrap();
    let mut document = document();
    document.family.requirements.clear();
    document.family.features.truncate(1);
    document.family.features[0].id = "source".into();
    document.family.features.push(FeatureDefinition {
        id: "copies".into(),
        operation: FeatureOperation::LinearPattern {
            input: "source".into(),
            step: VectorExpr::Literal(VectorQuantity::lengths(
                40.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            count: ScalarExpr::Literal(Quantity::scalar(3.0)),
        },
    });
    for (count, step) in [
        (
            Quantity::scalar(0.0),
            VectorQuantity::lengths(40.0, 0.0, 0.0, LengthUnit::Millimeter),
        ),
        (
            Quantity::scalar(2.5),
            VectorQuantity::lengths(40.0, 0.0, 0.0, LengthUnit::Millimeter),
        ),
        (
            Quantity::scalar(10001.0),
            VectorQuantity::lengths(40.0, 0.0, 0.0, LengthUnit::Millimeter),
        ),
        (
            Quantity::length(3.0, LengthUnit::Millimeter),
            VectorQuantity::lengths(40.0, 0.0, 0.0, LengthUnit::Millimeter),
        ),
        (
            Quantity::scalar(3.0),
            VectorQuantity::scalars(40.0, 0.0, 0.0),
        ),
        (
            Quantity::scalar(3.0),
            VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        ),
        (
            Quantity::scalar(3.0),
            VectorQuantity::lengths(1e308, 0.0, 0.0, LengthUnit::Millimeter),
        ),
    ] {
        if let FeatureOperation::LinearPattern {
            count: c, step: s, ..
        } = &mut document.family.features[1].operation
        {
            *c = ScalarExpr::Literal(count);
            *s = VectorExpr::Literal(step);
        }
        assert!(part(&document).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    if let FeatureOperation::LinearPattern { count, step, .. } =
        &mut document.family.features[1].operation
    {
        *count = ScalarExpr::Literal(Quantity::scalar(4.0));
        *step = VectorExpr::Literal(VectorQuantity::lengths(
            -40.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        ));
    }
    let result = part(&document).regenerate(&session).unwrap();
    assert!(
        (session
            .exact_bounds(result.shape("copies").unwrap())
            .unwrap()
            .min
            .x
            + 120.0)
            .abs()
            < 1e-7
    );
    drop(result);
    document.family.features.push(FeatureDefinition {
        id: "nested".into(),
        operation: FeatureOperation::LinearPattern {
            input: "copies".into(),
            step: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                40.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            count: ScalarExpr::Literal(Quantity::scalar(10000.0)),
        },
    });
    let Err(error) = part(&document).regenerate(&session) else {
        panic!("nested expansion must fail");
    };
    assert!(error.message.contains("estimated topology"));
    assert_eq!(session.shape_count().unwrap(), 0);
}
