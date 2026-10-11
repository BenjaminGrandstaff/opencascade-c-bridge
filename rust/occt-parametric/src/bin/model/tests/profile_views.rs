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

#[test]
fn equal_radius_plate_view_links_matched_circle_controls_and_real_residuals() {
    let dir = Directory::new();
    view_request(&dir, view_example("equal-radius-plate"), "plate").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("plate/view.json")).unwrap()).unwrap();
    assert_eq!(data["scenes"].as_array().unwrap().len(), 4);
    assert_eq!(data["scenes"][0]["valid"], true);
    for scene in data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["kind"] == "sketch" && s["feature"] != "outer")
    {
        let a = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["label"] == "=R")
            .unwrap();
        assert_eq!(a["status"], "passed");
        assert_eq!(a["kind"], "constraint");
        assert!(
            a["parameters"]
                .as_array()
                .unwrap()
                .contains(&json!("hole_radius"))
        );
        assert_eq!(a["targets"], json!(["left-circle", "right-circle"]));
        assert_eq!(a["anchors"].as_array().unwrap().len(), 2);
        assert_eq!(a["detail"]["residual_unit"], "mm");
        assert!(a["detail"]["max_residual"].as_f64().unwrap() < 1e-7);
    }
}

#[test]
fn face_attached_sketch_view_reports_the_native_plane_and_keeps_local_coordinates() {
    let dir = Directory::new();
    let mut request = view_example("face-pocket");
    request["model"]["instances"][0]["base"]["overrides"]["height"] =
        json!({"scalar":{"value":30.0,"dimension":"length","unit":"millimeter"}});
    view_request(&dir, request.clone(), "pocket").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("pocket/view.json")).unwrap()).unwrap();
    let scenes = data["scenes"].as_array().unwrap();
    let sketch = scenes.iter().find(|s| s["kind"] == "sketch").unwrap();
    assert_eq!(
        sketch["face_support"]["origin_mm"],
        json!([30.0, 20.0, 30.0])
    );
    assert_eq!(sketch["face_support"]["normal"], json!([0.0, 0.0, 1.0]));
    assert_eq!(sketch["points"]["a"], json!([-10.0, -6.0, 0.0]));
    assert!(sketch["profile_error"].is_null());
    let annotation = sketch["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "sketch-face-support")
        .unwrap();
    assert_eq!(annotation["status"], "constructed");
    assert!(
        annotation["parameters"]
            .as_array()
            .unwrap()
            .contains(&json!("support_offset"))
    );
    let profile = scenes
        .iter()
        .find(|s| s["kind"] == "solid" && s["feature"] == "profile")
        .unwrap();
    assert!((profile["bounds"][0][2].as_f64().unwrap() - 30.0).abs() < 1e-7);
    let support = &mut request["model"]["family"]["features"][1]["operation"]["sketch_face"]["sketch"]
        ["face_support"];
    support["face"] = json!({"union":[support["face"].clone(),{"normal_aligned":{"direction":{"literal":{"x":{"value":0,"dimension":"scalar","unit":null},"y":{"value":0,"dimension":"scalar","unit":null},"z":{"value":-1,"dimension":"scalar","unit":null}}},"minimum_dot":{"literal":{"value":0.999999,"dimension":"scalar","unit":null}}}}]});
    view_request(&dir, request, "ambiguous").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("ambiguous/view.json")).unwrap())
            .unwrap();
    let sketch = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch")
        .unwrap();
    assert_eq!(sketch["face_support"]["status"], "unavailable");
    assert!(
        sketch["face_support"]["error"]
            .as_str()
            .unwrap()
            .contains("exactly one face")
    );
}

#[test]
fn projected_pocket_view_identifies_external_geometry_and_links_source_controls() {
    let dir = Directory::new();
    let mut request = view_example("projected-pocket");
    request["model"]["instances"][0]["base"]["overrides"]["depth"] =
        json!({"scalar":{"value":50,"dimension":"length","unit":"millimeter"}});
    view_request(&dir, request.clone(), "projected").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("projected/view.json")).unwrap())
            .unwrap();
    let scene = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch")
        .unwrap();
    assert!(scene["solver"]["solved"].as_bool().unwrap());
    assert_eq!(scene["solver"]["free_degrees"], 0);
    assert_eq!(scene["projections"][0]["definition"]["input"], "block");
    let edge = scene["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "front-edge")
        .unwrap();
    assert_eq!(edge["external"], true);
    assert!((scene["points"]["guide"][1].as_f64().unwrap() - 25.0).abs() < 1e-7);
    let a = scene["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "projection-front-edge")
        .unwrap();
    assert_eq!(a["status"], "constructed");
    assert_eq!(a["targets"], json!(["front-edge"]));
    assert!(
        a["parameters"]
            .as_array()
            .unwrap()
            .contains(&json!("depth"))
    );
    assert!(
        fs::read_to_string(dir.0.join("projected/view-0003.svg"))
            .unwrap()
            .contains("#16857a")
    );
    request["model"]["family"]["features"][1]["operation"]["sketch_face"]["sketch"]["projections"]
        [0]["edge"] = json!({"at_extreme":{"axis":"z","extremum":"maximum","tolerance":{"literal":{"value":0.000001,"dimension":"length","unit":"millimeter"}}}});
    view_request(&dir, request, "ambiguous").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("ambiguous/view.json")).unwrap())
            .unwrap();
    let scene = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch")
        .unwrap();
    assert!(
        scene["error"]
            .as_str()
            .unwrap()
            .contains("exactly one edge")
    );
}

