//! Lofts through parameter-placed planar outlines.

use super::*;

fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn direction(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::scalars(x, y, z))
}

/// A section in the XZ plane at span position `y`.
fn section(profile: Vec<[f64; 2]>, y: f64, scale: ScalarExpr) -> LoftSection {
    LoftSection {
        profile,
        origin: point(0.0, y, 0.0),
        x_axis: direction(1.0, 0.0, 0.0),
        y_axis: direction(0.0, 0.0, 1.0),
        scale,
        rotation_radians: None,
        pivot: [0.0, 0.0],
    }
}

fn unit_square() -> Vec<[f64; 2]> {
    vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
}

fn circle(points: usize) -> Vec<[f64; 2]> {
    (0..points)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / points as f64;
            [angle.cos(), angle.sin()]
        })
        .collect()
}

fn loft_family(sections: Vec<LoftSection>, smooth: bool, ruled: bool) -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.parameters.push(length_parameter("size", 10.0));
    let mut turn = length_parameter("turn", 0.0);
    turn.parameter_type = ParameterType::Scalar(Dimension::Scalar);
    turn.default = ParameterValue::Scalar(Quantity::scalar(0.0));
    turn.minimum = None;
    family.parameters.push(turn);
    family.features = vec![FeatureDefinition {
        id: "loft".into(),
        operation: FeatureOperation::Loft {
            sections,
            smooth,
            ruled,
        },
    }];
    family
}

fn part(family: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition: family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

fn volume(session: &Session, result: &GeneratedResult<'_>) -> f64 {
    session.volume(result.shape("loft").unwrap()).unwrap()
}

#[test]
fn parameter_driven_sections_place_scale_and_rotate_profiles() {
    let session = Session::new().unwrap();
    let size = ScalarExpr::Parameter("size".into());
    // A square turning about its center is still a 20 mm prism of its area.
    let mut tip = section(unit_square(), 20.0, size.clone());
    tip.rotation_radians = Some(ScalarExpr::Parameter("turn".into()));
    tip.pivot = [0.5, 0.5];
    let family = loft_family(vec![section(unit_square(), 0.0, size), tip], false, true);
    let mut instance = part(&family);
    let first = instance.regenerate(&session).unwrap();
    assert!((volume(&session, &first) - 10.0 * 10.0 * 20.0).abs() < 1e-6);

    instance.overrides.insert(
        "size".into(),
        ParameterValue::Scalar(Quantity::length(2.0, LengthUnit::Centimeter)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.rebuilt, vec!["loft"]);
    assert!((volume(&session, &second) - 20.0 * 20.0 * 20.0).abs() < 1e-6);
    let bounds = session.bounds(second.shape("loft").unwrap()).unwrap();
    assert!((bounds.max.x - 20.0).abs() < 1e-6 && (bounds.max.z - 20.0).abs() < 1e-6);

    // Rotating the tip a quarter turn about its center moves its corners but
    // keeps the profile within the same 20 mm square.
    instance.overrides.insert(
        "turn".into(),
        ParameterValue::Scalar(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
    );
    let turned = instance.regenerate_incremental(&session, &second).unwrap();
    assert!(session.is_valid(turned.shape("loft").unwrap()).unwrap());
    drop((first, second, turned));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn smooth_sections_interpolate_curves_and_polygons_do_not() {
    let session = Session::new().unwrap();
    let radius = || ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter));
    let sections = || {
        vec![
            section(circle(24), 0.0, radius()),
            section(circle(24), 30.0, radius()),
        ]
    };
    let exact = std::f64::consts::PI * 25.0 * 30.0;
    let smooth = loft_family(sections(), true, true);
    let polygon = loft_family(sections(), false, true);
    let smooth_volume = volume(&session, &part(&smooth).regenerate(&session).unwrap());
    let polygon_volume = volume(&session, &part(&polygon).regenerate(&session).unwrap());
    assert!(
        (smooth_volume - exact).abs() / exact < 1e-3,
        "{smooth_volume}"
    );
    assert!(
        (polygon_volume - exact).abs() / exact > 1e-2,
        "{polygon_volume}"
    );
}

#[test]
fn invalid_sections_fail_without_partial_output_and_lofts_persist() {
    let session = Session::new().unwrap();
    let size = || ScalarExpr::Parameter("size".into());
    let good = || section(unit_square(), 0.0, size());
    let far = || section(unit_square(), 20.0, size());
    let mut three_points = far();
    three_points.profile.pop();
    let mut slanted = far();
    slanted.y_axis = direction(1.0, 0.0, 1.0);
    let mut flat = far();
    flat.scale = ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter));
    let mut unitless = far();
    unitless.scale = ScalarExpr::Literal(Quantity::scalar(1.0));
    let mut nan = far();
    nan.profile[1][0] = f64::NAN;
    for (sections, message) in [
        (vec![good()], "2-1000 sections"),
        (vec![good(), three_points], "same number"),
        (vec![good(), slanted], "perpendicular"),
        (vec![good(), flat], "positive length"),
        (vec![good(), unitless], "dimension"),
        (vec![good(), nan], "finite"),
    ] {
        let family = loft_family(sections, true, true);
        let error = part(&family).regenerate(&session).err().unwrap();
        assert!(error.message.contains(message), "{}", error.message);
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    let family = loft_family(vec![good(), far()], true, false);
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(!json.contains("rotation_radians") && !json.contains("pivot"));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
}
