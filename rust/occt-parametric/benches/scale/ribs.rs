use super::*;

pub(super) fn rib_features_case() -> Outcome {
    const COUNT: usize = 1000;
    let mut definition = block();
    definition.datums.clear();
    definition.features.clear();
    let point =
        |x, y, z| VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter));
    let value = |x| ScalarExpr::Literal(Quantity::length(x, LengthUnit::Millimeter));
    definition.parameters.push(ParameterDefinition {
        id: "thickness".into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: length(2.0),
        minimum: None,
        maximum: None,
    });
    definition.features.push(FeatureDefinition {
        id: "body".into(),
        operation: FeatureOperation::Box {
            origin: point(0.0, 0.0, 0.0),
            size: point(10.0, 10.0, 1.0),
        },
    });
    definition.features.push(FeatureDefinition {
        id: "profile".into(),
        operation: FeatureOperation::SketchWire {
            sketch: Box::new(SketchDefinition {
                id: "brace".into(),
                datum_plane: None,
                origin: point(2.0, 4.0, 1.0),
                x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                points: vec![
                    SketchPoint {
                        id: "a".into(),
                        x: value(0.0),
                        y: value(0.0),
                        fixed: true,
                    },
                    SketchPoint {
                        id: "b".into(),
                        x: value(6.0),
                        y: value(0.0),
                        fixed: true,
                    },
                    SketchPoint {
                        id: "c".into(),
                        x: value(0.0),
                        y: value(6.0),
                        fixed: true,
                    },
                ],
                lines: vec![
                    SketchLine {
                        id: "ab".into(),
                        start: "a".into(),
                        end: "b".into(),
                    },
                    SketchLine {
                        id: "bc".into(),
                        start: "b".into(),
                        end: "c".into(),
                    },
                    SketchLine {
                        id: "ca".into(),
                        start: "c".into(),
                        end: "a".into(),
                    },
                ],
                circles: Vec::new(),
                arcs: Vec::new(),
                profile: Vec::new(),
                constraints: Vec::new(),
            }),
        },
    });
    for index in 0..COUNT {
        definition.features.push(FeatureDefinition {
            id: format!("rib{index}"),
            operation: FeatureOperation::Rib {
                input: "body".into(),
                profile: "profile".into(),
                thickness: ScalarExpr::Parameter("thickness".into()),
                direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
            },
        });
    }
    timed(
        "1000 rib features: build and edit".into(),
        ms(15_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let mut part = PartInstance {
                id: "ribs".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            part.overrides.insert("thickness".into(), length(3.0));
            let edited = part.regenerate_incremental(&session, &first)?;
            if edited.regeneration.reused != ["body", "profile"]
                || edited.regeneration.rebuilt.len() != COUNT
            {
                return Err(failure("rib edit did not reuse input and profile".into()));
            }
            for index in 0..COUNT {
                let id = format!("rib{index}");
                for (generation, expected) in [(&first, 136.0), (&edited, 154.0)] {
                    let shape = generation
                        .shape(&id)
                        .ok_or_else(|| failure("rib output missing".into()))?;
                    if (session.volume(shape)? - expected).abs() > 1e-6
                        || !session.is_valid(shape)?
                    {
                        return Err(failure("rib volume or validity differs".into()));
                    }
                }
            }
            drop((first, edited));
            if session.shape_count()? != 0 {
                return Err(failure("rib temporary handles retained".into()));
            }
            Ok(
                "1000 exact reinforcing solids rebuilt; body/profile reused; handles released"
                    .into(),
            )
        },
    )
}
