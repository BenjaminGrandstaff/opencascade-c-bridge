//! Builds rejected by required requirements report the whole verification
//! table and output measurements, not just the first failing check.

use super::*;

#[test]
fn rejected_builds_report_every_requirement_with_evidence() {
    let dir = Directory::new();
    let model = fixture();
    let error = invoke(&dir, request(&model, edit("thickness", 1.0)), "thin").unwrap_err();
    assert_eq!(error.stage, "regeneration");
    assert_eq!(error.message, "required verification failed: bracket/wall");
    assert!(!dir.0.join("thin").exists());
    let details = &error.details;
    assert_eq!(details["accepted"], false);
    assert_eq!(details["required_failures"], json!(["bracket/wall"]));
    let rows = details["verification"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    // Failures first, with the measured value, sampling evidence and witness.
    let wall = &rows[0];
    assert_eq!(
        (&wall["scope"], &wall["requirement"], &wall["status"]),
        (&json!("bracket"), &json!("wall"), &json!("failed"))
    );
    assert!((wall["measured"]["value"].as_f64().unwrap() - 1.0).abs() < 1e-6);
    assert_eq!(wall["measured"]["minimum"], 2.0);
    assert_eq!(wall["evidence"]["kind"], "sampled");
    assert_eq!(wall["witness"]["points_mm"].as_array().unwrap().len(), 2);
    assert_eq!(rows[1]["requirement"], "valid");
    assert_eq!(rows[1]["status"], "passed");
    assert_eq!(
        details["verification_counts"],
        json!({"total": 2, "passed": 1, "failed": 1, "required_failed": 1, "omitted": 0})
    );
    // The rejected geometry is still measured.
    let output = &details["outputs"][0];
    assert_eq!(output["valid"], true);
    assert!(output["volume_mm3"].as_f64().unwrap() > 0.0);
    assert_eq!(output["bounds_mm"]["max"][2], 25.0);
    assert_eq!(details["outputs_omitted"], 0);
    // An accepted build reports the same table shape.
    let built = invoke(&dir, request(&model, json!([])), "built").unwrap();
    let accepted = built["verification"].as_array().unwrap();
    assert_eq!(accepted.len(), 2);
    assert!(accepted.iter().all(|r| r["status"] == "passed"));
    // Only required failures reject a build.
    let mut advisory = fixture();
    for requirement in &mut advisory.family.requirements {
        requirement.priority = RequirementPriority::Advisory;
    }
    let built = invoke(&dir, request(&advisory, edit("thickness", 1.0)), "advisory").unwrap();
    assert_eq!(built["status"], "built");
    assert_eq!(built["verification"][1]["status"], "failed");
}

#[test]
fn rejected_reports_are_bounded_across_many_instances() {
    let dir = Directory::new();
    let model = fixture();
    let mut graph = model.instance_graph().unwrap();
    graph
        .set_override(
            "bracket",
            "thickness",
            ParameterValue::Scalar(Quantity::length(1.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    let mut outputs = vec![json!({"instance": "bracket", "output": "body"})];
    for i in 1..600 {
        let id = format!("copy-{i}");
        graph
            .add_clone(&id, "bracket", HashMap::new(), "scale")
            .unwrap();
        outputs.push(json!({"instance": id, "output": "body"}));
    }
    let model = ModelDocument::from_graph(&graph);
    let mut value = request(&model, json!([]));
    value["outputs"] = json!(outputs);
    let started = std::time::Instant::now();
    let error = invoke(&dir, value, "many").unwrap_err();
    let elapsed = started.elapsed().as_secs_f64();
    let details = &error.details;
    assert_eq!(
        details["verification_counts"],
        json!({"total": 1200, "passed": 600, "failed": 600, "required_failed": 600, "omitted": 700})
    );
    let rows = details["verification"].as_array().unwrap();
    assert_eq!(rows.len(), report::MAX_REJECTED_ROWS);
    assert!(rows.iter().all(|r| r["status"] == "failed"));
    assert_eq!(details["required_failures"].as_array().unwrap().len(), 500);
    assert_eq!(details["outputs"].as_array().unwrap().len(), 500);
    assert_eq!(details["outputs_omitted"], 100);
    assert_eq!(details["generated_variants"], 1);
    assert!(
        error
            .message
            .ends_with("copy-105/wall, copy-106/wall and 590 more"),
        "{}",
        error.message
    );
    // The whole report stays far below the MCP server's 1 MiB stderr read.
    let mut line = json!({"stage": error.stage, "message": error.message});
    line.as_object_mut()
        .unwrap()
        .extend(details.as_object().unwrap().clone());
    let size = line.to_string().len() + error.message.len();
    assert!(size < 700_000, "{size} bytes");
    assert!(elapsed < 10.0, "{elapsed} s");
}
