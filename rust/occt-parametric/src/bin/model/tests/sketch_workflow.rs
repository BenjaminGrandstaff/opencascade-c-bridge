use super::*;

fn model() -> ModelDocument {
    ModelDocument::from_json(&view_example("reliable-sketch")["model"].to_string()).unwrap()
}
fn definition(model: &ModelDocument) -> &SketchDefinition {
    let FeatureOperation::SketchFace { sketch } = &model.family.features[0].operation else {
        panic!()
    };
    sketch
}
fn change(model: &ModelDocument, operations: Value) -> Value {
    json!([{"action":"edit_sketch","family":model.family.id,"feature":"profile","expected":definition(model),"changes":operations}])
}
fn edit(
    dir: &Directory,
    model: &ModelDocument,
    changes: Value,
    name: &str,
    preview: bool,
) -> Result<Value, Failure> {
    let p = dir.0.join("edit-sketch.json");
    fs::write(&p,json!({"schema":"occb-model-edit-v1","model":model,"changes":changes,"outputs":[{"instance":"part","output":"body"}],"revision":{"id":"sketch-repair-1","author":"test","recorded_at":"test-time","message":"guarded sketch repair"},"step":false,"preview":false}).to_string()).unwrap();
    if preview {
        patch::preview(&p.into_os_string(), &dir.0.join(name).into_os_string())
    } else {
        patch::run(&p.into_os_string(), &dir.0.join(name).into_os_string())
    }
}
fn inspect_sketch(dir: &Directory, model: &ModelDocument, name: &str) -> Value {
    let p = dir.0.join("inspect-sketch.json");
    fs::write(&p,json!({"schema":"occb-model-inspection-v1","model":model,"instance":"part","sketch_feature":"profile","limit":2}).to_string()).unwrap();
    inspect::run(&p.into_os_string(), &dir.0.join(name).into_os_string()).unwrap()
}

#[test]
fn sketch_diagnose_preview_repair_publish_preserves_source_requirements_and_revision() {
    let dir = Directory::new();
    let mut model = model();
    let duplicate = definition(&model).constraints[0].clone();
    let FeatureOperation::SketchFace { sketch } = &mut model.family.features[0].operation else {
        panic!()
    };
    sketch.constraints.push(duplicate.clone());
    let before = model.to_json_pretty().unwrap();
    let diagnosis = inspect_sketch(&dir, &model, "diagnosis.json");
    assert_eq!(diagnosis["sketch"]["solver"]["solved"], true);
    assert_eq!(diagnosis["sketch"]["native_profile"]["valid"], true);
    assert_eq!(diagnosis["sketch"]["constraints"]["next_offset"], 2);
    let candidate = diagnosis["sketch"]["repair_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "remove_locally_redundant_constraint")
        .unwrap();
    let operations = json!([candidate["change"]]);
    let changes = change(&model, operations);
    let report = edit(&dir, &model, changes.clone(), "preview", true).unwrap();
    assert_eq!(report["status"], "previewed");
    assert_eq!(report["accepted"], false);
    let preview =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("preview/model.json")).unwrap())
            .unwrap();
    assert_eq!(preview.revisions, model.revisions);
    assert_eq!(preview.family.requirements, model.family.requirements);
    assert!(!dir.0.join("preview/changes.json").exists());
    let accepted = edit(&dir, &model, changes, "accepted", false).unwrap();
    assert_eq!(accepted["status"], "built");
    assert_eq!(accepted["revision"]["id"], "sketch-repair-1");
    let saved =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("accepted/model.json")).unwrap())
            .unwrap();
    assert_eq!(saved.family.version, model.family.version + 1);
    assert_eq!(saved.revisions.len(), 1);
    assert_eq!(saved.family.requirements, model.family.requirements);
    assert_eq!(
        definition(&saved).constraints.len() + 1,
        definition(&model).constraints.len()
    );
    assert_eq!(model.to_json_pretty().unwrap(), before);
}

