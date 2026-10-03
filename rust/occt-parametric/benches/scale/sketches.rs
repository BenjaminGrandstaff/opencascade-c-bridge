//! Sketch solving, datum-linked wires, and profile sweeps at scale.

use super::*;

pub(crate) fn profile_sweep_case(revolve: bool) -> Outcome {
    const COUNT: usize = 1_000;
    let value = |x| ScalarExpr::Literal(Quantity::length(x, LengthUnit::Millimeter));
    let mut definition = block();
    definition.datums.clear();
    definition.features.clear();
    let sketch = SketchDefinition {
        id: "circle".into(),
        datum_plane: None,
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(
            0.0,
            if revolve { 0.0 } else { 1.0 },
            if revolve { 1.0 } else { 0.0 },
        )),
        points: vec![
            SketchPoint {
                id: "c".into(),
                x: value(3.0),
                y: value(0.0),
                fixed: true,
            },
            SketchPoint {
                id: "r".into(),
                x: value(4.0),
                y: value(0.0),
                fixed: true,
            },
        ],
        lines: Vec::new(),
        circles: vec![SketchCircle {
            id: "circle".into(),
            center: "c".into(),
            rim: "r".into(),
        }],
        arcs: Vec::new(),
        profile: Vec::new(),
        constraints: Vec::new(),
    };
    if revolve {
        definition.parameters.push(ParameterDefinition {
            id: "angle".into(),
            parameter_type: ParameterType::Scalar(Dimension::Scalar),
            default: ParameterValue::Scalar(Quantity::scalar(std::f64::consts::PI)),
            minimum: None,
            maximum: None,
        });
    }
    definition.features.push(FeatureDefinition {
        id: "profile".into(),
        operation: FeatureOperation::SketchWire {
            sketch: Box::new(sketch),
        },
    });
    for index in 0..COUNT {
        let operation = if revolve {
            FeatureOperation::Revolve {
                input: "profile".into(),
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                angle_radians: ScalarExpr::Parameter("angle".into()),
            }
        } else {
            FeatureOperation::Extrude {
                input: "profile".into(),
                direction: VectorExpr::Components {
                    x: value(0.0),
                    y: value(0.0),
                    z: ScalarExpr::Parameter("height".into()),
                },
            }
        };
        definition.features.push(FeatureDefinition {
            id: format!("solid{index}"),
            operation,
        });
    }
    let name = if revolve { "revolve" } else { "extrude" };
    timed(
        format!("{COUNT} {name} features: build and edit"),
        Duration::from_secs(5),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let mut part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            let (parameter, value, expected_volume) = if revolve {
                (
                    "angle",
                    Quantity::scalar(std::f64::consts::TAU),
                    6.0 * std::f64::consts::PI.powi(2),
                )
            } else {
                (
                    "height",
                    Quantity::length(60.0, LengthUnit::Millimeter),
                    60.0 * std::f64::consts::PI,
                )
            };
            part.overrides
                .insert(parameter.into(), ParameterValue::Scalar(value));
            let edited = part.regenerate_incremental(&session, &first)?;
            if edited.regeneration.rebuilt.len() != COUNT
                || edited.regeneration.reused != ["profile"]
            {
                return Err(failure("sweep edit did not reuse only its profile".into()));
            }
            for index in 0..COUNT {
                let solid = edited
                    .shape(&format!("solid{index}"))
                    .ok_or_else(|| failure("sweep output is missing".into()))?;
                if (session.volume(solid)? - expected_volume).abs() > 1e-7 {
                    return Err(failure("sweep volume differs after edit".into()));
                }
            }
            drop((first, edited));
            if session.shape_count()? != 0 {
                return Err(failure("sweep handles were retained".into()));
            }
            Ok(format!(
                "{COUNT} exact solids rebuilt; profile reused; handles released"
            ))
        },
    )
}

