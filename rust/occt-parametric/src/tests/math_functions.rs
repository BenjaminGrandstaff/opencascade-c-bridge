//! Mathematical functions in scalar expressions: values, dimensions, errors.

use super::*;

fn p(name: &str) -> Box<ScalarExpr> {
    Box::new(ScalarExpr::Parameter(name.into()))
}

fn number(value: f64) -> Box<ScalarExpr> {
    Box::new(ScalarExpr::Literal(Quantity::scalar(value)))
}

fn mm(value: f64) -> Box<ScalarExpr> {
    Box::new(ScalarExpr::Literal(Quantity::length(
        value,
        LengthUnit::Millimeter,
    )))
}

fn lengths(x: f64, y: f64, z: f64) -> Box<VectorExpr> {
    Box::new(VectorExpr::Literal(VectorQuantity::lengths(
        x,
        y,
        z,
        LengthUnit::Millimeter,
    )))
}

fn derived(id: &str, dimension: Dimension, expression: ScalarExpr) -> DerivedParameterDefinition {
    DerivedParameterDefinition {
        id: id.into(),
        dimension,
        expression,
    }
}

/// The block family (width 10, depth 20, height 30 mm) with an input
/// vector parameter `span` of (30, 40, 0) mm and the given derived scalars.
fn with(derived: Vec<DerivedParameterDefinition>) -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.parameters.push(ParameterDefinition {
        id: "span".into(),
        parameter_type: ParameterType::Vector(Dimension::Length),
        default: ParameterValue::Vector(VectorQuantity::lengths(
            30.0,
            40.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        minimum: None,
        maximum: None,
    });
    family.derived_parameters = derived;
    family
}

fn value(parameters: &HashMap<String, ParameterValue>, id: &str) -> Quantity {
    match &parameters[id] {
        ParameterValue::Scalar(quantity) => *quantity,
        other => panic!("{id}: {other:?}"),
    }
}

#[test]
fn functions_evaluate_with_their_dimensions() {
    use Dimension::{Length, Scalar};
    let round = |mode| ScalarExpr::RoundToStep {
        value: mm(13.2),
        step: mm(2.0),
        mode,
    };
    let family = with(vec![
        derived(
            "diagonal",
            Length,
            ScalarExpr::Hypotenuse(p("height"), p("depth")),
        ),
        derived("root", Scalar, ScalarExpr::SquareRoot(number(2.0))),
        derived(
            "power",
            Scalar,
            ScalarExpr::Power {
                base: number(2.0),
                exponent: number(10.0),
            },
        ),
        derived(
            "slope",
            Scalar,
            ScalarExpr::ArcTangent2 {
                y: p("depth"),
                x: p("height"),
            },
        ),
        derived(
            "rise",
            Length,
            ScalarExpr::Multiply(p("diagonal"), Box::new(ScalarExpr::Sine(p("slope")))),
        ),
        derived(
            "run",
            Length,
            ScalarExpr::Multiply(p("diagonal"), Box::new(ScalarExpr::Cosine(p("slope")))),
        ),
        derived("ratio", Scalar, ScalarExpr::Tangent(p("slope"))),
        derived("half_turn", Scalar, ScalarExpr::ArcCosine(number(-1.0))),
        derived("quarter_turn", Scalar, ScalarExpr::ArcSine(number(1.0))),
        derived(
            "chord",
            Length,
            ScalarExpr::Interpolate {
                from: mm(200.0),
                to: mm(120.0),
                fraction: number(0.25),
            },
        ),
        derived("nearest", Length, round(RoundingMode::Nearest)),
        derived("down", Length, round(RoundingMode::Down)),
        derived("up", Length, round(RoundingMode::Up)),
        derived(
            "exact_up",
            Length,
            ScalarExpr::RoundToStep {
                value: Box::new(ScalarExpr::Multiply(mm(0.1), number(120.0))),
                step: mm(2.0),
                mode: RoundingMode::Up,
            },
        ),
        derived(
            "reach",
            Length,
            ScalarExpr::VectorLength(Box::new(VectorExpr::Parameter("span".into()))),
        ),
        derived(
            "along",
            Length,
            ScalarExpr::DotProduct(
                lengths(1.0, 2.0, 3.0),
                Box::new(VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0))),
            ),
        ),
    ]);
    let parameters = resolve_parameters(&family, &HashMap::new()).unwrap();
    let close = |id: &str, expected: f64, dimension: Dimension| {
        let found = value(&parameters, id);
        assert_eq!(found.dimension, dimension, "{id}");
        let normalized = found.normalized().unwrap();
        assert!(
            (normalized - expected).abs() < 1e-12 * expected.abs().max(1.0),
            "{id}: {normalized}"
        );
    };
    close("diagonal", 1300f64.sqrt(), Length);
    close("root", 2f64.sqrt(), Scalar);
    close("power", 1024.0, Scalar);
    close("slope", 20f64.atan2(30.0), Scalar);
    close("rise", 20.0, Length);
    close("run", 30.0, Length);
    close("ratio", 20.0 / 30.0, Scalar);
    close("half_turn", std::f64::consts::PI, Scalar);
    close("quarter_turn", std::f64::consts::FRAC_PI_2, Scalar);
    close("chord", 180.0, Length);
    close("nearest", 14.0, Length);
    close("down", 12.0, Length);
    close("up", 14.0, Length);
    close("exact_up", 12.0, Length);
    close("reach", 50.0, Length);
    close("along", 3.0, Length);
}

