use super::*;
use occt_parametric::*;
use serde_json::json;
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
        parameters: vec![ParameterDefinition {
            id: "thickness".into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
            minimum: Some(Quantity::length(1.0, LengthUnit::Millimeter)),
            maximum: Some(Quantity::length(20.0, LengthUnit::Millimeter)),
        }],
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
                    size: VectorExpr::Components {
                        x: ScalarExpr::Literal(Quantity::length(40.0, LengthUnit::Millimeter)),
                        y: ScalarExpr::Literal(Quantity::length(20.0, LengthUnit::Millimeter)),
                        z: ScalarExpr::Parameter("thickness".into()),
                    },
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

fn get(studio: &mut serve::Studio, path: &str) -> (u16, serde_json::Value) {
    let response = studio.handle("GET", path, b"");
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap_or(serde_json::Value::Null),
    )
}

fn thickness(state: &serde_json::Value) -> f64 {
    state["parameters"][0]["value"].as_f64().unwrap()
}

#[test]
fn studio_serves_the_page_and_model_and_applies_saves_and_follows_edits() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let page = studio.handle("GET", "/", b"");
    assert_eq!(page.status, 200);
    assert!(
        String::from_utf8(page.body)
            .unwrap()
            .contains("./viewer.mjs")
    );
    assert_eq!(studio.handle("GET", "/viewer.mjs", b"").status, 200);
    assert_eq!(
        studio.handle("GET", "/sketch-dependencies.mjs", b"").status,
        200
    );
    assert_eq!(
        get(&mut studio, "/api/sketches").1["feature_dependencies"]["bracket"],
        json!(["boss", "plate"])
    );
    let (status, state) = get(&mut studio, "/api/state");
    assert_eq!(status, 200);
    assert_eq!(
        (state["version"].as_u64(), thickness(&state)),
        (Some(1), 5.0)
    );
    assert_eq!(state["parameters"][0]["unit"], "millimeter");
    let (_, gltf) = get(&mut studio, "/api/model.gltf");
    let names: Vec<_> = gltf["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].clone())
        .collect();
    assert_eq!(
        names,
        [json!("member[0]"), json!("member[1]"), json!("source")]
    );

    // Edits apply together or not at all, and never touch the file.
    let before = fs::read(&model).unwrap();
    let edited = studio.handle("POST", "/api/parameters", br#"{"set": {"thickness": 8}}"#);
    assert_eq!(edited.status, 200);
    let (_, state) = get(&mut studio, "/api/state");
    assert_eq!(
        (
            state["version"].as_u64(),
            state["dirty"].as_bool(),
            thickness(&state)
        ),
        (Some(2), Some(true), 8.0)
    );
    assert_ne!(get(&mut studio, "/api/model.gltf").1, gltf);
    for bad in [
        &br#"{"set": {"thickness": 100}}"#[..],
        br#"{"set": {"missing": 1}}"#,
        br#"{"set": {"thickness": true}}"#,
        b"[",
        br#"{"set": {"thickness": 9, "missing": 1}}"#,
        br#"{"thickness": 9}"#,
        br#"{"clear": ["thickness"]}"#,
        br#"{"instance": "nobody", "set": {"thickness": 9}}"#,
        br#"{"instance": "source", "set": {"thickness": 50}}"#,
    ] {
        assert_eq!(studio.handle("POST", "/api/parameters", bad).status, 422);
    }
    assert_eq!(thickness(&get(&mut studio, "/api/state").1), 8.0);
    assert_eq!(fs::read(&model).unwrap(), before);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    assert_eq!(
        saved.family.parameters[0].default,
        ParameterValue::Scalar(Quantity::length(8.0, LengthUnit::Millimeter))
    );
    assert_eq!(get(&mut studio, "/api/state").1["dirty"], false);

    assert_eq!(get(&mut studio, "/api/state").1["undo_count"], 1);

    // Saves made elsewhere are followed once settled; broken ones are reported.
    let follow = |studio: &mut serve::Studio| {
        for _ in 0..4 {
            thread::sleep(POLL + Duration::from_millis(20));
            studio.handle("GET", "/api/state", b"");
        }
        get(studio, "/api/state").1
    };
    let mut changed = saved.clone();
    changed.family.parameters[0].default =
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter));
    fs::write(&model, changed.to_json_pretty().unwrap()).unwrap();
    let state = follow(&mut studio);
    assert_eq!(state["undo_count"], 0);
    assert_eq!(state["redo_count"], 0);
    assert_eq!(
        (thickness(&state), state["version"].as_u64()),
        (3.0, Some(3))
    );
    fs::write(&model, "{").unwrap();
    let state = follow(&mut studio);
    assert_eq!(
        (thickness(&state), state["version"].as_u64()),
        (3.0, Some(3))
    );
    assert!(state["error"].as_str().unwrap().contains("not loaded"));
    assert_eq!(studio.handle("GET", "/missing", b"").status, 404);
    assert_eq!(studio.handle("DELETE", "/api/state", b"").status, 405);
}

#[test]
fn server_accepts_only_local_hosts_and_marked_posts() {
    use std::io::{Read as _, Write as _};
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    // Serves until the test process exits.
    thread::spawn(move || {
        let _ = serve::serve(&mut studio, &listener);
    });
    let request = |text: String| {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(text.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u16>()
            .unwrap()
    };
    let host = format!("127.0.0.1:{port}");
    assert_eq!(
        request(format!("GET /api/state HTTP/1.1\r\nHost: {host}\r\n\r\n")),
        200
    );
    assert_eq!(
        request(format!(
            "GET /api/state HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n"
        )),
        200
    );
    assert_eq!(
        request("GET /api/state HTTP/1.1\r\nHost: attacker.example\r\n\r\n".into()),
        403
    );
    assert_eq!(request("GET /api/state HTTP/1.1\r\n\r\n".into()), 403);
    let body = r#"{"set": {"thickness": 6}}"#;
    assert_eq!(
        request(format!(
            "POST /api/parameters HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )),
        403
    );
    assert_eq!(
        request(format!(
            "POST /api/parameters HTTP/1.1\r\nHost: {host}\r\nX-OCCT-View: 1\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )),
        200
    );
    assert_eq!(
        request(format!(
            "POST /api/save HTTP/1.1\r\nHost: {host}\r\nX-OCCT-View: 1\r\nContent-Length: 99999999\r\n\r\n"
        )),
        413
    );
    assert_eq!(request("garbage\r\n\r\n".into()), 400);
}

fn source_of(view: &serde_json::Value) -> (f64, String) {
    let parameter = &view["parameters"][0];
    (
        parameter["value"].as_f64().unwrap(),
        parameter["source"].as_str().unwrap().to_owned(),
    )
}

#[test]
fn instances_take_own_overrides_inherit_from_sources_and_reset() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let (_, list) = get(&mut studio, "/api/instances");
    let ids: Vec<_> = list["instances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, ["member[0]", "member[1]", "source"]);
    let view =
        |studio: &mut serve::Studio, id: &str| get(studio, &format!("/api/instance?id={id}")).1;
    assert_eq!(
        source_of(&view(&mut studio, "member%5B0%5D")),
        (5.0, "default".into())
    );

    // A source override is inherited by its pattern members.
    let edit = |studio: &mut serve::Studio, body: &str| {
        studio
            .handle("POST", "/api/parameters", body.as_bytes())
            .status
    };
    assert_eq!(
        edit(
            &mut studio,
            r#"{"instance": "source", "set": {"thickness": 9}}"#
        ),
        200
    );
    assert_eq!(source_of(&view(&mut studio, "source")), (9.0, "own".into()));
    assert_eq!(
        source_of(&view(&mut studio, "member%5B1%5D")),
        (9.0, "inherited".into())
    );
    // A member's own override wins, and the family default stays put.
    assert_eq!(
        edit(
            &mut studio,
            r#"{"instance": "member[1]", "set": {"thickness": 12}}"#
        ),
        200
    );
    assert_eq!(
        source_of(&view(&mut studio, "member%5B1%5D")),
        (12.0, "own".into())
    );
    assert_eq!(thickness(&get(&mut studio, "/api/state").1), 5.0);
    let (_, list) = get(&mut studio, "/api/instances");
    assert_eq!(list["instances"][1]["overrides"], json!(["thickness"]));
    // The overridden member is generated differently from its sibling.
    let (_, gltf) = get(&mut studio, "/api/model.gltf");
    assert_eq!(gltf["meshes"].as_array().unwrap().len(), 2);
    // Reset removes the override so the member inherits again.
    assert_eq!(
        edit(
            &mut studio,
            r#"{"instance": "member[1]", "clear": ["thickness"]}"#
        ),
        200
    );
    assert_eq!(
        source_of(&view(&mut studio, "member%5B1%5D")),
        (9.0, "inherited".into())
    );
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    let source = saved.instances.iter().find(|n| n.id() == "source").unwrap();
    assert_eq!(
        source.overrides().get("thickness"),
        Some(&ParameterValue::Scalar(Quantity::length(
            9.0,
            LengthUnit::Millimeter
        )))
    );
    assert_eq!(
        studio.handle("GET", "/api/instance?id=nobody", b"").status,
        404
    );
    assert_eq!(studio.handle("GET", "/api/instance", b"").status, 400);
    assert_eq!(
        studio.handle("GET", "/api/instance?id=%zz", b"").status,
        400
    );
}

#[test]
fn instance_placements_are_shown_and_edited_but_pattern_members_follow_their_rule() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let placement = |studio: &mut serve::Studio, id: &str| {
        get(studio, &format!("/api/instance?id={id}")).1["placement"].clone()
    };
    assert_eq!(
        placement(&mut studio, "member%5B1%5D"),
        json!({"translation_mm": [60.0, 0.0, 0.0], "rotation": null})
    );
    let edit = |studio: &mut serve::Studio, body: serde_json::Value| {
        let response = studio.handle("POST", "/api/parameters", body.to_string().as_bytes());
        (response.status, String::from_utf8(response.body).unwrap())
    };
    let turned = json!({
        "translation_mm": [5.0, -10.0, 2.5],
        "rotation": {"origin_mm": [20.0, 10.0, 0.0], "axis": [0.0, 0.0, 2.0], "angle_degrees": 30.0}
    });
    assert_eq!(
        edit(
            &mut studio,
            json!({"instance": "source", "placement": turned})
        )
        .0,
        200
    );
    assert_eq!(placement(&mut studio, "source"), turned);
    // The turned source is drawn with a rotation matrix.
    let (_, gltf) = get(&mut studio, "/api/model.gltf");
    let source = gltf["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "source")
        .unwrap();
    assert!(source.get("matrix").is_some() || gltf["meshes"].as_array().unwrap().len() > 1);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    let node = saved.instances.iter().find(|n| n.id() == "source").unwrap();
    assert_eq!(
        node.placement().rotation.unwrap().angle_radians,
        30f64.to_radians()
    );

    let member = json!({"translation_mm": [999.0, 0.0, 0.0], "rotation": null});
    let (status, message) = edit(
        &mut studio,
        json!({"instance": "member[1]", "placement": member}),
    );
    assert_eq!(status, 422);
    assert!(
        message.contains("pattern") || message.contains("member"),
        "{message}"
    );
    assert_eq!(
        placement(&mut studio, "member%5B1%5D")["translation_mm"],
        json!([60.0, 0.0, 0.0])
    );
    for bad in [
        json!({"placement": turned}),
        json!({"instance": "source", "placement": {"translation_mm": [0.0, 0.0, 0.0], "rotation": {"origin_mm": [0.0, 0.0, 0.0], "axis": [0.0, 0.0, 0.0], "angle_degrees": 5.0}}}),
        json!({"instance": "source", "placement": {"translation_mm": [0.0, 0.0]}}),
    ] {
        assert_eq!(edit(&mut studio, bad).0, 422);
    }
}

#[test]
fn instances_are_copied_and_deleted_unless_something_depends_on_them_and_revert_undoes() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let change = |studio: &mut serve::Studio, body: serde_json::Value| {
        let response = studio.handle("POST", "/api/instances", body.to_string().as_bytes());
        let value: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        (response.status, value)
    };
    let ids = |studio: &mut serve::Studio| -> Vec<String> {
        get(studio, "/api/instances").1["instances"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let moved = json!({"translation_mm": [0.0, 80.0, 0.0], "rotation": null});
    let (status, state) = change(
        &mut studio,
        json!({"add": {"source": "source", "placement": moved}}),
    );
    assert_eq!(
        (status, state["added"].as_str()),
        (200, Some("source-copy"))
    );
    assert_eq!(state["dirty"], true);
    let (_, state) = change(&mut studio, json!({"add": {"source": "source"}}));
    assert_eq!(state["added"], "source-copy2");
    assert_eq!(
        ids(&mut studio),
        [
            "member[0]",
            "member[1]",
            "source",
            "source-copy",
            "source-copy2"
        ]
    );
    // A copy inherits the source's parameters and material.
    assert_eq!(
        studio
            .handle(
                "POST",
                "/api/parameters",
                br#"{"instance": "source", "set": {"thickness": 7}}"#
            )
            .status,
        200
    );
    let copy = get(&mut studio, "/api/instance?id=source-copy").1;
    assert_eq!(source_of(&copy), (7.0, "inherited".into()));
    assert_eq!(copy["placement"], moved);
    let (_, gltf) = get(&mut studio, "/api/model.gltf");
    assert_eq!(gltf["nodes"].as_array().unwrap().len(), 5);

    // Instances others depend on cannot be deleted; others can.
    for (body, fragment) in [
        (json!({"delete": "source"}), "pattern 'row'"),
        (json!({"delete": "member[0]"}), "pattern 'row'"),
        (json!({"delete": "nobody"}), "unknown"),
        (json!({"add": {"source": "nobody"}}), "unknown"),
        (
            json!({"add": {"source": "source", "id": "member[0]"}}),
            "exists",
        ),
        (json!({"add": {"source": "source", "id": " "}}), "empty"),
        (json!({"rename": "x"}), "invalid"),
    ] {
        let (status, value) = change(&mut studio, body.clone());
        assert_eq!(status, 422, "{body}");
        assert!(
            value["error"].as_str().unwrap().contains(fragment),
            "{body}: {value}"
        );
    }
    assert_eq!(
        change(&mut studio, json!({"delete": "source-copy2"})).0,
        200
    );
    assert_eq!(ids(&mut studio).len(), 4);

    // Revert discards every unsaved change.
    let (status, state) = {
        let response = studio.handle("POST", "/api/revert", b"{}");
        (
            response.status,
            serde_json::from_slice::<serde_json::Value>(&response.body).unwrap(),
        )
    };
    assert_eq!((status, state["dirty"].as_bool()), (200, Some(false)));
    assert_eq!(ids(&mut studio), ["member[0]", "member[1]", "source"]);
}

/// The fixture with an advisory volume limit per part and an advisory 30 mm
/// clearance between parts that sit 20 mm apart.
fn checked_document() -> ModelDocument {
    let mut document = document();
    document.family.requirements.push(Requirement {
        id: "light".into(),
        version: 1,
        kind: RequirementKind::Dimensional,
        priority: RequirementPriority::Advisory,
        statement: "Each part stays under 5,500 mm³.".into(),
        rule: VerificationRule::VolumeRange {
            output: "bracket".into(),
            minimum: Volume {
                value: 1.0,
                unit: LengthUnit::Millimeter,
            },
            maximum: Volume {
                value: 5_500.0,
                unit: LengthUnit::Millimeter,
            },
        },
        provenance: "test".into(),
        traces: Vec::new(),
    });
    document.assembly.requirements.push(AssemblyRequirement {
        id: "spacing".into(),
        version: 1,
        kind: RequirementKind::Assembly,
        priority: RequirementPriority::Advisory,
        statement: "Parts keep 30 mm apart.".into(),
        rule: AssemblyVerificationRule::MinimumClearance {
            first: OutputSet::AllWithOutput("bracket".into()),
            second: None,
            minimum: Quantity::length(30.0, LengthUnit::Millimeter),
        },
        provenance: "test".into(),
    });
    document
}

#[test]
fn requirement_results_follow_edits_per_variant_with_assembly_witnesses() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&checked_document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let (status, report) = get(&mut studio, "/api/requirements");
    assert_eq!(status, 200);
    assert_eq!(report["requirements"]["light"]["priority"], "advisory");
    assert_eq!(report["requirements"]["spacing"]["scope"], "assembly");
    assert_eq!(report["instances"].as_object().unwrap().len(), 3);
    // All three share one variant, which passes the volume limit.
    let variants = report["variants"].as_object().unwrap();
    assert_eq!(variants.len(), 1);
    let light = &variants.values().next().unwrap()[0];
    assert_eq!(
        (light["requirement"].as_str(), light["passed"].as_bool()),
        (Some("light"), Some(true))
    );
    assert_eq!(light["measured"]["unit"], "cubicmillimeter");
    // The 20 mm gaps fail the 30 mm clearance, with witness points.
    let spacing = &report["assembly"][0];
    assert_eq!(spacing["passed"], false);
    assert!(
        !spacing["witness"]["points_mm"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // A thicker member becomes its own variant and fails the volume limit.
    let edit = br#"{"instance": "member[1]", "set": {"thickness": 6}}"#;
    assert_eq!(studio.handle("POST", "/api/parameters", edit).status, 200);
    let (_, report) = get(&mut studio, "/api/requirements");
    assert_eq!(report["variants"].as_object().unwrap().len(), 2);
    let variant = report["instances"]["member[1]"].as_str().unwrap();
    assert_eq!(report["variants"][variant][0]["passed"], false);
    let other = report["instances"]["member[0]"].as_str().unwrap();
    assert_eq!(report["variants"][other][0]["passed"], true);
    assert_eq!(report["version"], 2);
}

#[test]
fn annotated_dimensions_follow_live_edits_instance_scope_and_save_revert() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/sketch-block.request.json"
    ))
    .unwrap();
    let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    document
        .instances
        .push(occt_parametric::InstanceNode::Clone {
            id: "copy".into(),
            source: "block".into(),
            overrides: Default::default(),
            placement: Default::default(),
            frame: None,
            provenance: "test".into(),
        });
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let get = |studio: &mut serve::Studio, id: &str| {
        let response = studio.handle("GET", &format!("/api/annotations?id={id}"), b"");
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        serde_json::from_slice::<serde_json::Value>(&response.body).unwrap()
    };
    let width = |data: &serde_json::Value| {
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "solid")
            .unwrap();
        scene["bounds"][1][0].as_f64().unwrap() - scene["bounds"][0][0].as_f64().unwrap()
    };
    let first = get(&mut studio, "block");
    assert_eq!(first["version"], 1);
    assert_eq!(first["accepted"], true);
    assert!((width(&first) - 40.0).abs() < 1e-6);
    assert_eq!(first, get(&mut studio, "block")); // Cached result has the same evidence.
    assert!(
        first["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["kind"] == "sketch" && s["solver"]["solved"] == true)
    );
    let edit = studio.handle(
        "POST",
        "/api/parameters",
        br#"{"instance":"copy","set":{"width":60}}"#,
    );
    assert_eq!(edit.status, 200);
    let changed = get(&mut studio, "copy");
    assert_eq!(changed["version"], 2);
    assert!((width(&changed) - 60.0).abs() < 1e-6);
    let sketch = changed["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch")
        .unwrap();
    assert!(
        sketch["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "dimension"
                && a["parameters"] == serde_json::json!(["width"])
                && a["label"] == "60.000 mm"
                && a["status"] == "passed")
    );
    assert!((width(&get(&mut studio, "block")) - 40.0).abs() < 1e-6);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let saved = fs::read_to_string(&model).unwrap();
    assert_ne!(saved, original);
    assert_eq!(
        studio
            .handle(
                "POST",
                "/api/parameters",
                br#"{"instance":"copy","set":{"width":70}}"#
            )
            .status,
        200
    );
    assert_eq!(studio.handle("POST", "/api/revert", b"{}").status, 200);
    assert!((width(&get(&mut studio, "copy")) - 60.0).abs() < 1e-6);
    assert_eq!(studio.handle("GET", "/api/annotations", b"").status, 400);
    assert_eq!(
        studio
            .handle("GET", "/api/annotations?id=missing", b"")
            .status,
        422
    );
    for asset in [
        "/assembly_annotations.mjs",
        "/annotations.js",
        "/annotations.css",
        "/annotations.mjs",
        "/annotation-panel.html",
    ] {
        assert_eq!(studio.handle("GET", asset, b"").status, 200);
    }
}

#[test]
fn rejected_dimension_edits_show_conflicts_without_accepting_or_saving_geometry() {
    let directory = Directory::new();
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/sketch-conflict.request.json"
    ))
    .unwrap();
    request["model"]["instances"][0]["base"]["overrides"] = serde_json::json!({});
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let gltf = studio.handle("GET", "/api/model.gltf", b"").body;
    let response = studio.handle(
        "POST",
        "/api/parameters",
        br#"{"instance":"block","set":{"width":60}}"#,
    );
    assert_eq!(response.status, 422);
    let rejected: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(rejected["annotations"]["accepted"], false);
    assert_eq!(rejected["annotations"]["version"], 1);
    let scenes = rejected["annotations"]["scenes"].as_array().unwrap();
    let sketch = scenes.iter().find(|s| s["kind"] == "sketch").unwrap();
    assert_eq!(sketch["solver"]["solved"], false);
    assert!(
        sketch["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["status"] == "failed"
                && a["parameters"] == serde_json::json!(["width"])
                && (a["detail"]["max_residual"].as_f64().unwrap() - 20.0).abs() < 1e-6)
    );
    let state: serde_json::Value =
        serde_json::from_slice(&studio.handle("GET", "/api/state", b"").body).unwrap();
    assert_eq!(state["version"], 1);
    assert_eq!(state["dirty"], false);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, gltf);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let accepted: serde_json::Value =
        serde_json::from_slice(&studio.handle("GET", "/api/annotations?id=block", b"").body)
            .unwrap();
    assert_eq!(accepted["accepted"], true);
    assert!(
        accepted["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["kind"] == "sketch" && s["solver"]["solved"] == true)
    );
}

#[test]
fn sketch_drafts_are_guarded_previewed_verified_and_saved() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/sketch-repair.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let (_, sketches) = get(&mut studio, "/api/sketches");
    let (_, state) = get(&mut studio, "/api/state");
    let (_, instances) = get(&mut studio, "/api/instances");
    let gltf = studio.handle("GET", "/api/model.gltf", b"").body;
    let expected = sketches["sketches"][0]["definition"].clone();
    let mut draft = expected.clone();
    draft["constraints"].as_array_mut().unwrap().pop();
    let mut edit = json!({"version": state["version"], "instance": instances["instances"][0]["id"], "feature": sketches["sketches"][0]["feature"], "expected": expected, "definition": draft, "apply": false});
    let post = |studio: &mut serve::Studio, body: &serde_json::Value| {
        studio.handle("POST", "/api/sketch", body.to_string().as_bytes())
    };
    let preview = post(&mut studio, &edit);
    assert_eq!(preview.status, 200);
    let preview: serde_json::Value = serde_json::from_slice(&preview.body).unwrap();
    assert_eq!(preview["can_apply"], true);
    assert_eq!(preview["annotations"]["accepted"], false);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, gltf);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let mut bad = edit.clone();
    bad["apply"] = json!(true);
    bad["definition"]["profile"] = json!(["missing-curve"]);
    assert_eq!(post(&mut studio, &bad).status, 422);
    bad = edit.clone();
    bad["apply"] = json!(true);
    bad["expected"]["id"] = json!("stale-definition");
    assert_eq!(post(&mut studio, &bad).status, 422);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    edit["apply"] = json!(true);
    assert_eq!(post(&mut studio, &edit).status, 200);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    assert_eq!(get(&mut studio, "/api/state").1["dirty"], true);
    assert_eq!(history(&mut studio, "undo")["dirty"], false);
    assert_eq!(
        get(&mut studio, "/api/sketches").1["sketches"][0]["definition"],
        expected
    );
    history(&mut studio, "redo");
    assert_eq!(
        get(&mut studio, "/api/sketches").1["sketches"][0]["definition"],
        draft
    );

    assert_eq!(
        post(&mut studio, &edit).status,
        422,
        "old version cannot overwrite an applied edit"
    );
    assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
    assert_ne!(fs::read_to_string(&model).unwrap(), original);
    assert_eq!(
        get(&mut studio, "/api/sketches").1["sketches"][0]["definition"],
        edit["definition"]
    );
}

#[test]
fn sketch_apply_rejects_failed_required_checks_and_keeps_the_source() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/sketch-repair.request.json"
    ))
    .unwrap();
    let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    document.family.requirements.push(Requirement {
        id: "volume-budget".into(),
        version: 1,
        kind: RequirementKind::Dimensional,
        priority: RequirementPriority::Required,
        statement: "Keep volume under 11,000 mm³".into(),
        rule: VerificationRule::VolumeRange {
            output: "body".into(),
            minimum: Volume {
                value: 1.0,
                unit: LengthUnit::Millimeter,
            },
            maximum: Volume {
                value: 11_000.0,
                unit: LengthUnit::Millimeter,
            },
        },
        provenance: "test".into(),
        traces: vec![],
    });
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let (_, sketches) = get(&mut studio, "/api/sketches");
    let (_, state) = get(&mut studio, "/api/state");
    let (_, instances) = get(&mut studio, "/api/instances");
    let expected = sketches["sketches"][0]["definition"].clone();
    let mut definition = expected.clone();
    definition["constraints"][4]["horizontal_distance"]["value"] =
        json!({"literal":{"value":80,"dimension":"length","unit":"millimeter"}});
    let mut edit = json!({"version": state["version"], "instance": instances["instances"][0]["id"], "feature": sketches["sketches"][0]["feature"], "expected":expected, "definition":definition, "apply":false});
    let preview = studio.handle("POST", "/api/sketch", edit.to_string().as_bytes());
    assert_eq!(preview.status, 200);
    let preview: serde_json::Value = serde_json::from_slice(&preview.body).unwrap();
    assert_eq!(preview["can_apply"], false);
    assert!(preview["error"].as_str().unwrap().contains("volume-budget"));
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/sketch", edit.to_string().as_bytes())
            .status,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
}

#[test]
fn visual_constraint_conflicts_preview_without_replacing_the_accepted_model() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/sketch-repair.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let (_, sketches) = get(&mut studio, "/api/sketches");
    assert!(
        sketches["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "width")
    );
    assert_eq!(
        studio.handle("GET", "/constraint-controls.mjs", b"").status,
        200
    );
    let (_, state) = get(&mut studio, "/api/state");
    let (_, instances) = get(&mut studio, "/api/instances");
    let gltf = studio.handle("GET", "/api/model.gltf", b"").body;
    let expected = sketches["sketches"][0]["definition"].clone();
    let mut definition = expected.clone();
    // A 45-degree bottom edge conflicts with this rectangle's horizontal top
    // and vertical sides. Visual fields store degrees as native scalar radians.
    definition["constraints"][0]["line_angle"]["value"] =
        json!({"literal":{"value":std::f64::consts::FRAC_PI_4,"dimension":"scalar","unit":null}});
    let mut edit = json!({"version":state["version"],"instance":instances["instances"][0]["id"],"feature":sketches["sketches"][0]["feature"],"expected":expected,"definition":definition,"apply":false});
    let preview = studio.handle("POST", "/api/sketch", edit.to_string().as_bytes());
    assert_eq!(preview.status, 200);
    let preview: serde_json::Value = serde_json::from_slice(&preview.body).unwrap();
    assert_eq!(preview["can_apply"], false);
    let sketch = preview["annotations"]["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch")
        .unwrap();
    assert_eq!(sketch["solver"]["solved"], false);
    assert!(
        sketch["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["status"] == "failed")
    );
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/sketch", edit.to_string().as_bytes())
            .status,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, gltf);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
}