pub(crate) fn datum_sketch_wire_case() -> Outcome {
    const COUNT: usize = 10_000;
    let value = |x| ScalarExpr::Literal(Quantity::length(x, LengthUnit::Millimeter));
    let mut sketch = SketchDefinition {
        id: "circle".into(),
        datum_plane: None,
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        points: vec![
            SketchPoint {
                id: "c".into(),
                x: value(0.0),
                y: value(0.0),
                fixed: true,
            },
            SketchPoint {
                id: "r".into(),
                x: value(2.0),
                y: value(0.0),
                fixed: true,
            },
        ],
        lines: Vec::new(),
        circles: vec![SketchCircle {
            id: "circle".into(),
            center: "c".into(),
            rim: "r".into(),
        }],
        arcs: Vec::new(),
        profile: Vec::new(),
        constraints: Vec::new(),
    };
    let mut definition = block();
    definition.features.clear();
    definition.datums.clear();
    for index in 0..COUNT {
        let datum = format!("plane{index}");
        definition.datums.push(DatumDefinition {
            id: datum.clone(),
            kind: DatumKind::Plane {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    index as f64,
                    LengthUnit::Millimeter,
                )),
                normal: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            },
        });
        sketch.datum_plane = Some(datum);
        definition.features.push(FeatureDefinition {
            id: format!("wire{index}"),
            operation: FeatureOperation::SketchWire {
                sketch: Box::new(sketch.clone()),
            },
        });
    }
    timed(
        format!("{COUNT} datum-linked sketch wires: regenerate"),
        Duration::from_secs(5),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let generated = part.regenerate(&session)?;
            if generated.regeneration.rebuilt.len() != COUNT || session.shape_count()? != COUNT {
                return Err(failure(
                    "datum-linked wire output or handle count differs".into(),
                ));
            }
            let last = generated
                .shape(&format!("wire{}", COUNT - 1))
                .ok_or_else(|| failure("last datum-linked wire is missing".into()))?;
            if session.shape_type(last)? != ShapeType::Wire
                || (session.bounds(last)?.min.z - (COUNT - 1) as f64).abs() > 1e-6
            {
                return Err(failure(
                    "datum-linked wire is not on its named plane".into(),
                ));
            }
            drop(generated);
            if session.shape_count()? != 0 {
                return Err(failure("datum-linked wire handles were retained".into()));
            }
            Ok(format!("{COUNT} indexed plane lookups; handles released"))
        },
    )
}

pub(crate) fn curved_sketch_solver_case() -> Outcome {
    const SOLVES: usize = 10_000;
    let value = |x| ScalarExpr::Literal(Quantity::length(x, LengthUnit::Millimeter));
    let point = |id: &str, x, y, fixed| SketchPoint {
        id: id.into(),
        x: value(x),
        y: value(y),
        fixed,
    };
    let sketch = SketchDefinition {
        id: "curved".into(),
        datum_plane: None,
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        points: vec![
            point("c", 0.0, 0.0, true),
            point("a", 2.0, 0.0, true),
            point("b", 0.0, 3.0, false),
            point("tip", 2.5, 3.0, false),
        ],
        lines: vec![SketchLine {
            id: "line".into(),
            start: "a".into(),
            end: "tip".into(),
        }],
        arcs: vec![SketchArc {
            id: "arc".into(),
            center: "c".into(),
            start: "a".into(),
            end: "b".into(),
            clockwise: false,
        }],
        circles: Vec::new(),
        profile: Vec::new(),
        constraints: vec![
            SketchConstraint::Tangent {
                first: "arc".into(),
                second: "line".into(),
                point: "a".into(),
            },
            SketchConstraint::Distance {
                first: "a".into(),
                second: "tip".into(),
                value: value(3.0),
            },
        ],
    };
    timed(
        format!("solve {SOLVES} arc/tangent sketches"),
        Duration::from_secs(2),
        Expectation::Required,
        || {
            for _ in 0..SOLVES {
                let solution = sketch.solve(&HashMap::new())?;
                if !solution.solved
                    || solution.free_degrees != 1
                    || (solution.points["b"].y - 2.0).abs() > 1e-8
                    || (solution.points["tip"].x - 2.0).abs() > 1e-8
                {
                    return Err(failure(format!(
                        "unexpected curved sketch solution: {solution:?}"
                    )));
                }
            }
            Ok(format!(
                "{SOLVES} solved with exact arc radii and tangent contact"
            ))
        },
    )
}

