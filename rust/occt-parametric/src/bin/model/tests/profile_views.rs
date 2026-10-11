//! Viewer annotations for lofts, sweeps, revolves, and hollow profiles.

use super::*;
use occt_bridge::{ShapeType, Vec3};

#[test]
fn edited_regions_viewer_and_incremental_builds_keep_named_pieces_holes_and_source_constraints() {
    let dir = Directory::new();
    let request = view_example("edited-regions");
    let family = ModelDocument::from_json(&request["model"].to_string())
        .unwrap()
        .family;
    let session = Session::new().unwrap();
    let mut part = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let original = part.regenerate(&session).unwrap();
    let original_volume = session.volume(original.shape("body").unwrap()).unwrap();
    assert_eq!(
        session
            .subshape_count(original.shape("body").unwrap(), ShapeType::Solid)
            .unwrap(),
        3
    );
    part.overrides.insert(
        "width".into(),
        ParameterValue::Scalar(Quantity::length(24., LengthUnit::Millimeter)),
    );
    let changed = part.regenerate_incremental(&session, &original).unwrap();
    assert!(changed.regeneration.reused.contains(&"ring_profile".into()));
    assert!(changed.regeneration.reused.contains(&"ring".into()));
    assert!(
        (session.volume(changed.shape("body").unwrap()).unwrap() - original_volume - 80.).abs()
            < 1e-6
    );
    // An oversized corner edit rejects without changing either accepted generation.
    part.overrides.insert(
        "fillet".into(),
        ParameterValue::Scalar(Quantity::length(100., LengthUnit::Millimeter)),
    );
    assert!(part.regenerate_incremental(&session, &changed).is_err());
    assert!(
        (session.volume(original.shape("body").unwrap()).unwrap() - original_volume).abs() < 1e-7
    );
    assert!(session.is_valid(changed.shape("body").unwrap()).unwrap());
    view_request(&dir, request, "edited").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("edited/view.json")).unwrap()).unwrap();
    let scenes = data["scenes"].as_array().unwrap();
    let pads = scenes
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
        .unwrap();
    let curves = pads["edited_curves"].as_array().unwrap();
    assert_eq!(curves.len(), 2);
    for id in ["bottom-left", "bottom-right"] {
        let curve = curves.iter().find(|c| c["id"] == id).unwrap();
        assert_eq!(curve["source"], "bottom");
    }
    assert!(pads["profile_error"].is_null());
    assert_eq!(pads["region_edits"].as_array().unwrap().len(), 2);
    let ring = scenes
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "ring_profile")
        .unwrap();
    assert!(ring["profile_error"].is_null());
    assert_eq!(ring["region_edits"].as_array().unwrap().len(), 1);
    assert!(
        pads["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "curve-edit-1" && a["status"] == "constructed")
    );
}

