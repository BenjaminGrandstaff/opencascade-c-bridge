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
fn native_corner_edits_accept_rotated_planes_and_spline_neighbors_and_reject_nonplanar() {
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
    // The lens is under 2 mm tall near the tip: r=1 only fits as a mid-lens
    // half circle, which is not a corner fillet.
    assert!(
        s.fillet_wire_corner(&curved, Vec3::new(10., 0., 0.), 1.)
            .is_err()
    );
    let rounded = s
        .fillet_wire_corner(&curved, Vec3::new(10., 0., 0.), 0.25)
        .unwrap();
    let face = s.create_face_from_wire(&rounded).unwrap();
    assert!(s.is_valid(&face).unwrap());
    let length = assert_tangent_fillet(&s, &rounded, 0.25);
    assert!(length > 20. && length < 21., "{length}");
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

/// Asserts one circular edge of `radius` whose ends meet their neighbors
/// tangentially, and returns the summed edge length.
fn assert_tangent_fillet(s: &Session, wire: &Shape<'_>, radius: f64) -> f64 {
    let edges = s.subshapes(wire, ShapeType::Edge).unwrap();
    let ends = edges
        .iter()
        .map(|e| s.curve_end_data(e).unwrap())
        .collect::<Vec<_>>();
    let arcs = edges
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            s.edge_circle_radius(e)
                .unwrap()
                .is_some_and(|r| (r - radius).abs() < 1e-9 * radius.max(1.))
        })
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert!(!arcs.is_empty(), "no fillet arc of radius {radius}");
    let unit = |v: Vec3| {
        let n = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
        Vec3::new(v.x / n, v.y / n, v.z / n)
    };
    let close =
        |a: Vec3, b: Vec3| (a.x - b.x).hypot(a.y - b.y).hypot(a.z - b.z) < 1e-7 * radius.max(1.);
    let tangent = arcs.iter().any(|&i| {
        let (a, b, u, v) = ends[i];
        [(a, u), (b, v)].into_iter().all(|(point, direction)| {
            ends.iter().enumerate().any(|(j, &(c, d, w, x))| {
                j != i
                    && [(c, w), (d, x)].into_iter().any(|(q, t)| {
                        let (m, n) = (unit(direction), unit(t));
                        close(point, q) && (m.x * n.x + m.y * n.y + m.z * n.z).abs() > 1. - 1e-7
                    })
            })
        })
    });
    assert!(tangent, "fillet arc is not tangent to both neighbors");
    edges.iter().map(|e| s.edge_length(e).unwrap()).sum()
}

fn has_vertex(s: &Session, wire: &Shape<'_>, point: Vec3, tolerance: f64) -> bool {
    s.subshapes(wire, ShapeType::Vertex)
        .unwrap()
        .iter()
        .any(|v| {
            let q = s.exact_bounds(v).unwrap().min;
            (q.x - point.x).hypot(q.y - point.y).hypot(q.z - point.z) < tolerance
        })
}

