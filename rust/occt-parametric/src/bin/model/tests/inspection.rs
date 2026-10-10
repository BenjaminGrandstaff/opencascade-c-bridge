//! Read-only model inspection requests.

use super::*;

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
