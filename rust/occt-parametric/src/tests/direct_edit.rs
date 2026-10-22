//! Direct face edits (schema 183): remove, offset and move selected faces,
//! checked against closed-form volumes.

use super::*;
use std::f64::consts::PI;

fn mm(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn at(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn feature(id: &str, operation: FeatureOperation) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation,
    }
}
fn near(x: f64, y: f64, z: f64) -> FaceSelector {
    FaceSelector::NearestCenter {
        target: at(x, y, z),
        maximum_distance: mm(0.01),
    }
}
fn top() -> FaceSelector {
    FaceSelector::AtExtreme {
        axis: CoordinateAxis::Z,
        extremum: Extremum::Maximum,
        tolerance: mm(1e-6),
    }
}

/// A 40 × 30 × 20 block with a Ø6 through hole at (10, 15) and a Ø8 × 5
/// boss on top at (30, 15), followed by `edits`.
fn family(edits: Vec<FeatureDefinition>) -> FamilyDefinition {
    let mut features = vec![
        feature(
            "block",
            FeatureOperation::Box {
                origin: at(0.0, 0.0, 0.0),
                size: at(40.0, 30.0, 20.0),
            },
        ),
        feature(
            "rod",
            FeatureOperation::Cylinder {
                origin: at(10.0, 15.0, -1.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: mm(3.0),
                height: mm(22.0),
            },
        ),
        feature(
            "drilled",
            FeatureOperation::Cut {
                object: "block".into(),
                tool: "rod".into(),
            },
        ),
        feature(
            "post",
            FeatureOperation::Cylinder {
                origin: at(30.0, 15.0, 20.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: mm(4.0),
                height: mm(5.0),
            },
        ),
        feature(
            "part",
            FeatureOperation::Fuse {
                left: "drilled".into(),
                right: "post".into(),
            },
        ),
    ];
    features.extend(edits);
    FamilyDefinition {
        id: "direct".into(),
        version: 1,
        parameters: vec![ParameterDefinition {
            id: "push".into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(2.0, LengthUnit::Millimeter)),
            minimum: None,
            maximum: None,
        }],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        features,
        requirements: vec![],
        datums: vec![],
        references: vec![],
        feature_colors: Default::default(),
        assumptions: vec![],
        history: FeatureHistory::default(),
    }
}

const BLOCK: f64 = 24_000.0;
fn hole() -> f64 {
    PI * 9.0 * 20.0
}
fn boss() -> f64 {
    PI * 16.0 * 5.0
}

/// The last feature's volume and bounds.
fn build(
    family: &FamilyDefinition,
    overrides: HashMap<String, ParameterValue>,
) -> Result<(f64, occt_bridge::Bounds), ModelError> {
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: family,
        overrides,
        provenance: "test".into(),
    };
    let last = &family.features.last().unwrap().id;
    let result = part.regenerate(&session).map(|generated| {
        let shape = generated.shape(last).unwrap();
        (
            session.volume(shape).unwrap(),
            session.exact_bounds(shape).unwrap(),
        )
    });
    assert_eq!(session.shape_count().unwrap(), 0, "handles released");
    result
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} vs {expected}");
}

#[test]
fn removed_faces_take_the_hole_and_boss_away() {
    let (volume, _) = build(&family(vec![]), HashMap::new()).unwrap();
    close(volume, BLOCK - hole() + boss());
    let remove = |id: &str, input: &str, faces| {
        feature(
            id,
            FeatureOperation::RemoveFaces {
                input: input.into(),
                faces,
            },
        )
    };
    // The hole wall alone; then the boss's side and top together.
    let (volume, _) = build(
        &family(vec![remove(
            "unholed",
            "part",
            vec![near(10.0, 15.0, 10.0)],
        )]),
        HashMap::new(),
    )
    .unwrap();
    close(volume, BLOCK + boss());
    let (volume, bounds) = build(
        &family(vec![
            remove("unholed", "part", vec![near(10.0, 15.0, 10.0)]),
            remove(
                "flat",
                "unholed",
                vec![near(30.0, 15.0, 22.5), near(30.0, 15.0, 25.0)],
            ),
        ]),
        HashMap::new(),
    )
    .unwrap();
    close(volume, BLOCK);
    close(bounds.max.z, 20.0);
}

#[test]
fn offset_faces_push_pull_by_parameter_and_resize_holes() {
    let offset = |faces, distance| {
        feature(
            "edited",
            FeatureOperation::OffsetFaces {
                input: "drilled".into(),
                faces,
                distance,
                tolerance: mm(1e-6),
            },
        )
    };
    let pushed = family(vec![offset(
        vec![top()],
        ScalarExpr::Parameter("push".into()),
    )]);
    let (volume, bounds) = build(&pushed, HashMap::new()).unwrap();
    close(volume, 40.0 * 30.0 * 22.0 - PI * 9.0 * 22.0);
    close(bounds.max.z, 22.0);
    // A negative parameter pushes the top down.
    let overrides = HashMap::from([(
        "push".to_string(),
        ParameterValue::Scalar(Quantity::length(-4.0, LengthUnit::Millimeter)),
    )]);
    let (volume, _) = build(&pushed, overrides).unwrap();
    close(volume, 40.0 * 30.0 * 16.0 - PI * 9.0 * 16.0);
    // The hole wall's normal points at its axis: -1 widens it to Ø8.
    let (volume, _) = build(
        &family(vec![offset(vec![near(10.0, 15.0, 10.0)], mm(-1.0))]),
        HashMap::new(),
    )
    .unwrap();
    close(volume, BLOCK - PI * 16.0 * 20.0);
    // Selecting the same face twice is one face.
    let (volume, _) = build(
        &family(vec![offset(vec![top(), top()], mm(1.0))]),
        HashMap::new(),
    )
    .unwrap();
    close(volume, 40.0 * 30.0 * 21.0 - PI * 9.0 * 21.0);
}

