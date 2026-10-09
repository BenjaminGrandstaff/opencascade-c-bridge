//! Modeled ISO 68-1 threads cut into rods and holes.

use super::*;
use crate::features::threads::{basic_depth, groove};
use std::f64::consts::TAU;

const MAJOR: f64 = 10.0;
const PITCH: f64 = 1.5;
const RUN: f64 = 15.0;

fn mm(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

fn up() -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0))
}

fn thread(input: &str, internal: bool, left_handed: bool) -> FeatureDefinition {
    FeatureDefinition {
        id: "body".into(),
        operation: FeatureOperation::Thread {
            input: input.into(),
            origin: mm(0.0, 0.0, 5.0),
            axis: up(),
            major_diameter: length(MAJOR),
            pitch: ScalarExpr::Parameter("pitch".into()),
            length: length(RUN),
            internal,
            left_handed,
        },
    }
}

/// A 30 mm rod of the major diameter (external) or a 20 mm block drilled
/// through at the minor diameter (internal), threaded for 15 mm from z = 5.
fn threaded(internal: bool, left_handed: bool) -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.parameters = vec![ParameterDefinition {
        id: "pitch".into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: ParameterValue::Scalar(Quantity::length(PITCH, LengthUnit::Millimeter)),
        minimum: None,
        maximum: None,
    }];
    family.features = if internal {
        vec![
            FeatureDefinition {
                id: "block".into(),
                operation: FeatureOperation::Box {
                    origin: mm(-12.0, -12.0, 0.0),
                    size: mm(24.0, 24.0, 25.0),
                },
            },
            FeatureDefinition {
                id: "drilled".into(),
                operation: FeatureOperation::Hole {
                    input: "block".into(),
                    position: mm(0.0, 0.0, 25.0),
                    axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
                    diameter: length(MAJOR - 2.0 * basic_depth(PITCH)),
                    extent: HoleExtent::ThroughAll,
                    bottom: HoleBottom::default(),
                    finish: HoleFinish::Plain,
                    thread: None,
                },
            },
            thread("drilled", true, left_handed),
        ]
    } else {
        vec![
            FeatureDefinition {
                id: "rod".into(),
                operation: FeatureOperation::Cylinder {
                    origin: mm(0.0, 0.0, 0.0),
                    axis: up(),
                    radius: length(MAJOR / 2.0),
                    height: length(30.0),
                },
            },
            thread("rod", false, left_handed),
        ]
    };
    family
}

fn part(family: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition: family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

/// A screw motion of the in-material groove section: area × 2π × centroid
/// radius × turns (Pappus for helical sweeps in planes through the axis).
fn removed(internal: bool, pitch: f64) -> f64 {
    let (_, area, centroid) = groove(MAJOR / 2.0, pitch, internal);
    area * TAU * centroid * (RUN / pitch)
}

#[test]
fn basic_profile_grooves_match_iso_68_1_proportions() {
    let h = 3f64.sqrt() / 2.0 * PITCH;
    assert!((basic_depth(PITCH) - 5.0 * h / 8.0).abs() < 1e-12);
    for internal in [false, true] {
        let (points, area, centroid) = groove(MAJOR / 2.0, PITCH, internal);
        let minor = MAJOR / 2.0 - basic_depth(PITCH);
        // Crest + groove widths sum to the pitch on both diameters.
        let (at_minor, at_major) = if internal {
            (0.75, 0.125)
        } else {
            (0.25, 0.875)
        };
        assert!((area - (at_minor + at_major) * PITCH / 2.0 * basic_depth(PITCH)).abs() < 1e-12);
        assert!(centroid > minor && centroid < MAJOR / 2.0);
        // Successive turns of the swept groove never overlap.
        let widest = points.iter().map(|p| p[1].abs()).fold(0.0, f64::max) * 2.0;
        assert!(widest < 0.96 * PITCH, "{widest}");
        // The overshoot lies in free space: beyond the major radius outside,
        // inside the minor radius for holes.
        let radii: Vec<f64> = points.iter().map(|p| p[0]).collect();
        if internal {
            assert!(radii.iter().cloned().fold(f64::INFINITY, f64::min) < minor);
        } else {
            assert!(radii.iter().cloned().fold(0.0, f64::max) > MAJOR / 2.0);
        }
    }
}

#[test]
fn external_and_internal_threads_remove_the_screw_swept_groove() {
    let session = Session::new().unwrap();
    for internal in [false, true] {
        let family = threaded(internal, false);
        let result = part(&family).regenerate(&session).unwrap();
        let before_id = if internal { "drilled" } else { "rod" };
        let before = session.volume(result.shape(before_id).unwrap()).unwrap();
        let body = result.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap(), "internal {internal}");
        assert_eq!(session.subshape_count(body, ShapeType::Solid).unwrap(), 1);
        let cut = before - session.volume(body).unwrap();
        let expected = removed(internal, PITCH);
        if let Some(directory) = std::env::var_os("OCCB_THREAD_QA_DIR") {
            let mut graph = InstanceGraph::new(&family);
            graph.add_base("part", HashMap::new(), "test").unwrap();
            let name = if internal { "nut.json" } else { "rod.json" };
            std::fs::write(
                std::path::Path::new(&directory).join(name),
                ModelDocument::from_graph(&graph).to_json_pretty().unwrap(),
            )
            .unwrap();
        }
        assert!(
            (cut - expected).abs() < 5e-3 * expected,
            "internal {internal}: {cut} vs {expected}"
        );
        // The crest flats keep the original rod or hole diameter.
        let bounds = session.exact_bounds(body).unwrap();
        if !internal {
            assert!((bounds.max.x - MAJOR / 2.0).abs() < 1e-6, "{bounds:?}");
        }
    }
}

#[test]
fn hand_and_pitch_edits_regenerate_threads() {
    let session = Session::new().unwrap();
    let right = threaded(false, false);
    let left = threaded(false, true);
    let a = part(&right).regenerate(&session).unwrap();
    let b = part(&left).regenerate(&session).unwrap();
    let (va, vb) = (
        session.volume(a.shape("body").unwrap()).unwrap(),
        session.volume(b.shape("body").unwrap()).unwrap(),
    );
    assert!((va - vb).abs() < 1e-3 * va);
    let mut instance = part(&right);
    instance.overrides.insert(
        "pitch".into(),
        ParameterValue::Scalar(Quantity::length(1.0, LengthUnit::Millimeter)),
    );
    let fine = instance.regenerate_incremental(&session, &a).unwrap();
    let rod = session.volume(fine.shape("rod").unwrap()).unwrap();
    let cut = rod - session.volume(fine.shape("body").unwrap()).unwrap();
    assert!(
        (cut - removed(false, 1.0)).abs() < 1e-4 * removed(false, 1.0),
        "{cut} vs {}",
        removed(false, 1.0)
    );
    assert!(fine.regeneration.reused.contains(&"rod".to_string()));
    drop((a, b, fine));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn invalid_threads_fail_without_handles_and_threads_persist() {
    let session = Session::new().unwrap();
    for pitch in [0.0, -1.0, 8.0, 0.001] {
        let family = threaded(false, false);
        let mut instance = part(&family);
        instance.overrides.insert(
            "pitch".into(),
            ParameterValue::Scalar(Quantity::length(pitch, LengthUnit::Millimeter)),
        );
        // 8 mm is too coarse for a 10 mm rod; 0.001 mm needs 15,000 turns.
        assert!(instance.regenerate(&session).is_err(), "pitch {pitch}");
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let family = threaded(true, true);
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"thread\"") && json.contains("\"internal\": true"));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
}
