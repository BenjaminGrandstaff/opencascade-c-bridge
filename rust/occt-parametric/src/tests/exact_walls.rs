//! Exact minimum wall (schema 181) against walls of known thickness.

use super::*;

fn mm(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn at(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn block(id: &str, origin: (f64, f64, f64), size: (f64, f64, f64)) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::Box {
            origin: at(origin.0, origin.1, origin.2),
            size: at(size.0, size.1, size.2),
        },
    }
}
fn rod(
    id: &str,
    origin: (f64, f64, f64),
    axis: (f64, f64, f64),
    radius: f64,
    height: f64,
) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::Cylinder {
            origin: at(origin.0, origin.1, origin.2),
            axis: VectorExpr::Literal(VectorQuantity::scalars(axis.0, axis.1, axis.2)),
            radius: mm(radius),
            height: mm(height),
        },
    }
}
fn binary(
    id: &str,
    op: fn(String, String) -> FeatureOperation,
    a: &str,
    b: &str,
) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation: op(a.into(), b.into()),
    }
}
fn cut(object: String, tool: String) -> FeatureOperation {
    FeatureOperation::Cut { object, tool }
}

fn family(features: Vec<FeatureDefinition>) -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "walls".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        datums: Vec::new(),
        requirements: Vec::new(),
        features,
    }
}

fn verify(
    features: Vec<FeatureDefinition>,
    rules: Vec<(&str, &str, f64)>,
) -> Vec<VerificationResult> {
    let mut definition = family(features);
    definition.requirements = rules
        .into_iter()
        .map(|(id, output, minimum)| Requirement {
            id: id.into(),
            version: 1,
            kind: RequirementKind::Manufacturing,
            priority: RequirementPriority::Advisory,
            statement: format!("{id} holds"),
            rule: VerificationRule::ExactMinimumWall {
                output: output.into(),
                minimum: Quantity::length(minimum, LengthUnit::Millimeter),
            },
            provenance: "test".into(),
            traces: Vec::new(),
        })
        .collect();
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let results = part.regenerate(&session).unwrap().verification.clone();
    assert_eq!(session.shape_count().unwrap(), 0, "handles released");
    results
}

fn thinnest(result: &VerificationResult) -> f64 {
    result.measured.unwrap().value
}

#[test]
fn exact_walls_match_known_thicknesses() {
    let results = verify(
        vec![
            block("plate", (0.0, 0.0, 0.0), (60.0, 30.0, 4.0)),
            // A Ø10 hole 8 mm from the edge leaves a 3 mm ligament.
            block("slab", (0.0, 0.0, 0.0), (40.0, 40.0, 10.0)),
            rod("hole", (8.0, 20.0, -1.0), (0.0, 0.0, 1.0), 5.0, 12.0),
            binary("drilled", cut, "slab", "hole"),
            // Ø20 / Ø16 tube: 2 mm wall.
            rod("outer", (0.0, 0.0, 0.0), (1.0, 0.0, 0.0), 10.0, 50.0),
            rod("bore", (-1.0, 0.0, 0.0), (1.0, 0.0, 0.0), 8.0, 52.0),
            binary("tube", cut, "outer", "bore"),
            // A cup with 2 mm walls and floor.
            block("shell", (0.0, 0.0, 0.0), (20.0, 20.0, 10.0)),
            block("pocket", (2.0, 2.0, 2.0), (16.0, 16.0, 9.0)),
            binary("cup", cut, "shell", "pocket"),
        ],
        vec![
            ("plate.ok", "plate", 3.0),
            ("plate.thin", "plate", 5.0),
            ("ligament", "drilled", 4.0),
            ("tube", "tube", 2.5),
            ("cup", "cup", 2.5),
        ],
    );
    assert_eq!(results[0].status, VerificationStatus::Passed);
    assert!(
        results[0]
            .message
            .starts_with("no opposed faces closer than 3"),
        "{}",
        results[0].message
    );
    assert_eq!(results[1].status, VerificationStatus::Failed);
    assert!((thinnest(&results[1]) - 4.0).abs() < 1e-9);
    assert!(
        (thinnest(&results[2]) - 3.0).abs() < 1e-7,
        "{}",
        results[2].message
    );
    let witness = results[2].witness.as_ref().unwrap();
    assert!(witness.points_mm.iter().any(|p| p.x.abs() < 1e-7));
    assert!(
        (thinnest(&results[3]) - 2.0).abs() < 1e-7,
        "{}",
        results[3].message
    );
    assert!(
        (thinnest(&results[4]) - 2.0).abs() < 1e-9,
        "{}",
        results[4].message
    );
    assert!(results.iter().all(|r| r.evidence == Evidence::Exact));
}

#[test]
fn gaps_and_faces_sharing_edges_are_not_walls() {
    let results = verify(
        vec![
            block("left", (0.0, 0.0, 0.0), (20.0, 20.0, 20.0)),
            block("right", (25.0, 0.0, 0.0), (20.0, 20.0, 20.0)),
            FeatureDefinition {
                id: "pair".into(),
                operation: FeatureOperation::Compound {
                    inputs: vec!["left".into(), "right".into()],
                },
            },
            // An L-bracket: the inside corner's faces share an edge.
            block("base", (0.0, 0.0, 0.0), (40.0, 20.0, 6.0)),
            block("upright", (0.0, 0.0, 0.0), (6.0, 20.0, 30.0)),
            FeatureDefinition {
                id: "bracket".into(),
                operation: FeatureOperation::Fuse {
                    left: "base".into(),
                    right: "upright".into(),
                },
            },
        ],
        vec![("gap", "pair", 10.0), ("bracket", "bracket", 7.0)],
    );
    // The 5 mm gap between the blocks faces away from material.
    assert_eq!(
        results[0].status,
        VerificationStatus::Passed,
        "{}",
        results[0].message
    );
    // Both legs are 6 mm; the corner faces are not compared.
    assert_eq!(results[1].status, VerificationStatus::Failed);
    assert!(
        (thinnest(&results[1]) - 6.0).abs() < 1e-9,
        "{}",
        results[1].message
    );
}

#[test]
fn exact_walls_scale_to_hundreds_of_holes() {
    // A 100 × 100 × 5 plate with a 20 × 20 grid of Ø2 holes at 5 mm pitch:
    // 3 mm between holes and 1.5 mm to the plate edges.
    let mut features = vec![block("plate", (0.0, 0.0, 0.0), (100.0, 100.0, 5.0))];
    features.push(rod("hole", (2.5, 2.5, -1.0), (0.0, 0.0, 1.0), 1.0, 7.0));
    let pattern = |id: &str, input: &str, step: (f64, f64, f64)| FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::LinearPattern {
            input: input.into(),
            step: at(step.0, step.1, step.2),
            count: ScalarExpr::Literal(Quantity::scalar(20.0)),
        },
    };
    features.push(pattern("row", "hole", (5.0, 0.0, 0.0)));
    features.push(pattern("holes", "row", (0.0, 5.0, 0.0)));
    features.push(binary("perforated", cut, "plate", "holes"));
    let started = std::time::Instant::now();
    let results = verify(features, vec![("web", "perforated", 3.5)]);
    let elapsed = started.elapsed().as_secs_f64();
    assert_eq!(results[0].status, VerificationStatus::Failed);
    assert!(
        (thinnest(&results[0]) - 1.5).abs() < 1e-7,
        "{}",
        results[0].message
    );
    assert!(elapsed < 20.0, "{elapsed} s");
    eprintln!(
        "400-hole exact wall: {elapsed:.2} s; {}",
        results[0].message
    );
}
