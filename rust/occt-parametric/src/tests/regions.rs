//! Native hollow profiles from independent saved sketch boundaries.
use super::*;
use occt_bridge::{HistoryRelation, ShapeType};
fn document() -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/hollow-profile.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}
fn part(document: &ModelDocument) -> PartInstance<'_> {
    PartInstance {
        id: "tube".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
#[test]
fn native_annular_regions_extrude_exactly_and_retain_original_boundary_edges() {
    let document = document();
    let session = Session::new().unwrap();
    let generated = part(&document).regenerate(&session).unwrap();
    let region = generated.shape("region").unwrap();
    let body = generated.shape("body").unwrap();
    assert_eq!(session.shape_type(region).unwrap(), ShapeType::Face);
    assert_eq!(session.subshape_count(region, ShapeType::Wire).unwrap(), 2);
    assert!((session.surface_area(region).unwrap() - 48.0 * std::f64::consts::PI).abs() < 1e-7);
    assert!((session.volume(body).unwrap() - 960.0 * std::f64::consts::PI).abs() < 1e-6);
    assert!(session.is_valid(body).unwrap());
    for id in ["outer", "inner"] {
        let profile = generated.shape(id).unwrap();
        let edge = session.subshape(profile, ShapeType::Edge, 0).unwrap();
        assert_eq!(session.subshape_count(profile, ShapeType::Edge).unwrap(), 1);
        assert!(
            session
                .history_count(body, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0,
            "source {id} edge lost"
        );
    }
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn hole_edits_rebuild_the_region_and_body_while_reusing_the_outer_sketch() {
    let document = document();
    let session = Session::new().unwrap();
    let mut instance = part(&document);
    let first = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "inner_radius".into(),
        ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.reused, vec!["outer"]);
    assert_eq!(second.regeneration.rebuilt, vec!["inner", "region", "body"]);
    assert!(
        (session.volume(second.shape("body").unwrap()).unwrap() - 780.0 * std::f64::consts::PI)
            .abs()
            < 1e-6
    );
    let count = session.shape_count().unwrap();
    for invalid in [8.0, 9.0] {
        instance.overrides.insert(
            "inner_radius".into(),
            ParameterValue::Scalar(Quantity::length(invalid, LengthUnit::Millimeter)),
        );
        assert!(instance.regenerate_incremental(&session, &second).is_err());
        assert!(session.is_valid(second.shape("body").unwrap()).unwrap());
        assert_eq!(session.shape_count().unwrap(), count);
    }
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    drop((first, second));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn holed_planar_regions_revolve_into_hollow_toroidal_solids() {
    let mut document = document();
    for parameter in &mut document.family.parameters {
        if parameter.id == "outer_radius" {
            parameter.default =
                ParameterValue::Scalar(Quantity::length(1.0, LengthUnit::Millimeter));
        }
        if parameter.id == "inner_radius" {
            parameter.default =
                ParameterValue::Scalar(Quantity::length(0.5, LengthUnit::Millimeter));
        }
    }
    for feature in &mut document.family.features {
        if let FeatureOperation::SketchFace { sketch } | FeatureOperation::SketchWire { sketch } =
            &mut feature.operation
        {
            sketch.origin = VectorExpr::Literal(VectorQuantity::lengths(
                3.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            ));
            sketch.y_axis = VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0));
        }
    }
    let session = Session::new().unwrap();
    for angle in [
        std::f64::consts::TAU,
        std::f64::consts::FRAC_PI_2,
        -std::f64::consts::FRAC_PI_2,
    ] {
        document.family.features[0].operation = FeatureOperation::Revolve {
            input: "region".into(),
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(angle)),
            extent: RevolveExtent::Symmetric,
        };
        let generated = part(&document).regenerate(&session).unwrap();
        let body = generated.shape("body").unwrap();
        assert_eq!(session.shape_type(body).unwrap(), ShapeType::Solid);
        assert!(session.is_valid(body).unwrap());
        assert!(
            (session.volume(body).unwrap() - 2.25 * std::f64::consts::PI * angle.abs()).abs()
                < 1e-6
        );
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