#[test]
fn constrained_drag_solves_a_temporary_pin_and_releases_it_without_accepting() {
    let directory = Directory::new();
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/sketch-repair.request.json"
    ))
    .unwrap();
    let sketch =
        &mut request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"];
    sketch["acceptance"] = json!("solved");
    sketch["constraints"].as_array_mut().unwrap().remove(4); // Width is free.
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let (_, sketches) = get(&mut studio, "/api/sketches");
    let (_, state) = get(&mut studio, "/api/state");
    let (_, instances) = get(&mut studio, "/api/instances");
    let expected = sketches["sketches"][0]["definition"].clone();
    let mut drag = json!({"version":state["version"],"instance":instances["instances"][0]["id"],"feature":sketches["sketches"][0]["feature"],"expected":expected,"definition":expected,"point":"B","target":[55,0]});
    let response = studio.handle("POST", "/api/sketch-drag", drag.to_string().as_bytes());
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let solved: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(solved["can_apply"], true);
    assert_eq!(solved["annotations"]["accepted"], false);
    let points = solved["definition"]["points"].as_array().unwrap();
    let b = points.iter().find(|p| p["id"] == "B").unwrap();
    let c = points.iter().find(|p| p["id"] == "C").unwrap();
    assert_eq!(b["fixed"], false, "temporary pin must be released");
    assert!((b["x"]["literal"]["value"].as_f64().unwrap() - 55.0).abs() < 1e-7);
    assert!(
        (c["x"]["literal"]["value"].as_f64().unwrap() - 55.0).abs() < 1e-7,
        "connected geometry must follow the drag"
    );
    assert_eq!(
        solved["definition"]["constraints"],
        drag["definition"]["constraints"]
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    drag["target"] = json!([55, 3]); // Horizontal constraint forbids this y.
    assert_eq!(
        studio
            .handle("POST", "/api/sketch-drag", drag.to_string().as_bytes())
            .status,
        422
    );
    drag["point"] = json!("A");
    assert_eq!(
        studio
            .handle("POST", "/api/sketch-drag", drag.to_string().as_bytes())
            .status,
        422
    );
    drag["point"] = json!("B");
    drag["version"] = json!(0);
    assert_eq!(
        studio
            .handle("POST", "/api/sketch-drag", drag.to_string().as_bytes())
            .status,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
}

#[test]
fn solid_controls_preview_add_union_cut_and_guard_all_variants_and_outputs() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/sketch-block.request.json"
    ))
    .unwrap();
    let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    document.instances.push(InstanceNode::Clone {
        id: "thin".into(),
        source: "block".into(),
        overrides: HashMap::from([(
            "depth".into(),
            ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
        )]),
        placement: Default::default(),
        frame: None,
        provenance: "test".into(),
    });
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let (_, features) = get(&mut studio, "/api/features");
    let (_, state) = get(&mut studio, "/api/state");
    let build = |mode: &str, depth: f64, reverse: bool| {
        let extrusion = json!({"id":"draft-extrusion","operation":{"extrude":{"input":"profile","direction":{"components":{"x":{"literal":{"value":0,"dimension":"length","unit":"millimeter"}},"y":{"literal":{"value":0,"dimension":"length","unit":"millimeter"}},"z":{"multiply":[{"literal":{"value":depth,"dimension":"length","unit":"millimeter"}},{"literal":{"value":if reverse{-1}else{1},"dimension":"scalar","unit":null}}]}}}}}});
        let changes = if mode == "new" {
            vec![extrusion]
        } else {
            vec![
                extrusion,
                json!({"id":"draft","operation":if mode=="cut"{json!({"cut":{"object":"body","tool":"draft-extrusion"}})}else{json!({"fuse":{"left":"body","right":"draft-extrusion"}})}}),
            ]
        };
        json!({"version":features["version"],"instance":"block","expected":features["features"],"action":"add","features":changes,"output":if mode=="new"{"draft-extrusion"}else{"draft"},"apply":false})
    };
    let post = |studio: &mut serve::Studio, edit: &serde_json::Value| {
        studio.handle("POST", "/api/features", edit.to_string().as_bytes())
    };
    for (mode, depth, expected_volume) in [
        ("new", 15.0, 15000.0),
        ("fuse", 15.0, 15000.0),
        ("cut", 3.0, 7000.0),
    ] {
        let response = post(&mut studio, &build(mode, depth, false));
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(result["can_apply"], true, "{result}");
        assert!(
            (result["geometry"]["block"]["volume_mm3"].as_f64().unwrap() - expected_volume).abs()
                < 1e-6
        );
        assert_eq!(result["annotations"]["accepted"], false);
        assert_eq!(result["version"], state["version"]);
        assert_eq!(result["instance"], "block");
        assert_eq!(result["gltf"]["asset"]["version"], "2.0");
        assert!(!result["gltf"]["meshes"].as_array().unwrap().is_empty());
    }
    for (mode, depth, reverse) in [
        ("cut", 7.0, false),
        ("cut", 20.0, false),
        ("cut", 3.0, true),
        ("fuse", 5.0, false),
    ] {
        let mut edit = build(mode, depth, reverse);
        let response = post(&mut studio, &edit);
        assert_eq!(response.status, 200);
        let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(result["can_apply"], false, "{result}");
        assert!(
            result["gltf"].is_null(),
            "Rejected solids cannot be displayed as verified previews."
        );
        edit["apply"] = json!(true);
        assert_eq!(post(&mut studio, &edit).status, 422);
    }
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let mut edit = build("cut", 3.0, false);
    edit["apply"] = json!(true);
    assert_eq!(post(&mut studio, &edit).status, 200);
    assert_eq!(get(&mut studio, "/api/features").1["output"], "draft");
    let current = get(&mut studio, "/api/state").1;
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"depth":2}}"#)
            .status,
        422,
        "later parameters cannot erase the applied solid"
    );
    assert_eq!(get(&mut studio, "/api/state").1, current);
    assert_eq!(get(&mut studio, "/api/state").1["dirty"], true);
    assert_eq!(
        post(&mut studio, &edit).status,
        422,
        "stale version/graph must not overwrite accepted state"
    );
    assert_eq!(studio.handle("POST", "/api/revert", b"").status, 200);
    assert_eq!(
        get(&mut studio, "/api/features").1["output"],
        "body",
        "revert restores an explicit CLI output"
    );
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
}

#[test]
fn existing_extrusion_edits_preserve_requirements_and_save_the_verified_graph() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/sketch-block.request.json"
    ))
    .unwrap();
    let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    document.family.requirements.push(Requirement {
        id: "budget".into(),
        version: 1,
        kind: RequirementKind::Dimensional,
        priority: RequirementPriority::Required,
        statement: "Volume below 11,000 mm3".into(),
        rule: VerificationRule::VolumeRange {
            output: "body".into(),
            minimum: Volume {
                value: 1.0,
                unit: LengthUnit::Millimeter,
            },
            maximum: Volume {
                value: 11000.0,
                unit: LengthUnit::Millimeter,
            },
        },
        provenance: "test".into(),
        traces: vec![],
    });
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let (_, data) = get(&mut studio, "/api/features");
    let mut feature = data["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "body")
        .unwrap()
        .clone();
    feature["operation"]["extrude"]["direction"]["components"]["z"] =
        json!({"literal":{"value":12,"dimension":"length","unit":"millimeter"}});
    let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"replace","features":[feature],"output":"body","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    assert_eq!(response.status, 200);
    let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(result["can_apply"], false);
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        422
    );
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    edit["features"][0]["operation"]["extrude"]["direction"]["components"]["z"]["literal"]["value"] =
        json!(8);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        200
    );
    assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    assert!(
        document
            .family
            .requirements
            .iter()
            .all(|r| saved.family.requirements.contains(r))
    );
    assert!(
        saved
            .family
            .requirements
            .iter()
            .any(|r| r.id.contains("solid-material"))
    );
    assert_eq!(saved.family.version, document.family.version + 1);
}

#[test]
fn a_standalone_sketch_becomes_a_saved_solid_with_persistent_guarantees() {
    let directory = Directory::new();
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/sketch-block.request.json"
    ))
    .unwrap();
    request["model"]["family"]["features"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    request["model"]["family"]["requirements"] = json!([]);
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let (_, data) = get(&mut studio, "/api/features");
    let feature = FeatureDefinition {
        id: "made-part".into(),
        operation: FeatureOperation::Extrude {
            input: "profile".into(),
            direction: VectorExpr::Components {
                x: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
                y: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
                z: ScalarExpr::Parameter("depth".into()),
            },
            extent: ExtrudeExtent::Distance,
        },
    };
    let edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"add","features":[feature],"output":"made-part","apply":true});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(result["geometry"]["block"]["solids"], 1);
    assert_eq!(get(&mut studio, "/api/state").1["undo_count"], 1);
    assert_eq!(history(&mut studio, "undo")["dirty"], false);
    assert_eq!(get(&mut studio, "/api/features").1["output"], "profile");
    assert_eq!(
        get(&mut studio, "/api/features").1["features"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // Saving the restored sketch must not persist the undone solid guarantees.
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let restored = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    assert!(restored.family.requirements.is_empty());
    assert_eq!(history(&mut studio, "redo")["dirty"], true);
    assert_eq!(get(&mut studio, "/api/features").1["output"], "made-part");

    assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
    assert_eq!(studio.handle("POST", "/api/revert", b"").status, 200);
    assert_eq!(get(&mut studio, "/api/features").1["output"], "made-part");
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    assert_eq!(saved.family.features.len(), 2);
    assert_eq!(saved.family.requirements.len(), 2);
    assert!(
        saved
            .family
            .requirements
            .iter()
            .all(|r| r.priority == RequirementPriority::Required)
    );
}

#[test]
fn revolutions_preview_boolean_material_and_preserve_accepted_history_and_drivers() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/revolved-ring.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
    let post = |studio: &mut serve::Studio, edit: &serde_json::Value| {
        studio.handle("POST", "/api/features", edit.to_string().as_bytes())
    };
    for (mode, expected) in [
        ("new", 70.0 * std::f64::consts::PI),
        ("cut", 210.0 * std::f64::consts::PI),
        ("fuse", 0.0),
    ] {
        let mut tool = data["features"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == "body")
            .unwrap()
            .clone();
        tool["id"] = json!("tool");
        if mode == "fuse" {
            tool["operation"]["revolve"]["origin"]["literal"]["x"]["value"] = json!(1);
        } else {
            tool["operation"]["revolve"]["angle_radians"] = json!({"literal":{"value":-std::f64::consts::FRAC_PI_2,"dimension":"scalar","unit":null}});
            tool["operation"]["revolve"]["extent"] = json!("symmetric");
        }
        let features = match mode {
            "cut" => {
                json!([tool,{"id":"turned-result","operation":{"cut":{"object":"body","tool":"tool"}}}])
            }
            "fuse" => {
                json!([tool,{"id":"turned-result","operation":{"fuse":{"left":"body","right":"tool"}}}])
            }
            _ => json!([tool]),
        };
        let edit = json!({"version":data["version"],"instance":"ring","expected":data["features"],"action":"add","features":features,"output":if mode=="new"{"tool"}else{"turned-result"},"apply":false});
        let response = post(&mut studio, &edit);
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(result["can_apply"], true, "{}", result["error"]);
        let volume = result["geometry"]["ring"]["volume_mm3"].as_f64().unwrap();
        if mode == "fuse" {
            assert!(volume > 280.0 * std::f64::consts::PI && volume < 560.0 * std::f64::consts::PI);
        } else {
            assert!((volume - expected).abs() < 1e-6);
        }
        assert!(!result["gltf"]["meshes"].as_array().unwrap().is_empty());
    }
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
    let mut feature = data["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "body")
        .unwrap()
        .clone();
    feature["operation"]["revolve"]["extent"] = json!("symmetric");
    let mut edit = json!({"version":data["version"],"instance":"ring","expected":data["features"],"action":"replace","features":[feature],"output":"body","apply":true});
    edit["features"][0]["operation"]["revolve"]["axis"]["literal"]["z"]["value"] = json!(0);
    assert_eq!(post(&mut studio, &edit).status, 422);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    edit["features"][0]["operation"]["revolve"]["axis"]["literal"]["z"]["value"] = json!(1);
    assert_eq!(post(&mut studio, &edit).status, 200);
    assert_eq!(get(&mut studio, "/api/state").1["undo_count"], 1);
    history(&mut studio, "undo");
    assert_eq!(
        get(&mut studio, "/api/features").1["features"],
        data["features"]
    );
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    let operation = serde_json::to_value(&saved.family.features[0].operation).unwrap();
    assert_eq!(
        operation["revolve"]["angle_radians"],
        json!({"parameter":"angle"})
    );
    assert_eq!(operation["revolve"]["extent"], "symmetric");
    assert!(
        document
            .family
            .requirements
            .iter()
            .all(|r| saved.family.requirements.contains(r))
    );
}

#[test]
fn revolution_controls_reject_angles_that_fail_another_instance() {
    let directory = Directory::new();
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/revolved-ring.request.json"
    ))
    .unwrap();
    request["model"]["family"]["parameters"][3]["default"]["scalar"]["value"] =
        json!(std::f64::consts::FRAC_PI_2);
    request["model"]["instances"].as_array_mut().unwrap().push(json!({"clone":{"id":"half","source":"ring","overrides":{"angle":{"scalar":{"value":std::f64::consts::PI,"dimension":"scalar","unit":null}}},"provenance":"test"}}));
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let mut tool = data["features"][0].clone();
    tool["id"] = json!("tool");
    tool["operation"]["revolve"]["angle_radians"] = json!({"multiply":[{"parameter":"angle"},{"literal":{"value":3,"dimension":"scalar","unit":null}}]});
    let mut edit = json!({"version":data["version"],"instance":"ring","expected":data["features"],"action":"add","features":[tool],"output":"tool","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    assert_eq!(response.status, 200);
    let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(result["can_apply"], false, "{result}");
    assert!(result["gltf"].is_null());
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
}

#[test]
fn sweep_controls_preview_holes_and_boolean_tools_then_apply_with_history_and_save() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/hollow-sweep.request.json"
    ))
    .unwrap();
    let length = 10.0 + 5.0 * std::f64::consts::PI;
    for (mode, area_factor) in [("new", 3.0), ("cut", 1.0), ("fuse", 4.0)] {
        let directory = Directory::new();
        let mut model_json = request["model"].clone();
        if mode == "cut" {
            model_json["family"]["features"][0]["operation"]["sweep"]["profile"] = json!("outer");
        }
        let document = ModelDocument::from_json(&model_json.to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let original = fs::read_to_string(&model).unwrap();
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let data = get(&mut studio, "/api/features").1;
        let state = get(&mut studio, "/api/state").1;
        let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
        let tool = json!({"id":"tool","operation":{"sweep":{"profile":if mode=="fuse"{"inner"}else{"profile"},"path":"path","orientation":"corrected_frenet"}}});
        let features = match mode {
            "cut" => {
                json!([tool,{"id":"result","operation":{"cut":{"object":"body","tool":"tool"}}}])
            }
            "fuse" => {
                json!([tool,{"id":"result","operation":{"fuse":{"left":"body","right":"tool"}}}])
            }
            _ => json!([tool]),
        };
        let mut edit = json!({"version":data["version"],"instance":"pipe","expected":data["features"],"action":"add","features":features,"output":if mode=="new"{"tool"}else{"result"},"apply":false});
        let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(result["can_apply"], true, "{}", result["error"]);
        assert!(
            (result["geometry"]["pipe"]["volume_mm3"].as_f64().unwrap()
                - area_factor * std::f64::consts::PI * length)
                .abs()
                < 1e-5
        );
        assert_eq!(result["geometry"]["pipe"]["solids"], 1);
        assert!(!result["gltf"]["meshes"].as_array().unwrap().is_empty());
        assert_eq!(get(&mut studio, "/api/state").1, state);
        assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
        edit["apply"] = json!(true);
        let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        history(&mut studio, "undo");
        assert_eq!(
            get(&mut studio, "/api/features").1["features"],
            data["features"]
        );
        history(&mut studio, "redo");
        assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
        assert_ne!(fs::read_to_string(&model).unwrap(), original);
        let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
        assert!(saved.family.features.iter().any(|f| f.id == "tool"));
        assert!(
            document
                .family
                .requirements
                .iter()
                .all(|r| saved.family.requirements.contains(r))
        );
    }
}

#[test]
fn sweep_edits_preserve_binormal_drivers_and_reject_another_instances_zero_direction() {
    let directory = Directory::new();
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/curved-pipe.request.json"
    ))
    .unwrap();
    request["model"]["family"]["parameters"].as_array_mut().unwrap().push(json!({"id":"normal","parameter_type":{"scalar":"scalar"},"default":{"scalar":{"value":1,"dimension":"scalar","unit":null}}}));
    request["model"]["instances"].as_array_mut().unwrap().push(json!({"clone":{"id":"zero","source":"pipe","overrides":{"normal":{"scalar":{"value":0,"dimension":"scalar","unit":null}}},"provenance":"test"}}));
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let mut feature = data["features"][0].clone();
    feature["operation"]["sweep"]["orientation"] = json!({"binormal":{"direction":{"components":{"x":{"literal":{"value":0,"dimension":"scalar","unit":null}},"y":{"parameter":"normal"},"z":{"literal":{"value":0,"dimension":"scalar","unit":null}}}}}});
    let mut edit = json!({"version":data["version"],"instance":"pipe","expected":data["features"],"action":"replace","features":[feature],"output":"body","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    assert_eq!(response.status, 200);
    let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(result["can_apply"], false, "{}", result["error"]);
    assert!(result["gltf"].is_null());
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    // A nonzero expression works in both instances and survives save without flattening.
    edit["features"][0]["operation"]["sweep"]["orientation"]["binormal"]["direction"]["components"]
        ["y"] = json!({"add":[{"parameter":"normal"},{"literal":{"value":1,"dimension":"scalar","unit":null}}]});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    let expected: FeatureDefinition = serde_json::from_value(edit["features"][0].clone()).unwrap();
    assert_eq!(saved.family.features[0], expected);
}

#[test]
fn profile_loft_controls_preview_hollow_boolean_tools_and_save_ordered_tracks() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/hollow-loft.request.json"
    ))
    .unwrap();
    for (mode, factor) in [("new", 560.0), ("cut", 560.0 / 3.0), ("fuse", 2240.0 / 3.0)] {
        let directory = Directory::new();
        let mut model_json = request["model"].clone();
        if mode == "cut" {
            model_json["family"]["features"][0]["operation"]["profile_loft"]["holes"] = json!([]);
        }
        let document = ModelDocument::from_json(&model_json.to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        assert_eq!(studio.handle("GET", "/solid-loft.mjs", b"").status, 200);
        let data = get(&mut studio, "/api/features").1;
        let state = get(&mut studio, "/api/state").1;
        let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
        let loft = if mode == "fuse" {
            json!({"profiles":["lower-bore","upper-bore"],"holes":[],"ruled":true})
        } else {
            json!({"profiles":["lower","upper"],"holes":[["lower-bore","upper-bore"]],"ruled":true})
        };
        let tool = json!({"id":"tool","operation":{"profile_loft":loft}});
        let features = match mode {
            "cut" => {
                json!([tool,{"id":"result","operation":{"cut":{"object":"body","tool":"tool"}}}])
            }
            "fuse" => {
                json!([tool,{"id":"result","operation":{"fuse":{"left":"body","right":"tool"}}}])
            }
            _ => json!([tool]),
        };
        let output = if mode == "new" { "tool" } else { "result" };
        let mut edit = json!({"version":data["version"],"instance":"duct","expected":data["features"],"action":"add","features":features,"output":output,"apply":false});
        let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(result["can_apply"], true, "{}", result["error"]);
        assert!(
            (result["geometry"]["duct"]["volume_mm3"].as_f64().unwrap()
                - factor * std::f64::consts::PI)
                .abs()
                < 1e-5
        );
        assert_eq!(result["geometry"]["duct"]["solids"], 1);
        assert!(!result["gltf"]["meshes"].as_array().unwrap().is_empty());
        assert_eq!(get(&mut studio, "/api/state").1, state);
        assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
        edit["apply"] = json!(true);
        assert_eq!(
            studio
                .handle("POST", "/api/features", edit.to_string().as_bytes())
                .status,
            200
        );
        history(&mut studio, "undo");
        assert_eq!(
            get(&mut studio, "/api/features").1["features"],
            data["features"]
        );
        history(&mut studio, "redo");
        let current = get(&mut studio, "/api/features").1;
        let mut replacement = current["features"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == "tool")
            .unwrap()
            .clone();
        replacement["operation"]["profile_loft"]["ruled"] = json!(false);
        let edit = json!({"version":current["version"],"instance":"duct","expected":current["features"],"action":"replace","features":[replacement],"output":output,"apply":true});
        assert_eq!(
            studio
                .handle("POST", "/api/features", edit.to_string().as_bytes())
                .status,
            200
        );
        assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
        let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
        let expected: FeatureDefinition =
            serde_json::from_value(edit["features"][0].clone()).unwrap();
        assert_eq!(
            saved.family.features.iter().find(|f| f.id == "tool"),
            Some(&expected)
        );
        assert!(
            document
                .family
                .requirements
                .iter()
                .all(|r| saved.family.requirements.contains(r))
        );
        assert_eq!(saved.family.parameters, document.family.parameters);
    }
}

#[test]
fn profile_loft_controls_reject_hole_tracks_that_fail_another_instance() {
    let directory = Directory::new();
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/hollow-loft.request.json"
    ))
    .unwrap();
    request["model"]["family"]["features"][0]["operation"]["profile_loft"]["holes"] = json!([]);
    request["model"]["instances"].as_array_mut().unwrap().push(json!({"clone":{"id":"wide-bore","source":"duct","overrides":{"upper_bore_radius":{"scalar":{"value":5,"dimension":"length","unit":"millimeter"}}},"provenance":"test"}}));
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let mut feature = data["features"][0].clone();
    feature["operation"]["profile_loft"]["holes"] = json!([["lower-bore", "upper-bore"]]);
    let mut edit = json!({"version":data["version"],"instance":"duct","expected":data["features"],"action":"replace","features":[feature],"output":"body","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    assert_eq!(response.status, 200);
    let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(result["can_apply"], false, "{}", result["error"]);
    assert!(result["gltf"].is_null());
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
}

fn extrusion_limit_document() -> ModelDocument {
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/extrusion-limits.request.json"
    ))
    .unwrap();
    request["model"]["family"]["parameters"][2]["default"]["scalar"]["value"] = json!(10);
    let mut lifted = request["model"]["family"]["features"][0].clone();
    lifted["id"] = json!("lifted");
    lifted["operation"]["sketch_face"]["sketch"]["id"] = json!("lifted-profile");
    lifted["operation"]["sketch_face"]["sketch"]["origin"]["literal"]["z"]["value"] = json!(2);
    request["model"]["family"]["features"]
        .as_array_mut()
        .unwrap()
        .push(lifted);
    request["model"]["instances"].as_array_mut().unwrap().push(json!({"clone":{"id":"thin","source":"block","overrides":{"depth":{"scalar":{"value":5,"dimension":"length","unit":"millimeter"}}},"provenance":"test"}}));
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}

#[test]
fn geometric_extrusion_controls_follow_limits_and_save_unflattened_boolean_tools() {
    for (mode, face, profile, volume, thin_volume) in [
        ("new", false, "profile", 10000.0, 5000.0),
        ("new", true, "profile", 12000.0, 7000.0),
        ("cut", false, "lifted", 2000.0, 2000.0),
        ("fuse", true, "profile", 12000.0, 7000.0),
    ] {
        let directory = Directory::new();
        let document = extrusion_limit_document();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        assert_eq!(
            studio.handle("GET", "/solid-extrusion.mjs", b"").status,
            200
        );
        let data = get(&mut studio, "/api/features").1;
        let state = get(&mut studio, "/api/state").1;
        let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
        let extent = if face {
            json!({"up_to_face":{"target":"limit","face":{"at_extreme":{"axis":"z","extremum":"maximum","tolerance":{"literal":{"value":1e-6,"dimension":"length","unit":"millimeter"}}}}}})
        } else {
            json!({"up_to_next":{"target":"limit"}})
        };
        let tool = json!({"id":"tool","operation":{"extrude":{"input":profile,"direction":{"components":{"x":{"literal":{"value":0,"dimension":"length","unit":"millimeter"}},"y":{"literal":{"value":0,"dimension":"length","unit":"millimeter"}},"z":{"literal":{"value":1,"dimension":"length","unit":"millimeter"}}}},"extent":extent}}});
        let features = match mode {
            "cut" => {
                json!([tool,{"id":"result","operation":{"cut":{"object":"body","tool":"tool"}}}])
            }
            "fuse" => {
                json!([tool,{"id":"result","operation":{"fuse":{"left":"body","right":"tool"}}}])
            }
            _ => json!([tool]),
        };
        let output = if mode == "new" { "tool" } else { "result" };
        let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"add","features":features,"output":output,"apply":false});
        let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(result["can_apply"], true, "{}", result["error"]);
        assert!(
            (result["geometry"]["block"]["volume_mm3"].as_f64().unwrap() - volume).abs() < 1e-6
        );
        assert!(
            (result["geometry"]["thin"]["volume_mm3"].as_f64().unwrap() - thin_volume).abs() < 1e-6
        );
        assert!(!result["gltf"]["meshes"].as_array().unwrap().is_empty());
        assert_eq!(get(&mut studio, "/api/state").1, state);
        assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
        edit["apply"] = json!(true);
        assert_eq!(
            studio
                .handle("POST", "/api/features", edit.to_string().as_bytes())
                .status,
            200
        );
        history(&mut studio, "undo");
        assert_eq!(
            get(&mut studio, "/api/features").1["features"],
            data["features"]
        );
        history(&mut studio, "redo");
        assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
        let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
        let expected: FeatureDefinition =
            serde_json::from_value(edit["features"][0].clone()).unwrap();
        assert_eq!(
            saved.family.features.iter().find(|f| f.id == "tool"),
            Some(&expected)
        );
        assert_eq!(saved.family.parameters, document.family.parameters);
        // Later parameter changes move the limiting feature and regenerate the dependent solid.
        let response = studio.handle("POST", "/api/parameters", br#"{"set":{"depth":12}}"#);
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let current = get(&mut studio, "/api/features").1;
        let preview = json!({"version":current["version"],"instance":"block","expected":current["features"],"action":"add","features":[{"id":"moved-check","operation":{"extrude":{"input":"profile","direction":{"components":{"x":{"literal":{"value":0,"dimension":"length","unit":"millimeter"}},"y":{"literal":{"value":0,"dimension":"length","unit":"millimeter"}},"z":{"literal":{"value":1,"dimension":"length","unit":"millimeter"}}}},"extent":extent}}}],"output":"moved-check","apply":false});
        let response = studio.handle("POST", "/api/features", preview.to_string().as_bytes());
        assert_eq!(response.status, 200);
        let moved: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(moved["can_apply"], true, "{}", moved["error"]);
        assert!(
            (moved["geometry"]["block"]["volume_mm3"].as_f64().unwrap()
                - if face { 14000.0 } else { 12000.0 })
            .abs()
                < 1e-6
        );
    }
}