#[test]
fn failed_sketch_preview_stays_diagnostic_and_failed_publication_is_atomic() {
    let dir = Directory::new();
    let model = model();
    let before = model.to_json_pretty().unwrap();
    let changes = change(
        &model,
        json!([{"action":"add_constraint","constraint":{"horizontal_distance":{"first":"A","second":"B","value":mm(41.)}}}]),
    );
    let preview = edit(&dir, &model, changes.clone(), "bad-preview", true).unwrap();
    assert_eq!(preview["accepted"], false);
    let view: Value =
        serde_json::from_str(&fs::read_to_string(dir.0.join("bad-preview/view.json")).unwrap())
            .unwrap();
    let sketch = view["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "sketch")
        .unwrap();
    assert_eq!(sketch["solver"]["accepted"], false);
    assert!(edit(&dir, &model, changes, "bad-publication", false).is_err());
    assert!(!dir.0.join("bad-publication").exists());
    let candidate = ModelDocument::from_json(
        &fs::read_to_string(dir.0.join("bad-preview/model.json")).unwrap(),
    )
    .unwrap();
    let diagnosis = inspect_sketch(&dir, &candidate, "conflict.json");
    assert_eq!(diagnosis["sketch"]["solver"]["solved"], false);
    assert!(
        diagnosis["sketch"]["repair_candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "review_conflict_group")
    );
    assert_eq!(model.to_json_pretty().unwrap(), before);
}

#[test]
fn stale_element_guards_and_dangling_removals_do_not_publish() {
    let dir = Directory::new();
    let model = model();
    let point = definition(&model).points[1].clone();
    let mut stale = point.clone();
    stale.x = mm(99.);
    let changes = change(
        &model,
        json!([{"action":"upsert_element","expected":{"point":stale},"element":{"point":point}}]),
    );
    assert_eq!(
        edit(&dir, &model, changes, "stale", false)
            .unwrap_err()
            .stage,
        "conflict"
    );
    assert!(!dir.0.join("stale").exists());
    let changes = change(
        &model,
        json!([{"action":"remove_element","expected":{"point":point}}]),
    );
    assert!(edit(&dir, &model, changes, "dangling", false).is_err());
    assert!(!dir.0.join("dangling").exists());
    let mut replacement = point.clone();
    replacement.id = "renamed".into();
    let changes = change(
        &model,
        json!([{"action":"upsert_element","expected":{"point":point},"element":{"point":replacement}}]),
    );
    assert_eq!(
        edit(&dir, &model, changes, "rename", false)
            .unwrap_err()
            .stage,
        "patch"
    );
}

#[test]
fn point_configuration_and_constraint_edits_keep_identity_and_parameter_links() {
    let dir = Directory::new();
    let model = model();
    let old = definition(&model).points[1].clone();
    let mut point = old.clone();
    point.x = mm(1.);
    point.y = mm(2.);
    let config = patch::sketch::SketchConfiguration::from(definition(&model));
    let mut changed = config.clone();
    changed.acceptance = SketchAcceptance::Solved;
    let constraint = definition(&model).constraints[4].clone();
    let changes = change(
        &model,
        json!([
            {"action":"upsert_element","expected":{"point":old},"element":{"point":point}},
            {"action":"configure","expected":config,"configuration":changed},
            {"action":"replace_constraint","index":4,"expected":constraint,"constraint":{"horizontal_distance":{"first":"A","second":"B","value":{"add":[{"parameter":"width"},mm(1.)]}}}}
        ]),
    );
    let report = edit(&dir, &model, changes, "edited", false).unwrap();
    assert!(
        (report["outputs"][0]["bounds_mm"]["max"][0]
            .as_f64()
            .unwrap()
            - 41.)
            .abs()
            < 1e-7
    );
    let saved =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("edited/model.json")).unwrap())
            .unwrap();
    assert_eq!(definition(&saved).id, definition(&model).id);
    assert_eq!(definition(&saved).points[1], point);
    assert_eq!(saved.family.requirements, model.family.requirements);
}

#[test]
fn sketch_inspection_ignores_unrelated_failure_and_resolves_linked_dependencies() {
    let dir = Directory::new();
    let mut model = model();
    // An unrelated invalid native box must not prevent diagnosing this sketch.
    model.family.features.insert(
        0,
        FeatureDefinition {
            id: "aaa-failure".into(),
            operation: FeatureOperation::Box {
                origin: point(0., 0., 0.),
                size: point(-1., 2., 3.),
            },
        },
    );
    let result = inspect_sketch(&dir, &model, "isolated.json");
    assert_eq!(result["sketch"]["native_profile"]["valid"], true);
    let linked =
        ModelDocument::from_json(&view_example("face-pocket")["model"].to_string()).unwrap();
    let p = dir.0.join("linked-inspection.json");
    fs::write(&p,json!({"schema":"occb-model-inspection-v1","model":linked,"instance":linked.instances[0].id(),"sketch_feature":"profile"}).to_string()).unwrap();
    let result = inspect::run(
        &p.into_os_string(),
        &dir.0.join("linked.json").into_os_string(),
    )
    .unwrap();
    assert_eq!(result["sketch"]["dependency_geometry_generated"], true);
    assert_eq!(result["sketch"]["native_profile"]["valid"], true);
}

