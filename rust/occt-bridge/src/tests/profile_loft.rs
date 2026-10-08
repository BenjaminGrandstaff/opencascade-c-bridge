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
