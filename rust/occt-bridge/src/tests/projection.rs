use super::*;

fn top() -> ProjectionFrame {
    ProjectionFrame {
        origin: Vec3::new(0.0, 0.0, 0.0),
        direction: Vec3::new(0.0, 0.0, 1.0),
        x_axis: Vec3::new(1.0, 0.0, 0.0),
    }
}

#[test]
fn orthographic_projection_has_visible_hidden_xy_geometry_without_input_changes() {
    let session = Session::new().unwrap();
    let body = session
        .create_box(Vec3::new(2.0, 3.0, 4.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let volume = session.volume(&body).unwrap();
    let projection = session.orthographic_projection(&body, top()).unwrap();
    let bounds = session.exact_bounds(&projection.visible).unwrap();
    assert!((bounds.min.x - 2.0).abs() < 1e-7, "{bounds:?}");
    assert!((bounds.max.x - 12.0).abs() < 1e-7);
    assert!((bounds.min.y - 3.0).abs() < 1e-7);
    assert!((bounds.max.y - 23.0).abs() < 1e-7);
    assert!(bounds.min.z.abs() < 1e-7 && bounds.max.z.abs() < 1e-7);
    assert!(
        session
            .subshape_count(&projection.visible, ShapeType::Edge)
            .unwrap()
            >= 4
    );
    assert!(
        session
            .subshape_count(&projection.hidden, ShapeType::Edge)
            .unwrap()
            >= 4
    );
    let edge = session
        .subshape(&projection.visible, ShapeType::Edge, 0)
        .unwrap();
    let points = session.edge_sample_points(&edge, 3).unwrap();
    assert_eq!(points.len(), 3);
    assert!((points[1].x - (points[0].x + points[2].x) * 0.5).abs() < 1e-7);
    assert!((points[1].y - (points[0].y + points[2].y) * 0.5).abs() < 1e-7);
    assert_eq!(session.volume(&body).unwrap(), volume);
    assert!(session.is_valid(&body).unwrap());
    drop(edge);
    drop(projection);
    assert_eq!(session.shape_count().unwrap(), 1);
    let side = session
        .orthographic_projection(
            &body,
            ProjectionFrame {
                origin: Vec3::new(2.0, 3.0, 4.0),
                direction: Vec3::new(0.0, -1.0, 0.0),
                x_axis: Vec3::new(1.0, 0.0, 0.0),
            },
        )
        .unwrap();
    let bounds = session.exact_bounds(&side.visible).unwrap();
    assert!(bounds.min.x.abs() < 1e-7 && bounds.min.y.abs() < 1e-7);
    assert!((bounds.max.x - 10.0).abs() < 1e-7 && (bounds.max.y - 30.0).abs() < 1e-7);
}

#[test]
fn projection_validates_axes_sessions_and_bounded_sampling() {
    let session = Session::new().unwrap();
    let body = unit_box(&session, 0.0);
    for frame in [
        ProjectionFrame {
            direction: Vec3::new(0.0, 0.0, 0.0),
            ..top()
        },
        ProjectionFrame {
            x_axis: Vec3::new(0.0, 0.0, 1.0),
            ..top()
        },
        ProjectionFrame {
            origin: Vec3::new(f64::NAN, 0.0, 0.0),
            ..top()
        },
    ] {
        assert!(session.orthographic_projection(&body, frame).is_err());
        assert_eq!(session.shape_count().unwrap(), 1);
    }
    let edge = session.subshape(&body, ShapeType::Edge, 0).unwrap();
    for count in [0, 1, 100001, usize::MAX] {
        assert!(session.edge_sample_points(&edge, count).is_err());
    }
    assert!(session.edge_sample_points(&body, 2).is_err());
    let other = Session::new().unwrap();
    assert_wrong_session(other.orthographic_projection(&body, top()).unwrap_err());
    assert_wrong_session(other.edge_sample_points(&edge, 2).unwrap_err());
    let huge_axes = ProjectionFrame {
        direction: Vec3::new(0.0, 0.0, 1e300),
        x_axis: Vec3::new(1e-300, 0.0, 0.0),
        ..top()
    };
    let result = session.orthographic_projection(&body, huge_axes).unwrap();
    drop(result);
    assert_eq!(session.shape_count().unwrap(), 2);
}

#[test]
fn curved_projection_samples_and_parallel_calls_are_stable() {
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                let session = Session::new().unwrap();
                let cylinder = session
                    .create_cylinder(
                        Vec3::new(0.0, 0.0, 0.0),
                        Vec3::new(0.0, 0.0, 1.0),
                        5.0,
                        10.0,
                    )
                    .unwrap();
                for _ in 0..8 {
                    let projected = session.orthographic_projection(&cylinder, top()).unwrap();
                    let edge = session
                        .subshape(&projected.visible, ShapeType::Edge, 0)
                        .unwrap();
                    let samples = session.edge_sample_points(&edge, 17).unwrap();
                    assert!(
                        samples
                            .iter()
                            .all(|point| (point.x.hypot(point.y) - 5.0).abs() < 1e-7
                                && point.z.abs() < 1e-7)
                    );
                    assert!(
                        (samples[0].x - samples[16].x).abs() < 1e-7
                            && (samples[0].y - samples[16].y).abs() < 1e-7
                    );
                    drop(edge);
                    drop(projected);
                    assert_eq!(session.shape_count().unwrap(), 1);
                }
                assert!(
                    (session.volume(&cylinder).unwrap() - 250.0 * std::f64::consts::PI).abs()
                        < 1e-7
                );
            });
        }
    });
}

