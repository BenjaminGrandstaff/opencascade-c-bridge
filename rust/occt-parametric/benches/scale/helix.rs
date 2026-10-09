//! Helical sweeps at scale: a 200-turn spring, and 10,000 pattern members of
//! a spring family sharing one generated variant.

use super::*;
use occt_bridge::{HelixOptions, SweepOrientation};

pub(crate) fn helix_case() -> Outcome {
    timed(
        "helix: 200-turn spring sweep".into(),
        Duration::from_secs(2),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let turns = 200.0;
            let (radius, pitch, wire) = (10.0, 4.0, 1.5);
            let path = session
                .create_helix_wire(HelixOptions {
                    origin: Vec3::new(0.0, 0.0, 0.0),
                    axis: Vec3::new(0.0, 0.0, 1.0),
                    start_direction: Vec3::new(1.0, 0.0, 0.0),
                    radius,
                    pitch,
                    turns,
                    left_handed: false,
                })
                .map_err(|error| failure(error.to_string()))?;
            let lead = std::f64::consts::TAU * radius;
            let circle = session
                .create_circle_wire(
                    Vec3::new(radius, 0.0, 0.0),
                    Vec3::new(0.0, lead, pitch),
                    wire,
                )
                .map_err(|error| failure(error.to_string()))?;
            let profile = session
                .create_face_from_wire(&circle)
                .map_err(|error| failure(error.to_string()))?;
            let spring = session
                .sweep(
                    &profile,
                    &path,
                    SweepOrientation::Binormal(Vec3::new(0.0, 0.0, 1.0)),
                )
                .map_err(|error| failure(error.to_string()))?;
            let volume = session
                .volume(&spring)
                .map_err(|error| failure(error.to_string()))?;
            let expected = std::f64::consts::PI * wire * wire * turns * lead.hypot(pitch);
            let valid = session
                .is_valid(&spring)
                .map_err(|error| failure(error.to_string()))?;
            if !valid || (volume - expected).abs() > 1e-3 * expected {
                return Err(failure(format!(
                    "valid {valid}, volume {volume} vs {expected}"
                )));
            }
            drop((path, circle, profile, spring));
            if session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?
                != 0
            {
                return Err(failure("retained handles".into()));
            }
            Ok(format!(
                "valid spring, volume within 0.1% of {expected:.0} mm³"
            ))
        },
    )
}

pub(crate) fn helix_pattern_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        "helix: 10000 pattern members of a helix family".into(),
        Duration::from_secs(2),
        Expectation::Required,
        || {
            let mut definition = definition.clone();
            definition.features.push(FeatureDefinition {
                id: "coil".into(),
                operation: FeatureOperation::Helix {
                    origin: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                    axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    start: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                    radius: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
                    pitch: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
                    turns: ScalarExpr::Literal(Quantity::scalar(10.0)),
                    left_handed: false,
                },
            });
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            graph.add_linear_pattern(
                "row",
                "member",
                "part",
                10_000,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
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
            Ok("10001 instances, one helix variant, no retained handles".into())
        },
    )
}
