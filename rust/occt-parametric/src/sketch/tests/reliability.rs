use super::*;

fn empty() -> SketchDefinition {
    let mut s = rectangle();
    s.points.clear();
    s.lines.clear();
    s.constraints.clear();
    s
}

#[test]
fn local_differentiation_solves_small_geometry_far_from_origin_and_preserves_rank() {
    for (offset, radius) in [(0., 10.), (1e9, 10.), (1e6, 1e-4), (0., 1e-4)] {
        let mut s = empty();
        s.points = vec![
            point("center", offset, offset, true),
            point("rim", offset + radius * 0.8, offset + radius * 0.3, false),
        ];
        s.lines = vec![SketchLine {
            id: "line".into(),
            start: "center".into(),
            end: "rim".into(),
        }];
        s.constraints = vec![
            SketchConstraint::Horizontal {
                line: "line".into(),
            },
            SketchConstraint::Distance {
                first: "center".into(),
                second: "rim".into(),
                value: length(radius),
            },
        ];
        let result = s.solve(&HashMap::new()).unwrap();
        assert!(
            result.solved,
            "offset={offset}, radius={radius}: {result:?}"
        );
        assert_eq!(result.free_degrees, 0);
        assert!(result.solver_report.rank_reliable);
        assert_eq!(
            result.solver_report.termination,
            SketchTermination::Converged
        );
        assert!((result.points["rim"].x - offset - radius).abs() < 2e-7_f64.max(radius * 1e-8));
    }
}

#[test]
fn independent_small_angular_derivatives_do_not_disappear_from_strict_rank() {
    let mut s = empty();
    s.acceptance = SketchAcceptance::FullyConstrained;
    s.points = vec![point("origin", 0., 0., true), point("tip", 1e8, 0., false)];
    s.lines = vec![SketchLine {
        id: "axis".into(),
        start: "origin".into(),
        end: "tip".into(),
    }];
    s.constraints = vec![
        SketchConstraint::HorizontalDistance {
            first: "origin".into(),
            second: "tip".into(),
            value: length(1e8),
        },
        SketchConstraint::LineAngle {
            line: "axis".into(),
            value: ScalarExpr::Literal(Quantity::scalar(0.)),
        },
    ];
    let solved = s.solve(&HashMap::new()).unwrap();
    assert!(solved.solved);
    assert_eq!(solved.free_degrees, 0);
    assert!(solved.solver_report.rank_reliable);
    s.check_acceptance(&solved).unwrap();
}

#[test]
fn periodic_angle_differentiation_does_not_take_the_long_way_across_pi() {
    let mut s = empty();
    s.points = vec![point("a", 0., 0., true), point("b", -10., 0., false)];
    s.lines = vec![SketchLine {
        id: "line".into(),
        start: "a".into(),
        end: "b".into(),
    }];
    s.constraints = vec![
        SketchConstraint::Distance {
            first: "a".into(),
            second: "b".into(),
            value: length(10.),
        },
        SketchConstraint::LineAngle {
            line: "line".into(),
            value: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::PI - 0.1)),
        },
    ];
    let result = s.solve(&HashMap::new()).unwrap();
    assert!(result.solved, "{result:?}");
    assert!((result.points["b"].y - 10. * 0.1_f64.sin()).abs() < 1e-7);
    assert!(result.iterations < 15);
}

#[test]
fn warm_starts_match_point_ids_and_keep_fixed_and_new_points_authored() {
    let mut s = rectangle();
    let original = s.solve(&HashMap::new()).unwrap();
    assert!(original.solved);
    let repeated = s
        .solve_with_seed(&HashMap::new(), &original.points)
        .unwrap();
    assert!(repeated.solved && repeated.solver_report.warm_started);
    assert_eq!(repeated.iterations, 0);
    let mut seed = original.points.clone();
    seed.insert(
        "p0".into(),
        SketchPoint2 {
            x: f64::NAN,
            y: f64::NAN,
        },
    );
    seed.insert(
        "deleted".into(),
        SketchPoint2 {
            x: f64::NAN,
            y: f64::NAN,
        },
    );
    s.points.push(point("new", 13., 14., false));
    let result = s.solve_with_seed(&HashMap::new(), &seed).unwrap();
    assert_eq!(result.points["p0"], SketchPoint2 { x: 0., y: 0. });
    assert_eq!(result.points["new"], SketchPoint2 { x: 13., y: 14. });
    seed.insert(
        "p1".into(),
        SketchPoint2 {
            x: f64::INFINITY,
            y: 0.,
        },
    );
    assert!(
        s.solve_with_seed(&HashMap::new(), &seed)
            .unwrap_err()
            .message
            .contains("seed for 'p1' is not finite")
    );
}