#[test]
fn midpoint_and_concentric_viewer_annotations_use_solved_anchors_and_real_residuals() {
    let dir = Directory::new();
    for (name, relation) in [
        ("concentric-bushing", "concentric"),
        ("projected-pocket", "midpoint"),
    ] {
        view_request(&dir, view_example(name), name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], true);
        let annotation = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| !a["detail"]["constraint"][relation].is_null())
            .unwrap();
        assert_eq!(annotation["kind"], "constraint");
        assert_eq!(annotation["status"], "passed");
        assert_eq!(annotation["detail"]["residual_unit"], "mm");
        assert_eq!(annotation["detail"]["by_construction"], false);
        assert!(annotation["detail"]["max_residual"].as_f64().unwrap() < 1e-7);
        assert_eq!(annotation["anchors"].as_array().unwrap().len(), 2);
        for axis in 0..2 {
            assert!(
                (annotation["anchors"][0][axis].as_f64().unwrap()
                    - annotation["anchors"][1][axis].as_f64().unwrap())
                .abs()
                    < 1e-7
            );
        }
        if relation == "concentric" {
            assert!(
                annotation["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("center_x"))
            );
        }
    }
}

#[test]
fn point_line_dimension_anchors_follow_the_perpendicular_foot_and_link_margin() {
    let dir = Directory::new();
    for margin in [4., 6.] {
        let mut request = view_example("projected-pocket");
        let parameter = request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "margin")
            .unwrap();
        parameter["default"]["scalar"]["value"] = json!(margin);
        let name = format!("point-line-{margin}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let sketch = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(sketch["solver"]["solved"], true);
        let dimension = sketch["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| !a["detail"]["constraint"]["point_line_distance"].is_null())
            .unwrap();
        assert_eq!(dimension["kind"], "dimension");
        assert_eq!(dimension["status"], "passed");
        assert!(
            dimension["parameters"]
                .as_array()
                .unwrap()
                .contains(&json!("margin"))
        );
        assert_eq!(dimension["detail"]["residual_unit"], "mm");
        assert!(dimension["detail"]["max_residual"].as_f64().unwrap() < 1e-7);
        let anchors = &dimension["anchors"];
        assert!((anchors[0][0].as_f64().unwrap()).abs() < 1e-7);
        assert!((anchors[0][1].as_f64().unwrap() - 20.).abs() < 1e-7);
        assert!((anchors[1][0].as_f64().unwrap()).abs() < 1e-7);
        assert!((anchors[1][1].as_f64().unwrap() - (20. - margin)).abs() < 1e-7);
    }
}

#[test]
fn independent_tangency_viewer_shows_native_contacts_and_real_millimetre_residuals() {
    let dir = Directory::new();
    for radius in [3., 4.] {
        let mut request = view_example("tangent-boss");
        let parameter = request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "boss_radius")
            .unwrap();
        parameter["default"]["scalar"]["value"] = json!(radius);
        let name = format!("tangent-{radius}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let sketch = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(sketch["solver"]["solved"], true);
        for kind in ["line_circle_tangent", "circle_circle_tangent"] {
            let relation = sketch["annotations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| !a["detail"]["constraint"][kind].is_null())
                .unwrap();
            assert_eq!(relation["kind"], "constraint");
            assert_eq!(relation["status"], "passed");
            assert_eq!(relation["detail"]["residual_unit"], "mm");
            assert_eq!(relation["detail"]["by_construction"], false);
            assert!(relation["detail"]["max_residual"].as_f64().unwrap() < 1e-7);
            for axis in 0..2 {
                assert!(
                    (relation["anchors"][0][axis].as_f64().unwrap()
                        - relation["anchors"][1][axis].as_f64().unwrap())
                    .abs()
                        < 1e-7
                );
            }
            assert!(
                relation["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("reference_radius"))
            );
        }
        let c = &sketch["points"]["boss-center"];
        assert!((c[0].as_f64().unwrap() - (20_f64 * radius).sqrt()).abs() < 1e-7);
        assert!((c[1].as_f64().unwrap() - (radius - 5.)).abs() < 1e-7);
    }
}

#[test]
fn internal_circle_tangency_viewer_contacts_agree_and_native_profile_uses_solved_center() {
    let dir = Directory::new();
    let mut request = view_example("tangent-boss");
    let s = &mut request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"];
    s["lines"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"centers","start":"reference-center","end":"boss-center"}));
    s["constraints"][0] = json!({"horizontal":{"line":"centers"}});
    s["constraints"][1]["circle_circle_tangent"]["mode"] = json!("internal");
    s["points"][4]["x"]["literal"]["value"] = json!(2.);
    s["points"][4]["y"]["literal"]["value"] = json!(0.);
    s["points"][5]["x"]["literal"]["value"] = json!(5.);
    s["points"][5]["y"]["literal"]["value"] = json!(0.);
    view_request(&dir, request, "internal").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("internal/view.json")).unwrap())
            .unwrap();
    let sketch = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch")
        .unwrap();
    assert_eq!(sketch["solver"]["solved"], true);
    assert!((sketch["points"]["boss-center"][0].as_f64().unwrap() - 2.).abs() < 1e-7);
    let relation = sketch["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| !a["detail"]["constraint"]["circle_circle_tangent"].is_null())
        .unwrap();
    assert_eq!(relation["label"], "T INT");
    assert_eq!(relation["status"], "passed");
    for point in relation["anchors"].as_array().unwrap() {
        assert!((point[0].as_f64().unwrap() - 5.).abs() < 1e-7);
        assert!(point[1].as_f64().unwrap().abs() < 1e-7);
    }
    assert!(
        data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["kind"] == "solid")
            .all(|s| s["valid"] == true)
    );
}

