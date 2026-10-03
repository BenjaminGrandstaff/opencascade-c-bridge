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
