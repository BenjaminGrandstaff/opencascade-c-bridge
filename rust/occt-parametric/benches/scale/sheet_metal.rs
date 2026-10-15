use super::*;
use occt_parametric::{
    BendTable, BendTableKind, SheetMetalBend, SheetMetalCutout, SheetMetalCutoutShape,
    SheetMetalDefinition,
};
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
                cutouts: Vec::new(),
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
                        bend_table: None,
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

/// One sheet of ten 200 mm flanges and nine bends, 50 mm wide, with 100
/// Ø4 holes in each flange: 1,000 cutouts cut from the folded sheet and its
/// blank, each in one boolean.
pub(crate) fn cutout_case() -> Outcome {
    timed(
        "sheet metal: 1000 cutouts through 10 flanges, folded and flat".into(),
        ms(10_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let length =
                |value| ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter));
            let cutouts = (0..1_000)
                .map(|i| SheetMetalCutout {
                    id: format!("hole-{i}"),
                    flange: i / 100,
                    along: length(5.0 + 9.5 * ((i % 100) / 5) as f64),
                    across: length(5.0 + 10.0 * (i % 5) as f64),
                    shape: SheetMetalCutoutShape::Circle {
                        diameter: length(4.0),
                    },
                })
                .collect();
            let sheet = SheetMetalDefinition {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                width_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
                start_direction: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                width: length(50.0),
                thickness: length(2.0),
                flanges: vec![length(200.0); 10],
                bends: (0..9)
                    .map(|i| SheetMetalBend {
                        angle_radians: ScalarExpr::Literal(Quantity::scalar(if i % 2 == 0 {
                            std::f64::consts::FRAC_PI_2
                        } else {
                            -std::f64::consts::FRAC_PI_2
                        })),
                        inside_radius: length(3.0),
                    })
                    .collect(),
                cutouts,
            };
            let mut definition = block();
            definition.features = vec![
                FeatureDefinition {
                    id: "folded".into(),
                    operation: FeatureOperation::SheetMetal {
                        definition: Box::new(sheet),
                    },
                },
                FeatureDefinition {
                    id: "flat".into(),
                    operation: FeatureOperation::SheetMetalFlat {
                        input: "folded".into(),
                        neutral_factor: ScalarExpr::Literal(Quantity::scalar(0.5)),
                        bend_table: None,
                    },
                },
            ];
            definition.datums.clear();
            definition.requirements.clear();
            let part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let generated = part.regenerate(&session)?;
            let pi = std::f64::consts::PI;
            let strip = (2000.0 + 9.0 * pi / 2.0 * 4.0) * 50.0 * 2.0;
            let expected = strip - 1_000.0 * pi * 4.0 * 2.0;
            for id in ["folded", "flat"] {
                let shape = generated
                    .shape(id)
                    .ok_or_else(|| failure(format!("missing {id}")))?;
                let volume = session
                    .volume(shape)
                    .map_err(|error| failure(error.to_string()))?;
                if (volume - expected).abs() > 1e-4 * expected {
                    return Err(failure(format!(
                        "{id} volume {volume}, expected {expected}"
                    )));
                }
            }
            Ok("1000 holes cut from folded sheet and blank; exact removed volume".into())
        },
    )
}

/// A 500 mm square face with the API's maximum 10,000 Ø2 holes on a 5 mm
/// grid, built without booleans. Its checks are box-filtered, so the cost
/// stays near linear in the holes.
pub(crate) fn face_holes_case() -> Outcome {
    timed(
        "face with 10000 holes built without booleans".into(),
        ms(5_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let point = |x: f64, y: f64| Vec3::new(x, y, 0.0);
            let line = |a: Vec3, b: Vec3| occt_bridge::WireSegment::Line { start: a, end: b };
            let outer = session.create_segment_wire(
                &[
                    line(point(0.0, 0.0), point(500.0, 0.0)),
                    line(point(500.0, 0.0), point(500.0, 500.0)),
                    line(point(500.0, 500.0), point(0.0, 500.0)),
                    line(point(0.0, 500.0), point(0.0, 0.0)),
                ],
                true,
            )?;
            let holes = (0..10_000)
                .map(|i| {
                    let (x, y) = (2.5 + 5.0 * (i % 100) as f64, 2.5 + 5.0 * (i / 100) as f64);
                    session.create_segment_wire(
                        &[
                            occt_bridge::WireSegment::Arc {
                                start: point(x + 1.0, y),
                                middle: point(x, y + 1.0),
                                end: point(x - 1.0, y),
                            },
                            occt_bridge::WireSegment::Arc {
                                start: point(x - 1.0, y),
                                middle: point(x, y - 1.0),
                                end: point(x + 1.0, y),
                            },
                        ],
                        true,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let face = session.create_face_with_holes(&outer, &holes.iter().collect::<Vec<_>>())?;
            let area = session.surface_area(&face)?;
            let expected = 250_000.0 - 10_000.0 * std::f64::consts::PI;
            if (area - expected).abs() > 1e-6 * expected {
                return Err(failure(format!("area {area}, expected {expected}")));
            }
            Ok("10000 holes; exact remaining area".into())
        },
    )
}

/// A 1,024-flange blank through a full 256 × 256 bend table, 100 times.
/// The table is linear in radius and angle, so bilinear interpolation must
/// reproduce it exactly at every bend.
pub(crate) fn bend_table_case() -> Outcome {
    timed(
        "sheet metal: 100 blanks of 1023 bends through a 256x256 bend table".into(),
        ms(1_000),
        Expectation::Required,
        || {
            let length =
                |value| ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter));
            let value = |radius: f64, degrees: f64| 1.0 + 0.5 * radius + 0.02 * degrees;
            let radii: Vec<f64> = (1..=256).map(|i| i as f64 * 0.1).collect();
            let angles: Vec<f64> = (1..=256).map(|i| i as f64 * 0.7).collect();
            let table = BendTable {
                name: "bench".into(),
                kind: BendTableKind::Allowance,
                thickness_mm: 1.0,
                values_mm: radii
                    .iter()
                    .map(|r| angles.iter().map(|a| value(*r, *a)).collect())
                    .collect(),
                inside_radii_mm: radii,
                angles_degrees: angles,
            };
            let bends: Vec<(f64, f64)> = (0..1023)
                .map(|i| {
                    let degrees = 1.0 + (i % 177) as f64;
                    let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
                    (sign * degrees, 0.15 + (i % 250) as f64 * 0.1)
                })
                .collect();
            let sheet = SheetMetalDefinition {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                width_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                start_direction: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                width: length(10.0),
                thickness: length(1.0),
                flanges: vec![length(1.0); 1024],
                bends: bends
                    .iter()
                    .map(|(degrees, radius)| SheetMetalBend {
                        angle_radians: ScalarExpr::Literal(Quantity::scalar(degrees.to_radians())),
                        inside_radius: length(*radius),
                    })
                    .collect(),
                cutouts: Vec::new(),
            };
            let expected = 1024.0
                + bends
                    .iter()
                    .map(|(degrees, radius)| value(*radius, degrees.abs()))
                    .sum::<f64>();
            for _ in 0..100 {
                let metrics = sheet.flat_pattern_with_table(&HashMap::new(), 0.5, &table)?;
                if (metrics.length_mm - expected).abs() > 1e-6 {
                    return Err(failure(format!(
                        "bend-table blank length {} != {expected}",
                        metrics.length_mm
                    )));
                }
            }
            Ok("1023 table bends per blank; exact bilinear lengths".into())
        },
    )
}
