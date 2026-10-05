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