#[test]
fn moved_faces_carry_holes_and_bosses_to_their_new_place() {
    let moved = |faces, offset| {
        feature(
            "moved",
            FeatureOperation::MoveFaces {
                input: "part".into(),
                faces,
                offset,
            },
        )
    };
    // Move the hole 15 mm along x; a later edit finds its wall at x = 25.
    let mut edits = vec![moved(vec![near(10.0, 15.0, 10.0)], at(15.0, 0.0, 0.0))];
    let (volume, _) = build(&family(edits.clone()), HashMap::new()).unwrap();
    close(volume, BLOCK - hole() + boss());
    edits.push(feature(
        "check",
        FeatureOperation::RemoveFaces {
            input: "moved".into(),
            faces: vec![near(25.0, 15.0, 10.0)],
        },
    ));
    close(
        build(&family(edits), HashMap::new()).unwrap().0,
        BLOCK + boss(),
    );
    // Move the boss 4 mm towards -y: same volume, shifted bounds.
    let (volume, bounds) = build(
        &family(vec![moved(
            vec![near(30.0, 15.0, 22.5), near(30.0, 15.0, 25.0)],
            at(0.0, -4.0, 0.0),
        )]),
        HashMap::new(),
    )
    .unwrap();
    close(volume, BLOCK - hole() + boss());
    close(bounds.max.z, 25.0);
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &family(vec![moved(
            vec![near(30.0, 15.0, 22.5), near(30.0, 15.0, 25.0)],
            at(0.0, -4.0, 0.0),
        )]),
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let generated = part.regenerate(&session).unwrap();
    let post_top = session
        .subshapes(generated.shape("moved").unwrap(), ShapeType::Face)
        .unwrap()
        .into_iter()
        .find(|face| session.exact_bounds(face).unwrap().min.z > 24.99)
        .unwrap();
    let center = session.center_of_mass(&post_top).unwrap();
    close(center.x, 30.0);
    close(center.y, 11.0);
}

#[test]
fn bad_direct_edits_are_refused_with_their_selector() {
    let error = |operation| {
        build(&family(vec![feature("bad", operation)]), HashMap::new())
            .unwrap_err()
            .message
    };
    // The outer top face cannot heal: its neighbours never close.
    let message = error(FeatureOperation::RemoveFaces {
        input: "drilled".into(),
        faces: vec![top()],
    });
    assert!(
        message.contains("BOPAlgo_AlertUnableToRemoveTheFeature")
            && message.contains("face selector 0"),
        "{message}"
    );
    let message = error(FeatureOperation::MoveFaces {
        input: "part".into(),
        faces: vec![near(10.0, 15.0, 10.0)],
        offset: at(0.0, 0.0, 0.0),
    });
    assert!(message.contains("nonzero vector"), "{message}");
    let message = error(FeatureOperation::OffsetFaces {
        input: "part".into(),
        faces: vec![top()],
        distance: mm(0.0),
        tolerance: mm(1e-6),
    });
    assert!(message.contains("nonzero length"), "{message}");
    let message = error(FeatureOperation::RemoveFaces {
        input: "part".into(),
        faces: vec![],
    });
    assert!(message.contains("at least one face"), "{message}");
}

#[test]
fn removals_beyond_the_face_limit_are_refused_before_the_kernel_runs() {
    // An 11 × 10 grid of Ø2 holes: 110 walls, over the 100-face limit.
    let mut definition = family(vec![]);
    let pattern = |input: &str, step: VectorExpr, count: f64| FeatureOperation::LinearPattern {
        skip: Vec::new(),
        input: input.into(),
        step,
        count: ScalarExpr::Literal(Quantity::scalar(count)),
    };
    definition.features = vec![
        feature(
            "plate",
            FeatureOperation::Box {
                origin: at(0.0, 0.0, 0.0),
                size: at(55.0, 50.0, 5.0),
            },
        ),
        feature(
            "hole",
            FeatureOperation::Cylinder {
                origin: at(2.5, 2.5, -1.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: mm(1.0),
                height: mm(7.0),
            },
        ),
        feature("row", pattern("hole", at(5.0, 0.0, 0.0), 11.0)),
        feature("holes", pattern("row", at(0.0, 5.0, 0.0), 10.0)),
        feature(
            "perforated",
            FeatureOperation::Cut {
                object: "plate".into(),
                tool: "holes".into(),
            },
        ),
        feature(
            "bad",
            FeatureOperation::RemoveFaces {
                input: "perforated".into(),
                faces: (0..110)
                    .map(|i| {
                        near(
                            2.5 + 5.0 * (i % 11) as f64,
                            2.5 + 5.0 * (i / 11) as f64,
                            2.5,
                        )
                    })
                    .collect(),
            },
        ),
    ];
    let message = build(&definition, HashMap::new()).unwrap_err().message;
    assert!(
        message.contains("more than 100 faces") && message.contains("several features"),
        "{message}"
    );
}
