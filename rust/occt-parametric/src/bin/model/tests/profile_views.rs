//! Viewer annotations for lofts, sweeps, revolves, and hollow profiles.

use super::*;

#[test]
fn saved_profile_loft_viewer_keeps_sketches_and_measures_section_spacing() {
    let dir = Directory::new();
    for height in [20.0, 30.0] {
        let mut request = view_example("profile-loft");
        let parameter = request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "height")
            .unwrap();
        parameter["default"]["scalar"]["value"] = json!(height);
        let name = format!("loft-{height}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(&name).join("view.json")).unwrap())
                .unwrap();
        let scenes = data["scenes"].as_array().unwrap();
        let solid = scenes.iter().find(|s| s["feature"] == "body").unwrap();
        let spacing = solid["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "loft-spacing-0")
            .unwrap();
        assert!((spacing["detail"]["value_mm"].as_f64().unwrap() - height).abs() < 1e-7);
        assert!(
            spacing["parameters"]
                .as_array()
                .unwrap()
                .contains(&json!("height"))
        );
        assert_eq!(
            spacing["detail"]["measurement"],
            "section_area_centroid_spacing"
        );
        assert!((spacing["anchors"][1][2].as_f64().unwrap() - height).abs() < 1e-7);
        for profile in ["lower", "upper"] {
            let sketch = scenes
                .iter()
                .find(|s| s["feature"] == profile && s["kind"] == "sketch")
                .unwrap();
            assert_eq!(sketch["solver"]["solved"], true);
        }
    }
}

#[test]
fn sweep_viewer_measures_native_route_length_and_links_path_controls() {
    let dir = Directory::new();
    for (run, bend) in [(10.0, 10.0), (15.0, 20.0)] {
        let mut request = view_example("curved-pipe");
        for parameter in request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
        {
            match parameter["id"].as_str().unwrap() {
                "run" => parameter["default"]["scalar"]["value"] = json!(run),
                "bend_radius" => parameter["default"]["scalar"]["value"] = json!(bend),
                _ => {}
            }
        }
        let name = format!("sweep-{run}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(&name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "solid")
            .unwrap();
        let route = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "sweep-route-length")
            .unwrap();
        let expected = run + bend * std::f64::consts::FRAC_PI_2;
        assert!((route["detail"]["value_mm"].as_f64().unwrap() - expected).abs() < 1e-7);
        assert_eq!(route["detail"]["measurement"], "native_edge_length_sum");
        assert_eq!(route["detail"]["driving"], false);
        assert_eq!(route["parameters"], json!(["bend_radius", "run"]));
        assert_eq!(route["anchors"].as_array().unwrap().len(), 1);
        let paths = route["detail"]["dimension_paths"].as_array().unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|p| p.as_array().unwrap().len() == 32));
        let samples = paths
            .iter()
            .flat_map(|p| p.as_array().unwrap())
            .collect::<Vec<_>>();
        assert!(samples.iter().all(|p| p[2] == 0.0));
        assert!(
            samples
                .iter()
                .any(|p| (p[0].as_f64().unwrap() - run - bend).abs() < 1e-7
                    && (p[1].as_f64().unwrap() - bend).abs() < 1e-7)
        );
        let snapshot = fs::read_to_string(dir.0.join(&name).join("view-0001.svg")).unwrap();
        assert_eq!(
            snapshot.matches("data-entity=\"route-dimension\"").count(),
            2
        );
        assert!(snapshot.contains("route length"));
    }
}

#[test]
fn sweep_route_overlay_vertices_obey_the_global_view_budget() {
    let dir = Directory::new();
    let mut request = view_example("curved-pipe");
    request["sketches"] = json!(false);
    view_request(&dir, request.clone(), "full-route").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("full-route/view.json")).unwrap())
            .unwrap();
    let scene = &data["scenes"][0];
    let geometry_vertices = scene["mesh"].as_array().unwrap().len() * 3
        + scene["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line.as_array().unwrap().len())
            .sum::<usize>();
    // The route contributes two separately sampled 32-point edges.
    request["options"] = json!({"maximum_vertices":geometry_vertices+63});
    let error = view_request(&dir, request.clone(), "short-budget").unwrap_err();
    assert!(error.message.contains("vertex"), "{error}");
    request["options"]["maximum_vertices"] = json!(geometry_vertices + 64);
    view_request(&dir, request, "exact-budget").unwrap();
}

