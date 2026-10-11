use super::*;
fn line(session: &Session, a: Vec3, b: Vec3) -> Shape<'_> {
    session
        .create_curve_wire(&[CurveSegment::Line { start: a, end: b }], false)
        .unwrap()
}
fn parabola(session: &Session, weight: f64) -> Shape<'_> {
    session
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: false,
                poles: vec![
                    Vec3::new(0., 0., 0.),
                    Vec3::new(20., 20., 0.),
                    Vec3::new(40., 0., 0.),
                ],
                degree: 2,
                knots: vec![0., 1.],
                multiplicities: vec![3, 3],
                weights: vec![1., weight, 1.],
            }],
            false,
        )
        .unwrap()
}
fn verify(session: &Session, a: &Shape<'_>, b: &Shape<'_>, count: usize) -> Vec<Vec3> {
    let before = session.shape_count().unwrap();
    let result = session.planar_curve_intersections(a, b, 1000).unwrap();
    assert_eq!(result.len(), count);
    assert_eq!(session.shape_count().unwrap(), before);
    for p in &result {
        for curve in [a, b] {
            let q = session.curve_closest_point(curve, *p).unwrap();
            assert!(
                (q.x - p.x).hypot(q.y - p.y).hypot(q.z - p.z) < 1e-7,
                "{q:?}, {p:?}"
            );
        }
    }
    result
}
#[test]
fn native_rational_crossings_are_bounded_trimmed_and_source_preserving() {
    let s = Session::new().unwrap();
    {
        let guide = line(&s, Vec3::new(-10., 6., 0.), Vec3::new(50., 6., 0.));
        let curve = parabola(&s, 1.);
        let p = verify(&s, &curve, &guide, 2);
        assert!((p[0].x - (20. - 160f64.sqrt())).abs() < 1e-7);
        assert!((p[1].x - (20. + 160f64.sqrt())).abs() < 1e-7);
        assert_eq!(
            s.planar_curve_intersections(&guide, &curve, 1000).unwrap(),
            p
        );
        let trimmed = s.trim_curve(&curve, 0.5, 1.).unwrap();
        verify(&s, &trimmed, &guide, 1);
        let short = line(&s, Vec3::new(15., 6., 0.), Vec3::new(25., 6., 0.));
        verify(&s, &curve, &short, 0);
        let opposing = s
            .create_curve_wire(
                &[CurveSegment::BSpline {
                    non_clamped: false,
                    periodic: false,
                    degree: 2,
                    knots: vec![0., 1.],
                    multiplicities: vec![3, 3],
                    weights: vec![],
                    poles: vec![
                        Vec3::new(0., 12., 0.),
                        Vec3::new(20., -8., 0.),
                        Vec3::new(40., 12., 0.),
                    ],
                }],
                false,
            )
            .unwrap();
        verify(&s, &curve, &opposing, 2);
        let weighted = parabola(&s, 0.75);
        let q = verify(&s, &weighted, &guide, 2);
        assert!((q[0].x - p[0].x).abs() > 1.);
        assert!(s.planar_curve_intersections(&curve, &guide, 1).is_err());
        assert!(s.planar_curve_intersections(&curve, &guide, 0).is_err());
        assert!(s.planar_curve_intersections(&curve, &guide, 1001).is_err());
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}
#[test]
fn native_remaining_conic_pairs_and_located_curves_have_exact_contacts() {
    let s = Session::new().unwrap();
    let normal = Vec3::new(0., 0., 1.);
    {
        let ellipse = s
            .create_ellipse_wire_axes(
                Vec3::new(0., 0., 0.),
                normal,
                Vec3::new(1., 0., 0.),
                10.,
                6.,
            )
            .unwrap();
        let other = s
            .create_ellipse_wire_axes(
                Vec3::new(5., 0., 0.),
                normal,
                Vec3::new(1., 0., 0.),
                10.,
                6.,
            )
            .unwrap();
        let p = verify(&s, &ellipse, &other, 2);
        for p in &p {
            assert!((p.x - 2.5).abs() < 1e-7);
        }
        let circle = s
            .create_circle_wire(Vec3::new(4., 0., 0.), normal, 8.)
            .unwrap();
        verify(&s, &ellipse, &circle, 2);
        let arc = s.trim_curve(&circle, 0., 0.5).unwrap();
        verify(&s, &ellipse, &arc, 1);
        let moved = s.translate(&ellipse, Vec3::new(5., 0., 0.)).unwrap();
        assert_eq!(
            s.planar_curve_intersections(&ellipse, &moved, 1000)
                .unwrap(),
            p
        );
        let rotated = s
            .create_ellipse_wire_axes(
                Vec3::new(0., 0., 0.),
                normal,
                Vec3::new(1., 1., 0.),
                10.,
                6.,
            )
            .unwrap();
        verify(&s, &ellipse, &rotated, 4);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}
#[test]
fn native_intersection_errors_do_not_allocate_or_accept_nonplanar_or_wrong_session_shapes() {
    let s = Session::new().unwrap();
    let other = Session::new().unwrap();
    {
        let a = line(&s, Vec3::new(0., 0., 0.), Vec3::new(10., 0., 0.));
        let overlap = line(&s, Vec3::new(2., 0., 0.), Vec3::new(12., 0., 0.));
        let outside = s.translate(&a, Vec3::new(0., 0., 1.)).unwrap();
        let curve = parabola(&s, 1.);
        let box_shape = s
            .create_box(Vec3::new(0., 0., 0.), Vec3::new(1., 1., 1.))
            .unwrap();
        let multi = s
            .create_polyline_wire(
                &[
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 1., 0.),
                ],
                false,
            )
            .unwrap();
        let before = s.shape_count().unwrap();
        assert!(
            s.planar_curve_intersections(&a, &overlap, 1000)
                .unwrap_err()
                .message
                .contains("overlap")
        );
        assert!(s.planar_curve_intersections(&curve, &curve, 1000).is_err());
        assert!(
            s.planar_curve_intersections(&a, &outside, 1000)
                .unwrap_err()
                .message
                .contains("XY plane")
        );
        assert!(s.planar_curve_intersections(&box_shape, &a, 1000).is_err());
        assert!(s.planar_curve_intersections(&multi, &a, 1000).is_err());
        assert_wrong_session(other.planar_curve_intersections(&a, &a, 1000).unwrap_err());
        let foreign = line(&other, Vec3::new(0., 0., 0.), Vec3::new(1., 0., 0.));
        assert_wrong_session(
            s.planar_curve_intersections(&a, &foreign, 1000)
                .unwrap_err(),
        );
        assert_eq!(s.shape_count().unwrap(), before);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn native_intersection_span_product_budget_is_checked_before_kernel_work() {
    let s = Session::new().unwrap();
    let make = |y| {
        s.create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: false,
                degree: 1,
                poles: (0..102).map(|i| Vec3::new(i as f64, y, 0.)).collect(),
                knots: (0..102).map(|i| i as f64).collect(),
                multiplicities: (0..102)
                    .map(|i| if i == 0 || i == 101 { 2 } else { 1 })
                    .collect(),
                weights: vec![],
            }],
            false,
        )
        .unwrap()
    };
    {
        let a = make(0.);
        let b = make(1.);
        let before = s.shape_count().unwrap();
        assert!(
            s.planar_curve_intersections(&a, &b, 1000)
                .unwrap_err()
                .message
                .contains("10000 spline span pairs")
        );
        assert_eq!(s.shape_count().unwrap(), before);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn native_line_spline_touches_find_repeated_roots_and_preserve_nearby_crossings() {
    let s = Session::new().unwrap();
    {
        let curve = parabola(&s, 1.);
        let tangent = line(&s, Vec3::new(-10., 10., 0.), Vec3::new(50., 10., 0.));
        let p = verify(&s, &curve, &tangent, 1);
        assert!((p[0].x - 20.).abs() < 1e-9 && (p[0].y - 10.).abs() < 1e-9);
        assert_eq!(
            s.planar_curve_intersections(&tangent, &curve, 1000)
                .unwrap(),
            p
        );
        let miss = line(
            &s,
            Vec3::new(-10., 10. + 1e-6, 0.),
            Vec3::new(50., 10. + 1e-6, 0.),
        );
        verify(&s, &curve, &miss, 0);
        let crossing = line(
            &s,
            Vec3::new(-10., 10. - 1e-10, 0.),
            Vec3::new(50., 10. - 1e-10, 0.),
        );
        let two = verify(&s, &curve, &crossing, 2);
        assert!(two[1].x - two[0].x > 1e-4);
        assert!(s.planar_curve_intersections(&curve, &crossing, 1).is_err());
        let trimmed = s.trim_curve(&curve, 0.6, 1.).unwrap();
        verify(&s, &trimmed, &tangent, 0);
        let short = line(&s, Vec3::new(-10., 10., 0.), Vec3::new(19., 10., 0.));
        verify(&s, &curve, &short, 0);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}
#[test]
fn rational_high_order_and_endpoint_line_spline_touches_are_isolated() {
    let s = Session::new().unwrap();
    {
        let rational = s
            .create_curve_wire(
                &[CurveSegment::BSpline {
                    non_clamped: false,
                    periodic: false,
                    degree: 2,
                    knots: vec![0., 1.],
                    multiplicities: vec![3, 3],
                    weights: vec![1., 2., 1.],
                    poles: vec![
                        Vec3::new(0., 0., 0.),
                        Vec3::new(1., 3., 0.),
                        Vec3::new(2., 0., 0.),
                    ],
                }],
                false,
            )
            .unwrap();
        let upper = line(&s, Vec3::new(-1., 2., 0.), Vec3::new(3., 2., 0.));
        let p = verify(&s, &rational, &upper, 1);
        assert!((p[0].x - 1.).abs() < 1e-9);
        let quartic = s
            .create_curve_wire(
                &[CurveSegment::BSpline {
                    non_clamped: false,
                    periodic: false,
                    degree: 4,
                    knots: vec![0., 1.],
                    multiplicities: vec![5, 5],
                    weights: vec![],
                    poles: [1., -1., 1., -1., 1.]
                        .iter()
                        .enumerate()
                        .map(|(i, y)| Vec3::new(i as f64 / 4., *y, 0.))
                        .collect(),
                }],
                false,
            )
            .unwrap();
        let zero = line(&s, Vec3::new(-1., 0., 0.), Vec3::new(3., 0., 0.));
        let p = verify(&s, &quartic, &zero, 1);
        assert!((p[0].x - 0.5).abs() < 1e-9);
        let endpoint = s
            .create_curve_wire(
                &[CurveSegment::BSpline {
                    non_clamped: false,
                    periodic: false,
                    degree: 2,
                    knots: vec![0., 1.],
                    multiplicities: vec![3, 3],
                    weights: vec![],
                    poles: vec![
                        Vec3::new(0., 0., 0.),
                        Vec3::new(1., 0., 0.),
                        Vec3::new(2., 1., 0.),
                    ],
                }],
                false,
            )
            .unwrap();
        let p = verify(&s, &endpoint, &zero, 1);
        assert!(p[0].x.abs() < 1e-9);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}
#[test]
fn collinear_line_spline_spans_reject_overlap_but_ignore_disjoint_finite_ranges() {
    let s = Session::new().unwrap();
    {
        let curve = s
            .create_curve_wire(
                &[CurveSegment::BSpline {
                    non_clamped: false,
                    periodic: false,
                    degree: 2,
                    knots: vec![0., 1.],
                    multiplicities: vec![3, 3],
                    weights: vec![],
                    poles: vec![
                        Vec3::new(0., 0., 0.),
                        Vec3::new(1., 0., 0.),
                        Vec3::new(2., 0., 0.),
                    ],
                }],
                false,
            )
            .unwrap();
        let guide = line(&s, Vec3::new(-1., 0., 0.), Vec3::new(3., 0., 0.));
        let before = s.shape_count().unwrap();
        assert!(
            s.planar_curve_intersections(&curve, &guide, 1000)
                .unwrap_err()
                .message
                .contains("overlap")
        );
        assert_eq!(s.shape_count().unwrap(), before);
        let disjoint = line(&s, Vec3::new(-3., 0., 0.), Vec3::new(-1., 0., 0.));
        verify(&s, &curve, &disjoint, 0);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn line_spline_root_work_budget_rejects_dense_high_degree_inputs_without_handles() {
    let s = Session::new().unwrap();
    let choose = |n: usize, k: usize| (0..k).fold(1., |v, i| v * (n - i) as f64 / (i + 1) as f64);
    let coefficients = (0..=10)
        .map(|k| (if k % 2 == 0 { 1. } else { -1. }) * choose(20, 2 * k) / choose(10, k))
        .collect::<Vec<_>>();
    {
        let curve = s
            .create_curve_wire(
                &[CurveSegment::BSpline {
                    non_clamped: false,
                    periodic: false,
                    degree: 10,
                    weights: vec![],
                    poles: (0..=600)
                        .map(|i| Vec3::new(i as f64 / 10., coefficients[i % 10], 0.))
                        .collect(),
                    knots: (0..=60).map(|i| i as f64).collect(),
                    multiplicities: (0..=60)
                        .map(|i| if i == 0 || i == 60 { 11 } else { 10 })
                        .collect(),
                }],
                false,
            )
            .unwrap();
        let guide = line(&s, Vec3::new(-1., 0., 0.), Vec3::new(61., 0., 0.));
        let before = s.shape_count().unwrap();
        let error = s
            .planar_curve_intersections(&curve, &guide, 1000)
            .unwrap_err();
        assert!(error.message.contains("root-work"), "{}", error.message);
        assert_eq!(s.shape_count().unwrap(), before);
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}
