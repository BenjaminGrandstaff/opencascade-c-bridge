//! Modeled threads at scale: a 100-turn thread, a 100-turn tapped hole, and
//! 10,000 pattern members of a threaded rod sharing one generated variant.

use super::*;

fn threaded_rod(definition: &FamilyDefinition, turns: f64) -> FamilyDefinition {
    threaded_rod_with_runout(definition, turns, None)
}

fn threaded_rod_with_runout(
    definition: &FamilyDefinition,
    turns: f64,
    runout: Option<f64>,
) -> FamilyDefinition {
    let mm = |x: f64, y: f64, z: f64| {
        VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
    };
    let length = |value: f64| ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter));
    let mut definition = definition.clone();
    definition.features = vec![
        FeatureDefinition {
            id: "rod".into(),
            operation: FeatureOperation::Cylinder {
                origin: mm(0.0, 0.0, 0.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: length(5.0),
                height: length(turns * 1.5 + 10.0),
            },
        },
        FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Thread {
                input: "rod".into(),
                origin: mm(0.0, 0.0, 5.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                major_diameter: length(10.0),
                pitch: length(1.5),
                length: length(turns * 1.5),
                internal: false,
                left_handed: false,
                runout: runout.map(length),
            },
        },
    ];
    definition
}

pub(crate) fn thread_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        "threads: 100-turn M10x1.5 external thread".into(),
        Duration::from_secs(12),
        Expectation::Required,
        || {
            let definition = threaded_rod(definition, 100.0);
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let generation = graph.regenerate_all(&session)?;
            let body = generation
                .result("part")
                .and_then(|result| result.shape("body"))
                .ok_or_else(|| failure("threaded output missing".into()))?;
            let valid = session
                .is_valid(body)
                .map_err(|error| failure(error.to_string()))?;
            drop(generation);
            let retained = session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?;
            if !valid || retained != 0 {
                return Err(failure(format!(
                    "valid {valid}, {retained} retained handles"
                )));
            }
            Ok("valid 100-turn threaded rod, no retained handles".into())
        },
    )
}

pub(crate) fn thread_pattern_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        "threads: 10000 pattern members of a threaded rod".into(),
        Duration::from_secs(4),
        Expectation::Required,
        || {
            let definition = threaded_rod(definition, 10.0);
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            graph.add_linear_pattern(
                "row",
                "member",
                "part",
                10_000,
                VectorQuantity::lengths(20.0, 0.0, 0.0, LengthUnit::Millimeter),
                "bench",
            )?;
            let generation = graph.regenerate_all(&session)?;
            let variants = generation.generated_variants();
            drop(generation);
            let retained = session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?;
            if variants != 1 || retained != 0 {
                return Err(failure(format!(
                    "{variants} variants, {retained} retained handles"
                )));
            }
            Ok("10001 instances, one threaded variant, no retained handles".into())
        },
    )
}

/// A 100-turn M10x1.5 tapped through hole in a 160 mm block: the groove is
/// fused into the hole cutter and removed by one boolean cut.
pub(crate) fn tapped_hole_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        "threads: 100-turn M10x1.5 tapped through hole".into(),
        Duration::from_secs(12),
        Expectation::Required,
        || {
            let mm = |x: f64, y: f64, z: f64| {
                VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
            };
            let length =
                |value: f64| ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter));
            let mut definition = definition.clone();
            definition.features = vec![
                FeatureDefinition {
                    id: "block".into(),
                    operation: FeatureOperation::Box {
                        origin: mm(-10.0, -10.0, 0.0),
                        size: mm(20.0, 20.0, 150.0),
                    },
                },
                FeatureDefinition {
                    id: "body".into(),
                    operation: FeatureOperation::Hole {
                        input: "block".into(),
                        position: mm(0.0, 0.0, 150.0),
                        axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
                        diameter: length(8.5),
                        extent: HoleExtent::ThroughAll,
                        bottom: HoleBottom::Flat,
                        finish: HoleFinish::Plain,
                        thread: Some(Box::new(ThreadSpecification {
                            designation: "M10x1.5".into(),
                            nominal_diameter: length(10.0),
                            pitch: length(1.5),
                            handedness: ThreadHandedness::Right,
                            modeled: Some(ModeledThread::default()),
                        })),
                    },
                },
            ];
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let generation = graph.regenerate_all(&session)?;
            let body = generation
                .result("part")
                .and_then(|result| result.shape("body"))
                .ok_or_else(|| failure("tapped output missing".into()))?;
            let valid = session
                .is_valid(body)
                .map_err(|error| failure(error.to_string()))?;
            drop(generation);
            let retained = session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?;
            if !valid || retained != 0 {
                return Err(failure(format!(
                    "valid {valid}, {retained} retained handles"
                )));
            }
            Ok("valid 100-turn tapped hole, no retained handles".into())
        },
    )
}

/// The 100-turn rod thread ending in the longest allowed runout (10 pitches),
/// whose spiral path adds at most 161 interpolation points.
pub(crate) fn thread_runout_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        "threads: 100-turn M10x1.5 thread with a 10-pitch runout".into(),
        Duration::from_secs(12),
        Expectation::Required,
        || {
            let definition = threaded_rod_with_runout(definition, 100.0, Some(15.0));
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let generation = graph.regenerate_all(&session)?;
            let body = generation
                .result("part")
                .and_then(|result| result.shape("body"))
                .ok_or_else(|| failure("runout output missing".into()))?;
            let valid = session
                .is_valid(body)
                .map_err(|error| failure(error.to_string()))?;
            drop(generation);
            let retained = session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?;
            if !valid || retained != 0 {
                return Err(failure(format!(
                    "valid {valid}, {retained} retained handles"
                )));
            }
            Ok("valid 100-turn thread with runout, no retained handles".into())
        },
    )
}
