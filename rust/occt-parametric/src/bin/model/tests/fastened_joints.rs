//! Fastened joints in build reports: the screw chosen, how it fits, and the
//! parts list, for local and placed instances.

use super::*;

fn set(parameter: &str, v: f64) -> Value {
    json!({"instance": "lid", "parameter": parameter,
        "value": ParameterValue::Scalar(Quantity::length(v, LengthUnit::Millimeter))})
}

fn lid(edits: Value) -> Value {
    let mut request = view_example("fastened-lid");
    request["step"] = json!(false);
    request["preview"] = json!(false);
    request["edits"] = edits;
    request
}

#[test]
fn builds_report_each_joint_screw_and_parts_list() {
    let dir = Directory::new();
    let built = invoke(&dir, lid(json!([])), "lid").unwrap();
    let joint = &built["outputs"][0]["fastened_joints"][0];
    assert_eq!(joint["feature"], "joint");
    assert_eq!(joint["stack"], json!(["lid", "base"]));
    assert_eq!(joint["positions"], 4);
    // M5 counterbored 5 mm into the 8 mm lid: 3 mm grip plus 7.5 mm of
    // thread asks for 10.5 mm, so 12 mm, engaging 9 mm.
    assert_eq!(
        joint["screw"],
        json!({"nominal_diameter_mm": 5.0, "pitch_mm": 0.8, "length_mm": 12.0,
            "chosen": true, "minimum_length_mm": 10.5})
    );
    assert_eq!(joint["grip_mm"], json!({"min": 3.0, "max": 3.0}));
    assert_eq!(joint["thread_engagement_mm"]["min"], 9.0);
    assert_eq!(
        joint["hardware"],
        json!([{"part": "socket_head_cap_screw_iso_4762",
            "designation": "ISO 4762 M5 × 12", "quantity": 4}])
    );
    // M6: a 6 mm counterbore leaves 2 mm of grip; 2 + 9 asks for 11 mm.
    let m6 = invoke(&dir, lid(json!([set("bolt", 6.0)])), "m6").unwrap();
    assert_eq!(
        m6["outputs"][0]["fastened_joints"][0]["hardware"][0]["designation"],
        "ISO 4762 M6 × 12"
    );
    // A lid thinner than the counterbore is refused with the reason.
    let error = invoke(&dir, lid(json!([set("lid", 4.0)])), "thin").unwrap_err();
    assert_eq!(error.stage, "regeneration");
    assert!(
        error.message.contains("counterbore does not fit in 'lid'"),
        "{}",
        error.message
    );
}

#[test]
fn placed_instances_measure_their_joints_where_they_are() {
    // The same lid in a frame turned a quarter turn about x and moved: the
    // report measures the placed parts along the placed screw lines.
    let mut request = lid(json!([]));
    let length = |v: f64| json!({"value": v, "dimension": "length", "unit": "millimeter"});
    let scalar = |v: f64| json!({"value": v, "dimension": "scalar", "unit": null});
    request["model"]["frames"] = json!([{"id": "turned", "parent": null, "provenance": "test",
        "placement": {"translation": {"x": length(100.0), "y": length(-40.0), "z": length(5.0)},
            "rotation": {"origin": {"x": length(0.0), "y": length(0.0), "z": length(0.0)},
                "axis": {"x": scalar(1.0), "y": scalar(0.0), "z": scalar(0.0)},
                "angle_radians": std::f64::consts::FRAC_PI_2}}}]);
    request["model"]["instances"][0]["base"]["frame"] = json!("turned");
    let dir = Directory::new();
    let built = invoke(&dir, request, "placed").unwrap();
    let output = &built["outputs"][0];
    // The assembly really moved: its 28 mm height now runs along -y.
    let (min, max) = (&output["bounds_mm"]["min"], &output["bounds_mm"]["max"]);
    let height = max[1].as_f64().unwrap() - min[1].as_f64().unwrap();
    assert!((height - 28.0).abs() < 1e-6, "{output}");
    let joint = &output["fastened_joints"][0];
    assert_eq!(joint["screw"]["length_mm"], 12.0);
    assert_eq!(joint["grip_mm"]["min"], 3.0);
}
