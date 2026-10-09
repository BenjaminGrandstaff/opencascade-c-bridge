//! Holed sketch sections transported along exact native paths.
use super::*;
use occt_bridge::{HistoryRelation, ShapeType};
fn document() -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/hollow-sweep.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}
fn part(document: &ModelDocument) -> PartInstance<'_> {
    PartInstance {
        id: "pipe".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
#[test]
fn hollow_path_sweeps_have_exact_volume_source_history_and_unmodified_profiles() {
    let session = Session::new().unwrap();
    let mut document = document();
    for orientation in [
        SweepOrientation::CorrectedFrenet,
        SweepOrientation::Frenet,
        SweepOrientation::Binormal {
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        },
    ] {
        let FeatureOperation::Sweep {
            orientation: mode, ..
        } = &mut document.family.features[0].operation
        else {
            panic!()
        };
        *mode = orientation;
        let generated = part(&document).regenerate(&session).unwrap();
        let body = generated.shape("body").unwrap();
        assert_eq!(session.shape_type(body).unwrap(), ShapeType::Solid);
        assert!(session.is_valid(body).unwrap());
        let expected = 3.0 * std::f64::consts::PI * (10.0 + 5.0 * std::f64::consts::PI);
        assert!((session.volume(body).unwrap() - expected).abs() < 1e-6);
        for id in ["outer", "inner"] {
            let source = generated.shape(id).unwrap();
            let edge = session.subshape(source, ShapeType::Edge, 0).unwrap();
            assert_eq!(session.subshape_count(source, ShapeType::Edge).unwrap(), 1);
            let count = session
                .history_count(body, &edge, HistoryRelation::Generated)
                .unwrap();
            assert!(count > 0, "{id} wall ancestry lost");
            for index in 0..count {
                let target = session
                    .history(body, &edge, HistoryRelation::Generated, index)
                    .unwrap();
                assert_eq!(session.shape_type(&target).unwrap(), ShapeType::Face);
            }
        }
        assert_eq!(
            session
                .subshape_count(generated.shape("profile").unwrap(), ShapeType::Wire)
                .unwrap(),
            2
        );
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
#[test]
fn bore_edits_reuse_outer_section_and_path_and_invalid_bends_keep_accepted_shapes() {
    let session = Session::new().unwrap();
    let document = document();
    let mut instance = part(&document);
    let first = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "inner_radius".into(),
        ParameterValue::Scalar(Quantity::length(1.5, LengthUnit::Millimeter)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.reused, vec!["outer", "path"]);
    assert_eq!(
        second.regeneration.rebuilt,
        vec!["inner", "profile", "body"]
    );
    assert!(
        (session.volume(second.shape("body").unwrap()).unwrap()
            - 1.75 * std::f64::consts::PI * (10.0 + 5.0 * std::f64::consts::PI))
            .abs()
            < 1e-6
    );
    let handles = session.shape_count().unwrap();
    instance.overrides.insert(
        "bend_radius".into(),
        ParameterValue::Scalar(Quantity::length(1.0, LengthUnit::Millimeter)),
    );
    assert!(instance.regenerate_incremental(&session, &second).is_err());
    assert!(session.is_valid(second.shape("body").unwrap()).unwrap());
    assert_eq!(session.shape_count().unwrap(), handles);
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    drop((first, second));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn fixed_straight_sweeps_support_multiple_displaced_bores_and_keep_wall_history() {
    let session = Session::new().unwrap();
    let mut document = document();
    document
        .family
        .parameters
        .iter_mut()
        .find(|p| p.id == "inner_radius")
        .unwrap()
        .default = ParameterValue::Scalar(Quantity::length(0.4, LengthUnit::Millimeter));
    let FeatureOperation::Sweep { orientation, .. } = &mut document.family.features[0].operation
    else {
        panic!()
    };
    *orientation = SweepOrientation::Fixed;
    let FeatureOperation::PlanarRegion { holes, .. } = &mut document.family.features[1].operation
    else {
        panic!()
    };
    *holes = vec!["upper-hole".into(), "lower-hole".into()];
    let FeatureOperation::SketchOpenWire { sketch } = &mut document.family.features[4].operation
    else {
        panic!()
    };
    sketch.arcs.clear();
    sketch.profile = vec!["run".into()];
    for (id, z) in [("upper-hole", 0.75), ("lower-hole", -0.75)] {
        document.family.features.push(FeatureDefinition {
            id: id.into(),
            operation: FeatureOperation::Translate {
                input: "inner".into(),
                offset: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    z,
                    LengthUnit::Millimeter,
                )),
            },
        });
    }
    let generated = part(&document).regenerate(&session).unwrap();
    let body = generated.shape("body").unwrap();
    assert_eq!(session.shape_type(body).unwrap(), ShapeType::Solid);
    assert!(session.is_valid(body).unwrap());
    assert!((session.volume(body).unwrap() - 36.8 * std::f64::consts::PI).abs() < 1e-6);
    for id in ["upper-hole", "lower-hole"] {
        let source = session
            .subshape(generated.shape(id).unwrap(), ShapeType::Edge, 0)
            .unwrap();
        assert!(
            session
                .history_count(body, &source, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
    }
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}
