//! Direct face edits (schema 183) on perforated plates: 100 hole walls
//! removed and moved (the per-feature limit), and 400 offset, in one feature
//! each.
use super::*;

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}
fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

/// A 5·side square plate, 5 mm thick, with a side × side grid of Ø2 holes
/// at 5 mm pitch, then `edit` on every hole wall.
fn plate(
    side: usize,
    edit: impl Fn(String, Vec<FaceSelector>) -> FeatureOperation,
) -> FamilyDefinition {
    let mut definition = block();
    definition.datums.clear();
    definition.requirements.clear();
    definition.parameters.clear();
    let pattern = |input: &str, step: VectorExpr| FeatureOperation::LinearPattern {
        skip: Vec::new(),
        input: input.into(),
        step,
        count: ScalarExpr::Literal(Quantity::scalar(side as f64)),
    };
    let walls = (0..side * side)
        .map(|index| FaceSelector::NearestCenter {
            target: point(
                2.5 + 5.0 * (index % side) as f64,
                2.5 + 5.0 * (index / side) as f64,
                2.5,
            ),
            maximum_distance: length(0.01),
        })
        .collect();
    let feature = |id: &str, operation| FeatureDefinition {
        id: id.into(),
        operation,
    };
    definition.features = vec![
        feature(
            "plate",
            FeatureOperation::Box {
                origin: point(0.0, 0.0, 0.0),
                size: point(5.0 * side as f64, 5.0 * side as f64, 5.0),
            },
        ),
        feature(
            "hole",
            FeatureOperation::Cylinder {
                origin: point(2.5, 2.5, -1.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: length(1.0),
                height: length(7.0),
            },
        ),
        feature("row", pattern("hole", point(5.0, 0.0, 0.0))),
        feature("holes", pattern("row", point(0.0, 5.0, 0.0))),
        feature(
            "perforated",
            FeatureOperation::Cut {
                object: "plate".into(),
                tool: "holes".into(),
            },
        ),
        feature("edited", edit("perforated".into(), walls)),
    ];
    definition
}

pub(super) fn direct_edit_case() -> Outcome {
    let solid = |side: usize| 25.0 * (side * side) as f64 * 5.0;
    let holes = |side: usize, radius: f64| {
        std::f64::consts::PI * radius * radius * 5.0 * (side * side) as f64
    };
    let cases = [
        (
            "remove 100",
            plate(10, |input, faces| FeatureOperation::RemoveFaces {
                input,
                faces,
            }),
            solid(10),
        ),
        (
            "offset 400",
            plate(20, |input, faces| FeatureOperation::OffsetFaces {
                input,
                faces,
                distance: length(-0.2),
                tolerance: length(1e-6),
            }),
            solid(20) - holes(20, 1.2),
        ),
        (
            "move 100",
            plate(10, |input, faces| FeatureOperation::MoveFaces {
                input,
                faces,
                offset: point(0.5, 1.0, 0.0),
            }),
            solid(10) - holes(10, 1.0),
        ),
    ];
    timed(
        "direct face edits: remove and move 100 hole walls, offset 400".into(),
        ms(80_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let mut timings = Vec::new();
            for (name, definition, volume) in &cases {
                let started = Instant::now();
                let part = PartInstance {
                    id: "plate".into(),
                    definition,
                    overrides: HashMap::new(),
                    provenance: "bench".into(),
                };
                let generated = part.regenerate(&session)?;
                let shape = generated
                    .shape("edited")
                    .ok_or_else(|| failure(format!("{name} output missing")))?;
                let measured = session.volume(shape)?;
                if (measured - volume).abs() > 1e-6 * volume || !session.is_valid(shape)? {
                    return Err(failure(format!(
                        "{name}: volume {measured} vs {volume} or invalid"
                    )));
                }
                drop(generated);
                timings.push(format!("{name} {:.2} s", started.elapsed().as_secs_f64()));
            }
            if session.shape_count()? != 0 {
                return Err(failure("direct edit handles retained".into()));
            }
            Ok(format!(
                "exact volumes; handles released; {}",
                timings.join(", ")
            ))
        },
    )
}
