use super::*;
fn linked() -> SketchDefinition {
    let mut s = rectangle();
    s.lines.clear();
    s.profile.clear();
    s.constraints.clear();
    s.points = vec![
        point("a", 0., 0., true),
        point("b", 5., 10., true),
        point("c", 10., 0., true),
        point("sample", 0., 0., false),
    ];
    s.splines = vec![SketchSpline {
        id: "curve".into(),
        points: vec!["a".into(), "b".into(), "c".into()],
        basis: Some(SketchBSplineBasis {
            degree: 2,
            periodic: false,
            non_clamped: false,
            knots: vec![0., 1.],
            multiplicities: vec![3, 3],
            weights: vec![
                ScalarExpr::Literal(Quantity::scalar(1.)),
                ScalarExpr::Parameter("weight".into()),
                ScalarExpr::Literal(Quantity::scalar(1.)),
            ],
        }),
        endpoints: None,
    }];
    s.constraints = vec![SketchConstraint::SplinePoint {
        point: "sample".into(),
        spline: "curve".into(),
        fraction: ScalarExpr::Parameter("fraction".into()),
    }];
    s
}
#[test]
fn saved_spline_points_follow_poles_weights_parameters_and_native_curves_without_unknowns() {
    let mut s = linked();
    let native = Session::new().unwrap();
    for (fraction, weight, height) in [
        (0., 1., 10.),
        (0.37, 0.7, 12.),
        (0.5, 1.4, 15.),
        (1., 2., 8.),
    ] {
        s.points[1].y = length(height);
        let parameters = HashMap::from([
            (
                "fraction".into(),
                ParameterValue::Scalar(Quantity::scalar(fraction)),
            ),
            (
                "weight".into(),
                ParameterValue::Scalar(Quantity::scalar(weight)),
            ),
        ]);
        let solution = s.solve(&parameters).unwrap();
        assert!(solution.solved);
        assert_eq!(solution.free_degrees, 0);
        assert_eq!(solution.redundant_equations, 0);
        let b = (1. - fraction) * (1. - fraction);
        let c = 2. * fraction * (1. - fraction) * weight;
        let d = fraction * fraction;
        let q = solution.points["sample"];
        assert!((q.x - (5. * c + 10. * d) / (b + c + d)).abs() < 1e-10);
        assert!((q.y - height * c / (b + c + d)).abs() < 1e-10);
        let spline = &s.splines[0];
        let poles = spline
            .points
            .iter()
            .map(|id| {
                let p = solution.points[id];
                Vec3::new(p.x, p.y, 0.)
            })
            .collect();
        let shape = native
            .create_curve_wire(
                &[spline
                    .explicit_segment(
                        poles,
                        spline.basis.as_ref().unwrap().weights(&parameters).unwrap(),
                    )
                    .unwrap()],
                false,
            )
            .unwrap();
        let closest = native
            .curve_closest_point(&shape, Vec3::new(q.x, q.y, 0.))
            .unwrap();
        assert!((closest.x - q.x).abs() < 1e-8 && (closest.y - q.y).abs() < 1e-8);
        assert!(s.constraint_checks(&parameters, &solution).unwrap()[0].by_construction);
    }
}
#[test]
fn spline_point_dependencies_drive_source_poles_and_reject_self_cycles_and_invalid_modes() {
    let parameters = HashMap::from([
        (
            "fraction".into(),
            ParameterValue::Scalar(Quantity::scalar(0.5)),
        ),
        (
            "weight".into(),
            ParameterValue::Scalar(Quantity::scalar(1.)),
        ),
    ]);
    let mut s = linked();
    s.points[1].fixed = false;
    s.constraints.extend([
        SketchConstraint::HorizontalDistance {
            first: "a".into(),
            second: "sample".into(),
            value: length(5.),
        },
        SketchConstraint::VerticalDistance {
            first: "a".into(),
            second: "sample".into(),
            value: length(6.),
        },
    ]);
    let solution = s.solve(&parameters).unwrap();
    assert!(solution.solved);
    assert_eq!(solution.free_degrees, 0);
    assert!((solution.points["b"].y - 12.).abs() < 1e-8);
    let mut bad = s.clone();
    if let SketchConstraint::SplinePoint { point, .. } = &mut bad.constraints[0] {
        *point = "b".into();
    }
    assert!(bad.validate_structure().is_err());
    let mut bad = s.clone();
    bad.constraints.push(SketchConstraint::WeightedPoint {
        point: "b".into(),
        terms: vec![SketchWeightedPointTerm {
            point: "sample".into(),
            weight: ScalarExpr::Literal(Quantity::scalar(1.)),
        }],
    });
    assert!(
        bad.validate_structure()
            .unwrap_err()
            .to_string()
            .contains("acyclic")
    );
    let mut bad = s.clone();
    bad.splines[0].basis.as_mut().unwrap().periodic = true;
    assert!(bad.validate_structure().is_err());
    let mut bad = s.clone();
    bad.points[3].fixed = true;
    assert!(bad.validate_structure().is_err());
    let mut bad_params = parameters.clone();
    bad_params.insert(
        "fraction".into(),
        ParameterValue::Scalar(Quantity::scalar(1.1)),
    );
    assert!(s.solve(&bad_params).is_err());
}
#[test]
fn spline_point_links_use_active_non_clamped_domain_and_current_refined_basis() {
    let mut s = linked();
    let parameters = HashMap::from([
        (
            "fraction".into(),
            ParameterValue::Scalar(Quantity::scalar(0.37)),
        ),
        (
            "weight".into(),
            ParameterValue::Scalar(Quantity::scalar(0.7)),
        ),
    ]);
    let baseline = s.solve(&parameters).unwrap().points["sample"];
    // Exact elevation of the rational quadratic in homogeneous coordinates.
    let w = 0.7;
    s.points = vec![
        point("a", 0., 0., true),
        point(
            "e1",
            (10. * w / 3.) / (1. / 3. + 2. * w / 3.),
            (20. * w / 3.) / (1. / 3. + 2. * w / 3.),
            true,
        ),
        point(
            "e2",
            (10. / 3. + 10. * w / 3.) / (1. / 3. + 2. * w / 3.),
            (20. * w / 3.) / (1. / 3. + 2. * w / 3.),
            true,
        ),
        point("c", 10., 0., true),
        point("sample", 0., 0., false),
    ];
    s.splines[0].points = vec!["a".into(), "e1".into(), "e2".into(), "c".into()];
    let b = s.splines[0].basis.as_mut().unwrap();
    b.degree = 3;
    b.multiplicities = vec![4, 4];
    b.weights = vec![1., (1. + 2. * w) / 3., (1. + 2. * w) / 3., 1.]
        .into_iter()
        .map(|v| ScalarExpr::Literal(Quantity::scalar(v)))
        .collect();
    let refined = s.solve(&parameters).unwrap().points["sample"];
    assert!((baseline.x - refined.x).abs() < 1e-10);
    assert!((baseline.y - refined.y).abs() < 1e-10);
    // Unclamped uniform quadratic: active interval [2,3], not [0,5].
    let b = s.splines[0].basis.as_mut().unwrap();
    b.degree = 2;
    b.non_clamped = true;
    b.knots = vec![0., 1., 2., 3., 4., 5.];
    b.multiplicities = vec![1; 6];
    b.weights.clear();
    s.splines[0].points = vec!["a".into(), "e1".into(), "c".into()];
    s.splines[0].endpoints = Some(SketchSplineEndpoints {
        start: "start".into(),
        end: "end".into(),
    });
    s.points.push(point("start", 0., 0., false));
    s.points.push(point("end", 0., 0., false));
    let solution = s.solve(&parameters).unwrap();
    assert!(solution.solved);
    let q = solution.points["sample"];
    let native = Session::new().unwrap();
    let spline = &s.splines[0];
    let poles = spline
        .points
        .iter()
        .map(|id| {
            let p = solution.points[id];
            Vec3::new(p.x, p.y, 0.)
        })
        .collect();
    let curve = native
        .create_curve_wire(&[spline.explicit_segment(poles, vec![]).unwrap()], false)
        .unwrap();
    let closest = native
        .curve_closest_point(&curve, Vec3::new(q.x, q.y, 0.))
        .unwrap();
    assert!((closest.x - q.x).abs() < 1e-8 && (closest.y - q.y).abs() < 1e-8);
}

