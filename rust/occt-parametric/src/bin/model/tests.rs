use super::*;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
fn mm(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn size(x: f64, y: f64) -> VectorExpr {
    VectorExpr::Components {
        x: mm(x),
        y: mm(y),
        z: ScalarExpr::Parameter("thickness".into()),
    }
}
fn fixture() -> ModelDocument {
    let mut family = FamilyDefinition {
        id: "bracket".into(),
        version: 1,
        references: vec![],
        feature_colors: Default::default(),
        assumptions: vec![],
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        requirements: vec![],
        datums: vec![],
        features: vec![],
    };
    for (id, default) in [("thickness", 4.0), ("hole_spacing", 30.0)] {
        family.parameters.push(ParameterDefinition {
            id: id.into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(default, LengthUnit::Millimeter)),
            minimum: None,
            maximum: None,
        });
    }
    family.features = vec![
        FeatureDefinition {
            id: "base".into(),
            operation: FeatureOperation::Box {
                origin: point(0.0, 0.0, 0.0),
                size: size(60.0, 30.0),
            },
        },
        FeatureDefinition {
            id: "upright".into(),
            operation: FeatureOperation::Box {
                origin: point(0.0, 0.0, 0.0),
                size: VectorExpr::Components {
                    x: mm(60.0),
                    y: ScalarExpr::Parameter("thickness".into()),
                    z: mm(25.0),
                },
            },
        },
        FeatureDefinition {
            id: "blank".into(),
            operation: FeatureOperation::Fuse {
                left: "base".into(),
                right: "upright".into(),
            },
        },
    ];
    for (id, input, x) in [
        ("first_hole", "blank", mm(10.0)),
        (
            "body",
            "first_hole",
            ScalarExpr::Add(
                Box::new(mm(10.0)),
                Box::new(ScalarExpr::Parameter("hole_spacing".into())),
            ),
        ),
    ] {
        family.features.push(FeatureDefinition {
            id: id.into(),
            operation: FeatureOperation::Hole {
                bottom: HoleBottom::Flat,
                input: input.into(),
                position: VectorExpr::Components {
                    x,
                    y: mm(18.0),
                    z: mm(0.0),
                },
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                diameter: mm(5.0),
                extent: HoleExtent::ThroughAll,
                finish: HoleFinish::Plain,
                thread: None,
            },
        });
    }
    for (id, rule) in [
        (
            "valid",
            VerificationRule::ShapeValid {
                output: "body".into(),
            },
        ),
        (
            "wall",
            VerificationRule::MinimumWall {
                output: "body".into(),
                minimum: Quantity::length(2.0, LengthUnit::Millimeter),
                mesh: Default::default(),
                maximum_samples: 1000,
            },
        ),
    ] {
        family.requirements.push(Requirement {
            id: id.into(),
            version: 1,
            kind: RequirementKind::Validation,
            priority: RequirementPriority::Required,
            statement: id.into(),
            rule,
            provenance: "example; assumed 2 mm minimum for workflow demonstration".into(),
            traces: vec![TraceTarget::Feature("body".into())],
        });
    }
    let mut graph = InstanceGraph::new(&family);
    graph
        .add_base("bracket", HashMap::new(), "example")
        .unwrap();
    ModelDocument::from_graph(&graph)
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "occb-model-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn request(model: &ModelDocument, edits: Value) -> Value {
    json!({"schema":"occb-model-request-v1","model":model,"outputs":[{"instance":"bracket","output":"body"}],"edits":edits,"step":false,"stl":false,"preview":false})
}
fn invoke(dir: &Directory, value: Value, name: &str) -> Result<Value, Failure> {
    let p = dir.0.join("request.json");
    fs::write(&p, value.to_string()).unwrap();
    run(&[p.into_os_string(), dir.0.join(name).into_os_string()])
}
fn edit(parameter: &str, v: f64) -> Value {
    json!([{"instance":"bracket","parameter":parameter,"value":ParameterValue::Scalar(Quantity::length(v,LengthUnit::Millimeter))}])
}
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

