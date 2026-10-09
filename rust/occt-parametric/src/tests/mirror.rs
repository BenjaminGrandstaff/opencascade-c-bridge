use super::*;
use occt_bridge::{HistoryRelation, ShapeType};
fn document() -> ModelDocument {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/mirrored-part.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&r["model"].to_string()).unwrap()
}
fn part(document: &ModelDocument) -> PartInstance<'_> {
    PartInstance {
        id: "bracket".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
#[test]
fn mirrored_parts_preserve_volume_and_source_face_ancestry_and_plane_edits_reuse_inputs() {
    let session = Session::new().unwrap();
    let document = document();
    let mut instance = part(&document);
    let first = instance.regenerate(&session).unwrap();
    let source = first.shape("source").unwrap();
    let body = first.shape("body").unwrap();
    assert!(session.is_valid(body).unwrap());
    assert!((session.volume(body).unwrap() - 1920.0).abs() < 1e-6);
    let bounds = session.exact_bounds(body).unwrap();
    assert!((bounds.min.x + 25.0).abs() < 1e-7 && (bounds.max.x + 5.0).abs() < 1e-7);
    let face = session.subshape(source, ShapeType::Face, 0).unwrap();
    assert!(
        session
            .history_count(body, &face, HistoryRelation::Modified)
            .unwrap()
            > 0
    );
    instance.overrides.insert(
        "plane_x".into(),
        ParameterValue::Scalar(Quantity::length(2.0, LengthUnit::Millimeter)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.reused, vec!["base", "tab", "source"]);
    assert_eq!(second.regeneration.rebuilt, vec!["body"]);
    let bounds = session.exact_bounds(second.shape("body").unwrap()).unwrap();
    assert!((bounds.min.x + 21.0).abs() < 1e-7 && (bounds.max.x + 1.0).abs() < 1e-7);
    assert!((session.exact_bounds(source).unwrap().min.x - 5.0).abs() < 1e-7);
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    drop((face, first, second));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn mirror_requires_length_origins_and_dimensionless_nonzero_normals() {
    let session = Session::new().unwrap();
    for (origin, normal) in [
        (
            VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0)),
            VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        ),
        (
            VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            VectorExpr::Literal(VectorQuantity::lengths(
                1.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        ),
        (
            VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0)),
        ),
    ] {
        let mut document = document();
        let FeatureOperation::Mirror {
            origin: o,
            normal: n,
            ..
        } = &mut document.family.features[0].operation
        else {
            panic!()
        };
        *o = origin;
        *n = normal;
        assert!(part(&document).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
