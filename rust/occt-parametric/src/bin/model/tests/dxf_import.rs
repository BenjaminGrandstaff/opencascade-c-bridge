//! `--import-dxf`: a DXF file's outline as a sketch feature that builds.

use super::*;

fn import(dir: &Directory, request: Value, name: &str) -> Result<Value, Failure> {
    let path = dir.0.join(format!("{name}.request.json"));
    fs::write(&path, request.to_string()).unwrap();
    crate::dxf_import::run(
        &path.into_os_string(),
        &dir.0.join(format!("{name}.json")).into_os_string(),
    )
}

fn bracket() -> &'static str {
    include_str!("../../../../../../tools/model/dxf/bracket.dxf")
}

#[test]
fn imported_outlines_come_back_as_a_checked_sketch_feature() {
    let dir = Directory::new();
    let result = import(
        &dir,
        json!({"schema": "occb-dxf-import-v1", "dxf": bracket(), "feature_id": "bracket-outline",
               "options": {"layers": ["OUTLINE", "HOLES"]}}),
        "bracket",
    )
    .unwrap();
    assert_eq!(result["status"], "imported");
    assert_eq!(result["feature"]["id"], "bracket-outline");
    let sketch = &result["feature"]["operation"]["sketch_face"]["sketch"];
    assert_eq!(sketch["holes"].as_array().unwrap().len(), 3);
    assert_eq!(result["face"]["built"], true);
    let area = 2525.0 - 33.25 * std::f64::consts::PI;
    assert!((result["face"]["area_mm2"].as_f64().unwrap() - area).abs() < 1e-6);
    // The result file holds the same, and the feature drops into a model.
    let saved: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("bracket.json")).unwrap()).unwrap();
    assert_eq!(saved, result);
    let feature: FeatureDefinition = serde_json::from_value(result["feature"].clone()).unwrap();
    assert_eq!(feature.id, "bracket-outline");
    // Every layer: the note is skipped and counted.
    let all = import(
        &dir,
        json!({"schema": "occb-dxf-import-v1", "dxf": bracket()}),
        "all",
    )
    .unwrap();
    assert_eq!(all["feature"]["id"], "outline");
    assert_eq!(
        all["report"]["skipped"]["TEXT: not geometry for a sketch"],
        1
    );
}

#[test]
fn open_outlines_import_but_report_that_no_face_builds() {
    let dir = Directory::new();
    let open = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n0\n\
0\nLINE\n8\n0\n10\n10\n20\n0\n11\n10\n21\n10\n0\nENDSEC\n0\nEOF\n";
    let result = import(
        &dir,
        json!({"schema": "occb-dxf-import-v1", "dxf": open}),
        "open",
    )
    .unwrap();
    assert_eq!(result["report"]["open_curves"], 2);
    assert_eq!(result["face"]["built"], false);
    assert!(
        result["face"]["error"]
            .as_str()
            .unwrap()
            .contains("outline")
    );
}

#[test]
fn bad_import_requests_fail_at_the_right_stage() {
    let dir = Directory::new();
    let stage = |request: Value, name: &str| import(&dir, request, name).unwrap_err().stage;
    assert_eq!(
        stage(json!({"schema": "other", "dxf": ""}), "schema"),
        "request"
    );
    assert_eq!(
        stage(
            json!({"schema": "occb-dxf-import-v1", "dxf": "x", "extra": 1}),
            "extra"
        ),
        "request"
    );
    assert_eq!(
        stage(
            json!({"schema": "occb-dxf-import-v1", "dxf": "AutoCAD Binary DXF"}),
            "binary"
        ),
        "import"
    );
    // An existing result file is never overwritten.
    import(
        &dir,
        json!({"schema": "occb-dxf-import-v1", "dxf": bracket()}),
        "once",
    )
    .unwrap();
    assert_eq!(
        stage(
            json!({"schema": "occb-dxf-import-v1", "dxf": bracket()}),
            "once"
        ),
        "publication"
    );
}
