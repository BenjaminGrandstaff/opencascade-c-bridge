//! Guarded feature patch requests.

use super::*;

fn patch_request(
    dir: &Directory,
    model: &ModelDocument,
    changes: Value,
    edits: Value,
    name: &str,
    revision: &str,
) -> Result<Value, Failure> {
    let path = dir.0.join("edit.request.json");
    fs::write(&path,json!({"schema":"occb-model-edit-v1","model":model,"changes":changes,"edits":edits,
        "outputs":[{"instance":"bracket","output":"body"}],"revision":{"id":revision,"author":"test","recorded_at":"test-time","message":"guarded edit"},
        "step":false,"stl":false,"preview":false}).to_string()).unwrap();
    patch::run(&path.into_os_string(), &dir.0.join(name).into_os_string())
}
#[test]
fn guarded_feature_edits_preserve_requirements_and_record_actual_changes() {
    let dir = Directory::new();
    let model = fixture();
    let expected = model.family.features[1].clone();
    let mut replacement = expected.clone();
    if let FeatureOperation::Box { size, .. } = &mut replacement.operation {
        *size = VectorExpr::Components {
            x: mm(60.0),
            y: ScalarExpr::Parameter("thickness".into()),
            z: mm(35.0),
        };
    }
    let report=patch_request(&dir,&model,json!([{"action":"replace_feature","family":"bracket","expected":expected,"feature":replacement}]),edit("hole_spacing",35.0),"edited","revision-1").unwrap();
    assert_eq!(report["revision"]["id"], "revision-1");
    let saved =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("edited/model.json")).unwrap())
            .unwrap();
    assert_eq!(saved.family.requirements, model.family.requirements);
    assert_eq!(saved.family.version, model.family.version + 1);
    assert_eq!(saved.family.features[1], replacement);
    assert_eq!(saved.revisions.len(), 1);
    assert_eq!(
        saved.instances[0].overrides()["hole_spacing"],
        ParameterValue::Scalar(Quantity::length(35.0, LengthUnit::Millimeter))
    );
    let changes: DocumentRevision =
        serde_json::from_str(&fs::read_to_string(dir.0.join("edited/changes.json")).unwrap())
            .unwrap();
    assert_eq!(changes, saved.revisions[0]);
    assert!(changes.changes.iter().any(|c| {
        c.path
            .contains(&DocumentPathSegment::Field("overrides".into()))
    }));
    assert!(changes.changes.iter().any(|c| {
        c.path
            .contains(&DocumentPathSegment::Entity("upright".into()))
    }));
    assert_eq!(report["outputs"][0]["bounds_mm"]["max"][2], 35.0);
    assert!(patch_request(&dir,&saved,json!([{"action":"replace_feature","family":"bracket","expected":expected,"feature":replacement}]),json!([]),"stale","revision-2").is_err());
    assert!(!dir.0.join("stale").exists());
    // Parameter-only edits also record a revision, without changing family version.
    patch_request(
        &dir,
        &saved,
        json!([]),
        edit("hole_spacing", 30.0),
        "parameter-only",
        "revision-2",
    )
    .unwrap();
    let next = ModelDocument::from_json(
        &fs::read_to_string(dir.0.join("parameter-only/model.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(next.family.version, saved.family.version);
    assert_eq!(next.revisions[1].parent.as_deref(), Some("revision-1"));
}
#[test]
fn guarded_additions_removal_and_new_intent_validate_as_one_transaction() {
    let dir = Directory::new();
    let mut model = fixture();
    let unused = FeatureDefinition {
        id: "unused".into(),
        operation: FeatureOperation::Sphere {
            center: point(100.0, 0.0, 0.0),
            radius: mm(1.0),
        },
    };
    model.family.features.push(unused.clone());
    let added = FeatureDefinition {
        id: "extra".into(),
        operation: FeatureOperation::Sphere {
            center: point(100.0, 0.0, 0.0),
            radius: ScalarExpr::Parameter("extra_radius".into()),
        },
    };
    let parameter = ParameterDefinition {
        id: "extra_radius".into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: ParameterValue::Scalar(Quantity::length(2.0, LengthUnit::Millimeter)),
        minimum: None,
        maximum: None,
    };
    let requirement = Requirement {
        id: "extra.valid".into(),
        version: 1,
        kind: RequirementKind::Validation,
        priority: RequirementPriority::Required,
        statement: "extra sphere is valid".into(),
        rule: VerificationRule::ShapeValid {
            output: "extra".into(),
        },
        provenance: "test".into(),
        traces: vec![TraceTarget::Feature("extra".into())],
    };
    let reference = NamedReference {
        name: "top".into(),
        target: ReferenceTarget::Faces(FaceSelector::NormalAligned {
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.99)),
        }),
    };
    let report = patch_request(
        &dir,
        &model,
        json!([
            {"action":"add_feature","family":"bracket","feature":added},
            {"action":"add_requirement","family":"bracket","requirement":requirement},
            {"action":"add_reference","family":"bracket","reference":reference},
            {"action":"add_parameter","family":"bracket","parameter":parameter},
            {"action":"remove_feature","family":"bracket","expected":unused}
        ]),
        json!([]),
        "added",
        "add-1",
    )
    .unwrap();
    assert_eq!(report["verification"].as_array().unwrap().len(), 3);
    let saved =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("added/model.json")).unwrap())
            .unwrap();
    assert!(saved.family.features.iter().any(|f| f.id == "extra"));
    assert!(!saved.family.features.iter().any(|f| f.id == "unused"));
    assert_eq!(saved.family.requirements[..2], model.family.requirements);
    assert_eq!(saved.family.requirements[2], requirement);
    assert_eq!(saved.family.references[0], reference);
}
#[test]
fn patch_conflicts_invalid_dependencies_required_failures_and_noops_leave_no_build() {
    let dir = Directory::new();
    let model = fixture();
    let expected = model.family.features[0].clone();
    for (name, changes, stage) in [
        (
            "collision",
            json!([{"action":"add_feature","family":"bracket","feature":expected}]),
            "conflict",
        ),
        (
            "duplicate",
            json!([{"action":"remove_feature","family":"bracket","expected":expected},{"action":"remove_feature","family":"bracket","expected":expected}]),
            "patch",
        ),
        (
            "unknown",
            json!([{"action":"remove_feature","family":"missing","expected":expected}]),
            "patch",
        ),
        (
            "dependent",
            json!([{"action":"remove_feature","family":"bracket","expected":expected}]),
            "validation",
        ),
    ] {
        assert_eq!(
            patch_request(&dir, &model, changes, json!([]), name, "r1")
                .unwrap_err()
                .stage,
            stage
        );
        assert!(!dir.0.join(name).exists());
    }
    assert_eq!(
        patch_request(
            &dir,
            &model,
            json!([]),
            edit("thickness", 1.0),
            "thin",
            "r1"
        )
        .unwrap_err()
        .stage,
        "regeneration"
    );
    assert!(!dir.0.join("thin").exists());
    assert_eq!(patch_request(&dir,&model,json!([{"action":"replace_feature","family":"bracket","expected":expected,"feature":expected}]),json!([]),"same-feature","r1").unwrap_err().stage,"revision");
    assert!(!dir.0.join("same-feature").exists());
    // A failed/no-op revision after regeneration removes its own output directory.
    assert_eq!(
        patch_request(
            &dir,
            &model,
            json!([]),
            edit("thickness", 4.0),
            "first-override",
            "r1"
        )
        .unwrap()["status"],
        "built"
    );
    let saved = ModelDocument::from_json(
        &fs::read_to_string(dir.0.join("first-override/model.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        patch_request(
            &dir,
            &saved,
            json!([]),
            edit("thickness", 4.0),
            "noop",
            "r2"
        )
        .unwrap_err()
        .stage,
        "revision"
    );
    assert!(!dir.0.join("noop").exists());
    assert_eq!(
        patch_request(
            &dir,
            &saved,
            json!([]),
            edit("thickness", 5.0),
            "duplicate-revision",
            "r1"
        )
        .unwrap_err()
        .stage,
        "revision"
    );
    assert!(!dir.0.join("duplicate-revision").exists());
}

#[test]
fn feature_history_edits_report_states_guard_snapshots_and_record_reorders() {
    let dir = Directory::new();
    let model = fixture();
    let ids: Vec<_> = model.family.features.iter().map(|f| f.id.clone()).collect();
    let head = ids.last().unwrap();
    let report = patch_request(
        &dir,
        &model,
        json!([
            {"action":"checkout_feature","family":"bracket","expected":{},"head":head}
        ]),
        json!([]),
        "checkout",
        "history-1",
    )
    .unwrap();
    assert!(
        report["feature_states"][0]["features"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["state"] == "active")
    );
    let saved =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("checkout/model.json")).unwrap())
            .unwrap();
    let stale = patch_request(
        &dir,
        &saved,
        json!([
            {"action":"checkout_feature","family":"bracket","expected":{},"head":null}
        ]),
        json!([]),
        "stale-history",
        "history-2",
    )
    .unwrap_err();
    assert_eq!(stale.stage, "conflict");
    assert!(!dir.0.join("stale-history").exists());
    let report = patch_request(&dir, &saved, json!([
        {"action":"suppress_feature","family":"bracket","expected":{"head":head},"feature":"body","suppressed":true}
    ]), json!([]), "suppressed", "history-2").unwrap();
    let state = report["feature_states"][0]["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["feature"] == "body")
        .unwrap();
    assert_eq!(state["state"], "suppressed");
    assert!(state["passes"].is_string());
    // Independent primitives may change position, and the revision records order.
    let report = patch_request(&dir, &model, json!([
        {"action":"move_feature","family":"bracket","feature":ids[1],"expected":ids,"after":null}
    ]), json!([]), "reordered", "move-1").unwrap();
    assert_eq!(report["status"], "built");
    let moved =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("reordered/model.json")).unwrap())
            .unwrap();
    assert_eq!(moved.family.features[0].id, ids[1]);
    assert!(
        moved.revisions[0]
            .changes
            .iter()
            .any(|c| c.path.last() == Some(&DocumentPathSegment::Field("feature_order".into())))
    );
    let invalid = patch_request(&dir, &model, json!([
        {"action":"move_feature","family":"bracket","feature":"body","expected":ids,"after":null}
    ]), json!([]), "invalid-move", "move-2").unwrap_err();
    assert_eq!(invalid.stage, "patch");
}