#[test]
fn native_open_corner_edits_round_and_cut_spline_neighbors() {
    let s = Session::new().unwrap();
    let p = |x, y| Vec3::new(x, y, 0.);
    {
        let wire = s
            .create_curve_wire(
                &[
                    CurveSegment::Spline {
                        points: vec![p(0., 0.), p(10., 4.), p(20., 0.)],
                        start_tangent: None,
                        end_tangent: None,
                        periodic: false,
                    },
                    CurveSegment::Line {
                        start: p(20., 0.),
                        end: p(20., 15.),
                    },
                ],
                false,
            )
            .unwrap();
        let source = s
            .subshapes(&wire, ShapeType::Edge)
            .unwrap()
            .iter()
            .map(|e| s.edge_length(e).unwrap())
            .collect::<Vec<_>>();
        let rounded = s.fillet_wire_corner(&wire, p(20., 0.), 2.).unwrap();
        assert!(s.is_valid(&rounded).unwrap());
        assert!(!s.wire_is_closed(&rounded).unwrap());
        assert_eq!(s.subshapes(&rounded, ShapeType::Edge).unwrap().len(), 3);
        let length = assert_tangent_fillet(&s, &rounded, 2.);
        assert!(length < source.iter().sum::<f64>());
        for point in [p(0., 0.), p(20., 15.)] {
            assert!(has_vertex(&s, &rounded, point, 1e-9));
        }
        let cut = s.chamfer_wire_corner(&wire, p(20., 0.), 3., 4.).unwrap();
        let lengths = s
            .subshapes(&cut, ShapeType::Edge)
            .unwrap()
            .iter()
            .map(|e| s.edge_length(e).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(lengths.len(), 3);
        let spline = source.iter().copied().fold(0., f64::max);
        for expected in [spline - 3., 11.] {
            // Spline length queries are adaptive; trims use 1e-7 abscissa tolerance.
            assert!(
                lengths.iter().any(|l| (l - expected).abs() < 1e-4),
                "{lengths:?}"
            );
        }
        assert!(has_vertex(&s, &cut, p(20., 4.), 1e-9));
        let count = s.shape_count().unwrap();
        assert!(s.fillet_wire_corner(&wire, p(20., 0.), 50.).is_err());
        assert!(s.chamfer_wire_corner(&wire, p(20., 0.), 30., 1.).is_err());
        assert_eq!(s.shape_count().unwrap(), count);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn native_closed_corner_edits_round_spline_and_ellipse_neighbors() {
    let s = Session::new().unwrap();
    let p = |x, y| Vec3::new(x, y, 0.);
    let ellipse = s
        .create_ellipse_wire_axes(
            p(0., 0.),
            Vec3::new(0., 0., 1.),
            Vec3::new(1., 0., 0.),
            10.,
            5.,
        )
        .unwrap();
    let upper = s.trim_curve(&ellipse, 0., 0.5).unwrap();
    let base = s
        .create_polyline_wire(&[p(-10., 0.), p(10., 0.)], false)
        .unwrap();
    let half = s.join_wires(&[&upper, &base], true).unwrap();
    let area = |w: &Shape<'_>| {
        s.surface_area(&s.create_face_from_wire(w).unwrap())
            .unwrap()
    };
    let full = area(&half);
    assert!((full - 25. * std::f64::consts::PI).abs() < 1e-6);
    let rounded = s.fillet_wire_corner(&half, p(10., 0.), 1.).unwrap();
    assert!(s.wire_is_closed(&rounded).unwrap());
    assert_tangent_fillet(&s, &rounded, 1.);
    let treated = s
        .chamfer_wire_corner(&rounded, p(-10., 0.), 1., 2.)
        .unwrap();
    assert_eq!(s.subshapes(&treated, ShapeType::Edge).unwrap().len(), 4);
    let (a, b) = (area(&rounded), area(&treated));
    assert!(a < full && a > full - 1. && b < a && b > a - 2.);
    assert!(has_vertex(&s, &treated, p(-8., 0.), 1e-9));

    let lens = s
        .create_curve_wire(
            &[
                CurveSegment::Spline {
                    points: vec![p(0., 0.), p(10., 5.), p(20., 0.)],
                    start_tangent: None,
                    end_tangent: None,
                    periodic: false,
                },
                CurveSegment::Spline {
                    points: vec![p(20., 0.), p(10., -5.), p(0., 0.)],
                    start_tangent: None,
                    end_tangent: None,
                    periodic: false,
                },
            ],
            true,
        )
        .unwrap();
    let once = s.fillet_wire_corner(&lens, p(20., 0.), 1.).unwrap();
    let twice = s.fillet_wire_corner(&once, p(0., 0.), 1.).unwrap();
    // Each spline shares both lens vertices; the fillet must stay at the
    // selected tip instead of collapsing the lens around the other one.
    let source = s.subshapes(&lens, ShapeType::Edge).unwrap();
    let total = source
        .iter()
        .map(|e| s.edge_length(e).unwrap())
        .sum::<f64>();
    for (wire, tips) in [(&once, 1.), (&twice, 2.)] {
        let length = assert_tangent_fillet(&s, wire, 1.);
        assert!(
            length < total && length > total - 2.5 * tips,
            "{length} of {total}"
        );
    }
    assert_eq!(s.subshapes(&twice, ShapeType::Edge).unwrap().len(), 4);
    let (lens_area, once_area, twice_area) = (area(&lens), area(&once), area(&twice));
    assert!(twice_area < once_area && once_area < lens_area && twice_area > 0.95 * lens_area);
}

#[test]
fn native_spline_corner_edits_reject_tangent_joins_and_scale_with_distant_planes() {
    let s = Session::new().unwrap();
    let p = |x, y| Vec3::new(x, y, 0.);
    let smooth = s
        .create_curve_wire(
            &[
                CurveSegment::Spline {
                    points: vec![p(0., 0.), p(10., 4.), p(20., 0.)],
                    start_tangent: None,
                    end_tangent: Some(Vec3::new(1., 0., 0.)),
                    periodic: false,
                },
                CurveSegment::Line {
                    start: p(20., 0.),
                    end: p(30., 0.),
                },
            ],
            false,
        )
        .unwrap();
    assert!(s.fillet_wire_corner(&smooth, p(20., 0.), 1.).is_err());
    assert!(s.chamfer_wire_corner(&smooth, p(20., 0.), 1., 1.).is_err());
    for scale in [0.01, 1., 1000.] {
        // Sketch plane XZ, far from the origin.
        let q = |x: f64, y: f64| Vec3::new(5000. + scale * x, -2000., 300. + scale * y);
        let wire = s
            .create_curve_wire(
                &[
                    CurveSegment::Line {
                        start: q(0., 15.),
                        end: q(0., 0.),
                    },
                    CurveSegment::Spline {
                        points: vec![q(0., 0.), q(10., 3.), q(20., -1.)],
                        start_tangent: None,
                        end_tangent: None,
                        periodic: false,
                    },
                ],
                false,
            )
            .unwrap();
        let rounded = s.fillet_wire_corner(&wire, q(0., 0.), 2. * scale).unwrap();
        assert!(s.is_valid(&rounded).unwrap(), "scale {scale}");
        assert_tangent_fillet(&s, &rounded, 2. * scale);
        for point in [q(0., 15.), q(20., -1.)] {
            assert!(
                has_vertex(&s, &rounded, point, 1e-9 * scale.max(1.)),
                "scale {scale}"
            );
        }
    }
}

#[test]
fn curves_trim_between_points_wrapping_closed_seams() {
    let s = Session::new().unwrap();
    let v = |x: f64, y: f64| Vec3::new(x, y, 0.0);
    let circle = s
        .create_circle_wire(v(0., 0.), Vec3::new(0., 0., 1.), 10.)
        .unwrap();
    let length = |w: &Shape<'_>| {
        s.subshapes(w, ShapeType::Edge)
            .unwrap()
            .iter()
            .map(|e| s.edge_length(e).unwrap())
            .sum::<f64>()
    };
    // Counterclockwise from 90° to 0° wraps through the seam: three quarters.
    let (top, right) = (v(0., 10.), v(10., 0.));
    for (from, to, turns) in [(top, right, 0.75), (right, top, 0.25)] {
        let arc = s.trim_curve_between(&circle, from, to).unwrap();
        assert!((length(&arc) - turns * 20. * std::f64::consts::PI).abs() < 1e-9);
        let (a, b, _, _) = s.curve_end_data(&arc).unwrap();
        assert!((a.x - from.x).hypot(a.y - from.y) < 1e-9 && (b.x - to.x).hypot(b.y - to.y) < 1e-9);
    }
    // Open curves keep their order; reversed points are rejected.
    let line = s
        .create_polyline_wire(&[v(0., 0.), v(10., 0.)], false)
        .unwrap();
    let piece = s.trim_curve_between(&line, v(2., 0.), v(7., 0.)).unwrap();
    assert!((length(&piece) - 5.).abs() < 1e-12);
    let count = s.shape_count().unwrap();
    for (from, to, message) in [
        (v(7., 0.), v(2., 0.), "out of order"),
        (v(2., 0.5), v(7., 0.), "not on the curve"),
        (v(f64::NAN, 0.), v(7., 0.), "finite"),
    ] {
        let error = s.trim_curve_between(&line, from, to).unwrap_err();
        assert!(error.message.contains(message), "{}", error.message);
    }
    assert!(
        s.trim_curve_between(&circle, top, top)
            .unwrap_err()
            .message
            .contains("coincide")
    );
    assert_eq!(s.shape_count().unwrap(), count);
}

#[test]
fn curvature_vectors_match_circles_lines_ellipses_and_bezier_ends() {
    let s = Session::new().unwrap();
    let v = |x: f64, y: f64| Vec3::new(x, y, 0.0);
    let close = |a: Vec3, x: f64, y: f64| (a.x - x).hypot(a.y - y) < 1e-9 && a.z.abs() < 1e-12;
    let circle = s
        .create_circle_wire(v(0., 0.), Vec3::new(0., 0., 1.), 10.)
        .unwrap();
    assert!(close(
        s.curve_curvature_at(&circle, v(10., 0.)).unwrap(),
        -0.1,
        0.
    ));
    assert!(close(
        s.curve_curvature_at(&circle, v(0., 12.)).unwrap(),
        0.,
        -0.1
    ));
    let line = s
        .create_polyline_wire(&[v(0., 0.), v(5., 5.)], false)
        .unwrap();
    assert!(close(
        s.curve_curvature_at(&line, v(1., 1.)).unwrap(),
        0.,
        0.
    ));
    // Ellipse 12 × 5: at the major vertex the radius of curvature is b²/a.
    let ellipse = s
        .create_ellipse_wire_axes(v(0., 0.), Vec3::new(0., 0., 1.), v(1., 0.), 12., 5.)
        .unwrap();
    assert!(close(
        s.curve_curvature_at(&ellipse, v(12., 0.)).unwrap(),
        -12. / 25.,
        0.
    ));
    // Cubic Bezier: κ(0) = (2/3) |Δ1 × Δ2| / |Δ1|³ toward the turn.
    let bezier = s
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: false,
                poles: vec![v(0., 0.), v(4., 0.), v(8., 3.), v(10., 8.)],
                degree: 3,
                knots: vec![0., 1.],
                multiplicities: vec![4, 4],
                weights: vec![],
            }],
            false,
        )
        .unwrap();
    let k = s.curve_curvature_at(&bezier, v(0., 0.)).unwrap();
    assert!(close(k, 0., 2. / 3. * (4. * 3.) / 64.), "{k:?}");
    let count = s.shape_count().unwrap();
    assert!(s.curve_curvature_at(&line, v(f64::NAN, 0.)).is_err());
    assert_eq!(s.shape_count().unwrap(), count);
}

