//! Profile sweeps along sketch paths at scale.

use super::*;
use occt_parametric::{GeneratedResult, SketchArc, SweepOrientation};

fn fixed_point(id: &str, x: ScalarExpr, y: ScalarExpr) -> SketchPoint {
    SketchPoint {
        id: id.into(),
        x,
        y,
        fixed: true,
    }
}

fn sketch(id: &str, x_axis: (f64, f64, f64), y_axis: (f64, f64, f64)) -> SketchDefinition {
    SketchDefinition {
        id: id.into(),
        datum_plane: None,
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(x_axis.0, x_axis.1, x_axis.2)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(y_axis.0, y_axis.1, y_axis.2)),
        points: Vec::new(),
        lines: Vec::new(),
        circles: Vec::new(),
        arcs: Vec::new(),
        splines: Vec::new(),
        profile: Vec::new(),
        constraints: Vec::new(),
    }
}

/// 1,000 sweeps of a parameter-sized disc along a line and a tangent arc.
pub(crate) fn sweep_case() -> Outcome {
    const COUNT: usize = 1_000;
    timed(
        format!("{COUNT} disc sweeps along a line and arc: build and radius edit"),
        ms(4_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let value = |x| ScalarExpr::Literal(Quantity::length(x, LengthUnit::Millimeter));
            let mut profile = sketch("disc", (0.0, 1.0, 0.0), (0.0, 0.0, 1.0));
            profile.points = vec![
                fixed_point("c", value(0.0), value(0.0)),
                fixed_point("r", ScalarExpr::Parameter("radius".into()), value(0.0)),
            ];
            profile.circles = vec![SketchCircle {
                id: "rim".into(),
                center: "c".into(),
                rim: "r".into(),
            }];
            let mut path = sketch("route", (1.0, 0.0, 0.0), (0.0, 1.0, 0.0));
            path.points = vec![
                fixed_point("a", value(0.0), value(0.0)),
                fixed_point("b", value(10.0), value(0.0)),
                fixed_point("o", value(10.0), value(10.0)),
                fixed_point("e", value(20.0), value(10.0)),
            ];
            path.lines = vec![SketchLine {
                id: "run".into(),
                start: "a".into(),
                end: "b".into(),
            }];
            path.arcs = vec![SketchArc {
                id: "bend".into(),
                center: "o".into(),
                start: "b".into(),
                end: "e".into(),
                clockwise: false,
            }];
            path.profile = vec!["run".into(), "bend".into()];
            let mut definition = block();
            definition.datums.clear();
            definition.requirements.clear();
            definition.parameters = vec![ParameterDefinition {
                id: "radius".into(),
                parameter_type: ParameterType::Scalar(Dimension::Length),
                default: length(2.0),
                minimum: None,
                maximum: None,
            }];
            definition.features = vec![
                FeatureDefinition {
                    id: "profile".into(),
                    operation: FeatureOperation::SketchFace {
                        sketch: Box::new(profile),
                    },
                },
                FeatureDefinition {
                    id: "path".into(),
                    operation: FeatureOperation::SketchOpenWire {
                        sketch: Box::new(path),
                    },
                },
            ];
            definition
                .features
                .extend((0..COUNT).map(|index| FeatureDefinition {
                    id: format!("pipe-{index}"),
                    operation: FeatureOperation::Sweep {
                        profile: "profile".into(),
                        path: "path".into(),
                        orientation: SweepOrientation::CorrectedFrenet,
                    },
                }));
            let mut part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            part.overrides.insert("radius".into(), length(3.0));
            let second = part.regenerate_incremental(&session, &first)?;
            if second.regeneration.rebuilt.len() != COUNT + 1
                || second.regeneration.reused != ["path"]
            {
                return Err(failure(
                    "radius edit did not rebuild exactly the sweeps".into(),
                ));
            }
            let pi = std::f64::consts::PI;
            let expected = pi * 9.0 * (10.0 + pi * 5.0);
            let check = |result: &GeneratedResult<'_>| -> Result<(), ModelError> {
                for index in [0, COUNT / 2, COUNT - 1] {
                    let shape = result
                        .shape(&format!("pipe-{index}"))
                        .ok_or_else(|| failure("pipe missing".into()))?;
                    let volume = session.volume(shape)?;
                    if (volume - expected).abs() > 1e-6 * expected {
                        return Err(failure(format!(
                            "pipe volume {volume}, expected {expected}"
                        )));
                    }
                }
                Ok(())
            };
            check(&second)?;
            drop((first, second));
            if session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?
                != 0
            {
                return Err(failure("sweep handles retained".into()));
            }
            Ok(format!(
                "{COUNT} sweeps rebuilt after a radius edit; Pappus volumes exact; path reused"
            ))
        },
    )
}
