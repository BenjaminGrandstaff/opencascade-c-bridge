use super::*;

#[test]
fn lofts_native_circle_and_ellipse_profiles_with_history_without_mutating_inputs() {
    let session = Session::new().unwrap();
    {
        let bottom = session
            .create_circle_wire(Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 2.)
            .unwrap();
        let top = session
            .create_circle_wire(Vec3::new(0., 0., 10.), Vec3::new(0., 0., 1.), 4.)
            .unwrap();
        let before = session.exact_bounds(&bottom).unwrap();
        let edge = session.subshape(&bottom, ShapeType::Edge, 0).unwrap();
        for ruled in [false, true] {
            let solid = session
                .create_loft_from_wires(&[&bottom, &top], true, ruled)
                .unwrap();
            let expected = 10. * std::f64::consts::PI * (4. + 8. + 16.) / 3.;
            assert!((session.volume(&solid).unwrap() - expected).abs() < 1e-5);
            assert!(session.is_valid(&solid).unwrap());
            assert!(
                session
                    .history_count(&solid, &edge, HistoryRelation::Generated)
                    .unwrap()
                    > 0
            );
            assert_eq!(session.exact_bounds(&bottom).unwrap(), before);
            assert_eq!(session.subshape_count(&bottom, ShapeType::Edge).unwrap(), 1);
        }
        let a = session
            .create_ellipse_wire_axes(
                Vec3::new(20., 0., 0.),
                Vec3::new(0., 0., 1.),
                Vec3::new(1., 0., 0.),
                4.,
                2.,
            )
            .unwrap();
        let b = session
            .create_ellipse_wire_axes(
                Vec3::new(20., 0., 10.),
                Vec3::new(0., 0., 1.),
                Vec3::new(1., 0., 0.),
                8.,
                4.,
            )
            .unwrap();
        let solid = session
            .create_loft_from_wires(&[&a, &b], true, true)
            .unwrap();
        assert!((session.volume(&solid).unwrap() - 560. * std::f64::consts::PI / 3.).abs() < 1e-5);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn profile_loft_rejects_open_duplicate_nonplanar_and_foreign_sections() {
    let session = Session::new().unwrap();
    {
        let a = session
            .create_circle_wire(Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 2.)
            .unwrap();
        let open = session
            .create_polyline_wire(&[Vec3::new(0., 0., 10.), Vec3::new(2., 0., 10.)], false)
            .unwrap();
        assert!(session.create_loft_from_wires(&[&a], true, true).is_err());
        assert!(
            session
                .create_loft_from_wires(&[&a, &a], true, true)
                .is_err()
        );
        assert!(
            session
                .create_loft_from_wires(&[&a, &open], true, true)
                .is_err()
        );
        let warped = session
            .create_polyline_wire(
                &[
                    Vec3::new(0., 0., 10.),
                    Vec3::new(2., 0., 10.),
                    Vec3::new(2., 2., 11.),
                    Vec3::new(0., 2., 10.),
                ],
                true,
            )
            .unwrap();
        assert!(
            session
                .create_loft_from_wires(&[&a, &warped], true, true)
                .is_err()
        );
        let foreign_session = Session::new().unwrap();
        let foreign = foreign_session
            .create_circle_wire(Vec3::new(0., 0., 10.), Vec3::new(0., 0., 1.), 2.)
            .unwrap();
        assert_wrong_session(
            session
                .create_loft_from_wires(&[&a, &foreign], true, true)
                .unwrap_err(),
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn profile_lofts_support_mixed_edge_counts_arcs_and_native_splines() {
    let session = Session::new().unwrap();
    {
        let circle = session
            .create_circle_wire(Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 2.)
            .unwrap();
        let square = session
            .create_polyline_wire(
                &[
                    Vec3::new(-2., -2., 10.),
                    Vec3::new(2., -2., 10.),
                    Vec3::new(2., 2., 10.),
                    Vec3::new(-2., 2., 10.),
                ],
                true,
            )
            .unwrap();
        let mixed = session
            .create_loft_from_wires(&[&circle, &square], true, true)
            .unwrap();
        assert!(session.is_valid(&mixed).unwrap());
        assert!(session.volume(&mixed).unwrap() > 0.0);
        assert_eq!(session.subshape_count(&circle, ShapeType::Edge).unwrap(), 1);
        assert_eq!(session.subshape_count(&square, ShapeType::Edge).unwrap(), 4);
        let d = |z: f64, r: f64| {
            session
                .create_curve_wire(
                    &[
                        CurveSegment::Arc {
                            start: Vec3::new(-r, 0., z),
                            middle: Vec3::new(0., r, z),
                            end: Vec3::new(r, 0., z),
                        },
                        CurveSegment::Line {
                            start: Vec3::new(r, 0., z),
                            end: Vec3::new(-r, 0., z),
                        },
                    ],
                    true,
                )
                .unwrap()
        };
        let lower = d(0., 2.);
        let upper = d(10., 4.);
        let solid = session
            .create_loft_from_wires(&[&lower, &upper], true, true)
            .unwrap();
        assert!(
            (session.volume(&solid).unwrap() - 140.0 * std::f64::consts::PI / 3.0).abs() < 1e-5
        );
        let spline = |z: f64, r: f64| {
            let points = (0..8)
                .map(|i| {
                    let angle = std::f64::consts::TAU * f64::from(i) / 8.0;
                    Vec3::new(r * angle.cos(), r * angle.sin(), z)
                })
                .collect();
            session
                .create_curve_wire(
                    &[CurveSegment::Spline {
                        points,
                        start_tangent: None,
                        end_tangent: None,
                        periodic: true,
                    }],
                    true,
                )
                .unwrap()
        };
        let lower = spline(0., 2.);
        let upper = spline(10., 4.);
        let before = session.exact_bounds(&lower).unwrap();
        let solid = session
            .create_loft_from_wires(&[&lower, &upper], true, false)
            .unwrap();
        assert!(session.is_valid(&solid).unwrap());
        assert!(session.volume(&solid).unwrap() > 0.0);
        assert_eq!(session.exact_bounds(&lower).unwrap(), before);
        let shell = session
            .create_loft_from_wires(&[&circle, &square], false, true)
            .unwrap();
        assert_eq!(session.subshape_count(&shell, ShapeType::Solid).unwrap(), 0);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn constrained_loft_enforces_exact_end_derivatives_and_retains_rational_sections() {
    let session = Session::new().unwrap();
    {
        let a = session
            .create_circle_wire(Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 2.)
            .unwrap();
        let b = session
            .create_circle_wire(Vec3::new(0., 0., 10.), Vec3::new(0., 0., 1.), 4.)
            .unwrap();
        let before = session.exact_bounds(&a).unwrap();
        let solid = session
            .create_constrained_loft(
                &[&a, &b],
                Some(Vec3::new(0., 0., 10.)),
                Some(Vec3::new(0., 0., 10.)),
            )
            .unwrap();
        assert!(session.is_valid(&solid).unwrap());
        let expected = 10. * std::f64::consts::PI * 332. / 35.;
        assert!((session.volume(&solid).unwrap() - expected).abs() < 1e-5);
        for t in [0.001, 0.01, 0.25, 0.5, 0.75, 0.99, 0.999] {
            let radius = 2. + 2. * (3. * t * t - 2. * t * t * t);
            let (_, r) = session
                .ray_first_hit(
                    &solid,
                    Vec3::new(0., 0., 10. * t),
                    Vec3::new(1., 0., 0.),
                    10.,
                )
                .unwrap()
                .unwrap();
            assert!((r - radius).abs() < 1e-6, "{t}: {r} vs {radius}");
        }
        let edge = session.subshape(&a, ShapeType::Edge, 0).unwrap();
        assert!(
            session
                .history_count(&solid, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
        assert_eq!(session.exact_bounds(&a).unwrap(), before);
        assert!(
            session
                .create_constrained_loft(&[&a, &b], Some(Vec3::new(1., 0., 0.)), None)
                .is_err()
        );
        assert!(
            session
                .create_constrained_loft(&[&a, &b], Some(Vec3::new(0., 0., 0.)), None)
                .is_err()
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn constrained_loft_strength_changes_shape_and_multiple_stations_remain_interpolated() {
    let session = Session::new().unwrap();
    let a = session
        .create_circle_wire(Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 2.)
        .unwrap();
    let b = session
        .create_circle_wire(Vec3::new(0., 0., 10.), Vec3::new(0., 0., 1.), 4.)
        .unwrap();
    let c = session
        .create_circle_wire(Vec3::new(0., 0., 20.), Vec3::new(0., 0., 1.), 3.)
        .unwrap();
    let strong = session
        .create_constrained_loft(
            &[&a, &b],
            Some(Vec3::new(0., 0., 20.)),
            Some(Vec3::new(0., 0., 10.)),
        )
        .unwrap();
    let (_, r) = session
        .ray_first_hit(&strong, Vec3::new(0., 0., 5.), Vec3::new(1., 0., 0.), 10.)
        .unwrap()
        .unwrap();
    assert!(r < 2.8, "strong start must alter the middle radius: {r}");
    assert!(
        session
            .create_constrained_loft(
                &[&a, &b],
                Some(Vec3::new(0., 0., 100.)),
                Some(Vec3::new(0., 0., 10.))
            )
            .is_err()
    );
    let three = session
        .create_constrained_loft(
            &[&a, &b, &c],
            Some(Vec3::new(0., 0., 10.)),
            Some(Vec3::new(0., 0., 10.)),
        )
        .unwrap();
    for (z, radius) in [(0.00001, 2.), (10., 4.), (19.99999, 3.)] {
        let (_, r) = session
            .ray_first_hit(&three, Vec3::new(0., 0., z), Vec3::new(1., 0., 0.), 10.)
            .unwrap()
            .unwrap();
        assert!((r - radius).abs() < 1e-5);
    }
    let square = session
        .create_polyline_wire(
            &[
                Vec3::new(-2., -2., 10.),
                Vec3::new(2., -2., 10.),
                Vec3::new(2., 2., 10.),
                Vec3::new(-2., 2., 10.),
            ],
            true,
        )
        .unwrap();
    assert!(
        session
            .create_constrained_loft(&[&a, &square], Some(Vec3::new(0., 0., 10.)), None)
            .is_err()
    );
}

#[test]
fn constrained_polygon_lofts_preserve_ordered_sections_and_single_end_constraints() {
    let session = Session::new().unwrap();
    let square = |r: f64, z: f64| {
        session
            .create_polyline_wire(
                &[
                    Vec3::new(-r, -r, z),
                    Vec3::new(r, -r, z),
                    Vec3::new(r, r, z),
                    Vec3::new(-r, r, z),
                ],
                true,
            )
            .unwrap()
    };
    let a = square(1., 0.);
    let b = square(2., 10.);
    let t = Vec3::new(0., 0., 10.);
    let solid = session
        .create_constrained_loft(&[&a, &b], Some(t), Some(t))
        .unwrap();
    assert!((session.volume(&solid).unwrap() - 3320. / 35.).abs() < 1e-6);
    let one = session
        .create_constrained_loft(&[&a, &b], Some(Vec3::new(2., 0., 10.)), None)
        .unwrap();
    assert!(session.is_valid(&one).unwrap());
}

#[test]
fn exact_profile_alignment_repairs_polygon_start_and_reverse_without_input_mutation() {
    let s = Session::new().unwrap();
    {
        let a = s
            .create_polyline_wire(
                &[
                    Vec3::new(-1., -1., 0.),
                    Vec3::new(1., -1., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(-1., 1., 0.),
                ],
                true,
            )
            .unwrap();
        // Same section, authored from another corner with opposite traversal.
        let b = s
            .create_polyline_wire(
                &[
                    Vec3::new(1., 1., 10.),
                    Vec3::new(1., -1., 10.),
                    Vec3::new(-1., -1., 10.),
                    Vec3::new(-1., 1., 10.),
                ],
                true,
            )
            .unwrap();
        let before = s.exact_bounds(&b).unwrap();
        let aligned = s
            .align_profile_wire(&b, Some(Vec3::new(-1., -1., 10.)), true)
            .unwrap();
        let t = Vec3::new(0., 0., 10.);
        let loft = s
            .create_constrained_loft(&[&a, &aligned], Some(t), Some(t))
            .unwrap();
        assert!((s.volume(&loft).unwrap() - 40.).abs() < 1e-7);
        assert_eq!(s.exact_bounds(&b).unwrap(), before);
        assert!(
            s.align_profile_wire(&b, Some(Vec3::new(0., -1., 10.)), false)
                .is_err()
        );
        assert!(
            s.align_profile_wire(&b, Some(Vec3::new(9., 9., 10.)), false)
                .is_err()
        );
        let edge = s.subshape(&b, ShapeType::Edge, 0).unwrap();
        let loft = s.compose_history(&loft, &aligned).unwrap();
        assert!(
            s.history_count(&loft, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn exact_periodic_seams_preserve_curves_volume_and_original_edge_history() {
    let s = Session::new().unwrap();
    {
        let a = s
            .create_circle_wire(Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 2.)
            .unwrap();
        let b = s
            .create_circle_wire(Vec3::new(0., 0., 10.), Vec3::new(0., 0., 1.), 2.)
            .unwrap();
        let ea = s.subshape(&a, ShapeType::Edge, 0).unwrap();
        for reverse in [false, true] {
            let aa = s
                .align_profile_wire(&a, Some(Vec3::new(0., 2., 0.)), reverse)
                .unwrap();
            let bb = s
                .align_profile_wire(&b, Some(Vec3::new(0., 2., 10.)), reverse)
                .unwrap();
            assert_eq!(s.subshape_count(&aa, ShapeType::Edge).unwrap(), 1);
            let f = s.create_face_from_wire(&aa).unwrap();
            assert!((s.surface_area(&f).unwrap() - 4. * std::f64::consts::PI).abs() < 1e-7);
            let t = Vec3::new(0., 0., 10.);
            let solid = s
                .create_constrained_loft(&[&aa, &bb], Some(t), Some(t))
                .unwrap();
            assert!((s.volume(&solid).unwrap() - 40. * std::f64::consts::PI).abs() < 1e-6);
            let solid = s.compose_history(&solid, &aa).unwrap();
            assert!(
                s.history_count(&solid, &ea, HistoryRelation::Generated)
                    .unwrap()
                    > 0
            );
        }
        assert!(
            s.align_profile_wire(&a, Some(Vec3::new(0., 3., 0.)), false)
                .is_err()
        );
        assert!(
            s.align_profile_wire(&a, Some(Vec3::new(f64::NAN, 2., 0.)), false)
                .is_err()
        );
        assert_eq!(s.subshape_count(&a, ShapeType::Edge).unwrap(), 1);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}
