//! Viewer annotations for mirrors, scales, offsets, groups, and patterns.

use super::*;

#[test]
fn mirror_viewer_exposes_plane_controls_and_follows_native_reflected_bounds() {
    let dir = Directory::new();
    for plane in [0.0, 2.0] {
        let mut request = view_example("mirrored-part");
        request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "plane_x")
            .unwrap()["default"]["scalar"]["value"] = json!(plane);
        let name = format!("mirror-{plane}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let solid = &data["scenes"][0];
        assert_eq!(solid["valid"], true);
        assert!((solid["bounds"][0][0].as_f64().unwrap() - (2.0 * plane - 25.0)).abs() < 1e-7);
        let annotation = solid["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "driving-mirror-plane")
            .unwrap();
        assert_eq!(
            annotation["detail"]["plane_origin"],
            json!([plane, 0.0, 0.0])
        );
        assert_eq!(annotation["detail"]["plane_normal"], json!([1.0, 0.0, 0.0]));
        assert_eq!(annotation["parameters"], json!(["plane_tilt", "plane_x"]));
    }
}

#[test]
fn scale_viewer_exposes_dimensionless_factor_centre_and_resized_bounds() {
    let dir = Directory::new();
    view_request(&dir, view_example("scaled-part"), "scaled").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("scaled/view.json")).unwrap()).unwrap();
    let solid = &data["scenes"][0];
    assert_eq!(solid["valid"], true);
    assert!((solid["bounds"][1][0].as_f64().unwrap() - 30.0).abs() < 1e-7);
    let a = solid["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "driving-scale-factor")
        .unwrap();
    assert_eq!(a["detail"]["factor"], 1.2);
    assert_eq!(a["detail"]["scale_center"], json!([0.0, 0.0, 0.0]));
    assert_eq!(a["parameters"], json!(["scale_center_x", "scale_factor"]));
}

#[test]
fn offset_viewer_exposes_signed_distance_tolerance_and_regenerated_bounds() {
    let dir = Directory::new();
    for (name, allowance, radius) in [("outward", 1.0, 11.0), ("inward", -2.0, 8.0)] {
        let mut request = view_example("offset-part");
        request["model"]["instances"][0]["base"]["overrides"] = json!({"allowance":{"scalar":{"value":allowance,"dimension":"length","unit":"millimeter"}}});
        view_request(&dir, request, name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(name).join("view.json")).unwrap())
                .unwrap();
        let scene = &data["scenes"][0];
        assert_eq!(scene["valid"], true);
        assert!((scene["bounds"][1][0].as_f64().unwrap() - radius).abs() < 1e-7);
        let a = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "driving-skin-offset")
            .unwrap();
        assert_eq!(a["detail"]["value_mm"], allowance);
        assert_eq!(a["detail"]["tolerance_mm"], 1e-6);
        assert_eq!(a["parameters"], json!(["allowance", "offset_tolerance"]));
        assert_eq!(a["anchors"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn grouped_cutter_view_exposes_membership_and_multi_hole_plate_controls() {
    let dir = Directory::new();
    let mut request = view_example("multi-hole-plate");
    request["outputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"instance":"plate","output":"tools"}));
    request["model"]["instances"][0]["base"]["overrides"] =
        json!({"spacing":{"scalar":{"value":14,"dimension":"length","unit":"millimeter"}}});
    view_request(&dir, request, "grouped").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("grouped/view.json")).unwrap())
            .unwrap();
    assert_eq!(data["scenes"].as_array().unwrap().len(), 2);
    let body = &data["scenes"][0];
    assert_eq!(body["valid"], true);
    assert!((body["bounds"][1][0].as_f64().unwrap() - 38.0).abs() < 1e-7);
    assert!(body["parameters"].get("hole_radius").is_some());
    let tools = &data["scenes"][1];
    assert_eq!(tools["valid"], true);
    let group = tools["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "compound-inputs")
        .unwrap();
    assert_eq!(group["detail"]["input_count"], 9);
    assert_eq!(group["detail"]["inputs"].as_array().unwrap().len(), 9);
    assert_eq!(group["kind"], "group");
}

#[test]
fn linear_pattern_viewer_follows_count_and_spacing_and_labels_placement_span() {
    let dir = Directory::new();
    let mut request = view_example("patterned-plate");
    request["outputs"] = json!([{"instance":"plate","output":"body"},{"instance":"plate","output":"column-tools"},{"instance":"plate","output":"tools"}]);
    request["model"]["instances"][0]["base"]["overrides"] = json!({"columns":{"scalar":{"value":4,"dimension":"scalar","unit":null}},"rows":{"scalar":{"value":2,"dimension":"scalar","unit":null}},"spacing":{"scalar":{"value":12,"dimension":"length","unit":"millimeter"}}});
    view_request(&dir, request, "patterns").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("patterns/view.json")).unwrap())
            .unwrap();
    assert_eq!(data["scenes"].as_array().unwrap().len(), 3);
    let body = &data["scenes"][0];
    assert_eq!(body["valid"], true);
    assert!((body["bounds"][1][0].as_f64().unwrap() - 46.0).abs() < 1e-7);
    assert!((body["bounds"][1][1].as_f64().unwrap() - 22.0).abs() < 1e-7);
    for (index, count, span, control) in [(1, 4.0, 36.0, "columns"), (2, 2.0, 12.0, "rows")] {
        let scene = &data["scenes"][index];
        let get = |id| {
            scene["annotations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == id)
                .unwrap()
        };
        assert_eq!(get("linear-pattern-count")["detail"]["count"], count);
        assert_eq!(get("linear-pattern-count")["parameters"], json!([control]));
        assert_eq!(
            get("linear-pattern-spacing")["parameters"],
            json!(["spacing"])
        );
        let anchors = get("linear-pattern-span")["anchors"].as_array().unwrap();
        let a = anchors[0].as_array().unwrap();
        let b = anchors[1].as_array().unwrap();
        let distance = (0..3)
            .map(|i| (b[i].as_f64().unwrap() - a[i].as_f64().unwrap()).powi(2))
            .sum::<f64>()
            .sqrt();
        assert!((distance - span).abs() < 1e-7);
    }
}

#[test]
fn circular_pattern_viewer_links_count_sweep_and_an_arc_at_the_source_radius() {
    let dir = Directory::new();
    let mut request = view_example("bolt-circle");
    request["outputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"instance":"plate","output":"tools"}));
    view_request(&dir, request, "circle").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("circle/view.json")).unwrap()).unwrap();
    assert_eq!(data["scenes"][0]["valid"], true);
    let scene = &data["scenes"][1];
    let get = |id| {
        scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap()
    };
    assert_eq!(get("circular-pattern-count")["detail"]["count"], 6.0);
    assert_eq!(
        get("circular-pattern-count")["parameters"],
        json!(["count"])
    );
    let angle = get("circular-pattern-angle");
    assert_eq!(angle["parameters"], json!(["count", "sweep_angle"]));
    assert!(
        (angle["detail"]["value_radians"].as_f64().unwrap() - std::f64::consts::TAU / 6.0).abs()
            < 1e-12
    );
    let arc = angle["detail"]["angular_arc"].as_array().unwrap();
    assert_eq!(arc.len(), 33);
    for p in arc {
        assert!((p[0].as_f64().unwrap().hypot(p[1].as_f64().unwrap()) - 12.0).abs() < 1e-6);
    }
    assert!((arc[32][0].as_f64().unwrap() - 6.0).abs() < 1e-6);
    assert!((arc[32][1].as_f64().unwrap() - 6.0 * 3.0_f64.sqrt()).abs() < 1e-6);
}
