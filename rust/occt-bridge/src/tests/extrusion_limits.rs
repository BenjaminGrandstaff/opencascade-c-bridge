use super::*;

fn profile(session: &Session) -> Shape<'_> {
    let wire = session
        .create_circle_wire(Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 2.)
        .unwrap();
    session.create_face_from_wire(&wire).unwrap()
}

#[test]
fn finite_inclined_and_spherical_caps_cover_the_whole_profile() {
    let session = Session::new().unwrap();
    {
        let face = profile(&session);
        let plane_wire = session
            .create_circle_wire(Vec3::new(0., 0., 10.), Vec3::new(0., 0., 1.), 10.)
            .unwrap();
        let plane = session.create_face_from_wire(&plane_wire).unwrap();
        let inclined = session
            .rotate(&plane, Vec3::new(0., 0., 10.), Vec3::new(0., 1., 0.), 0.4)
            .unwrap();
        let solid = session
            .create_prism_until_face(&face, Vec3::new(0., 0., 30.), &inclined)
            .unwrap();
        assert_eq!(
            session
                .create_prism_until_face(&face, Vec3::new(0., 0., 10.2), &inclined)
                .unwrap_err()
                .status,
            4
        );
        assert!((session.volume(&solid).unwrap() - 40. * std::f64::consts::PI).abs() < 1e-7);
        let bounds = session.exact_bounds(&solid).unwrap();
        assert!(
            (bounds.max.z - (10. + 2. * 0.4_f64.tan())).abs() < 1e-6,
            "{bounds:?}"
        );
        let edge = session.subshape(&face, ShapeType::Edge, 0).unwrap();
        assert!(
            session
                .history_count(&solid, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
        let sphere = session.create_sphere(Vec3::new(0., 0., 15.), 5.).unwrap();
        let spherical_face = session.subshape(&sphere, ShapeType::Face, 0).unwrap();
        let rounded = session
            .create_prism_until_face(&face, Vec3::new(0., 0., 30.), &spherical_face)
            .unwrap();
        let r = 5.0_f64;
        let expected = 15. * 4. * std::f64::consts::PI
            - 2. * std::f64::consts::PI / 3. * (r.powi(3) - (r * r - 4.).powf(1.5));
        assert!((session.volume(&rounded).unwrap() - expected).abs() < 1e-6);
        assert!(session.is_valid(&rounded).unwrap());
        assert_eq!(session.shape_type(&rounded).unwrap(), ShapeType::Solid);
        assert!(
            session
                .history_count(&rounded, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
        assert!(
            session
                .history_count(&rounded, &spherical_face, HistoryRelation::Modified)
                .unwrap()
                > 0
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn bounded_limit_rejects_partial_backward_touching_and_short_search() {
    let session = Session::new().unwrap();
    {
        let face = profile(&session);
        for (z, r, travel) in [
            (10., 1., 30.),
            (-10., 10., 30.),
            (0., 10., 30.),
            (10., 10., 5.),
        ] {
            let wire = session
                .create_circle_wire(Vec3::new(0., 0., z), Vec3::new(0., 0., 1.), r)
                .unwrap();
            let limit = session.create_face_from_wire(&wire).unwrap();
            assert_eq!(
                session
                    .create_prism_until_face(&face, Vec3::new(0., 0., travel), &limit)
                    .unwrap_err()
                    .status,
                4
            );
        }
        let second = Session::new().unwrap();
        let foreign = profile(&second);
        assert_wrong_session(
            session
                .create_prism_until_face(&face, Vec3::new(0., 0., 30.), &foreign)
                .unwrap_err(),
        );
        assert_wrong_session(
            session
                .create_prism_until_face(&foreign, Vec3::new(0., 0., 30.), &face)
                .unwrap_err(),
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn native_ray_measures_curved_caps_without_centroid_estimates() {
    let session = Session::new().unwrap();
    {
        let face = profile(&session);
        let sphere = session.create_sphere(Vec3::new(0., 0., 15.), 5.).unwrap();
        let cap = session.subshape(&sphere, ShapeType::Face, 0).unwrap();
        let solid = session
            .create_prism_until_face(&face, Vec3::new(0., 0., 30.), &cap)
            .unwrap();
        let (point, length) = session
            .ray_first_hit(&solid, Vec3::new(0., 0., 0.), Vec3::new(0., 0., 2.), 30.)
            .unwrap()
            .unwrap();
        assert!((length - 10.).abs() < 1e-7);
        assert!((point.z - 10.).abs() < 1e-7);
        let (_, tiny_axis_length) = session
            .ray_first_hit(&solid, Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1e-12), 30.)
            .unwrap()
            .unwrap();
        assert!((tiny_axis_length - 10.).abs() < 1e-7);
        assert!(
            session
                .ray_first_hit(&solid, Vec3::new(5., 0., 0.), Vec3::new(0., 0., 1.), 30.)
                .unwrap()
                .is_none()
        );
        assert!(
            session
                .ray_first_hit(&solid, Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 5.)
                .unwrap()
                .is_none()
        );
        assert!(
            session
                .ray_first_hit(&solid, Vec3::new(0., 0., 0.), Vec3::new(0., 0., 0.), 30.)
                .is_err()
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
