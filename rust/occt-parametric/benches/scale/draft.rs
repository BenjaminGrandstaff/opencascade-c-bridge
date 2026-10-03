use super::*;

pub(super) fn draft_features_case() -> Outcome {
    const COUNT: usize = 1000;
    let mut definition = block();
    definition.datums.clear();
    let point =
        |x, y, z| VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter));
    definition.features[0].operation = FeatureOperation::Box {
        origin: point(0.0, 0.0, 0.0),
        size: point(10.0, 10.0, 10.0),
    };
    definition.parameters.push(ParameterDefinition {
        id: "angle".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(0.1)),
        minimum: None,
        maximum: None,
    });
    for index in 0..COUNT {
        definition.features.push(FeatureDefinition {
            id: format!("draft{index}"),
            operation: FeatureOperation::Draft {
                input: "body".into(),
                faces: vec![FaceSelector::AtExtreme {
                    axis: CoordinateAxis::X,
                    extremum: Extremum::Maximum,
                    tolerance: ScalarExpr::Literal(Quantity::length(1e-6, LengthUnit::Millimeter)),
                }],
                neutral_origin: point(0.0, 0.0, 0.0),
                neutral_normal: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                pull_direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                angle_radians: ScalarExpr::Parameter("angle".into()),
            },
        });
    }
    timed(
        "1000 draft features: build and edit".into(),
        ms(10_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let mut part = PartInstance {
                id: "drafts".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            part.overrides.insert(
                "angle".into(),
                ParameterValue::Scalar(Quantity::scalar(-0.1)),
            );
            let edited = part.regenerate_incremental(&session, &first)?;
            if edited.regeneration.reused != ["body"] || edited.regeneration.rebuilt.len() != COUNT
            {
                return Err(failure("draft edit did not reuse only its input".into()));
            }
            for index in 0..COUNT {
                let id = format!("draft{index}");
                for (generation, angle) in [(&first, 0.1_f64), (&edited, -0.1)] {
                    let shape = generation
                        .shape(&id)
                        .ok_or_else(|| failure("draft output missing".into()))?;
                    if (session.volume(shape)? - (1000.0 - 500.0 * angle.tan())).abs() > 1e-6
                        || !session.is_valid(shape)?
                    {
                        return Err(failure("draft volume or validity differs".into()));
                    }
                }
            }
            drop((first, edited));
            if session.shape_count()? != 0 {
                return Err(failure("draft handles retained".into()));
            }
            Ok("1000 exact tapered solids rebuilt; input reused; handles released".into())
        },
    )
}
