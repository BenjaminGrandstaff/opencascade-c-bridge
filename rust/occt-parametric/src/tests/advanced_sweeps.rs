//! Advanced sweeps (schema 175) working with sketch constraints: a duct
//! whose square ends sit on planes normal to a constrained route, twisting
//! and flaring between them, follows every edit of the route.
use super::*;

const PI: f64 = std::f64::consts::PI;

fn document() -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/advanced-sweep.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}

fn part(family: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "duct".into(),
        definition: family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

fn set(part: &mut PartInstance<'_>, id: &str, value: f64, length: bool) {
    let quantity = if length {
        Quantity::length(value, LengthUnit::Millimeter)
    } else {
        Quantity::scalar(value)
    };
    part.overrides
        .insert(id.into(), ParameterValue::Scalar(quantity));
}

/// Sections blend linearly between two squares centered on the route and
/// normal to it, so the volume is the route length times the mean of
/// s0², s0·s1 and s1²; the twist turns squares into themselves.
fn volume(run: f64, radius: f64, angle: f64, s0: f64, s1: f64) -> f64 {
    (run + radius * angle) * (s0 * s0 + s0 * s1 + s1 * s1) / 3.0
}

/// Where the route ends: the bend's centre sits `radius` above the run's end.
fn route_end(run: f64, radius: f64, angle: f64) -> Vec3 {
    Vec3::new(
        run + radius * angle.sin(),
        radius * (1.0 - angle.cos()),
        0.0,
    )
}

fn check(session: &Session, result: &GeneratedResult<'_>, expected_volume: f64, end: Vec3) {
    let duct = result.shape("duct").unwrap();
    assert!(session.is_valid(duct).unwrap());
    let actual = session.volume(duct).unwrap();
    assert!(
        (actual - expected_volume).abs() <= 1e-5 * expected_volume,
        "{actual} vs {expected_volume}"
    );
    // Each square sits centered on its end of the route.
    let inlet = session
        .center_of_mass(result.shape("inlet_section").unwrap())
        .unwrap();
    assert!(inlet.x.abs() < 1e-7 && inlet.y.abs() < 1e-7 && inlet.z.abs() < 1e-7);
    let outlet = session
        .center_of_mass(result.shape("outlet_section").unwrap())
        .unwrap();
    assert!(
        (outlet.x - end.x).abs() < 1e-6 && (outlet.y - end.y).abs() < 1e-6,
        "{outlet:?} vs {end:?}"
    );
}

#[test]
fn a_twisted_flaring_duct_follows_its_constrained_route() {
    let document = document();
    let session = Session::new().unwrap();
    let mut duct = part(&document.family);
    let first = duct.regenerate(&session).unwrap();
    check(
        &session,
        &first,
        volume(40.0, 30.0, PI / 2.0, 8.0, 14.0),
        route_end(40.0, 30.0, PI / 2.0),
    );
    // Re-dimension the route: a longer run into a wider, shallower bend.
    // The solver moves the route, the path planes carry both squares to its
    // new ends, and the sweep follows.
    set(&mut duct, "run", 60.0, true);
    set(&mut duct, "bend_radius", 45.0, true);
    set(&mut duct, "bend_angle", PI / 3.0, false);
    let rerouted = duct.regenerate_incremental(&session, &first).unwrap();
    check(
        &session,
        &rerouted,
        volume(60.0, 45.0, PI / 3.0, 8.0, 14.0),
        route_end(60.0, 45.0, PI / 3.0),
    );
    // Resizing the outlet rebuilds only that section and the sweep.
    set(&mut duct, "end_side", 18.0, true);
    let wider = duct.regenerate_incremental(&session, &rerouted).unwrap();
    assert_eq!(wider.regeneration.rebuilt, ["outlet_section", "duct"]);
    check(
        &session,
        &wider,
        volume(60.0, 45.0, PI / 3.0, 8.0, 18.0),
        route_end(60.0, 45.0, PI / 3.0),
    );
    // Any number of quarter turns leaves the squares' volume unchanged.
    set(&mut duct, "twist", -PI, false);
    let turned = duct.regenerate_incremental(&session, &wider).unwrap();
    assert_eq!(turned.regeneration.rebuilt, ["duct"]);
    check(
        &session,
        &turned,
        volume(60.0, 45.0, PI / 3.0, 8.0, 18.0),
        route_end(60.0, 45.0, PI / 3.0),
    );
}

fn sweep(document: &mut ModelDocument) -> &mut FeatureOperation {
    &mut document
        .family
        .features
        .iter_mut()
        .find(|feature| feature.id == "duct")
        .unwrap()
        .operation
}

#[test]
fn scale_laws_guides_and_sections_round_trip_and_are_dependencies() {
    // The outlet square swapped for a doubling scale law along the route.
    let mut document = document();
    let FeatureOperation::Sweep {
        sections, scale, ..
    } = sweep(&mut document)
    else {
        panic!()
    };
    sections.clear();
    *scale = Some(SweepScale {
        end: ScalarExpr::Literal(Quantity::scalar(2.0)),
        law: SweepLaw::Linear,
    });
    let session = Session::new().unwrap();
    let generated = part(&document.family).regenerate(&session).unwrap();
    let duct = generated.shape("duct").unwrap();
    let length = 40.0 + 30.0 * PI / 2.0;
    let expected = length * 64.0 * (1.0 + 2.0 + 4.0) / 3.0;
    let actual = session.volume(duct).unwrap();
    assert!((actual - expected).abs() <= 1e-5 * expected, "{actual}");

    // Guides and sections are inputs of the sweep, and every option
    // survives JSON; a plain sweep keeps its old form.
    let FeatureOperation::Sweep { guide, .. } = sweep(&mut document) else {
        panic!()
    };
    *guide = Some(SweepGuide {
        input: "rail".into(),
        scales: false,
    });
    let operation = sweep(&mut document).clone();
    assert_eq!(operation.dependencies(), ["inlet_section", "path", "rail"]);
    let json = serde_json::to_string(&operation).unwrap();
    assert!(json.contains("\"twist\"") && json.contains("\"guide\"") && json.contains("\"scale\""));
    assert_eq!(
        serde_json::from_str::<FeatureOperation>(&json).unwrap(),
        operation
    );
    let with_sections = document_sections();
    assert_eq!(
        with_sections.dependencies(),
        ["inlet_section", "path", "outlet_section"]
    );
}

fn document_sections() -> FeatureOperation {
    let mut document = document();
    sweep(&mut document).clone()
}

#[test]
fn holed_profiles_twist_together_but_take_no_sections() {
    let mut document = document();
    // A square tube: the inlet square with a concentric inner square.
    let FeatureOperation::SketchFace { sketch } = &document
        .family
        .features
        .iter()
        .find(|feature| feature.id == "inlet_section")
        .unwrap()
        .operation
    else {
        panic!()
    };
    let mut inner = (**sketch).clone();
    inner.id = "inner_square".into();
    for constraint in &mut inner.constraints {
        if let SketchConstraint::CurveLength { value, .. } = constraint {
            *value = ScalarExpr::Literal(Quantity::length(4.0, LengthUnit::Millimeter));
        }
    }
    document.family.features.push(FeatureDefinition {
        id: "inner".into(),
        operation: FeatureOperation::SketchWire {
            sketch: Box::new(inner),
        },
    });
    document.family.features.push(FeatureDefinition {
        id: "tube_section".into(),
        operation: FeatureOperation::PlanarRegion {
            outer: "inlet_section".into(),
            holes: vec!["inner".into()],
        },
    });
    let FeatureOperation::Sweep {
        profile, sections, ..
    } = sweep(&mut document)
    else {
        panic!()
    };
    *profile = "tube_section".into();
    sections.clear();
    let session = Session::new().unwrap();
    let generated = part(&document.family).regenerate(&session).unwrap();
    let duct = generated.shape("duct").unwrap();
    assert!(session.is_valid(duct).unwrap());
    let length = 40.0 + 30.0 * PI / 2.0;
    let expected = length * (64.0 - 16.0);
    let actual = session.volume(duct).unwrap();
    assert!((actual - expected).abs() <= 1e-5 * expected, "{actual}");
    // A holed profile cannot blend into further sections.
    let FeatureOperation::Sweep { sections, .. } = sweep(&mut document) else {
        panic!()
    };
    *sections = vec![SweepSection {
        profile: "outlet_section".into(),
        position: ScalarExpr::Literal(Quantity::scalar(1.0)),
    }];
    let error = part(&document.family)
        .regenerate(&session)
        .err()
        .unwrap()
        .message;
    assert!(error.contains("profiles with one boundary"), "{error}");
}
