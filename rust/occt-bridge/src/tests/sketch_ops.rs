use super::*;
#[test]
fn native_profile_edits_preserve_inputs_project_exactly_and_release_handles() {
    let session = Session::new().unwrap();
    let zero = Vec3::new(0., 0., 0.);
    let normal = Vec3::new(0., 0., 1.);
    {
        let source = session
            .create_curve_wire(
                &[CurveSegment::Line {
                    start: zero,
                    end: Vec3::new(10., 0., 0.),
                }],
                false,
            )
            .unwrap();
        let trimmed = session.trim_curve(&source, 0.2, 0.8).unwrap();
        let extended = session.extend_curve(&trimmed, 1., 2.).unwrap();
        let b = session.exact_bounds(&extended).unwrap();
        assert!((b.min.x - 1.).abs() < 1e-7 && (b.max.x - 10.).abs() < 1e-7);
        let nearest = session
            .curve_closest_point(&extended, Vec3::new(5., 3., 0.))
            .unwrap();
        assert_eq!(nearest, Vec3::new(5., 0., 0.));
        assert_eq!(session.exact_bounds(&source).unwrap().min, zero);
        assert!(!session.wire_is_closed(&extended).unwrap());
        let shifted = session.offset_wire(&extended, normal, 2., true).unwrap();
        assert!((session.exact_bounds(&shifted).unwrap().min.y + 2.).abs() < 1e-7);
        let left = session.offset_wire(&extended, normal, -2.0, true).unwrap();
        assert!(
            (session.exact_bounds(&left).unwrap().min.y - 2.0).abs() < 1e-6,
            "left bounds {:?}",
            session.exact_bounds(&left).unwrap()
        );
        let corner = session
            .create_polyline_wire(
                &[zero, Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 0.0)],
                false,
            )
            .unwrap();
        for distance in [-1.0, 1.0] {
            let edited = session
                .offset_wire(&corner, normal, distance, false)
                .unwrap();
            let p = session
                .curve_closest_point(&edited, Vec3::new(5.0, 0.0, 0.0))
                .unwrap();
            assert!(
                (p.y + distance).abs() < 1e-7,
                "open corner offset side: {distance} -> {p:?}"
            );
        }
        let ellipse = session
            .create_ellipse_wire_axes(zero, normal, Vec3::new(1., 1., 0.), 6., 3.)
            .unwrap();
        assert!(session.wire_is_closed(&ellipse).unwrap());
        let arc = session.trim_curve(&ellipse, 0., 0.5).unwrap();
        assert!(!session.wire_is_closed(&arc).unwrap());
        let other = Session::new().unwrap();
        assert_wrong_session(other.trim_curve(&source, 0., 1.).unwrap_err());
        assert_wrong_session(other.join_wires(&[&source], false).unwrap_err());
        let before = session.shape_count().unwrap();
        assert!(session.trim_curve(&source, 0.7, 0.2).is_err());
        assert!(session.extend_curve(&ellipse, 1., 0.).is_err());
        assert!(session.offset_wire(&source, normal, 0., false).is_err());
        assert!(session.join_wires(&[&source], true).is_err());
        assert_eq!(session.shape_count().unwrap(), before);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