#[test]
fn geometric_extrusion_controls_reject_limits_behind_another_instance() {
    let directory = Directory::new();
    let mut document = extrusion_limit_document();
    let InstanceNode::Clone { overrides, .. } = &mut document.instances[1] else {
        panic!()
    };
    overrides.insert(
        "depth".into(),
        ParameterValue::Scalar(Quantity::length(2.0, LengthUnit::Millimeter)),
    );
    let FeatureOperation::SketchFace { sketch } =
        &mut document.family.features.last_mut().unwrap().operation
    else {
        panic!()
    };
    sketch.origin = point(0.0, 0.0, 5.0);
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"add","features":[{"id":"tool","operation":{"extrude":{"input":"lifted","direction":{"components":{"x":{"literal":{"value":0,"dimension":"length","unit":"millimeter"}},"y":{"literal":{"value":0,"dimension":"length","unit":"millimeter"}},"z":{"literal":{"value":1,"dimension":"length","unit":"millimeter"}}}},"extent":{"up_to_next":{"target":"limit"}}}}}],"output":"tool","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    assert_eq!(response.status, 200);
    let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(result["can_apply"], false, "{}", result["error"]);
    assert!(result["gltf"].is_null());
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
}

#[test]
fn picked_limiting_faces_author_live_extrusion_selectors_without_mesh_indices() {
    let directory = Directory::new();
    let document = extrusion_limit_document();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        studio
            .handle("GET", "/sketch-studio-limit-pick.mjs", b"")
            .status,
        200
    );
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let response = studio.handle(
        "POST",
        "/api/sketch-source-view",
        json!({"version":data["version"],"instance":"block","output":"limit"})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(response.status, 200);
    let source: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    let solid = source["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "solid" && s["feature"] == "limit" && s["instance"] == "block")
        .unwrap();
    let face = solid["mesh"]
        .as_array()
        .unwrap()
        .iter()
        .find(|triangle| {
            triangle["points"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| (p[2].as_f64().unwrap() - 12.0).abs() < 1e-8)
        })
        .unwrap()["face"]
        .as_u64()
        .unwrap() as usize;
    let (status, picked) = pick_face(&mut studio, "block", "limit", face);
    assert_eq!(status, 200, "{picked}");
    assert_eq!(picked["binding"], "normal");
    assert_eq!(get(&mut studio, "/api/state").1, state);
    let mut feature = data["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "selected")
        .unwrap()
        .clone();
    feature["id"] = json!("picked-limit");
    feature["operation"]["extrude"]["extent"]["up_to_face"]["face"] =
        picked["face_support"]["face"].clone();
    let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"add","features":[feature],"output":"picked-limit","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    assert_eq!(response.status, 200);
    let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(preview["can_apply"], true, "{}", preview["error"]);
    assert!((preview["geometry"]["block"]["volume_mm3"].as_f64().unwrap() - 12000.0).abs() < 1e-6);
    assert!((preview["geometry"]["thin"]["volume_mm3"].as_f64().unwrap() - 7000.0).abs() < 1e-6);
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        200
    );
    assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    let saved = saved
        .family
        .features
        .iter()
        .find(|f| f.id == "picked-limit")
        .unwrap();
    let expected: FeatureDefinition = serde_json::from_value(edit["features"][0].clone()).unwrap();
    assert_eq!(*saved, expected);
    assert!(!serde_json::to_string(saved).unwrap().contains("face_index"));
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"depth":14}}"#)
            .status,
        200
    );
    let annotations = get(&mut studio, "/api/annotations?id=block").1;
    let solid = annotations["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "solid" && s["feature"] == "picked-limit")
        .unwrap();
    let maximum = solid["mesh"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t["points"].as_array().unwrap())
        .map(|p| p[2].as_f64().unwrap())
        .fold(f64::NEG_INFINITY, f64::max);
    assert!((maximum - 16.0).abs() < 1e-8);
}

fn edge_finish_document() -> ModelDocument {
    let mut document = sketch_block_document();
    document.family.parameters.push(ParameterDefinition {
        id: "finish-size".into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: ParameterValue::Scalar(Quantity::length(1.0, LengthUnit::Millimeter)),
        minimum: None,
        maximum: None,
    });
    document.instances.push(InstanceNode::Clone {
        id: "thin".into(),
        source: "block".into(),
        overrides: HashMap::from([(
            "depth".into(),
            ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
        )]),
        placement: Default::default(),
        frame: None,
        provenance: "test".into(),
    });
    document
}

fn picked_top_edge(studio: &mut serve::Studio) -> serde_json::Value {
    let version = get(studio, "/api/state").1["version"].clone();
    let response = studio.handle(
        "POST",
        "/api/sketch-source-view",
        json!({"version":version,"instance":"block","output":"body"})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(response.status, 200);
    let source: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    let solid = source["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "solid" && s["instance"] == "block" && s["feature"] == "body")
        .unwrap();
    let edge = solid["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            solid["lines"][e["line"].as_u64().unwrap() as usize]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| {
                    p[1].as_f64().unwrap().abs() < 1e-8
                        && (p[2].as_f64().unwrap() - 10.0).abs() < 1e-8
                })
        })
        .unwrap()["edge"]
        .clone();
    let response=studio.handle("POST","/api/sketch-edge-pick",json!({"version":version,"instance":"block","output":"body","edge":edge,"id":"solid-finish-edge"}).to_string().as_bytes());
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let pick: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(pick["binding"], "extreme");
    pick["projection"]["edge"].clone()
}

#[test]
fn solid_edge_finishes_preview_picked_selectors_and_save_parameter_drivers_with_history() {
    for kind in ["fillet", "chamfer"] {
        let directory = Directory::new();
        let document = edge_finish_document();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        assert_eq!(studio.handle("GET", "/solid-finishes.mjs", b"").status, 200);
        let data = get(&mut studio, "/api/features").1;
        assert_eq!(data["dependencies"]["body"], json!(["profile"]));
        let state = get(&mut studio, "/api/state").1;
        let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
        let selector = picked_top_edge(&mut studio);
        let operation = if kind == "fillet" {
            json!({"fillet":{"input":"body","edges":[selector],"radius":{"parameter":"finish-size"}}})
        } else {
            json!({"chamfer":{"input":"body","edges":[selector],"distance":{"parameter":"finish-size"}}})
        };
        let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"add","features":[{"id":"finished","operation":operation}],"output":"finished","apply":false});
        let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
        assert_eq!(response.status, 200);
        let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(preview["can_apply"], true, "{}", preview["error"]);
        let removed = 40.0
            * if kind == "fillet" {
                1.0 - std::f64::consts::PI / 4.0
            } else {
                0.5
            };
        assert!(
            (preview["geometry"]["block"]["volume_mm3"].as_f64().unwrap() - (10000.0 - removed))
                .abs()
                < 1e-5
        );
        assert!(
            (preview["geometry"]["thin"]["volume_mm3"].as_f64().unwrap() - (5000.0 - removed))
                .abs()
                < 1e-5
        );
        assert!(!preview["gltf"]["meshes"].as_array().unwrap().is_empty());
        assert_eq!(get(&mut studio, "/api/state").1, state);
        assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
        edit["apply"] = json!(true);
        assert_eq!(
            studio
                .handle("POST", "/api/features", edit.to_string().as_bytes())
                .status,
            200
        );
        history(&mut studio, "undo");
        assert_eq!(
            get(&mut studio, "/api/features").1["features"],
            data["features"]
        );
        history(&mut studio, "redo");
        let current = get(&mut studio, "/api/features").1;
        let mut replacement = current["features"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == "finished")
            .unwrap()
            .clone();
        replacement["operation"][kind][if kind == "fillet" {
            "radius"
        } else {
            "distance"
        }] = json!({"add":[{"parameter":"finish-size"},{"literal":{"value":0.5,"dimension":"length","unit":"millimeter"}}]});
        let edit = json!({"version":current["version"],"instance":"block","expected":current["features"],"action":"replace","features":[replacement],"output":"finished","apply":true});
        assert_eq!(
            studio
                .handle("POST", "/api/features", edit.to_string().as_bytes())
                .status,
            200
        );
        assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
        let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
        let expected: FeatureDefinition =
            serde_json::from_value(edit["features"][0].clone()).unwrap();
        assert_eq!(
            saved.family.features.iter().find(|f| f.id == "finished"),
            Some(&expected)
        );
        assert!(
            !serde_json::to_string(&expected)
                .unwrap()
                .contains("edge_index")
        );
        assert!(
            document
                .family
                .requirements
                .iter()
                .all(|r| saved.family.requirements.contains(r))
        );
    }
}

#[test]
fn solid_edge_finishes_reject_sizes_that_fail_another_instance() {
    for kind in ["fillet", "chamfer"] {
        let directory = Directory::new();
        let document = edge_finish_document();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let data = get(&mut studio, "/api/features").1;
        let state = get(&mut studio, "/api/state").1;
        let selector = picked_top_edge(&mut studio);
        let operation = if kind == "fillet" {
            json!({"fillet":{"input":"body","edges":[selector],"radius":{"literal":{"value":6,"dimension":"length","unit":"millimeter"}}}})
        } else {
            json!({"chamfer":{"input":"body","edges":[selector],"distance":{"literal":{"value":6,"dimension":"length","unit":"millimeter"}}}})
        };
        let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"add","features":[{"id":"finished","operation":operation}],"output":"finished","apply":false});
        let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
        assert_eq!(response.status, 200);
        let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(result["can_apply"], false, "{}", result["error"]);
        assert!(result["gltf"].is_null());
        edit["apply"] = json!(true);
        assert_eq!(
            studio
                .handle("POST", "/api/features", edit.to_string().as_bytes())
                .status,
            422
        );
        assert_eq!(get(&mut studio, "/api/state").1, state);
    }
}

fn history(studio: &mut serve::Studio, direction: &str) -> serde_json::Value {
    let version = get(studio, "/api/state").1["version"].clone();
    let response = studio.handle(
        "POST",
        &format!("/api/{direction}"),
        json!({"version":version}).to_string().as_bytes(),
    );
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    serde_json::from_slice(&response.body).unwrap()
}

#[test]
fn history_tracks_saved_content_branches_and_rejected_edits() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let original = fs::read_to_string(&model).unwrap();
    let initial = get(&mut studio, "/api/state").1;
    assert_eq!(initial["undo_count"], 0);
    assert_eq!(
        studio
            .handle("POST", "/api/undo", br#"{"version":1}"#)
            .status,
        422
    );
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"thickness":7}}"#)
            .status,
        200
    );
    let edited = get(&mut studio, "/api/state").1;
    assert_eq!(edited["undo_count"], 1);
    // Rejected and no-op changes do not add history.
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"thickness":100}}"#)
            .status,
        422
    );
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"thickness":7}}"#)
            .status,
        200
    );
    assert_eq!(get(&mut studio, "/api/state").1, edited);
    assert_eq!(
        studio
            .handle("POST", "/api/undo", br#"{"version":1}"#)
            .status,
        422
    );
    assert_eq!(studio.handle("POST", "/api/undo", b"{}").status, 422);
    assert_eq!(get(&mut studio, "/api/state").1, edited);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let saved = fs::read_to_string(&model).unwrap();
    let undone = history(&mut studio, "undo");
    assert_eq!(thickness(&undone), 5.0);
    assert_eq!(undone["dirty"], true);
    assert_eq!(undone["redo_count"], 1);
    assert_eq!(fs::read_to_string(&model).unwrap(), saved);
    let redone = history(&mut studio, "redo");
    assert_eq!(thickness(&redone), 7.0);
    assert_eq!(redone["dirty"], false);
    assert!(redone["version"].as_u64() > undone["version"].as_u64());
    history(&mut studio, "undo");
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"thickness":100}}"#)
            .status,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1["redo_count"], 1);
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"thickness":6}}"#)
            .status,
        200
    );
    assert_eq!(get(&mut studio, "/api/state").1["redo_count"], 0);
    // Revert and successful external reload reset history; failed reload keeps it.
    fs::write(&model, "invalid").unwrap();
    assert_eq!(studio.handle("POST", "/api/revert", b"{}").status, 500);
    assert_eq!(get(&mut studio, "/api/state").1["undo_count"], 1);
    fs::write(&model, original).unwrap();
    assert_eq!(studio.handle("POST", "/api/revert", b"{}").status, 200);
    let reset = get(&mut studio, "/api/state").1;
    assert_eq!(reset["undo_count"], 0);
    assert_eq!(reset["redo_count"], 0);
    assert_eq!(reset["dirty"], false);
}

#[test]
fn history_restores_instance_membership_and_placement_atomically() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let original = get(&mut studio, "/api/instances").1;
    assert_eq!(studio.handle("POST", "/api/instances", br#"{"add":{"source":"source","id":"extra","placement":{"translation_mm":[0,80,0],"rotation":null}}}"#).status, 200);
    let added = get(&mut studio, "/api/instances").1;
    history(&mut studio, "undo");
    assert_eq!(
        get(&mut studio, "/api/instances").1["instances"],
        original["instances"]
    );
    history(&mut studio, "redo");
    assert_eq!(
        get(&mut studio, "/api/instances").1["instances"],
        added["instances"]
    );
    let moved = get(&mut studio, "/api/instance?id=extra").1;
    assert_eq!(
        moved["placement"]["translation_mm"],
        json!([0.0, 80.0, 0.0])
    );
    assert_eq!(
        studio
            .handle("POST", "/api/instances", br#"{"delete":"extra"}"#)
            .status,
        200
    );
    history(&mut studio, "undo");
    let restored = get(&mut studio, "/api/instance?id=extra").1;
    assert_eq!(restored["placement"], moved["placement"]);
    assert_eq!(restored["parameters"], moved["parameters"]);
    // A pattern placement refusal cannot append a snapshot or erase redo.
    let before = get(&mut studio, "/api/state").1;
    assert_eq!(studio.handle("POST", "/api/parameters", br#"{"instance":"member[1]","placement":{"translation_mm":[999,0,0],"rotation":null}}"#).status, 422);
    assert_eq!(get(&mut studio, "/api/state").1, before);
    history(&mut studio, "redo");
    assert_eq!(
        get(&mut studio, "/api/instances").1["instances"],
        original["instances"]
    );
}

fn starter_sketch() -> SketchDefinition {
    let mm = |v| ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter));
    serde_json::from_value(json!({
        "id":"new-profile","acceptance":"solved","origin":point(0.0,0.0,0.0),
        "x_axis":VectorExpr::Literal(VectorQuantity::scalars(1.0,0.0,0.0)),
        "y_axis":VectorExpr::Literal(VectorQuantity::scalars(0.0,1.0,0.0)),
        "points":[{"id":"C","x":mm(0.0),"y":mm(0.0),"fixed":true},{"id":"R","x":mm(5.0),"y":mm(0.0),"fixed":true}],
        "lines":[],"circles":[{"id":"circle","center":"C","rim":"R"}],"profile":["circle"],"constraints":[]
    })).unwrap()
}
fn new_sketch_request(
    studio: &mut serve::Studio,
    definition: &SketchDefinition,
) -> serde_json::Value {
    let data = get(studio, "/api/features").1;
    json!({"version":data["version"],"instance":"block","expected":data["features"],"feature":"new-profile","definition":definition,"apply":false})
}
fn create_sketch(studio: &mut serve::Studio, edit: &serde_json::Value) -> (u16, serde_json::Value) {
    let response = studio.handle("POST", "/api/sketch-create", edit.to_string().as_bytes());
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap(),
    )
}
fn sketch_block_document() -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/sketch-block.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}

#[test]
fn new_sketch_preview_apply_save_and_history_keep_the_existing_solid() {
    let directory = Directory::new();
    let document = sketch_block_document();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let before = get(&mut studio, "/api/state").1;
    let gltf = studio.handle("GET", "/api/model.gltf", b"").body;
    let mut edit = new_sketch_request(&mut studio, &starter_sketch());
    let (status, preview) = create_sketch(&mut studio, &edit);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true);
    assert!(
        preview["annotations"]["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["feature"] == "new-profile" && s["kind"] == "sketch")
    );
    assert_eq!(get(&mut studio, "/api/state").1, before);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, gltf);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    edit["apply"] = json!(true);
    let (status, applied) = create_sketch(&mut studio, &edit);
    assert_eq!(status, 200, "{applied}");
    assert_eq!(get(&mut studio, "/api/features").1["output"], "body");
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, gltf);
    assert_eq!(applied["state"]["undo_count"], 1);
    assert_eq!(create_sketch(&mut studio, &edit).0, 422);
    history(&mut studio, "undo");
    assert_eq!(
        get(&mut studio, "/api/features").1["features"],
        edit["expected"]
    );
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    assert_eq!(
        saved.family.features.len(),
        document.family.features.len() + 1
    );
    assert_eq!(saved.family.requirements, document.family.requirements);
    assert_eq!(saved.family.version, document.family.version + 1);
    assert_eq!(saved.schema_version, document.schema_version);
}

#[test]
fn created_face_support_tracks_source_height_and_rejects_bad_or_stale_supports() {
    let directory = Directory::new();
    let mut document = sketch_block_document();
    document.instances.push(InstanceNode::Clone {
        id: "thin".into(),
        source: "block".into(),
        overrides: HashMap::from([(
            "depth".into(),
            ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
        )]),
        placement: Default::default(),
        frame: None,
        provenance: "test".into(),
    });
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let mut definition = starter_sketch();
    definition.face_support = Some(SketchFaceSupport {
        input: "body".into(),
        face: FaceSelector::NormalAligned {
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999999)),
        },
        offset: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
    });
    let mut edit = new_sketch_request(&mut studio, &definition);
    let (status, preview) = create_sketch(&mut studio, &edit);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true);
    let support = |data: &serde_json::Value| {
        data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "new-profile" && s["kind"] == "sketch")
            .unwrap()["face_support"]
            .clone()
    };
    assert_eq!(
        support(&preview["annotations"])["origin_mm"],
        json!([20.0, 12.5, 12.0])
    );
    let before = get(&mut studio, "/api/state").1;
    for apply in [false, true] {
        let mut bad = edit.clone();
        bad["definition"]["face_support"]["input"] = json!("missing");
        bad["apply"] = json!(apply);
        assert_ne!(create_sketch(&mut studio, &bad).1["can_apply"], true);
        assert_eq!(get(&mut studio, "/api/state").1, before);
    }
    let mut bad = edit.clone();
    bad["expected"] = json!([]);
    bad["apply"] = json!(true);
    assert_eq!(create_sketch(&mut studio, &bad).0, 422);
    edit["apply"] = json!(true);
    assert_eq!(create_sketch(&mut studio, &edit).0, 200);
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"depth":12}}"#)
            .status,
        200
    );
    let annotations = get(&mut studio, "/api/annotations?id=block").1;
    assert!((support(&annotations)["origin_mm"][2].as_f64().unwrap() - 14.0).abs() < 1e-9);
    assert!(
        (support(&get(&mut studio, "/api/annotations?id=thin").1)["origin_mm"][2]
            .as_f64()
            .unwrap()
            - 7.0)
            .abs()
            < 1e-9
    );
    history(&mut studio, "undo");
    assert_eq!(
        support(&get(&mut studio, "/api/annotations?id=block").1)["origin_mm"],
        json!([20.0, 12.5, 12.0])
    );
}

#[test]
fn new_sketch_rejects_empty_duplicate_ambiguous_and_curved_supports() {
    let directory = Directory::new();
    let mut document = sketch_block_document();
    document.family.features.push(FeatureDefinition {
        id: "curved-source".into(),
        operation: FeatureOperation::Cylinder {
            origin: point(0.0, 0.0, 0.0),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            radius: ScalarExpr::Literal(Quantity::length(8.0, LengthUnit::Millimeter)),
            height: ScalarExpr::Literal(Quantity::length(20.0, LengthUnit::Millimeter)),
        },
    });
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let before = get(&mut studio, "/api/state").1;
    let mut empty = starter_sketch();
    empty.profile.clear();
    empty.points.clear();
    empty.circles.clear();
    let mut request = new_sketch_request(&mut studio, &empty);
    request["apply"] = json!(true);
    assert_eq!(create_sketch(&mut studio, &request).0, 422);
    let mut request = new_sketch_request(&mut studio, &starter_sketch());
    request["feature"] = json!("body");
    request["apply"] = json!(true);
    assert_eq!(create_sketch(&mut studio, &request).0, 422);
    let mut definition = starter_sketch();
    let top = FaceSelector::NormalAligned {
        direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999999)),
    };
    let bottom = FaceSelector::NormalAligned {
        direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
        minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999999)),
    };
    for (input, face) in [
        ("body", FaceSelector::Union(vec![top, bottom])),
        (
            "curved-source",
            FaceSelector::LargestArea {
                planar_only: false,
                allow_ties: false,
                relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-9)),
            },
        ),
    ] {
        definition.face_support = Some(SketchFaceSupport {
            input: input.into(),
            face,
            offset: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
        });
        let mut request = new_sketch_request(&mut studio, &definition);
        let (status, preview) = create_sketch(&mut studio, &request);
        assert_eq!(status, 200, "{preview}");
        assert_eq!(preview["can_apply"], false);
        request["apply"] = json!(true);
        assert_eq!(create_sketch(&mut studio, &request).0, 422);
        assert_eq!(get(&mut studio, "/api/state").1, before);
    }
}

#[test]
fn primitive_only_families_can_create_their_first_authored_sketch() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    assert!(
        get(&mut studio, "/api/sketches").1["sketches"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut request = new_sketch_request(&mut studio, &starter_sketch());
    request["instance"] = json!("source");
    request["apply"] = json!(true);
    let (status, result) = create_sketch(&mut studio, &request);
    assert_eq!(status, 200, "{result}");
    assert_eq!(get(&mut studio, "/api/features").1["output"], "bracket");
    let sketches = get(&mut studio, "/api/sketches").1;
    assert_eq!(sketches["sketches"].as_array().unwrap().len(), 1);
    assert_eq!(sketches["sketches"][0]["feature"], "new-profile");
}

fn pick_face(
    studio: &mut serve::Studio,
    instance: &str,
    output: &str,
    face: usize,
) -> (u16, serde_json::Value) {
    let version = get(studio, "/api/state").1["version"].clone();
    let response = studio.handle(
        "POST",
        "/api/sketch-face-pick",
        json!({"version":version,"instance":instance,"output":output,"face":face})
            .to_string()
            .as_bytes(),
    );
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap(),
    )
}
fn horizontal_mesh_face(studio: &mut serve::Studio, instance: &str, z: f64) -> usize {
    let data = get(studio, &format!("/api/annotations?id={instance}")).1;
    let scene = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "solid")
        .unwrap();
    scene["mesh"]
        .as_array()
        .unwrap()
        .iter()
        .find(|triangle| {
            triangle["points"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| (p[2].as_f64().unwrap() - z).abs() < 1e-7)
        })
        .unwrap()["face"]
        .as_u64()
        .unwrap() as usize
}
fn definition_on_pick(pick: &serde_json::Value) -> SketchDefinition {
    let mut definition = serde_json::to_value(starter_sketch()).unwrap();
    for key in ["face_support", "x_axis", "y_axis"] {
        definition[key] = pick[key].clone();
    }
    serde_json::from_value(definition).unwrap()
}

#[test]
fn direct_face_picks_are_read_only_versioned_and_follow_parameter_changes() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&sketch_block_document()));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let face = horizontal_mesh_face(&mut studio, "block", 10.0);
    let before = get(&mut studio, "/api/state").1;
    let (status, pick) = pick_face(&mut studio, "block", "body", face);
    assert_eq!(status, 200, "{pick}");
    assert_eq!(pick["binding"], "normal");
    assert_eq!(pick["origin_mm"], json!([20.0, 12.5, 10.0]));
    assert_eq!(get(&mut studio, "/api/state").1, before);
    assert!(pick["face_support"].get("face_index").is_none());
    assert_eq!(pick_face(&mut studio, "block", "body", usize::MAX).0, 422);
    let definition = definition_on_pick(&pick);
    let mut edit = new_sketch_request(&mut studio, &definition);
    edit["apply"] = json!(true);
    assert_eq!(create_sketch(&mut studio, &edit).0, 200);
    let response = studio.handle(
        "POST",
        "/api/sketch-face-pick",
        json!({"version":before["version"],"instance":"block","output":"body","face":face})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(response.status, 422);
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"depth":18}}"#)
            .status,
        200
    );
    let data = get(&mut studio, "/api/annotations?id=block").1;
    let sketch = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "new-profile")
        .unwrap();
    assert!((sketch["face_support"]["origin_mm"][2].as_f64().unwrap() - 18.0).abs() < 1e-8);
}

#[test]
fn picking_a_pocket_floor_uses_history_to_distinguish_parallel_faces() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/face-pocket.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let instance = get(&mut studio, "/api/instances").1["instances"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let face = horizontal_mesh_face(&mut studio, &instance, 15.0);
    let (status, pick) = pick_face(&mut studio, &instance, "body", face);
    assert_eq!(status, 200, "{pick}");
    assert_eq!(pick["binding"], "history", "{pick}");
    assert!((pick["origin_mm"][2].as_f64().unwrap() - 15.0).abs() < 1e-8);
    let mut edit = new_sketch_request(&mut studio, &definition_on_pick(&pick));
    edit["instance"] = json!(instance);
    edit["apply"] = json!(true);
    let (status, result) = create_sketch(&mut studio, &edit);
    assert_eq!(status, 200, "{result}");
    // Source height and cutter placement move together; the selected floor follows.
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"height":30}}"#)
            .status,
        200
    );
    let data = get(&mut studio, &format!("/api/annotations?id={instance}")).1;
    let sketch = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "new-profile")
        .unwrap();
    assert!(
        (sketch["face_support"]["origin_mm"][2].as_f64().unwrap() - 25.0).abs() < 1e-8,
        "{sketch}"
    );
}