pub(crate) fn sketch_solver_case() -> Outcome {
    const SOLVES: usize = 10_000;
    let value =
        |millimeters| ScalarExpr::Literal(Quantity::length(millimeters, LengthUnit::Millimeter));
    let sketch = SketchDefinition {
        id: "bench-line".into(),
        datum_plane: None,
        circles: Vec::new(),
        arcs: Vec::new(),
        profile: Vec::new(),
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        points: vec![
            SketchPoint {
                id: "fixed".into(),
                x: value(0.0),
                y: value(0.0),
                fixed: true,
            },
            SketchPoint {
                id: "free".into(),
                x: value(9.0),
                y: value(1.0),
                fixed: false,
            },
        ],
        lines: vec![SketchLine {
            id: "line".into(),
            start: "fixed".into(),
            end: "free".into(),
        }],
        constraints: vec![
            SketchConstraint::Horizontal {
                line: "line".into(),
            },
            SketchConstraint::Distance {
                first: "fixed".into(),
                second: "free".into(),
                value: value(10.0),
            },
        ],
    };
    timed(
        format!("solve {SOLVES} small constrained sketches"),
        ms(2_000),
        Expectation::Required,
        || {
            for _ in 0..SOLVES {
                let solution = sketch.solve(&HashMap::new())?;
                if !solution.solved || (solution.points["free"].x - 10.0).abs() > 1e-8 {
                    return Err(failure("sketch did not reach its dimension".into()));
                }
            }
            Ok(format!("{SOLVES} solved"))
        },
    )
}

/// One large sketch, either independent line components or a connected
/// chain. Exercises sparse storage, local derivatives, rank, and elimination.
pub(crate) fn large_sketch_case(chain: bool) -> Outcome {
    let count = if chain { 1_000 } else { 10_000 };
    let value = |x| ScalarExpr::Literal(Quantity::length(x, LengthUnit::Millimeter));
    let mut sketch = SketchDefinition {
        id: "large".into(),
        datum_plane: None,
        circles: Vec::new(),
        arcs: Vec::new(),
        profile: Vec::new(),
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        points: Vec::new(),
        lines: Vec::new(),
        constraints: Vec::new(),
    };
    sketch.points.push(SketchPoint {
        id: "p0".into(),
        x: value(0.0),
        y: value(0.0),
        fixed: true,
    });
    for index in 1..=count {
        let end = format!("p{index}");
        let start = if chain {
            format!("p{}", index - 1)
        } else {
            "p0".into()
        };
        let line = format!("l{index}");
        sketch.points.push(SketchPoint {
            id: end.clone(),
            x: value(if chain { index as f64 * 9.0 } else { 9.0 }),
            y: value(1.0),
            fixed: false,
        });
        sketch.lines.push(SketchLine {
            id: line.clone(),
            start: start.clone(),
            end: end.clone(),
        });
        sketch
            .constraints
            .push(SketchConstraint::Horizontal { line });
        sketch.constraints.push(SketchConstraint::Distance {
            first: start,
            second: end,
            value: value(10.0),
        });
    }
    timed(
        format!(
            "one sketch: {count} {} lines",
            if chain { "connected" } else { "independent" }
        ),
        ms(5_000),
        Expectation::Required,
        || {
            let solution = sketch.solve(&HashMap::new())?;
            let target = if chain { count as f64 * 10.0 } else { 10.0 };
            let last = solution.points[&format!("p{count}")];
            if !solution.solved
                || solution.free_degrees != 0
                || solution.redundant_equations != 0
                || (last.x - target).abs() > 1e-6
                || last.y.abs() > 1e-8
            {
                return Err(failure(format!(
                    "residual {}, freedoms {}, endpoint {last:?}",
                    solution.max_residual, solution.free_degrees
                )));
            }
            Ok(format!(
                "{} iterations; {} free coordinates",
                solution.iterations,
                count * 2
            ))
        },
    )
}
