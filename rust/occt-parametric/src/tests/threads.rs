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
            runout: None,
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
fn ai_threaded_rod_example_follows_diameter_pitch_run_and_retains_failed_edits() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/threaded-rod.request.json"
    ))
    .unwrap();
    let d = ModelDocument::from_json(&r["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let mut p = part(&d.family);
    let first = p.regenerate(&session).unwrap();
    p.overrides.insert(
        "major_diameter".into(),
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
    );
    p.overrides.insert(
        "pitch".into(),
        ParameterValue::Scalar(Quantity::length(2.0, LengthUnit::Millimeter)),
    );
    p.overrides.insert(
        "thread_length".into(),
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
    );
    let edited = p.regenerate_incremental(&session, &first).unwrap();
    let body = edited.shape("body").unwrap();
    assert!(session.is_valid(body).unwrap());
    let (_, area, centroid) = groove(6.0, 2.0, false);
    let expected = std::f64::consts::PI * 36.0 * 18.0 - area * TAU * centroid * 6.0;
    assert!((session.volume(body).unwrap() - expected).abs() < expected * 1e-4);
    let b = session.exact_bounds(body).unwrap();
    assert!((b.max.x - 6.0).abs() < 1e-5 && (b.max.z - 18.0).abs() < 1e-5);
    assert_eq!(
        ModelDocument::from_json(&d.to_json_pretty().unwrap()).unwrap(),
        d
    );
    let count = session.shape_count().unwrap();
    p.overrides.insert(
        "pitch".into(),
        ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
    );
    assert!(p.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(body).unwrap());
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
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

fn with_runout(family: &FamilyDefinition, runout: f64) -> FamilyDefinition {
    let mut family = family.clone();
    let last = family.features.len() - 1;
    if let FeatureOperation::Thread { runout: r, .. } = &mut family.features[last].operation {
        *r = Some(length(runout));
    }
    family
}

#[test]
fn runout_removes_a_fading_fraction_of_the_extra_turns_for_every_hand_and_side() {
    let session = Session::new().unwrap();
    for internal in [false, true] {
        let mut extras = Vec::new();
        for left_handed in [false, true] {
            let base = threaded(internal, left_handed);
            let faded = with_runout(&base, 2.0 * PITCH);
            let (a, b) = (
                part(&base).regenerate(&session).unwrap(),
                part(&faded).regenerate(&session).unwrap(),
            );
            let body = b.shape("body").unwrap();
            assert!(session.is_valid(body).unwrap());
            let extra =
                session.volume(a.shape("body").unwrap()).unwrap() - session.volume(body).unwrap();
            // Two more full-depth turns would remove this much.
            let two_turns = removed(internal, PITCH) * 2.0 * PITCH / RUN;
            assert!(
                extra > 0.1 * two_turns && extra < 0.7 * two_turns,
                "internal {internal}, LH {left_handed}: {extra} of {two_turns}"
            );
            extras.push(extra);
        }
        // Mirror images: left- and right-handed runouts remove equal volume.
        assert!(
            (extras[0] - extras[1]).abs() < 1e-4 * extras[0],
            "{extras:?}"
        );
    }
}

#[test]
fn runout_groove_fades_to_nothing_by_its_end() {
    let session = Session::new().unwrap();
    // A 3 mm pitch halves the turns (and boolean work) of the default fixture.
    let pitch = 3.0;
    let base = threaded(false, false);
    let runout = 3.0 * pitch;
    let faded = with_runout(&base, runout);
    let coarse = |family| {
        let mut p = part(family);
        p.overrides.insert(
            "pitch".into(),
            ParameterValue::Scalar(Quantity::length(pitch, LengthUnit::Millimeter)),
        );
        p.regenerate(&session).unwrap()
    };
    let (a, b) = (coarse(&base), coarse(&faded));
    // Material removed only by the runout, beyond the abrupt groove's end.
    let fade = session
        .cut(a.shape("body").unwrap(), b.shape("body").unwrap())
        .unwrap();
    let end = 5.0 + RUN;
    let slab = |from: f64, to: f64| {
        let block = session
            .create_box(
                occt_bridge::Vec3::new(-10.0, -10.0, from),
                occt_bridge::Vec3::new(20.0, 20.0, to - from),
            )
            .unwrap();
        session.overlap_volume(&fade, &block).unwrap()
    };
    let (near, far) = (
        slab(end, end + pitch),
        slab(end + 2.0 * pitch, end + 3.0 * pitch),
    );
    assert!(near > 0.0 && far < 0.25 * near, "near {near}, far {far}");
    // Nothing is cut once the groove has left the rod.
    let bounds = session.exact_bounds(&fade).unwrap();
    assert!(bounds.max.z < end + runout + pitch / 2.0, "{bounds:?}");
}

#[test]
fn invalid_runouts_and_blind_taps_without_room_fail_cleanly() {
    let session = Session::new().unwrap();
    for runout in [-1.0, 10.5 * PITCH] {
        let family = with_runout(&threaded(false, false), runout);
        assert!(part(&family).regenerate(&session).is_err(), "{runout}");
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/tapped-block.request.json"
    ))
    .unwrap();
    for (runout, ok) in [(4.0, true), (4.6, false)] {
        let mut r = request.clone();
        // Through tap left plain; the blind M6 tap is 9 mm of a 13.5 mm usable bore.
        r["model"]["family"]["features"][1]["operation"]["hole"]["thread"]["modeled"] =
            serde_json::Value::Null;
        r["model"]["family"]["features"][2]["operation"]["hole"]["thread"]["modeled"]["runout"] = serde_json::json!({"literal": {"value": runout, "dimension": "length", "unit": "millimeter"}});
        let doc = ModelDocument::from_json(&r["model"].to_string()).unwrap();
        let block = PartInstance {
            id: "block".into(),
            definition: &doc.family,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let result = block.regenerate(&session);
        assert_eq!(result.is_ok(), ok, "runout {runout}");
        if let Ok(built) = &result {
            assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
        }
    }
}