#[test]
fn arc_tangency_viewer_reports_span_failure_even_when_supporting_circle_contacts_coincide() {
    let dir = Directory::new();
    for excluded in [false, true] {
        let mut request = view_example("arc-tangent-boss");
        let sketch =
            &mut request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"];
        let x = 60_f64.sqrt();
        for p in sketch["points"].as_array_mut().unwrap() {
            let coords = match p["id"].as_str().unwrap() {
                "boss-center" => Some((x, -2.)),
                "boss-start" => Some((x, 1.)),
                "boss-end" => Some((x + 3., -2.)),
                _ => None,
            };
            if let Some((px, py)) = coords {
                p["x"]["literal"]["value"] = json!(px);
                p["y"]["literal"]["value"] = json!(py);
                p["fixed"] = json!(true);
            }
        }
        sketch["arcs"][0]["clockwise"] = json!(!excluded);
        let name = if excluded {
            "excluded-arc"
        } else {
            "valid-arc"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], !excluded);
        let relation = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| !a["detail"]["constraint"]["circle_circle_tangent"].is_null())
            .unwrap();
        assert_eq!(
            relation["status"],
            if excluded { "failed" } else { "passed" }
        );
        assert_eq!(relation["detail"]["residual_unit"], "mm");
        let residual = relation["detail"]["max_residual"].as_f64().unwrap();
        assert!(if excluded {
            residual > 1.
        } else {
            residual < 1e-7
        });
        for axis in 0..2 {
            assert!(
                (relation["anchors"][0][axis].as_f64().unwrap()
                    - relation["anchors"][1][axis].as_f64().unwrap())
                .abs()
                    < 1e-7
            );
        }
        assert!(
            scene["entities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == "boss-arc")
        );
    }
}

#[test]
fn explicit_bspline_viewer_exposes_basis_control_polygon_and_weight_parameter() {
    let dir = Directory::new();
    for weight in [0.5_f64.sqrt(), 1.] {
        let mut request = view_example("rational-spline-cap");
        let p = request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "middle_weight")
            .unwrap();
        p["default"]["scalar"]["value"] = json!(weight);
        let name = format!("basis-{weight}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(&name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], true);
        let curve = scene["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "curve")
            .unwrap();
        assert_eq!(curve["bspline"]["basis"]["degree"], 2);
        assert_eq!(curve["bspline"]["basis"]["knots"], json!([0., 1.]));
        assert_eq!(curve["bspline"]["control_points"], json!(["a", "b", "c"]));
        assert!(
            (curve["bspline"]["evaluated_weights"][1].as_f64().unwrap() - weight).abs() < 1e-12
        );
        let annotation = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "bspline-curve")
            .unwrap();
        assert!(
            annotation["parameters"]
                .as_array()
                .unwrap()
                .contains(&json!("middle_weight"))
        );
        assert_eq!(annotation["detail"]["periodic"], false);
        let svg = fs::read_to_string(dir.0.join(name).join("view-0003.svg")).unwrap();
        assert!(svg.contains("stroke-dasharray=\"5 4\""));
    }
}

#[test]
fn periodic_bspline_viewer_exposes_closed_control_polygon_and_live_weight_metadata() {
    let dir = Directory::new();
    for weight in [1., 2.] {
        let mut request = view_example("periodic-spline-pad");
        let p = request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "corner_weight")
            .unwrap();
        p["default"]["scalar"]["value"] = json!(weight);
        let name = format!("periodic-{weight}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(&name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], true);
        let curve = scene["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "curve")
            .unwrap();
        assert_eq!(curve["bspline"]["basis"]["periodic"], true);
        assert_eq!(
            curve["bspline"]["control_points"],
            json!(["a", "b", "c", "d"])
        );
        assert_eq!(curve["bspline"]["evaluated_weights"][0], weight);
        let annotation = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "bspline-curve")
            .unwrap();
        assert_eq!(annotation["detail"]["periodic"], true);
        assert!(
            annotation["parameters"]
                .as_array()
                .unwrap()
                .contains(&json!("corner_weight"))
        );
        let points = curve["points"].as_array().unwrap();
        for i in 0..3 {
            assert!(
                (points[0][i].as_f64().unwrap() - points.last().unwrap()[i].as_f64().unwrap())
                    .abs()
                    < 1e-7
            );
        }
        assert!(scene["profile_error"].is_null());
        let svg = fs::read_to_string(dir.0.join(name).join("view-0003.svg")).unwrap();
        assert!(svg.contains("stroke-dasharray=\"5 4\""));
    }
}

#[test]
fn non_clamped_viewer_exposes_measured_endpoint_links_and_reports_fixed_conflicts() {
    let dir = Directory::new();
    for conflict in [false, true] {
        let mut request = view_example("non-clamped-spline-cap");
        if conflict {
            request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]["points"]
                [3]["fixed"] = json!(true);
        }
        let name = if conflict {
            "endpoint-conflict"
        } else {
            "non-clamped"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], !conflict);
        let curve = scene["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "curve")
            .unwrap();
        assert_eq!(curve["bspline"]["basis"]["non_clamped"], true);
        assert_eq!(
            curve["bspline"]["endpoints"],
            json!({"start":"start","end":"end"})
        );
        assert!(
            curve["point_ids"]
                .as_array()
                .unwrap()
                .contains(&json!("start"))
        );
        let link = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "bspline-endpoints-curve")
            .unwrap();
        assert_eq!(link["status"], if conflict { "failed" } else { "passed" });
        assert_eq!(link["detail"]["by_construction"], false);
        assert_eq!(link["detail"]["residual_unit"], "mm");
        assert_eq!(link["anchors"].as_array().unwrap().len(), 4);
        if !conflict {
            assert!(link["detail"]["max_residual"].as_f64().unwrap() < 1e-9);
            assert!((scene["points"]["start"][0].as_f64().unwrap() - 5.).abs() < 1e-7);
            assert!((scene["points"]["end"][0].as_f64().unwrap() - 15.).abs() < 1e-7);
        } else {
            assert!(link["detail"]["max_residual"].as_f64().unwrap() > 0.5);
        }
    }
}