#[test]
fn picked_rotated_planes_produce_valid_frames_and_curved_faces_are_refused() {
    let directory = Directory::new();
    let mut document = sketch_block_document();
    document.family.features.push(FeatureDefinition {
        id: "tilted".into(),
        operation: FeatureOperation::Rotate {
            input: "body".into(),
            origin: point(0.0, 0.0, 0.0),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::FRAC_PI_4)),
        },
    });
    document.family.features.push(FeatureDefinition {
        id: "curved".into(),
        operation: FeatureOperation::Cylinder {
            origin: point(0.0, 0.0, 0.0),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            radius: ScalarExpr::Literal(Quantity::length(8.0, LengthUnit::Millimeter)),
            height: ScalarExpr::Literal(Quantity::length(20.0, LengthUnit::Millimeter)),
        },
    });
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("tilted".into())).unwrap();
    let before = get(&mut studio, "/api/state").1;
    let mut tilted = None;
    for face in 0..6 {
        let (status, pick) = pick_face(&mut studio, "block", "tilted", face);
        assert_eq!(status, 200, "{pick}");
        if pick["normal"][0].as_f64().unwrap().abs() > 0.5
            && pick["normal"][2].as_f64().unwrap().abs() > 0.5
        {
            tilted = Some(pick);
            break;
        }
    }
    let mut curved_refused = false;
    for face in 0..3 {
        let (status, pick) = pick_face(&mut studio, "block", "curved", face);
        if status == 422 {
            assert!(pick["error"].as_str().unwrap().contains("planar"));
            curved_refused = true;
        }
    }
    assert!(curved_refused);
    assert_eq!(get(&mut studio, "/api/state").1, before);
    let mut request = new_sketch_request(&mut studio, &definition_on_pick(&tilted.unwrap()));
    request["apply"] = json!(true);
    let (status, result) = create_sketch(&mut studio, &request);
    assert_eq!(status, 200, "{result}");
}

fn link_document() -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/face-pocket.request.json"
    ))
    .unwrap();
    let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let mut editable = starter_sketch();
    editable.id = "editable".into();
    editable.face_support = Some(SketchFaceSupport {
        input: "block".into(),
        face: FaceSelector::NormalAligned {
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
            minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999999)),
        },
        offset: ScalarExpr::Parameter("support_offset".into()),
    });
    document.family.features.push(FeatureDefinition {
        id: "editable".into(),
        operation: FeatureOperation::SketchFace {
            sketch: Box::new(editable),
        },
    });
    document
}
fn source_view(studio: &mut serve::Studio, output: &str) -> serde_json::Value {
    let version = get(studio, "/api/state").1["version"].clone();
    let response = studio.handle(
        "POST",
        "/api/sketch-source-view",
        json!({"version":version,"instance":"part","output":output})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    serde_json::from_slice(&response.body).unwrap()
}
fn edit_definition(
    studio: &mut serve::Studio,
    feature: &str,
    definition: serde_json::Value,
    apply: bool,
) -> (u16, serde_json::Value) {
    let data = get(studio, "/api/sketches").1;
    let expected = data["sketches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == feature)
        .unwrap()["definition"]
        .clone();
    let response=studio.handle("POST","/api/sketch",json!({"version":data["version"],"instance":"part","feature":feature,"expected":expected,"definition":definition,"apply":apply}).to_string().as_bytes());
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap(),
    )
}
fn editable_definition(studio: &mut serve::Studio) -> serde_json::Value {
    get(studio, "/api/sketches").1["sketches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == "editable")
        .unwrap()["definition"]
        .clone()
}

#[test]
fn existing_sketch_reattaches_without_changing_local_geometry_or_offset_expression() {
    let directory = Directory::new();
    let document = link_document();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let before = get(&mut studio, "/api/state").1;
    let source = source_view(&mut studio, "block");
    assert_eq!(source["accepted"], true);
    assert_eq!(source["source_view"], true);
    let face = source["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "solid")
        .unwrap()["mesh"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| {
            t["points"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| (p[2].as_f64().unwrap() - 20.0).abs() < 1e-8)
        })
        .unwrap()["face"]
        .as_u64()
        .unwrap() as usize;
    let (status, pick) = pick_face(&mut studio, "part", "block", face);
    assert_eq!(status, 200, "{pick}");
    assert_eq!(get(&mut studio, "/api/state").1, before);
    let original = editable_definition(&mut studio);
    let mut draft = original.clone();
    draft["face_support"] = pick["face_support"].clone();
    draft["face_support"]["offset"] = original["face_support"]["offset"].clone();
    draft["datum_plane"] = serde_json::Value::Null;
    draft["x_axis"] = pick["x_axis"].clone();
    draft["y_axis"] = pick["y_axis"].clone();
    let (status, preview) = edit_definition(&mut studio, "editable", draft.clone(), false);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true);
    assert_eq!(get(&mut studio, "/api/state").1, before);
    assert_eq!(
        edit_definition(&mut studio, "editable", draft.clone(), true).0,
        200
    );
    assert_eq!(
        editable_definition(&mut studio)["points"],
        original["points"]
    );
    assert_eq!(
        editable_definition(&mut studio)["face_support"]["offset"],
        original["face_support"]["offset"]
    );
    history(&mut studio, "undo");
    assert_eq!(editable_definition(&mut studio), original);
    history(&mut studio, "redo");
    assert_eq!(editable_definition(&mut studio), draft);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    assert_eq!(saved.family.requirements, document.family.requirements);
    assert_eq!(saved.schema_version, document.schema_version);
}

#[test]
fn picked_edge_projection_tracks_source_dimensions_and_rejects_cycles() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&link_document()));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let before = get(&mut studio, "/api/state").1;
    let source = source_view(&mut studio, "tool");
    let scene = source["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "solid")
        .unwrap();
    let edge = scene["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            scene["lines"][e["line"].as_u64().unwrap() as usize]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| {
                    (p[1].as_f64().unwrap() - 14.0).abs() < 1e-8
                        && (p[2].as_f64().unwrap() - 15.0).abs() < 1e-8
                })
        })
        .unwrap()["edge"]
        .clone();
    let response=studio.handle("POST","/api/sketch-edge-pick",json!({"version":before["version"],"instance":"part","output":"tool","edge":edge,"id":"reference"}).to_string().as_bytes());
    let pick: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 200, "{pick}");
    assert_eq!(pick["projection"]["kind"], "line");
    assert_ne!(pick["binding"], "anchored_center");
    assert_eq!(get(&mut studio, "/api/state").1, before);
    let mut definition = editable_definition(&mut studio);
    definition["projections"] = json!([pick["projection"]]);
    let (status, result) = edit_definition(&mut studio, "editable", definition, true);
    assert_eq!(status, 200, "{result}");
    assert_eq!(
        studio
            .handle("POST", "/api/parameters", br#"{"set":{"pocket_width":24}}"#)
            .status,
        200
    );
    let data = get(&mut studio, "/api/annotations?id=part").1;
    let sketch = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "editable")
        .unwrap();
    let a = sketch["points"]["reference:start"][0].as_f64().unwrap();
    let b = sketch["points"]["reference:end"][0].as_f64().unwrap();
    assert!(((a - b).abs() - 24.0).abs() < 1e-8, "{sketch}");
    history(&mut studio, "undo");
    let data = get(&mut studio, "/api/sketches").1;
    let mut cycle = data["sketches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == "profile")
        .unwrap()["definition"]
        .clone();
    cycle["projections"] = json!([pick["projection"]]);
    cycle["projections"][0]["input"] = json!("body");
    let before = get(&mut studio, "/api/state").1;
    assert_eq!(edit_definition(&mut studio, "profile", cycle, true).0, 422);
    assert_eq!(get(&mut studio, "/api/state").1, before);
}

#[test]
fn picked_curved_edges_import_exact_rational_splines() {
    let directory = Directory::new();
    let mut document = link_document();
    document.family.features.push(FeatureDefinition {
        id: "cylinder".into(),
        operation: FeatureOperation::Cylinder {
            origin: point(0.0, 0.0, 0.0),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            radius: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
            height: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
        },
    });
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let source = source_view(&mut studio, "cylinder");
    let scene = source["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "solid")
        .unwrap();
    let edge = scene["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            scene["lines"][e["line"].as_u64().unwrap() as usize]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| (p[2].as_f64().unwrap() - 10.0).abs() < 1e-8)
        })
        .unwrap()["edge"]
        .clone();
    let version = get(&mut studio, "/api/state").1["version"].clone();
    let response = studio.handle(
        "POST",
        "/api/sketch-edge-pick",
        json!({"version":version,"instance":"part","output":"cylinder","edge":edge,"id":"ring"})
            .to_string()
            .as_bytes(),
    );
    let pick: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 200, "{pick}");
    assert_eq!(pick["projection"]["kind"], "spline");
    let mut definition = editable_definition(&mut studio);
    definition["projections"] = json!([pick["projection"]]);
    let (status, result) = edit_definition(&mut studio, "editable", definition, true);
    assert_eq!(status, 200, "{result}");
    let data = get(&mut studio, "/api/annotations?id=part").1;
    let sketch = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "editable")
        .unwrap();
    let ring = sketch["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "ring")
        .unwrap();
    assert!(
        ring["bspline"]["evaluated_weights"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| (w.as_f64().unwrap() - 1.0).abs() > 1e-3),
        "{ring}"
    );
}

#[test]
fn profile_offsets_preview_apply_and_reject_broken_trim_without_mutation() {
    let directory = Directory::new();
    let mut value = serde_json::to_value(sketch_block_document()).unwrap();
    value["instances"][0]["base"]["id"] = json!("part");
    let document = ModelDocument::from_json(&value.to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let baseline = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let state = get(&mut studio, "/api/state").1;
    let original_mesh = studio.handle("GET", "/api/model.gltf", b"").body;
    let mut draft = baseline.clone();
    draft["profile_operations"] = json!([{"offset":{"distance":{"literal":{"value":2,"dimension":"length","unit":"millimeter"}},"join":"intersection"}}]);
    let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true, "{preview}");
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(
        studio.handle("GET", "/api/model.gltf", b"").body,
        original_mesh
    );
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), true);
    assert_eq!(status, 200, "{result}");
    assert_ne!(
        studio.handle("GET", "/api/model.gltf", b"").body,
        original_mesh
    );
    let accepted = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    assert_eq!(accepted["points"], baseline["points"]);
    assert_eq!(accepted["constraints"], baseline["constraints"]);
    let entity = baseline["profile"][0].as_str().unwrap();
    draft["profile_operations"] = json!([{"trim":{"entity":entity,"first":{"literal":{"value":0.2,"dimension":"scalar","unit":null}},"last":{"literal":{"value":0.8,"dimension":"scalar","unit":null}}}}]);
    let accepted_state = get(&mut studio, "/api/state").1;
    let accepted_mesh = studio.handle("GET", "/api/model.gltf", b"").body;
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 422, "{result}");
    assert_eq!(get(&mut studio, "/api/state").1, accepted_state);
    assert_eq!(
        studio.handle("GET", "/api/model.gltf", b"").body,
        accepted_mesh
    );
    assert_eq!(history(&mut studio, "undo")["dirty"], false);
    assert_eq!(
        studio.handle("GET", "/api/model.gltf", b"").body,
        original_mesh
    );
    assert_eq!(history(&mut studio, "redo")["dirty"], true);
}

#[test]
fn exact_crossing_trim_edits_regenerate_keyhole_solid_and_keep_constraints() {
    let directory = Directory::new();
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/keyhole-tag.request.json"
    ))
    .unwrap();
    request["model"]["instances"][0]["base"]["id"] = json!("part");
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let sketch = get(&mut studio, "/api/sketches").1["sketches"][0].clone();
    let mut draft = sketch["definition"].clone();
    // The existing named point at the leg's start is also an exact trim bound.
    let entity = draft["profile_operations"][0]["trim_between"]["entity"]
        .as_str()
        .unwrap()
        .to_string();
    let start = draft["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == entity)
        .unwrap()["start"]
        .clone();
    draft["profile_operations"][0]["trim_between"]["from"] = json!({"point":{"point":start}});
    let (status, preview) = edit_definition(
        &mut studio,
        sketch["feature"].as_str().unwrap(),
        draft.clone(),
        false,
    );
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true, "{preview}");
    let (status, result) = edit_definition(
        &mut studio,
        sketch["feature"].as_str().unwrap(),
        draft,
        true,
    );
    assert_eq!(status, 200, "{result}");
    let applied = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    assert_eq!(applied["constraints"], sketch["definition"]["constraints"]);
    assert_eq!(
        applied["profile_operations"][1]["trim_between"],
        sketch["definition"]["profile_operations"][1]["trim_between"]
    );
}

#[test]
fn profile_corners_regenerate_and_reject_oversized_treatments_without_mutation() {
    let directory = Directory::new();
    let mut value = serde_json::to_value(sketch_block_document()).unwrap();
    value["instances"][0]["base"]["id"] = json!("part");
    let document = ModelDocument::from_json(&value.to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let baseline = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let state = get(&mut studio, "/api/state").1;
    let original = studio.handle("GET", "/api/model.gltf", b"").body;
    let mm =
        |value: f64| json!({"literal":{"value":value,"dimension":"length","unit":"millimeter"}});
    let mut draft = baseline.clone();
    draft["profile_operations"] = json!([
        {"fillet":{"point":"A","radius":mm(2.0)}},
        {"chamfer":{"point":"B","incoming":mm(2.0),"outgoing":mm(3.0)}}
    ]);
    let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true, "{preview}");
    assert_eq!(get(&mut studio, "/api/state").1, state);
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), true);
    assert_eq!(status, 200, "{result}");
    let accepted = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    assert_eq!(accepted["points"], baseline["points"]);
    assert_eq!(accepted["constraints"], baseline["constraints"]);
    let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
    assert_ne!(mesh, original);
    let accepted_state = get(&mut studio, "/api/state").1;
    for invalid in [
        json!({"fillet":{"point":"A","radius":mm(1000.0)}}),
        json!({"chamfer":{"point":"B","incoming":mm(1000.0),"outgoing":mm(1000.0)}}),
    ] {
        draft["profile_operations"] = json!([invalid]);
        let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), true);
        assert_eq!(status, 422, "{result}");
        assert_eq!(get(&mut studio, "/api/state").1, accepted_state);
        assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
    }
    assert_eq!(history(&mut studio, "undo")["dirty"], false);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, original);
    assert_eq!(history(&mut studio, "redo")["dirty"], true);
}

#[test]
fn source_edits_and_seed_selected_region_edits_keep_other_regions_and_reject_lost_references() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/edited-regions.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/sketches").1;
    let baseline = data["sketches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == "profile")
        .unwrap()["definition"]
        .clone();
    let mm =
        |value: f64| json!({"literal":{"value":value,"dimension":"length","unit":"millimeter"}});
    let mut draft = baseline.clone();
    draft["curve_operations"][0]["extend_to"]["limit"] = mm(55.0);
    draft["region_edits"].as_array_mut().unwrap().push(json!({"seed":"extra-pick","operations":[{"offset":{"distance":mm(0.5),"join":"intersection"}}]}));
    let before = get(&mut studio, "/api/state").1;
    let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true, "{preview}");
    assert_eq!(get(&mut studio, "/api/state").1, before);
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), true);
    assert_eq!(status, 200, "{result}");
    let applied = get(&mut studio, "/api/sketches").1;
    let current = applied["sketches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == "profile")
        .unwrap()["definition"]
        .clone();
    assert_eq!(
        current["curve_operations"][1],
        baseline["curve_operations"][1]
    );
    assert_eq!(current["region_edits"][0], baseline["region_edits"][0]);
    let accepted = get(&mut studio, "/api/state").1;
    let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
    // Removing the split leaves explicit references to its named pieces.
    draft["curve_operations"].as_array_mut().unwrap().pop();
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 422, "{result}");
    assert_eq!(get(&mut studio, "/api/state").1, accepted);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
    // A seed on the outer circle cannot select a bounded material cell.
    let mut ring = applied["sketches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == "ring_profile")
        .unwrap()["definition"]
        .clone();
    let invalid_seed = ring["circles"][0]["rim"].clone();
    ring["region_discovery"]["seeds"] = json!([invalid_seed]);
    let (status, result) = edit_definition(&mut studio, "ring_profile", ring, true);
    assert_eq!(status, 422, "{result}");
    assert_eq!(get(&mut studio, "/api/state").1, accepted);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
}

#[test]
fn drawn_arc_and_periodic_spline_build_native_curves_and_reject_fixed_radius_conflicts() {
    let directory = Directory::new();
    let mut value = serde_json::to_value(sketch_block_document()).unwrap();
    value["instances"][0]["base"]["id"] = json!("part");
    let document = ModelDocument::from_json(&value.to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let baseline = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let original_mesh = studio.handle("GET", "/api/model.gltf", b"").body;
    let mm =
        |value: f64| json!({"literal":{"value":value,"dimension":"length","unit":"millimeter"}});
    let mut draft = baseline.clone();
    for (id, x, y) in [
        ("center", 10.0, 10.0),
        ("start", 15.0, 10.0),
        ("end", 10.0, 15.0),
        ("s0", 5.0, 5.0),
        ("s1", 20.0, 5.0),
        ("s2", 20.0, 18.0),
        ("s3", 5.0, 18.0),
    ] {
        draft["points"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"x":mm(x),"y":mm(y),"fixed":true}));
    }
    draft["arcs"] =
        json!([{"id":"arc","center":"center","start":"start","end":"end","clockwise":true}]);
    draft["splines"] = json!([{"id":"spline","points":["s0","s1","s2","s3","s0"]}]);
    draft["profile"] = json!(["spline"]);
    let before = get(&mut studio, "/api/state").1;
    let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true, "{preview}");
    assert_eq!(get(&mut studio, "/api/state").1, before);
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), true);
    assert_eq!(status, 200, "{result}");
    assert_ne!(
        studio.handle("GET", "/api/model.gltf", b"").body,
        original_mesh
    );
    let annotations = get(&mut studio, "/api/annotations?id=part").1;
    let sketch = annotations["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
        .unwrap();
    let spline = sketch["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "spline")
        .unwrap();
    let samples = spline["points"].as_array().unwrap();
    assert!(samples.len() > 5, "{spline}");
    assert_eq!(samples.first(), samples.last());
    // Native interpolation follows the curve rather than the click polygon.
    assert!(
        samples.iter().any(|p| p[1].as_f64().unwrap() < 5.0),
        "{spline}"
    );
    let accepted = get(&mut studio, "/api/state").1;
    draft["points"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "end")
        .unwrap()["y"] = mm(16.0);
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 422, "{result}");
    assert_eq!(get(&mut studio, "/api/state").1, accepted);
    assert_eq!(history(&mut studio, "undo")["dirty"], false);
    assert_eq!(
        studio.handle("GET", "/api/model.gltf", b"").body,
        original_mesh
    );
}

#[test]
fn drawn_ellipse_slot_and_polygon_profiles_extrude_to_their_analytic_volumes() {
    let directory = Directory::new();
    let mut value = serde_json::to_value(sketch_block_document()).unwrap();
    value["instances"][0]["base"]["id"] = json!("part");
    let document = ModelDocument::from_json(&value.to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let baseline = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let mm =
        |value: f64| json!({"literal":{"value":value,"dimension":"length","unit":"millimeter"}});
    for (kind, expected_volume) in [
        ("ellipse", 400.0 * std::f64::consts::PI),
        ("slot", 400.0 + 40.0 * std::f64::consts::PI),
        ("polygon", 3000.0 * (std::f64::consts::PI / 3.0).sin()),
    ] {
        let mut draft = baseline.clone();
        for (id, x, y) in [
            ("center", 0.0, 0.0),
            ("axis", 10.0, 0.0),
            ("minor", 0.0, 4.0),
        ] {
            draft["points"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":id,"x":mm(x),"y":mm(y),"fixed":true}));
        }
        match kind {
            "ellipse" => {
                draft["ellipses"] =
                    json!([{"id":"outline","center":"center","major":"axis","minor":"minor"}])
            }
            "slot" => {
                draft["shapes"] = json!([{"slot":{"id":"outline","start":"center","end":"axis","radius":mm(2.0)}}])
            }
            _ => {
                draft["shapes"] = json!([{"polygon":{"id":"outline","center":"center","vertex":"axis","sides":6}}])
            }
        }
        draft["profile"] = json!(["outline"]);
        let (status, result) = edit_definition(&mut studio, "profile", draft, true);
        assert_eq!(status, 200, "{kind}: {result}");
        let features = get(&mut studio, "/api/features").1;
        let response = studio.handle("POST", "/api/features", json!({
            "version":features["version"],"instance":"part","expected":features["features"],
            "action":"add","features":[{"id":"volume-probe","operation":{"extrude":{"input":"profile","direction":point(0.0,0.0,10.0)}}}],
            "output":"volume-probe","apply":false
        }).to_string().as_bytes());
        let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(response.status, 200, "{kind}: {preview}");
        assert_eq!(preview["can_apply"], true, "{kind}: {preview}");
        let volume = preview["geometry"]["part"]["volume_mm3"].as_f64().unwrap();
        assert!(
            (volume - expected_volume).abs() < 1e-6,
            "{kind}: {volume} != {expected_volume}"
        );
    }
}

#[test]
fn shape_parameter_edits_track_parameters_and_reject_lost_polygon_references() {
    let directory = Directory::new();
    let mut value = serde_json::to_value(sketch_block_document()).unwrap();
    value["instances"][0]["base"]["id"] = json!("part");
    let sketch = &mut value["family"]["features"][0]["operation"]["sketch_face"]["sketch"];
    sketch["shapes"] = json!([{"slot":{"id":"slot","start":"A","end":"B","radius":{"multiply":[{"parameter":"depth"},{"literal":{"value":0.2,"dimension":"scalar","unit":null}}]}}}]);
    sketch["profile"] = json!(["slot"]);
    let document = ModelDocument::from_json(&value.to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let baseline = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let mut draft = baseline.clone();
    draft["shapes"][0]["slot"]["radius"] = json!({"parameter":"depth"});
    let before = get(&mut studio, "/api/state").1;
    let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true, "{preview}");
    assert_eq!(get(&mut studio, "/api/state").1, before);
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), true);
    assert_eq!(status, 200, "{result}");
    let response = studio.handle("POST", "/api/parameters", br#"{"set":{"depth":12}}"#);
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let data = get(&mut studio, "/api/annotations?id=part").1;
    let scene = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
        .unwrap();
    assert!((scene["points"]["slot:start_right"][1].as_f64().unwrap() + 12.0).abs() < 1e-8);
    let accepted = get(&mut studio, "/api/state").1;
    draft["shapes"][0]["slot"]["radius"] =
        json!({"literal":{"value":0,"dimension":"length","unit":"millimeter"}});
    assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 422);
    assert_eq!(get(&mut studio, "/api/state").1, accepted);
    let mut polygon = baseline;
    polygon["shapes"] = json!([{"polygon":{"id":"poly","center":"A","vertex":"B","sides":6}}]);
    polygon["profile"] = json!([
        "poly:side0",
        "poly:side1",
        "poly:side2",
        "poly:side3",
        "poly:side4",
        "poly:side5"
    ]);
    let (status, result) = edit_definition(&mut studio, "profile", polygon.clone(), true);
    assert_eq!(status, 200, "{result}");
    let accepted = get(&mut studio, "/api/state").1;
    let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
    polygon["shapes"][0]["polygon"]["sides"] = json!(4);
    let (status, result) = edit_definition(&mut studio, "profile", polygon, true);
    assert_eq!(status, 422, "{result}");
    assert_eq!(get(&mut studio, "/api/state").1, accepted);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
}

#[test]
fn spline_pole_and_weight_edits_preserve_basis_and_reject_invalid_weights() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/rational-spline-cap.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let baseline = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let original_mesh = studio.handle("GET", "/api/model.gltf", b"").body;
    let scalar = |value: f64| json!({"literal":{"value":value,"dimension":"scalar","unit":null}});
    let mut draft = baseline.clone();
    let mut pole = draft["points"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "b")
        .unwrap()
        .clone();
    pole["id"] = json!("alternate-b");
    pole["x"] = json!({"literal":{"value":12,"dimension":"length","unit":"millimeter"}});
    draft["points"].as_array_mut().unwrap().push(pole);
    draft["splines"][0]["points"][1] = json!("alternate-b");
    draft["splines"][0]["basis"]["weights"][1] = scalar(1.2);
    let before = get(&mut studio, "/api/state").1;
    let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true, "{preview}");
    assert_eq!(get(&mut studio, "/api/state").1, before);
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), true);
    assert_eq!(status, 200, "{result}");
    let applied = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    assert_eq!(
        applied["splines"][0]["basis"]["knots"],
        baseline["splines"][0]["basis"]["knots"]
    );
    assert_eq!(
        applied["splines"][0]["basis"]["multiplicities"],
        baseline["splines"][0]["basis"]["multiplicities"]
    );
    assert_eq!(applied["constraints"], baseline["constraints"]);
    let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
    assert_ne!(mesh, original_mesh);
    let accepted = get(&mut studio, "/api/state").1;
    draft["splines"][0]["basis"]["weights"][1] = scalar(0.0);
    assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 422);
    assert_eq!(get(&mut studio, "/api/state").1, accepted);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
    assert_eq!(history(&mut studio, "undo")["dirty"], false);
    assert_eq!(
        studio.handle("GET", "/api/model.gltf", b"").body,
        original_mesh
    );
}

#[test]
fn spline_basis_edits_regenerate_all_modes_and_reject_inconsistent_multiplicities() {
    for (source, mode) in [
        (
            include_str!("../../../../../tools/model/rational-spline-cap.request.json"),
            "clamped",
        ),
        (
            include_str!("../../../../../tools/model/periodic-spline-pad.request.json"),
            "periodic",
        ),
        (
            include_str!("../../../../../tools/model/non-clamped-spline-cap.request.json"),
            "non_clamped",
        ),
    ] {
        let directory = Directory::new();
        let request: serde_json::Value = serde_json::from_str(source).unwrap();
        let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let baseline = get(&mut studio, "/api/sketches").1["sketches"][0].clone();
        let feature = baseline["feature"].as_str().unwrap();
        let mut draft = baseline["definition"].clone();
        let basis = &mut draft["splines"][0]["basis"];
        if mode == "clamped" {
            basis["degree"] = json!(1);
            basis["knots"] = json!([0.0, 0.5, 1.0]);
            basis["multiplicities"] = json!([2, 1, 2]);
        } else if mode == "periodic" {
            basis["degree"] = json!(2);
        } else {
            for knot in basis["knots"].as_array_mut().unwrap() {
                *knot = json!(knot.as_f64().unwrap() * 2.0);
            }
        }
        let before = get(&mut studio, "/api/state").1;
        let (status, preview) = edit_definition(&mut studio, feature, draft.clone(), false);
        assert_eq!(status, 200, "{mode}: {preview}");
        assert_eq!(preview["can_apply"], true, "{mode}: {preview}");
        assert_eq!(get(&mut studio, "/api/state").1, before);
        let (status, result) = edit_definition(&mut studio, feature, draft.clone(), true);
        assert_eq!(status, 200, "{mode}: {result}");
        let applied = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
        assert_eq!(
            applied["splines"][0]["basis"]["weights"],
            baseline["definition"]["splines"][0]["basis"]["weights"]
        );
        assert_eq!(
            applied["splines"][0]["endpoints"],
            baseline["definition"]["splines"][0]["endpoints"]
        );
        let accepted = get(&mut studio, "/api/state").1;
        let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
        draft["splines"][0]["basis"]["multiplicities"][0] = json!(0);
        assert_eq!(edit_definition(&mut studio, feature, draft, true).0, 422);
        assert_eq!(get(&mut studio, "/api/state").1, accepted);
        assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
        assert_eq!(history(&mut studio, "undo")["dirty"], false);
    }
}