#[test]
fn functions_reject_wrong_dimensions_and_domains() {
    use Dimension::{Length, Scalar};
    let cases = [
        (ScalarExpr::SquareRoot(number(-1.0)), Scalar, "negative"),
        (ScalarExpr::SquareRoot(p("width")), Scalar, "dimensionless"),
        (ScalarExpr::Sine(p("width")), Scalar, "dimensionless"),
        (ScalarExpr::ArcSine(number(1.5)), Scalar, "[-1, 1]"),
        (
            ScalarExpr::ArcTangent2 {
                y: mm(0.0),
                x: mm(0.0),
            },
            Scalar,
            "zero direction",
        ),
        (
            ScalarExpr::Hypotenuse(p("width"), number(1.0)),
            Length,
            "matching",
        ),
        (
            ScalarExpr::Interpolate {
                from: mm(1.0),
                to: mm(2.0),
                fraction: mm(0.5),
            },
            Length,
            "dimensionless",
        ),
        (
            ScalarExpr::RoundToStep {
                value: mm(3.0),
                step: mm(0.0),
                mode: RoundingMode::Nearest,
            },
            Length,
            "positive",
        ),
        (
            ScalarExpr::DotProduct(lengths(1.0, 0.0, 0.0), lengths(1.0, 0.0, 0.0)),
            Length,
            "dimensionless vector",
        ),
        (
            ScalarExpr::Power {
                base: number(1e300),
                exponent: number(10.0),
            },
            Scalar,
            "not finite",
        ),
    ];
    for (expression, dimension, message) in cases {
        let family = with(vec![derived("bad", dimension, expression.clone())]);
        let error = resolve_parameters(&family, &HashMap::new()).unwrap_err();
        assert!(
            error.message.contains(message),
            "{expression:?}: {}",
            error.message
        );
    }

    // Derived scalars resolve before derived vectors, so they may use only
    // input vectors; the message says so.
    let mut family = with(vec![derived(
        "reach",
        Length,
        ScalarExpr::VectorLength(Box::new(VectorExpr::Parameter("offset".into()))),
    )]);
    family
        .derived_vector_parameters
        .push(DerivedVectorParameterDefinition {
            id: "offset".into(),
            dimension: Dimension::Length,
            expression: VectorExpr::Parameter("span".into()),
        });
    let error = resolve_parameters(&family, &HashMap::new()).unwrap_err();
    assert!(
        error.message.contains("only input vector parameters"),
        "{}",
        error.message
    );
}

/// A box whose width is the diagonal of the height and depth, rounded up to
/// a 5 mm stock size; editing the height rebuilds it to the next size.
#[test]
fn functions_drive_geometry_and_persist() {
    let mut family = with(Vec::new());
    let width = ScalarExpr::RoundToStep {
        value: Box::new(ScalarExpr::Hypotenuse(p("height"), p("depth"))),
        step: mm(5.0),
        mode: RoundingMode::Up,
    };
    let body = family
        .features
        .iter_mut()
        .find(|feature| feature.id == "body")
        .unwrap();
    let FeatureOperation::Box { size, .. } = &mut body.operation else {
        panic!("body is a box")
    };
    *size = VectorExpr::Components {
        x: width,
        y: ScalarExpr::Parameter("depth".into()),
        z: ScalarExpr::Parameter("height".into()),
    };
    let session = Session::new().unwrap();
    let mut instance = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let first = instance.regenerate(&session).unwrap();
    let extent = |result: &GeneratedResult<'_>| {
        let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
        bounds.max.x - bounds.min.x
    };
    assert!(
        (extent(&first) - 40.0).abs() < 1e-6,
        "sqrt(30^2 + 20^2) = 36.1 -> 40"
    );
    instance.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(45.0, LengthUnit::Millimeter)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert!(second.regeneration.rebuilt.contains(&"body".to_string()));
    assert!(
        (extent(&second) - 50.0).abs() < 1e-6,
        "sqrt(45^2 + 20^2) = 49.2 -> 50"
    );
    drop((first, second));

    let json = serde_json::to_string(&family).unwrap();
    assert!(json.contains("\"round_to_step\"") && json.contains("\"hypotenuse\""));
    assert!(json.contains("\"up\""));
    let back: FamilyDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(back, family);
}