#[test]
fn ellipse_tangent_viewer_shows_actual_rotated_contact_linked_axes_and_fixed_conflicts() {
    let dir = Directory::new();
    for conflict in [false, true] {
        let mut request = view_example("ellipse-tangent-pad");
        if conflict {
            for p in request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]
                ["points"]
                .as_array_mut()
                .unwrap()
            {
                p["fixed"] = json!(true);
            }
        }
        let name = if conflict { "conflict" } else { "solved" };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], !conflict);
        let relation = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| !a["detail"]["constraint"]["line_ellipse_tangent"].is_null())
            .unwrap();
        assert_eq!(
            relation["status"],
            if conflict { "failed" } else { "passed" }
        );
        assert_eq!(relation["detail"]["by_construction"], false);
        assert_eq!(relation["detail"]["residual_unit"], "mm");
        assert_eq!(relation["anchors"].as_array().unwrap().len(), 2);
        let anchors = &relation["anchors"];
        let gap = (anchors[0][1].as_f64().unwrap() - anchors[1][1].as_f64().unwrap()).abs();
        if conflict {
            assert!(gap > 0.2);
        } else {
            assert!(gap < 1e-9);
            assert_eq!(scene["solver"]["free_degrees"], 0);
            // The contact of this rotated ellipse is not below its centre.
            assert!(anchors[1][0].as_f64().unwrap().abs() > 1.);
            let parameters = relation["parameters"].as_array().unwrap();
            for id in ["semi_major", "semi_minor", "tilt"] {
                assert!(parameters.contains(&json!(id)), "{parameters:?}");
            }
        }
        let snapshot = fs::read_to_string(dir.0.join(name).join("view-0003.svg")).unwrap();
        assert!(snapshot.contains("T LEFT"));
    }
}

#[test]
fn independent_spline_contact_viewer_exposes_position_tangent_units_controls_and_conflicts() {
    let dir = Directory::new();
    for conflict in [false, true] {
        let mut request = view_example("spline-tangent-cap");
        if conflict {
            for p in request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]
                ["points"]
                .as_array_mut()
                .unwrap()
            {
                p["fixed"] = json!(true);
            }
        }
        let name = if conflict {
            "spline-conflict"
        } else {
            "spline-solved"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], !conflict);
        let relation = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| !a["detail"]["constraint"]["line_spline_tangent"].is_null())
            .unwrap();
        assert_eq!(
            relation["status"],
            if conflict { "failed" } else { "passed" }
        );
        assert_eq!(relation["detail"]["by_construction"], false);
        assert_eq!(relation["detail"]["residual_unit"], "mixed");
        assert_eq!(
            relation["detail"]["contact_residuals"]["residual_units"],
            json!(["mm", "mm", "mm", "dimensionless"])
        );
        assert_eq!(relation["anchors"].as_array().unwrap().len(), 3);
        let residual = relation["detail"]["contact_residuals"]["position_residual_mm"]
            .as_f64()
            .unwrap();
        if conflict {
            assert!(residual > 0.1);
        } else {
            assert!(residual < 1e-9);
            assert_eq!(scene["solver"]["free_degrees"], 0);
            assert!(
                relation["detail"]["contact_residuals"]["tangent_sine"]
                    .as_f64()
                    .unwrap()
                    .abs()
                    < 1e-9
            );
            assert_eq!(
                relation["parameters"],
                json!(["middle_weight", "rise", "span"])
            );
            for p in relation["anchors"].as_array().unwrap() {
                assert!(
                    p[0].as_f64().unwrap().abs() < 1e-7
                        && (p[1].as_f64().unwrap() - 5.).abs() < 1e-7
                );
            }
        }
        let snapshot = fs::read_to_string(dir.0.join(name).join("view-0003.svg")).unwrap();
        assert!(snapshot.contains("T SPLINE"));
    }
}

#[test]
fn spline_circle_viewer_reports_native_contacts_arc_domains_and_linked_controls() {
    let dir = Directory::new();
    for mode in 0..3 {
        let mut request = view_example("spline-tangent-boss");
        let sketch =
            &mut request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"];
        if mode > 0 {
            // Fix an exact contact, then exclude it with the directed arc domain.
            let lit = |v| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
            for p in sketch["points"].as_array_mut().unwrap() {
                p["fixed"] = json!(true);
                if p["id"] == "contact" {
                    p["x"] = lit(0.);
                    p["y"] = lit(5.);
                }
            }
            if mode == 2 {
                sketch["points"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"left-rim","x":lit(-3.),"y":lit(8.),"fixed":true}));
                sketch["circles"] = json!([]);
                sketch["arcs"] = json!([{"id":"boss","center":"boss-center","start":"boss-rim","end":"left-rim","clockwise":false}]);
                sketch["profile"] = json!([]);
                request["outputs"] = json!([]);
            } else {
                // Move the fixed circle while preserving its radius.
                for p in sketch["points"].as_array_mut().unwrap() {
                    if p["id"] == "boss-center" || p["id"] == "boss-rim" {
                        p["y"] = lit(9.);
                    }
                }
            }
        }
        let name = format!("spline-circle-{mode}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(&name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], mode == 0);
        let relation = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| !a["detail"]["constraint"]["spline_circle_tangent"].is_null())
            .unwrap();
        assert_eq!(
            relation["status"],
            if mode == 0 { "passed" } else { "failed" }
        );
        assert_eq!(relation["detail"]["by_construction"], false);
        assert_eq!(relation["detail"]["residual_unit"], "mixed");
        let residuals = &relation["detail"]["contact_residuals"];
        if mode == 2 {
            assert!(residuals["position_residual_mm"].as_f64().unwrap() < 1e-9);
            assert!(residuals["tangent_sine"].as_f64().unwrap().abs() < 1e-9);
            assert!(residuals["arc_span_residual_mm"].as_f64().unwrap() > 4.);
            assert_eq!(
                residuals["residual_units"],
                json!(["mm", "mm", "mm", "dimensionless", "mm"])
            );
        } else if mode == 1 {
            assert!(residuals["position_residual_mm"].as_f64().unwrap() > 0.9);
        } else {
            assert_eq!(scene["solver"]["free_degrees"], 0);
            assert_eq!(
                relation["parameters"],
                json!(["boss_radius", "middle_weight", "rise", "span"])
            );
            assert!(residuals["position_residual_mm"].as_f64().unwrap() < 1e-9);
            assert!(residuals["arc_span_residual_mm"].is_null());
            for p in relation["anchors"].as_array().unwrap() {
                assert!(
                    p[0].as_f64().unwrap().abs() < 1e-7
                        && (p[1].as_f64().unwrap() - 5.).abs() < 1e-7
                );
            }
        }
    }
}