#[test]
fn spline_pole_count_edits_regenerate_all_modes_and_preserve_points() {
    for (source, mode) in [
        (
            include_str!("../../../../../tools/model/rational-spline-cap.request.json"),
            "clamped",
        ),
        (
            include_str!("../../../../../tools/model/periodic-spline-pad.request.json"),
            "periodic",
        ),
        (
            include_str!("../../../../../tools/model/non-clamped-spline-cap.request.json"),
            "non_clamped",
        ),
    ] {
        let directory = Directory::new();
        let request: serde_json::Value = serde_json::from_str(source).unwrap();
        let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let baseline = get(&mut studio, "/api/sketches").1["sketches"][0].clone();
        let feature = baseline["feature"].as_str().unwrap();
        let mut draft = baseline["definition"].clone();
        let spline = &mut draft["splines"][0];
        let pole = spline["points"][1].clone();
        spline["points"].as_array_mut().unwrap().insert(1, pole);
        spline["basis"]["weights"].as_array_mut().unwrap().insert(
            1,
            json!({"literal":{"value":1.0,"dimension":"scalar","unit":null}}),
        );
        let basis = &mut spline["basis"];
        if mode == "clamped" {
            basis["knots"] = json!([0.0, 0.5, 1.0]);
            basis["multiplicities"] = json!([3, 1, 3]);
        } else if mode == "periodic" {
            basis["knots"] = json!([0.0, 0.8, 1.6, 2.4, 3.2, 4.0]);
            basis["multiplicities"] = json!([1, 1, 1, 1, 1, 1]);
        } else {
            basis["knots"] = json!([1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0]);
            basis["multiplicities"] = json!([1, 1, 1, 1, 1, 1, 1]);
        }
        let before = get(&mut studio, "/api/state").1;
        let (status, preview) = edit_definition(&mut studio, feature, draft.clone(), false);
        assert_eq!(status, 200, "{mode}: {preview}");
        assert_eq!(preview["can_apply"], true, "{mode}: {preview}");
        assert_eq!(get(&mut studio, "/api/state").1, before);
        let (status, result) = edit_definition(&mut studio, feature, draft.clone(), true);
        assert_eq!(status, 200, "{mode}: {result}");
        let applied = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
        assert_eq!(applied["splines"], draft["splines"]);
        assert_eq!(applied["points"], baseline["definition"]["points"]);
        assert_eq!(
            applied["constraints"],
            baseline["definition"]["constraints"]
        );
        assert_eq!(
            applied["splines"][0]["endpoints"],
            baseline["definition"]["splines"][0]["endpoints"]
        );
        let accepted = get(&mut studio, "/api/state").1;
        let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
        draft["splines"][0]["basis"]["multiplicities"][0] = json!(0);
        assert_eq!(edit_definition(&mut studio, feature, draft, true).0, 422);
        assert_eq!(get(&mut studio, "/api/state").1, accepted);
        assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
        let (status, removed) =
            edit_definition(&mut studio, feature, baseline["definition"].clone(), true);
        assert_eq!(status, 200, "{mode}: {removed}");
        history(&mut studio, "undo");
        assert_eq!(
            get(&mut studio, "/api/sketches").1["sketches"][0]["definition"]["splines"],
            applied["splines"]
        );
        assert_eq!(history(&mut studio, "undo")["dirty"], false);
    }
}

#[test]
fn exact_rational_refinement_preserves_native_solid_volume_across_parameters() {
    let mut volumes = Vec::new();
    for source in [
        include_str!("../../../../../tools/model/rational-spline-cap.request.json"),
        include_str!("../../../../../tools/model/rational-spline-refinement.request.json"),
        include_str!("../../../../../tools/model/linked-spline-refinement.request.json"),
    ] {
        let mut cases = Vec::new();
        for (radius, weight) in [
            (10.0, std::f64::consts::FRAC_1_SQRT_2),
            (15.0, 1.2),
            (7.0, 0.35),
        ] {
            let directory = Directory::new();
            let mut request: serde_json::Value = serde_json::from_str(source).unwrap();
            request["model"]["family"]["parameters"][0]["default"]["scalar"]["value"] =
                json!(radius);
            request["model"]["family"]["parameters"][1]["default"]["scalar"]["value"] =
                json!(weight);
            assert_eq!(
                request["model"]["schema_version"],
                json!(CURRENT_SCHEMA_VERSION)
            );
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let model = PathBuf::from(directory.model(&document));
            let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
            let before = get(&mut studio, "/api/state").1;
            let features = get(&mut studio, "/api/features").1;
            let response = studio.handle("POST", "/api/features", json!({
                "version":features["version"],"instance":"part","expected":features["features"],
                "action":"add","features":[{"id":"volume-probe","operation":{"extrude":{"input":"profile","direction":point(0.0,0.0,8.0)}}}],
                "output":"volume-probe","apply":false
            }).to_string().as_bytes());
            let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(response.status, 200, "{preview}");
            assert_eq!(preview["can_apply"], true, "{preview}");
            cases.push(preview["geometry"]["part"]["volume_mm3"].as_f64().unwrap());
            assert_eq!(get(&mut studio, "/api/state").1, before);
        }
        volumes.push(cases);
    }
    for cases in &volumes[1..] {
        for (original, refined) in volumes[0].iter().zip(cases) {
            assert!((original - refined).abs() < 1e-7, "{original} != {refined}");
        }
    }
}

#[test]
fn native_spline_refinement_route_is_read_only_versioned_and_bounded() {
    for source in [
        include_str!("../../../../../tools/model/rational-spline-cap.request.json"),
        include_str!("../../../../../tools/model/periodic-spline-pad.request.json"),
        include_str!("../../../../../tools/model/non-clamped-spline-cap.request.json"),
    ] {
        let directory = Directory::new();
        let request: serde_json::Value = serde_json::from_str(source).unwrap();
        let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let sketches = get(&mut studio, "/api/sketches").1;
        let spline = &sketches["sketches"][0]["definition"]["splines"][0];
        let basis = &spline["basis"];
        let count = spline["points"].as_array().unwrap().len();
        let degree = basis["degree"].as_i64().unwrap();
        let state = get(&mut studio, "/api/state").1;
        let mesh = studio.handle("GET", "/api/model.gltf", b"").body;
        for action in ["insert_knot", "elevate_degree"] {
            let edit = json!({"version":sketches["version"],"basis":basis,"pole_count":count,"action":action,"knot":if basis["non_clamped"]==true {2.5} else {0.5},"degree":degree+1});
            let response = studio.handle("POST", "/api/spline-refine", edit.to_string().as_bytes());
            let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(response.status, 200, "{result}");
            assert_eq!(result["version"], sketches["version"]);
            assert_eq!(
                result["degree"],
                if action == "insert_knot" {
                    degree
                } else {
                    degree + 1
                }
            );
            let rows = result["coefficients"].as_array().unwrap();
            if action == "insert_knot" {
                assert_eq!(rows.len(), count + 1);
            }
            for row in rows {
                let values = row.as_array().unwrap();
                assert_eq!(values.len(), count);
                assert!(values.iter().all(|v| v.as_f64().unwrap() >= 0.));
                assert!(
                    (values.iter().map(|v| v.as_f64().unwrap()).sum::<f64>() - 1.).abs() < 1e-12
                );
            }
            for invalid in [
                json!({"version":0}),
                if action == "insert_knot" {
                    json!({"knot":1e100})
                } else {
                    json!({"degree":26})
                },
                json!({"pole_count":257}),
            ] {
                let mut rejected = edit.clone();
                for (key, value) in invalid.as_object().unwrap() {
                    rejected[key] = value.clone();
                }
                assert_eq!(
                    studio
                        .handle(
                            "POST",
                            "/api/spline-refine",
                            rejected.to_string().as_bytes()
                        )
                        .status,
                    422
                );
            }
            assert_eq!(get(&mut studio, "/api/state").1, state);
            assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, mesh);
        }
        assert_eq!(
            fs::read_to_string(model).unwrap(),
            document.to_json_pretty().unwrap()
        );
    }
}

#[test]
fn weighted_point_drafts_follow_solver_controls_and_reject_invalid_links_without_mutation() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/linked-spline-refinement.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let baseline = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let state = get(&mut studio, "/api/state").1;
    for cycle in [false, true] {
        let mut invalid = baseline.clone();
        if cycle {
            let target = invalid["constraints"][2]["weighted_point"]["point"].clone();
            invalid["constraints"][2]["weighted_point"]["terms"][0]["point"] = target;
        } else {
            invalid["constraints"][2]["weighted_point"]["terms"][0]["weight"] =
                json!({"literal":{"value":0.0,"dimension":"scalar","unit":null}});
        }
        assert_eq!(
            edit_definition(&mut studio, "profile", invalid, true).0,
            422
        );
        assert_eq!(get(&mut studio, "/api/state").1, state);
    }
    let version = get(&mut studio, "/api/sketches").1["version"].clone();
    let target = baseline["splines"][0]["points"][1].clone();
    let response=studio.handle("POST","/api/sketch-drag",json!({"version":version,"instance":"part","feature":"profile","expected":baseline,"definition":baseline,"point":target,"target":[0.,0.]}).to_string().as_bytes());
    assert_eq!(response.status, 422);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    let mut draft = baseline.clone();
    draft["constraints"][1]["vertical_distance"]["value"] =
        json!({"literal":{"value":12.0,"dimension":"length","unit":"millimeter"}});
    let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true, "{preview}");
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 200);
    let annotations = get(&mut studio, "/api/annotations?id=part").1;
    let scene = annotations["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch")
        .unwrap();
    assert_eq!(scene["solver"]["free_degrees"], 0);
    assert!((scene["points"]["b"][1].as_f64().unwrap() - 12.).abs() < 1e-9);
    let name = target.as_str().unwrap();
    let weight = std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        (scene["points"][name][1].as_f64().unwrap() - 24. * weight / (1. + 2. * weight)).abs()
            < 1e-9
    );
    assert!(
        scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["label"] == "WEIGHTED")
    );
    assert_eq!(history(&mut studio, "undo")["dirty"], false);
}

#[test]
fn projected_refinement_drafts_are_previewed_applied_undone_and_saved_with_live_links() {
    for source in [
        include_str!("../../../../../tools/model/projected-spline-pad.request.json"),
        include_str!("../../../../../tools/model/projected-chain-pad.request.json"),
    ] {
        let directory = Directory::new();
        let request: serde_json::Value = serde_json::from_str(source).unwrap();
        let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let original = fs::read_to_string(&model).unwrap();
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let definitions = get(&mut studio, "/api/sketches").1;
        let baseline = definitions["sketches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["definition"]
            .clone();
        let state = get(&mut studio, "/api/state").1;
        let mut draft = baseline.clone();
        draft["projections"][0]["refinements"] = json!([{"elevate_degree":{"degree":4}},{"insert_knot":{"fraction":0.37,"multiplicity":2}}]);
        let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
        assert_eq!(status, 200, "{preview}");
        assert_eq!(preview["can_apply"], true, "{preview}");
        assert_eq!(get(&mut studio, "/api/state").1, state);
        let (status, result) = edit_definition(&mut studio, "profile", draft, true);
        assert_eq!(status, 200, "{result}");
        let accepted = get(&mut studio, "/api/sketches").1;
        let projection = &accepted["sketches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["definition"]["projections"][0];
        assert_eq!(projection["input"], baseline["projections"][0]["input"]);
        assert_eq!(projection["edge"], baseline["projections"][0]["edge"]);
        assert_eq!(projection["refinements"].as_array().unwrap().len(), 2);
        let accepted_state = get(&mut studio, "/api/state").1;
        let mut invalid = baseline.clone();
        invalid["projections"][0]["refinements"] =
            json!([{"insert_knot":{"fraction":0.37,"multiplicity":25}}]);
        assert_eq!(
            edit_definition(&mut studio, "profile", invalid, true).0,
            422
        );
        assert_eq!(get(&mut studio, "/api/state").1, accepted_state);
        assert_eq!(fs::read_to_string(&model).unwrap(), original);
        assert_eq!(history(&mut studio, "undo")["dirty"], false);
        assert_eq!(history(&mut studio, "redo")["dirty"], true);
        assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
        let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
        assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
        let feature = saved
            .family
            .features
            .iter()
            .find(|f| f.id == "profile")
            .unwrap();
        let FeatureOperation::SketchFace { sketch } = &feature.operation else {
            panic!("profile")
        };
        assert_eq!(sketch.projections[0].refinements.len(), 2);
        assert_eq!(sketch.projections[0].input, "source");
    }
}

#[test]
fn detach_projected_splines_freezes_exact_draft_geometry_and_preserves_history() {
    for source in [
        include_str!("../../../../../tools/model/projected-spline-pad.request.json"),
        include_str!("../../../../../tools/model/projected-junction-pad.request.json"),
        include_str!("../../../../../tools/model/projected-spline-intersection-pad.request.json"),
    ] {
        let directory = Directory::new();
        let request: serde_json::Value = serde_json::from_str(source).unwrap();
        let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let original = fs::read_to_string(&model).unwrap();
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let data = get(&mut studio, "/api/sketches").1;
        let baseline = data["sketches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["definition"]
            .clone();
        let state = get(&mut studio, "/api/state").1;
        let mut draft = baseline.clone();
        draft["projections"][0]["refinements"] = json!([{"elevate_degree":{"degree":4}},{"insert_knot":{"fraction":0.37,"multiplicity":2}}]);
        let request = json!({"version":data["version"],"instance":"part","feature":"profile","expected":baseline,"definition":draft,"projection":"reference"});
        let mut unchanged = request.clone();
        unchanged["definition"] = baseline.clone();
        let unchanged_response = studio.handle(
            "POST",
            "/api/sketch-detach-projection",
            unchanged.to_string().as_bytes(),
        );
        assert_eq!(
            unchanged_response.status,
            200,
            "{}",
            String::from_utf8_lossy(&unchanged_response.body)
        );
        let mut conflicting = request.clone();
        conflicting["expected"]["id"] = json!("changed");
        assert_eq!(
            studio
                .handle(
                    "POST",
                    "/api/sketch-detach-projection",
                    conflicting.to_string().as_bytes()
                )
                .status,
            422
        );
        if baseline["projections"].as_array().unwrap().len() > 1 {
            let mut dependent = request.clone();
            let mut crossing = baseline["projections"][0]["intersections"][0].clone();
            crossing["other"] = json!("reference");
            dependent["definition"]["projections"][1]["intersections"] = json!([crossing]);
            let rejected = studio.handle(
                "POST",
                "/api/sketch-detach-projection",
                dependent.to_string().as_bytes(),
            );
            assert_eq!(rejected.status, 422);
            assert!(String::from_utf8_lossy(&rejected.body).contains("Another live projection"));
        }
        let response = studio.handle(
            "POST",
            "/api/sketch-detach-projection",
            request.to_string().as_bytes(),
        );
        let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(response.status, 200, "{result}");
        assert_eq!(get(&mut studio, "/api/state").1, state);
        assert_eq!(fs::read_to_string(&model).unwrap(), original);
        let detached = result["definition"].clone();
        assert!(
            !detached["projections"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|p| p["id"] == "reference")
        );
        assert_eq!(detached["splines"][0]["basis"]["degree"], 4);
        assert_eq!(detached["profile"], baseline["profile"]);
        assert_eq!(detached["constraints"], baseline["constraints"]);
        let (_, preview) = edit_definition(&mut studio, "profile", draft, false);
        let scene = |value: &serde_json::Value| {
            value["annotations"]["scenes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["feature"] == "profile")
                .unwrap()
                .clone()
        };
        let before = scene(&preview);
        let after = scene(&result);
        let entity = |value: &serde_json::Value| {
            value["entities"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["id"] == "reference")
                .unwrap()["bspline"]
                .clone()
        };
        assert_eq!(entity(&before), entity(&after));
        for point in detached["points"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["id"].as_str().unwrap().starts_with("reference:"))
        {
            let id = point["id"].as_str().unwrap();
            assert_eq!(point["fixed"], true);
            assert_eq!(before["points"][id], after["points"][id]);
        }
        let mut stale = request.clone();
        stale["version"] = json!(u64::MAX);
        assert_eq!(
            studio
                .handle(
                    "POST",
                    "/api/sketch-detach-projection",
                    stale.to_string().as_bytes()
                )
                .status,
            422
        );
        assert_eq!(get(&mut studio, "/api/state").1, state);
        let (status, result) = edit_definition(&mut studio, "profile", detached.clone(), true);
        assert_eq!(status, 200, "{result}");
        history(&mut studio, "undo");
        assert_eq!(get(&mut studio, "/api/state").1["dirty"], false);
        history(&mut studio, "redo");
        assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
        let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
        assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
        let mut changed = saved.clone();
        changed
            .family
            .parameters
            .iter_mut()
            .find(|p| p.id == "weight")
            .unwrap()
            .default = ParameterValue::Scalar(Quantity::scalar(1.4));
        let mut moved_studio = serve::Studio::load(
            &PathBuf::from(directory.model(&changed)),
            Some("body".into()),
        )
        .unwrap();
        let frozen = get(&mut moved_studio, "/api/annotations?id=part").1;
        let frozen_scene = frozen["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap();
        assert_eq!(entity(&after), entity(frozen_scene));
        for id in entity(&after)["control_points"].as_array().unwrap() {
            let id = id.as_str().unwrap();
            assert_eq!(after["points"][id], frozen_scene["points"][id]);
        }
    }
}

#[test]
fn saved_spline_point_links_preview_follow_edits_reject_drag_and_survive_save_history() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/spline-point-links.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let state = get(&mut studio, "/api/state").1;
    let data = get(&mut studio, "/api/sketches").1;
    let baseline = data["sketches"][0]["definition"].clone();
    let scene = |view: serde_json::Value| {
        view["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()
            .clone()
    };
    let before = scene(get(&mut studio, "/api/annotations?id=part").1);
    let drag = json!({"version":data["version"],"instance":"part","feature":"profile","expected":baseline,"definition":baseline,"point":"sample","target":[5.,5.]});
    let response = studio.handle("POST", "/api/sketch-drag", drag.to_string().as_bytes());
    assert_eq!(response.status, 422);
    assert!(String::from_utf8_lossy(&response.body).contains("saved point link"));
    let mut draft = baseline.clone();
    draft["points"][1]["y"] =
        json!({"literal":{"value":12.,"dimension":"length","unit":"millimeter"}});
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true, "{result}");
    let after = scene(result["annotations"].clone());
    assert_ne!(before["points"]["sample"], after["points"]["sample"]);
    assert_ne!(before["points"]["crossing"], after["points"]["crossing"]);
    assert!((after["points"]["crossing"][1].as_f64().unwrap() - 5.).abs() < 1e-8);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let mut invalid = draft.clone();
    invalid["constraints"][0]["spline_point"]["fraction"] =
        json!({"literal":{"value":1.1,"dimension":"scalar","unit":null}});
    assert_eq!(
        edit_definition(&mut studio, "profile", invalid, true).0,
        422
    );
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 200, "{result}");
    history(&mut studio, "undo");
    assert_eq!(
        scene(get(&mut studio, "/api/annotations?id=part").1)["points"]["sample"],
        before["points"]["sample"]
    );
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
    let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        scene(get(&mut reopened, "/api/annotations?id=part").1)["points"]["sample"],
        after["points"]["sample"]
    );
}

#[test]
fn periodic_named_point_links_follow_parameters_preview_the_seam_and_save() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/periodic-spline-point-links.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let scene = |view: serde_json::Value| {
        view["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()
            .clone()
    };
    let before = scene(get(&mut studio, "/api/annotations?id=part").1);
    assert_eq!(before["points"]["seam-start"], before["points"]["seam-end"]);
    let response = studio.handle(
        "POST",
        "/api/parameters",
        br#"{"set":{"location":0.73,"corner_weight":0.7}}"#,
    );
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let updated = scene(get(&mut studio, "/api/annotations?id=part").1);
    assert_ne!(before["points"]["sample"], updated["points"]["sample"]);
    assert_eq!(
        updated["points"]["seam-start"],
        updated["points"]["seam-end"]
    );
    let data = get(&mut studio, "/api/sketches").1;
    let baseline = data["sketches"][0]["definition"].clone();
    let state = get(&mut studio, "/api/state").1;
    let mut draft = baseline.clone();
    draft["constraints"][0]["spline_point"]["fraction"] =
        json!({"literal":{"value":1.,"dimension":"scalar","unit":null}});
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    let preview = scene(result["annotations"].clone());
    assert_eq!(preview["points"]["sample"], preview["points"]["seam-start"]);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let mut invalid = draft.clone();
    invalid["constraints"][0]["spline_point"]["fraction"]["literal"]["value"] = json!(1.1);
    assert_eq!(
        edit_definition(&mut studio, "profile", invalid, true).0,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 200, "{result}");
    history(&mut studio, "undo");
    assert_eq!(
        scene(get(&mut studio, "/api/annotations?id=part").1)["points"]["sample"],
        updated["points"]["sample"]
    );
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
    let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
    let scene = scene(get(&mut reopened, "/api/annotations?id=part").1);
    assert_eq!(scene["points"]["sample"], scene["points"]["seam-end"]);
}

#[test]
fn selected_crossing_drafts_keep_branch_reject_ambiguity_and_loss_and_save() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/selected-crossing-pad.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let scene = |view: serde_json::Value| {
        view["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()
            .clone()
    };
    let before = scene(get(&mut studio, "/api/annotations?id=part").1);
    assert!(before["points"]["crossing"][0].as_f64().unwrap() > 0.);
    let state = get(&mut studio, "/api/state").1;
    for body in [
        br#"{"set":{"search_radius":30}}"#.as_slice(),
        br#"{"set":{"guide_y":12}}"#.as_slice(),
    ] {
        let response = studio.handle("POST", "/api/parameters", body);
        assert_eq!(
            response.status,
            422,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        assert_eq!(get(&mut studio, "/api/state").1, state);
        assert_eq!(fs::read_to_string(&model).unwrap(), original);
    }
    let response = studio.handle("POST", "/api/parameters", br#"{"set":{"guide_y":6}}"#);
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let moved = scene(get(&mut studio, "/api/annotations?id=part").1);
    assert!((moved["points"]["crossing"][0].as_f64().unwrap() - 8.).abs() < 1e-8);
    let data = get(&mut studio, "/api/sketches").1;
    let baseline = data["sketches"][0]["definition"].clone();
    let state = get(&mut studio, "/api/state").1;
    let drag = json!({"version":data["version"],"instance":"part","feature":"profile","expected":baseline,"definition":baseline,"point":"crossing","target":[-8.,6.]});
    assert_eq!(
        studio
            .handle("POST", "/api/sketch-drag", drag.to_string().as_bytes())
            .status,
        422
    );
    let mut draft = baseline.clone();
    draft["constraints"][0]["crossing_point"]["near"] = json!({"components":{"x":{"literal":{"value":-8.5,"dimension":"length","unit":"millimeter"}},"y":{"parameter":"guide_y"},"z":{"literal":{"value":0.,"dimension":"length","unit":"millimeter"}}}});
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    assert!(
        scene(result["annotations"].clone())["points"]["crossing"][0]
            .as_f64()
            .unwrap()
            < 0.
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 200, "{result}");
    history(&mut studio, "undo");
    assert!(
        scene(get(&mut studio, "/api/annotations?id=part").1)["points"]["crossing"][0]
            .as_f64()
            .unwrap()
            > 0.
    );
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
    let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert!(
        scene(get(&mut reopened, "/api/annotations?id=part").1)["points"]["crossing"][0]
            .as_f64()
            .unwrap()
            < 0.
    );
}

#[test]
fn interpolated_named_links_preview_fit_edits_reject_degeneracy_and_save_history() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/interpolated-spline-point-links.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let state = get(&mut studio, "/api/state").1;
    let data = get(&mut studio, "/api/sketches").1;
    let baseline = data["sketches"][0]["definition"].clone();
    let scene = |v: serde_json::Value| {
        v["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()
            .clone()
    };
    let before = scene(get(&mut studio, "/api/annotations?id=part").1);
    let drag = json!({"version":data["version"],"instance":"part","feature":"profile","expected":baseline,"definition":baseline,"point":"sample","target":[5.,5.]});
    assert_eq!(
        studio
            .handle("POST", "/api/sketch-drag", drag.to_string().as_bytes())
            .status,
        422
    );
    let mut draft = baseline.clone();
    draft["points"][1]["y"] =
        json!({"literal":{"value":16.,"dimension":"length","unit":"millimeter"}});
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    let after = scene(result["annotations"].clone());
    assert_ne!(before["points"]["sample"], after["points"]["sample"]);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let mut invalid = draft.clone();
    invalid["points"][1]["x"] = invalid["points"][0]["x"].clone();
    invalid["points"][1]["y"] = invalid["points"][0]["y"].clone();
    assert_eq!(
        edit_definition(&mut studio, "profile", invalid, true).0,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 200, "{result}");
    history(&mut studio, "undo");
    assert_eq!(
        scene(get(&mut studio, "/api/annotations?id=part").1)["points"]["sample"],
        before["points"]["sample"]
    );
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        scene(get(&mut reopened, "/api/annotations?id=part").1)["points"]["sample"],
        after["points"]["sample"]
    );
}

#[test]
fn crossing_review_exports_native_resolved_windows_for_parameters_and_read_only_preview() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/crossing-window-review.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let windows = |v: &serde_json::Value| -> Vec<serde_json::Value> {
        v["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["detail"]["crossing_window"].is_object())
            .cloned()
            .collect()
    };
    let before = get(&mut studio, "/api/annotations?id=part").1;
    let rows = windows(&before);
    assert_eq!(rows.len(), 2);
    for (row, x) in rows.iter().zip([8.5, -8.5]) {
        assert_eq!(row["status"], "passed");
        assert_eq!(
            row["detail"]["crossing_window"]["center"],
            json!([x, 5., 0.])
        );
        assert_eq!(row["detail"]["crossing_window"]["radius"], 3.);
        assert!(
            row["detail"]["crossing_window"]["distance"]
                .as_f64()
                .unwrap()
                < 3.
        );
    }
    let response = studio.handle("POST", "/api/parameters", br#"{"set":{"guide_y":6}}"#);
    assert_eq!(response.status, 200);
    let moved = get(&mut studio, "/api/annotations?id=part").1;
    for row in windows(&moved) {
        assert_eq!(row["detail"]["crossing_window"]["center"][1], 6.);
    }
    let state = get(&mut studio, "/api/state").1;
    let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    draft["constraints"][0]["crossing_point"]["maximum_distance"] =
        json!({"literal":{"value":0.15,"dimension":"length","unit":"inch"}});
    let (status, result) = edit_definition(&mut studio, "profile", draft, false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    let rows = windows(&result["annotations"]);
    assert!(
        (rows[0]["detail"]["crossing_window"]["radius"]
            .as_f64()
            .unwrap()
            - 3.81)
            .abs()
            < 1e-12
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(get(&mut studio, "/api/annotations?id=part").1, moved);
    assert_eq!(
        studio.handle("GET", "/crossing-review.mjs", b"").status,
        200
    );
}

#[test]
fn native_crossing_candidate_queries_are_read_only_guarded_and_use_current_draft_geometry() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/selected-crossing-pad.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/sketches").1;
    let state = get(&mut studio, "/api/state").1;
    let annotations = get(&mut studio, "/api/annotations?id=part").1;
    let baseline = data["sketches"][0]["definition"].clone();
    let query = json!({"version":data["version"],"instance":"part","feature":"profile","expected":baseline,"definition":baseline,"first":"circle","second":"guide"});
    let post = |studio: &mut serve::Studio, value: &serde_json::Value| {
        let response = studio.handle(
            "POST",
            "/api/sketch-crossing-candidates",
            value.to_string().as_bytes(),
        );
        (
            response.status,
            serde_json::from_slice::<serde_json::Value>(&response.body).unwrap(),
        )
    };
    let now = std::time::Instant::now();
    for _ in 0..10 {
        let (status, result) = post(&mut studio, &query);
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["complete"], true);
        assert_eq!(result["points"].as_array().unwrap().len(), 2);
        for p in result["points"].as_array().unwrap() {
            assert!((p[0].as_f64().unwrap().abs() - 75_f64.sqrt()).abs() < 1e-8);
            assert!((p[1].as_f64().unwrap() - 5.).abs() < 1e-8);
        }
    }
    eprintln!(
        "10 read-only native crossing snapshots: {:?}",
        now.elapsed()
    );
    assert!(now.elapsed() < std::time::Duration::from_secs(2));
    let mut draft = query.clone();
    for i in [2, 3] {
        draft["definition"]["points"][i]["y"] =
            json!({"literal":{"value":6.,"dimension":"length","unit":"millimeter"}});
    }
    let (status, result) = post(&mut studio, &draft);
    assert_eq!(status, 200, "{result}");
    for p in result["points"].as_array().unwrap() {
        assert!((p[0].as_f64().unwrap().abs() - 8.).abs() < 1e-8);
        assert!((p[1].as_f64().unwrap() - 6.).abs() < 1e-8);
    }
    for (path, value) in [
        (vec!["version"], json!(999)),
        (vec!["instance"], json!("missing")),
        (vec!["first"], json!("missing")),
        (vec!["second"], json!("circle")),
        (vec!["definition", "id"], json!("changed")),
        (vec!["expected", "id"], json!("changed")),
        (vec!["apply"], json!(true)),
    ] {
        let mut invalid = query.clone();
        let mut target = &mut invalid;
        for key in path {
            target = &mut target[key];
        }
        *target = value;
        assert_eq!(post(&mut studio, &invalid).0, 422);
    }
    let mut overlap = query.clone();
    overlap["definition"]["lines"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"duplicate","start":"guide-start","end":"guide-end"}));
    overlap["first"] = json!("guide");
    overlap["second"] = json!("duplicate");
    assert_eq!(post(&mut studio, &overlap).0, 422);
    let mut missed = query.clone();
    missed["definition"]["constraints"] = json!([]);
    missed["definition"]["points"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "crossing")
        .unwrap()["fixed"] = json!(true);
    for (i, x) in [(2, -1.), (3, 1.)] {
        missed["definition"]["points"][i]["x"] =
            json!({"literal":{"value":x,"dimension":"length","unit":"millimeter"}});
    }
    let (status, result) = post(&mut studio, &missed);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["points"], json!([]));
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(get(&mut studio, "/api/annotations?id=part").1, annotations);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
}

#[test]
fn native_crossing_candidate_queries_materialize_live_projected_splines() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/projected-spline-refinement.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/sketches").1;
    let baseline = data["sketches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == "profile")
        .unwrap()["definition"]
        .clone();
    let state = get(&mut studio, "/api/state").1;
    let mut draft = baseline.clone();
    for (id, x) in [("probe-left", -100.), ("probe-right", 100.)] {
        draft["points"].as_array_mut().unwrap().push(json!({"id":id,"x":{"literal":{"value":x,"dimension":"length","unit":"millimeter"}},"y":{"literal":{"value":10.,"dimension":"length","unit":"millimeter"}},"fixed":true}));
    }
    draft["lines"] = json!([{"id":"probe","start":"probe-left","end":"probe-right"}]);
    let body = json!({"version":data["version"],"instance":"part","feature":"profile","expected":baseline,"definition":draft,"first":"reference","second":"probe"});
    let response = studio.handle(
        "POST",
        "/api/sketch-crossing-candidates",
        body.to_string().as_bytes(),
    );
    let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 200, "{result}");
    assert_eq!(result["complete"], true);
    assert!(!result["points"].as_array().unwrap().is_empty(), "{result}");
    for p in result["points"].as_array().unwrap() {
        assert!((p[1].as_f64().unwrap() - 10.).abs() < 1e-7);
    }
    assert_eq!(get(&mut studio, "/api/state").1, state);
}

