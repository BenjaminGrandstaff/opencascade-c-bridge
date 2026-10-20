//! Catalog material requirements (schema 179) against analytic answers:
//! mass, temperature limits, and beam bending at a section.

use super::*;

fn mm(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn length(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}

/// `bar`: 100 × 10 × 20 mm along x. `rod`: Ø10 × 100 mm along x.
/// `tube`: Ø20/Ø16 × 100 mm along x.
fn family() -> FamilyDefinition {
    let cylinder = |id: &str, radius: f64| FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::Cylinder {
            origin: mm(0.0, 0.0, 0.0),
            axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
            radius: length(radius),
            height: length(100.0),
        },
    };
    FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "beams".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        datums: Vec::new(),
        requirements: Vec::new(),
        features: vec![
            FeatureDefinition {
                id: "bar".into(),
                operation: FeatureOperation::Box {
                    origin: mm(0.0, 0.0, 0.0),
                    size: mm(100.0, 10.0, 20.0),
                },
            },
            cylinder("rod", 5.0),
            cylinder("outer", 10.0),
            cylinder("bore", 8.0),
            FeatureDefinition {
                id: "tube".into(),
                operation: FeatureOperation::Cut {
                    object: "outer".into(),
                    tool: "bore".into(),
                },
            },
        ],
    }
}

fn verify(rules: Vec<(&str, VerificationRule)>) -> Result<Vec<VerificationResult>, ModelError> {
    let mut definition = family();
    definition.requirements = rules
        .into_iter()
        .map(|(id, rule)| Requirement {
            id: id.into(),
            version: 1,
            kind: RequirementKind::Functional,
            priority: RequirementPriority::Advisory,
            statement: format!("{id} holds"),
            rule,
            provenance: "test".into(),
            traces: Vec::new(),
        })
        .collect();
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let result = part
        .regenerate(&session)
        .map(|result| result.verification.clone());
    assert_eq!(session.shape_count().unwrap(), 0, "handles released");
    result
}

fn bending(
    output: &str,
    material: MaterialGrade,
    moment: f64,
    minimum: f64,
    strength: MaterialStrength,
) -> VerificationRule {
    VerificationRule::BendingStress {
        output: output.into(),
        material,
        section_origin: VectorQuantity::lengths(50.0, 5.0, 10.0, LengthUnit::Millimeter),
        beam_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
        // Not square to the beam: only its part across the section counts.
        bending_axis: VectorQuantity::scalars(0.5, 1.0, 0.0),
        moment_newton_millimeters: moment,
        minimum_safety_factor: minimum,
        strength,
    }
}

/// The number after `key` in a result message.
fn number(message: &str, key: &str) -> f64 {
    let rest = &message[message.find(key).unwrap() + key.len()..];
    rest.split_whitespace()
        .next()
        .unwrap()
        .trim_end_matches([',', ';'])
        .parse()
        .unwrap()
}

#[test]
fn mass_and_temperature_limits_use_the_catalog_exactly() {
    let mass = |id, material, minimum, maximum| {
        (
            id,
            VerificationRule::MassRange {
                output: "bar".into(),
                material,
                minimum_kilograms: minimum,
                maximum_kilograms: maximum,
            },
        )
    };
    let temperature = |id, material, service, margin| {
        (
            id,
            VerificationRule::TemperatureLimit {
                output: "bar".into(),
                material,
                limit: MaterialTemperatureLimit::HeatDeflection045Mpa,
                service_temperature_c: service,
                minimum_margin_c: margin,
            },
        )
    };
    let results = verify(vec![
        // 20,000 mm³ of 6061 at 2,700 kg/m³ is 0.054 kg; of PLA, 0.0248 kg.
        mass("aluminum", MaterialGrade::Aluminum6061T6, 0.05, 0.06),
        mass("pla", MaterialGrade::PlaPrinted, 0.05, 0.06),
        // Printed PLA deflects at 55 °C under 0.45 MPa.
        temperature("warm", MaterialGrade::PlaPrinted, 40.0, 10.0),
        temperature("hot", MaterialGrade::PlaPrinted, 50.0, 10.0),
    ])
    .unwrap();
    let measured = |i: usize| results[i].measured.unwrap();
    assert_eq!(results[0].status, VerificationStatus::Passed);
    assert!((measured(0).value - 0.054).abs() < 1e-12);
    assert_eq!(measured(0).unit, MeasurementUnit::Kilogram);
    assert_eq!(results[1].status, VerificationStatus::Failed);
    assert!((measured(1).value - 0.0248).abs() < 1e-12);
    assert_eq!(results[2].status, VerificationStatus::Passed);
    assert_eq!(results[3].status, VerificationStatus::Failed);
    assert_eq!(measured(3).value, 55.0);
    assert_eq!(measured(3).minimum, Some(60.0));
    assert!(
        results
            .iter()
            .take(4)
            .all(|r| r.evidence == Evidence::Exact)
    );
    // A limit the source does not print is an error naming the source.
    let error = verify(vec![temperature(
        "ti",
        MaterialGrade::TitaniumCnc,
        20.0,
        0.0,
    )])
    .unwrap_err()
    .message;
    assert!(
        error.contains("Proto Labs titanium (2026) does not print"),
        "{error}"
    );
    assert!(verify(vec![mass("bad", MaterialGrade::PlaPrinted, 0.1, 0.05)]).is_err());
}