#[test]
fn spline_pair_viewer_exposes_both_native_contacts_tangent_units_and_controls() {
    let dir = Directory::new();
    for conflict in [false, true] {
        let mut request = view_example("spline-tangent-pair");
        if conflict {
            for p in request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]
                ["points"]
                .as_array_mut()
                .unwrap()
            {
                p["fixed"] = json!(true);
            }
        }
        let name = if conflict {
            "pair-conflict"
        } else {
            "pair-solved"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], !conflict);
        let a = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| !a["detail"]["constraint"]["spline_spline_tangent"].is_null())
            .unwrap();
        assert_eq!(a["status"], if conflict { "failed" } else { "passed" });
        assert_eq!(a["detail"]["by_construction"], false);
        assert_eq!(a["detail"]["residual_unit"], "mixed");
        assert_eq!(
            a["detail"]["contact_residuals"]["residual_units"],
            json!(["mm", "mm", "mm", "mm", "dimensionless"])
        );
        assert_eq!(a["anchors"].as_array().unwrap().len(), 3);
        let r = &a["detail"]["contact_residuals"];
        if conflict {
            assert!(r["position_residual_mm"].as_f64().unwrap() > 0.1);
        } else {
            assert_eq!(scene["solver"]["free_degrees"], 0);
            assert!(
                r["position_residual_mm"].as_f64().unwrap() < 1e-9
                    && r["tangent_sine"].as_f64().unwrap().abs() < 1e-9
            );
            assert_eq!(
                a["parameters"],
                json!(["lower_rise", "middle_weight", "span", "upper_rise"])
            );
            for p in a["anchors"].as_array().unwrap() {
                assert!(
                    p[0].as_f64().unwrap().abs() < 1e-7
                        && (p[1].as_f64().unwrap() - 5.).abs() < 1e-7
                );
            }
        }
        assert!(
            fs::read_to_string(dir.0.join(name).join("view-0003.svg"))
                .unwrap()
                .contains("T SPLINES")
        );
    }
}

#[test]
fn spline_ellipse_viewer_exposes_both_native_contacts_tangent_units_and_controls() {
    let dir = Directory::new();
    for conflict in [false, true] {
        let mut request = view_example("spline-tangent-ellipse");
        if conflict {
            for p in request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]
                ["points"]
                .as_array_mut()
                .unwrap()
            {
                p["fixed"] = json!(true);
            }
        }
        let name = if conflict {
            "ellipse-contact-conflict"
        } else {
            "ellipse-contact-solved"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], !conflict);
        let a = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| !a["detail"]["constraint"]["spline_ellipse_tangent"].is_null())
            .unwrap();
        assert_eq!(a["status"], if conflict { "failed" } else { "passed" });
        assert_eq!(a["detail"]["by_construction"], false);
        assert_eq!(a["detail"]["residual_unit"], "mixed");
        assert_eq!(
            a["detail"]["contact_residuals"]["residual_units"],
            json!(["mm", "mm", "mm", "mm", "dimensionless"])
        );
        assert_eq!(a["anchors"].as_array().unwrap().len(), 3);
        let r = &a["detail"]["contact_residuals"];
        if conflict {
            assert!(r["position_residual_mm"].as_f64().unwrap() > 0.1);
        } else {
            assert_eq!(scene["solver"]["free_degrees"], 0);
            assert!(
                r["position_residual_mm"].as_f64().unwrap() < 1e-9
                    && r["tangent_sine"].as_f64().unwrap().abs() < 1e-9
            );
            assert_eq!(
                a["parameters"],
                json!([
                    "middle_weight",
                    "rise",
                    "semi_major",
                    "semi_minor",
                    "span",
                    "tilt"
                ])
            );
            for p in a["anchors"].as_array().unwrap() {
                assert!(
                    p[0].as_f64().unwrap().abs() < 1e-7
                        && (p[1].as_f64().unwrap() - 5.).abs() < 1e-7
                );
            }
        }
        assert!(
            fs::read_to_string(dir.0.join(name).join("view-0003.svg"))
                .unwrap()
                .contains("T SPLINE ELLIPSE")
        );
    }
}