#[test]
fn spline_loop_corner_fillets_keep_the_selected_tip_across_seams_and_reversal() {
    let s = Session::new().unwrap();
    {
        let p = |x, y| Vec3::new(x, y, 0.);
        let lens = s
            .create_curve_wire(
                &[
                    CurveSegment::Spline {
                        points: vec![p(0., 0.), p(10., 5.), p(20., 0.)],
                        start_tangent: None,
                        end_tangent: None,
                        periodic: false,
                    },
                    CurveSegment::Spline {
                        points: vec![p(20., 0.), p(10., -5.), p(0., 0.)],
                        start_tangent: None,
                        end_tangent: None,
                        periodic: false,
                    },
                ],
                true,
            )
            .unwrap();
        let source_bounds = s.exact_bounds(&lens).unwrap();
        let area = |wire: &Shape<'_>| {
            s.surface_area(&s.create_face_from_wire(wire).unwrap())
                .unwrap()
        };
        for (tip, remote) in [(p(0., 0.), p(20., 0.)), (p(20., 0.), p(0., 0.))] {
            let mut areas = Vec::new();
            for reverse in [false, true] {
                let input = s.align_profile_wire(&lens, Some(tip), reverse).unwrap();
                let rounded = s.fillet_wire_corner(&input, tip, 1.).unwrap();
                assert!(s.is_valid(&rounded).unwrap() && s.wire_is_closed(&rounded).unwrap());
                assert_eq!(s.subshape_count(&rounded, ShapeType::Edge).unwrap(), 3);
                assert!(has_vertex(&s, &rounded, remote, 1e-8));
                assert!(!has_vertex(&s, &rounded, tip, 1e-8));
                assert_tangent_fillet(&s, &rounded, 1.);
                areas.push(area(&rounded));
                let count = s.shape_count().unwrap();
                assert!(s.fillet_wire_corner(&input, tip, 50.).is_err());
                assert_eq!(s.shape_count().unwrap(), count);
                assert_eq!(s.subshape_count(&input, ShapeType::Edge).unwrap(), 2);
            }
            assert!((areas[0] - areas[1]).abs() < 1e-6);
            assert!(areas[0] < area(&lens));
        }
        assert_eq!(s.exact_bounds(&lens).unwrap(), source_bounds);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn spline_loop_chamfers_preserve_asymmetric_distances_when_reversed() {
    let s = Session::new().unwrap();
    {
        let p = |x, y| Vec3::new(x, y, 0.);
        let lens = s
            .create_curve_wire(
                &[
                    CurveSegment::Spline {
                        points: vec![p(0., 0.), p(10., 5.), p(20., 0.)],
                        start_tangent: None,
                        end_tangent: None,
                        periodic: false,
                    },
                    CurveSegment::Spline {
                        points: vec![p(20., 0.), p(10., -5.), p(0., 0.)],
                        start_tangent: None,
                        end_tangent: None,
                        periodic: false,
                    },
                ],
                true,
            )
            .unwrap();
        let tip = p(20., 0.);
        let forward = s.align_profile_wire(&lens, Some(tip), false).unwrap();
        let reverse = s.align_profile_wire(&lens, Some(tip), true).unwrap();
        let a = s.chamfer_wire_corner(&forward, tip, 1., 2.).unwrap();
        let b = s.chamfer_wire_corner(&reverse, tip, 2., 1.).unwrap();
        let area = |wire: &Shape<'_>| {
            s.surface_area(&s.create_face_from_wire(wire).unwrap())
                .unwrap()
        };
        assert!((area(&a) - area(&b)).abs() < 1e-5);
        for wire in [&a, &b] {
            assert!(s.is_valid(wire).unwrap() && s.wire_is_closed(wire).unwrap());
            assert_eq!(s.subshape_count(wire, ShapeType::Edge).unwrap(), 3);
            assert!(has_vertex(&s, wire, p(0., 0.), 1e-8));
        }
        assert_eq!(s.subshape_count(&lens, ShapeType::Edge).unwrap(), 2);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn circular_offsets_preserve_radius_and_orientation_across_traversals() {
    let s = Session::new().unwrap();
    let center = Vec3::new(12., -7., 3.);
    let normal = Vec3::new(0., 0., 1.);
    {
        let circle = s.create_circle_wire(center, normal, 5.).unwrap();
        let ellipse = s
            .create_ellipse_wire_axes(center, normal, Vec3::new(1., 0., 0.), 5., 5.)
            .unwrap();
        for source in [&circle, &ellipse] {
            for reverse in [false, true] {
                let input = s.align_profile_wire(source, None, reverse).unwrap();
                for normal in [normal, Vec3::new(0., 0., -1.)] {
                    for intersection in [false, true] {
                        for distance in [-2., 2.] {
                            let edited = s
                                .offset_wire(&input, normal, distance, intersection)
                                .unwrap();
                            let bounds = s.exact_bounds(&edited).unwrap();
                            let radius = 5. + distance;
                            assert!((bounds.min.x - (center.x - radius)).abs() < 1e-7);
                            assert!((bounds.max.y - (center.y + radius)).abs() < 1e-7);
                            assert!(s.wire_is_closed(&edited).unwrap());
                            let (_, _, input_tangent, _) = s.curve_end_data(&input).unwrap();
                            let (_, _, tangent, _) = s.curve_end_data(&edited).unwrap();
                            assert!(input_tangent.x * tangent.x + input_tangent.y * tangent.y > 0.);
                        }
                    }
                }
                let count = s.shape_count().unwrap();
                assert!(s.offset_wire(&input, normal, -5., false).is_err());
                assert_eq!(s.shape_count().unwrap(), count);
                assert!((s.exact_bounds(&input).unwrap().max.x - 17.).abs() < 1e-7);
            }
        }
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn open_offsets_follow_traversal_normal_and_join_style() {
    let s = Session::new().unwrap();
    let p = |x, y| Vec3::new(x, y, 0.);
    {
        for points in [
            vec![p(0., 0.), p(10., 0.)],
            vec![p(0., 0.), p(10., 0.), p(10., 10.)],
        ] {
            for reverse in [false, true] {
                let mut points = points.clone();
                if reverse {
                    points.reverse();
                }
                let input = s.create_polyline_wire(&points, false).unwrap();
                for nz in [-1., 1.] {
                    for distance in [-1., 1.] {
                        for intersection in [false, true] {
                            let edited = s
                                .offset_wire(&input, Vec3::new(0., 0., nz), distance, intersection)
                                .unwrap();
                            let nearest = s.curve_closest_point(&edited, p(5., 0.)).unwrap();
                            let expected_y = if reverse {
                                nz * distance
                            } else {
                                -nz * distance
                            };
                            assert!((nearest.y - expected_y).abs() < 1e-7);
                            assert!(!s.wire_is_closed(&edited).unwrap());
                            let expected_edges =
                                if points.len() == 3 && expected_y < 0. && !intersection {
                                    3
                                } else {
                                    points.len() - 1
                                };
                            assert_eq!(
                                s.subshapes(&edited, ShapeType::Edge).unwrap().len(),
                                expected_edges
                            );
                        }
                    }
                }
            }
        }
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn analytic_extension_keeps_arc_lengths_and_rejects_periodic_overlap() {
    let s = Session::new().unwrap();
    {
        for minor in [3., 5.] {
            let ellipse = s
                .create_ellipse_wire_axes(
                    Vec3::new(500., -200., 70.),
                    Vec3::new(0., -1., 0.),
                    Vec3::new(1., 0., 0.),
                    5.,
                    minor,
                )
                .unwrap();
            for reverse in [false, true] {
                let loop_wire = s.align_profile_wire(&ellipse, None, reverse).unwrap();
                let source = s.trim_curve(&loop_wire, 0.2, 0.45).unwrap();
                let before = s.curve_end_data(&source).unwrap();
                let edge = s.subshape(&source, ShapeType::Edge, 0).unwrap();
                let length = s.edge_length(&edge).unwrap();
                for (start, end) in [(0.5, 1.), (0., 1.), (0.5, 0.)] {
                    let extended = s.extend_curve(&source, start, end).unwrap();
                    let edge = s.subshape(&extended, ShapeType::Edge, 0).unwrap();
                    assert!((s.edge_length(&edge).unwrap() - length - start - end).abs() < 1e-6);
                    assert!(s.is_valid(&extended).unwrap());
                }
                let count = s.shape_count().unwrap();
                let error = s.extend_curve(&source, 100., 0.).unwrap_err();
                assert_eq!(error.message, "extension overlaps a full periodic curve");
                assert_eq!(s.shape_count().unwrap(), count);
                assert_eq!(s.curve_end_data(&source).unwrap(), before);
            }
        }
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn spline_extension_reaches_oriented_tangent_targets_without_changing_its_source() {
    let s = Session::new().unwrap();
    let p = |x, y| Vec3::new(x, y, 0.);
    let shifted = |point: Vec3, tangent: Vec3, distance: f64| {
        let norm = (tangent.x * tangent.x + tangent.y * tangent.y + tangent.z * tangent.z).sqrt();
        Vec3::new(
            point.x + distance * tangent.x / norm,
            point.y + distance * tangent.y / norm,
            point.z + distance * tangent.z / norm,
        )
    };
    {
        let lens = s
            .create_curve_wire(
                &[
                    CurveSegment::Spline {
                        points: vec![p(0., 0.), p(5., 2.), p(10., 0.)],
                        start_tangent: None,
                        end_tangent: None,
                        periodic: false,
                    },
                    CurveSegment::Line {
                        start: p(10., 0.),
                        end: p(0., 0.),
                    },
                ],
                true,
            )
            .unwrap();
        for reverse in [false, true] {
            let wire = s.align_profile_wire(&lens, None, reverse).unwrap();
            let edge = s
                .subshape(&wire, ShapeType::Edge, usize::from(reverse))
                .unwrap();
            let before = s.curve_end_data(&edge).unwrap();
            for (start, end) in [(0.5, 1.5), (0., 1.5), (0.5, 0.)] {
                let extended = s.extend_curve(&edge, start, end).unwrap();
                let (a, b, _, _) = s.curve_end_data(&extended).unwrap();
                for (actual, expected) in [
                    (a, shifted(before.0, before.2, -start)),
                    (b, shifted(before.1, before.3, end)),
                ] {
                    assert!((actual.x - expected.x).abs() < 1e-7);
                    assert!((actual.y - expected.y).abs() < 1e-7);
                    assert!((actual.z - expected.z).abs() < 1e-7);
                }
                assert!(s.is_valid(&extended).unwrap());
                assert_eq!(s.curve_end_data(&edge).unwrap(), before);
            }
        }
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}
