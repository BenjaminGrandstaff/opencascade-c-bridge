use super::*;
use occt_bridge::{HistoryRelation, ShapeType};

#[test]
fn nested_and_circular_patterns_retain_all_bore_faces_from_the_original_cutter() {
    let session = Session::new().unwrap();
    for (example, count) in [
        (
            include_str!("../../../../tools/model/patterned-plate.request.json"),
            9,
        ),
        (
            include_str!("../../../../tools/model/bolt-circle.request.json"),
            6,
        ),
    ] {
        let request: serde_json::Value = serde_json::from_str(example).unwrap();
        let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let part = PartInstance {
            id: "plate".into(),
            definition: &document.family,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let result = part.regenerate(&session).unwrap();
        let face = session
            .subshape(result.shape("cutter").unwrap(), ShapeType::Face, 0)
            .unwrap();
        let tools = result.shape("tools").unwrap();
        let body = result.shape("body").unwrap();
        assert_eq!(
            session
                .history_count(tools, &face, HistoryRelation::Modified)
                .unwrap(),
            count
        );
        assert_eq!(
            session
                .history_count(body, &face, HistoryRelation::Modified)
                .unwrap(),
            count
        );
        let selector = FaceSelector::History {
            source_feature: "cutter".into(),
            source: Box::new(FaceSelector::LargestArea {
                planar_only: false,
                allow_ties: false,
                relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-9)),
            }),
            relation: SemanticHistoryRelation::Modified,
        };
        let definitions = Features::new(&document.family);
        let selected = resolve_face_selector(
            &session,
            body,
            &selector,
            &part.resolved_parameters().unwrap(),
            &result.shapes,
            &definitions,
        )
        .unwrap();
        assert_eq!(selected.len(), count);
        for selected_face in &selected {
            assert!(
                (0..session.subshape_count(body, ShapeType::Face).unwrap()).any(|i| {
                    let member = session.subshape(body, ShapeType::Face, i).unwrap();
                    session.is_same(&member, selected_face).unwrap()
                })
            );
        }
        cleanup_shapes(&session, selected);
        drop((face, result));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn grouped_operand_history_survives_common_and_union() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/bolt-circle.request.json"
    ))
    .unwrap();
    let mut d = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    d.family.features.push(FeatureDefinition {
        id: "intersection".into(),
        operation: FeatureOperation::Common {
            left: "tools".into(),
            right: "plate".into(),
        },
    });
    d.family.features.push(FeatureDefinition {
        id: "union".into(),
        operation: FeatureOperation::Fuse {
            left: "tools".into(),
            right: "plate".into(),
        },
    });
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "plate".into(),
        definition: &d.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let result = part.regenerate(&session).unwrap();
    let face = session
        .subshape(result.shape("cutter").unwrap(), ShapeType::Face, 0)
        .unwrap();
    let common = result.shape("intersection").unwrap();
    assert_eq!(
        session
            .history_count(common, &face, HistoryRelation::Modified)
            .unwrap(),
        6
    );
    assert!((session.volume(common).unwrap() - 96.0 * std::f64::consts::PI).abs() < 1e-5);
    let union = result.shape("union").unwrap();
    assert!(session.is_valid(union).unwrap());
    assert_eq!(
        session
            .history_count(union, &face, HistoryRelation::Modified)
            .unwrap(),
        12
    );
    assert!((session.volume(union).unwrap() - 1648.0 * std::f64::consts::PI).abs() < 1e-5);
    drop((face, result));
    assert_eq!(session.shape_count().unwrap(), 0);
}