#[test]
fn ellipse_circle_viewer_exposes_both_native_contacts_tangent_units_and_controls() {
    let dir = Directory::new();
    for conflict in [false, true] {
        let mut request = view_example("ellipse-tangent-boss");
        if conflict {
            for p in request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]
                ["points"]
                .as_array_mut()
                .unwrap()
            {
                p["fixed"] = json!(true);
            }
        }
        let name = if conflict {
            "ellipse-contact-conflict"
        } else {
            "ellipse-contact-solved"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], !conflict);
        let a = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| !a["detail"]["constraint"]["ellipse_circle_tangent"].is_null())
            .unwrap();
        assert_eq!(a["status"], if conflict { "failed" } else { "passed" });
        assert_eq!(a["detail"]["by_construction"], false);
        assert_eq!(a["detail"]["residual_unit"], "mixed");
        assert_eq!(
            a["detail"]["contact_residuals"]["residual_units"],
            json!(["mm", "mm", "mm", "dimensionless"])
        );
        assert_eq!(a["anchors"].as_array().unwrap().len(), 3);
        let r = &a["detail"]["contact_residuals"];
        if conflict {
            assert!(r["position_residual_mm"].as_f64().unwrap() > 0.1);
        } else {
            assert_eq!(scene["solver"]["free_degrees"], 0);
            assert!(
                r["position_residual_mm"].as_f64().unwrap() < 1e-9
                    && r["tangent_sine"].as_f64().unwrap().abs() < 1e-9
            );
            assert_eq!(
                a["parameters"],
                json!(["boss_radius", "semi_major", "semi_minor", "tilt"])
            );
            for p in a["anchors"].as_array().unwrap() {
                assert!(
                    p[0].as_f64().unwrap().abs() < 1e-7
                        && (p[1].as_f64().unwrap() - 3.).abs() < 1e-7
                );
            }
        }
        assert!(
            fs::read_to_string(dir.0.join(name).join("view-0003.svg"))
                .unwrap()
                .contains("T ELLIPSE CIRCLE")
        );
    }
}

#[test]
fn ellipse_pair_viewer_exposes_both_native_contacts_tangent_units_and_controls() {
    let dir = Directory::new();
    for conflict in [false, true] {
        let mut request = view_example("ellipse-tangent-pair");
        if conflict {
            for p in request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]
                ["points"]
                .as_array_mut()
                .unwrap()
            {
                p["fixed"] = json!(true);
            }
        }
        let name = if conflict {
            "pair-conflict"
        } else {
            "pair-solved"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], !conflict);
        let a = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| !a["detail"]["constraint"]["ellipse_ellipse_tangent"].is_null())
            .unwrap();
        assert_eq!(a["status"], if conflict { "failed" } else { "passed" });
        assert_eq!(a["detail"]["by_construction"], false);
        assert_eq!(a["detail"]["residual_unit"], "mixed");
        assert_eq!(
            a["detail"]["contact_residuals"]["residual_units"],
            json!(["mm", "mm", "mm", "mm", "dimensionless"])
        );
        assert_eq!(a["anchors"].as_array().unwrap().len(), 3);
        let r = &a["detail"]["contact_residuals"];
        if conflict {
            assert!(r["position_residual_mm"].as_f64().unwrap() > 0.1);
        } else {
            assert_eq!(scene["solver"]["free_degrees"], 0);
            assert!(
                r["position_residual_mm"].as_f64().unwrap() < 1e-9
                    && r["tangent_sine"].as_f64().unwrap().abs() < 1e-9
            );
            assert_eq!(
                a["parameters"],
                json!([
                    "semi_major",
                    "semi_minor",
                    "tilt",
                    "upper_major",
                    "upper_minor",
                    "upper_tilt"
                ])
            );
            for p in a["anchors"].as_array().unwrap() {
                assert!(
                    p[0].as_f64().unwrap().abs() < 1e-7
                        && (p[1].as_f64().unwrap() - 3.).abs() < 1e-7
                );
            }
        }
        assert!(
            fs::read_to_string(dir.0.join(name).join("view-0003.svg"))
                .unwrap()
                .contains("T ELLIPSES")
        );
    }
}

