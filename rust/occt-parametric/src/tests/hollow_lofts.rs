//! Explicit inner section tracks for tapered native hollow transitions.
use super::*;
use occt_bridge::{HistoryRelation, ShapeType};
fn document() -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/hollow-loft.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}
fn part(document: &ModelDocument) -> PartInstance<'_> {
    PartInstance {
        id: "duct".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
#[test]
fn hollow_lofts_have_exact_frustum_volume_and_inner_outer_wall_history() {
    let session = Session::new().unwrap();
    let mut document = document();
    for ruled in [true, false] {
        let FeatureOperation::ProfileLoft { ruled: mode, .. } =
            &mut document.family.features[0].operation
        else {
            panic!()
        };
        *mode = ruled;
        let generated = part(&document).regenerate(&session).unwrap();
        let body = generated.shape("body").unwrap();
        assert_eq!(session.shape_type(body).unwrap(), ShapeType::Solid);
        assert!(session.is_valid(body).unwrap());
        assert!((session.volume(body).unwrap() - 560.0 * std::f64::consts::PI).abs() < 1e-6);
        for id in ["lower", "lower-bore"] {
            let source = generated.shape(id).unwrap();
            let edge = session.subshape(source, ShapeType::Edge, 0).unwrap();
            assert_eq!(session.subshape_count(source, ShapeType::Edge).unwrap(), 1);
            let count = session
                .history_count(body, &edge, HistoryRelation::Generated)
                .unwrap();
            assert!(count > 0, "{id} wall history lost");
            for index in 0..count {
                let face = session
                    .history(body, &edge, HistoryRelation::Generated, index)
                    .unwrap();
                assert_eq!(session.shape_type(&face).unwrap(), ShapeType::Face);
            }
        }
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
#[test]
fn bore_edits_rebuild_only_the_changed_section_and_loft_and_keep_accepted_results_on_failure() {
    let session = Session::new().unwrap();
    let document = document();
    let mut instance = part(&document);
    let first = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "lower_bore_radius".into(),
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(
        second.regeneration.reused,
        vec!["lower", "upper", "upper-bore"]
    );
    assert_eq!(second.regeneration.rebuilt, vec!["lower-bore", "body"]);
    assert!(
        (session.volume(second.shape("body").unwrap()).unwrap() - 620.0 * std::f64::consts::PI)
            .abs()
            < 1e-6
    );
    let count = session.shape_count().unwrap();
    instance.overrides.insert(
        "lower_bore_radius".into(),
        ParameterValue::Scalar(Quantity::length(8.0, LengthUnit::Millimeter)),
    );
    assert!(instance.regenerate_incremental(&session, &second).is_err());
    assert!(session.is_valid(second.shape("body").unwrap()).unwrap());
    assert_eq!(session.shape_count().unwrap(), count);
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    drop((first, second));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn mismatched_counts_duplicate_tracks_and_shifted_section_planes_are_rejected_without_leaks() {
    let session = Session::new().unwrap();
    for holes in [
        vec![vec![]],
        vec![vec!["lower-bore"]],
        vec![
            vec!["lower-bore", "upper-bore"],
            vec!["lower-bore", "upper-bore"],
        ],
        vec![vec!["lower", "upper-bore"]],
    ] {
        let mut document = document();
        let FeatureOperation::ProfileLoft { holes: tracks, .. } =
            &mut document.family.features[0].operation
        else {
            panic!()
        };
        *tracks = holes
            .into_iter()
            .map(|t| t.into_iter().map(str::to_owned).collect())
            .collect();
        assert!(part(&document).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut document = document();
    let FeatureOperation::SketchWire { sketch } = &mut document.family.features[4].operation else {
        panic!()
    };
    sketch.origin = VectorExpr::Literal(VectorQuantity::lengths(
        0.0,
        0.0,
        15.0,
        LengthUnit::Millimeter,
    ));
    let error = part(&document).regenerate(&session).err().unwrap();
    assert!(
        error.message.contains("loft section 1"),
        "{}",
        error.message
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn crossing_hole_tracks_fail_even_when_their_station_profiles_are_disjoint() {
    let session = Session::new().unwrap();
    let mut document = document();
    for p in &mut document.family.parameters {
        if p.id == "lower_bore_radius" || p.id == "upper_bore_radius" {
            p.default = ParameterValue::Scalar(Quantity::length(0.5, LengthUnit::Millimeter));
        }
    }
    for (id, input, x) in [
        ("a-lower", "lower-bore", -2.0),
        ("a-upper", "upper-bore", 2.0),
        ("b-lower", "lower-bore", 2.0),
        ("b-upper", "upper-bore", -2.0),
    ] {
        document.family.features.push(FeatureDefinition {
            id: id.into(),
            operation: FeatureOperation::Translate {
                input: input.into(),
                offset: VectorExpr::Literal(VectorQuantity::lengths(
                    x,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
        });
    }
    let FeatureOperation::ProfileLoft { holes, .. } = &mut document.family.features[0].operation
    else {
        panic!()
    };
    *holes = vec![
        vec!["a-lower".into(), "a-upper".into()],
        vec!["b-lower".into(), "b-upper".into()],
    ];
    let error = part(&document).regenerate(&session).err().unwrap();
    assert!(
        error.message.contains("overlap or touch"),
        "{}",
        error.message
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn older_profile_lofts_without_hole_tracks_keep_their_solid_behavior() {
    let mut json: serde_json::Value =
        serde_json::from_str(&document().to_json_pretty().unwrap()).unwrap();
    json["schema_version"] = serde_json::json!(79);
    json["family"]["features"][0]["operation"]["profile_loft"]
        .as_object_mut()
        .unwrap()
        .remove("holes");
    let document = ModelDocument::from_json(&json.to_string()).unwrap();
    assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
    let FeatureOperation::ProfileLoft { holes, .. } = &document.family.features[0].operation else {
        panic!()
    };
    assert!(holes.is_empty());
    let session = Session::new().unwrap();
    let generated = part(&document).regenerate(&session).unwrap();
    assert!(
        (session.volume(generated.shape("body").unwrap()).unwrap()
            - 20.0 * std::f64::consts::PI * 112.0 / 3.0)
            .abs()
            < 1e-6
    );
}
