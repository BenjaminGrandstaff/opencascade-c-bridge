//! Catalog standard parts (schema 180): exact volumes and placement of
//! screws, nuts, washers and dowel pins, catalog expressions and errors.

use super::*;
use occt_bridge::Bounds;
use std::f64::consts::PI;

fn length(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}

fn part(
    id: &str,
    kind: StandardPartKind,
    nominal: f64,
    size: Option<f64>,
    origin: (f64, f64, f64),
    axis: (f64, f64, f64),
) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::StandardPart {
            part: kind,
            nominal_diameter: length(nominal),
            length: size.map(length),
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                origin.0,
                origin.1,
                origin.2,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(axis.0, axis.1, axis.2)),
        },
    }
}

fn family(features: Vec<FeatureDefinition>) -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "hardware".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        datums: Vec::new(),
        requirements: Vec::new(),
        features,
    }
}

/// Volume and exact bounds of each feature, regenerated once.
fn measure(features: Vec<FeatureDefinition>) -> Result<HashMap<String, (f64, Bounds)>, ModelError> {
    let ids: Vec<String> = features.iter().map(|f| f.id.clone()).collect();
    let definition = family(features);
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let generated = part.regenerate(&session)?;
    let mut out = HashMap::new();
    for id in ids {
        let shape = generated.shape(&id).unwrap();
        assert!(session.is_valid(shape).unwrap(), "{id}");
        out.insert(
            id,
            (
                session.volume(shape).unwrap(),
                session.exact_bounds(shape).unwrap(),
            ),
        );
    }
    Ok(out)
}

fn hexagon_area(across_flats: f64) -> f64 {
    3f64.sqrt() / 2.0 * across_flats * across_flats
}

fn frustum(h: f64, r1: f64, r2: f64) -> f64 {
    PI * h / 3.0 * (r1 * r1 + r1 * r2 + r2 * r2)
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual / expected - 1.0).abs() < 1e-9,
        "{actual} vs {expected}"
    );
}

#[test]
fn standard_parts_have_their_catalog_volumes_and_seats() {
    use StandardPartKind::*;
    let up = (0.0, 0.0, 1.0);
    let measured = measure(vec![
        part(
            "screw",
            SocketHeadCapScrewIso4762,
            6.0,
            Some(20.0),
            (0.0, 0.0, 0.0),
            up,
        ),
        part("nut", HexNutIso4032, 10.0, None, (0.0, 0.0, 0.0), up),
        part("washer", PlainWasherIso7089, 8.0, None, (0.0, 0.0, 0.0), up),
        part(
            "dowel",
            DowelPinIso8734,
            6.0,
            Some(30.0),
            (0.0, 0.0, 0.0),
            up,
        ),
        // Any axis and origin: an M4 screw lying along -x at (50, 0, 0).
        part(
            "tilted",
            SocketHeadCapScrewIso4762,
            4.0,
            Some(10.0),
            (50.0, 0.0, 0.0),
            (-1.0, 0.0, 0.0),
        ),
    ])
    .unwrap();
    // M6: head Ø10 × 6, shank Ø6 × 20, socket s5 × 3.
    let (volume, bounds) = measured["screw"];
    close(
        volume,
        PI * 25.0 * 6.0 + PI * 9.0 * 20.0 - hexagon_area(5.0) * 3.0,
    );
    assert!((bounds.min.z + 6.0).abs() < 1e-7 && (bounds.max.z - 20.0).abs() < 1e-7);
    assert!((bounds.max.x - 5.0).abs() < 1e-7);
    // M10 nut: s16 × 8.4 with a Ø10 bore.
    let (volume, bounds) = measured["nut"];
    close(volume, (hexagon_area(16.0) - PI * 25.0) * 8.4);
    assert!((bounds.max.z - 8.4).abs() < 1e-7 && bounds.min.z.abs() < 1e-7);
    // M8 washer: Ø16 / Ø8.4 × 1.6.
    close(
        measured["washer"].0,
        PI / 4.0 * (16.0f64.powi(2) - 8.4f64.powi(2)) * 1.6,
    );
    // Ø6 × 30 dowel with 1.2 mm chamfers at both ends.
    close(
        measured["dowel"].0,
        PI * 9.0 * (30.0 - 2.4) + 2.0 * frustum(1.2, 3.0, 1.8),
    );
    let (volume, bounds) = measured["tilted"];
    close(
        volume,
        PI * 3.5f64.powi(2) * 4.0 + PI * 4.0 * 10.0 - hexagon_area(3.0) * 2.0,
    );
    assert!((bounds.min.x - 40.0).abs() < 1e-7 && (bounds.max.x - 54.0).abs() < 1e-7);
}

#[test]
fn standard_part_errors_name_the_problem() {
    use StandardPartKind::*;
    let up = (0.0, 0.0, 1.0);
    for (feature, expected) in [
        (
            part(
                "p",
                SocketHeadCapScrewIso4762,
                7.0,
                Some(10.0),
                (0.0, 0.0, 0.0),
                up,
            ),
            "no 7 mm size",
        ),
        (
            part(
                "p",
                SocketHeadCapScrewIso4762,
                6.0,
                None,
                (0.0, 0.0, 0.0),
                up,
            ),
            "needs a positive finite length",
        ),
        (
            part("p", HexNutIso4032, 6.0, Some(5.0), (0.0, 0.0, 0.0), up),
            "takes no length",
        ),
        (
            part("p", DowelPinIso8734, 6.0, Some(2.0), (0.0, 0.0, 0.0), up),
            "must exceed its two",
        ),
        (
            part(
                "p",
                PlainWasherIso7089,
                6.0,
                None,
                (0.0, 0.0, 0.0),
                (0.0, 0.0, 0.0),
            ),
            "axis must be finite and nonzero",
        ),
    ] {
        let error = measure(vec![feature]).unwrap_err().message;
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn catalog_dimensions_size_counterbores_in_expressions() {
    // A counterbore sized from the screw it takes: head Ø + 1 mm.
    let expression = ScalarExpr::Add(
        Box::new(ScalarExpr::StandardPartDimensionV1 {
            part: StandardPartKind::SocketHeadCapScrewIso4762,
            nominal_diameter: Box::new(length(8.0)),
            dimension: StandardPartDimension::HeadDiameter,
        }),
        Box::new(length(1.0)),
    );
    let value = occt_parametric_core::expressions::evaluate_resolved_expression(
        &expression,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!((value.value, value.dimension), (14.0, Dimension::Length));
    let json = serde_json::to_value(&expression).unwrap();
    assert_eq!(
        json["add"][0],
        serde_json::json!({"standard_part_dimension_v1": {
            "part": "socket_head_cap_screw_iso_4762",
            "nominal_diameter": {"literal": {"value": 8.0, "dimension": "length", "unit": "millimeter"}},
            "dimension": "head_diameter"}})
    );
    assert_eq!(
        serde_json::from_value::<ScalarExpr>(json).unwrap(),
        expression
    );
    let unitless = ScalarExpr::StandardPartDimensionV1 {
        part: StandardPartKind::HexNutIso4032,
        nominal_diameter: Box::new(ScalarExpr::Literal(Quantity::scalar(8.0))),
        dimension: StandardPartDimension::AcrossFlats,
    };
    assert!(
        occt_parametric_core::expressions::evaluate_resolved_expression(&unitless, &HashMap::new())
            .is_err()
    );
}
