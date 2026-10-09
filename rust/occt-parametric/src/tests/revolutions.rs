//! Symmetric signed revolutions retain profile identity and incremental behavior.
use super::*;
use occt_bridge::{HistoryRelation, ShapeType};

fn document() -> ModelDocument {
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/revolved-ring.request.json"
    ))
    .unwrap();
    request["model"]["family"]["features"][0]["operation"]["revolve"]["extent"] =
        serde_json::json!("symmetric");
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}

fn part(document: &ModelDocument) -> PartInstance<'_> {
    PartInstance {
        id: "ring".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

#[test]
fn symmetric_revolves_preserve_volume_centered_bounds_source_history_and_inputs() {
    let session = Session::new().unwrap();
    for wire in [false, true] {
        for angle in [
            std::f64::consts::FRAC_PI_2,
            -std::f64::consts::FRAC_PI_2,
            std::f64::consts::TAU,
        ] {
            let mut document = document();
            if wire {
                let FeatureOperation::SketchFace { sketch } =
                    document.family.features[1].operation.clone()
                else {
                    panic!()
                };
                document.family.features[1].operation = FeatureOperation::SketchWire { sketch };
            }
            let mut instance = part(&document);
            instance.overrides.insert(
                "angle".into(),
                ParameterValue::Scalar(Quantity::scalar(angle)),
            );
            let generated = instance.regenerate(&session).unwrap();
            let body = generated.shape("body").unwrap();
            assert_eq!(session.shape_type(body).unwrap(), ShapeType::Solid);
            assert!(session.is_valid(body).unwrap());
            assert!((session.volume(body).unwrap() - 140.0 * angle.abs()).abs() < 1e-6);
            let bounds = session.exact_bounds(body).unwrap();
            assert!((bounds.min.y + bounds.max.y).abs() < 1e-7);
            if angle.abs() < std::f64::consts::PI {
                assert!((bounds.min.x - 6.0 * (angle.abs() / 2.0).cos()).abs() < 1e-7);
                assert!((bounds.max.x - 8.0).abs() < 1e-7);
                assert!((bounds.max.y - 8.0 * (angle.abs() / 2.0).sin()).abs() < 1e-7);
            }
            let profile = generated.shape("profile").unwrap();
            let source = session.subshape(profile, ShapeType::Edge, 0).unwrap();
            assert!(
                session
                    .history_count(body, &source, HistoryRelation::Generated)
                    .unwrap()
                    > 0
            );
            let original = session.exact_bounds(profile).unwrap();
            assert!((original.min.x - 6.0).abs() < 1e-7 && (original.max.x - 8.0).abs() < 1e-7);
            assert!(original.min.y.abs() < 1e-7 && original.max.y.abs() < 1e-7);
            assert_eq!(session.subshape_count(profile, ShapeType::Edge).unwrap(), 4);
            drop((source, generated));
            assert_eq!(session.shape_count().unwrap(), 0);
        }
    }
}

#[test]
fn symmetric_angle_edits_reuse_profiles_and_failed_edits_keep_accepted_geometry() {
    let session = Session::new().unwrap();
    let mut document = document();
    document
        .family
        .parameters
        .iter_mut()
        .find(|p| p.id == "angle")
        .unwrap()
        .maximum = None;
    let mut instance = part(&document);
    instance.overrides.insert(
        "angle".into(),
        ParameterValue::Scalar(Quantity::scalar(1.0)),
    );
    let first = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "angle".into(),
        ParameterValue::Scalar(Quantity::scalar(-2.0)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.reused, vec!["profile"]);
    assert_eq!(second.regeneration.rebuilt, vec!["body"]);
    assert!((session.volume(second.shape("body").unwrap()).unwrap() - 280.0).abs() < 1e-6);
    let accepted_handles = session.shape_count().unwrap();
    for angle in [0.0, 7.0] {
        instance.overrides.insert(
            "angle".into(),
            ParameterValue::Scalar(Quantity::scalar(angle)),
        );
        assert!(instance.regenerate_incremental(&session, &second).is_err());
        assert!(session.is_valid(second.shape("body").unwrap()).unwrap());
        assert_eq!(session.shape_count().unwrap(), accepted_handles);
    }
    let mut alternate = document.clone();
    let FeatureOperation::Revolve { extent, .. } = &mut alternate.family.features[0].operation
    else {
        panic!()
    };
    *extent = RevolveExtent::Angle;
    let mut one_sided = part(&alternate);
    one_sided.overrides.insert(
        "angle".into(),
        ParameterValue::Scalar(Quantity::scalar(-2.0)),
    );
    let third = one_sided.regenerate_incremental(&session, &second).unwrap();
    assert_eq!(third.regeneration.reused, vec!["profile"]);
    assert_eq!(third.regeneration.rebuilt, vec!["body"]);
    let bounds = session.exact_bounds(third.shape("body").unwrap()).unwrap();
    assert!(bounds.max.y.abs() < 1e-7 && bounds.min.y < -7.9);
    drop((first, second, third));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn schema_78_persists_symmetric_revolves_and_old_documents_keep_one_sided_behavior() {
    let document = document();
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
    let mut old: serde_json::Value =
        serde_json::from_str(&document.to_json_pretty().unwrap()).unwrap();
    old["schema_version"] = serde_json::json!(77);
    old["family"]["features"][0]["operation"]["revolve"]
        .as_object_mut()
        .unwrap()
        .remove("extent");
    let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    assert!(matches!(
        migrated.family.features[0].operation,
        FeatureOperation::Revolve {
            extent: RevolveExtent::Angle,
            ..
        }
    ));
    let session = Session::new().unwrap();
    let mut instance = part(&migrated);
    instance.overrides.insert(
        "angle".into(),
        ParameterValue::Scalar(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
    );
    let generated = instance.regenerate(&session).unwrap();
    let bounds = session
        .exact_bounds(generated.shape("body").unwrap())
        .unwrap();
    assert!(bounds.min.x.abs() < 1e-7 && bounds.min.y.abs() < 1e-7);
    assert!((bounds.max.x - 8.0).abs() < 1e-7 && (bounds.max.y - 8.0).abs() < 1e-7);
}
