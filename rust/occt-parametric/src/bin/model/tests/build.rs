//! One-shot build requests: edits, failures, exports, and migration.

use super::*;

#[test]
fn bracket_edit_failure_repair_exports_and_preservation() {
    let dir = Directory::new();
    let model = fixture();
    let original = model.to_json_pretty().unwrap();
    let mut initial = request(&model, json!([]));
    initial["step"] = json!(true);
    initial["stl"] = json!(true);
    initial["preview"] = json!(true);
    let built = invoke(&dir, initial, "accepted").unwrap();
    assert_eq!(built["status"], "built");
    assert_eq!(built["verification"].as_array().unwrap().len(), 2);
    assert!(
        built["verification"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["status"] == "passed")
    );
    let session = Session::new().unwrap();
    let imported = session
        .load_step(dir.0.join("accepted/parts.step"))
        .unwrap();
    assert!(session.is_valid(&imported).unwrap());
    assert!(fs::metadata(dir.0.join("accepted/0001.stl")).unwrap().len() > 84);
    assert!(
        fs::read_to_string(dir.0.join("accepted/0001.svg"))
            .unwrap()
            .contains("<polyline")
    );
    let changed = invoke(&dir, request(&model, edit("hole_spacing", 35.0)), "changed").unwrap();
    assert!(
        (changed["outputs"][0]["volume_mm3"].as_f64().unwrap()
            - built["outputs"][0]["volume_mm3"].as_f64().unwrap())
        .abs()
            < 1e-7
    );
    let accepted = fs::read(dir.0.join("accepted/report.json")).unwrap();
    let err = invoke(&dir, request(&model, edit("thickness", 1.0)), "rejected").unwrap_err();
    assert_eq!(err.stage, "regeneration");
    assert!(err.message.contains("wall"));
    assert!(!dir.0.join("rejected").exists());
    invoke(&dir, request(&model, edit("thickness", 5.0)), "repaired").unwrap();
    assert_eq!(
        fs::read(dir.0.join("accepted/report.json")).unwrap(),
        accepted
    );
    assert_eq!(model.to_json_pretty().unwrap(), original);
    let saved =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("changed/model.json")).unwrap())
            .unwrap();
    assert_eq!(saved.family.requirements, model.family.requirements);
    assert_eq!(
        saved.instances[0].overrides()["hole_spacing"],
        ParameterValue::Scalar(Quantity::length(35.0, LengthUnit::Millimeter))
    );
    assert_eq!(
        invoke(&dir, request(&model, json!([])), "accepted")
            .unwrap_err()
            .stage,
        "publication"
    );
}
#[test]
fn malformed_requests_edits_selections_and_future_schema_rejected() {
    let dir = Directory::new();
    let model = fixture();
    for (name, stage, change) in [
        ("future", "validation", ("future", json!(999))),
        ("unknown", "request", ("unknown", json!(true))),
        ("empty", "request", ("outputs", json!([]))),
        (
            "missing",
            "selection",
            (
                "outputs",
                json!([{"instance":"bracket","output":"missing"}]),
            ),
        ),
        ("edit", "regeneration", ("edits", edit("missing", 1.0))),
        (
            "duplicate",
            "edits",
            (
                "edits",
                json!([edit("thickness", 4.0)[0], edit("thickness", 5.0)[0]]),
            ),
        ),
    ] {
        let mut value = request(&model, json!([]));
        if change.0 == "future" {
            value["model"]["schema_version"] = change.1;
        } else {
            value[change.0] = change.1;
        }
        assert_eq!(invoke(&dir, value, name).unwrap_err().stage, stage);
        assert!(!dir.0.join(name).exists());
    }
}
#[test]
fn migration_and_source_metadata_survive() {
    let dir = Directory::new();
    let model = fixture();
    let mut value = request(&model, json!([]));
    value["model"]["schema_version"] = json!(68);
    invoke(&dir, value, "old").unwrap();
    let saved =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("old/model.json")).unwrap())
            .unwrap();
    assert_eq!(saved, model);
}
