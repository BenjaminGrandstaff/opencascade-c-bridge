use super::*;

#[test]
fn translated_open_profiles_preserve_exact_area_and_edge_ancestry() {
    let session = Session::new().unwrap();
    let source = session
        .create_polyline_wire(&[Vec3::new(2.0, 4.0, 1.0), Vec3::new(8.0, 4.0, 1.0)], false)
        .unwrap();
    let face = session
        .create_open_profile_face(&source, Vec3::new(0.0, 0.0, 6.0))
        .unwrap();
    assert!((session.surface_area(&face).unwrap() - 36.0).abs() < 1e-8);
    let edge = session.subshape(&source, ShapeType::Edge, 0).unwrap();
    assert_eq!(
        session
            .history_count(&face, &edge, HistoryRelation::Generated)
            .unwrap(),
        1
    );
    let shifted = session
        .history(&face, &edge, HistoryRelation::Generated, 0)
        .unwrap();
    assert!((session.center_of_mass(&shifted).unwrap().z - 7.0).abs() < 1e-8);
    let prism = session
        .create_prism_from_face(&face, Vec3::new(0.0, 2.0, 0.0))
        .unwrap();
    let wall = session.compose_history(&prism, &face).unwrap();
    let body = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 1.0))
        .unwrap();
    let fused = session.fuse(&body, &wall).unwrap();
    let result = session.compose_history(&fused, &wall).unwrap();
    assert!((session.volume(&result).unwrap() - 172.0).abs() < 1e-8);
    assert_eq!(
        session
            .history_count(&result, &edge, HistoryRelation::Generated)
            .unwrap(),
        1
    );
    let top = session
        .history(&result, &edge, HistoryRelation::Generated, 0)
        .unwrap();
    assert_eq!(session.shape_type(&top).unwrap(), ShapeType::Face);
    assert!((session.surface_area(&top).unwrap() - 12.0).abs() < 1e-8);
    drop((
        source, face, edge, shifted, prism, wall, body, fused, result, top,
    ));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn translated_closure_supports_multi_segment_and_exact_arc_profiles() {
    let session = Session::new().unwrap();
    let zigzag = session
        .create_polyline_wire(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(2.0, 1.0, 0.0),
                Vec3::new(4.0, 0.0, 0.0),
            ],
            false,
        )
        .unwrap();
    let face = session
        .create_open_profile_face(&zigzag, Vec3::new(0.0, 3.0, 0.0))
        .unwrap();
    assert!((session.surface_area(&face).unwrap() - 12.0).abs() < 1e-8);
    let arc = session
        .create_segment_wire(
            &[WireSegment::Arc {
                start: Vec3::new(-2.0, 0.0, 0.0),
                middle: Vec3::new(0.0, -2.0, 0.0),
                end: Vec3::new(2.0, 0.0, 0.0),
            }],
            false,
        )
        .unwrap();
    let curved = session
        .create_open_profile_face(&arc, Vec3::new(0.0, 5.0, 0.0))
        .unwrap();
    assert!((session.surface_area(&curved).unwrap() - 20.0).abs() < 1e-8);
    assert!(session.is_valid(&curved).unwrap());
    drop((zigzag, face, arc, curved));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn translated_closure_rejects_invalid_offsets_crossings_and_foreign_handles() {
    let session = Session::new().unwrap();
    let wire = session
        .create_polyline_wire(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(2.0, 2.0, 0.0),
                Vec3::new(4.0, 0.0, 0.0),
            ],
            false,
        )
        .unwrap();
    let handles = session.shape_count().unwrap();
    for offset in [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(f64::NAN, 1.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ] {
        assert!(session.create_open_profile_face(&wire, offset).is_err());
        assert_eq!(session.shape_count().unwrap(), handles);
    }
    let closed = session
        .create_circle_wire(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 1.0)
        .unwrap();
    assert!(
        session
            .create_open_profile_face(&closed, Vec3::new(0.0, 2.0, 0.0))
            .is_err()
    );
    let other = Session::new().unwrap();
    assert_wrong_session(
        other
            .create_open_profile_face(&wire, Vec3::new(0.0, 3.0, 0.0))
            .unwrap_err(),
    );
    session.clear().unwrap();
    assert_eq!(
        session
            .create_open_profile_face(&wire, Vec3::new(0.0, 3.0, 0.0))
            .unwrap_err()
            .status,
        3
    );
}