#[test]
fn revolved_views_show_signed_angles_on_the_actual_axis() {
    let dir = Directory::new();
    for (index, angle, y_axis, wire) in [
        (0, std::f64::consts::FRAC_PI_2, false, false),
        (1, -std::f64::consts::FRAC_PI_2, true, true),
        (2, std::f64::consts::TAU, false, false),
    ] {
        let mut request = view_example("revolved-ring");
        let f = &mut request["model"]["family"];
        f["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "angle")
            .unwrap()["default"]["scalar"]["value"] = json!(angle);
        let origin = point(4.0, -3.0, 2.0);
        let direction = VectorExpr::Literal(VectorQuantity::scalars(
            0.0,
            if y_axis { 100.0 } else { 0.0 },
            if y_axis { 0.0 } else { 100.0 },
        ));
        f["features"][0]["operation"]["revolve"]["origin"] = serde_json::to_value(&origin).unwrap();
        f["features"][0]["operation"]["revolve"]["axis"] = serde_json::to_value(direction).unwrap();
        let operation = &mut f["features"][1]["operation"];
        let mut sketch = operation["sketch_face"]["sketch"].clone();
        sketch["origin"] = serde_json::to_value(origin).unwrap();
        if y_axis {
            sketch["y_axis"] =
                serde_json::to_value(VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)))
                    .unwrap();
        }
        *operation = if wire {
            json!({"sketch_wire":{"sketch":sketch}})
        } else {
            json!({"sketch_face":{"sketch":sketch}})
        };
        let name = format!("revolve-{index}");
        view_request(&dir, request.clone(), &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(&name).join("view.json")).unwrap())
                .unwrap();
        let solid = &data["scenes"][0];
        assert_eq!(solid["valid"], true);
        let dimension = solid["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "driving-revolve-angle")
            .unwrap();
        assert_eq!(dimension["parameters"], json!(["angle"]));
        assert_eq!(dimension["detail"]["value_radians"], json!(angle));
        assert!((dimension["detail"]["arc_radius_mm"].as_f64().unwrap() - 7.0).abs() < 1e-7);
        let arc = dimension["detail"]["angular_arc"].as_array().unwrap();
        assert_eq!(arc.len(), if index == 2 { 65 } else { 17 });
        let center = if y_axis {
            [4.0, 2.0, 2.0]
        } else {
            [4.0, -3.0, 7.0]
        };
        let expected_end = match index {
            0 => [4.0, 4.0, 7.0],
            1 => [4.0, 2.0, 9.0],
            _ => [11.0, -3.0, 7.0],
        };
        for (a, b) in arc
            .last()
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .zip(expected_end)
        {
            assert!((a.as_f64().unwrap() - b).abs() < 1e-7);
        }
        for sample in arc {
            let p = sample
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>();
            assert!(
                (p.iter()
                    .zip(center)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f64>()
                    .sqrt()
                    - 7.0)
                    .abs()
                    < 1e-7
            );
            assert!(
                (p[if y_axis { 1 } else { 2 }] - center[if y_axis { 1 } else { 2 }]).abs() < 1e-7
            );
        }
        let snapshot = fs::read_to_string(dir.0.join(&name).join("view-0001.svg")).unwrap();
        assert!(snapshot.contains("data-entity=\"angular-dimension\""));
        let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let session = Session::new().unwrap();
        let generated = PartInstance {
            id: "ring".into(),
            definition: &document.family,
            overrides: HashMap::new(),
            provenance: "test".into(),
        }
        .regenerate(&session)
        .unwrap();
        assert!(
            (session.volume(generated.shape("body").unwrap()).unwrap() - 140.0 * angle.abs()).abs()
                < 1e-6
        );
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn revolve_angle_arc_obeys_the_global_vertex_budget() {
    let dir = Directory::new();
    let mut request = view_example("revolved-ring");
    request["sketches"] = json!(false);
    view_request(&dir, request.clone(), "ring-full").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("ring-full/view.json")).unwrap())
            .unwrap();
    let scene = &data["scenes"][0];
    let geometry_vertices = scene["mesh"].as_array().unwrap().len() * 3
        + scene["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line.as_array().unwrap().len())
            .sum::<usize>();
    request["options"] = json!({"maximum_vertices":geometry_vertices+64});
    let error = view_request(&dir, request.clone(), "ring-short").unwrap_err();
    assert!(error.message.contains("vertex"), "{error}");
    request["options"]["maximum_vertices"] = json!(geometry_vertices + 65);
    view_request(&dir, request, "ring-exact").unwrap();
}