#[test]
fn bending_matches_beam_theory_for_rectangles_rods_and_tubes() {
    let results = verify(vec![
        // b h³/12 = 10 × 20³ / 12; c = 10; 100,000 N·mm gives 150 MPa.
        (
            "bar",
            bending(
                "bar",
                MaterialGrade::Aluminum6061T6,
                1e5,
                1.5,
                MaterialStrength::Yield,
            ),
        ),
        (
            "bar.strict",
            bending(
                "bar",
                MaterialGrade::Aluminum6061T6,
                1e5,
                2.0,
                MaterialStrength::Yield,
            ),
        ),
        // Across printed layers the 17 MPa bond governs.
        (
            "bar.layers",
            bending(
                "bar",
                MaterialGrade::PlaPrinted,
                1e4,
                1.0,
                MaterialStrength::Interlayer,
            ),
        ),
        (
            "rod",
            bending(
                "rod",
                MaterialGrade::PetgPrinted,
                1e4,
                0.4,
                MaterialStrength::Yield,
            ),
        ),
        (
            "tube",
            bending(
                "tube",
                MaterialGrade::StainlessSteel304,
                1e5,
                1.0,
                MaterialStrength::Yield,
            ),
        ),
    ])
    .unwrap();
    let check =
        |r: &VerificationResult, second_moment: f64, fiber: f64, strength: f64, moment: f64| {
            let i = number(&r.message, "I ");
            let c = number(&r.message, "c ");
            assert!(
                (i / second_moment - 1.0).abs() < 1e-6,
                "{}: I {i} vs {second_moment}",
                r.message
            );
            assert!(
                c <= fiber + 1e-9 && c > fiber * 0.998,
                "{}: c {c} vs {fiber}",
                r.message
            );
            let factor = r.measured.unwrap().value;
            let expected = strength * i / (moment * c);
            // From the rounded I and c in the message.
            assert!((factor / expected - 1.0).abs() < 1e-5);
            assert!(matches!(r.evidence, Evidence::Sampled { samples, .. } if samples > 0));
            factor
        };
    let bar = 10.0 * 20f64.powi(3) / 12.0;
    let factor = check(&results[0], bar, 10.0, 276.0, 1e5);
    assert!((factor - 276.0 / 150.0).abs() < 1e-6);
    assert_eq!(results[0].status, VerificationStatus::Passed);
    assert_eq!(results[1].status, VerificationStatus::Failed);
    // 10,000 N·mm on the bar is 15 MPa against the 17 MPa layer bond.
    let factor = check(&results[2], bar, 10.0, 17.0, 1e4);
    assert!((factor - 17.0 / 15.0).abs() < 1e-6);
    let pi = std::f64::consts::PI;
    check(&results[3], pi * 10f64.powi(4) / 64.0, 5.0, 47.0, 1e4);
    assert_eq!(results[3].status, VerificationStatus::Passed);
    let tube = pi * (20f64.powi(4) - 16f64.powi(4)) / 64.0;
    check(
        &results[4],
        tube,
        10.0,
        48.0 * 1000.0 * 0.006_894_757_293_168_361,
        1e5,
    );
    let witness = results[0].witness.as_ref().unwrap();
    assert!((witness.points_mm[1].x - 50.0).abs() < 0.01);
    // Degenerate setups are errors, not passes.
    let mut parallel = bending(
        "bar",
        MaterialGrade::Aluminum6061T6,
        1e5,
        1.0,
        MaterialStrength::Yield,
    );
    if let VerificationRule::BendingStress { bending_axis, .. } = &mut parallel {
        *bending_axis = VectorQuantity::scalars(2.0, 0.0, 0.0);
    }
    assert!(
        verify(vec![("parallel", parallel)])
            .unwrap_err()
            .message
            .contains("bending axis")
    );
    let mut missed = bending(
        "bar",
        MaterialGrade::Aluminum6061T6,
        1e5,
        1.0,
        MaterialStrength::Yield,
    );
    if let VerificationRule::BendingStress { section_origin, .. } = &mut missed {
        *section_origin = VectorQuantity::lengths(500.0, 0.0, 0.0, LengthUnit::Millimeter);
    }
    assert!(
        verify(vec![("missed", missed)])
            .unwrap_err()
            .message
            .contains("does not cut")
    );
    let missing = bending(
        "bar",
        MaterialGrade::SteelAlloy4140,
        1e5,
        1.0,
        MaterialStrength::Yield,
    );
    assert!(
        verify(vec![("4140", missing)])
            .unwrap_err()
            .message
            .contains("does not print")
    );
}

#[test]
fn material_properties_evaluate_in_expressions_and_round_trip() {
    let expression = ScalarExpr::MaterialPropertyV1 {
        material: MaterialGrade::PlaPrinted,
        property: MaterialProperty::InterlayerStrengthMpa,
    };
    let value = occt_parametric_core::expressions::evaluate_resolved_expression(
        &expression,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!((value.value, value.dimension), (17.0, Dimension::Scalar));
    let json = serde_json::to_value(&expression).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"material_property_v1": {"material": "pla_printed", "property": "interlayer_strength_mpa"}})
    );
    assert_eq!(
        serde_json::from_value::<ScalarExpr>(json).unwrap(),
        expression
    );
    let missing = ScalarExpr::MaterialPropertyV1 {
        material: MaterialGrade::TitaniumCnc,
        property: MaterialProperty::YoungsModulusMpa,
    };
    assert!(
        occt_parametric_core::expressions::evaluate_resolved_expression(&missing, &HashMap::new())
            .is_err()
    );
}
