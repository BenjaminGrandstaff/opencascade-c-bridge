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
        measurements: Vec::new(),
        shapes: Vec::new(),
        regions: Vec::new(),
        region_discovery: None,
        curve_operations: Vec::new(),
        region_edits: Vec::new(),
        texts: Vec::new(),
        acceptance: Default::default(),
        id: id.into(),
        datum_plane: None,
        face_support: None,
        projections: Vec::new(),
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
        ellipses: Vec::new(),
        profile_operations: Vec::new(),
        splines: Vec::new(),
        profile: Vec::new(),
        holes: Vec::new(),
        patterns: Vec::new(),
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
                        twist: None,
                        scale: None,
                        guide: None,
                        sections: Vec::new(),
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

/// 250 twisted ducts flaring between square sections on planes normal to
/// one constrained route (the advanced-sweep example), built and then
/// rebuilt after the route's run changes, which moves the outlet plane.
/// Measured 5.9 s: about 11 ms per duct each time, of which the twist's
/// auxiliary-spine pipe surface takes about 7 ms.
pub(crate) fn advanced_sweep_case() -> Outcome {
    const COUNT: usize = 250;
    timed(
        format!(
            "{COUNT} twisted, sectioned duct sweeps on a constrained route: build and route edit"
        ),
        ms(10_000),
        Expectation::Required,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../../tools/model/advanced-sweep.request.json"
            ))
            .map_err(|error| failure(error.to_string()))?;
            let mut document = ModelDocument::from_json(&request["model"].to_string())?;
            let template = document
                .family
                .features
                .iter()
                .find(|feature| feature.id == "duct")
                .cloned()
                .ok_or_else(|| failure("duct missing".into()))?;
            for index in 1..COUNT {
                let mut duct = template.clone();
                duct.id = format!("duct-{index}");
                if let FeatureOperation::Sweep {
                    twist: Some(twist), ..
                } = &mut duct.operation
                {
                    // Quarter turns keep every duct's volume the same.
                    twist.angle_radians = ScalarExpr::Literal(Quantity::scalar(
                        std::f64::consts::FRAC_PI_2 * (index % 8) as f64,
                    ));
                }
                document.family.features.push(duct);
            }
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut part = PartInstance {
                id: "duct".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let started = Instant::now();
            let first = part.regenerate(&session)?;
            let build = started.elapsed();
            part.overrides.insert(
                "run".into(),
                ParameterValue::Scalar(Quantity::length(60.0, LengthUnit::Millimeter)),
            );
            let started = Instant::now();
            let second = part.regenerate_incremental(&session, &first)?;
            let edit = started.elapsed();
            if second.regeneration.rebuilt.len() != COUNT + 3 {
                return Err(failure(format!(
                    "route edit rebuilt {:?}",
                    second.regeneration.rebuilt.len()
                )));
            }
            let pi = std::f64::consts::PI;
            let expected = (60.0 + 30.0 * pi / 2.0) * (64.0 + 112.0 + 196.0) / 3.0;
            for index in [0, COUNT / 2, COUNT - 1] {
                let id = if index == 0 {
                    "duct".to_owned()
                } else {
                    format!("duct-{index}")
                };
                let shape = second
                    .shape(&id)
                    .ok_or_else(|| failure("duct missing".into()))?;
                let volume = session.volume(shape)?;
                if (volume - expected).abs() > 1e-5 * expected || !session.is_valid(shape)? {
                    return Err(failure(format!(
                        "{id} volume {volume}, expected {expected}"
                    )));
                }
            }
            drop((first, second));
            if session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?
                != 0
            {
                return Err(failure("sweep handles retained".into()));
            }
            Ok(format!(
                "built in {:.2} s, rebuilt after the route edit in {:.2} s; volumes exact",
                build.as_secs_f64(),
                edit.as_secs_f64()
            ))
        },
    )
}