#[test]
fn corner_profile_viewer_shows_sizes_named_corners_derived_boundary_and_failures() {
    let dir = Directory::new();
    for oversized in [false, true] {
        let mut request = view_example("corner-treated-plate");
        if oversized {
            request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]["profile_operations"]
                [0]["fillet"]["radius"] =
                json!({"literal":{"value":100,"dimension":"length","unit":"millimeter"}});
        }
        let name = if oversized {
            "corners-oversized"
        } else {
            "corners-valid"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], true);
        let operations = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["kind"] == "profile_operation")
            .collect::<Vec<_>>();
        assert_eq!(operations.len(), 2);
        for a in &operations {
            assert_eq!(a["anchors"].as_array().unwrap().len(), 1);
            assert_eq!(
                a["status"],
                if oversized { "unverified" } else { "driving" }
            );
            assert_eq!(a["detail"]["error"].is_null(), !oversized);
        }
        if !oversized {
            assert!(
                operations[0]["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("fillet_radius"))
            );
            assert!(
                operations[1]["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("chamfer_incoming"))
            );
            assert!(
                operations[1]["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("chamfer_outgoing"))
            );
            assert_eq!(scene["edited_profile"].as_array().unwrap().len(), 6);
            assert_eq!(operations[0]["targets"], json!(["C"]));
            assert_eq!(operations[1]["targets"], json!(["A"]));
        }
        let svg = fs::read_to_string(dir.0.join(name).join("view-0003.svg")).unwrap();
        assert!(svg.contains("Fillet R") && svg.contains("Chamfer"));
    }
}

#[test]
fn open_corner_viewer_shows_sizes_named_corners_derived_boundary_and_failures() {
    let dir = Directory::new();
    for oversized in [false, true] {
        let mut request = view_example("open-corner-pipe");
        if oversized {
            request["model"]["family"]["features"][2]["operation"]["sketch_open_wire"]["sketch"]
                ["profile_operations"][0]["fillet"]["radius"] =
                json!({"literal":{"value":100,"dimension":"length","unit":"millimeter"}});
        }
        let name = if oversized {
            "corners-oversized"
        } else {
            "corners-valid"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch" && s["feature"] == "path")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], true);
        let operations = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["kind"] == "profile_operation")
            .collect::<Vec<_>>();
        assert_eq!(operations.len(), 2);
        for a in &operations {
            assert_eq!(a["anchors"].as_array().unwrap().len(), 1);
            assert_eq!(
                a["status"],
                if oversized { "unverified" } else { "driving" }
            );
            assert_eq!(a["detail"]["error"].is_null(), !oversized);
        }
        if !oversized {
            assert!(
                operations[0]["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("fillet_radius"))
            );
            assert!(
                operations[1]["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("chamfer_incoming"))
            );
            assert!(
                operations[1]["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("chamfer_outgoing"))
            );
            assert_eq!(scene["edited_profile"].as_array().unwrap().len(), 5);
            assert_eq!(operations[0]["targets"], json!(["B"]));
            assert_eq!(operations[1]["targets"], json!(["C"]));
        }
        let svg = fs::read_to_string(dir.0.join(name).join("view-0004.svg")).unwrap();
        assert!(svg.contains("Fillet R") && svg.contains("Chamfer"));
    }
}

#[test]
fn spline_corner_viewer_shows_native_edits_at_both_spline_ends_and_failures() {
    let dir = Directory::new();
    for oversized in [false, true] {
        let mut request = view_example("spline-corner-pad");
        if oversized {
            request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"]["profile_operations"]
                [0]["fillet"]["radius"] =
                json!({"literal":{"value":100,"dimension":"length","unit":"millimeter"}});
        }
        let name = if oversized {
            "spline-corners-oversized"
        } else {
            "spline-corners-valid"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], true);
        let operations = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["kind"] == "profile_operation")
            .collect::<Vec<_>>();
        assert_eq!(operations.len(), 2);
        assert_eq!(operations[0]["targets"], json!(["C"]));
        assert_eq!(operations[1]["targets"], json!(["D"]));
        for a in &operations {
            assert_eq!(a["anchors"].as_array().unwrap().len(), 1);
            assert_eq!(
                a["status"],
                if oversized { "unverified" } else { "driving" }
            );
            assert_eq!(a["detail"]["error"].is_null(), !oversized);
        }
        if !oversized {
            // Four source edges plus one fillet arc and one chamfer chord.
            assert_eq!(scene["edited_profile"].as_array().unwrap().len(), 6);
        }
        let svg = fs::read_to_string(dir.0.join(name).join("view-0003.svg")).unwrap();
        assert!(svg.contains("Fillet R") && svg.contains("Chamfer"));
    }
}

#[test]
fn pattern_viewer_draws_linked_copies_holes_and_pattern_groups() {
    let dir = Directory::new();
    view_request(&dir, view_example("mirrored-flange"), "flange").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("flange").join("view.json")).unwrap())
            .unwrap();
    let scene = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
        .unwrap();
    assert_eq!(scene["solver"]["solved"], true);
    let ids = scene["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    for id in [
        "m:1:AB",
        "m:1:BC",
        "m:1:crown",
        "bolts:1:bolt",
        "bolts:5:bolt",
    ] {
        assert!(ids.contains(&id), "{id} missing from {ids:?}");
    }
    assert_eq!(scene["holes"].as_array().unwrap().len(), 7);
    let groups = scene["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["id"].as_str().unwrap().starts_with("pattern-"))
        .collect::<Vec<_>>();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["label"], "Mirror m about axis");
    assert_eq!(groups[1]["label"], "Circular bolts × 6 @ 60.000°");
    assert_eq!(groups[1]["detail"]["copies"].as_array().unwrap().len(), 5);
    assert!(
        groups
            .iter()
            .all(|g| g["anchors"].as_array().unwrap().len() == 1)
    );
    // Mirrored points sit at the reflection of their sources.
    let points = &scene["points"];
    assert!((points["m:1:B"][0].as_f64().unwrap() + points["B"][0].as_f64().unwrap()).abs() < 1e-8);
    let solid = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "solid")
        .unwrap();
    assert_eq!(solid["valid"], true);
}