#[test]
fn drawing_to_projected_named_points_follows_source_edits_without_frozen_duplicates() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/projected-spline-refinement.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let baseline = get(&mut studio, "/api/sketches").1["sketches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == "profile")
        .unwrap()["definition"]
        .clone();
    let state = get(&mut studio, "/api/state").1;
    let mut draft = baseline.clone();
    draft["points"].as_array_mut().unwrap().push(json!({"id":"drawn-end","x":{"literal":{"value":55.,"dimension":"length","unit":"millimeter"}},"y":{"literal":{"value":-5.,"dimension":"length","unit":"millimeter"}},"fixed":true}));
    draft["lines"] =
        json!([{"id":"snapped-guide","start":"reference:junction:right","end":"drawn-end"}]);
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 200, "{result}");
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    let change = studio.handle("POST", "/api/parameters", br#"{"set":{"width":45}}"#);
    assert_eq!(
        change.status,
        200,
        "{}",
        String::from_utf8_lossy(&change.body)
    );
    let scene = |v: serde_json::Value| {
        v["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()
            .clone()
    };
    let moved = scene(get(&mut studio, "/api/annotations?id=part").1);
    let line = moved["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "snapped-guide")
        .unwrap();
    assert_eq!(
        line["points"][0],
        moved["points"]["reference:junction:right"]
    );
    assert!((line["points"][0][0].as_f64().unwrap() - 45.).abs() < 1e-8);
    let saved_definition = get(&mut studio, "/api/sketches").1["sketches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == "profile")
        .unwrap()["definition"]
        .clone();
    assert_eq!(
        saved_definition["points"].as_array().unwrap().len(),
        baseline["points"].as_array().unwrap().len() + 1
    );
    assert_eq!(saved_definition["projections"], baseline["projections"]);
    let mut invalid = saved_definition.clone();
    invalid["projections"][0]["junctions"]
        .as_array_mut()
        .unwrap()
        .retain(|j| j["id"] != "right");
    let stable = get(&mut studio, "/api/state").1;
    assert_eq!(
        edit_definition(&mut studio, "profile", invalid, true).0,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, stable);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        scene(get(&mut reopened, "/api/annotations?id=part").1)["points"]["reference:junction:right"],
        moved["points"]["reference:junction:right"]
    );
}

#[test]
fn generated_point_catalogues_and_drawn_consumers_survive_source_edits_and_save() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/slotted-plate.request.json"
    ))
    .unwrap();
    let mut value = request["model"].clone();
    value["instances"][0]["base"]["id"] = json!("part");
    value["family"]["features"][0]["operation"]["sketch_face"]["sketch"]["patterns"] = json!([
        {"linear":{"id":"row","entities":["outline:side1"],"direction":"outline:side0","spacing":{"literal":{"value":5,"dimension":"length","unit":"millimeter"}},"count":3}}
    ]);
    let document = ModelDocument::from_json(&value.to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        studio.handle("GET", "/generated-points.mjs", b"").status,
        200
    );
    let metadata = get(&mut studio, "/api/sketches").1["sketches"][0].clone();
    let names = metadata["generated_points"].as_array().unwrap();
    assert!(names.contains(&json!("row:2:outline:corner1")));
    assert!(names.contains(&json!("lower_slot:end_left")));
    let mut draft = metadata["definition"].clone();
    draft["lines"] =
        json!([{"id":"snapped-guide","start":"row:2:outline:corner1","end":"outline:corner3"}]);
    let state = get(&mut studio, "/api/state").1;
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    assert_eq!(result["generated_points"], metadata["generated_points"]);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), true);
    assert_eq!(status, 200, "{result}");
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    let response = studio.handle("POST", "/api/parameters", br#"{"set":{"width":65}}"#);
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let scene = |v: serde_json::Value| {
        v["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()
            .clone()
    };
    let moved = scene(get(&mut studio, "/api/annotations?id=part").1);
    let line = moved["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "snapped-guide")
        .unwrap();
    assert_eq!(line["points"][0], moved["points"]["row:2:outline:corner1"]);
    assert!((line["points"][0][0].as_f64().unwrap() - 75.).abs() < 1e-7);
    let saved = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    assert_eq!(saved["points"], metadata["definition"]["points"]);
    assert!(saved.get("generated_points").is_none());
    let mut invalid = saved.clone();
    invalid["patterns"][0]["linear"]["count"] = json!(2);
    let stable = get(&mut studio, "/api/state").1;
    assert_eq!(
        edit_definition(&mut studio, "profile", invalid, true).0,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, stable);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        get(&mut reopened, "/api/sketches").1["sketches"][0]["definition"],
        saved
    );
    assert_eq!(
        scene(get(&mut reopened, "/api/annotations?id=part").1)["points"]["row:2:outline:corner1"],
        moved["points"]["row:2:outline:corner1"]
    );
}

#[test]
fn generated_curve_dimensions_and_splits_follow_source_edits() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/slotted-plate.request.json"
    ))
    .unwrap();
    let mut value = request["model"].clone();
    value["instances"][0]["base"]["id"] = json!("part");
    value["family"]["features"][0]["operation"]["sketch_face"]["sketch"]["patterns"] = json!([
        {"linear":{"id":"row","entities":["outline:side1"],"direction":"outline:side0","spacing":{"literal":{"value":5,"dimension":"length","unit":"millimeter"}},"count":3}}
    ]);
    let document = ModelDocument::from_json(&value.to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/sketches").1;
    let metadata = data["sketches"][0].clone();
    assert!(
        metadata["generated_curves"]["lines"]
            .as_array()
            .unwrap()
            .contains(&json!("row:2:outline:side1"))
    );
    assert!(
        metadata["generated_curves"]["arcs"]
            .as_array()
            .unwrap()
            .contains(&json!("lower_slot:end_cap"))
    );
    assert!(
        !metadata["generated_curves"]["lines"]
            .as_array()
            .unwrap()
            .contains(&json!("lower_slot:end_cap"))
    );
    let state = get(&mut studio, "/api/state").1;
    let query = json!({"version":data["version"],"instance":"part","feature":"profile","expected":metadata["definition"],"definition":metadata["definition"],"first":"outline:side0","second":"outline:side1"});
    let response = studio.handle(
        "POST",
        "/api/sketch-crossing-candidates",
        query.to_string().as_bytes(),
    );
    let crossings: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 200, "{crossings}");
    assert_eq!(crossings["points"].as_array().unwrap().len(), 1);
    let mut draft = metadata["definition"].clone();
    let mm = |v: f64| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
    for (id, x, y, fixed) in [
        ("guide-start", 0., -10., true),
        ("guide-end", 40., -10., false),
        ("cut", 90., 20., false),
    ] {
        draft["points"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"x":mm(x),"y":mm(y),"fixed":fixed}));
    }
    draft["lines"] = json!([{"id":"guide","start":"guide-start","end":"guide-end"}]);
    draft["constraints"] = json!([
        {"horizontal":{"line":"guide"}},
        {"equal_length":{"first":"guide","second":"row:2:outline:side1"}},
        {"midpoint":{"point":"cut","line":"row:2:outline:side1"}}
    ]);
    draft["curve_operations"] = json!([{ "split":{"entity":"row:2:outline:side1","at":{"point":{"point":"cut"}},"before":"copy-low","after":"copy-high"}}]);
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    assert_eq!(result["generated_curves"], metadata["generated_curves"]);
    assert!(
        !result["generated_curves"]["lines"]
            .as_array()
            .unwrap()
            .contains(&json!("copy-low")),
        "edited pieces are not solver curves"
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 200, "{result}");
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    let response = studio.handle(
        "POST",
        "/api/parameters",
        br#"{"set":{"height":45,"width":65}}"#,
    );
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let scene = |v: serde_json::Value| {
        v["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()
            .clone()
    };
    let moved = scene(get(&mut studio, "/api/annotations?id=part").1);
    assert!((moved["points"]["guide-end"][0].as_f64().unwrap() - 45.).abs() < 1e-7);
    assert!((moved["points"]["cut"][0].as_f64().unwrap() - 75.).abs() < 1e-7);
    assert!((moved["points"]["cut"][1].as_f64().unwrap() - 22.5).abs() < 1e-7);
    let saved = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    assert!(saved.get("generated_curves").is_none());
    let mut invalid = saved.clone();
    invalid["patterns"][0]["linear"]["count"] = json!(2);
    let stable = get(&mut studio, "/api/state").1;
    assert_eq!(
        edit_definition(&mut studio, "profile", invalid, true).0,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, stable);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        get(&mut reopened, "/api/sketches").1["sketches"][0]["definition"],
        saved
    );
    assert_eq!(
        scene(get(&mut reopened, "/api/annotations?id=part").1)["points"]["cut"],
        moved["points"]["cut"]
    );
}

#[test]
fn generated_spline_parameter_links_follow_native_copies_and_source_edits() {
    for (fixture, parameter, changed) in [
        (
            include_str!("../../../../../tools/model/spline-point-links.request.json"),
            "middle_weight",
            0.9,
        ),
        (
            include_str!("../../../../../tools/model/interpolated-spline-point-links.request.json"),
            "fit_height",
            16.,
        ),
        (
            include_str!("../../../../../tools/model/periodic-spline-point-links.request.json"),
            "corner_weight",
            1.4,
        ),
    ] {
        let directory = Directory::new();
        let request: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let mut value = request["model"].clone();
        let mm = |v: f64| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
        let sketch = &mut value["family"]["features"][0]["operation"]["sketch_face"]["sketch"];
        for (id, x, y) in [("axis-a", 0., -30.), ("axis-b", 10., -30.)] {
            sketch["points"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":id,"x":mm(x),"y":mm(y),"fixed":true}));
        }
        sketch["lines"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"pattern-axis","start":"axis-a","end":"axis-b"}));
        sketch["patterns"] = json!([{ "linear":{"id":"row","entities":["curve"],"direction":"pattern-axis","spacing":mm(20.),"count":3}}]);
        let document = ModelDocument::from_json(&value.to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let original = fs::read_to_string(&model).unwrap();
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let metadata = get(&mut studio, "/api/sketches").1["sketches"][0].clone();
        assert!(
            metadata["generated_curves"]["splines"]
                .as_array()
                .unwrap()
                .contains(&json!("row:2:curve"))
        );
        let mut draft = metadata["definition"].clone();
        draft["points"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"copy-sample","x":mm(0.),"y":mm(0.),"fixed":false}));
        draft["constraints"].as_array_mut().unwrap().push(json!({"spline_point":{"point":"copy-sample","spline":"row:2:curve","fraction":{"parameter":"location"}}}));
        let state = get(&mut studio, "/api/state").1;
        let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
        assert_eq!(status, 200, "{parameter}: {result}");
        assert_eq!(result["can_apply"], true, "{result}");
        assert_eq!(get(&mut studio, "/api/state").1, state);
        assert_eq!(fs::read_to_string(&model).unwrap(), original);
        let (status, result) = edit_definition(&mut studio, "profile", draft, true);
        assert_eq!(status, 200, "{result}");
        history(&mut studio, "undo");
        history(&mut studio, "redo");
        let scene = |v: serde_json::Value| {
            v["scenes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["feature"] == "profile")
                .unwrap()
                .clone()
        };
        for fraction in [0., 0.72, 1.] {
            let response = studio.handle(
                "POST",
                "/api/parameters",
                json!({"set":{parameter:changed,"location":fraction}})
                    .to_string()
                    .as_bytes(),
            );
            assert_eq!(
                response.status,
                200,
                "{parameter}: {}",
                String::from_utf8_lossy(&response.body)
            );
            let solved = scene(get(&mut studio, "/api/annotations?id=part").1);
            let source = &solved["points"]["sample"];
            let copy = &solved["points"]["copy-sample"];
            assert!(
                (copy[0].as_f64().unwrap() - source[0].as_f64().unwrap() - 40.).abs() < 1e-7,
                "{parameter}: {solved}"
            );
            assert!((copy[1].as_f64().unwrap() - source[1].as_f64().unwrap()).abs() < 1e-7);
        }
        let saved = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
        let mut invalid = saved.clone();
        invalid["patterns"][0]["linear"]["count"] = json!(2);
        let stable = get(&mut studio, "/api/state").1;
        assert_eq!(
            edit_definition(&mut studio, "profile", invalid, true).0,
            422
        );
        assert_eq!(get(&mut studio, "/api/state").1, stable);
        assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
        let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
        assert_eq!(
            get(&mut reopened, "/api/sketches").1["sketches"][0]["definition"],
            saved
        );
    }
}

#[test]
fn conic_spline_links_respect_mirror_traversal_and_circular_transforms() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/conic-nose.request.json"
    ))
    .unwrap();
    let mut value = request["model"].clone();
    value["instances"][0]["base"]["id"] = json!("part");
    let mm = |v: f64| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
    let authored = &mut value["family"]["features"][0]["operation"]["sketch_face"]["sketch"];
    authored["patterns"] = json!([
        {"mirror":{"id":"mirror","entities":["skin"],"axis":"axis","on_axis":["tip"]}},
        {"circular":{"id":"ring","entities":["skin"],"center":"o","count":4}},
        {"linear":{"id":"row","entities":["skin"],"direction":"base","spacing":mm(20.),"count":2}}
    ]);
    let document = ModelDocument::from_json(&value.to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    for (id, spline, fraction) in [
        ("sample", "skin", 0.27),
        ("reverse-sample", "skin", 0.73),
        ("mirror-sample", "mirror:1:skin", 0.27),
        ("ring-sample", "ring:1:skin", 0.27),
        ("row-sample", "row:1:skin", 0.27),
    ] {
        draft["points"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"x":mm(0.),"y":mm(0.),"fixed":false}));
        draft["constraints"].as_array_mut().unwrap().push(json!({"spline_point":{"point":id,"spline":spline,"fraction":{"literal":{"value":fraction,"dimension":"scalar","unit":null}}}}));
    }
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true, "{result}");
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 200, "{result}");
    for rho in [0.4, 0.7] {
        let response = studio.handle(
            "POST",
            "/api/parameters",
            json!({"set":{"rho":rho,"length":45}})
                .to_string()
                .as_bytes(),
        );
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let annotations = get(&mut studio, "/api/annotations?id=part").1;
        let points = &annotations["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["points"];
        let xy = |id: &str| {
            (
                points[id][0].as_f64().unwrap(),
                points[id][1].as_f64().unwrap(),
            )
        };
        let (x, y) = xy("sample");
        let (rx, ry) = xy("reverse-sample");
        let (mx, my) = xy("mirror-sample");
        let (cx, cy) = xy("ring-sample");
        let (lx, ly) = xy("row-sample");
        assert!(
            (mx + rx).abs() < 1e-7 && (my - ry).abs() < 1e-7,
            "mirrored traversal: {points}"
        );
        assert!(
            (cx + y).abs() < 1e-7 && (cy - x).abs() < 1e-7,
            "circular copy: {points}"
        );
        assert!((lx - x - 20.).abs() < 1e-7 && (ly - y).abs() < 1e-7);
    }
    let stable = get(&mut studio, "/api/state").1;
    let mut invalid = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    invalid["points"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "c")
        .unwrap()["fixed"] = json!(false);
    invalid["constraints"].as_array_mut().unwrap().push(json!({"spline_point":{"point":"c","spline":"skin","fraction":{"literal":{"value":0.5,"dimension":"scalar","unit":null}}}}));
    assert_eq!(
        edit_definition(&mut studio, "profile", invalid, true).0,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, stable);
}

#[test]
fn projected_spline_parameter_links_materialize_native_sources_and_survive_save() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/projected-spline-refinement.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let definition = |data: serde_json::Value| {
        data["sketches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["definition"]
            .clone()
    };
    let mut draft = definition(get(&mut studio, "/api/sketches").1);
    draft["constraints"] = json!([{"spline_point":{"point":"marker","spline":"reference","fraction":{"literal":{"value":0.17,"dimension":"scalar","unit":null}}}}]);
    draft["points"][0]["fixed"] = json!(false);
    let state = get(&mut studio, "/api/state").1;
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 200, "{result}");
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    let marker = |data: serde_json::Value| {
        data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["points"]["marker"]
            .clone()
    };
    let before = marker(get(&mut studio, "/api/annotations?id=part").1);
    let response = studio.handle("POST", "/api/parameters", br#"{"set":{"width":45}}"#);
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let after = marker(get(&mut studio, "/api/annotations?id=part").1);
    assert!(
        after
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.as_f64().unwrap().is_finite())
    );
    assert!((before[0].as_f64().unwrap() - after[0].as_f64().unwrap()).abs() > 0.1);
    let saved = definition(get(&mut studio, "/api/sketches").1);
    let mut invalid = saved.clone();
    invalid["constraints"][0]["spline_point"]["spline"] = json!("reference:pole:1");
    let stable = get(&mut studio, "/api/state").1;
    assert_eq!(
        edit_definition(&mut studio, "profile", invalid, true).0,
        422
    );
    assert_eq!(get(&mut studio, "/api/state").1, stable);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(definition(get(&mut reopened, "/api/sketches").1), saved);
    assert_eq!(
        marker(get(&mut reopened, "/api/annotations?id=part").1),
        after
    );
}

#[test]
fn spline_sample_queries_are_read_only_guarded_and_create_source_linked_points() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/spline-point-links.request.json"
    ))
    .unwrap();
    let mut value = request["model"].clone();
    let mm = |v: f64| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
    let sketch = &mut value["family"]["features"][0]["operation"]["sketch_face"]["sketch"];
    for (id, x) in [("axis-a", 0.), ("axis-b", 10.)] {
        sketch["points"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"x":mm(x),"y":mm(-30.),"fixed":true}));
    }
    sketch["lines"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"pattern-axis","start":"axis-a","end":"axis-b"}));
    sketch["patterns"] = json!([{ "linear":{"id":"row","entities":["curve"],"direction":"pattern-axis","spacing":mm(20.),"count":2}}]);
    let document = ModelDocument::from_json(&value.to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    for path in ["/spline-sample-pick.mjs", "/geometry-names.mjs"] {
        assert_eq!(studio.handle("GET", path, b"").status, 200);
    }
    let data = get(&mut studio, "/api/sketches").1;
    let baseline = data["sketches"][0]["definition"].clone();
    let state = get(&mut studio, "/api/state").1;
    let scene = |v: serde_json::Value| {
        v["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()
            .clone()
    };
    let annotations = get(&mut studio, "/api/annotations?id=part").1;
    let source = scene(annotations.clone())["points"]["sample"].clone();
    let body = json!({"version":data["version"],"instance":"part","feature":"profile","expected":baseline,"definition":baseline,"spline":"row:1:curve","position":[source[0].as_f64().unwrap()+20.,source[1].as_f64().unwrap()],"maximum_distance":0.1});
    let query = |studio: &mut serve::Studio, body: &serde_json::Value| {
        let response = studio.handle(
            "POST",
            "/api/sketch-spline-sample",
            body.to_string().as_bytes(),
        );
        (
            response.status,
            serde_json::from_slice::<serde_json::Value>(&response.body).unwrap(),
        )
    };
    let start = std::time::Instant::now();
    let mut sample = serde_json::Value::Null;
    for _ in 0..10 {
        let (status, result) = query(&mut studio, &body);
        assert_eq!(status, 200, "{result}");
        sample = result;
    }
    eprintln!("10 read-only native sample picks: {:?}", start.elapsed());
    assert!((sample["fraction"].as_f64().unwrap() - 0.37).abs() < 1e-7);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(get(&mut studio, "/api/annotations?id=part").1, annotations);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    for (key, changed) in [
        ("version", json!(999)),
        ("instance", json!("missing")),
        ("spline", json!("guide")),
        ("maximum_distance", json!(0)),
        ("position", json!([1000, 1000])),
        ("expected", json!({})),
    ] {
        let mut invalid = body.clone();
        invalid[key] = changed;
        assert_eq!(query(&mut studio, &invalid).0, 422, "{invalid}");
    }
    let mut draft = baseline.clone();
    draft["points"].as_array_mut().unwrap().push(json!({"id":"picked","x":mm(sample["point"][0].as_f64().unwrap()),"y":mm(sample["point"][1].as_f64().unwrap()),"fixed":false}));
    draft["constraints"].as_array_mut().unwrap().push(json!({"spline_point":{"point":"picked","spline":"row:1:curve","fraction":{"literal":{"value":sample["fraction"],"dimension":"scalar","unit":null}}}}));
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 200, "{result}");
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    let response = studio.handle(
        "POST",
        "/api/parameters",
        br#"{"set":{"radius":12,"middle_weight":0.9}}"#,
    );
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let moved = scene(get(&mut studio, "/api/annotations?id=part").1);
    assert!(
        (moved["points"]["picked"][0].as_f64().unwrap()
            - moved["points"]["sample"][0].as_f64().unwrap()
            - 20.)
            .abs()
            < 1e-7
    );
    assert!(
        (moved["points"]["picked"][1].as_f64().unwrap()
            - moved["points"]["sample"][1].as_f64().unwrap())
        .abs()
            < 1e-7
    );
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        scene(get(&mut reopened, "/api/annotations?id=part").1)["points"]["picked"],
        moved["points"]["picked"]
    );
}

