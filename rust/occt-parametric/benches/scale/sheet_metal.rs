use super::*;
use occt_parametric::{SheetMetalBend, SheetMetalDefinition};
pub(crate) fn sheet_case() -> Outcome {
    timed(
        "1000 sheet-metal brackets: folded/flat build and edit".into(),
        ms(15_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let length =
                |value| ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter));
            let sheet = SheetMetalDefinition {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                width_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
                start_direction: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                width: length(10.0),
                thickness: ScalarExpr::Parameter("thickness".into()),
                flanges: vec![length(20.0); 3],
                bends: vec![
                    SheetMetalBend {
                        angle_radians: ScalarExpr::Literal(Quantity::scalar(
                            std::f64::consts::FRAC_PI_2
                        )),
                        inside_radius: length(3.0)
                    };
                    2
                ],
            };
            let mut definition = block();
            definition.parameters = vec![ParameterDefinition {
                id: "thickness".into(),
                parameter_type: ParameterType::Scalar(Dimension::Length),
                default: ParameterValue::Scalar(Quantity::length(2.0, LengthUnit::Millimeter)),
                minimum: None,
                maximum: None,
            }];
            definition.features.clear();
            definition.datums.clear();
            definition.requirements.clear();
            for index in 0..1000 {
                definition.features.push(FeatureDefinition {
                    id: format!("folded-{index}"),
                    operation: FeatureOperation::SheetMetal {
                        definition: Box::new(sheet.clone()),
                    },
                });
                definition.features.push(FeatureDefinition {
                    id: format!("flat-{index}"),
                    operation: FeatureOperation::SheetMetalFlat {
                        input: format!("folded-{index}"),
                        neutral_factor: ScalarExpr::Literal(Quantity::scalar(0.5)),
                    },
                });
            }
            let mut part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            part.overrides.insert(
                "thickness".into(),
                ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
            );
            let second = part.regenerate_incremental(&session, &first)?;
            let expected = (60.0 + std::f64::consts::PI * 4.5) * 30.0;
            for index in 0..1000 {
                for kind in ["folded", "flat"] {
                    let shape = second
                        .shape(&format!("{kind}-{index}"))
                        .ok_or_else(|| failure(format!("missing {kind}-{index}")))?;
                    let volume = session
                        .volume(shape)
                        .map_err(|error| failure(error.to_string()))?;
                    if (volume - expected).abs() > 1e-6 {
                        return Err(failure("sheet analytic volume mismatch".into()));
                    }
                }
            }
            if second.regeneration.rebuilt.len() != 2000 || !second.regeneration.reused.is_empty() {
                return Err(failure("linked flat patterns were not rebuilt".into()));
            }
            drop((first, second));
            if session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?
                != 0
            {
                return Err(failure("sheet handles retained".into()));
            }
            Ok("1000 folded brackets and linked blanks; exact volumes after thickness edit; no retained handles".into())
        },
    )
}