#[test]
fn spline_point_basis_matches_native_across_degrees_multi_spans_and_knots() {
    let native = Session::new().unwrap();
    for degree in [1, 2, 3, 7, 25] {
        let count = degree as usize + 5;
        let mut s = linked();
        s.points = (0..count)
            .map(|i| point(&format!("pole-{i}"), i as f64, (i as f64).sin(), true))
            .collect();
        s.points.push(point("sample", 0., 0., false));
        s.splines[0].points = (0..count).map(|i| format!("pole-{i}")).collect();
        let b = s.splines[0].basis.as_mut().unwrap();
        b.degree = degree;
        b.knots = (0..=5).map(f64::from).collect();
        b.multiplicities = vec![degree + 1, 1, 1, 1, 1, degree + 1];
        b.weights = (0..count)
            .map(|i| ScalarExpr::Literal(Quantity::scalar(1. + i as f64 * 0.1)))
            .collect();
        for fraction in [0., 0.1, 0.2, 0.333, 0.7, 1.] {
            let parameters = HashMap::from([(
                "fraction".into(),
                ParameterValue::Scalar(Quantity::scalar(fraction)),
            )]);
            let solution = s.solve(&parameters).unwrap();
            assert!(solution.solved);
            let q = solution.points["sample"];
            let spline = &s.splines[0];
            let poles = spline
                .points
                .iter()
                .map(|id| {
                    let p = solution.points[id];
                    Vec3::new(p.x, p.y, 0.)
                })
                .collect();
            let curve = native
                .create_curve_wire(
                    &[spline
                        .explicit_segment(
                            poles,
                            spline.basis.as_ref().unwrap().weights(&parameters).unwrap(),
                        )
                        .unwrap()],
                    false,
                )
                .unwrap();
            let closest = native
                .curve_closest_point(&curve, Vec3::new(q.x, q.y, 0.))
                .unwrap();
            assert!(
                (closest.x - q.x).abs() < 1e-7 && (closest.y - q.y).abs() < 1e-7,
                "degree {degree}, fraction {fraction}"
            );
        }
        assert_eq!(native.shape_count().unwrap(), 0);
    }
}

#[test]
fn one_thousand_spline_point_links_add_no_solver_unknowns_within_budget() {
    let mut s = linked();
    s.points.pop();
    s.constraints.clear();
    for i in 0..1000 {
        let id = format!("sample-{i}");
        s.points.push(point(&id, 0., 0., false));
        s.constraints.push(SketchConstraint::SplinePoint {
            point: id,
            spline: "curve".into(),
            fraction: ScalarExpr::Literal(Quantity::scalar(i as f64 / 999.)),
        });
    }
    let parameters = HashMap::from([(
        "weight".into(),
        ParameterValue::Scalar(Quantity::scalar(0.7)),
    )]);
    let start = std::time::Instant::now();
    let solution = s.solve(&parameters).unwrap();
    assert!(solution.solved);
    assert_eq!(solution.free_degrees, 0);
    assert_eq!(solution.redundant_equations, 0);
    assert_eq!(solution.points.len(), 1003);
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}