#[test]
fn timeline_branches_switch_geometry_bookmark_and_refuse_conflicting_merges() {
    let dir = Directory::new();
    let saved = |name: &str| {
        ModelDocument::from_json(&fs::read_to_string(dir.0.join(name).join("model.json")).unwrap())
            .unwrap()
    };
    let timeline = |model: &ModelDocument, action: Value| {
        let order: Vec<_> = model.family.features.iter().map(|f| f.id.clone()).collect();
        json!([{"action":"timeline","family":"bracket","expected":model.family.history,
                "expected_order":order,"timeline":action}])
    };
    let volume = |report: &Value| report["outputs"][0]["volume_mm3"].as_f64().unwrap();
    let model = fixture();
    let main_volume = volume(
        &patch_request(
            &dir,
            &model,
            json!([]),
            edit("thickness", 6.0),
            "start",
            "t0",
        )
        .unwrap(),
    );
    let model = saved("start");
    // A branch after the first hole, then make it: `body` keeps one hole.
    patch_request(
        &dir,
        &model,
        timeline(
            &model,
            json!({"kind":"create_branch","name":"one-hole","base":"first_hole"}),
        ),
        json!([]),
        "branched",
        "t1",
    )
    .unwrap();
    let model = saved("branched");
    patch_request(
        &dir,
        &model,
        timeline(&model, json!({"kind":"switch_branch","name":"one-hole"})),
        json!([]),
        "switched",
        "t2",
    )
    .unwrap();
    let model = saved("switched");
    assert_eq!(model.family.history.branch.as_deref(), Some("one-hole"));
    let body = model
        .family
        .features
        .iter()
        .find(|f| f.id == "body")
        .unwrap()
        .clone();
    let zero = json!({"literal":{"x":{"value":0.0,"dimension":"length","unit":"millimeter"},"y":{"value":0.0,"dimension":"length","unit":"millimeter"},"z":{"value":0.0,"dimension":"length","unit":"millimeter"}}});
    let report = patch_request(
        &dir,
        &model,
        json!([{"action":"replace_feature","family":"bracket","expected":body,
        "feature":{"id":"body","operation":{"translate":{"input":"first_hole","offset":zero}}}}]),
        json!([]),
        "one-hole",
        "t3",
    )
    .unwrap();
    let branch_volume = volume(&report);
    assert!(
        branch_volume > main_volume + 1.0,
        "{branch_volume} vs {main_volume}"
    );
    // Bookmark it, then back to main: the original part returns.
    let model = saved("one-hole");
    patch_request(
        &dir,
        &model,
        timeline(&model, json!({"kind":"bookmark","name":"one hole v1"})),
        json!([]),
        "bookmarked",
        "t4",
    )
    .unwrap();
    let model = saved("bookmarked");
    let report = patch_request(
        &dir,
        &model,
        timeline(&model, json!({"kind":"switch_branch","name":"main"})),
        json!([]),
        "main",
        "t5",
    )
    .unwrap();
    assert!((volume(&report) - main_volume).abs() < 1e-6);
    assert_eq!(report["feature_states"][0]["branch"], "main");
    assert_eq!(report["feature_states"][0]["branches"], json!(["one-hole"]));
    let model = saved("main");
    assert_eq!(model.family.history.branch, None);
    let stored = &model.family.history.branches[0];
    assert_eq!(
        (stored.name.as_str(), stored.bookmarks[0].name.as_str()),
        ("one-hole", "one hole v1")
    );
    // Both define `body` differently: the merge is refused with the conflict.
    let conflict = patch_request(
        &dir,
        &model,
        timeline(&model, json!({"kind":"merge_branch","name":"one-hole"})),
        json!([]),
        "merge",
        "t6",
    )
    .unwrap_err();
    assert_eq!(conflict.stage, "conflict");
    assert_eq!(conflict.details["branch_conflicts"][0]["feature"], "body");
    assert!(!dir.0.join("merge").exists());
    // Stale guards and mixed transactions are refused.
    let stale = timeline(&saved("start"), json!({"kind":"bookmark","name":"late"}));
    assert_eq!(
        patch_request(&dir, &model, stale, json!([]), "stale", "t7")
            .unwrap_err()
            .stage,
        "conflict"
    );
    let mut mixed = timeline(&model, json!({"kind":"bookmark","name":"x"}));
    mixed.as_array_mut().unwrap().push(json!({"action":"checkout_feature","family":"bracket","expected":model.family.history,"head":null}));
    assert_eq!(
        patch_request(&dir, &model, mixed, json!([]), "mixed", "t8")
            .unwrap_err()
            .stage,
        "request"
    );
}
