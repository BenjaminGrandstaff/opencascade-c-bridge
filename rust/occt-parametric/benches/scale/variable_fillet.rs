use super::*;

pub(super) fn variable_fillet_features_case() -> Outcome {
    fillet_case(false)
}
pub(super) fn station_fillet_features_case() -> Outcome {
    fillet_case(true)
}
fn fillet_case(stations: bool) -> Outcome {
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
        id: "end_radius".into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: length(2.0),
        minimum: None,
        maximum: None,
    });
    for index in 0..COUNT {
        definition.features.push(FeatureDefinition {
            id: format!("blend{index}"),
            operation: FeatureOperation::VariableFillet {
                input: "body".into(),
                edges: vec![occt_parametric::EdgeSelector::NearestCenter {
                    target: point(0.0, 0.0, 5.0),
                    maximum_distance: ScalarExpr::Literal(Quantity::length(
                        1e-6,
                        LengthUnit::Millimeter,
                    )),
                }],
                start_radius: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
                end_radius: ScalarExpr::Parameter("end_radius".into()),
                stations: if stations {
                    vec![occt_parametric::FilletRadiusStation {
                        position: ScalarExpr::Literal(Quantity::scalar(0.5)),
                        radius: ScalarExpr::Parameter("end_radius".into()),
                    }]
                } else {
                    Vec::new()
                },
                spine_direction: if stations {
                    occt_parametric::FilletSpineDirection::FromPoint {
                        point: point(0.0, 0.0, 0.0),
                    }
                } else {
                    occt_parametric::FilletSpineDirection::Kernel
                },
            },
        });
    }
    timed(
        if stations {
            "1000 multi-station fillet features: build and edit"
        } else {
            "1000 variable fillet features: build and edit"
        }
        .into(),
        // Measured 18 s for 2,000 OCCT builds plus validation; keep roughly
        // 3x headroom, matching the suite's policy for new benchmark budgets.
        ms(60_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let mut part = PartInstance {
                id: "blends".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            part.overrides.insert("end_radius".into(), length(3.0));
            let edited = part.regenerate_incremental(&session, &first)?;
            if edited.regeneration.reused != ["body"] || edited.regeneration.rebuilt.len() != COUNT
            {
                return Err(failure("fillet edit did not reuse only its input".into()));
            }
            let constant_volume =
                |r: f64| 1000.0 - (1.0 - std::f64::consts::FRAC_PI_4) * 10.0 * r * r;
            for index in 0..COUNT {
                let id = format!("blend{index}");
                let previous = first
                    .shape(&id)
                    .ok_or_else(|| failure("fillet output missing".into()))?;
                let changed = edited
                    .shape(&id)
                    .ok_or_else(|| failure("edited fillet output missing".into()))?;
                let previous_volume = session.volume(previous)?;
                let changed_volume = session.volume(changed)?;
                if !(constant_volume(if stations { 3.0 } else { 2.0 })..constant_volume(1.0))
                    .contains(&previous_volume)
                    || !(constant_volume(if stations { 4.0 } else { 3.0 })..previous_volume)
                        .contains(&changed_volume)
                    || !session.is_valid(previous)?
                    || !session.is_valid(changed)?
                {
                    return Err(failure(
                        "fillet volume bounds, radius edit, or validity differ".into(),
                    ));
                }
            }
            if (session.volume(
                first
                    .shape("body")
                    .ok_or_else(|| failure("input missing".into()))?,
            )? - 1000.0)
                .abs()
                > 1e-6
            {
                return Err(failure("fillet input changed".into()));
            }
            drop((first, edited));
            if session.shape_count()? != 0 {
                return Err(failure("fillet handles retained".into()));
            }
            Ok("1000 variable blends rebuilt; input reused; volume bounds checked; handles released".into())
        },
    )
}
