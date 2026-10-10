//! Viewer annotations for springs, helices, and threads.

use super::*;

#[test]
fn spring_viewer_follows_radius_pitch_turns_and_samples_each_turn() {
    let dir = Directory::new();
    for (name, radius, pitch, turns) in [
        ("spring", 10.0, 4.0, 5.0),
        ("edited-spring", 12.0, 6.0, 7.5),
    ] {
        let mut request = view_example("spring");
        request["model"]["instances"][0]["base"]["overrides"] = json!({
            "coil_radius":{"scalar":{"value":radius,"dimension":"length","unit":"millimeter"}},
            "pitch":{"scalar":{"value":pitch,"dimension":"length","unit":"millimeter"}},
            "turns":{"scalar":{"value":turns,"dimension":"scalar","unit":null}}
        });
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = &data["scenes"][0];
        assert_eq!(scene["valid"], true);
        let annotations = scene["annotations"].as_array().unwrap();
        let get = |id| annotations.iter().find(|a| a["id"] == id).unwrap();
        assert_eq!(get("helix-radius")["parameters"], json!(["coil_radius"]));
        assert_eq!(get("helix-pitch")["parameters"], json!(["pitch"]));
        assert_eq!(get("helix-turns")["parameters"], json!(["turns"]));
        assert_eq!(get("helix-rise")["parameters"], json!(["pitch", "turns"]));
        assert_eq!(get("helix-rise")["detail"]["value"], pitch * turns);
        let radius_anchors = get("helix-radius")["anchors"].as_array().unwrap();
        assert!((radius_anchors[1][0].as_f64().unwrap() - radius).abs() < 1e-6);
        let route = get("sweep-route-length");
        let expected = turns * (std::f64::consts::TAU * radius).hypot(pitch);
        assert!((route["detail"]["value_mm"].as_f64().unwrap() - expected).abs() < expected * 1e-6);
        let points = route["detail"]["dimension_paths"][0].as_array().unwrap();
        assert_eq!(points.len(), (turns * 32.0).ceil() as usize + 1);
        // A route segment must not shortcut across a full turn.
        for pair in points.windows(2) {
            let dx = pair[1][0].as_f64().unwrap() - pair[0][0].as_f64().unwrap();
            let dy = pair[1][1].as_f64().unwrap() - pair[0][1].as_f64().unwrap();
            assert!(dx.hypot(dy) < radius * 0.21);
        }
    }
}

#[test]
fn spring_viewer_rejects_excessive_sampling_and_obeys_global_vertex_budget() {
    let dir = Directory::new();
    let mut request = view_example("spring");
    request["sketches"] = json!(false);
    request["options"] = json!({"maximum_vertices":10});
    assert!(
        view_request(&dir, request, "limited")
            .unwrap_err()
            .message
            .contains("vertex budget")
    );
    assert!(!dir.0.join("limited").exists());
    let feature = FeatureOperation::Helix {
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        start: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        radius: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
        pitch: ScalarExpr::Literal(Quantity::length(4.0, LengthUnit::Millimeter)),
        turns: ScalarExpr::Literal(Quantity::scalar(10000.0)),
        left_handed: false,
    };
    assert!(
        view_data::helix_samples(&feature, &HashMap::new())
            .unwrap_err()
            .message
            .contains("100000-point")
    );
}

#[test]
fn standalone_helix_wire_view_has_native_lines_and_dimensions_without_a_surface_mesh() {
    let dir = Directory::new();
    let mut request = view_example("spring");
    request["model"]["family"]["features"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    request["model"]["family"]["requirements"] = json!([]);
    request["outputs"][0]["output"] = json!("coil");
    request["sketches"] = json!(false);
    view_request(&dir, request, "helix").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("helix/view.json")).unwrap()).unwrap();
    let scene = &data["scenes"][0];
    assert_eq!(scene["valid"], true);
    assert_eq!(scene["mesh"], json!([]));
    assert_eq!(scene["lines"][0].as_array().unwrap().len(), 161);
    assert!(
        scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "helix-rise" && a["detail"]["value"].as_f64() == Some(20.0))
    );
}

#[test]
fn thread_viewer_follows_nominal_dimensions_run_anchors_and_derived_turns() {
    let dir = Directory::new();
    for (name, diameter, pitch, run) in [
        ("thread", 10.0, 1.5, 8.0),
        ("edited-thread", 12.0, 2.0, 12.0),
    ] {
        let mut request = view_example("threaded-rod");
        request["model"]["instances"][0]["base"]["overrides"] = json!({"major_diameter":{"scalar":{"value":diameter,"dimension":"length","unit":"millimeter"}},"pitch":{"scalar":{"value":pitch,"dimension":"length","unit":"millimeter"}},"thread_length":{"scalar":{"value":run,"dimension":"length","unit":"millimeter"}}});
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = &data["scenes"][0];
        assert_eq!(scene["valid"], true);
        let get = |id| {
            scene["annotations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == id)
                .unwrap()
        };
        assert_eq!(get("thread-major-diameter")["detail"]["value_mm"], diameter);
        assert_eq!(
            get("thread-major-diameter")["parameters"],
            json!(["major_diameter"])
        );
        assert_eq!(get("thread-pitch")["detail"]["value_mm"], pitch);
        assert_eq!(get("thread-pitch")["parameters"], json!(["pitch"]));
        assert_eq!(get("thread-turns")["detail"]["turns"], run / pitch);
        assert_eq!(get("thread-turns")["detail"]["internal"], false);
        assert_eq!(get("thread-turns")["detail"]["left_handed"], false);
        let anchors = get("thread-run")["anchors"].as_array().unwrap();
        assert_eq!(anchors[0][2], 3.0);
        assert_eq!(anchors[1][2], 3.0 + run);
        assert_eq!(
            get("thread-run")["parameters"],
            json!(["start_margin", "thread_length"])
        );
        assert!((scene["bounds"][1][0].as_f64().unwrap() - diameter / 2.0).abs() < 1e-5);
    }
}
