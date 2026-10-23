//! `dxf` in build requests: cutting profiles of outputs.

use super::*;
use occt_parametric::dxf::{DxfImportOptions, import_dxf};

fn tray(dxf: Value) -> Value {
    let mut request = view_example("hemmed-tray");
    request["outputs"] = json!([{"instance": "flat", "output": "flat"}]);
    request["step"] = json!(false);
    request["preview"] = json!(false);
    request["dxf"] = dxf;
    request
}

#[test]
fn flat_patterns_export_their_outline_holes_and_bend_lines() {
    let dir = Directory::new();
    let built = invoke(&dir, tray(json!({})), "tray").unwrap();
    let entry = &built["outputs"][0]["dxf"];
    assert_eq!(entry["file"], "0001.dxf");
    // Five bends, each given on both faces of the sheet, written once.
    assert_eq!(entry["layers"]["BEND"], 5);
    // The slot crosses a bend zone, so each straight side comes in three
    // pieces: six lines and two arcs.
    assert_eq!(entry["layers"]["HOLES"], 8);
    assert_eq!(entry["entities"]["ARC"], 2);
    assert_eq!(entry["origin_mm"][0], 0.0);
    // Read back, the cut layers rebuild a face of the same area.
    let text = fs::read_to_string(dir.0.join("tray/0001.dxf")).unwrap();
    let options = DxfImportOptions {
        layers: vec!["OUTLINE".into(), "HOLES".into()],
        ..Default::default()
    };
    let imported = import_dxf(&text, &options).unwrap();
    assert_eq!((imported.report.loops, imported.report.holes), (2, 1));
    assert_eq!(imported.report.unit_source, "$INSUNITS");
    let session = Session::new().unwrap();
    let family: FamilyDefinition = serde_json::from_value(json!({"id": "check", "version": 1, "parameters": [],
        "derived_parameters": [], "derived_vector_parameters": [], "constraints": [], "requirements": [], "datums": [],
        "features": [{"id": "outline", "operation": {"sketch_face": {"sketch": imported.sketch}}}]}))
    .unwrap();
    let part = PartInstance {
        id: "check".into(),
        definition: &family,
        overrides: std::collections::HashMap::new(),
        provenance: "test".into(),
    };
    let generated = part.regenerate(&session).unwrap();
    let area = session
        .surface_area(generated.shape("outline").unwrap())
        .unwrap();
    let expected = entry["area_mm2"].as_f64().unwrap();
    assert!((area - expected).abs() < 1e-6, "{area} vs {expected}");
    // Without bend lines the BEND layer is empty.
    let plain = invoke(&dir, tray(json!({"bend_lines": false})), "plain").unwrap();
    assert!(plain["outputs"][0]["dxf"]["layers"].get("BEND").is_none());
}

#[test]
fn a_face_selector_picks_the_face_and_must_pick_one() {
    let dir = Directory::new();
    let mut request = request(&fixture(), json!([]));
    let top = json!({"at_extreme": {"axis": "z", "extremum": "maximum", "tolerance": {"literal": {"value": 1e-6, "dimension": "length", "unit": "millimeter"}}}});
    request["dxf"] = json!({"face": top});
    let built = invoke(&dir, request.clone(), "top").unwrap();
    let entry = &built["outputs"][0]["dxf"];
    assert_eq!(entry["y_axis"], json!([0.0, 1.0, 0.0]));
    assert!(dir.0.join("top/0001.dxf").exists());
    // A selector for every face is refused at export, and nothing is left.
    request["dxf"] = json!({"face": {"normal_aligned": {"direction": {"literal": {"x": {"value": 0, "dimension": "scalar", "unit": null},
        "y": {"value": 0, "dimension": "scalar", "unit": null}, "z": {"value": 1, "dimension": "scalar", "unit": null}}},
        "minimum_dot": {"literal": {"value": -1.0, "dimension": "scalar", "unit": null}}}}});
    let error = invoke(&dir, request, "every").unwrap_err();
    assert_eq!(error.stage, "export");
    assert!(
        error.message.contains("needs one planar face"),
        "{}",
        error.message
    );
}