#[test]
fn spline_sample_queries_resolve_interpolated_periodic_and_projected_sources() {
    for (fixture, spline, target) in [
        (
            include_str!("../../../../../tools/model/interpolated-spline-point-links.request.json"),
            "curve",
            "sample",
        ),
        (
            include_str!("../../../../../tools/model/periodic-spline-point-links.request.json"),
            "curve",
            "sample",
        ),
        (
            include_str!("../../../../../tools/model/projected-spline-refinement.request.json"),
            "reference",
            "marker",
        ),
    ] {
        let directory = Directory::new();
        let request: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let data = get(&mut studio, "/api/sketches").1;
        let baseline = data["sketches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["definition"]
            .clone();
        let annotations = get(&mut studio, "/api/annotations?id=part").1;
        let point = &annotations["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["points"][target];
        let body = json!({"version":data["version"],"instance":"part","feature":"profile","expected":baseline,"definition":baseline,"spline":spline,"position":[point[0],point[1]],"maximum_distance":0.1});
        let response = studio.handle(
            "POST",
            "/api/sketch-spline-sample",
            body.to_string().as_bytes(),
        );
        let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(response.status, 200, "{spline}: {result}");
        assert!(result["distance"].as_f64().unwrap() < 1e-7);
        assert!(
            result["fraction"]
                .as_f64()
                .is_some_and(|v| (0. ..=1.).contains(&v))
        );
        if target == "sample" {
            assert!((result["fraction"].as_f64().unwrap() - 0.37).abs() < 1e-7);
        }
    }
}

#[test]
fn exact_fraction_sample_queries_are_guarded_read_only_and_save_generated_links() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/spline-point-links.request.json"
    ))
    .unwrap();
    let mut value = request["model"].clone();
    value["family"]["features"][0]["operation"]["sketch_face"]["sketch"]["patterns"] = json!([{ "linear":{"id":"row","entities":["curve"],"direction":"guide","spacing":{"literal":{"value":20,"dimension":"length","unit":"millimeter"}},"count":2}}]);
    let document = ModelDocument::from_json(&value.to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/sketches").1;
    let baseline = data["sketches"][0]["definition"].clone();
    let state = get(&mut studio, "/api/state").1;
    let annotations = get(&mut studio, "/api/annotations?id=part").1;
    let mut body = json!({"version":data["version"],"instance":"part","feature":"profile","expected":baseline,"definition":baseline,"spline":"row:1:curve","fraction":0.5});
    let query = |studio: &mut serve::Studio, body: &serde_json::Value| {
        let response = studio.handle(
            "POST",
            "/api/sketch-spline-sample",
            body.to_string().as_bytes(),
        );
        (
            response.status,
            serde_json::from_slice::<serde_json::Value>(&response.body).unwrap(),
        )
    };
    for fraction in [0., 0.37, 0.5, 1.] {
        body["fraction"] = json!(fraction);
        let (status, result) = query(&mut studio, &body);
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["fraction"], json!(fraction));
        assert!(result["distance"].is_null());
        let a = (1_f64 - fraction).powi(2);
        let b = 2. * fraction * (1. - fraction) * 0.5_f64.sqrt();
        let c = fraction * fraction;
        let denominator = a + b + c;
        assert!(
            (result["point"][0].as_f64().unwrap() - 20. - 10. * (a + b) / denominator).abs() < 1e-7
        );
        assert!((result["point"][1].as_f64().unwrap() - 10. * (b + c) / denominator).abs() < 1e-7);
    }
    body["fraction"] = json!(0.5);
    let start = std::time::Instant::now();
    for _ in 0..10 {
        assert_eq!(query(&mut studio, &body).0, 200);
    }
    eprintln!(
        "10 exact-fraction native sample queries: {:?}",
        start.elapsed()
    );
    for (key, changed) in [
        ("fraction", json!(-0.01)),
        ("fraction", json!(1.01)),
        ("fraction", json!("half")),
        ("fraction", serde_json::Value::Null),
        ("position", json!([0, 0])),
        ("maximum_distance", json!(1)),
        ("spline", json!("guide")),
        ("version", json!(999)),
        ("expected", json!({})),
    ] {
        let mut invalid = body.clone();
        invalid[key] = changed;
        assert_eq!(query(&mut studio, &invalid).0, 422, "{invalid}");
    }
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(get(&mut studio, "/api/annotations?id=part").1, annotations);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    let result = query(&mut studio, &body).1;
    let mm = |v: f64| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
    let mut draft = baseline;
    draft["points"].as_array_mut().unwrap().push(json!({"id":"exact-sample","x":mm(result["point"][0].as_f64().unwrap()),"y":mm(result["point"][1].as_f64().unwrap()),"fixed":false}));
    draft["constraints"].as_array_mut().unwrap().push(json!({"spline_point":{"point":"exact-sample","spline":"row:1:curve","fraction":{"literal":{"value":0.5,"dimension":"scalar","unit":null}}}}));
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    let (status, result) = edit_definition(&mut studio, "profile", draft, true);
    assert_eq!(status, 200, "{result}");
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    let response = studio.handle(
        "POST",
        "/api/parameters",
        br#"{"set":{"radius":12,"middle_weight":0.9}}"#,
    );
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let sample = |v: serde_json::Value| {
        v["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["points"]["exact-sample"]
            .clone()
    };
    let moved = sample(get(&mut studio, "/api/annotations?id=part").1);
    let mid = 12. * (0.5 + 0.9) / (1. + 0.9);
    assert!((moved[0].as_f64().unwrap() - 20. - mid).abs() < 1e-7);
    assert!((moved[1].as_f64().unwrap() - mid).abs() < 1e-7);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let mut reopened = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        sample(get(&mut reopened, "/api/annotations?id=part").1),
        moved
    );
}

#[test]
fn exact_fraction_queries_resolve_interpolation_periodic_seams_conics_and_projections() {
    for (fixture, spline, closed) in [
        (
            include_str!("../../../../../tools/model/interpolated-spline-point-links.request.json"),
            "curve",
            false,
        ),
        (
            include_str!("../../../../../tools/model/periodic-spline-point-links.request.json"),
            "curve",
            true,
        ),
        (
            include_str!("../../../../../tools/model/conic-nose.request.json"),
            "skin",
            false,
        ),
        (
            include_str!("../../../../../tools/model/projected-spline-refinement.request.json"),
            "reference",
            true,
        ),
    ] {
        let directory = Directory::new();
        let request: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let mut value = request["model"].clone();
        value["instances"][0]["base"]["id"] = json!("part");
        let document = ModelDocument::from_json(&value.to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let data = get(&mut studio, "/api/sketches").1;
        let baseline = data["sketches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == "profile")
            .unwrap()["definition"]
            .clone();
        let mut endpoints = Vec::new();
        for fraction in [0., 0.37, 0.5, 1.] {
            let body = json!({"version":data["version"],"instance":"part","feature":"profile","expected":baseline,"definition":baseline,"spline":spline,"fraction":fraction});
            let response = studio.handle(
                "POST",
                "/api/sketch-spline-sample",
                body.to_string().as_bytes(),
            );
            let result: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(response.status, 200, "{spline}: {result}");
            assert_eq!(result["fraction"], json!(fraction));
            assert!(
                result["point"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v.as_f64().is_some_and(f64::is_finite))
            );
            if fraction == 0. || fraction == 1. {
                endpoints.push(result["point"].clone());
            }
            if fraction == 0.37 && spline == "curve" {
                let annotations = get(&mut studio, "/api/annotations?id=part").1;
                let sample = &annotations["scenes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["feature"] == "profile")
                    .unwrap()["points"]["sample"];
                for axis in 0..2 {
                    assert!(
                        (sample[axis].as_f64().unwrap() - result["point"][axis].as_f64().unwrap())
                            .abs()
                            < 1e-7
                    );
                }
            }
            if fraction == 0.5 && spline == "skin" {
                assert!((result["point"][0].as_f64().unwrap() - 11.25).abs() < 1e-7);
                assert!((result["point"][1].as_f64().unwrap() - 30.).abs() < 1e-7);
            }
        }
        if closed {
            for (start, end) in endpoints[0]
                .as_array()
                .unwrap()
                .iter()
                .zip(endpoints[1].as_array().unwrap())
            {
                assert!((start.as_f64().unwrap() - end.as_f64().unwrap()).abs() < 1e-7);
            }
        }
    }
}

#[test]
fn reference_dimension_drafts_measure_generated_geometry_and_survive_history_and_save() {
    let directory = Directory::new();
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/slotted-plate.request.json"
    ))
    .unwrap();
    request["model"]["instances"][0]["base"]["id"] = json!("part");
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let scene = |data: serde_json::Value| {
        data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
            .unwrap()
            .clone()
    };
    let accepted = scene(get(&mut studio, "/api/annotations?id=part").1);
    let state = get(&mut studio, "/api/state").1;
    let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let constraints = draft["constraints"].clone();
    draft["measurements"].as_array_mut().unwrap().push(json!({
        "id":"width_reference", "measure":{"curve_length":{"curve":"outline:side0"}}
    }));
    let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["can_apply"], true);
    let reviewed = scene(preview["annotations"].clone());
    assert_eq!(reviewed["points"], accepted["points"]);
    assert_eq!(reviewed["solver"], accepted["solver"]);
    let measurement = |s: &serde_json::Value| {
        s["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["detail"]["measurement"]["id"] == "width_reference")
            .unwrap()
            .clone()
    };
    assert_eq!(measurement(&reviewed)["detail"]["value"], 80.0);
    assert_eq!(measurement(&reviewed)["detail"]["driving"], false);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    assert_eq!(
        edit_definition(&mut studio, "profile", draft.clone(), true).0,
        200
    );
    history(&mut studio, "undo");
    assert_eq!(
        get(&mut studio, "/api/sketches").1["sketches"][0]["definition"]["measurements"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    history(&mut studio, "redo");
    let response = studio.handle("POST", "/api/parameters", br#"{"set":{"width":90}}"#);
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    assert_eq!(
        measurement(&scene(get(&mut studio, "/api/annotations?id=part").1))["detail"]["value"],
        90.0
    );
    assert_eq!(
        get(&mut studio, "/api/sketches").1["sketches"][0]["definition"]["constraints"],
        constraints
    );
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        measurement(&scene(get(&mut studio, "/api/annotations?id=part").1))["detail"]["value"],
        90.0
    );
    let stable = get(&mut studio, "/api/state").1;
    let saved = fs::read_to_string(&model).unwrap();
    draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    draft["measurements"][2]["measure"]["curve_length"]["curve"] = json!("missing:side0");
    assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 422);
    assert_eq!(get(&mut studio, "/api/state").1, stable);
    assert_eq!(fs::read_to_string(&model).unwrap(), saved);
    let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    draft["measurements"].as_array_mut().unwrap().pop();
    assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 200);
    assert_eq!(
        get(&mut studio, "/api/sketches").1["sketches"][0]["definition"]["measurements"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn advanced_relationship_drafts_restore_native_g2_and_seeded_contacts() {
    for (name, kind) in [
        ("g2-paddle", "curvature_continuous"),
        ("ellipse-tangent-pair", "ellipse_ellipse_tangent"),
        ("spline-tangent-pair", "spline_spline_tangent"),
        ("spline-tangent-ellipse", "spline_ellipse_tangent"),
    ] {
        let directory = Directory::new();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../tools/model/{name}.request.json"));
        let mut request: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(source).unwrap()).unwrap();
        request["model"]["instances"][0]["base"]["id"] = json!("part");
        let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
        let constraints = draft["constraints"].as_array_mut().unwrap();
        let index = constraints
            .iter()
            .position(|c| c.get(kind).is_some())
            .unwrap();
        let relationship = constraints.remove(index);
        let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), true);
        assert_eq!(status, 200, "{name}: {result}");
        draft["constraints"]
            .as_array_mut()
            .unwrap()
            .push(relationship.clone());
        let state = get(&mut studio, "/api/state").1;
        let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
        assert_eq!(status, 200, "{name}: {preview}");
        assert_eq!(preview["can_apply"], true, "{name}: {preview}");
        assert_eq!(get(&mut studio, "/api/state").1, state);
        let scene = preview["annotations"]["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
            .unwrap();
        let annotation = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["detail"]["constraint"] == relationship)
            .unwrap();
        assert_eq!(annotation["status"], "passed", "{name}: {annotation}");
        assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 200);
        history(&mut studio, "undo");
        history(&mut studio, "redo");
        assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
        assert_eq!(
            draft["constraints"].as_array().unwrap().last().unwrap(),
            &relationship
        );
        let stable = get(&mut studio, "/api/state").1;
        let saved = fs::read_to_string(&model).unwrap();
        draft["constraints"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap()[kind]["point"] = json!("missing-contact");
        assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 422);
        assert_eq!(get(&mut studio, "/api/state").1, stable);
        assert_eq!(fs::read_to_string(&model).unwrap(), saved);
    }
}

#[test]
fn weighted_authoring_drafts_follow_generated_points_and_parameter_weights_after_save() {
    let directory = Directory::new();
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/slotted-plate.request.json"
    ))
    .unwrap();
    request["model"]["instances"][0]["base"]["id"] = json!("part");
    request["model"]["family"]["parameters"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"bias","parameter_type":{"scalar":"scalar"},
            "default":{"scalar":{"value":2,"dimension":"scalar","unit":null}}
        }));
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let original = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        studio
            .handle("GET", "/weighted-point-controls.mjs", b"")
            .status,
        200
    );
    let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let mm = |value| json!({"literal":{"value":value,"dimension":"length","unit":"millimeter"}});
    draft["points"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"average","x":mm(0),"y":mm(0),"fixed":false}));
    let link = json!({"weighted_point":{"point":"average","terms":[
        {"point":"a","weight":{"literal":{"value":1.0,"dimension":"scalar","unit":null}}},
        {"point":"outline:corner1","weight":{"parameter":"bias"}}
    ]}});
    draft["constraints"]
        .as_array_mut()
        .unwrap()
        .push(link.clone());
    let state = get(&mut studio, "/api/state").1;
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(fs::read_to_string(&model).unwrap(), original);
    assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 200);
    let average = |data: serde_json::Value| {
        data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
            .unwrap()["points"]["average"][0]
            .as_f64()
            .unwrap()
    };
    assert!((average(get(&mut studio, "/api/annotations?id=part").1) - 160.0 / 3.0).abs() < 1e-8);
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    let response = studio.handle(
        "POST",
        "/api/parameters",
        br#"{"set":{"bias":3,"width":90}}"#,
    );
    assert_eq!(response.status, 200);
    assert!((average(get(&mut studio, "/api/annotations?id=part").1) - 67.5).abs() < 1e-8);
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert!((average(get(&mut studio, "/api/annotations?id=part").1) - 67.5).abs() < 1e-8);
    let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    assert_eq!(draft["constraints"].as_array().unwrap().last(), Some(&link));
    let stable = get(&mut studio, "/api/state").1;
    let saved = fs::read_to_string(&model).unwrap();
    draft["constraints"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap()["weighted_point"]["terms"][1]["point"] = json!("lost:corner1");
    assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 422);
    assert_eq!(get(&mut studio, "/api/state").1, stable);
    assert_eq!(fs::read_to_string(&model).unwrap(), saved);
}

#[test]
fn authored_pattern_drafts_generate_all_three_kinds_and_guard_dependent_references() {
    for pattern in [
        json!({"linear":{"id":"copies","entities":["curve"],"direction":"guide","spacing":{"literal":{"value":20.0,"dimension":"length","unit":"millimeter"}},"count":3}}),
        json!({"circular":{"id":"copies","entities":["curve"],"center":"guide-start","count":4}}),
        json!({"mirror":{"id":"copies","entities":["curve"],"axis":"guide"}}),
        json!({"linear":{"id":"copies","entities":["outline:side0","outline:side1","outline:side2","outline:side3"],"direction":"pattern-axis","spacing":{"literal":{"value":100.0,"dimension":"length","unit":"millimeter"}},"count":2}}),
    ] {
        let directory = Directory::new();
        let shape = pattern["linear"]["entities"][0] == "outline:side0";
        let source = if shape {
            include_str!("../../../../../tools/model/slotted-plate.request.json")
        } else {
            include_str!("../../../../../tools/model/spline-point-links.request.json")
        };
        let mut request: serde_json::Value = serde_json::from_str(source).unwrap();
        request["model"]["instances"][0]["base"]["id"] = json!("part");
        if shape {
            request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]["lines"] =
                json!([{"id":"pattern-axis","start":"s1a","end":"s1b"}]);
        }
        let copy = if shape {
            "copies:1:outline:side0"
        } else {
            "copies:1:curve"
        };
        let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let model = PathBuf::from(directory.model(&document));
        let original = fs::read_to_string(&model).unwrap();
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        assert_eq!(
            studio.handle("GET", "/pattern-controls.mjs", b"").status,
            200
        );
        let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
        let constraints = draft["constraints"].clone();
        draft["patterns"] = json!([pattern]);
        draft["measurements"] =
            json!([{"id":"copy_length","measure":{"curve_length":{"curve":copy}}}]);
        let state = get(&mut studio, "/api/state").1;
        let (status, preview) = edit_definition(&mut studio, "profile", draft.clone(), false);
        assert_eq!(status, 200, "{pattern}: {preview}");
        assert_eq!(preview["can_apply"], true, "{preview}");
        assert_eq!(get(&mut studio, "/api/state").1, state);
        assert_eq!(fs::read_to_string(&model).unwrap(), original);
        assert!(
            preview["generated_curves"][if shape { "lines" } else { "splines" }]
                .as_array()
                .unwrap()
                .contains(&json!(copy))
        );
        let scene = preview["annotations"]["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
            .unwrap();
        assert!(scene["points"].get("copies:1:a").is_some());
        assert!(
            scene["annotations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["detail"]["pattern"] == pattern)
        );
        assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 200);
        history(&mut studio, "undo");
        history(&mut studio, "redo");
        assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
        assert_eq!(draft["patterns"][0], pattern);
        assert_eq!(draft["constraints"], constraints);
        let stable = get(&mut studio, "/api/state").1;
        draft["patterns"] = json!([]);
        assert_eq!(
            edit_definition(&mut studio, "profile", draft.clone(), true).0,
            422
        );
        assert_eq!(get(&mut studio, "/api/state").1, stable);
        draft["measurements"] = json!([]);
        assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 200);
    }
}

#[test]
fn sketch_studio_page_and_every_module_it_imports_are_served() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let page = studio.handle("GET", "/studio", b"");
    assert_eq!(page.status, 200);
    assert!(page.content_type.starts_with("text/html"));
    // Follow relative imports from the page through every module.
    let mut pending = vec![String::from_utf8(page.body).unwrap()];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(source) = pending.pop() {
        for quote in ['\'', '"'] {
            let marker = format!("from {quote}./");
            for (index, _) in source.match_indices(&marker) {
                let rest = &source[index + marker.len()..];
                let name = &rest[..rest.find(quote).unwrap()];
                if seen.insert(name.to_owned()) {
                    let response = studio.handle("GET", &format!("/{name}"), b"");
                    assert_eq!(response.status, 200, "{name} is not served");
                    assert!(
                        response.content_type.starts_with("text/javascript"),
                        "{name}"
                    );
                    pending.push(String::from_utf8(response.body).unwrap());
                }
            }
        }
    }
    assert!(seen.contains("sketch-studio-app.mjs"), "{seen:?}");
    // The classic viewer links to the studio.
    let classic = String::from_utf8(studio.handle("GET", "/", b"").body).unwrap();
    assert!(classic.contains("href=\"/studio\""));
    assert!(seen.contains("viewer.mjs"), "{seen:?}");
}

#[test]
fn studio_primitive_gallery_regenerates_exact_curves_text_and_closed_control_splines() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/studio-primitives.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        studio
            .handle("GET", "/sketch-studio-primitives.mjs", b"")
            .status,
        200
    );
    let data = get(&mut studio, "/api/annotations?id=part").1;
    let scene = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
        .unwrap();
    assert_eq!(scene["solver"]["solved"], true);
    for id in ["A1", "E1", "Conic1", "S1"] {
        assert!(
            scene["entities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == id),
            "{id}"
        );
    }
    assert!(
        scene["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["text"] == "Text1")
    );
    let constraints = scene["annotations"].as_array().unwrap();
    assert!(constraints.iter().any(|a|a["detail"]["constraint"].get("midpoint").is_some() && a["status"]=="passed"));
    let ellipse = scene["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "E1")
        .unwrap();
    let points = ellipse["points"].as_array().unwrap();
    for (p, xy) in [
        (points.first().unwrap(), [45., 0.]),
        (points.last().unwrap(), [40., 3.]),
    ] {
        assert!((p[0].as_f64().unwrap() - xy[0]).hypot(p[1].as_f64().unwrap() - xy[1]) < 1e-7);
    }
    let mut draft = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let spline = draft["splines"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|s| s["id"] == "S1")
        .unwrap();
    let first = spline["points"][0].clone();
    spline["points"].as_array_mut().unwrap().push(first);
    spline["basis"]["knots"] = json!([0., 0.5, 1.]);
    spline["basis"]["multiplicities"] = json!([4, 1, 4]);
    spline["basis"]["weights"]
        .as_array_mut()
        .unwrap()
        .push(json!({"literal":{"value":1.,"dimension":"scalar","unit":null}}));
    let state = get(&mut studio, "/api/state").1;
    let (status, result) = edit_definition(&mut studio, "profile", draft.clone(), false);
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["can_apply"], true, "{result}");
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(edit_definition(&mut studio, "profile", draft, true).0, 200);
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"{}").status, 200);
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let mut bad = get(&mut studio, "/api/sketches").1["sketches"][0]["definition"].clone();
    let stable = get(&mut studio, "/api/state").1;
    bad["texts"][0]["font"] = json!("Missing Studio Test Font 998877");
    assert_eq!(edit_definition(&mut studio, "profile", bad, true).0, 422);
    assert_eq!(get(&mut studio, "/api/state").1, stable);
}

#[test]
fn studio_holes_preview_all_instances_and_apply_with_history_and_save() {
    for extent in ["blind", "through_all", "up_to_next", "up_to_face"] {
        let directory = Directory::new();
        let document = edge_finish_document();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        assert_eq!(studio.handle("GET", "/solid-holes.mjs", b"").status, 200);
        let data = get(&mut studio, "/api/features").1;
        let state = get(&mut studio, "/api/state").1;
        let q = |v| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
        // Start at the bottom face so both thickness variants share the entry.
        let hole = json!({"id":"drilled","operation":{"hole":{"input":"body","position":{"components":{"x":q(10.0),"y":q(10.0),"z":q(0.0)}},"axis":{"literal":{"x":{"value":0,"dimension":"scalar","unit":null},"y":{"value":0,"dimension":"scalar","unit":null},"z":{"value":1,"dimension":"scalar","unit":null}}},"diameter":q(4.0),"extent":if extent=="blind" {json!({"blind":{"depth":{"parameter":"finish-size"}}})}else if extent=="up_to_face" {json!({"up_to_face":{"face":{"at_extreme":{"axis":"z","extremum":"maximum","tolerance":q(1e-6)}}}})}else{json!(extent)},"bottom":"flat","finish":"plain"}}});
        let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"add","features":[hole],"output":"drilled","apply":false});
        let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
        assert_eq!(response.status, 200);
        let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(preview["can_apply"], true, "{preview}");
        for (id, height) in [("block", 10.0), ("thin", 5.0)] {
            let bore_depth = if extent == "blind" { 1.0 } else { height };
            let expected = 1000.0 * height - std::f64::consts::PI * 4.0 * bore_depth;
            assert!(
                (preview["geometry"][id]["volume_mm3"].as_f64().unwrap() - expected).abs() < 1e-5
            );
        }
        assert!(preview["gltf"]["meshes"].is_array());
        assert_eq!(get(&mut studio, "/api/state").1, state);
        if extent == "blind" {
            let mut invalid = edit.clone();
            invalid["features"][0]["operation"]["hole"]["extent"]["blind"]["depth"] = q(6.0);
            invalid["features"][0]["operation"]["hole"]["bottom"] = json!({"drill_point":{"angle_radians":{"literal":{"value":2.0,"dimension":"scalar","unit":null}}}});
            let response = studio.handle("POST", "/api/features", invalid.to_string().as_bytes());
            let rejected: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(rejected["can_apply"], false, "{rejected}");
            assert!(rejected.get("gltf").is_none());
            invalid["apply"] = json!(true);
            assert_ne!(
                studio
                    .handle("POST", "/api/features", invalid.to_string().as_bytes())
                    .status,
                200
            );
            assert_eq!(get(&mut studio, "/api/state").1, state);
        }
        edit["apply"] = json!(true);
        assert_eq!(
            studio
                .handle("POST", "/api/features", edit.to_string().as_bytes())
                .status,
            200
        );
        history(&mut studio, "undo");
        assert_eq!(
            get(&mut studio, "/api/features").1["features"],
            data["features"]
        );
        history(&mut studio, "redo");
        assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
        let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
        let expected: FeatureDefinition =
            serde_json::from_value(edit["features"][0].clone()).unwrap();
        assert_eq!(
            saved.family.features.iter().find(|f| f.id == "drilled"),
            Some(&expected)
        );
    }
}