fn inspect_request(dir: &Directory, value: Value, name: &str) -> Result<Value, Failure> {
    let source = dir.0.join("inspection.json");
    fs::write(&source, value.to_string()).unwrap();
    inspect::run(&source.into_os_string(), &dir.0.join(name).into_os_string())
}
#[test]
fn inspection_inventory_inheritance_paging_geometry_and_semantic_queries() {
    let dir = Directory::new();
    let mut model = fixture();
    let mut graph = model.instance_graph().unwrap();
    graph
        .add_clone("copy", "bracket", HashMap::new(), "test clone")
        .unwrap();
    graph
        .set_override(
            "copy",
            "thickness",
            ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    model.instances = ModelDocument::from_graph(&graph).instances;
    let original = model.to_json_pretty().unwrap();
    let inventory = inspect_request(
        &dir,
        json!({"schema":"occb-model-inspection-v1","model":model,"limit":1}),
        "inventory.json",
    )
    .unwrap();
    assert_eq!(inventory["instances"]["total"], 2);
    assert_eq!(inventory["instances"]["next_offset"], 1);
    assert_eq!(inventory["geometry_generated"], false);
    let part = inspect_request(
        &dir,
        json!({"schema":"occb-model-inspection-v1","model":model,"instance":"copy"}),
        "part.json",
    )
    .unwrap();
    let thickness = part["instance"]["resolved_parameters"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "thickness")
        .unwrap();
    assert_eq!(thickness["value"]["scalar"]["value"], 5.0);
    assert_eq!(
        part["instance"]["features"]["items"][2]["inputs"],
        json!(["base", "upright"])
    );
    let face = FaceSelector::NormalAligned {
        direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.99)),
    };
    let report = inspect_request(&dir,json!({"schema":"occb-model-inspection-v1","model":model,"instance":"copy","output":"base","face_selector":face,"limit":2}),"geometry.json").unwrap();
    assert_eq!(report["geometry"]["faces"]["total"], 1);
    assert_eq!(report["geometry"]["faces"]["items"][0]["area_mm2"], 1800.0);
    assert_eq!(
        report["geometry"]["edges"]["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(report["geometry"]["edges"]["next_offset"], 2);
    assert_eq!(report["geometry"]["volume_mm3"], 9000.0);
    assert_eq!(model.to_json_pretty().unwrap(), original);
    let mut referenced = model.clone();
    referenced.family.references.push(NamedReference {
        name: "top_base".into(),
        target: ReferenceTarget::Faces(FaceSelector::Persistent {
            feature: "base".into(),
            select: Box::new(face.clone()),
        }),
    });
    referenced.family.features.push(FeatureDefinition {
        id: "shell".into(),
        operation: FeatureOperation::Hollow {
            input: "body".into(),
            faces: vec![FaceSelector::Named("top_base".into())],
            thickness: mm(-0.5),
            tolerance: mm(1e-4),
        },
    });
    let metadata = inspect_request(
        &dir,
        json!({"schema":"occb-model-inspection-v1","model":referenced,"instance":"copy"}),
        "references.json",
    )
    .unwrap();
    assert_eq!(
        metadata["instance"]["features"]["items"][5]["inputs"],
        json!(["base", "body"])
    );
}
#[test]
fn inspection_queries_release_handles_and_use_local_authoring_coordinates() {
    let dir = Directory::new();
    let mut model = fixture();
    // The two sides of a box tie in area; normals uniquely select the top.
    let selector = FaceSelector::NormalAligned {
        direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.99)),
    };
    let mut graph = model.instance_graph().unwrap();
    graph
        .set_placement(
            "bracket",
            Placement {
                translation: VectorQuantity::lengths(100.0, 200.0, 300.0, LengthUnit::Millimeter),
                ..Default::default()
            },
        )
        .unwrap();
    model.instances = ModelDocument::from_graph(&graph).instances;
    let report=inspect_request(&dir,json!({"schema":"occb-model-inspection-v1","model":model,"instance":"bracket","output":"base","face_selector":selector}),"placed.json").unwrap();
    assert_eq!(
        report["geometry"]["bounds_mm"]["min"],
        json!([0.0, 0.0, 0.0])
    );
    assert_eq!(report["geometry"]["faces"]["total"], 1);
    let session = Session::new().unwrap();
    let part = model.instance_graph().unwrap().resolve("bracket").unwrap();
    let generated = part.regenerate(&session).unwrap();
    let baseline = session.shape_count().unwrap();
    for _ in 0..10 {
        let faces = part
            .select_faces(&session, &generated, "base", &selector)
            .unwrap();
        assert_eq!(faces.len(), 1);
        drop(faces);
        assert_eq!(session.shape_count().unwrap(), baseline);
        let edges = part
            .select_edges(
                &session,
                &generated,
                "body",
                &EdgeSelector::CircularRadius {
                    minimum: mm(2.49),
                    maximum: mm(2.51),
                },
            )
            .unwrap();
        assert_eq!(edges.len(), 4);
        for edge in &edges {
            assert_eq!(session.edge_circle_radius(edge).unwrap(), Some(2.5));
        }
        drop(edges);
        assert_eq!(session.shape_count().unwrap(), baseline);
        assert!(
            part.select_faces(&session, &generated, "missing", &selector)
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), baseline);
    }
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn inspection_limits_missing_references_and_report_overwrite_rejected() {
    let dir = Directory::new();
    let model = fixture();
    for (name, extra, stage) in [
        ("limit", json!({"limit":0}), "request"),
        ("output", json!({"output":"base"}), "request"),
        ("missing", json!({"instance":"missing"}), "resolution"),
        (
            "missing-output",
            json!({"instance":"bracket","output":"missing"}),
            "selection",
        ),
    ] {
        let mut req = json!({"schema":"occb-model-inspection-v1","model":model});
        req.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_eq!(inspect_request(&dir, req, name).unwrap_err().stage, stage);
        assert!(!dir.0.join(name).exists());
    }
    let request = json!({"schema":"occb-model-inspection-v1","model":model});
    inspect_request(&dir, request.clone(), "report.json").unwrap();
    let before = fs::read(dir.0.join("report.json")).unwrap();
    assert_eq!(
        inspect_request(&dir, request, "report.json")
            .unwrap_err()
            .stage,
        "publication"
    );
    assert_eq!(fs::read(dir.0.join("report.json")).unwrap(), before);
}

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

fn view_request(dir: &Directory, value: Value, name: &str) -> Result<Value, Failure> {
    let source = dir.0.join("view.request.json");
    fs::write(&source, value.to_string()).unwrap();
    visualize::run(&source.into_os_string(), &dir.0.join(name).into_os_string())
}
fn view_example(name: &str) -> Value {
    serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../tools/model/{name}.request.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
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
        "../../../../../tools/model/sketch-advanced.request.json"
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
