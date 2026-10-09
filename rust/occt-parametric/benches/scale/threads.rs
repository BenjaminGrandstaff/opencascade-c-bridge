//! Modeled threads at scale: a 100-turn thread, and 10,000 pattern members
//! of a threaded rod sharing one generated variant.

use super::*;

fn threaded_rod(definition: &FamilyDefinition, turns: f64) -> FamilyDefinition {
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