#[test]
fn plane_clipping_retains_closed_solids_and_input_ancestry() {
    let session = Session::new().unwrap();
    let body = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    for positive in [true, false] {
        let clipped = session
            .clip_by_plane(
                &body,
                Vec3::new(5.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                positive,
            )
            .unwrap();
        assert!(session.is_valid(&clipped).unwrap());
        assert!((session.volume(&clipped).unwrap() - 3000.0).abs() < 1e-7);
        let bounds = session.exact_bounds(&clipped).unwrap();
        let expected = if positive { (5.0, 10.0) } else { (0.0, 5.0) };
        assert!(
            (bounds.min.x - expected.0).abs() < 1e-7 && (bounds.max.x - expected.1).abs() < 1e-7
        );
        assert!(
            session
                .history_count(&clipped, &body, HistoryRelation::Modified)
                .unwrap()
                > 0
        );
    }
    let diagonal = session
        .clip_by_plane(
            &body,
            Vec3::new(5.0, 10.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            true,
        )
        .unwrap();
    assert!((session.volume(&diagonal).unwrap() - 3000.0).abs() < 1e-7);
    drop(diagonal);
    let empty = session
        .clip_by_plane(
            &body,
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            true,
        )
        .unwrap();
    assert_eq!(session.subshape_count(&empty, ShapeType::Solid).unwrap(), 0);
    assert!(session.orthographic_projection(&empty, top()).is_err());
    drop(empty);
    assert!((session.volume(&body).unwrap() - 6000.0).abs() < 1e-7);
    assert_eq!(session.shape_count().unwrap(), 1);
}

#[test]
fn plane_clipping_rejects_invalid_units_axes_and_sessions_without_handles() {
    let session = Session::new().unwrap();
    let body = unit_box(&session, 0.0);
    for (origin, normal) in [
        (Vec3::new(f64::NAN, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0)),
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(f64::INFINITY, 0.0, 0.0)),
    ] {
        assert!(session.clip_by_plane(&body, origin, normal, true).is_err());
        assert_eq!(session.shape_count().unwrap(), 1);
    }
    let edge = session.subshape(&body, ShapeType::Edge, 0).unwrap();
    assert!(
        session
            .clip_by_plane(
                &edge,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                true
            )
            .is_err()
    );
    let other = Session::new().unwrap();
    assert_wrong_session(
        other
            .clip_by_plane(
                &body,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                true,
            )
            .unwrap_err(),
    );
    assert_eq!(session.shape_count().unwrap(), 2);
}

#[test]
fn analytic_edges_match_samples_for_located_lines_circles_ellipses_and_arcs() {
    use crate::AnalyticCurve;
    let session = Session::new().unwrap();
    let cylinder = session
        .create_cylinder(
            Vec3::new(1e6, -1e6, 4.0),
            Vec3::new(0.0, 0.0, 1.0),
            5.0,
            10.0,
        )
        .unwrap();
    let arc = session
        .create_segment_wire(
            &[WireSegment::Arc {
                start: Vec3::new(5.0, 0.0, 0.0),
                middle: Vec3::new(0.0, -5.0, 0.0),
                end: Vec3::new(-5.0, 0.0, 0.0),
            }],
            false,
        )
        .unwrap();
    for shape in [&arc, &cylinder] {
        for edge in session.subshapes(shape, ShapeType::Edge).unwrap() {
            check_analytic_samples(&session, &edge);
        }
    }
    let projected = session
        .orthographic_projection(
            &cylinder,
            ProjectionFrame {
                origin: Vec3::new(1e6, -1e6, 4.0),
                direction: Vec3::new(0.0, 1.0, 1.0),
                x_axis: Vec3::new(1.0, 0.0, 0.0),
            },
        )
        .unwrap();
    let mut ellipse = false;
    for edge in session
        .subshapes(&projected.visible, ShapeType::Edge)
        .unwrap()
    {
        if let Some(AnalyticCurve::Conic { major, minor, .. }) =
            session.edge_analytic_curve(&edge).unwrap()
        {
            ellipse |= (major.x.hypot(major.y) - minor.x.hypot(minor.y)).abs() > 1.0;
        }
        check_analytic_samples(&session, &edge);
    }
    assert!(ellipse);
    assert!(session.edge_analytic_curve(&cylinder).is_err());
    let edge = session.subshape(&arc, ShapeType::Edge, 0).unwrap();
    assert_wrong_session(
        Session::new()
            .unwrap()
            .edge_analytic_curve(&edge)
            .unwrap_err(),
    );
    drop(edge);
    drop(projected);
    drop(arc);
    drop(cylinder);
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn check_analytic_samples(session: &Session, edge: &Shape<'_>) {
    use crate::AnalyticCurve;
    let curve = session.edge_analytic_curve(edge).unwrap().unwrap();
    let points = session.edge_sample_points(edge, 9).unwrap();
    for (i, point) in points.iter().enumerate() {
        let fraction = i as f64 / 8.0;
        let expected = match curve {
            AnalyticCurve::Line { start, end } => Vec3::new(
                start.x * (1.0 - fraction) + end.x * fraction,
                start.y * (1.0 - fraction) + end.y * fraction,
                start.z * (1.0 - fraction) + end.z * fraction,
            ),
            AnalyticCurve::Conic {
                center,
                major,
                minor,
                first,
                last,
            } => {
                let (sin, cos) = (first * (1.0 - fraction) + last * fraction).sin_cos();
                Vec3::new(
                    center.x + major.x * cos + minor.x * sin,
                    center.y + major.y * cos + minor.y * sin,
                    center.z + major.z * cos + minor.z * sin,
                )
            }
        };
        assert!(
            (expected.x - point.x)
                .abs()
                .max((expected.y - point.y).abs())
                .max((expected.z - point.z).abs())
                < 1e-7
        );
    }
}

#[test]
fn bezier_export_preserves_standard_curves_spline_spans_and_periodic_edges() {
    let session = Session::new().unwrap();
    let fixture = session
        .load_brep(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/curvature_edges.brep"
        ))
        .unwrap();
    for index in 0..7 {
        let edge = session.subshape(&fixture, ShapeType::Edge, index).unwrap();
        let spans = session.edge_bezier_spans(&edge, 1000).unwrap();
        assert!(!spans.is_empty());
        let samples = session.edge_sample_points(&edge, 97).unwrap();
        let evaluate = |span: &BezierSpan, t: f64| {
            let mut values: Vec<_> = span
                .poles
                .iter()
                .zip(&span.weights)
                .map(|(p, w)| [p.x * w, p.y * w, p.z * w, *w])
                .collect();
            for n in (1..values.len()).rev() {
                for i in 0..n {
                    let next = values[i + 1];
                    for (value, next) in values[i].iter_mut().zip(next) {
                        *value = (1.0 - t) * *value + t * next;
                    }
                }
            }
            Vec3::new(
                values[0][0] / values[0][3],
                values[0][1] / values[0][3],
                values[0][2] / values[0][3],
            )
        };
        let close = |a: Vec3, b: Vec3| {
            assert!(
                (a.x - b.x).hypot((a.y - b.y).hypot(a.z - b.z)) < 1e-8,
                "{index}: {a:?} {b:?}"
            )
        };
        close(evaluate(&spans[0], 0.0), samples[0]);
        close(evaluate(spans.last().unwrap(), 1.0), samples[96]);
        for pair in spans.windows(2) {
            close(evaluate(&pair[0], 1.0), evaluate(&pair[1], 0.0));
        }
        if index == 2 {
            assert_eq!(spans.len(), 3);
            for (span, piece) in spans.iter().enumerate() {
                for i in 0..=32 {
                    close(evaluate(piece, i as f64 / 32.0), samples[span * 32 + i]);
                }
            }
        }
        for span in &spans {
            for i in 0..=10 {
                let p = evaluate(span, i as f64 / 10.0);
                match index {
                    0 => assert!((p.y - p.x * p.x).abs() < 1e-9),
                    1 => assert!((p.x.hypot(p.y) - 2.0).abs() < 1e-9),
                    5 => assert!((p.x - 0.5 * p.y * p.y).abs() < 1e-9),
                    6 => assert!((p.x * p.x / 9.0 - p.y * p.y / 4.0 - 1.0).abs() < 1e-9),
                    _ => (),
                }
            }
        }
        assert!(session.edge_bezier_spans(&edge, 1).is_err());
        assert_wrong_session(
            Session::new()
                .unwrap()
                .edge_bezier_spans(&edge, 100)
                .unwrap_err(),
        );
    }
    let periodic = session
        .create_curve_wire(
            &[CurveSegment::Spline {
                points: vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(5.0, 0.0, 0.0),
                    Vec3::new(6.0, 4.0, 0.0),
                    Vec3::new(0.0, 5.0, 0.0),
                ],
                start_tangent: None,
                end_tangent: None,
                periodic: true,
            }],
            true,
        )
        .unwrap();
    let edge = session.subshape(&periodic, ShapeType::Edge, 0).unwrap();
    let spans = session.edge_bezier_spans(&edge, 100).unwrap();
    assert!((spans[0].poles[0].x - spans.last().unwrap().poles.last().unwrap().x).abs() < 1e-9);
    assert!(session.edge_bezier_spans(&edge, 2).is_err());
    drop(edge);
    drop(periodic);
    drop(fixture);
    assert_eq!(session.shape_count().unwrap(), 0);
}
