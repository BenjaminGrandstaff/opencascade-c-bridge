//! Sketch solving, datum-linked wires, and profile sweeps at scale.

use super::*;
use occt_parametric::{ExtrudeExtent, GeneratedResult, RevolveExtent, SketchSpline};

pub(crate) fn profile_sweep_case(revolve: bool) -> Outcome {
    const COUNT: usize = 1_000;
    let value = |x| ScalarExpr::Literal(Quantity::length(x, LengthUnit::Millimeter));
    let mut definition = block();
    definition.datums.clear();
    definition.features.clear();
    let sketch = SketchDefinition {
        id: "circle".into(),
        datum_plane: None,
        face_support: None,
        projections: Vec::new(),
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
        ellipses: Vec::new(),
        profile_operations: Vec::new(),
        splines: Vec::new(),
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
                extent: RevolveExtent::Angle,
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
                extent: ExtrudeExtent::Distance,
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
        face_support: None,
        projections: Vec::new(),
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
        ellipses: Vec::new(),
        profile_operations: Vec::new(),
        splines: Vec::new(),
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
        face_support: None,
        projections: Vec::new(),
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
        ellipses: Vec::new(),
        profile_operations: Vec::new(),
        splines: Vec::new(),
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
        face_support: None,
        projections: Vec::new(),
        circles: Vec::new(),
        arcs: Vec::new(),
        ellipses: Vec::new(),
        profile_operations: Vec::new(),
        splines: Vec::new(),
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
        face_support: None,
        projections: Vec::new(),
        circles: Vec::new(),
        arcs: Vec::new(),
        ellipses: Vec::new(),
        profile_operations: Vec::new(),
        splines: Vec::new(),
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

fn spline_slot(crown: ScalarExpr) -> SketchDefinition {
    let value = |x| ScalarExpr::Literal(Quantity::length(x, LengthUnit::Millimeter));
    let point = |id: &str, x: ScalarExpr, y: ScalarExpr| SketchPoint {
        id: id.into(),
        x,
        y,
        fixed: true,
    };
    let line = |id: &str, start: &str, end: &str| SketchLine {
        id: id.into(),
        start: start.into(),
        end: end.into(),
    };
    let tangent = |first: &str, second: &str, at: &str| SketchConstraint::Tangent {
        first: first.into(),
        second: second.into(),
        point: at.into(),
    };
    SketchDefinition {
        id: "slot".into(),
        datum_plane: None,
        face_support: None,
        projections: Vec::new(),
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        points: vec![
            point("p0", value(0.0), value(0.0)),
            point("p1", value(20.0), value(0.0)),
            point("p2", value(20.0), value(10.0)),
            point("p3", value(0.0), value(10.0)),
            point("crown", value(10.0), crown),
        ],
        lines: vec![
            line("bottom", "p0", "p1"),
            line("right", "p1", "p2"),
            line("left", "p3", "p0"),
        ],
        circles: Vec::new(),
        arcs: Vec::new(),
        ellipses: Vec::new(),
        profile_operations: Vec::new(),
        splines: vec![SketchSpline {
            id: "top".into(),
            points: vec!["p2".into(), "crown".into(), "p3".into()],
        }],
        profile: vec!["bottom".into(), "right".into(), "top".into(), "left".into()],
        constraints: vec![tangent("right", "top", "p2"), tangent("top", "left", "p3")],
    }
}

/// 1,000 spline-topped slot sketches, each extruded, sharing a crown height.
pub(crate) fn spline_sketch_case() -> Outcome {
    const COUNT: usize = 1_000;
    timed(
        format!("{COUNT} spline-arch sketch extrusions: build and crown edit"),
        ms(6_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut definition = block();
            definition.datums.clear();
            definition.requirements.clear();
            definition.parameters = vec![ParameterDefinition {
                id: "crown".into(),
                parameter_type: ParameterType::Scalar(Dimension::Length),
                default: length(15.0),
                minimum: None,
                maximum: None,
            }];
            definition.features = (0..COUNT)
                .flat_map(|index| {
                    [
                        FeatureDefinition {
                            id: format!("slot-{index}"),
                            operation: FeatureOperation::SketchFace {
                                sketch: Box::new(spline_slot(ScalarExpr::Parameter(
                                    "crown".into(),
                                ))),
                            },
                        },
                        FeatureDefinition {
                            id: format!("bar-{index}"),
                            operation: FeatureOperation::Extrude {
                                extent: ExtrudeExtent::Distance,
                                input: format!("slot-{index}"),
                                direction: VectorExpr::Literal(VectorQuantity::lengths(
                                    0.0,
                                    0.0,
                                    5.0,
                                    LengthUnit::Millimeter,
                                )),
                            },
                        },
                    ]
                })
                .collect();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            part.overrides.insert("crown".into(), length(18.0));
            let second = part.regenerate_incremental(&session, &first)?;
            if second.regeneration.rebuilt.len() != 2 * COUNT {
                return Err(failure("crown edit did not rebuild every sketch".into()));
            }
            let volume = |result: &GeneratedResult<'_>| -> Result<f64, ModelError> {
                Ok(session.volume(
                    result
                        .shape("bar-0")
                        .ok_or_else(|| failure("bar missing".into()))?,
                )?)
            };
            let (low, high) = (volume(&first)?, volume(&second)?);
            if !(low > 1_000.0 && high > low + 10.0) {
                return Err(failure(format!("crown edit volumes {low} -> {high}")));
            }
            drop((first, second));
            if session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?
                != 0
            {
                return Err(failure("spline sketch handles retained".into()));
            }
            Ok(format!(
                "{COUNT} tangent spline profiles; crown edit raised volume {low:.1} -> {high:.1} mm³"
            ))
        },
    )
}

/// One closed spline through 1,000 points on a 100 mm circle.
pub(crate) fn large_spline_case() -> Outcome {
    const POINTS: usize = 1_000;
    timed(
        format!("one sketch spline through {POINTS} points: solve and face"),
        ms(250),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut sketch = spline_slot(ScalarExpr::Literal(Quantity::length(
                15.0,
                LengthUnit::Millimeter,
            )));
            sketch.lines.clear();
            sketch.constraints.clear();
            sketch.profile.clear();
            sketch.points = (0..POINTS)
                .map(|index| {
                    let angle = std::f64::consts::TAU * index as f64 / POINTS as f64;
                    SketchPoint {
                        id: format!("q{index}"),
                        x: ScalarExpr::Literal(Quantity::length(
                            100.0 * angle.cos(),
                            LengthUnit::Millimeter,
                        )),
                        y: ScalarExpr::Literal(Quantity::length(
                            100.0 * angle.sin(),
                            LengthUnit::Millimeter,
                        )),
                        fixed: true,
                    }
                })
                .collect();
            sketch.splines = vec![SketchSpline {
                id: "ring".into(),
                points: (0..=POINTS)
                    .map(|index| format!("q{}", index % POINTS))
                    .collect(),
            }];
            let mut definition = block();
            definition.datums.clear();
            definition.requirements.clear();
            definition.features = vec![FeatureDefinition {
                id: "ring".into(),
                operation: FeatureOperation::SketchFace {
                    sketch: Box::new(sketch),
                },
            }];
            let part = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let generated = part.regenerate(&session)?;
            let face = generated
                .shape("ring")
                .ok_or_else(|| failure("ring missing".into()))?;
            let area = session.surface_area(face)?;
            let circle = std::f64::consts::PI * 100.0 * 100.0;
            if (area - circle).abs() / circle > 1e-6 {
                return Err(failure(format!("ring area {area}, circle {circle}")));
            }
            Ok(format!(
                "periodic spline face area within 1e-6 of the circle ({area:.3} mm²)"
            ))
        },
    )
}
