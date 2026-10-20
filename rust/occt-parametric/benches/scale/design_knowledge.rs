//! Design-knowledge checks at scale (schemas 180–181, ABI 85): exact walls
//! between hundreds of holes, refined draft on hundreds of freeform faces,
//! and hundreds of catalog standard parts in one model.

use super::*;
use occt_parametric::{
    Requirement, StandardPartKind, VerificationResult, VerificationRule, VerificationStatus,
};

fn mm(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn at(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn family(features: Vec<FeatureDefinition>, requirements: Vec<Requirement>) -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "bench".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        datums: Vec::new(),
        requirements,
        features,
    }
}
fn regenerate(definition: &FamilyDefinition) -> Result<Vec<VerificationResult>, ModelError> {
    let session = Session::new().map_err(|e| failure(e.to_string()))?;
    let part = PartInstance {
        id: "part".into(),
        definition,
        overrides: HashMap::new(),
        provenance: "bench".into(),
    };
    Ok(part.regenerate(&session)?.verification.clone())
}

/// A 100 × 100 × 5 plate with a 20 × 20 grid of Ø2 holes: 400 bores and
/// about 1,650 face pairs within 3.5 mm.
pub(crate) fn exact_wall_case() -> Outcome {
    timed(
        "exact wall: plate with 400 holes".into(),
        ms(5_000),
        Expectation::Required,
        || {
            let pattern = |id: &str, input: &str, step: (f64, f64, f64)| FeatureDefinition {
                id: id.into(),
                operation: FeatureOperation::LinearPattern {
                    input: input.into(),
                    step: at(step.0, step.1, step.2),
                    count: ScalarExpr::Literal(Quantity::scalar(20.0)),
                },
            };
            let definition = family(
                vec![
                    FeatureDefinition {
                        id: "plate".into(),
                        operation: FeatureOperation::Box {
                            origin: at(0.0, 0.0, 0.0),
                            size: at(100.0, 100.0, 5.0),
                        },
                    },
                    FeatureDefinition {
                        id: "hole".into(),
                        operation: FeatureOperation::Cylinder {
                            origin: at(2.5, 2.5, -1.0),
                            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                            radius: mm(1.0),
                            height: mm(7.0),
                        },
                    },
                    pattern("row", "hole", (5.0, 0.0, 0.0)),
                    pattern("holes", "row", (0.0, 5.0, 0.0)),
                    FeatureDefinition {
                        id: "web".into(),
                        operation: FeatureOperation::Cut {
                            object: "plate".into(),
                            tool: "holes".into(),
                        },
                    },
                ],
                vec![Requirement {
                    id: "wall".into(),
                    version: 1,
                    kind: RequirementKind::Manufacturing,
                    priority: RequirementPriority::Advisory,
                    statement: "wall".into(),
                    rule: VerificationRule::ExactMinimumWall {
                        output: "web".into(),
                        minimum: Quantity::length(3.5, LengthUnit::Millimeter),
                    },
                    provenance: "bench".into(),
                    traces: Vec::new(),
                }],
            );
            let results = regenerate(&definition)?;
            let thinnest = results[0].measured.map(|m| m.value);
            if results[0].status != VerificationStatus::Failed
                || thinnest.is_none_or(|t| (t - 1.5).abs() > 1e-7)
            {
                return Err(failure(format!("unexpected wall: {}", results[0].message)));
            }
            Ok(results[0].message.clone())
        },
    )
}

/// 500 elliptical prisms in one compound: 500 surfaces of extrusion refined
/// on the surface, each matching its closed form.
pub(crate) fn freeform_draft_case() -> Outcome {
    timed(
        "refined draft: 500 freeform faces".into(),
        ms(10_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|e| failure(e.to_string()))?;
            let up = Vec3::new(0.0, 0.0, 1.0);
            let mut prisms = Vec::with_capacity(500);
            for k in 0..500 {
                let center = Vec3::new(20.0 * (k % 25) as f64, 20.0 * (k / 25) as f64, 0.0);
                let ellipse = session
                    .create_ellipse_wire(center, up, 6.0 + (k % 3) as f64, 3.0)
                    .map_err(|e| failure(e.to_string()))?;
                let face = session
                    .create_face_from_wire(&ellipse)
                    .map_err(|e| failure(e.to_string()))?;
                prisms.push(
                    session
                        .create_prism_from_face(&face, Vec3::new(0.0, 0.0, 10.0))
                        .map_err(|e| failure(e.to_string()))?,
                );
            }
            let refs: Vec<_> = prisms.iter().collect();
            let compound = session
                .create_compound(&refs)
                .map_err(|e| failure(e.to_string()))?;
            let angle = 0.3f64;
            let started = Instant::now();
            let ranges = session
                .face_pull_ranges(&compound, Vec3::new(angle.sin(), 0.0, angle.cos()))
                .map_err(|e| failure(e.to_string()))?;
            let elapsed = started.elapsed().as_secs_f64();
            let refined: Vec<_> = ranges.iter().flatten().filter(|r| r.refined).collect();
            if refined.len() != 500
                || refined
                    .iter()
                    .any(|r| (r.maximum.0 - angle.sin()).abs() > 1e-9)
            {
                return Err(failure(format!("{} refined faces", refined.len())));
            }
            Ok(format!(
                "{} faces, 500 refined to 1e-9 in {elapsed:.3}s",
                ranges.len()
            ))
        },
    )
}

/// 200 catalog screws, nuts and washers of every size in one model.
pub(crate) fn standard_parts_case() -> Outcome {
    timed(
        "standard parts: 200 in one model".into(),
        ms(5_000),
        Expectation::Required,
        || {
            let kinds = [
                StandardPartKind::SocketHeadCapScrewIso4762,
                StandardPartKind::HexNutIso4032,
                StandardPartKind::PlainWasherIso7089,
                StandardPartKind::DowelPinIso8734,
            ];
            let mut features: Vec<FeatureDefinition> = (0..200)
                .map(|k| {
                    let kind = kinds[k % 4];
                    FeatureDefinition {
                        id: format!("part-{k}"),
                        operation: FeatureOperation::StandardPart {
                            part: kind,
                            nominal_diameter: mm(kind.sizes()[(k / 4) % kind.sizes().len()]),
                            length: kind.takes_length().then(|| mm(40.0)),
                            origin: at(50.0 * (k % 20) as f64, 50.0 * (k / 20) as f64, 0.0),
                            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                        },
                    }
                })
                .collect();
            features.push(FeatureDefinition {
                id: "all".into(),
                operation: FeatureOperation::Compound {
                    inputs: (0..200).map(|k| format!("part-{k}")).collect(),
                },
            });
            let definition = family(
                features,
                vec![Requirement {
                    id: "valid".into(),
                    version: 1,
                    kind: RequirementKind::Validation,
                    priority: RequirementPriority::Required,
                    statement: "valid".into(),
                    rule: VerificationRule::ShapeValid {
                        output: "all".into(),
                    },
                    provenance: "bench".into(),
                    traces: Vec::new(),
                }],
            );
            regenerate(&definition)?;
            Ok("200 parts valid in one compound".into())
        },
    )
}