#[test]
fn studio_hole_finishes_preview_analytic_volumes_and_preserve_edited_drivers() {
    let q = |v| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
    let angle = |v| json!({"literal":{"value":v,"dimension":"scalar","unit":null}});
    let pi = std::f64::consts::PI;
    let tip_height = 2.0 / (59.0_f64.to_radians()).tan();
    for (name, finish, bottom, removed) in [
        (
            "drill",
            json!("plain"),
            json!({"drill_point":{"angle_radians":angle(118.0_f64.to_radians())}}),
            pi * 4.0 * 3.0 + pi * 4.0 * tip_height / 3.0,
        ),
        (
            "counterbore",
            json!({"counterbore":{"diameter":q(8.0),"depth":{"parameter":"finish-size"}}}),
            json!("flat"),
            pi * 24.0,
        ),
        (
            "relief",
            json!({"counterbore":{"diameter":q(8.0),"depth":{"parameter":"finish-size"},"relief":q(6.0)}}),
            json!("flat"),
            pi * 24.0 + pi * 3.0_f64.sqrt() * 7.0 / 3.0,
        ),
        (
            "countersink",
            json!({"countersink":{"diameter":q(8.0),"angle_radians":angle(pi/2.0)}}),
            json!("flat"),
            pi * 68.0 / 3.0,
        ),
    ] {
        let directory = Directory::new();
        let document = edge_finish_document();
        let model = PathBuf::from(directory.model(&document));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let data = get(&mut studio, "/api/features").1;
        let state = get(&mut studio, "/api/state").1;
        let hole = json!({"id":"finished-hole","operation":{"hole":{"input":"body","position":{"components":{"x":q(10.0),"y":q(10.0),"z":q(0.0)}},"axis":{"components":{"x":angle(0.0),"y":angle(0.0),"z":angle(1.0)}},"diameter":q(4.0),"extent":{"blind":{"depth":q(3.0)}},"bottom":bottom,"finish":finish}}});
        let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"add","features":[hole],"output":"finished-hole","apply":false});
        let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
        assert_eq!(response.status, 200);
        let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(preview["can_apply"], true, "{name}: {preview}");
        for (id, volume) in [("block", 10000.0), ("thin", 5000.0)] {
            assert!(
                (preview["geometry"][id]["volume_mm3"].as_f64().unwrap() - (volume - removed))
                    .abs()
                    < 1e-5,
                "{name}: {preview}"
            );
        }
        assert_eq!(get(&mut studio, "/api/state").1, state);
        let mut invalid = edit.clone();
        let invalid_hole = &mut invalid["features"][0]["operation"]["hole"];
        match name {
            "drill" => invalid_hole["extent"]["blind"]["depth"] = q(4.0),
            "counterbore" => invalid_hole["finish"]["counterbore"]["diameter"] = q(3.0),
            "relief" => invalid_hole["finish"]["counterbore"]["relief"] = q(8.0),
            _ => invalid_hole["finish"]["countersink"]["angle_radians"] = angle(pi),
        }
        let response = studio.handle("POST", "/api/features", invalid.to_string().as_bytes());
        let rejected: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(rejected["can_apply"], false, "{name}: {rejected}");
        assert!(rejected.get("gltf").is_none());
        invalid["apply"] = json!(true);
        assert_ne!(
            studio
                .handle("POST", "/api/features", invalid.to_string().as_bytes())
                .status,
            200
        );
        assert_eq!(get(&mut studio, "/api/state").1, state);
        edit["apply"] = json!(true);
        assert_eq!(
            studio
                .handle("POST", "/api/features", edit.to_string().as_bytes())
                .status,
            200
        );
        history(&mut studio, "undo");
        history(&mut studio, "redo");
        let current = get(&mut studio, "/api/features").1;
        edit["version"] = current["version"].clone();
        edit["expected"] = current["features"].clone();
        edit["action"] = json!("replace");
        edit["features"][0]["operation"]["hole"]["diameter"] = json!({"add":[q(3.5),q(0.5)]});
        assert_eq!(
            studio
                .handle("POST", "/api/features", edit.to_string().as_bytes())
                .status,
            200
        );
        assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
        let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
        let expected: FeatureDefinition =
            serde_json::from_value(edit["features"][0].clone()).unwrap();
        assert_eq!(
            saved
                .family
                .features
                .iter()
                .find(|f| f.id == "finished-hole"),
            Some(&expected)
        );
    }
}

#[test]
fn canvas_hole_face_normals_point_inward_for_top_bottom_and_side_entries() {
    for (axis, coordinate, point, normal, extent) in [
        (
            2,
            10.0,
            [8.0, 9.0, 10.0],
            [0.0, 0.0, 1.0],
            json!("through_all"),
        ),
        (
            2,
            0.0,
            [8.0, 9.0, 0.0],
            [0.0, 0.0, -1.0],
            json!({"blind":{"depth":{"literal":{"value":2.0,"dimension":"length","unit":"millimeter"}}}}),
        ),
        (
            0,
            0.0,
            [0.0, 9.0, 2.5],
            [-1.0, 0.0, 0.0],
            json!({"blind":{"depth":{"literal":{"value":2.0,"dimension":"length","unit":"millimeter"}}}}),
        ),
    ] {
        let directory = Directory::new();
        let model = PathBuf::from(directory.model(&edge_finish_document()));
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let data = get(&mut studio, "/api/features").1;
        let response = studio.handle(
            "POST",
            "/api/sketch-source-view",
            json!({"version":data["version"],"instance":"block","output":"body"})
                .to_string()
                .as_bytes(),
        );
        let source: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        let scene = source["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "solid" && s["feature"] == "body")
            .unwrap();
        let face = scene["mesh"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| {
                t["points"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|p| (p[axis].as_f64().unwrap() - coordinate).abs() < 1e-8)
            })
            .unwrap()["face"]
            .as_u64()
            .unwrap() as usize;
        let (status, picked) = pick_face(&mut studio, "block", "body", face);
        assert_eq!(status, 200);
        for (i, n) in normal.iter().enumerate() {
            assert!((picked["normal"][i].as_f64().unwrap() - n).abs() < 1e-8);
        }
        let q = |v| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
        let a = |i: usize| json!({"literal":{"value":-picked["normal"][i].as_f64().unwrap(),"dimension":"scalar","unit":null}});
        let feature = json!({"id":"placed-hole","operation":{"hole":{"input":"body","position":{"components":{"x":q(point[0]),"y":q(point[1]),"z":q(point[2])}},"axis":{"components":{"x":a(0),"y":a(1),"z":a(2)}},"diameter":q(2.0),"extent":extent,"bottom":"flat","finish":"plain"}}});
        let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"add","features":[feature],"output":"placed-hole","apply":false});
        let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
        let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(preview["can_apply"], true, "{preview}");
        for (id, height) in [("block", 10.0), ("thin", 5.0)] {
            let depth = if coordinate == 10.0 { height } else { 2.0 };
            let expected = height * 1000.0 - std::f64::consts::PI * depth;
            assert!(
                (preview["geometry"][id]["volume_mm3"].as_f64().unwrap() - expected).abs() < 1e-5
            );
        }
        edit["apply"] = json!(true);
        assert_eq!(
            studio
                .handle("POST", "/api/features", edit.to_string().as_bytes())
                .status,
            200
        );
        assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
        let saved = ModelDocument::from_json(&fs::read_to_string(model).unwrap()).unwrap();
        let expected: FeatureDefinition =
            serde_json::from_value(edit["features"][0].clone()).unwrap();
        assert_eq!(
            saved.family.features.iter().find(|f| f.id == "placed-hole"),
            Some(&expected)
        );
    }
}

#[test]
fn loft_end_constraints_preview_save_and_reject_unsatisfied_conditions() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/profile-loft.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        studio
            .handle("GET", "/solid-loft-constraints.mjs", b"")
            .status,
        200
    );
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let mut feature = data["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "body")
        .unwrap()
        .clone();
    let constraint = json!({"direction":{"literal":{"x":{"value":0,"dimension":"scalar","unit":null},"y":{"value":0,"dimension":"scalar","unit":null},"z":{"value":1,"dimension":"scalar","unit":null}}},"magnitude":{"parameter":"height"}});
    feature["operation"]["profile_loft"]["ruled"] = json!(false);
    feature["operation"]["profile_loft"]["start_constraint"] = constraint.clone();
    feature["operation"]["profile_loft"]["end_constraint"] = constraint;
    let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"replace","features":[feature],"output":"body","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(preview["can_apply"], true, "{preview}");
    let scene = preview["annotations"]["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "solid" && s["feature"] == "body")
        .unwrap();
    for end in ["start", "end"] {
        let tangent = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == format!("loft-{end}-tangent"))
            .unwrap();
        assert_eq!(tangent["detail"]["strength_mm"], 20.);
        assert_eq!(tangent["detail"]["direction"], json!([0., 0., 1.]));
    }
    assert_eq!(
        studio
            .handle("GET", "/sketch-studio-loft-guides.mjs", b"")
            .status,
        200
    );
    let expected = 20. * std::f64::consts::PI * (64. - 32. + 16. * 13. / 35.);
    assert!((preview["geometry"]["block"]["volume_mm3"].as_f64().unwrap() - expected).abs() < 1e-4);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    for invalid in ["ruled", "parallel", "zero"] {
        let mut bad = edit.clone();
        let op = &mut bad["features"][0]["operation"]["profile_loft"];
        if invalid == "ruled" {
            op["ruled"] = json!(true);
        } else if invalid == "parallel" {
            op["start_constraint"]["direction"]["literal"]["z"]["value"] = json!(0);
            op["start_constraint"]["direction"]["literal"]["x"]["value"] = json!(1);
        } else {
            op["start_constraint"]["magnitude"] =
                json!({"literal":{"value":0,"dimension":"length","unit":"millimeter"}});
        }
        let response = studio.handle("POST", "/api/features", bad.to_string().as_bytes());
        let rejected: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(rejected["can_apply"], false, "{rejected}");
        assert!(rejected.get("gltf").is_none());
        bad["apply"] = json!(true);
        assert_ne!(
            studio
                .handle("POST", "/api/features", bad.to_string().as_bytes())
                .status,
            200
        );
        assert_eq!(get(&mut studio, "/api/state").1, state);
    }
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        200
    );
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    let expected: FeatureDefinition = serde_json::from_value(edit["features"][0].clone()).unwrap();
    assert_eq!(
        saved.family.features.iter().find(|f| f.id == "body"),
        Some(&expected)
    );
}

#[test]
fn loft_constraint_drafts_cannot_apply_when_another_variants_strength_is_invalid() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/profile-loft.request.json"
    ))
    .unwrap();
    let mut doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    doc.family.parameters.push(ParameterDefinition {
        id: "constraint-strength".into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: ParameterValue::Scalar(Quantity::length(20., LengthUnit::Millimeter)),
        minimum: None,
        maximum: None,
    });
    doc.instances.push(InstanceNode::Clone {
        id: "bad-strength".into(),
        source: "block".into(),
        overrides: HashMap::from([(
            "constraint-strength".into(),
            ParameterValue::Scalar(Quantity::length(-1., LengthUnit::Millimeter)),
        )]),
        placement: Default::default(),
        frame: None,
        provenance: "test".into(),
    });
    let model = PathBuf::from(directory.model(&doc));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let mut feature = data["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "body")
        .unwrap()
        .clone();
    feature["operation"]["profile_loft"]["ruled"] = json!(false);
    feature["operation"]["profile_loft"]["start_constraint"] = json!({"direction":{"literal":{"x":{"value":0,"dimension":"scalar","unit":null},"y":{"value":0,"dimension":"scalar","unit":null},"z":{"value":1,"dimension":"scalar","unit":null}}},"magnitude":{"parameter":"constraint-strength"}});
    let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"replace","features":[feature],"output":"body","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    let rejected: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(rejected["can_apply"], false, "{rejected}");
    assert!(rejected.get("gltf").is_none());
    edit["apply"] = json!(true);
    assert_ne!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        200
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
}

#[test]
fn normal_loft_constraints_validate_both_station_directions_and_save_without_fixed_vectors() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/profile-loft.request.json"
    ))
    .unwrap();
    let mut doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    doc.family
        .parameters
        .iter_mut()
        .find(|p| p.id == "height")
        .unwrap()
        .minimum = None;
    doc.instances.push(InstanceNode::Clone {
        id: "below".into(),
        source: "block".into(),
        overrides: HashMap::from([(
            "height".into(),
            ParameterValue::Scalar(Quantity::length(-20., LengthUnit::Millimeter)),
        )]),
        placement: Default::default(),
        frame: None,
        provenance: "test".into(),
    });
    let model = PathBuf::from(directory.model(&doc));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let mut feature = data["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "body")
        .unwrap()
        .clone();
    let c = json!({"normal_to_profile":true,"magnitude":{"literal":{"value":20.,"dimension":"length","unit":"millimeter"}}});
    feature["operation"]["profile_loft"]["ruled"] = json!(false);
    feature["operation"]["profile_loft"]["start_constraint"] = c.clone();
    feature["operation"]["profile_loft"]["end_constraint"] = c;
    let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"replace","features":[feature],"output":"body","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(preview["can_apply"], true, "{preview}");
    let expected = 20. * std::f64::consts::PI * (64. - 32. + 16. * 13. / 35.);
    for id in ["block", "below"] {
        assert!((preview["geometry"][id]["volume_mm3"].as_f64().unwrap() - expected).abs() < 1e-4);
    }
    for both in [false, true] {
        let mut invalid = edit.clone();
        let c = &mut invalid["features"][0]["operation"]["profile_loft"]["start_constraint"];
        if both {
            c["direction"] = json!({"literal":{"x":{"value":0,"dimension":"scalar","unit":null},"y":{"value":0,"dimension":"scalar","unit":null},"z":{"value":1,"dimension":"scalar","unit":null}}});
        } else {
            c["normal_to_profile"] = json!(false);
        }
        let response = studio.handle("POST", "/api/features", invalid.to_string().as_bytes());
        let rejected: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(rejected["can_apply"], false, "{rejected}");
        assert!(rejected.get("gltf").is_none());
        invalid["apply"] = json!(true);
        assert_ne!(
            studio
                .handle("POST", "/api/features", invalid.to_string().as_bytes())
                .status,
            200
        );
        assert_eq!(get(&mut studio, "/api/state").1, state);
    }
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        200
    );
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    let expected: FeatureDefinition = serde_json::from_value(edit["features"][0].clone()).unwrap();
    assert_eq!(
        saved.family.features.iter().find(|f| f.id == "body"),
        Some(&expected)
    );
    assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
}

#[test]
fn loft_correspondence_checks_every_variant_and_survives_history_and_save() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/profile-loft.request.json"
    ))
    .unwrap();
    let mut doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    doc.family.parameters.push(ParameterDefinition {
        id: "anchor".into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: ParameterValue::Scalar(Quantity::length(8., LengthUnit::Millimeter)),
        minimum: None,
        maximum: None,
    });
    doc.instances.push(InstanceNode::Clone {
        id: "bad-anchor".into(),
        source: "block".into(),
        overrides: HashMap::from([(
            "anchor".into(),
            ParameterValue::Scalar(Quantity::length(9., LengthUnit::Millimeter)),
        )]),
        placement: Default::default(),
        frame: None,
        provenance: "test".into(),
    });
    let model = PathBuf::from(directory.model(&doc));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        studio
            .handle("GET", "/solid-loft-correspondence.mjs", b"")
            .status,
        200
    );
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let mut feature = data["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "body")
        .unwrap()
        .clone();
    let q = |v| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
    let op = &mut feature["operation"]["profile_loft"];
    op["ruled"] = json!(false);
    op["start_constraint"] = json!({"normal_to_profile":true,"magnitude":{"parameter":"height"}});
    op["end_constraint"] = op["start_constraint"].clone();
    op["correspondence"] = json!({"lower":{"start_point":{"components":{"x":q(0.),"y":{"parameter":"anchor"},"z":q(0.)}}},"upper":{"start_point":{"components":{"x":q(0.),"y":{"parameter":"upper_radius"},"z":{"parameter":"height"}}}}});
    let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"replace","features":[feature],"output":"body","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    let rejected: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(rejected["can_apply"], false, "{rejected}");
    assert!(rejected.get("gltf").is_none());
    edit["apply"] = json!(true);
    assert_ne!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        200
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    edit["features"][0]["operation"]["profile_loft"]["correspondence"]["lower"]["start_point"]["components"]
        ["y"] = json!({"parameter":"lower_radius"});
    edit["apply"] = json!(false);
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(preview["can_apply"], true, "{preview}");
    for id in ["block", "bad-anchor"] {
        assert!(preview["geometry"][id]["volume_mm3"].as_f64().unwrap() > 0.);
    }
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        200
    );
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    let expected: FeatureDefinition = serde_json::from_value(edit["features"][0].clone()).unwrap();
    assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        saved.family.features.iter().find(|f| f.id == "body"),
        Some(&expected)
    );
}

#[test]
fn loft_canvas_picks_snap_exact_corners_and_periodic_boundaries_without_editing_the_model() {
    let circle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/aligned-profile-loft.request.json"
    ))
    .unwrap();
    for (doc, profile, continuous) in [
        (sketch_block_document(), "profile", false),
        (
            ModelDocument::from_json(&circle["model"].to_string()).unwrap(),
            "lower",
            true,
        ),
    ] {
        let directory = Directory::new();
        let model = PathBuf::from(directory.model(&doc));
        let file = fs::read_to_string(&model).unwrap();
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let state = get(&mut studio, "/api/state").1;
        let gltf = studio.handle("GET", "/api/model.gltf", b"").body;
        for path in [
            "/sketch-studio-loft-seams.mjs",
            "/sketch-studio-loft-pick.mjs",
        ] {
            assert_eq!(studio.handle("GET", path, b"").status, 200);
        }
        let query = json!({"version":state["version"],"instance":"block","profile":profile});
        let response = studio.handle(
            "POST",
            "/api/loft-profile-pick",
            query.to_string().as_bytes(),
        );
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let source: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(source["continuous"], continuous);
        assert_eq!(
            source["vertices"].as_array().unwrap().len(),
            if continuous { 1 } else { 4 }
        );
        assert!(
            source["lines"]
                .as_array()
                .unwrap()
                .iter()
                .all(|line| line.as_array().unwrap().len() <= 257)
        );
        let anchor = if continuous {
            json!([0., 8., 0.])
        } else {
            source["vertices"][0].clone()
        };
        let mut position = anchor.clone();
        position[1] = json!(position[1].as_f64().unwrap() + 0.01);
        let mut pick = query.clone();
        pick["position_mm"] = position;
        pick["maximum_distance_mm"] = json!(0.02);
        let response = studio.handle(
            "POST",
            "/api/loft-profile-pick",
            pick.to_string().as_bytes(),
        );
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let chosen: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        for i in 0..3 {
            assert!(
                (chosen["position_mm"][i].as_f64().unwrap() - anchor[i].as_f64().unwrap()).abs()
                    < 1e-7
            );
        }
        assert!(chosen.get("vertex").is_none());
        assert!(chosen.get("edge").is_none());
        for bad in [
            "stale",
            "instance",
            "solid",
            "missing",
            "distance",
            "partial",
            "off-boundary",
        ] {
            let mut rejected = pick.clone();
            match bad {
                "stale" => rejected["version"] = json!(99999),
                "instance" => rejected["instance"] = json!("absent"),
                "solid" => rejected["profile"] = json!("body"),
                "missing" => rejected["profile"] = json!("absent"),
                "distance" => rejected["maximum_distance_mm"] = json!(-1.),
                "partial" => {
                    rejected
                        .as_object_mut()
                        .unwrap()
                        .remove("maximum_distance_mm");
                }
                _ => rejected["position_mm"] = json!([10000., 10000., 10000.]),
            }
            assert_ne!(
                studio
                    .handle(
                        "POST",
                        "/api/loft-profile-pick",
                        rejected.to_string().as_bytes()
                    )
                    .status,
                200,
                "{bad}"
            );
        }
        assert_eq!(get(&mut studio, "/api/state").1, state);
        assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, gltf);
        assert_eq!(fs::read_to_string(&model).unwrap(), file);
    }
}

#[test]
fn loft_partitions_pick_new_vertices_validate_all_variants_and_survive_save_and_history() {
    let directory = Directory::new();
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/partitioned-profile-loft.request.json"
    ))
    .unwrap();
    let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    document.instances.push(InstanceNode::Clone {
        id: "wide".into(),
        source: "block".into(),
        overrides: HashMap::from([(
            "lower_radius".into(),
            ParameterValue::Scalar(Quantity::length(10., LengthUnit::Millimeter)),
        )]),
        placement: Default::default(),
        frame: None,
        provenance: "test".into(),
    });
    let model = PathBuf::from(directory.model(&document));
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    assert_eq!(
        studio.handle("GET", "/solid-loft-splits.mjs", b"").status,
        200
    );
    let data = get(&mut studio, "/api/features").1;
    let state = get(&mut studio, "/api/state").1;
    let feature = data["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "body")
        .unwrap()
        .clone();
    let splits =
        feature["operation"]["profile_loft"]["correspondence"]["lower"]["split_points"].clone();
    let mut query = json!({"version":data["version"],"instance":"block","profile":"lower","split_points":splits});
    let response = studio.handle(
        "POST",
        "/api/loft-profile-pick",
        query.to_string().as_bytes(),
    );
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let source: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(source["continuous"], false);
    assert_eq!(source["split_count"], 3);
    assert_eq!(source["vertices"].as_array().unwrap().len(), 4);
    query["position_mm"] = json!([0., 8.001, 0.]);
    query["maximum_distance_mm"] = json!(0.01);
    let response = studio.handle(
        "POST",
        "/api/loft-profile-pick",
        query.to_string().as_bytes(),
    );
    assert_eq!(response.status, 200);
    let chosen: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert!((chosen["position_mm"][1].as_f64().unwrap() - 8.).abs() < 1e-7);
    assert_eq!(get(&mut studio, "/api/state").1, state);
    let q = |n| json!({"literal":{"value":n,"dimension":"length","unit":"millimeter"}});
    let mut invalid = feature.clone();
    invalid["operation"]["profile_loft"]["correspondence"]["lower"]["split_points"][0] =
        json!({"components":{"x":q(0.),"y":q(8.),"z":q(0.)}});
    let mut edit = json!({"version":data["version"],"instance":"block","expected":data["features"],"action":"replace","features":[invalid],"output":"body","apply":false});
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    let rejected: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(rejected["can_apply"], false, "{rejected}");
    assert!(rejected.get("gltf").is_none());
    edit["apply"] = json!(true);
    assert_ne!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        200
    );
    assert_eq!(get(&mut studio, "/api/state").1, state);
    edit["features"][0] = feature;
    for id in ["lower", "upper"] {
        edit["features"][0]["operation"]["profile_loft"]["correspondence"][id]["reverse"] =
            json!(true);
    }
    edit["apply"] = json!(false);
    let response = studio.handle("POST", "/api/features", edit.to_string().as_bytes());
    let preview: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(preview["can_apply"], true, "{preview}");
    for id in ["block", "wide"] {
        assert!(preview["geometry"][id]["volume_mm3"].as_f64().unwrap() > 0.);
    }
    edit["apply"] = json!(true);
    assert_eq!(
        studio
            .handle("POST", "/api/features", edit.to_string().as_bytes())
            .status,
        200
    );
    history(&mut studio, "undo");
    history(&mut studio, "redo");
    assert_eq!(studio.handle("POST", "/api/save", b"").status, 200);
    let saved = ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap();
    assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
    let expected: FeatureDefinition = serde_json::from_value(edit["features"][0].clone()).unwrap();
    assert_eq!(
        saved.family.features.iter().find(|f| f.id == "body"),
        Some(&expected)
    );
}

#[test]
fn loft_canvas_splits_snap_curve_interiors_and_reject_existing_vertices_without_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/partitioned-profile-loft.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let q = |n| json!({"literal":{"value":n,"dimension":"length","unit":"millimeter"}});
    let point = |x, y, z| json!({"components":{"x":q(x),"y":q(y),"z":q(z)}});
    // Includes a polygon edge and a circular arc after two exact draft partitions.
    for (profile, splits, hint, expected, corner) in [
        (
            "upper",
            json!([]),
            [2., 2.001, 20.],
            [1.9995, 2.0005, 20.],
            [4., 0., 20.],
        ),
        (
            "lower",
            json!([point(0., 8., 0.), point(-8., 0., 0.)]),
            [0., -8.001, 0.],
            [0., -8., 0.],
            [0., 8., 0.],
        ),
    ] {
        let directory = Directory::new();
        let model = PathBuf::from(directory.model(&document));
        let file = fs::read_to_string(&model).unwrap();
        let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
        let state = get(&mut studio, "/api/state").1;
        let gltf = studio.handle("GET", "/api/model.gltf", b"").body;
        let mut query = json!({"version":state["version"],"instance":"block","profile":profile,"purpose":"split","split_points":splits});
        let response = studio.handle(
            "POST",
            "/api/loft-profile-pick",
            query.to_string().as_bytes(),
        );
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let snapshot: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(snapshot["purpose"], "split");
        assert_eq!(snapshot["continuous"], true);
        assert!(
            snapshot["lines"]
                .as_array()
                .unwrap()
                .iter()
                .all(|l| l.as_array().unwrap().len() <= 257)
        );
        query["position_mm"] = json!(hint);
        query["maximum_distance_mm"] = json!(0.01);
        let response = studio.handle(
            "POST",
            "/api/loft-profile-pick",
            query.to_string().as_bytes(),
        );
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        let picked: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(picked["purpose"], "split");
        for (i, value) in expected.iter().enumerate() {
            assert!((picked["position_mm"][i].as_f64().unwrap() - value).abs() < 1e-7);
        }
        for bad in ["corner", "off-boundary", "purpose", "budget", "stale"] {
            let mut invalid = query.clone();
            match bad {
                "corner" => invalid["position_mm"] = json!(corner),
                "off-boundary" => invalid["position_mm"] = json!([10000., 10000., 10000.]),
                "purpose" => invalid["purpose"] = json!("invalid"),
                "budget" => invalid["split_points"] = json!(vec![point(0., 8., 0.); 128]),
                _ => invalid["version"] = json!(99999),
            }
            assert_ne!(
                studio
                    .handle(
                        "POST",
                        "/api/loft-profile-pick",
                        invalid.to_string().as_bytes()
                    )
                    .status,
                200,
                "{bad}"
            );
        }
        // Start picking still requires an existing corner on these multi-edge boundaries.
        query["purpose"] = json!("start");
        assert_ne!(
            studio
                .handle(
                    "POST",
                    "/api/loft-profile-pick",
                    query.to_string().as_bytes()
                )
                .status,
            200
        );
        assert_eq!(get(&mut studio, "/api/state").1, state);
        assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, gltf);
        assert_eq!(fs::read_to_string(&model).unwrap(), file);
    }
}

#[test]
fn loft_profile_diagnosis_reads_drafts_without_accepting_them_or_mutating_geometry() {
    use serde_json::Value;
    let request: Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/partitioned-profile-loft.request.json"
    ))
    .unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document));
    let file = fs::read_to_string(&model).unwrap();
    let mut studio = serve::Studio::load(&model, Some("body".into())).unwrap();
    let state = get(&mut studio, "/api/state").1;
    let gltf = studio.handle("GET", "/api/model.gltf", b"").body;
    let mut query = json!({"version":state["version"],"instance":"block","operation":document.family.features[0].operation});
    let call = |studio: &mut serve::Studio, q: &Value| {
        let response = studio.handle("POST", "/api/loft-profile-report", q.to_string().as_bytes());
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        serde_json::from_slice::<Value>(&response.body).unwrap()
    };
    let report = call(&mut studio, &query);
    assert_eq!(report["report"]["checks_passed"], true);
    assert_eq!(report["report"]["solid_checked"], false);
    assert_eq!(report["operation"], query["operation"]);
    query["operation"]["profile_loft"]["correspondence"] = json!({});
    let report = call(&mut studio, &query);
    assert_eq!(report["report"]["checks_passed"], false);
    assert_eq!(report["report"]["profiles"][0]["aligned_edges"], 1);
    assert_eq!(report["report"]["profiles"][1]["aligned_edges"], 4);
    for mode in ["version", "instance", "kind"] {
        let mut bad = query.clone();
        match mode {
            "version" => bad["version"] = json!(99999),
            "instance" => bad["instance"] = json!("absent"),
            _ => {
                bad["operation"] =
                    serde_json::to_value(&document.family.features[1].operation).unwrap()
            }
        };
        assert_ne!(
            studio
                .handle(
                    "POST",
                    "/api/loft-profile-report",
                    bad.to_string().as_bytes()
                )
                .status,
            200
        );
    }
    assert_eq!(get(&mut studio, "/api/state").1, state);
    assert_eq!(studio.handle("GET", "/api/model.gltf", b"").body, gltf);
    assert_eq!(fs::read_to_string(model).unwrap(), file);
}