#[test]
fn composed_regions_keep_a_bore_and_two_solids_through_parameter_edits() {
    let dir = Directory::new();
    for (outer, bore, island, thickness) in [(12., 3., 4., 2.), (16., 4., 5., 3.)] {
        let mut request = view_example("composed-regions");
        for p in request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
        {
            let value = match p["id"].as_str().unwrap() {
                "outer_radius" => outer,
                "bore_radius" => bore,
                "island_radius" => island,
                "thickness" => thickness,
                _ => panic!(),
            };
            p["default"]["scalar"]["value"] = json!(value);
        }
        let family = ModelDocument::from_json(&request["model"].to_string())
            .unwrap()
            .family;
        let session = Session::new().unwrap();
        let part = PartInstance {
            id: "set".into(),
            definition: &family,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let built = part.regenerate(&session).unwrap();
        let body = built.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        assert_eq!(session.subshape_count(body, ShapeType::Solid).unwrap(), 2);
        let area = std::f64::consts::PI * (outer * outer - bore * bore + island * island);
        assert!((session.volume(body).unwrap() - area * thickness).abs() < 1e-6);
        // Radial midpoint on the original divider is now interior material.
        let midpoint = session
            .create_box(
                Vec3::new((outer + bore) / 2. - 0.1, -0.1, 0.5),
                Vec3::new(0.2, 0.2, 0.2),
            )
            .unwrap();
        assert!((session.overlap_volume(body, &midpoint).unwrap() - 0.008).abs() < 1e-7);
        // The excluded centre remains a through bore.
        let probe = session
            .create_box(Vec3::new(-0.1, -0.1, 0.5), Vec3::new(0.2, 0.2, 0.2))
            .unwrap();
        assert!(session.overlap_volume(body, &probe).unwrap().abs() < 1e-9);
        let name = format!("composed-{outer}");
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
        assert_eq!(sketch["region_discovery"]["merge_adjacent"], true);
        assert!(sketch["profile_error"].is_null());
        assert_eq!(
            sketch["annotations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == "region-discovery")
                .unwrap()["label"],
            "Composed material regions"
        );
        let preview = sketch["edited_profile"].as_array().unwrap();
        // Source divider remains visible as source geometry, but is absent
        // from the actual material boundaries drawn by the edited preview.
        for point in preview.iter().flat_map(|line| line.as_array().unwrap()) {
            let x = point[0].as_f64().unwrap();
            let y = point[1].as_f64().unwrap();
            let r = x.hypot(y);
            let ir = (x - 40.).hypot(y);
            assert!(
                (r - outer).abs() < 1e-6 || (r - bore).abs() < 1e-6 || (ir - island).abs() < 1e-6
            );
        }
    }
}

#[test]
fn discovered_lens_viewer_tracks_seed_selection_and_native_volume_after_edits() {
    let dir = Directory::new();
    for spacing in [8., 12.] {
        let mut request = view_example("discovered-lens");
        let parameters = request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap();
        parameters
            .iter_mut()
            .find(|p| p["id"] == "spacing")
            .unwrap()["default"]["scalar"]["value"] = json!(spacing);
        let name = format!("lens-{spacing}");
        view_request(&dir, request.clone(), &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(&name).join("view.json")).unwrap())
                .unwrap();
        let sketch = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(sketch["region_discovery"]["seeds"], json!(["pick"]));
        assert!(sketch["profile_error"].is_null());
        assert!(!sketch["edited_profile"].as_array().unwrap().is_empty());
        let annotation = sketch["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "region-discovery")
            .unwrap();
        assert_eq!(annotation["status"], "constructed");
        assert!((annotation["anchors"][0][0].as_f64().unwrap() - spacing / 2.).abs() < 1e-7);
        let family = ModelDocument::from_json(&request["model"].to_string())
            .unwrap()
            .family;
        let session = Session::new().unwrap();
        let part = PartInstance {
            id: "lens".into(),
            definition: &family,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let built = part.regenerate(&session).unwrap();
        let body = built.shape("body").unwrap();
        assert!(session.is_valid(body).unwrap());
        let radius: f64 = 12.;
        let area = 2. * radius.powi(2) * (spacing / (2. * radius)).acos()
            - spacing * (4. * radius.powi(2) - spacing.powi(2)).sqrt() / 2.;
        assert!((session.volume(body).unwrap() - area * 2.).abs() < 1e-6);
    }
}

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

#[test]
fn native_intersection_viewer_links_rational_and_ellipse_crossings_to_both_sources() {
    let dir = Directory::new();
    for example in [
        "projected-spline-intersection-pad",
        "projected-ellipse-intersection-pad",
    ] {
        view_request(&dir, view_example(example), example).unwrap();
        let data: Value = serde_json::from_str(
            &fs::read_to_string(dir.0.join(example).join("view.json")).unwrap(),
        )
        .unwrap();
        let sk = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
            .unwrap();
        assert_eq!(sk["solver"]["accepted"], true);
        let refs = sk["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["id"].as_str().unwrap().starts_with("intersection-"))
            .collect::<Vec<_>>();
        assert_eq!(refs.len(), 2);
        let right = refs
            .iter()
            .find(|a| a["id"] == "intersection-reference-right")
            .unwrap();
        let points = &sk["points"];
        assert_eq!(right["anchors"][0], points["reference:intersection:right"]);
        for name in if example.contains("spline") {
            vec!["width", "rise", "weight", "offset"]
        } else {
            vec!["radius", "minor_radius", "offset"]
        } {
            assert!(
                right["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(name)),
                "{name}"
            );
        }
        assert_eq!(
            right["targets"],
            json!(["reference", "guide", "reference:intersection:right"])
        );
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
    }
}

#[test]
fn projected_touch_viewer_exposes_a_single_fixed_contact_and_direct_dimension() {
    let dir = Directory::new();
    view_request(
        &dir,
        view_example("projected-tangent-intersection-pad"),
        "touch",
    )
    .unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("touch/view.json")).unwrap()).unwrap();
    let sk = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch" && s["feature"] == "profile")
        .unwrap();
    assert_eq!(sk["solver"]["accepted"], true);
    let refs = sk["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["id"].as_str().unwrap().starts_with("intersection-"))
        .collect::<Vec<_>>();
    assert_eq!(refs.len(), 1);
    let touch = refs[0];
    assert_eq!(touch["detail"]["point"], "reference:intersection:touch");
    assert_eq!(touch["anchors"][0], json!([20., 10., 0.]));
    for name in ["width", "rise", "offset"] {
        assert!(
            touch["parameters"]
                .as_array()
                .unwrap()
                .contains(&json!(name))
        );
    }
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
            .contains(&json!("reference:intersection:touch"))
    );
}

#[test]
fn dimensioned_plate_reports_driving_and_reference_dimensions_after_edits() {
    let dir = Directory::new();
    for (name, angle) in [("default", 2.6), ("steeper", 2.3)] {
        let mut request = view_example("dimensioned-plate");
        request["model"]["instances"][0]["base"]["overrides"] =
            json!({"slope_angle":{"scalar":{"value":angle,"dimension":"scalar","unit":null}}});
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let sketch = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(sketch["solver"]["free_degrees"], 0);
        assert_eq!(sketch["solver"]["redundant_equations"], 0);
        let annotations = sketch["annotations"].as_array().unwrap();
        let measured = |id: &str| {
            annotations
                .iter()
                .find(|a| a["id"] == format!("measurement-{id}"))
                .unwrap()
        };
        let height = measured("height");
        assert_eq!(height["status"], "measured");
        assert_eq!(height["detail"]["driving"], false);
        let expected = 20.0 + 24.0 * f64::sin(angle);
        assert!((height["detail"]["value"].as_f64().unwrap() - expected).abs() < 1e-8);
        let run = measured("slope_run")["detail"]["value"].as_f64().unwrap();
        assert!((run + 24.0 * f64::cos(angle)).abs() < 1e-8);
        assert_eq!(measured("shoulder")["detail"]["unit"], "rad");
        let perimeter = measured("hole_perimeter")["detail"]["value"]
            .as_f64()
            .unwrap();
        assert!((perimeter - 8.0 * std::f64::consts::PI).abs() < 1e-9);
        // Every driving constraint, including the new kinds, is satisfied.
        let constraints = annotations
            .iter()
            .filter(|a| a["id"].as_str().unwrap().starts_with("constraint-"))
            .collect::<Vec<_>>();
        assert!(constraints.iter().all(|a| a["status"] == "passed"));
        for label in ["collinear", "∠x 2", "length 24.000 mm"] {
            assert!(
                constraints
                    .iter()
                    .any(|a| a["label"].as_str().unwrap().starts_with(label)),
                "{label}"
            );
        }
        let slope = constraints
            .iter()
            .find(|a| a["label"].as_str().unwrap().starts_with("∠x"))
            .unwrap();
        assert_eq!(slope["parameters"], json!(["slope_angle"]));
    }
}

#[test]
fn slotted_plate_groups_generated_shapes_and_follows_slot_edits() {
    let dir = Directory::new();
    for (name, travel, radius) in [("default", 30.0, 3.5), ("longer", 36.0, 4.5)] {
        let mut request = view_example("slotted-plate");
        request["model"]["instances"][0]["base"]["overrides"] = json!({
            "slot_travel":{"scalar":{"value":travel,"dimension":"length","unit":"millimeter"}},
            "slot_radius":{"scalar":{"value":radius,"dimension":"length","unit":"millimeter"}}});
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let sketch = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(sketch["solver"]["free_degrees"], 0);
        let annotations = sketch["annotations"].as_array().unwrap();
        let find = |id: &str| annotations.iter().find(|a| a["id"] == id).unwrap();
        let slot = find("shape-lower_slot");
        assert_eq!(slot["label"], format!("slot R {radius:.3} mm"));
        assert_eq!(slot["parameters"], json!(["slot_radius"]));
        assert_eq!(
            slot["targets"],
            json!([
                "lower_slot:right",
                "lower_slot:end_cap",
                "lower_slot:left",
                "lower_slot:start_cap"
            ])
        );
        assert_eq!(find("shape-hex")["label"], "regular 6-gon");
        assert_eq!(find("shape-hex")["targets"].as_array().unwrap().len(), 6);
        let length = find("measurement-slot_length")["detail"]["value"]
            .as_f64()
            .unwrap();
        assert!((length - travel).abs() < 1e-9);
        let side = find("measurement-hex_side")["detail"]["value"]
            .as_f64()
            .unwrap();
        assert!((side - 6.0).abs() < 1e-9);
        // The solid is the plate minus two slots and the hexagon.
        let body = &data["scenes"][0];
        assert_eq!(body["valid"], true);
    }
}

#[test]
fn arched_panel_draws_its_elliptical_arc_and_follows_rise_edits() {
    let dir = Directory::new();
    for (name, rise) in [("default", 12.0), ("flatter", 7.0)] {
        let mut request = view_example("arched-panel");
        request["model"]["instances"][0]["base"]["overrides"] =
            json!({"rise":{"scalar":{"value":rise,"dimension":"length","unit":"millimeter"}}});
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let body = &data["scenes"][0];
        assert_eq!(body["valid"], true);
        assert!((body["bounds"][1][1].as_f64().unwrap() - (30.0 + rise)).abs() < 1e-9);
        let sketch = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        assert_eq!(sketch["solver"]["free_degrees"], 0);
        assert_eq!(sketch["solver"]["redundant_equations"], 0);
        // The arch preview stays on the upper half of the ellipse.
        let arch = sketch["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "arch")
            .unwrap();
        for p in arch["points"].as_array().unwrap() {
            let (x, y) = (p[0].as_f64().unwrap(), p[1].as_f64().unwrap());
            assert!(y >= 30.0 - 1e-9, "{x} {y}");
            let r = ((x - 20.0) / 20.0).hypot((y - 30.0) / rise);
            assert!((r - 1.0).abs() < 1e-6, "{x} {y}");
        }
        let length = sketch["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "measurement-arch_length")
            .unwrap()["detail"]["value"]
            .as_f64()
            .unwrap();
        assert!(length > 40.0 && length < 20.0 * std::f64::consts::PI);
    }
}

/// Volume of revolving the rational quadratic conic from (r, 0) through
/// control (r, l) to (0, l) about the z axis, by midpoint integration.
fn conic_nose_volume(r: f64, l: f64, rho: f64) -> f64 {
    let w = rho / (1.0 - rho);
    let point = |t: f64| {
        let (a, b, c) = ((1.0 - t).powi(2), 2.0 * w * t * (1.0 - t), t * t);
        let d = a + b + c;
        (r * (a + b) / d, l * (b + c) / d)
    };
    let n = 200_000;
    (0..n)
        .map(|i| {
            let (t0, t1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let ((x0, z0), (x1, z1)) = (point(t0), point(t1));
            // Frustum stack: second-order accurate in the step.
            std::f64::consts::PI * (x0 * x0 + x0 * x1 + x1 * x1) / 3.0 * (z1 - z0)
        })
        .sum()
}

#[test]
fn conic_nose_revolves_elliptic_parabolic_and_hyperbolic_profiles() {
    let dir = Directory::new();
    for (rho, kind) in [(0.3, "elliptic"), (0.5, "parabola"), (0.7, "hyperbolic")] {
        let mut request = view_example("conic-nose");
        request["model"]["instances"][0]["base"]["overrides"] =
            json!({"rho":{"scalar":{"value":rho,"dimension":"scalar","unit":null}}});
        let name = format!("rho-{rho}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(&name).join("view.json")).unwrap())
                .unwrap();
        let sketch = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        let shape = sketch["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "shape-skin")
            .unwrap();
        assert_eq!(shape["label"], format!("conic ρ {rho:.3} ({kind})"));
        assert_eq!(shape["parameters"], json!(["rho"]));
        assert_eq!(data["scenes"][0]["valid"], true);
    }
    // The solid volume matches the revolved exact conic.
    let session = Session::new().unwrap();
    let request = view_example("conic-nose");
    let family = ModelDocument::from_json(&request["model"].to_string())
        .unwrap()
        .family;
    for rho in [0.3, 0.5, 0.7] {
        let mut overrides = HashMap::new();
        overrides.insert(
            "rho".to_owned(),
            ParameterValue::Scalar(Quantity::scalar(rho)),
        );
        let part = PartInstance {
            id: "nose".into(),
            definition: &family,
            overrides,
            provenance: "test".into(),
        };
        let built = part.regenerate(&session).unwrap();
        let volume = session.volume(built.shape("body").unwrap()).unwrap();
        let expected = conic_nose_volume(15.0, 40.0, rho);
        assert!(
            (volume - expected).abs() < 1e-6 * expected,
            "{rho}: {volume} vs {expected}"
        );
    }
}

#[test]
fn keyhole_tag_previews_trims_at_crossings_and_follows_neck_edits() {
    let dir = Directory::new();
    for (name, neck) in [("default", 6.0f64), ("narrow", 3.5)] {
        let mut request = view_example("keyhole-tag");
        request["model"]["instances"][0]["base"]["overrides"] = json!({"neck_width":{"scalar":{"value":neck,"dimension":"length","unit":"millimeter"}}});
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let body = &data["scenes"][0];
        assert_eq!(body["valid"], true);
        assert!((body["bounds"][1][1].as_f64().unwrap() - 28.0).abs() < 1e-9);
        let sketch = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "sketch")
            .unwrap();
        let trims = sketch["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["label"].as_str().unwrap().starts_with("Trim"))
            .collect::<Vec<_>>();
        assert_eq!(trims.len(), 3);
        assert_eq!(
            trims[1]["targets"],
            json!(["head", "right", "hint_r", "left", "hint_l"])
        );
        // The edited profile meets each leg where it crosses the head:
        // y = neck_length − √(r² − (w/2)²).
        let crossing = 20.0 - (64.0f64 - neck * neck / 4.0).sqrt();
        let edited = sketch["edited_profile"].as_array().unwrap();
        let ends = edited
            .iter()
            .flat_map(|edge| {
                let edge = edge.as_array().unwrap();
                [edge[0].clone(), edge[edge.len() - 1].clone()]
            })
            .collect::<Vec<_>>();
        for x in [neck / 2.0, -neck / 2.0] {
            assert!(
                ends.iter()
                    .any(|p| (p[0].as_f64().unwrap() - x).abs() < 1e-9
                        && (p[1].as_f64().unwrap() - crossing).abs() < 1e-9),
                "{x}"
            );
        }
    }
}

#[test]
fn spacer_set_extrudes_three_regions_and_rejects_overlap_edits() {
    let dir = Directory::new();
    view_request(&dir, view_example("spacer-set"), "set").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("set/view.json")).unwrap()).unwrap();
    let body = &data["scenes"][0];
    assert_eq!(body["valid"], true);
    assert!((body["bounds"][1][0].as_f64().unwrap() - 62.0).abs() < 1e-9);
    // A negative gap pushes the washer into the shim: the regions overlap.
    let mut request = view_example("spacer-set");
    request["model"]["family"]["parameters"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "gap")
        .unwrap()["minimum"]["value"] = json!(-10);
    request["model"]["instances"][0]["base"]["overrides"] =
        json!({"gap":{"scalar":{"value":-3,"dimension":"length","unit":"millimeter"}}});
    // The viewer keeps the scene and reports the regeneration failure.
    view_request(&dir, request, "overlap").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("overlap/view.json")).unwrap())
            .unwrap();
    let failure = data["scenes"][0]["error"].as_str().unwrap();
    assert!(
        failure.contains("sketch profile and region 0 overlap"),
        "{failure}"
    );
    // The solid compound holds one solid per region.
    let session = Session::new().unwrap();
    let family = ModelDocument::from_json(&view_example("spacer-set")["model"].to_string())
        .unwrap()
        .family;
    let part = PartInstance {
        id: "set".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let built = part.regenerate(&session).unwrap();
    let body = built.shape("body").unwrap();
    assert_eq!(
        session
            .subshape_count(body, occt_bridge::ShapeType::Solid)
            .unwrap(),
        3
    );
    let shim = 20.0 * 14.0;
    let washer = std::f64::consts::PI * (49.0 - 9.0);
    let slot = 8.0 * 12.0 + std::f64::consts::PI * 16.0;
    let volume = session.volume(body).unwrap();
    assert!(
        (volume - 2.0 * (shim + washer + slot)).abs() < 1e-7,
        "{volume}"
    );
}

#[test]
fn g2_paddle_joins_match_curvature_on_the_built_profile() {
    let dir = Directory::new();
    view_request(&dir, view_example("g2-paddle"), "paddle").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("paddle/view.json")).unwrap()).unwrap();
    let sketch = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch")
        .unwrap();
    assert_eq!(sketch["solver"]["free_degrees"], 0);
    assert_eq!(sketch["solver"]["redundant_equations"], 0);
    let g2 = sketch["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["label"] == "G2")
        .collect::<Vec<_>>();
    assert_eq!(g2.len(), 4);
    assert!(g2.iter().all(|a| a["status"] == "passed"));
    // On the native face, the edges meeting at each joint have equal
    // curvature vectors: 0 at the sides, 1/12 toward the crown centre.
    let session = Session::new().unwrap();
    let family = ModelDocument::from_json(&view_example("g2-paddle")["model"].to_string())
        .unwrap()
        .family;
    let part = PartInstance {
        id: "paddle".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let built = part.regenerate(&session).unwrap();
    let edges = session
        .subshapes(
            built.shape("profile").unwrap(),
            occt_bridge::ShapeType::Edge,
        )
        .unwrap();
    for (x, y, curvature) in [
        (40.0, 30.0, (0.0, 0.0)),
        (32.0, 40.0, (-1.0 / 12.0, 0.0)),
        (8.0, 40.0, (1.0 / 12.0, 0.0)),
        (0.0, 30.0, (0.0, 0.0)),
    ] {
        let at = occt_bridge::Vec3::new(x, y, 0.0);
        let meeting = edges
            .iter()
            .filter(|e| {
                let (a, b, _, _) = session.curve_end_data(e).unwrap();
                (a.x - x).hypot(a.y - y) < 1e-7 || (b.x - x).hypot(b.y - y) < 1e-7
            })
            .collect::<Vec<_>>();
        assert_eq!(meeting.len(), 2, "{x} {y}");
        for edge in meeting {
            let k = session.curve_curvature_at(edge, at).unwrap();
            assert!(
                (k.x - curvature.0).abs() < 1e-8 && (k.y - curvature.1).abs() < 1e-8,
                "{x} {y}: {k:?}"
            );
        }
    }
}

#[test]
fn engraved_nameplate_draws_glyphs_and_cuts_them_from_the_plate() {
    let session = Session::new().unwrap();
    if session
        .font_file("Liberation Sans", occt_bridge::FontStyle::Bold)
        .is_err()
    {
        eprintln!("Liberation Sans Bold is not installed; skipping");
        return;
    }
    let dir = Directory::new();
    view_request(&dir, view_example("engraved-nameplate"), "plate").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("plate/view.json")).unwrap()).unwrap();
    let body = &data["scenes"][0];
    assert_eq!(body["valid"], true);
    let label = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == "letters_profile")
        .unwrap();
    let glyphs = label["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["text"] == "marking")
        .count();
    let annotation = label["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "text-marking")
        .unwrap();
    assert_eq!(annotation["detail"]["glyph_edges"], glyphs);
    assert!(glyphs > 20);
    assert!(
        annotation["detail"]["font_file"]
            .as_str()
            .unwrap()
            .ends_with(".ttf")
    );
    assert_eq!(annotation["parameters"], json!(["text_height"]));
    // Engraving removes exactly the letters' area times the depth.
    let family = ModelDocument::from_json(&view_example("engraved-nameplate")["model"].to_string())
        .unwrap()
        .family;
    let part = PartInstance {
        id: "plate".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let built = part.regenerate(&session).unwrap();
    let letters = session
        .surface_area(built.shape("letters_profile").unwrap())
        .unwrap();
    let removed = 80.0 * 25.0 * 4.0 - session.volume(built.shape("body").unwrap()).unwrap();
    assert!(
        (removed - letters * 1.0).abs() < 1e-6 * removed,
        "{removed} {letters}"
    );
}

#[test]
fn reliable_sketch_viewer_exposes_convergence_rank_and_recovery_report() {
    let dir = Directory::new();
    let request = view_example("reliable-sketch");
    let report = view_request(&dir, request, "reliable").unwrap();
    assert_eq!(report["status"], "visualized");
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("reliable/view.json")).unwrap())
            .unwrap();
    let scenes = data["scenes"].as_array().unwrap();
    let sketch = scenes.iter().find(|s| s["kind"] == "sketch").unwrap();
    assert_eq!(sketch["solver"]["report"]["termination"], "converged");
    assert_eq!(sketch["solver"]["report"]["rank_reliable"], true);
    assert_eq!(sketch["solver"]["free_degrees"], 0);
    assert_eq!(sketch["solver"]["accepted"], true);
    assert!(
        sketch["solver"]["report"]["accepted_steps"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        sketch["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "solver-report" && a["status"] == "passed")
    );
    assert!(
        scenes
            .iter()
            .any(|s| s["kind"] == "solid" && s["valid"] == true)
    );
}