#[test]
fn symmetric_revolve_views_center_signed_arcs_on_the_source_plane() {
    let dir = Directory::new();
    for angle in [std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2] {
        let mut request = view_example("symmetric-revolve");
        request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "angle")
            .unwrap()["default"]["scalar"]["value"] = json!(angle);
        let name = if angle > 0.0 {
            "symmetric-positive"
        } else {
            "symmetric-negative"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let solid = &data["scenes"][0];
        let a = solid["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "driving-revolve-angle")
            .unwrap();
        assert_eq!(a["detail"]["extent"], "symmetric");
        assert_eq!(a["detail"]["start_angle_radians"], json!(-angle / 2.0));
        assert_eq!(a["detail"]["end_angle_radians"], json!(angle / 2.0));
        assert_eq!(a["parameters"], json!(["angle"]));
        let arc = a["detail"]["angular_arc"].as_array().unwrap();
        assert_eq!(arc.len(), 17);
        let first = &arc[0];
        let last = arc.last().unwrap();
        assert!((first[0].as_f64().unwrap() - 7.0 * (angle / 2.0).cos()).abs() < 1e-7);
        assert!((first[1].as_f64().unwrap() + 7.0 * (angle / 2.0).sin()).abs() < 1e-7);
        assert!((last[1].as_f64().unwrap() - 7.0 * (angle / 2.0).sin()).abs() < 1e-7);
        assert_eq!(a["anchors"][0], *first);
        assert_eq!(a["anchors"][2], *last);
        assert!(
            (solid["bounds"][0][1].as_f64().unwrap() + solid["bounds"][1][1].as_f64().unwrap())
                .abs()
                < 1e-7
        );
    }
}

#[test]
fn hollow_profile_viewer_retains_both_sketches_and_inner_radius_controls() {
    let dir = Directory::new();
    for radius in [4.0, 5.0] {
        let mut request = view_example("hollow-profile");
        request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "inner_radius")
            .unwrap()["default"]["scalar"]["value"] = json!(radius);
        let name = format!("hollow-{radius}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scenes = data["scenes"].as_array().unwrap();
        assert_eq!(scenes.len(), 3);
        assert_eq!(scenes[0]["valid"], true);
        assert!(
            scenes[0]["parameters"]
                .as_object()
                .unwrap()
                .contains_key("inner_radius")
        );
        assert!((scenes[0]["bounds"][1][2].as_f64().unwrap() - 20.0).abs() < 1e-7);
        for profile in ["outer", "inner"] {
            let sketch = scenes.iter().find(|s| s["feature"] == profile).unwrap();
            assert_eq!(sketch["solver"]["solved"], true);
        }
    }
}

#[test]
fn hollow_sweep_viewer_exposes_route_length_bore_controls_and_all_source_sketches() {
    let dir = Directory::new();
    view_request(&dir, view_example("hollow-sweep"), "hollow-sweep").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("hollow-sweep/view.json")).unwrap())
            .unwrap();
    let scenes = data["scenes"].as_array().unwrap();
    assert_eq!(scenes.len(), 4);
    let solid = &scenes[0];
    assert_eq!(solid["valid"], true);
    assert!(
        solid["parameters"]
            .as_object()
            .unwrap()
            .contains_key("inner_radius")
    );
    let route = solid["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "sweep-route-length")
        .unwrap();
    assert!(
        (route["detail"]["value_mm"].as_f64().unwrap() - (10.0 + 5.0 * std::f64::consts::PI)).abs()
            < 1e-7
    );
    for feature in ["outer", "inner", "path"] {
        assert_eq!(
            scenes.iter().find(|s| s["feature"] == feature).unwrap()["solver"]["solved"],
            true
        );
    }
}

#[test]
fn hollow_loft_viewer_keeps_hole_sketches_spacing_and_linked_bore_controls() {
    let dir = Directory::new();
    view_request(&dir, view_example("hollow-loft"), "hollow-loft").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("hollow-loft/view.json")).unwrap())
            .unwrap();
    let scenes = data["scenes"].as_array().unwrap();
    assert_eq!(scenes.len(), 5);
    let solid = &scenes[0];
    assert_eq!(solid["valid"], true);
    for parameter in ["lower_bore_radius", "upper_bore_radius"] {
        assert!(
            solid["parameters"]
                .as_object()
                .unwrap()
                .contains_key(parameter)
        );
    }
    let spacing = solid["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "loft-spacing-0")
        .unwrap();
    assert_eq!(spacing["detail"]["value_mm"], 20.0);
    for feature in ["lower", "upper", "lower-bore", "upper-bore"] {
        assert_eq!(
            scenes.iter().find(|s| s["feature"] == feature).unwrap()["solver"]["solved"],
            true
        );
    }
}
