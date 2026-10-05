use super::*;
use config::{Component, Hinge, Setup, Slider};
use occt_parametric::*;
use serde_json::json;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

fn vector(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn fixture() -> ModelDocument {
    let family = FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "ControlSurface".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        requirements: vec![],
        datums: vec![],
        features: vec![
            FeatureDefinition {
                id: "elevon".into(),
                operation: FeatureOperation::Box {
                    origin: vector(2.0, 0.0, -0.5),
                    size: vector(6.0, 20.0, 1.0),
                },
            },
            FeatureDefinition {
                id: "wing".into(),
                operation: FeatureOperation::Box {
                    origin: vector(-4.0, 0.0, -0.5),
                    size: vector(3.0, 20.0, 1.0),
                },
            },
        ],
    };
    let mut graph = InstanceGraph::new(&family);
    graph
        .add_base("source", HashMap::new(), "motion fixture")
        .unwrap();
    graph.add_configuration("alternate").unwrap();
    ModelDocument::from_graph(&graph)
}

fn setup(start: f64, end: f64, samples: usize) -> Setup {
    Setup {
        schema: "occb-motion-study-v1".into(),
        samples,
        components: vec![
            Component {
                id: "fixed".into(),
                source: "source".into(),
                output: "wing".into(),
                hinge: None,
                slider: None,
            },
            Component {
                id: "moving".into(),
                source: "source".into(),
                output: "elevon".into(),
                slider: None,
                hinge: Some(Hinge {
                    origin_mm: [0.0; 3],
                    axis: [0.0, 1.0, 0.0],
                    minimum_deg: -360.0,
                    maximum_deg: 360.0,
                    start_deg: start,
                    end_deg: end,
                }),
            },
        ],
        collision_options: CollisionOptions::default(),
        continuous_options: ContinuousCollisionOptions::default(),
        excluded_pairs: Vec::new(),
    }
}

