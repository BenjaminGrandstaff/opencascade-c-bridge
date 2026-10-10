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

#[test]
fn native_corner_edits_preserve_area_order_inputs_and_handles() {
    let session = Session::new().unwrap();
    {
        let p = |x, y| Vec3::new(x, y, 0.);
        let source = session
            .create_polyline_wire(&[p(0., 0.), p(20., 0.), p(20., 10.), p(0., 10.)], true)
            .unwrap();
        let fillet = session
            .fillet_wire_corner(&source, p(20., 10.), 2.)
            .unwrap();
        let chamfer = session
            .chamfer_wire_corner(&fillet, p(0., 0.), 3., 5.)
            .unwrap();
        let face = session.create_face_from_wire(&chamfer).unwrap();
        assert!(session.is_valid(&face).unwrap());
        assert!(
            (session.surface_area(&face).unwrap() - (200. - (4. - std::f64::consts::PI) - 7.5))
                .abs()
                < 1e-7
        );
        for point in [p(0., 3.), p(5., 0.)] {
            assert!(
                session
                    .subshapes(&chamfer, ShapeType::Vertex)
                    .unwrap()
                    .iter()
                    .any(
                        |v| (session.exact_bounds(v).unwrap().min.x - point.x).abs() < 1e-7
                            && (session.exact_bounds(v).unwrap().min.y - point.y).abs() < 1e-7
                    )
            );
        }
        let original = session.create_face_from_wire(&source).unwrap();
        assert!((session.surface_area(&original).unwrap() - 200.).abs() < 1e-7);
        let count = session.shape_count().unwrap();
        for point in [p(20., 10.), p(100., 100.)] {
            assert!(session.fillet_wire_corner(&fillet, point, 2.).is_err());
        }
        for radius in [0., -1., 100., f64::NAN] {
            assert!(
                session
                    .fillet_wire_corner(&source, p(0., 0.), radius)
                    .is_err()
            );
        }
        assert!(
            session
                .chamfer_wire_corner(&source, p(0., 0.), 21., 3.)
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), count);
        let other = Session::new().unwrap();
        assert_wrong_session(
            other
                .fillet_wire_corner(&source, p(0., 0.), 2.)
                .unwrap_err(),
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn native_corner_edits_accept_rotated_planes_and_reject_nonplanar_and_spline_neighbors() {
    let s = Session::new().unwrap();
    let p = |x, y| Vec3::new(100. + x, 200., 300. + y);
    let wire = s
        .create_polyline_wire(&[p(0., 0.), p(20., 0.), p(20., 10.), p(0., 10.)], true)
        .unwrap();
    let edit = s.fillet_wire_corner(&wire, p(20., 10.), 2.).unwrap();
    let face = s.create_face_from_wire(&edit).unwrap();
    assert!((s.surface_area(&face).unwrap() - (200. - 4. + std::f64::consts::PI)).abs() < 1e-7);
    let open = s
        .create_polyline_wire(&[p(0., 0.), p(20., 0.), p(20., 10.)], false)
        .unwrap();
    let rounded_open = s.fillet_wire_corner(&open, p(20., 0.), 2.).unwrap();
    assert!(!s.wire_is_closed(&rounded_open).unwrap());
    let crooked = s
        .create_polyline_wire(
            &[
                Vec3::new(0., 0., 0.),
                Vec3::new(10., 0., 0.),
                Vec3::new(10., 10., 1.),
                Vec3::new(0., 10., 0.),
            ],
            true,
        )
        .unwrap();
    assert!(
        s.fillet_wire_corner(&crooked, Vec3::new(0., 0., 0.), 1.)
            .is_err()
    );
    let curved = s
        .create_curve_wire(
            &[
                CurveSegment::Spline {
                    points: vec![
                        Vec3::new(0., 0., 0.),
                        Vec3::new(5., 2., 0.),
                        Vec3::new(10., 0., 0.),
                    ],
                    start_tangent: None,
                    end_tangent: None,
                    periodic: false,
                },
                CurveSegment::Line {
                    start: Vec3::new(10., 0., 0.),
                    end: Vec3::new(0., 0., 0.),
                },
            ],
            true,
        )
        .unwrap();
    assert!(
        s.fillet_wire_corner(&curved, Vec3::new(10., 0., 0.), 1.)
            .is_err()
    );
}

#[test]
fn native_corner_edits_support_circular_arc_neighbors_and_curvilinear_chamfer_sizes() {
    let s = Session::new().unwrap();
    let p = |x, y| Vec3::new(x, y, 0.);
    let source = s
        .create_curve_wire(
            &[
                CurveSegment::Line {
                    start: p(0., 0.),
                    end: p(10., 0.),
                },
                CurveSegment::Arc {
                    start: p(10., 0.),
                    middle: p(10. / 2f64.sqrt(), 10. / 2f64.sqrt()),
                    end: p(0., 10.),
                },
                CurveSegment::Line {
                    start: p(0., 10.),
                    end: p(0., 0.),
                },
            ],
            true,
        )
        .unwrap();
    let rounded = s.fillet_wire_corner(&source, p(10., 0.), 1.).unwrap();
    let face = s.create_face_from_wire(&rounded).unwrap();
    assert!(s.is_valid(&face).unwrap());
    assert!(s.surface_area(&face).unwrap() < 25. * std::f64::consts::PI);
    let cut = s.chamfer_wire_corner(&source, p(0., 10.), 1., 2.).unwrap();
    let vertices = s.subshapes(&cut, ShapeType::Vertex).unwrap();
    for point in [p(10. * 0.1f64.sin(), 10. * 0.1f64.cos()), p(0., 8.)] {
        assert!(
            vertices.iter().any(|v| {
                let q = s.exact_bounds(v).unwrap().min;
                (q.x - point.x).hypot(q.y - point.y) < 1e-6
            }),
            "{point:?}"
        );
    }
}

#[test]
fn native_open_corner_edits_keep_endpoints_order_length_and_handles() {
    let s = Session::new().unwrap();
    let p = |x, y| Vec3::new(x, y, 0.);
    {
        let wire = s
            .create_polyline_wire(&[p(0., 0.), p(20., 0.), p(20., 10.), p(40., 10.)], false)
            .unwrap();
        let rounded = s.fillet_wire_corner(&wire, p(20., 0.), 2.).unwrap();
        let treated = s
            .chamfer_wire_corner(&rounded, p(20., 10.), 3., 5.)
            .unwrap();
        assert!(s.is_valid(&treated).unwrap());
        assert!(!s.wire_is_closed(&treated).unwrap());
        let edges = s.subshapes(&treated, ShapeType::Edge).unwrap();
        assert_eq!(edges.len(), 5);
        let length = edges.iter().map(|e| s.edge_length(e).unwrap()).sum::<f64>();
        assert!((length - (50. - 4. + std::f64::consts::PI - 8. + 34f64.sqrt())).abs() < 1e-7);
        let vertices = s.subshapes(&treated, ShapeType::Vertex).unwrap();
        for point in [p(0., 0.), p(40., 10.), p(20., 7.), p(25., 10.)] {
            assert!(vertices.iter().any(|v| {
                let q = s.exact_bounds(v).unwrap().min;
                (q.x - point.x).hypot(q.y - point.y) < 1e-7
            }));
        }
        let count = s.shape_count().unwrap();
        for point in [p(0., 0.), p(40., 10.)] {
            assert!(s.fillet_wire_corner(&wire, point, 2.).is_err());
        }
        assert!(s.fillet_wire_corner(&wire, p(20., 0.), 100.).is_err());
        assert!(s.chamfer_wire_corner(&wire, p(20., 0.), 20., 2.).is_err());
        assert_eq!(s.shape_count().unwrap(), count);
        let other = Session::new().unwrap();
        assert_wrong_session(
            other
                .chamfer_wire_corner(&wire, p(20., 0.), 2., 3.)
                .unwrap_err(),
        );
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn native_open_corner_edits_support_arcs_and_reversed_distance_roles() {
    let s = Session::new().unwrap();
    let p = |x, y| Vec3::new(x, y, 0.);
    let wire = s
        .create_curve_wire(
            &[
                CurveSegment::Line {
                    start: p(0., 0.),
                    end: p(10., 0.),
                },
                CurveSegment::Arc {
                    start: p(10., 0.),
                    middle: p(10. / 2f64.sqrt(), 10. / 2f64.sqrt()),
                    end: p(0., 10.),
                },
            ],
            false,
        )
        .unwrap();
    let rounded = s.fillet_wire_corner(&wire, p(10., 0.), 1.).unwrap();
    assert!(s.is_valid(&rounded).unwrap() && !s.wire_is_closed(&rounded).unwrap());
    let cut = s.chamfer_wire_corner(&wire, p(10., 0.), 2., 3.).unwrap();
    let vertices = s.subshapes(&cut, ShapeType::Vertex).unwrap();
    for point in [
        p(8., 0.),
        p(10. * 0.3f64.cos(), 10. * 0.3f64.sin()),
        p(0., 0.),
        p(0., 10.),
    ] {
        assert!(
            vertices.iter().any(|v| {
                let q = s.exact_bounds(v).unwrap().min;
                (q.x - point.x).hypot(q.y - point.y) < 1e-6
            }),
            "{point:?}"
        );
    }
    let reverse_arc = s
        .create_curve_wire(
            &[
                CurveSegment::Arc {
                    start: p(0., 10.),
                    middle: p(10. / 2f64.sqrt(), 10. / 2f64.sqrt()),
                    end: p(10., 0.),
                },
                CurveSegment::Line {
                    start: p(10., 0.),
                    end: p(0., 0.),
                },
            ],
            false,
        )
        .unwrap();
    let reverse_fillet = s.fillet_wire_corner(&reverse_arc, p(10., 0.), 1.).unwrap();
    assert!(s.is_valid(&reverse_fillet).unwrap() && !s.wire_is_closed(&reverse_fillet).unwrap());
    let reverse = s
        .create_polyline_wire(&[p(20., 10.), p(20., 0.), p(0., 0.)], false)
        .unwrap();
    let cut = s.chamfer_wire_corner(&reverse, p(20., 0.), 2., 3.).unwrap();
    let vertices = s.subshapes(&cut, ShapeType::Vertex).unwrap();
    for point in [p(20., 2.), p(17., 0.)] {
        assert!(vertices.iter().any(|v| {
            let q = s.exact_bounds(v).unwrap().min;
            (q.x - point.x).hypot(q.y - point.y) < 1e-7
        }));
    }
}

#[test]
fn native_open_corner_edits_preserve_small_large_and_distant_rotated_geometry() {
    let s = Session::new().unwrap();
    for scale in [0.01, 1., 1000.] {
        let p = |x, y| Vec3::new(1e6 + x * scale, 2e6, 3e6 + y * scale);
        let wire = s
            .create_polyline_wire(&[p(0., 0.), p(20., 0.), p(20., 10.)], false)
            .unwrap();
        let rounded = s.fillet_wire_corner(&wire, p(20., 0.), 2. * scale).unwrap();
        assert!(s.is_valid(&rounded).unwrap());
        let length = s
            .subshapes(&rounded, ShapeType::Edge)
            .unwrap()
            .iter()
            .map(|e| s.edge_length(e).unwrap())
            .sum::<f64>();
        assert!(
            (length - (26. + std::f64::consts::PI) * scale).abs() < scale * 1e-5,
            "{scale}: {length}"
        );
    }
    let p = |x, y, z| Vec3::new(x, y, z);
    let wire = s
        .create_polyline_wire(
            &[
                p(0., 0., 0.),
                p(20., 0., 0.),
                p(20., 10., 0.),
                p(40., 10., 1.),
            ],
            false,
        )
        .unwrap();
    assert!(s.fillet_wire_corner(&wire, p(20., 0., 0.), 2.).is_err());
}