#[test]
fn sketch_and_named_reference_repairs_can_be_committed_in_one_guarded_transaction() {
    let dir = Directory::new();
    let model =
        ModelDocument::from_json(&view_example("stable-sketch-references")["model"].to_string())
            .unwrap();
    let old = patch::sketch::SketchConfiguration::from(definition(&model));
    let mut config = old.clone();
    let SketchCurveOperation::Split { after, .. } = &mut config.curve_operations[1] else {
        panic!()
    };
    *after = "new-tail".into();
    for id in &mut config.profile {
        if id == "bottom-right" {
            *id = "new-tail".into();
        }
    }
    let changes = change(
        &model,
        json!([{"action":"configure","expected":old,"configuration":config}]),
    );
    assert!(edit(&dir, &model, changes.clone(), "lost-reference", false).is_err());
    let expected = model.family.references[1].clone();
    let mut reference = expected.clone();
    let ReferenceTarget::Faces(FaceSelector::GeneratedFromEdges { source, .. }) =
        &mut reference.target
    else {
        panic!()
    };
    **source = EdgeSelector::SketchCurve {
        entity: "new-tail".into(),
        require_complete: true,
    };
    let mut changes = changes.as_array().unwrap().clone();
    changes.push(json!({"action":"replace_reference","family":model.family.id,"expected":expected,"reference":reference}));
    edit(&dir, &model, json!(changes), "repaired-reference", false).unwrap();
    let saved = ModelDocument::from_json(
        &fs::read_to_string(dir.0.join("repaired-reference/model.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved.family.references[1], reference);
    assert_eq!(saved.family.references[0], model.family.references[0]);
    assert_eq!(saved.family.requirements, model.family.requirements);
}

#[test]
fn unused_references_can_be_removed_but_used_reference_removal_is_rejected() {
    let dir = Directory::new();
    let mut model = model();
    let reference = NamedReference {
        name: "unused".into(),
        target: ReferenceTarget::Faces(FaceSelector::Persistent {
            feature: "profile".into(),
            select: Box::new(FaceSelector::SketchRegion {
                witnesses: vec!["A".into()],
                excludes: Vec::new(),
            }),
        }),
    };
    model.family.references.push(reference.clone());
    edit(
        &dir,
        &model,
        json!([{"action":"remove_reference","family":model.family.id,"expected":reference}]),
        "removed-unused",
        false,
    )
    .unwrap();
    let saved = ModelDocument::from_json(
        &fs::read_to_string(dir.0.join("removed-unused/model.json")).unwrap(),
    )
    .unwrap();
    assert!(saved.family.references.is_empty());
    let linked =
        ModelDocument::from_json(&view_example("stable-sketch-references")["model"].to_string())
            .unwrap();
    assert!(edit(&dir,&linked,json!([{"action":"remove_reference","family":linked.family.id,"expected":linked.family.references[1]}]),"removed-used",false).is_err());
    assert!(!dir.0.join("removed-used").exists());
}

#[test]
fn preview_and_publication_enforce_the_same_output_selection_contract() {
    let dir = Directory::new();
    let model = model();
    let changes = change(
        &model,
        json!([{"action":"add_constraint","constraint":definition(&model).constraints[0]}]),
    );
    let p = dir.0.join("duplicate-outputs.json");
    fs::write(&p,json!({"schema":"occb-model-edit-v1","model":model,"changes":changes,"outputs":[{"instance":"part","output":"body"},{"instance":"part","output":"profile"}],"revision":{"id":"duplicate-1","author":"test","recorded_at":"test-time","message":"selection contract"},"step":false,"preview":false}).to_string()).unwrap();
    for (name, preview) in [("invalid-preview", true), ("invalid-publication", false)] {
        let result = if preview {
            patch::preview(
                &p.clone().into_os_string(),
                &dir.0.join(name).into_os_string(),
            )
        } else {
            patch::run(
                &p.clone().into_os_string(),
                &dir.0.join(name).into_os_string(),
            )
        };
        let error = result.unwrap_err();
        assert_eq!(error.stage, "request");
        assert!(error.message.contains("one output per instance"));
        assert!(!dir.0.join(name).exists());
    }
}
