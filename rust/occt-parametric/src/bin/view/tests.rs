use super::*;
use occt_parametric::*;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn document() -> ModelDocument {
    document_with(2)
}

/// A plate with a fused boss and `members` pattern copies, colored by material.
fn document_with(members: usize) -> ModelDocument {
    let family = FamilyDefinition {
        references: Vec::new(),
        assumptions: Vec::new(),
        feature_colors: Default::default(),
        id: "bracket".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        requirements: Vec::new(),
        datums: Vec::new(),
        features: vec![
            FeatureDefinition {
                id: "plate".into(),
                operation: FeatureOperation::Box {
                    origin: point(0.0, 0.0, 0.0),
                    size: point(40.0, 20.0, 5.0),
                },
            },
            FeatureDefinition {
                id: "boss".into(),
                operation: FeatureOperation::Cylinder {
                    origin: point(20.0, 10.0, 5.0),
                    axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    radius: ScalarExpr::Literal(Quantity::length(6.0, LengthUnit::Millimeter)),
                    height: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
                },
            },
            FeatureDefinition {
                id: "bracket".into(),
                operation: FeatureOperation::Fuse {
                    left: "plate".into(),
                    right: "boss".into(),
                },
            },
        ],
    };
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "row",
            "member",
            "source",
            members,
            VectorQuantity::lengths(60.0, 0.0, 0.0, LengthUnit::Millimeter),
            "test",
        )
        .unwrap();
    graph
        .add_material(Material {
            id: "al".into(),
            name: "Aluminum".into(),
            density_kg_per_cubic_meter: 2700.0,
        })
        .unwrap();
    graph.assign_material("source", Some("al")).unwrap();
    graph
        .set_material_appearance(
            "al",
            Some(MaterialAppearance {
                base_color: [1.0, 0.0, 0.0, 1.0],
                ..MaterialAppearance::default()
            }),
        )
        .unwrap();
    ModelDocument::from_graph(&graph)
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "occt-view-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn model(&self, document: &ModelDocument) -> OsString {
        let path = self.0.join("model.json");
        fs::write(&path, document.to_json_pretty().unwrap()).unwrap();
        path.into_os_string()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn args(values: &[&OsStr]) -> Vec<OsString> {
    values.iter().map(|v| v.to_os_string()).collect()
}

#[test]
fn view_shows_final_feature_of_every_instance_in_material_color() {
    let directory = Directory::new();
    let model = directory.model(&document());
    let view = directory.0.join("view");
    run(
        &args(&[
            &model,
            "--dir".as_ref(),
            view.as_os_str(),
            "--no-open".as_ref(),
        ]),
        "unused".as_ref(),
    )
    .unwrap();
    assert!(view.join("model.brep").metadata().unwrap().len() > 0);
    let script = fs::read_to_string(view.join("view.tcl")).unwrap();
    for name in ["source", "member_0_", "member_1_"] {
        assert!(
            script.contains(&format!("vdisplay -dispMode 1 {name}\n")),
            "{script}"
        );
        // Members inherit the source's material.
        assert!(
            script.contains(&format!("vsetcolor {name} #FF0000\n")),
            "{script}"
        );
    }
    assert!(script.contains("source = source:bracket"));
    // An explicit output overrides the final feature; a new directory is required.
    let plate = directory.0.join("plate");
    run(
        &args(&[
            &model,
            "--output".as_ref(),
            "plate".as_ref(),
            "--dir".as_ref(),
            plate.as_os_str(),
            "--no-open".as_ref(),
        ]),
        "unused".as_ref(),
    )
    .unwrap();
    assert!(
        fs::read_to_string(plate.join("view.tcl"))
            .unwrap()
            .contains("source = source:plate")
    );
    assert!(
        run(
            &args(&[
                &model,
                "--dir".as_ref(),
                view.as_os_str(),
                "--no-open".as_ref()
            ]),
            "unused".as_ref()
        )
        .is_err()
    );
}

