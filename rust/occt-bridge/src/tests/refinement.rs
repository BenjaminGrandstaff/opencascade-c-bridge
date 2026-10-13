use super::*;

#[test]
fn native_refinement_preserves_rational_geometry_seams_and_domains_in_all_modes() {
    let session = Session::new().unwrap();
    for mode in [0, 1, 2] {
        let periodic = mode == 1;
        let knots = match mode {
            0 => vec![0., 1.],
            1 => vec![0., 1., 2., 3., 4.],
            _ => vec![0., 1., 2., 3., 4., 5., 6., 7.],
        };
        let mults = if mode == 0 {
            vec![4, 4]
        } else {
            vec![1; knots.len()]
        };
        let poles = vec![
            Vec3::new(10., 10., 0.),
            Vec3::new(-10., 10., 0.),
            Vec3::new(-10., -10., 0.),
            Vec3::new(10., -10., 0.),
        ];
        for weight in [1., 0.35, 1.7] {
            let weights = vec![weight, 1., 1., 1.];
            let source = CurveSegment::BSpline {
                non_clamped: mode == 2,
                periodic,
                poles: poles.clone(),
                degree: 3,
                knots: knots.clone(),
                multiplicities: mults.clone(),
                weights: weights.clone(),
            };
            let wire = session.create_curve_wire(&[source], periodic).unwrap();
            let edge = session
                .subshapes(&wire, ShapeType::Edge)
                .unwrap()
                .pop()
                .unwrap();
            let before = session.edge_sample_points(&edge, 101).unwrap();
            for action in [
                BSplineRefinementAction::InsertKnot(if mode == 2 { 3.5 } else { 0.5 }),
                BSplineRefinementAction::ElevateDegree(5),
            ] {
                let count = session.shape_count().unwrap();
                let refined = session
                    .refine_bspline(
                        &BSplineBasis {
                            degree: 3,
                            periodic,
                            pole_count: 4,
                            knots: &knots,
                            multiplicities: &mults,
                        },
                        action,
                    )
                    .unwrap();
                assert_eq!(session.shape_count().unwrap(), count);
                let expected = refined.coefficients.len() as i32
                    + if periodic {
                        *refined.multiplicities.last().unwrap()
                    } else {
                        refined.degree + 1
                    };
                assert_eq!(refined.multiplicities.iter().sum::<i32>(), expected);
                let mut next_poles = Vec::new();
                let mut next_weights = Vec::new();
                for row in &refined.coefficients {
                    let w: f64 = row.iter().zip(&weights).map(|(a, b)| a * b).sum();
                    let coordinate = |f: fn(&Vec3) -> f64| {
                        row.iter()
                            .zip(&weights)
                            .zip(&poles)
                            .map(|((a, b), p)| a * b * f(p))
                            .sum::<f64>()
                            / w
                    };
                    next_poles.push(Vec3::new(coordinate(|p| p.x), coordinate(|p| p.y), 0.));
                    next_weights.push(w);
                }
                let spline = CurveSegment::BSpline {
                    non_clamped: mode == 2,
                    periodic,
                    poles: next_poles,
                    degree: refined.degree,
                    knots: refined.knots,
                    multiplicities: refined.multiplicities,
                    weights: next_weights,
                };
                let next = session.create_curve_wire(&[spline], periodic).unwrap();
                assert_eq!(session.wire_is_closed(&next).unwrap(), periodic);
                let next_edge = session
                    .subshapes(&next, ShapeType::Edge)
                    .unwrap()
                    .pop()
                    .unwrap();
                for (a, b) in before
                    .iter()
                    .zip(session.edge_sample_points(&next_edge, 101).unwrap())
                {
                    assert!(
                        (a.x - b.x).hypot(a.y - b.y) < 1e-9,
                        "mode {mode}: {a:?} != {b:?}"
                    );
                }
            }
        }
    }
}
#[test]
fn native_refinement_rejects_invalid_inputs_and_excessive_output_without_handles() {
    let session = Session::new().unwrap();
    let knots = [0., 1.];
    let mults = [3, 3];
    let basis = BSplineBasis {
        degree: 2,
        periodic: false,
        pole_count: 3,
        knots: &knots,
        multiplicities: &mults,
    };
    for action in [
        BSplineRefinementAction::InsertKnot(f64::NAN),
        BSplineRefinementAction::InsertKnot(0.),
        BSplineRefinementAction::ElevateDegree(2),
        BSplineRefinementAction::ElevateDegree(26),
    ] {
        assert!(session.refine_bspline(&basis, action).is_err());
    }
    let knots: Vec<_> = (0..160).map(|i| i as f64).collect();
    let mut mults = vec![1; 160];
    mults[0] = 2;
    mults[159] = 2;
    assert!(
        session
            .refine_bspline(
                &BSplineBasis {
                    degree: 1,
                    periodic: false,
                    pole_count: 160,
                    knots: &knots,
                    multiplicities: &mults
                },
                BSplineRefinementAction::ElevateDegree(3)
            )
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
