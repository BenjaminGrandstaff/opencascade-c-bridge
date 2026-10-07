use super::*;
use std::f64::consts::PI;

pub(super) fn round_primitive_case(sphere: bool) -> Outcome {
    let mut definition = block();
    definition.features.clear();
    definition.datums.clear();
    let point =
        |x, y, z| VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter));
    definition.features.push(FeatureDefinition {
        id: "anchor".into(),
        operation: FeatureOperation::Box {
            origin: point(0.0, 0.0, 0.0),
            size: point(1.0, 1.0, 1.0),
        },
    });
    for i in 0..1000 {
        let origin = point(1e6 + i as f64 * 50.0, -1e6, 1e6);
        let operation = if sphere {
            FeatureOperation::Sphere {
                center: origin,
                radius: ScalarExpr::Parameter("width".into()),
            }
        } else {
            FeatureOperation::Cone {
                origin,
                axis: VectorExpr::Literal(VectorQuantity::scalars(
                    if i % 2 == 0 { 1e300 } else { 1e-300 },
                    0.0,
                    0.0,
                )),
                base_radius: ScalarExpr::Parameter("width".into()),
                top_radius: ScalarExpr::Literal(Quantity::length(4.0, LengthUnit::Millimeter)),
                height: ScalarExpr::Literal(Quantity::length(8.0, LengthUnit::Millimeter)),
            }
        };
        definition.features.push(FeatureDefinition {
            id: format!("round-{i}"),
            operation,
        });
    }
    timed(
        format!(
            "1000 {} features: build/edit at distant coordinates",
            if sphere { "sphere" } else { "cone" }
        ),
        ms(10_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let start = session.shape_count()?;
            let mut part = PartInstance {
                id: "primitives".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session)?;
            part.overrides.insert("width".into(), length(12.0));
            let edited = part.regenerate_incremental(&session, &first)?;
            if edited.regeneration.rebuilt.len() != 1000
                || edited.regeneration.reused != ["anchor"]
                || !session.is_same(
                    first.shape("anchor").unwrap(),
                    edited.shape("anchor").unwrap(),
                )?
            {
                return Err(failure(
                    "primitive edit failed to preserve unaffected geometry".into(),
                ));
            }
            for (result, radius) in [(&first, 10.0_f64), (&edited, 12.0)] {
                let expected = if sphere {
                    4.0 * PI * radius.powi(3) / 3.0
                } else {
                    PI * 8.0 * (radius * radius + 4.0 * radius + 16.0) / 3.0
                };
                for i in 0..1000 {
                    let shape = result.shape(&format!("round-{i}")).unwrap();
                    if !session.is_valid(shape)?
                        || (session.volume(shape)? - expected).abs() > 1e-7 * expected
                    {
                        return Err(failure(
                            "round primitive failed validity or analytic volume check".into(),
                        ));
                    }
                }
            }
            if session.shape_count()? != start + 2002 {
                return Err(failure(
                    "primitive construction retained temporary handles".into(),
                ));
            }
            drop((first, edited));
            if session.shape_count()? != start {
                return Err(failure("primitive outputs failed to release".into()));
            }
            Ok("1000 solids rebuilt; analytic volumes/validity checked; anchor reused; all handles released".into())
        },
    )
}
