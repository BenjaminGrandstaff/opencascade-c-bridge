//! Many-hole feature-chain construction, editing, and handle cleanup.

use super::*;

pub(super) fn clearance_catalog_case() -> Outcome {
    timed(
        "100000 clearance catalog lookups".into(),
        ms(200),
        Expectation::Required,
        || {
            for index in 0..100_000 {
                let (series, expected) = [
                    (ClearanceSeries::Fine, 6.4),
                    (ClearanceSeries::Medium, 6.6),
                    (ClearanceSeries::Coarse, 7.0),
                ][index % 3];
                let nominal = std::hint::black_box(Quantity::length(0.006, LengthUnit::Meter));
                let diameter = iso273_clearance_v1(nominal, std::hint::black_box(series))?;
                if diameter != Quantity::length(expected, LengthUnit::Millimeter) {
                    return Err(failure("clearance catalog diameter differs".into()));
                }
            }
            Ok("all three series; meter-to-mm conversion; exact diameters".into())
        },
    )
}

/// Worst-case rows: the last entries of the longest inch tables.
pub(super) fn carr_lane_catalog_case() -> Outcome {
    timed(
        "100000 Carr Lane tap and socket-head lookups".into(),
        ms(200),
        Expectation::Required,
        || {
            let inch = |value| std::hint::black_box(Quantity::length(value, LengthUnit::Inch));
            for index in 0..100_000 {
                if index % 2 == 0 {
                    let drill = carr_lane_tap_drill_v1(
                        HoleCatalogSystem::Inch,
                        inch(1.125),
                        inch(1.0 / 12.0),
                    )?;
                    if (drill.value - 1.046875 * 25.4).abs() > 1e-12 {
                        return Err(failure("tap drill differs".into()));
                    }
                } else {
                    let recess = carr_lane_socket_head_v1(HoleCatalogSystem::Inch, inch(2.0))?;
                    if (recess.counterbore_diameter.value - 3.125 * 25.4).abs() > 1e-12 {
                        return Err(failure("socket-head recess differs".into()));
                    }
                }
            }
            Ok("last inch tap and recess rows; inch-to-mm conversion; exact values".into())
        },
    )
}

pub(super) fn hole_features_case() -> Outcome {
    hole_case(HoleFinish::Plain, "hole", 10_000, None)
}

pub(super) fn threaded_hole_features_case() -> Outcome {
    hole_case(
        HoleFinish::Plain,
        "thread-recorded hole",
        10_000,
        Some(Box::new(ThreadSpecification {
            designation: "custom internal thread".into(),
            nominal_diameter: ScalarExpr::Literal(Quantity::length(1.5, LengthUnit::Millimeter)),
            pitch: ScalarExpr::Parameter("thread_pitch".into()),
            handedness: ThreadHandedness::Right,
        })),
    )
}

pub(super) fn entry_hole_features_case(sink: bool) -> Outcome {
    let diameter = ScalarExpr::Literal(Quantity::length(1.6, LengthUnit::Millimeter));
    let finish = if sink {
        HoleFinish::Countersink {
            diameter,
            angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
        }
    } else {
        HoleFinish::Counterbore {
            diameter,
            depth: ScalarExpr::Literal(Quantity::length(0.5, LengthUnit::Millimeter)),
        }
    };
    hole_case(
        finish,
        if sink { "countersink" } else { "counterbore" },
        15_000,
        None,
    )
}

fn hole_case(
    finish: HoleFinish,
    label: &str,
    budget_ms: u64,
    thread: Option<Box<ThreadSpecification>>,
) -> Outcome {
    const COUNT: usize = 100;
    let point =
        |x, y, z| VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter));
    let mut definition = block();
    definition.datums.clear();
    if thread.is_some() {
        definition.parameters.push(ParameterDefinition {
            id: "thread_pitch".into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: length(0.25),
            minimum: None,
            maximum: None,
        });
    }
    definition.parameters.push(ParameterDefinition {
        id: "diameter".into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: length(1.0),
        minimum: None,
        maximum: None,
    });
    definition.features[0].operation = FeatureOperation::Box {
        origin: point(0.0, 0.0, 0.0),
        size: point(22.0, 22.0, 2.0),
    };
    for index in 0..COUNT {
        definition.features.push(FeatureDefinition {
            id: format!("hole{index}"),
            operation: FeatureOperation::Hole {
                input: if index == 0 {
                    "body".into()
                } else {
                    format!("hole{}", index - 1)
                },
                position: point(
                    2.0 + (index % 10) as f64 * 2.0,
                    2.0 + (index / 10) as f64 * 2.0,
                    0.0,
                ),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                diameter: ScalarExpr::Parameter("diameter".into()),
                extent: HoleExtent::ThroughAll,
                finish: finish.clone(),
                thread: thread.clone(),
            },
        });
    }
    timed(
        format!("{COUNT} {label} features: build and edit"),
        ms(budget_ms),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let mut part = PartInstance {
                id: "plate".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            let final_id = format!("hole{}", COUNT - 1);
            let initial = first
                .shape(&final_id)
                .ok_or_else(|| failure("last hole output is missing".into()))?;
            let expected = plate_volume(COUNT, 0.5, &finish);
            if (session.volume(initial)? - expected).abs() > 1e-6 {
                return Err(failure("initial many-hole plate volume differs".into()));
            }
            if thread.is_some() {
                part.overrides.insert("thread_pitch".into(), length(0.3));
            } else {
                part.overrides.insert("diameter".into(), length(1.2));
            }
            let edited = part.regenerate_incremental(&session, &first)?;
            if edited.regeneration.reused != ["body"] || edited.regeneration.rebuilt.len() != COUNT
            {
                return Err(failure(
                    "hole edit did not reuse only its input plate".into(),
                ));
            }
            let result = edited
                .shape(&final_id)
                .ok_or_else(|| failure("edited hole output is missing".into()))?;
            let expected = plate_volume(COUNT, if thread.is_some() { 0.5 } else { 0.6 }, &finish);
            if (session.volume(result)? - expected).abs() > 1e-6 || !session.is_valid(result)? {
                return Err(failure("edited many-hole plate geometry differs".into()));
            }
            drop((first, edited));
            if session.shape_count()? != 0 {
                return Err(failure("hole tool/result handles were retained".into()));
            }
            Ok(format!(
                "{COUNT} exact bores rebuilt; body reused; handles released"
            ))
        },
    )
}

fn plate_volume(count: usize, radius: f64, finish: &HoleFinish) -> f64 {
    let entry_radius = 0.8;
    let extra = match finish {
        HoleFinish::Plain => 0.0,
        HoleFinish::Counterbore { .. } => (entry_radius * entry_radius - radius * radius) * 0.5,
        HoleFinish::Countersink { .. } => {
            let depth = entry_radius - radius; // included angle pi/2
            depth * (entry_radius * entry_radius + entry_radius * radius - 2.0 * radius * radius)
                / 3.0
        }
    };
    22.0 * 22.0 * 2.0 - count as f64 * std::f64::consts::PI * (2.0 * radius * radius + extra)
}
