use super::*;
fn scalar_weight(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::scalar(value))
}
fn blend(target: &str, terms: &[(&str, ScalarExpr)]) -> SketchConstraint {
    SketchConstraint::WeightedPoint {
        point: target.into(),
        terms: terms
            .iter()
            .map(|(point, weight)| SketchWeightedPointTerm {
                point: (*point).into(),
                weight: weight.clone(),
            })
            .collect(),
    }
}
fn weighted() -> SketchDefinition {
    let mut s = rectangle();
    s.points = vec![
        point("a", 0., 0., true),
        point("b", 5., 2., false),
        point("p", 0., 0., false),
        point("q", 0., 0., false),
    ];
    s.lines.clear();
    s.profile.clear();
    s.constraints = vec![
        blend(
            "p",
            &[
                ("a", scalar_weight(1.)),
                ("b", ScalarExpr::Parameter("weight".into())),
            ],
        ),
        blend("q", &[("p", scalar_weight(1.)), ("b", scalar_weight(1.))]),
    ];
    s
}
#[test]
fn weighted_points_are_dependent_coordinates_and_constraints_drive_original_sources() {
    let mut s = weighted();
    s.constraints.push(SketchConstraint::HorizontalDistance {
        first: "a".into(),
        second: "q".into(),
        value: ScalarExpr::Parameter("span".into()),
    });
    s.constraints.push(SketchConstraint::VerticalDistance {
        first: "a".into(),
        second: "b".into(),
        value: length(0.),
    });
    for (weight, span) in [(1., 7.5), (3., 14.)] {
        let parameters = HashMap::from([
            (
                "weight".into(),
                ParameterValue::Scalar(Quantity::scalar(weight)),
            ),
            (
                "span".into(),
                ParameterValue::Scalar(Quantity::length(span, LengthUnit::Millimeter)),
            ),
        ]);
        let solution = s.solve(&parameters).unwrap();
        assert!(solution.solved, "{solution:?}");
        assert_eq!(solution.free_degrees, 0);
        assert_eq!(solution.redundant_equations, 0);
        assert_eq!(solution.diagnosis.local_rank, Some(2));
        assert!((solution.points["q"].x - span).abs() < 1e-9);
        assert!(
            (solution.points["p"].x - solution.points["b"].x * weight / (1. + weight)).abs() < 1e-9
        );
        let checks = s.constraint_checks(&parameters, &solution).unwrap();
        assert!(checks[0].by_construction);
        assert!(checks[0].satisfied);
    }
}
#[test]
fn weighted_links_preserve_free_motion_and_reject_cycles_fixed_targets_and_invalid_weights() {
    let parameters = HashMap::from([(
        "weight".into(),
        ParameterValue::Scalar(Quantity::scalar(1.)),
    )]);
    let s = weighted();
    let solution = s.solve(&parameters).unwrap();
    assert!(solution.solved);
    assert_eq!(solution.free_degrees, 2);
    assert_eq!(solution.points["b"].x, 5.);
    assert_eq!(solution.points["b"].y, 2.);
    assert_eq!(solution.points["p"].x, 2.5);
    assert_eq!(solution.points["q"].x, 3.75);
    let mut invalid = s.clone();
    invalid
        .constraints
        .push(blend("b", &[("q", scalar_weight(1.))]));
    assert!(
        invalid
            .validate_structure()
            .unwrap_err()
            .to_string()
            .contains("acyclic")
    );
    invalid = s.clone();
    invalid.points[2].fixed = true;
    assert!(invalid.validate_structure().is_err());
    invalid = s.clone();
    invalid.constraints.push(invalid.constraints[0].clone());
    assert!(invalid.validate_structure().is_err());
    for weight in [0., -1., f64::INFINITY] {
        let mut invalid = s.clone();
        if let SketchConstraint::WeightedPoint { terms, .. } = &mut invalid.constraints[0] {
            terms[0].weight = scalar_weight(weight);
        }
        assert!(invalid.solve(&parameters).is_err());
    }
    let mut names = HashSet::new();
    s.collect_parameters(&mut names);
    assert!(names.contains("weight"));
}
