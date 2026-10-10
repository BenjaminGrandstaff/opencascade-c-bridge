//! Core viewer contract: sketches, solids, budgets, extrusions, and holes.

use super::*;

#[test]
fn annotated_sketch_and_solid_have_true_dimensions_and_linked_controls() {
    let dir = Directory::new();
    let request = view_example("sketch-block");
    let report = view_request(&dir, request, "view").unwrap();
    assert_eq!(report["status"], "visualized");
    assert_eq!(report["scenes"], 2);
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("view/view.json")).unwrap()).unwrap();
    let solid = &data["scenes"][0];
    let sketch = &data["scenes"][1];
    assert!(!solid["mesh"].as_array().unwrap().is_empty());
    assert_eq!(solid["bounds"][1], json!([40.0, 25.0, 10.0]));
    let travel = solid["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "driving-extrusion")
        .unwrap();
    assert_eq!(travel["parameters"], json!(["depth"]));
    assert_eq!(travel["detail"]["value_mm"], 10.0);
    assert_eq!(sketch["solver"]["solved"], true);
    let width = sketch["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "constraint-4")
        .unwrap();
    assert_eq!(width["parameters"], json!(["width"]));
    assert_eq!(width["anchors"][1], json!([40.0, 0.0, 0.0]));
    assert_eq!(width["status"], "passed");
    let svg = fs::read_to_string(dir.0.join("view/view-0002.svg")).unwrap();
    assert!(svg.contains("data-annotation=\"constraint-4\""));
    assert!(svg.contains("40.000 mm"));
    let html = fs::read_to_string(dir.0.join("view/viewer.html")).unwrap();
    assert!(html.contains("Driving values come from the model"));
    assert!(!html.contains("VIEWER_DATA"));
}
#[test]
fn failed_sketch_and_required_shape_checks_are_visible_without_acceptance() {
    let dir = Directory::new();
    view_request(&dir, view_example("sketch-conflict"), "conflict").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("conflict/view.json")).unwrap())
            .unwrap();
    assert!(data["scenes"][0]["error"].is_string());
    let sketch = &data["scenes"][1];
    assert_eq!(sketch["solver"]["solved"], false);
    let failed = sketch["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "constraint-4")
        .unwrap();
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["detail"]["max_residual"], 20.0);
    let mut model = fixture();
    let mut graph = model.instance_graph().unwrap();
    graph
        .set_override(
            "bracket",
            "thickness",
            ParameterValue::Scalar(Quantity::length(1.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    model.instances = ModelDocument::from_graph(&graph).instances;
    view_request(&dir,json!({"schema":"occb-model-view-v1","model":model,"outputs":[{"instance":"bracket","output":"body"}]}),"thin").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("thin/view.json")).unwrap()).unwrap();
    let solid = &data["scenes"][0];
    assert!(!solid["mesh"].as_array().unwrap().is_empty());
    let wall = solid["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "requirement-wall")
        .unwrap();
    assert_eq!(wall["status"], "failed");
    assert!(!wall["anchors"].as_array().unwrap().is_empty());
    let session = Session::new().unwrap();
    let part = model.instance_graph().unwrap().resolve("bracket").unwrap();
    assert!(part.regenerate(&session).is_err());
    let diagnostic = part.diagnostic_geometry(&session).unwrap();
    assert!(
        diagnostic
            .generated
            .verification
            .iter()
            .any(|v| v.status == VerificationStatus::Failed)
    );
    drop(diagnostic);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn visualization_budgets_and_untrusted_label_roundtrip_are_safe() {
    let dir = Directory::new();
    let mut request = view_example("sketch-block");
    request["options"] = json!({"maximum_vertices":1});
    assert_eq!(
        view_request(&dir, request, "limited").unwrap_err().stage,
        "visualization"
    );
    assert!(!dir.0.join("limited").exists());
    let mut request = view_example("sketch-block");
    let text = "<script>window.untrusted=true</script>&dimension";
    request["model"]["family"]["requirements"][0]["statement"] = json!(text);
    view_request(&dir, request, "safe").unwrap();
    let html = fs::read_to_string(dir.0.join("safe/viewer.html")).unwrap();
    assert!(!html.contains("<script>window.untrusted"));
    let start = html
        .find("<script id=\"view-data\" type=\"application/json\">")
        .unwrap()
        + "<script id=\"view-data\" type=\"application/json\">".len();
    let end = start + html[start..].find("</script>").unwrap();
    let data: Value = serde_json::from_str(&html[start..end]).unwrap();
    assert!(
        data["scenes"][0]["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["label"] == text)
    );
}

#[test]
fn advanced_sketch_dimensions_and_profile_operations_reach_the_ai_view_contract() {
    let dir = Directory::new();
    let request: Value = serde_json::from_str(include_str!(
        "../../../../../../tools/model/sketch-advanced.request.json"
    ))
    .unwrap();
    let model = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    view_request(&dir, request, "advanced").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("advanced/view.json")).unwrap())
            .unwrap();
    let scenes = data["scenes"].as_array().unwrap();
    let sketch = scenes
        .iter()
        .find(|s| s["feature"] == "profile" && s["kind"] == "sketch")
        .unwrap();
    assert_eq!(sketch["solver"]["solved"], true);
    let annotations = sketch["annotations"].as_array().unwrap();
    for label in ["R 6.000 mm", "Ø 12.000 mm", "∠ 1.047 rad", "SYM", "ON"] {
        assert!(
            annotations
                .iter()
                .any(|a| a["label"] == label && a["status"] == "passed"),
            "missing {label}"
        );
    }
    let angle = annotations
        .iter()
        .find(|a| a["label"] == "∠ 1.047 rad")
        .unwrap();
    assert_eq!(angle["detail"]["residual_unit"], "rad");
    assert_eq!(angle["detail"]["angular_arc"].as_array().unwrap().len(), 17);
    let diameter = annotations
        .iter()
        .find(|a| a["label"] == "Ø 12.000 mm")
        .unwrap();
    let anchors = diameter["anchors"].as_array().unwrap();
    assert!((anchors[1][0].as_f64().unwrap() - anchors[0][0].as_f64().unwrap() - 12.).abs() < 1e-7);
    let edited = scenes
        .iter()
        .find(|s| s["feature"] == "edited-path")
        .unwrap();
    assert_eq!(edited["profile_error"], Value::Null);
    assert!(!edited["edited_profile"].as_array().unwrap().is_empty());
    assert_eq!(edited["profile_operations"].as_array().unwrap().len(), 3);
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &model.family,
        overrides: Default::default(),
        provenance: "test".into(),
    };
    let generated = part.regenerate(&session).unwrap();
    assert!(
        (session.volume(generated.shape("body").unwrap()).unwrap() - 320. * std::f64::consts::PI)
            .abs()
            < 1e-6
    );
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn extrusion_extent_annotations_use_generated_lengths_and_centered_anchors() {
    let dir = Directory::new();
    view_request(&dir, view_example("extrusion-limits"), "limits").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("limits/view.json")).unwrap()).unwrap();
    for (feature, expected, centered) in [
        ("body", 12.0, false),
        ("selected", 14.0, false),
        ("symmetric", 12.0, true),
    ] {
        let scene = data["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["feature"] == feature && s["kind"] == "solid")
            .unwrap();
        let annotation = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "driving-extrusion")
            .unwrap();
        assert!((annotation["detail"]["value_mm"].as_f64().unwrap() - expected).abs() < 1e-6);
        assert_eq!(annotation["detail"]["driving"], centered);
        let start = annotation["anchors"][0][2].as_f64().unwrap();
        let end = annotation["anchors"][1][2].as_f64().unwrap();
        assert!((start - if centered { -expected / 2.0 } else { 0.0 }).abs() < 1e-6);
        assert!((end - if centered { expected / 2.0 } else { expected }).abs() < 1e-6);
    }
    let mut wire_request = view_example("extrusion-limits");
    let operation = wire_request["model"]["family"]["features"][0]["operation"]
        .as_object_mut()
        .unwrap();
    let mut profile = operation.remove("sketch_face").unwrap();
    profile["sketch"]["constraints"] = json!([]);
    for point in profile["sketch"]["points"].as_array_mut().unwrap() {
        point["fixed"] = json!(true);
    }
    profile["sketch"]["points"][2]["x"] = serde_json::to_value(mm(30.0)).unwrap();
    operation.insert("sketch_wire".into(), profile);
    view_request(&dir, wire_request, "wire").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("wire/view.json")).unwrap()).unwrap();
    let scene = data["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["feature"] == "body" && s["kind"] == "solid")
        .unwrap();
    let annotation = scene["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "driving-extrusion")
        .unwrap();
    assert!((annotation["detail"]["value_mm"].as_f64().unwrap() - 12.0).abs() < 1e-6);
    for axis in 0..2 {
        assert!(
            (annotation["anchors"][0][axis].as_f64().unwrap()
                - annotation["anchors"][1][axis].as_f64().unwrap())
            .abs()
                < 1e-6
        );
    }
}

#[test]
fn curved_extent_viewer_measures_actual_cap_hits_and_target_edits() {
    let dir = Directory::new();
    for depth in [12.0, 22.0] {
        let mut request = view_example("curved-extrusions");
        let parameter = request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "depth")
            .unwrap();
        parameter["default"]["scalar"]["value"] = json!(depth);
        let name = format!("curved-{depth}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(&name).join("view.json")).unwrap())
                .unwrap();
        for (feature, expected) in [("body", depth + 8.0), ("selected", depth)] {
            let scene = data["scenes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["feature"] == feature && s["kind"] == "solid")
                .unwrap();
            let annotation = scene["annotations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == "driving-extrusion")
                .unwrap();
            assert!(
                (annotation["detail"]["value_mm"].as_f64().unwrap() - expected).abs() < 1e-6,
                "{annotation}"
            );
            assert_eq!(annotation["detail"]["measurement"], "profile_centroid_ray");
            assert!((annotation["anchors"][1][2].as_f64().unwrap() - expected).abs() < 1e-6);
        }
    }
}

#[test]
fn drill_point_viewer_shows_bore_depth_tip_depth_angle_and_linked_controls() {
    let dir = Directory::new();
    let request = view_example("drill-point");
    view_request(&dir, request.clone(), "point").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("point/view.json")).unwrap()).unwrap();
    let scene = &data["scenes"][0];
    let annotations = scene["annotations"].as_array().unwrap();
    let find = |id: &str| annotations.iter().find(|a| a["id"] == id).unwrap();
    assert_eq!(
        find("driving-hole-depth")["detail"]["depth_reference"],
        "full_diameter"
    );
    assert_eq!(find("driving-hole-depth")["detail"]["value_mm"], 8.0);
    let tip = find("measured-drill-tip");
    assert!((tip["detail"]["value_mm"].as_f64().unwrap() - 3.0_f64.sqrt()).abs() < 1e-7);
    assert!(
        (tip["detail"]["total_depth_mm"].as_f64().unwrap() - 8.0 - 3.0_f64.sqrt()).abs() < 1e-7
    );
    assert_eq!(tip["parameters"], json!(["diameter", "point_angle"]));
    let angle = find("driving-drill-angle");
    assert_eq!(angle["detail"]["angular_arc"].as_array().unwrap().len(), 17);
    assert_eq!(angle["parameters"], json!(["point_angle"]));
    assert!((tip["anchors"][1][2].as_f64().unwrap() - (12.0 - 3.0_f64.sqrt())).abs() < 1e-7);
    let model = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let result = model
        .instance_graph()
        .unwrap()
        .resolve("block")
        .unwrap()
        .regenerate(&session)
        .unwrap();
    let expected =
        15000.0 - 72.0 * std::f64::consts::PI - 3.0 * std::f64::consts::PI * 3.0_f64.sqrt();
    assert!((session.volume(result.shape("body").unwrap()).unwrap() - expected).abs() < 1e-7);
}

#[test]
fn geometric_hole_viewer_measures_curved_limits_and_links_upstream_controls() {
    let dir = Directory::new();
    for depth in [12.0, 22.0] {
        let mut request = view_example("hole-limits");
        let parameter = request["model"]["family"]["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "depth")
            .unwrap();
        parameter["default"]["scalar"]["value"] = json!(depth);
        let name = format!("hole-{depth}");
        view_request(&dir, request, &name).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join(&name).join("view.json")).unwrap())
                .unwrap();
        for feature in ["bored", "selected-hole"] {
            let scene = data["scenes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["feature"] == feature)
                .unwrap();
            let extent = scene["annotations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == "measured-hole-limit")
                .unwrap_or_else(|| panic!("missing limit on {feature}: {scene}"));
            assert!((extent["detail"]["value_mm"].as_f64().unwrap() - depth - 8.0).abs() < 1e-6);
            assert_eq!(extent["detail"]["measurement"], "bore_centre_ray");
            assert!(
                extent["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("depth"))
            );
            assert!(
                extent["parameters"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("diameter"))
            );
            assert!((extent["anchors"][1][2].as_f64().unwrap() - depth - 8.0).abs() < 1e-6);
        }
    }
}