#[test]
fn sketch_diagnosis_viewer_links_conflicts_redundancy_and_coupled_free_motion() {
    let dir = Directory::new();
    for conflict in [false, true] {
        let mut request = view_example("sketch-diagnosis");
        if conflict {
            request["model"]["instances"][0]["base"]["overrides"]["extra_width"] =
                json!({"scalar":{"value":5,"dimension":"length","unit":"millimeter"}});
        }
        let name = if conflict {
            "diagnosis-conflict"
        } else {
            "diagnosis-free"
        };
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        let d = &scene["solver"]["diagnosis"];
        assert_eq!(d["complete"], true);
        assert_eq!(scene["solver"]["solved"], !conflict);
        assert_eq!(d["free_coordinates"].as_array().unwrap().len(), 1);
        let motion = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "free-motion-0")
            .unwrap();
        assert_eq!(motion["kind"], "constraint");
        assert_eq!(motion["anchors"].as_array().unwrap().len(), 2);
        assert_eq!(motion["targets"], json!(["D"]));
        assert!(
            d["locally_redundant_constraints"]
                .as_array()
                .unwrap()
                .contains(&json!(5))
        );
        if conflict {
            let group = scene["annotations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == "conflict-group-0")
                .unwrap();
            assert_eq!(group["status"], "failed");
            assert!(group["targets"].as_array().unwrap().contains(&json!("B")));
            assert!(
                group["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("extra_width"))
            );
            assert!(group["anchors"].as_array().unwrap().len() >= 2);
        } else {
            assert!(d["conflict_groups"].as_array().unwrap().is_empty());
        }
    }
}

#[test]
fn strict_sketch_acceptance_rejects_geometry_without_hiding_solved_diagnostics() {
    let dir = Directory::new();
    for lift in [4, 0] {
        let mut request = view_example("strict-sketch-block");
        request["model"]["instances"][0]["base"]["overrides"]["lift"] =
            json!({"scalar":{"value":lift,"dimension":"length","unit":"millimeter"}});
        let name = format!("strict-view-{lift}");
        view_request(&dir, request.clone(), &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(scene["solver"]["solved"], true);
        assert_eq!(scene["solver"]["accepted"], lift != 0);
        assert_eq!(
            scene["solver"]["free_degrees"],
            if lift == 0 { 1 } else { 0 }
        );
        assert_eq!(scene["acceptance"], "fully_constrained");
        let policy = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "acceptance-policy")
            .unwrap();
        assert_eq!(
            policy["status"],
            if lift == 0 { "failed" } else { "passed" }
        );
        if lift == 0 {
            assert!(
                scene["acceptance_error"]
                    .as_str()
                    .unwrap()
                    .contains("1 free degrees")
            );
        }
        request["schema"] = json!("occb-model-request-v1");
        request.as_object_mut().unwrap().remove("sketches");
        request["step"] = json!(false);
        request["stl"] = json!(false);
        request["preview"] = json!(false);
        let name = format!("strict-build-{lift}");
        if lift == 0 {
            assert_eq!(
                invoke(&dir, request, &name).unwrap_err().stage,
                "regeneration"
            );
            assert!(!dir.0.join(name).exists());
        } else {
            assert_eq!(invoke(&dir, request, &name).unwrap()["status"], "built");
        }
    }
}

#[test]
fn spline_projection_viewer_exposes_exact_basis_source_links_and_fixed_geometry() {
    let dir = Directory::new();
    view_request(
        &dir,
        view_example("projected-spline-pad"),
        "projected-spline",
    )
    .unwrap();
    let data: Value = serde_json::from_str(
        &fs::read_to_string(dir.0.join("projected-spline/view.json")).unwrap(),
    )
    .unwrap();
    let scene = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
        .unwrap();
    assert_eq!(scene["solver"]["accepted"], true);
    assert_eq!(scene["solver"]["free_degrees"], 0);
    let entity = scene["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "reference")
        .unwrap();
    assert_eq!(entity["external"], true);
    assert_eq!(entity["bspline"]["basis"]["degree"], 2);
    assert!(
        entity["bspline"]["control_points"]
            .as_array()
            .unwrap()
            .len()
            > 5
    );
    let link = scene["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "projection-reference")
        .unwrap();
    assert_eq!(link["detail"]["definition"]["kind"], "spline");
    assert!(
        link["parameters"]
            .as_array()
            .unwrap()
            .contains(&json!("weight"))
    );
    assert_eq!(link["targets"], json!(["reference"]));
    assert_eq!(
        data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "solid" && s["feature"] == "body")
            .unwrap()["valid"],
        true
    );
}

#[test]
fn chain_projection_viewer_links_the_complete_boundary_to_source_controls() {
    let dir = Directory::new();
    view_request(&dir, view_example("projected-chain-pad"), "chain").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("chain/view.json")).unwrap()).unwrap();
    let sk = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
        .unwrap();
    assert_eq!(sk["solver"]["accepted"], true);
    assert_eq!(sk["entities"].as_array().unwrap().len(), 1);
    let curve = &sk["entities"][0];
    assert_eq!(curve["external"], true);
    let points = curve["bspline"]["control_points"].as_array().unwrap();
    assert_eq!(points.first(), points.last());
    assert!(points.len() > 7);
    let projection = sk["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "projection-reference")
        .unwrap();
    assert_eq!(projection["detail"]["definition"]["kind"], "chain");
    assert!(
        projection["parameters"]
            .as_array()
            .unwrap()
            .contains(&json!("weight"))
    );
    assert_eq!(
        data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "solid" && s["feature"] == "body")
            .unwrap()["valid"],
        true
    );
}

#[test]
fn named_junction_viewer_exposes_point_references_controls_and_dimension_targets() {
    let dir = Directory::new();
    view_request(&dir, view_example("projected-junction-pad"), "junction").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("junction/view.json")).unwrap())
            .unwrap();
    let sk = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
        .unwrap();
    assert_eq!(sk["solver"]["accepted"], true);
    let right = sk["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "junction-reference-right")
        .unwrap();
    assert_eq!(right["detail"]["point"], "reference:junction:right");
    assert_eq!(right["anchors"].as_array().unwrap().len(), 1);
    assert!(
        right["targets"]
            .as_array()
            .unwrap()
            .contains(&json!("reference:junction:right"))
    );
    assert!(
        right["parameters"]
            .as_array()
            .unwrap()
            .contains(&json!("junction_shift"))
    );
    let dimension = sk["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["detail"]["constraint_index"] == 1)
        .unwrap();
    assert_eq!(dimension["status"], "passed");
    assert!(
        dimension["targets"]
            .as_array()
            .unwrap()
            .contains(&json!("reference:junction:right"))
    );
}

#[test]
fn intersection_viewer_exposes_both_source_controls_and_fixed_crossing_targets() {
    let dir = Directory::new();
    view_request(
        &dir,
        view_example("projected-intersection-pad"),
        "intersection",
    )
    .unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("intersection/view.json")).unwrap())
            .unwrap();
    let sk = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
        .unwrap();
    assert_eq!(sk["solver"]["accepted"], true);
    let right = sk["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "intersection-reference-right")
        .unwrap();
    assert_eq!(right["detail"]["point"], "reference:intersection:right");
    assert_eq!(
        right["targets"],
        json!(["reference", "guide", "reference:intersection:right"])
    );
    for name in ["radius", "offset", "intersection_shift", "match_radius"] {
        assert!(
            right["parameters"]
                .as_array()
                .unwrap()
                .contains(&json!(name)),
            "{name}"
        );
    }
    assert!((right["anchors"][0][0].as_f64().unwrap() - 8.).abs() < 1e-8);
    let dimension = sk["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["detail"]["constraint_index"] == 2)
        .unwrap();
    assert_eq!(dimension["status"], "passed");
    assert!(
        dimension["targets"]
            .as_array()
            .unwrap()
            .contains(&json!("reference:intersection:right"))
    );
    assert!(
        !sk["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "fixed-reference:intersection:right")
    );
}