#[test]
fn separate_outputs_share_geometry_and_keep_source_document() {
    let mut original = fixture();
    let baseline = original.clone();
    original.family.version += 1;
    original
        .record_revision(
            &baseline,
            RevisionMetadata {
                id: "source-version".into(),
                author: "test".into(),
                recorded_at: "2026-10-04".into(),
                message: "Update family version".into(),
            },
        )
        .unwrap();
    original.generation_records.push(GenerationRecord {
        instance_id: "source".into(),
        attempted_revision: 2,
        accepted_revision: Some(1),
        state: RegenerationState::Stale,
        last_error: Some("source audit retained".into()),
    });
    let accepted = original.clone();
    let setup = setup(-30.0, 30.0, 5);
    let (graph, study) = config::prepare(&original, &setup).unwrap();
    let assembled = config::assembly_document(&original, &graph);
    assert_eq!(assembled.family, original.family);
    assert_eq!(
        assembled.assembly.configurations,
        original.assembly.configurations
    );
    assert_eq!(assembled.generation_records, original.generation_records);
    assert_eq!(assembled.drawings, original.drawings);
    assert_eq!(assembled.revisions, original.revisions);
    assert_eq!(assembled.mesh_exports, original.mesh_exports);
    let loaded = ModelDocument::from_json(&assembled.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, assembled);
    let persisted_study: MotionStudy =
        serde_json::from_str(&serde_json::to_string(&study).unwrap()).unwrap();
    assert_eq!(persisted_study, study);
    let session = Session::new().unwrap();
    let sampled = graph.run_motion_study(&session, &study).unwrap();
    let continuous = graph
        .check_continuous_motion(&session, &study, setup.continuous_options)
        .unwrap();
    assert!(report::clear(&sampled, &continuous));
    assert_eq!(sampled.generated_variants, 1);
    assert_eq!(continuous.generated_variants, 1);
    assert_eq!(sampled.samples.len(), 5);
    assert_eq!(
        report::document(&study, &sampled, &continuous)["continuous"]["status"],
        "clear"
    );
    assert_eq!(original, accepted);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn full_turn_detects_interference_between_clear_endpoints_and_reports_witness() {
    let original = fixture();
    let setup = setup(0.0, 360.0, 2);
    let (graph, study) = config::prepare(&original, &setup).unwrap();
    let session = Session::new().unwrap();
    let sampled = graph.run_motion_study(&session, &study).unwrap();
    assert!(
        sampled
            .samples
            .iter()
            .all(|sample| sample.collisions.is_empty())
    );
    let continuous = graph
        .check_continuous_motion(&session, &study, setup.continuous_options)
        .unwrap();
    assert!(!report::clear(&sampled, &continuous));
    let report = report::document(&study, &sampled, &continuous);
    assert_eq!(report["continuous"]["status"], "collision");
    let witness = &report["continuous"]["pairs"][0];
    assert_eq!(witness["segment"], 0);
    assert_eq!(witness["fraction"], 0.5);
    assert_eq!(witness["check"]["status"], "interference");
    assert!(witness["check"]["overlap_volume_mm3"].as_f64().unwrap() > 0.0);
    assert_eq!(
        witness["check"]["first_witness_mm"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let bounded = graph
        .check_continuous_motion(
            &session,
            &study,
            ContinuousCollisionOptions {
                maximum_queries: 1,
                ..setup.continuous_options
            },
        )
        .unwrap();
    assert!(!report::clear(&sampled, &bounded));
    assert_eq!(
        report::document(&study, &sampled, &bounded)["continuous"]["status"],
        "unresolved"
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn parent_frames_and_source_placements_are_retained_for_fixed_and_hinged_parts() {
    let mut original = fixture();
    let mut graph = original.instance_graph().unwrap();
    graph
        .add_frame(
            "mount",
            None,
            Placement::translated(VectorQuantity::lengths(
                100.0,
                200.0,
                300.0,
                LengthUnit::Millimeter,
            )),
            "test",
        )
        .unwrap();
    graph.set_instance_frame("source", Some("mount")).unwrap();
    graph
        .set_placement(
            "source",
            Placement::translated(VectorQuantity::lengths(
                10.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    original = config::assembly_document(&original, &graph);
    let setup = setup(90.0, -90.0, 3);
    let (graph, study) = config::prepare(&original, &setup).unwrap();
    let session = Session::new().unwrap();
    let moving = graph
        .resolve_with_placement("moving")
        .unwrap()
        .regenerate(&session)
        .unwrap();
    let bounds = session
        .exact_bounds(moving.shape("elevon").unwrap())
        .unwrap();
    assert!((bounds.min.x - 99.5).abs() < 1e-7);
    assert!((bounds.min.y - 200.0).abs() < 1e-7);
    assert!((bounds.min.z - 282.0).abs() < 1e-7);
    let fixed = graph
        .resolve_with_placement("fixed")
        .unwrap()
        .regenerate(&session)
        .unwrap();
    let bounds = session.exact_bounds(fixed.shape("wing").unwrap()).unwrap();
    assert!((bounds.min.x - 106.0).abs() < 1e-7);
    assert_eq!(study.samples[1].positions[0].value.value, 0.0);
    drop((moving, fixed));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn malformed_setups_reject_before_geometry_or_source_changes() {
    let original = fixture();
    let accepted = original.clone();
    let valid = setup(0.0, 30.0, 3);
    let mut cases = vec![];
    let mut add = |edit: fn(&mut Setup)| {
        let mut value = valid.clone();
        edit(&mut value);
        cases.push(value);
    };
    add(|s| s.schema = "future".into());
    add(|s| s.samples = 1);
    add(|s| s.components.clear());
    add(|s| s.components[1].hinge = None);
    add(|s| s.components[1].id = "fixed".into());
    add(|s| s.components[1].id = "source".into());
    add(|s| s.components[1].source = "missing".into());
    add(|s| s.components[1].output = "missing".into());
    add(|s| s.components[1].hinge.as_mut().unwrap().axis = [0.0; 3]);
    add(|s| s.components[1].hinge.as_mut().unwrap().minimum_deg = 40.0);
    add(|s| s.components[1].hinge.as_mut().unwrap().end_deg = 400.0);
    add(|s| s.components[1].hinge.as_mut().unwrap().start_deg = f64::NAN);
    for setup in cases {
        assert!(config::prepare(&original, &setup).is_err(), "{setup:?}");
    }
    assert_eq!(original, accepted);
    assert!(
        serde_json::from_value::<Setup>(
            json!({"schema": "occb-motion-study-v1", "samples": 3, "components": [], "typo": true})
        )
        .is_err()
    );
}

#[test]
fn ten_thousand_hinges_prepare_in_one_batch_and_coordinate_budget_is_enforced() {
    let original = fixture();
    let mut setup = setup(-30.0, 30.0, 2);
    let part = setup.components[1].clone();
    setup.components = (0..10_000)
        .map(|index| Component {
            id: format!("hinged-{index}"),
            ..part.clone()
        })
        .collect();
    let started = std::time::Instant::now();
    let (graph, study) = config::prepare(&original, &setup).unwrap();
    assert_eq!(graph.joints().count(), 10_000);
    assert_eq!(study.outputs.len(), 10_000);
    assert_eq!(study.samples[1].positions.len(), 10_000);
    assert!(
        started.elapsed().as_secs_f64() < 10.0,
        "10000 hinge setup exceeds 10s budget"
    );
    eprintln!("10000 hinge setup: {:?} (10s budget)", started.elapsed());
    setup.samples = 101;
    assert!(
        config::prepare(&original, &setup)
            .err()
            .unwrap()
            .to_string()
            .contains("one million")
    );
}

#[test]
fn coordinated_hinges_follow_independent_signed_travel_and_clearance_policy() {
    let original = fixture();
    let mut setup = setup(-30.0, 30.0, 3);
    let mut second = setup.components[1].clone();
    second.id = "opposite".into();
    let hinge = second.hinge.as_mut().unwrap();
    hinge.start_deg = 20.0;
    hinge.end_deg = -40.0;
    setup.components.push(second);
    let (_, study) = config::prepare(&original, &setup).unwrap();
    assert_eq!(study.samples[1].positions.len(), 2);
    assert_eq!(study.samples[1].positions[0].value.value, 0.0);
    assert!((study.samples[1].positions[1].value.value - (-10.0_f64).to_radians()).abs() < 1e-14);
    setup.components.pop();
    setup.collision_options.minimum_clearance = Quantity::length(4.0, LengthUnit::Millimeter);
    let (graph, study) = config::prepare(&original, &setup).unwrap();
    let session = Session::new().unwrap();
    let sampled = graph.run_motion_study(&session, &study).unwrap();
    let continuous = graph
        .check_continuous_motion(&session, &study, setup.continuous_options)
        .unwrap();
    assert!(!report::clear(&sampled, &continuous));
    assert_eq!(
        report::document(&study, &sampled, &continuous)["sampled"]["samples"][1]["collisions"][0]["status"],
        "insufficient_clearance"
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "occb-motion-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn inputs(directory: &Directory, start: f64, end: f64) -> Vec<OsString> {
    let model = directory.0.join("source.json");
    let setup = directory.0.join("setup.json");
    fs::write(&model, fixture().to_json_pretty().unwrap()).unwrap();
    fs::write(
        &setup,
        json!({
            "schema":"occb-motion-study-v1", "samples":2,
            "components":[
                {"id":"fixed","source":"source","output":"wing"},
                {"id":"moving","source":"source","output":"elevon", "hinge":{
                    "origin_mm":[0,0,0], "axis":[0,1,0], "minimum_deg":-360,"maximum_deg":360,
                    "start_deg":start,"end_deg":end
                }}
            ]
        })
        .to_string(),
    )
    .unwrap();
    vec![
        model.into_os_string(),
        setup.into_os_string(),
        directory.0.join("output").into_os_string(),
    ]
}

#[test]
fn command_writes_reloadable_artifacts_and_rejects_overwrites_or_invalid_inputs() {
    let directory = Directory::new();
    let args = inputs(&directory, -30.0, 30.0);
    let source = fs::read(&args[0]).unwrap();
    assert!(run(&args).unwrap());
    let output = Path::new(&args[2]);
    ModelDocument::from_json(&fs::read_to_string(output.join("assembly.model.json")).unwrap())
        .unwrap();
    serde_json::from_str::<MotionStudy>(&fs::read_to_string(output.join("study.json")).unwrap())
        .unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output.join("motion.report.json")).unwrap())
            .unwrap();
    assert_eq!(report["continuous"]["status"], "clear");
    assert!(
        run(&args)
            .unwrap_err()
            .to_string()
            .contains("already exists")
    );
    assert!(run(&[]).unwrap_err().to_string().contains("usage"));
    assert_eq!(fs::read(&args[0]).unwrap(), source);
    let directory = Directory::new();
    let args = inputs(&directory, 0.0, 360.0);
    assert!(!run(&args).unwrap());
    let directory = Directory::new();
    let args = inputs(&directory, 0.0, 400.0);
    assert!(run(&args).is_err());
    assert!(!Path::new(&args[2]).exists());
}

#[test]
fn command_exclusions_are_explicit_in_reloadable_study_and_report() {
    let directory = Directory::new();
    let args = inputs(&directory, 0.0, 360.0);
    let source = fs::read(&args[0]).unwrap();
    let mut setup: serde_json::Value =
        serde_json::from_slice(&fs::read(&args[1]).unwrap()).unwrap();
    setup["excluded_pairs"] = json!([["moving", "fixed"]]);
    fs::write(&args[1], setup.to_string()).unwrap();
    assert!(run(&args).unwrap());
    let output = Path::new(&args[2]);
    let study: MotionStudy =
        serde_json::from_str(&fs::read_to_string(output.join("study.json")).unwrap()).unwrap();
    assert_eq!(study.excluded_pairs.len(), 1);
    assert_eq!(study.excluded_pairs[0].first.instance, "moving");
    assert_eq!(study.excluded_pairs[0].second.output, "wing");
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output.join("motion.report.json")).unwrap())
            .unwrap();
    assert_eq!(
        report["excluded_pairs"],
        serde_json::to_value(&study.excluded_pairs).unwrap()
    );
    assert_eq!(report["continuous"]["candidate_pairs"], 0);
    assert_eq!(report["continuous"]["exact_queries"], 0);
    assert_eq!(report["passed"], true);
    assert_eq!(fs::read(&args[0]).unwrap(), source);
}
#[test]
fn invalid_command_exclusions_reject_without_publishing_artifacts() {
    for pairs in [
        json!([["moving", "moving"]]),
        json!([["moving", "unknown"]]),
        json!([["moving", "fixed"], ["fixed", "moving"]]),
        json!([["moving"]]),
    ] {
        let directory = Directory::new();
        let args = inputs(&directory, 0.0, 360.0);
        let source = fs::read(&args[0]).unwrap();
        let mut setup: serde_json::Value =
            serde_json::from_slice(&fs::read(&args[1]).unwrap()).unwrap();
        setup["excluded_pairs"] = pairs;
        fs::write(&args[1], setup.to_string()).unwrap();
        assert!(run(&args).is_err());
        assert!(!Path::new(&args[2]).exists());
        assert_eq!(fs::read(&args[0]).unwrap(), source);
    }
}

fn slider_setup(start: f64, end: f64) -> Setup {
    let mut setup = setup(0.0, 0.0, 3);
    setup.components[1].hinge = None;
    setup.components[1].slider = Some(Slider {
        axis: [2.0, 0.0, 0.0],
        minimum_mm: -50.0,
        maximum_mm: 50.0,
        start_mm: start,
        end_mm: end,
    });
    setup
}

#[test]
fn sliders_use_normalized_axes_and_millimeter_coordinates_and_reload() {
    let original = fixture();
    let accepted = original.clone();
    let setup = slider_setup(4.0, 40.0);
    let (graph, study) = config::prepare(&original, &setup).unwrap();
    let frame = config::slider_frame_id("moving");
    assert_eq!(study.samples[0].positions[0].coordinate, JointDof::Axial);
    assert_eq!(study.samples[1].positions[0].value.value, 22.0);
    assert_eq!(study.samples[2].positions[0].value.value, 40.0);
    assert_eq!(
        study.samples[1].positions[0].value.dimension,
        Dimension::Length
    );
    let assembled = config::assembly_document(&original, &graph);
    let loaded = ModelDocument::from_json(&assembled.to_json_pretty().unwrap()).unwrap();
    let mut restored = loaded.instance_graph().unwrap();
    assert!(matches!(
        restored
            .joints()
            .find(|joint| joint.frame == frame)
            .unwrap()
            .kind,
        JointKind::Prismatic { .. }
    ));
    restored
        .set_joint_coordinate(&frame, JointDof::Axial, study.samples[2].positions[0].value)
        .unwrap();
    let session = Session::new().unwrap();
    let generated = restored
        .resolve_with_placement("moving")
        .unwrap()
        .regenerate(&session)
        .unwrap();
    let bounds = session
        .exact_bounds(generated.shape("elevon").unwrap())
        .unwrap();
    assert!((bounds.min.x - 42.0).abs() < 1e-7);
    assert!((bounds.max.x - 48.0).abs() < 1e-7);
    drop(generated);
    let sampled = graph.run_motion_study(&session, &study).unwrap();
    let continuous = graph
        .check_continuous_motion(&session, &study, Default::default())
        .unwrap();
    assert!(report::clear(&sampled, &continuous));
    assert_eq!(continuous.generated_variants, 1);
    assert_eq!(session.shape_count().unwrap(), 0);
    assert_eq!(original, accepted);
}

#[test]
fn sliders_follow_source_placements_and_rotated_parent_frames_in_reverse_travel() {
    let mut original = fixture();
    let mut graph = original.instance_graph().unwrap();
    graph
        .add_frame(
            "mount",
            None,
            Placement {
                translation: VectorQuantity::lengths(100.0, 200.0, 300.0, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: std::f64::consts::FRAC_PI_2,
                }),
            },
            "test",
        )
        .unwrap();
    graph.set_instance_frame("source", Some("mount")).unwrap();
    graph
        .set_placement(
            "source",
            Placement::translated(VectorQuantity::lengths(
                10.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    original = config::assembly_document(&original, &graph);
    let (mut graph, study) = config::prepare(&original, &slider_setup(4.0, -6.0)).unwrap();
    assert_eq!(study.samples[1].positions[0].value.value, -1.0);
    let session = Session::new().unwrap();
    for (distance, y) in [(4.0, 216.0), (-6.0, 206.0)] {
        graph
            .set_joint_coordinate(
                &config::slider_frame_id("moving"),
                JointDof::Axial,
                Quantity::length(distance, LengthUnit::Millimeter),
            )
            .unwrap();
        let generated = graph
            .resolve_with_placement("moving")
            .unwrap()
            .regenerate(&session)
            .unwrap();
        let bounds = session
            .exact_bounds(generated.shape("elevon").unwrap())
            .unwrap();
        assert!((bounds.min.x - 80.0).abs() < 1e-7);
        assert!((bounds.min.y - y).abs() < 1e-7);
        assert!((bounds.min.z - 299.5).abs() < 1e-7);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn mixed_hinges_and_sliders_advance_together_and_reject_invalid_drives() {
    let original = fixture();
    let mut setup = slider_setup(0.0, 40.0);
    let mut hinged = self::setup(-30.0, 30.0, 3).components[1].clone();
    hinged.id = "rotating".into();
    setup.components.push(hinged);
    let (_, study) = config::prepare(&original, &setup).unwrap();
    assert_eq!(study.samples[1].positions.len(), 2);
    assert_eq!(study.samples[1].positions[0].coordinate, JointDof::Axial);
    assert_eq!(study.samples[1].positions[0].value.value, 20.0);
    assert_eq!(study.samples[1].positions[1].coordinate, JointDof::Angle);
    assert_eq!(study.samples[1].positions[1].value.value, 0.0);
    let mut sources = original.instance_graph().unwrap();
    sources
        .add_clone("turn-source", "source", HashMap::new(), "test")
        .unwrap();
    sources
        .set_placement(
            "turn-source",
            Placement::translated(VectorQuantity::lengths(
                100.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    let placed = config::assembly_document(&original, &sources);
    setup.components[2].source = "turn-source".into();
    setup.components[2].hinge.as_mut().unwrap().origin_mm = [100.0, 0.0, 0.0];
    let (graph, study) = config::prepare(&placed, &setup).unwrap();
    let session = Session::new().unwrap();
    let sampled = graph.run_motion_study(&session, &study).unwrap();
    let continuous = graph
        .check_continuous_motion(&session, &study, Default::default())
        .unwrap();
    assert!(report::clear(&sampled, &continuous));
    assert_eq!(continuous.generated_variants, 1);
    assert_eq!(session.shape_count().unwrap(), 0);
    let valid = slider_setup(0.0, 40.0);
    let mut cases = Vec::new();
    for edit in [
        |s: &mut Slider| s.axis = [0.0; 3],
        |s: &mut Slider| s.axis[0] = f64::NAN,
        |s: &mut Slider| s.minimum_mm = 45.0,
        |s: &mut Slider| s.maximum_mm = -1.0,
        |s: &mut Slider| s.start_mm = 51.0,
        |s: &mut Slider| s.end_mm = -51.0,
        |s: &mut Slider| s.end_mm = f64::INFINITY,
    ] {
        let mut setup = valid.clone();
        edit(setup.components[1].slider.as_mut().unwrap());
        cases.push(setup);
    }
    let mut both = valid.clone();
    both.components[1].hinge = self::setup(0.0, 30.0, 3).components[1].hinge.clone();
    cases.push(both);
    for setup in cases {
        assert!(config::prepare(&original, &setup).is_err());
    }
}

#[test]
fn ten_thousand_mixed_drives_prepare_within_budget() {
    let original = fixture();
    let mut setup = slider_setup(0.0, 40.0);
    setup.samples = 2;
    let slider = setup.components[1].clone();
    let hinge = self::setup(-30.0, 30.0, 2).components[1].clone();
    setup.components = (0..10_000)
        .map(|index| Component {
            id: format!("drive-{index}"),
            ..if index % 2 == 0 {
                slider.clone()
            } else {
                hinge.clone()
            }
        })
        .collect();
    let start = std::time::Instant::now();
    let (graph, study) = config::prepare(&original, &setup).unwrap();
    assert_eq!(graph.joints().count(), 10_000);
    assert_eq!(study.samples[1].positions.len(), 10_000);
    assert_eq!(
        study.samples[1]
            .positions
            .iter()
            .filter(|position| position.coordinate == JointDof::Axial)
            .count(),
        5000
    );
    assert!(start.elapsed().as_secs_f64() < 10.0);
    eprintln!(
        "10000 mixed hinge/slider preparation: {:?} (10s budget)",
        start.elapsed()
    );
    setup.samples = 101;
    assert!(config::prepare(&original, &setup).is_err());
}

#[test]
fn slider_command_reports_clear_paths_and_crossings_missed_by_samples() {
    for (start, end, clear) in [(0.0, 40.0, true), (20.0, -20.0, false)] {
        let directory = Directory::new();
        let args = inputs(&directory, 0.0, 0.0);
        let source = fs::read(&args[0]).unwrap();
        let mut setup: serde_json::Value =
            serde_json::from_slice(&fs::read(&args[1]).unwrap()).unwrap();
        setup["components"][1]
            .as_object_mut()
            .unwrap()
            .remove("hinge");
        setup["components"][1]["slider"] = json!({"axis":[2,0,0], "minimum_mm":-50, "maximum_mm":50, "start_mm":start, "end_mm":end});
        fs::write(&args[1], setup.to_string()).unwrap();
        assert_eq!(run(&args).unwrap(), clear);
        let output = Path::new(&args[2]);
        let document = ModelDocument::from_json(
            &fs::read_to_string(output.join("assembly.model.json")).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            document
                .instance_graph()
                .unwrap()
                .joints()
                .find(|joint| joint.frame == config::slider_frame_id("moving"))
                .unwrap()
                .kind,
            JointKind::Prismatic { .. }
        ));
        let study: MotionStudy =
            serde_json::from_str(&fs::read_to_string(output.join("study.json")).unwrap()).unwrap();
        assert_eq!(study.samples[1].positions[0].coordinate, JointDof::Axial);
        assert_eq!(study.samples[1].positions[0].value.value, end);
        let report: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(output.join("motion.report.json")).unwrap())
                .unwrap();
        assert_eq!(report["passed"], clear);
        assert!(
            report["sampled"]["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|sample| sample["collisions"].as_array().unwrap().is_empty())
        );
        if clear {
            assert_eq!(report["continuous"]["status"], "clear");
        } else {
            assert_eq!(report["continuous"]["status"], "collision");
            let fraction = report["continuous"]["pairs"][0]["fraction"]
                .as_f64()
                .unwrap();
            assert!(fraction > 0.0 && fraction < 1.0);
        }
        assert_eq!(fs::read(&args[0]).unwrap(), source);
    }
}

#[test]
fn invalid_slider_commands_reject_without_publishing() {
    for slider in [
        json!({"axis":[0,0,0],"minimum_mm":-50,"maximum_mm":50,"start_mm":0,"end_mm":40}),
        json!({"axis":[1,0,0],"minimum_mm":-50,"maximum_mm":50,"start_mm":0,"end_mm":60}),
        json!({"axis":[1,0,0],"minimum_mm":-50,"maximum_mm":50,"start_mm":0,"end_mm":40,"typo":1}),
    ] {
        let directory = Directory::new();
        let args = inputs(&directory, 0.0, 0.0);
        let mut setup: serde_json::Value =
            serde_json::from_slice(&fs::read(&args[1]).unwrap()).unwrap();
        setup["components"][1]
            .as_object_mut()
            .unwrap()
            .remove("hinge");
        setup["components"][1]["slider"] = slider;
        fs::write(&args[1], setup.to_string()).unwrap();
        assert!(run(&args).is_err());
        assert!(!Path::new(&args[2]).exists());
    }
}

#[test]
fn shipped_slider_example_detects_crossing_between_clear_box_poses() {
    let mut family = fixture().family;
    family.features = vec![FeatureDefinition {
        id: "body".into(),
        operation: FeatureOperation::Box {
            origin: vector(0.0, 0.0, 0.0),
            size: vector(10.0, 2.0, 2.0),
        },
    }];
    let mut source = InstanceGraph::new(&family);
    source
        .add_base("prototype", HashMap::new(), "test")
        .unwrap();
    let document = ModelDocument::from_graph(&source);
    let setup: Setup = serde_json::from_str(include_str!(
        "../../../../../tools/motion-study/slider-example.json"
    ))
    .unwrap();
    let (graph, study) = config::prepare(&document, &setup).unwrap();
    let session = Session::new().unwrap();
    let sampled = graph.run_motion_study(&session, &study).unwrap();
    assert!(
        sampled
            .samples
            .iter()
            .all(|sample| sample.collisions.is_empty())
    );
    let continuous = graph
        .check_continuous_motion(&session, &study, Default::default())
        .unwrap();
    assert_eq!(continuous.status, ContinuousStatus::Collision);
    assert_eq!(continuous.generated_variants, 1);
    assert_eq!(session.shape_count().unwrap(), 0);
}
