use super::*;
use occt_bridge::{HistoryRelation, ShapeType};
fn document() -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/multi-hole-plate.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}
#[test]
fn compound_tools_cut_nine_holes_preserve_child_identity_and_rebuild_after_radius_edits() {
    let document = document();
    let session = Session::new().unwrap();
    let mut instance = PartInstance {
        id: "plate".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let first = instance.regenerate(&session).unwrap();
    let tools = first.shape("tools").unwrap();
    assert_eq!(session.shape_type(tools).unwrap(), ShapeType::Compound);
    assert_eq!(session.subshape_count(tools, ShapeType::Solid).unwrap(), 9);
    let face = session
        .subshape(first.shape("tool-0-0").unwrap(), ShapeType::Face, 0)
        .unwrap();
    let faces = session.subshapes(tools, ShapeType::Face).unwrap();
    assert!(faces.iter().any(|f| session.is_same(&face, f).unwrap()));
    drop(faces);
    let body = first.shape("body").unwrap();
    assert!(session.is_valid(body).unwrap());
    let expected = 30.0 * 30.0 * 4.0 - 9.0 * std::f64::consts::PI * 2.0_f64.powi(2) * 4.0;
    assert!((session.volume(body).unwrap() - expected).abs() < 1e-5);
    assert!(
        session
            .history_count(body, &face, HistoryRelation::Modified)
            .unwrap()
            + session
                .history_count(body, &face, HistoryRelation::Generated)
                .unwrap()
            > 0
    );
    instance.overrides.insert(
        "hole_radius".into(),
        ParameterValue::Scalar(Quantity::length(2.5, LengthUnit::Millimeter)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.reused, vec!["plate"]);
    assert_eq!(edited.regeneration.rebuilt.len(), 11);
    let expected = 30.0 * 30.0 * 4.0 - 9.0 * std::f64::consts::PI * 2.5_f64.powi(2) * 4.0;
    assert!((session.volume(edited.shape("body").unwrap()).unwrap() - expected).abs() < 1e-5);
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    let count = session.shape_count().unwrap();
    instance.overrides.insert(
        "hole_radius".into(),
        ParameterValue::Scalar(Quantity::length(-1.0, LengthUnit::Millimeter)),
    );
    assert!(instance.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    drop((face, first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn compound_rejects_empty_duplicate_oversized_and_missing_input_sets_without_leaks() {
    let session = Session::new().unwrap();
    for inputs in [
        vec![],
        vec!["plate".to_string(); 2],
        vec!["plate".to_string(); 10001],
        vec!["missing".into()],
    ] {
        let mut document = document();
        if let FeatureOperation::Compound { inputs: children } =
            &mut document.family.features[10].operation
        {
            *children = inputs;
        } else {
            panic!("expected compound");
        }
        assert!(
            PartInstance {
                id: "plate".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "test".into()
            }
            .regenerate(&session)
            .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
