use super::*;
use std::f64::consts::PI;

#[test]
fn faces_with_holes_flip_inner_orientation_and_reject_bad_layouts() {
    let s = Session::new().unwrap();
    let z = Vec3::new(0., 0., 1.);
    let p = |x, y| Vec3::new(x, y, 0.);
    {
        let outer = s
            .create_polyline_wire(&[p(0., 0.), p(40., 0.), p(40., 20.), p(0., 20.)], true)
            .unwrap();
        let circle = |x: f64, y: f64, r: f64| s.create_circle_wire(p(x, y), z, r).unwrap();
        // Same orientation as the outer loop: flipped natively into a hole.
        let square = s
            .create_polyline_wire(&[p(30., 5.), p(35., 5.), p(35., 10.), p(30., 10.)], true)
            .unwrap();
        let ellipse = s
            .create_ellipse_wire_axes(p(20., 14.), z, Vec3::new(1., 0., 0.), 4., 2.)
            .unwrap();
        let (a, b) = (circle(8., 10., 3.), circle(20., 6., 2.));
        let face = s
            .create_face_with_holes(&outer, &[&a, &b, &square, &ellipse])
            .unwrap();
        assert!(s.is_valid(&face).unwrap());
        assert_eq!(s.subshape_count(&face, ShapeType::Wire).unwrap(), 5);
        let expected = 800. - PI * 9. - PI * 4. - 25. - PI * 8.;
        assert!((s.surface_area(&face).unwrap() - expected).abs() < 1e-6);
        let plain = s.create_face_with_holes(&outer, &[]).unwrap();
        assert!((s.surface_area(&plain).unwrap() - 800.).abs() < 1e-9);
        let prism = s
            .create_prism_from_face(&face, Vec3::new(0., 0., 5.))
            .unwrap();
        assert!((s.volume(&prism).unwrap() - 5. * expected).abs() < 1e-5);

        let count = s.shape_count().unwrap();
        let open = s
            .create_polyline_wire(&[p(1., 1.), p(2., 1.)], false)
            .unwrap();
        let above = s.create_circle_wire(Vec3::new(8., 10., 1.), z, 1.).unwrap();
        let count = count + 2;
        for (holes, case) in [
            (vec![circle(50., 10., 2.)], "outside"),
            (vec![circle(38., 10., 2.)], "crossing the boundary"),
            (vec![circle(37., 10., 3.)], "touching the boundary"),
            (
                vec![circle(8., 10., 3.), circle(11., 10., 1.)],
                "overlapping",
            ),
            (
                vec![circle(8., 10., 3.), circle(14., 10., 3.)],
                "touching each other",
            ),
            (vec![circle(8., 10., 3.), circle(8., 10., 1.)], "nested"),
        ] {
            let refs = holes.iter().collect::<Vec<_>>();
            let error = s.create_face_with_holes(&outer, &refs).unwrap_err();
            assert_eq!(error.status, 4, "{case}: {}", error.message);
        }
        assert!(s.create_face_with_holes(&outer, &[&above]).is_err());
        assert!(s.create_face_with_holes(&outer, &[&open]).is_err());
        assert!(s.create_face_with_holes(&open, &[]).is_err());
        let other = Session::new().unwrap();
        assert_wrong_session(other.create_face_with_holes(&outer, &[]).unwrap_err());
        // Each rejected case released its scratch circles when dropped.
        assert_eq!(s.shape_count().unwrap(), count);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}
