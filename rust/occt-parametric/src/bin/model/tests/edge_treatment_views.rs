//! Viewer annotations for fillets, chamfers, and variable fillets.

use super::*;

#[test]
fn fillet_and_chamfer_views_expose_values_selected_source_edges_and_linked_controls() {
    let dir = Directory::new();
    let request = view_example("edge-treatments");
    view_request(&dir, request, "treatments").unwrap();
    let data: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("treatments/view.json")).unwrap())
            .unwrap();
    for (index, kind, value, parameter) in [
        (0, "fillet", 2.0, "fillet_radius"),
        (1, "chamfer", 1.5, "chamfer_distance"),
    ] {
        let scene = &data["scenes"][index];
        assert_eq!(scene["valid"], true);
        let a = scene["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == format!("driving-{kind}"))
            .unwrap();
        assert_eq!(a["detail"]["value_mm"], value);
        assert_eq!(a["detail"]["selected_edge_count"], 4);
        assert_eq!(a["detail"]["source_reference"], true);
        assert_eq!(a["detail"]["measurement"], false);
        assert!(
            a["parameters"]
                .as_array()
                .unwrap()
                .contains(&json!(parameter))
        );
        let paths = a["detail"]["dimension_paths"].as_array().unwrap();
        assert_eq!(paths.len(), 4);
        for path in paths {
            assert_eq!(path.as_array().unwrap().len(), 8);
            let start_z = path[0][2].as_f64().unwrap();
            let end_z = path[7][2].as_f64().unwrap();
            assert_eq!(start_z.min(end_z), 0.0);
            assert_eq!(start_z.max(end_z), 10.0);
        }
    }
}

#[test]
fn variable_fillet_view_exposes_contour_law_without_inventing_spatial_station_positions() {
    let dir = Directory::new();
    let mut request = view_example("variable-fillet");
    view_request(&dir, request.clone(), "first").unwrap();
    let read = |name: &str| -> Value {
        serde_json::from_str(&fs::read_to_string(dir.0.join(format!("{name}/view.json"))).unwrap())
            .unwrap()
    };
    let data = read("first");
    let scene = &data["scenes"][0];
    assert_eq!(scene["valid"], true);
    let annotations = scene["annotations"].as_array().unwrap();
    let law = annotations
        .iter()
        .find(|a| a["id"] == "driving-variable-fillet")
        .unwrap();
    assert_eq!(law["detail"]["selected_edge_count"], 1);
    assert_eq!(
        law["detail"]["dimension_paths"][0]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        law["detail"]["radius_law"],
        json!([
            {"position":0.0,"radius_mm":1.0}, {"position":0.25,"radius_mm":2.5}, {"position":1.0,"radius_mm":2.0}
        ])
    );
    assert_eq!(law["detail"]["spatial_stations"], false);
    for name in [
        "start_radius",
        "end_radius",
        "middle_radius",
        "station_position",
    ] {
        assert!(law["parameters"].as_array().unwrap().contains(&json!(name)));
    }
    let station = annotations
        .iter()
        .find(|a| a["id"] == "fillet-station-1")
        .unwrap();
    assert_eq!(station["detail"]["position"], 0.25);
    assert_eq!(station["detail"]["value_mm"], 2.5);
    assert_eq!(station["detail"]["spatial_station"], false);
    assert!(
        station["parameters"]
            .as_array()
            .unwrap()
            .contains(&json!("station_position"))
    );
    let overrides = &mut request["model"]["instances"][0]["base"]["overrides"];
    overrides["middle_radius"] =
        json!({"scalar":{"value":2.0,"dimension":"length","unit":"millimeter"}});
    overrides["station_position"] =
        json!({"scalar":{"value":0.5,"dimension":"scalar","unit":null}});
    view_request(&dir, request, "edited").unwrap();
    let edited = read("edited");
    let station = edited["scenes"][0]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "fillet-station-1")
        .unwrap();
    assert_eq!(station["detail"]["position"], 0.5);
    assert_eq!(station["detail"]["value_mm"], 2.0);
    assert_eq!(edited["scenes"][0]["valid"], true);
}