#[test]
fn viewer_is_started_detached_and_a_missing_viewer_keeps_the_written_view() {
    let directory = Directory::new();
    let model = directory.model(&document());
    let started = directory.0.join("started");
    run(
        &args(&[&model, "--dir".as_ref(), started.as_os_str()]),
        "true".as_ref(),
    )
    .unwrap();
    let missing = directory.0.join("missing");
    let error = run(
        &args(&[&model, "--dir".as_ref(), missing.as_os_str()]),
        "occt-view-no-such-viewer".as_ref(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("could not start"), "{error}");
    assert!(missing.join("view.tcl").exists());
}

#[test]
fn invalid_arguments_models_and_outputs_fail() {
    let directory = Directory::new();
    let model = directory.model(&document());
    let bad = directory.0.join("bad.json");
    fs::write(&bad, "{}").unwrap();
    let fresh = |name: &str| directory.0.join(name).into_os_string();
    let none: [&OsStr; 0] = [];
    for (case, values) in [
        args(&none),
        args(&[&model, "--unknown".as_ref()]),
        args(&[&model, &model]),
        args(&[&model, "--output".as_ref()]),
        args(&[
            &model,
            "--output".as_ref(),
            "missing".as_ref(),
            "--dir".as_ref(),
            &fresh("a"),
            "--no-open".as_ref(),
        ]),
        args(&[
            bad.as_os_str(),
            "--dir".as_ref(),
            &fresh("b"),
            "--no-open".as_ref(),
        ]),
    ]
    .into_iter()
    .enumerate()
    {
        assert!(run(&values, "unused".as_ref()).is_err(), "case {case}");
    }
    assert!(!directory.0.join("b").exists());
}

#[test]
fn appearances_require_known_materials_and_valid_values() {
    let document = document();
    let mut graph = document.instance_graph().unwrap();
    assert!(graph.set_material_appearance("missing", None).is_err());
    let invalid = MaterialAppearance {
        base_color: [2.0, 0.0, 0.0, 1.0],
        ..MaterialAppearance::default()
    };
    assert!(graph.set_material_appearance("al", Some(invalid)).is_err());
    graph.set_material_appearance("al", None).unwrap();
    assert!(graph.assembly().material_appearances.is_empty());
}

#[test]
fn watch_reloads_the_viewer_after_valid_saves_and_stops_when_it_closes() {
    use std::os::unix::fs::PermissionsExt;
    let directory = Directory::new();
    let model = directory.model(&document());
    // Stand-in viewer: records the first command it is sent, then exits.
    let viewer = directory.0.join("viewer.sh");
    fs::write(&viewer, "#!/bin/sh\nhead -n 1 > received\n").unwrap();
    fs::set_permissions(&viewer, fs::Permissions::from_mode(0o755)).unwrap();
    let view = directory.0.join("view");
    let watch_args = args(&[
        &model,
        "--dir".as_ref(),
        view.as_os_str(),
        "--watch".as_ref(),
    ]);
    let (done, finished) = std::sync::mpsc::channel();
    let viewer_path = viewer.clone().into_os_string();
    thread::spawn(move || {
        let result = run(&watch_args, &viewer_path).map_err(|e| e.to_string());
        done.send(result).unwrap();
    });
    let wait_for = |path: &Path| {
        for _ in 0..200 {
            if path.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("{} never appeared", path.display());
    };
    wait_for(&view.join("view.tcl"));
    assert!(
        !fs::read_to_string(view.join("reload.tcl"))
            .unwrap()
            .contains("member_2_")
    );
    // A broken save is reported and skipped; the next valid one reloads.
    fs::write(&model, "{").unwrap();
    thread::sleep(POLL * 4);
    fs::write(&model, document_with(3).to_json_pretty().unwrap()).unwrap();
    let result = finished.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        fs::read_to_string(view.join("received")).unwrap(),
        "source reload.tcl\n"
    );
    assert!(
        fs::read_to_string(view.join("reload.tcl"))
            .unwrap()
            .contains("vdisplay -dispMode 1 member_2_\n")
    );
}