#[test]
fn first_contact_closure_has_exact_area_history_and_bounded_reach() {
    let session = Session::new().unwrap();
    let body = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 1.0))
        .unwrap();
    let wire = session
        .create_polyline_wire(
            &[
                Vec3::new(2.0, 4.0, 7.0),
                Vec3::new(5.0, 4.0, 7.0),
                Vec3::new(8.0, 4.0, 7.0),
            ],
            false,
        )
        .unwrap();
    // The exact reach boundary is accepted; direction magnitude is irrelevant.
    let face = session
        .create_open_profile_face_to_next(&wire, &body, Vec3::new(0.0, 0.0, -1e-12), 6.0)
        .unwrap();
    assert!((session.surface_area(&face).unwrap() - 36.0).abs() < 1e-8);
    assert!((session.center_of_mass(&face).unwrap().z - 4.0).abs() < 1e-8);
    assert!(session.is_valid(&face).unwrap());
    for index in 0..2 {
        let edge = session.subshape(&wire, ShapeType::Edge, index).unwrap();
        let shifted = session
            .history(&face, &edge, HistoryRelation::Generated, 0)
            .unwrap();
        assert!((session.center_of_mass(&shifted).unwrap().z - 1.0).abs() < 1e-8);
    }
    assert!((session.center_of_mass(&wire).unwrap().z - 7.0).abs() < 1e-8);
    assert!((session.volume(&body).unwrap() - 100.0).abs() < 1e-8);
    let handles = session.shape_count().unwrap();
    for (direction, reach) in [
        (Vec3::new(0.0, 0.0, -1.0), 5.9),
        (Vec3::new(0.0, 0.0, 1.0), 10.0),
        (Vec3::new(0.0, 0.0, 0.0), 10.0),
        (Vec3::new(f64::NAN, 0.0, -1.0), 10.0),
        (Vec3::new(0.0, 0.0, -1.0), 0.0),
        (Vec3::new(0.0, 0.0, -1.0), -1.0),
        (Vec3::new(0.0, 0.0, -1.0), f64::INFINITY),
        (Vec3::new(0.0, 0.0, -1.0), f64::NAN),
    ] {
        assert!(
            session
                .create_open_profile_face_to_next(&wire, &body, direction, reach)
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), handles);
    }
    let foreign = Session::new().unwrap();
    let other = foreign
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
        .unwrap();
    assert!(
        session
            .create_open_profile_face_to_next(&wire, &other, Vec3::new(0.0, 0.0, -1.0), 10.0)
            .is_err()
    );
    assert!(
        session
            .create_open_profile_face_to_next(&other, &body, Vec3::new(0.0, 0.0, -1.0), 10.0)
            .is_err()
    );
    drop((body, wire, face));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn first_contact_closure_rejects_partial_obstacles_and_nonuniform_profiles() {
    let session = Session::new().unwrap();
    let plate = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 1.0))
        .unwrap();
    let step = session
        .create_box(Vec3::new(0.0, 0.0, 1.0), Vec3::new(3.0, 10.0, 3.0))
        .unwrap();
    let stepped = session.fuse(&plate, &step).unwrap();
    let wire = session
        .create_polyline_wire(&[Vec3::new(2.0, 4.0, 7.0), Vec3::new(8.0, 4.0, 7.0)], false)
        .unwrap();
    let error = session
        .create_open_profile_face_to_next(&wire, &stepped, Vec3::new(0.0, 0.0, -1.0), 10.0)
        .unwrap_err();
    assert!(
        error.message.contains("entire translated profile"),
        "{error:?}"
    );
    let lid = session
        .create_box(Vec3::new(0.0, 0.0, 4.0), Vec3::new(10.0, 10.0, 1.0))
        .unwrap();
    let enclosed = session.fuse(&stepped, &lid).unwrap();
    let first = session
        .create_open_profile_face_to_next(&wire, &enclosed, Vec3::new(0.0, 0.0, -1.0), 10.0)
        .unwrap();
    // Stops on the nearer lid, not the plate behind the first support.
    assert!((session.surface_area(&first).unwrap() - 12.0).abs() < 1e-8);
    let handles = session.shape_count().unwrap();
    for points in [
        [Vec3::new(2.0, 4.0, 1.0), Vec3::new(8.0, 4.0, 1.0)],
        [Vec3::new(2.0, 4.0, 0.5), Vec3::new(8.0, 4.0, 0.5)],
        [Vec3::new(2.0, 4.0, 7.0), Vec3::new(8.0, 4.0, 8.0)],
        [Vec3::new(12.0, 4.0, 7.0), Vec3::new(18.0, 4.0, 7.0)],
    ] {
        let invalid = session.create_polyline_wire(&points, false).unwrap();
        assert!(
            session
                .create_open_profile_face_to_next(&invalid, &plate, Vec3::new(0.0, 0.0, -1.0), 10.0)
                .is_err()
        );
        drop(invalid);
        assert_eq!(session.shape_count().unwrap(), handles);
    }
    let circle = session
        .create_circle_wire(Vec3::new(5.0, 4.0, 7.0), Vec3::new(0.0, 1.0, 0.0), 2.0)
        .unwrap();
    assert!(
        session
            .create_open_profile_face_to_next(&circle, &plate, Vec3::new(0.0, 0.0, -1.0), 10.0)
            .is_err()
    );
    assert!(
        session
            .create_open_profile_face_to_next(&plate, &plate, Vec3::new(0.0, 0.0, -1.0), 10.0)
            .is_err()
    );
    assert!(
        session
            .create_open_profile_face_to_next(&wire, &wire, Vec3::new(0.0, 0.0, -1.0), 10.0)
            .is_err()
    );
    drop((plate, step, stepped, wire, lid, enclosed, first, circle));
    assert_eq!(session.shape_count().unwrap(), 0);
}
