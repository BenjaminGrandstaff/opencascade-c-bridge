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

#[test]
fn holed_faces_keep_area_and_volume_across_boundary_reversal_and_rotated_planes() {
    let s = Session::new().unwrap();
    {
        let p = |x, y| Vec3::new(500. + x, -200., 70. + y);
        let normal = Vec3::new(0., -1., 0.);
        let outer = s
            .create_polyline_wire(&[p(0., 0.), p(20., 0.), p(20., 12.), p(0., 12.)], true)
            .unwrap();
        let circle = s.create_circle_wire(p(5., 5.), normal, 2.).unwrap();
        let ellipse = s
            .create_ellipse_wire_axes(p(14., 6.), normal, Vec3::new(1., 0., 0.), 3., 1.)
            .unwrap();
        let expected = 240. - 7. * PI;
        for reverse_outer in [false, true] {
            for reverse_holes in [false, true] {
                let outer = s.align_profile_wire(&outer, None, reverse_outer).unwrap();
                let circle = s.align_profile_wire(&circle, None, reverse_holes).unwrap();
                let ellipse = s
                    .align_profile_wire(&ellipse, None, !reverse_holes)
                    .unwrap();
                let bounds = [&outer, &circle, &ellipse].map(|w| s.exact_bounds(w).unwrap());
                for holes in [[&circle, &ellipse], [&ellipse, &circle]] {
                    let face = s.create_face_with_holes(&outer, &holes).unwrap();
                    assert!(s.is_valid(&face).unwrap());
                    assert_eq!(s.subshape_count(&face, ShapeType::Wire).unwrap(), 3);
                    assert!((s.surface_area(&face).unwrap() - expected).abs() < 1e-7);
                    let prism = s
                        .create_prism_from_face(&face, Vec3::new(0., -4., 0.))
                        .unwrap();
                    assert!((s.volume(&prism).unwrap() - 4. * expected).abs() < 1e-6);
                }
                assert_eq!(
                    [&outer, &circle, &ellipse].map(|w| s.exact_bounds(w).unwrap()),
                    bounds
                );
            }
        }
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn close_hole_pairs_and_nested_orders_fail_without_partial_results() {
    let s = Session::new().unwrap();
    {
        let normal = Vec3::new(0., 0., 1.);
        let outer = s
            .create_circle_wire(Vec3::new(0., 0., 0.), normal, 10.)
            .unwrap();
        for (radius, x) in [(1., 2. + 5e-8), (0.5, 0.)] {
            let a = s
                .create_circle_wire(Vec3::new(0., 0., 0.), normal, 1.)
                .unwrap();
            let b = s
                .create_circle_wire(Vec3::new(x, 0., 0.), normal, radius)
                .unwrap();
            for holes in [[&a, &b], [&b, &a]] {
                let before = s.shape_count().unwrap();
                let error = s.create_face_with_holes(&outer, &holes).unwrap_err();
                assert_eq!(error.status, 4);
                assert_eq!(error.message, "inner wires overlap, nest or touch");
                assert_eq!(s.shape_count().unwrap(), before);
            }
        }
        let inside = s
            .create_circle_wire(Vec3::new(2. + 1e-4, 0., 0.), normal, 1.)
            .unwrap();
        let center = s
            .create_circle_wire(Vec3::new(0., 0., 0.), normal, 1.)
            .unwrap();
        let face = s
            .create_face_with_holes(&outer, &[&center, &inside])
            .unwrap();
        assert!((s.surface_area(&face).unwrap() - 98. * PI).abs() < 1e-7);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}
