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