#[test]
fn fixed_conflicts_report_stationarity_without_claiming_a_global_proof() {
    let mut s = empty();
    s.points = vec![point("a", 0., 0., true), point("b", 10., 0., true)];
    s.constraints = vec![SketchConstraint::Coincident {
        first: "a".into(),
        second: "b".into(),
    }];
    let result = s.solve(&HashMap::new()).unwrap();
    assert!(!result.solved);
    assert_eq!(
        result.solver_report.termination,
        SketchTermination::Stationary
    );
    assert_eq!(result.solver_report.accepted_steps, 0);
    assert!(
        s.check_acceptance(&result)
            .unwrap_err()
            .message
            .contains("did not converge")
    );
}

#[test]
fn nonfinite_squared_cost_does_not_prevent_a_finite_linear_solve() {
    let mut s = empty();
    s.points = vec![point("p", 1e160, 0., false)];
    s.points.push(point("anchor", 0., 0., true));
    s.constraints = vec![SketchConstraint::Coincident {
        first: "p".into(),
        second: "anchor".into(),
    }];
    let result = s.solve(&HashMap::new()).unwrap();
    assert!(result.solved, "{result:?}");
    assert_eq!(result.points["p"], SketchPoint2 { x: 0., y: 0. });
}

#[test]
fn nearly_dependent_diagonal_constraints_cannot_certify_strict_rank() {
    let mut s = empty();
    s.acceptance = SketchAcceptance::FullyConstrained;
    s.points = vec![
        point("a", -1., -1., true),
        point("b", 1., 1., true),
        point("c", -1., -1.000001, true),
        point("d", 1., 1.000001, true),
        point("p", 0., 0., false),
    ];
    s.lines = vec![
        SketchLine {
            id: "one".into(),
            start: "a".into(),
            end: "b".into(),
        },
        SketchLine {
            id: "two".into(),
            start: "c".into(),
            end: "d".into(),
        },
    ];
    s.constraints = vec![
        SketchConstraint::PointOnCurve {
            point: "p".into(),
            curve: "one".into(),
        },
        SketchConstraint::PointOnCurve {
            point: "p".into(),
            curve: "two".into(),
        },
    ];
    let result = s.solve(&HashMap::new()).unwrap();
    assert!(result.solved);
    assert_eq!(result.diagnosis.local_rank, Some(2));
    assert!(!result.solver_report.rank_reliable);
    assert!(
        s.check_acceptance(&result)
            .unwrap_err()
            .message
            .contains("local rank is uncertain")
    );
}

#[test]
fn finite_difference_uses_valid_side_when_the_other_collapses_a_line() {
    let mut s = empty();
    s.acceptance = SketchAcceptance::FullyConstrained;
    s.points = vec![
        point("a", 0., 0., true),
        point("b", 1e-6, 0., false),
        point("pin", 1., 0., true),
    ];
    s.lines = vec![SketchLine {
        id: "line".into(),
        start: "a".into(),
        end: "b".into(),
    }];
    s.constraints = vec![
        SketchConstraint::PointOnCurve {
            point: "pin".into(),
            curve: "line".into(),
        },
        SketchConstraint::HorizontalDistance {
            first: "a".into(),
            second: "b".into(),
            value: length(1e-6),
        },
    ];
    let result = s.solve(&HashMap::new()).unwrap();
    assert!(result.solved && result.solver_report.rank_reliable);
    assert!(result.solver_report.derivative_fallbacks > 0);
    s.check_acceptance(&result).unwrap();
}

#[test]
fn a_collapsed_trial_line_is_rejected_and_backtracking_keeps_the_valid_iterate() {
    let mut s = empty();
    s.points = vec![
        point("a", 0., 0., true),
        point("tip", 1., 0., false),
        point("pin", 1., 0., true),
    ];
    s.lines = vec![SketchLine {
        id: "line".into(),
        start: "a".into(),
        end: "tip".into(),
    }];
    s.constraints = vec![
        SketchConstraint::HorizontalDistance {
            first: "a".into(),
            second: "tip".into(),
            value: length(-0.5),
        },
        SketchConstraint::PointOnCurve {
            point: "pin".into(),
            curve: "line".into(),
        },
    ];
    let params = HashMap::new();
    let mut report = s.solve(&params).unwrap().solver_report;
    let problem = super::super::solver::SketchProblem::new(
        &s,
        &params,
        HashMap::from([("tip", 0)]),
        HashMap::from([
            ("a", SketchPoint2 { x: 0., y: 0. }),
            ("pin", SketchPoint2 { x: 1., y: 0. }),
        ]),
    )
    .unwrap();
    let before = report.rejected_trials;
    let (accepted, residual) = problem
        .line_search(&[1., 0.], &[-1., 0.], &[1.5, 0.], &mut report)
        .unwrap();
    assert_eq!(accepted, vec![0.5, 0.]);
    assert_eq!(residual, vec![1., 0.]);
    assert_eq!(report.rejected_trials, before + 1);
    assert!(report.last_rejected_trial.unwrap().contains("zero length"));
}
