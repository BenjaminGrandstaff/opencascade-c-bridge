//! Session ownership: foreign and cleared handles, and removal.

use super::*;

fn assert_cleared(error: BridgeError) {
    assert_eq!(error.status, 3);
    assert_eq!(error.category, "shape not found");
    assert_eq!(
        error.message,
        "shape was invalidated by clearing its session"
    );
}

#[test]
fn rejects_shapes_from_another_session_for_every_shape_operation() {
    let first = Session::new().unwrap();
    let second = Session::new().unwrap();
    let first_shape = unit_box(&first, 0.0);
    let first_shape_to_remove = unit_box(&first, 2.0);
    let second_shape = unit_box(&second, 10.0);

    assert_wrong_session(second.create_compound(&[&first_shape]).unwrap_err());
    assert_wrong_session(second.sew(&[&first_shape], 1e-6).unwrap_err());
    assert_wrong_session(second.exact_bounds(&first_shape).unwrap_err());
    assert_wrong_session(second.make_solid(&first_shape).unwrap_err());
    assert_wrong_session(second.make_solid_from_shells(&[&first_shape]).unwrap_err());
    assert_wrong_session(second.fuse(&first_shape, &second_shape).unwrap_err());
    assert_wrong_session(second.cut(&second_shape, &first_shape).unwrap_err());
    assert_wrong_session(second.common(&second_shape, &first_shape).unwrap_err());
    assert_wrong_session(second.fillet(&first_shape, &[], 1.0).unwrap_err());
    assert_wrong_session(second.chamfer(&first_shape, &[], 1.0).unwrap_err());
    assert_wrong_session(second.offset(&first_shape, 1.0, 1e-6).unwrap_err());
    assert_wrong_session(second.hollow(&first_shape, &[], -1.0, 1e-6).unwrap_err());
    assert_wrong_session(
        second
            .fillet(&second_shape, &[&first_shape], 1.0)
            .unwrap_err(),
    );
    assert_wrong_session(second.shape_type(&first_shape).unwrap_err());
    assert_wrong_session(second.duplicate(&first_shape).unwrap_err());
    assert_wrong_session(
        second
            .subshape_count(&first_shape, ShapeType::Face)
            .unwrap_err(),
    );
    assert_wrong_session(
        second
            .subshape(&first_shape, ShapeType::Face, 0)
            .unwrap_err(),
    );
    assert_wrong_session(second.surface_area(&first_shape).unwrap_err());
    assert_wrong_session(second.volume(&first_shape).unwrap_err());
    assert_wrong_session(second.center_of_mass(&first_shape).unwrap_err());
    assert_wrong_session(second.face_normal(&first_shape).unwrap_err());
    assert_wrong_session(second.face_is_planar(&first_shape).unwrap_err());
    assert_wrong_session(second.edge_length(&first_shape).unwrap_err());
    assert_wrong_session(second.edge_circle_radius(&first_shape).unwrap_err());
    assert_wrong_session(second.edge_curvature(&first_shape).unwrap_err());
    assert_wrong_session(second.edge_curvature_range(&first_shape, 5).unwrap_err());
    assert_wrong_session(
        second
            .faces_are_tangent(&second_shape, &first_shape, &second_shape)
            .unwrap_err(),
    );
    assert_wrong_session(second.is_same(&first_shape, &second_shape).unwrap_err());
    assert_wrong_session(
        second
            .is_adjacent(&second_shape, &first_shape, &second_shape)
            .unwrap_err(),
    );
    assert_wrong_session(
        second
            .history_count(&second_shape, &first_shape, HistoryRelation::Modified)
            .unwrap_err(),
    );
    assert_wrong_session(
        second
            .history(&second_shape, &first_shape, HistoryRelation::Modified, 0)
            .unwrap_err(),
    );
    assert_wrong_session(
        second
            .history_is_deleted(&second_shape, &first_shape)
            .unwrap_err(),
    );
    assert_wrong_session(second.create_face_from_wire(&first_shape).unwrap_err());
    assert_wrong_session(
        second
            .create_prism_from_face(&first_shape, Vec3::new(0.0, 0.0, 1.0))
            .unwrap_err(),
    );
    assert_wrong_session(
        second
            .translate(&first_shape, Vec3::new(1.0, 0.0, 0.0))
            .unwrap_err(),
    );
    assert_wrong_session(
        second
            .rotate(
                &first_shape,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                1.0,
            )
            .unwrap_err(),
    );
    assert_wrong_session(
        second
            .scale(&first_shape, Vec3::new(0.0, 0.0, 0.0), 2.0)
            .unwrap_err(),
    );
    assert_wrong_session(second.bounds(&first_shape).unwrap_err());
    assert_wrong_session(second.is_valid(&first_shape).unwrap_err());
    assert_wrong_session(
        second
            .save_brep(&first_shape, "cross-session-must-not-exist.brep")
            .unwrap_err(),
    );
    assert_wrong_session(
        second
            .save_step(&first_shape, "cross-session-must-not-exist.step")
            .unwrap_err(),
    );
    assert_wrong_session(
        second
            .save_stl(
                &first_shape,
                "cross-session-must-not-exist.stl",
                StlOptions::default(),
            )
            .unwrap_err(),
    );
    assert_wrong_session(second.remove(first_shape_to_remove).unwrap_err());

    // The rejected handle was moved into `remove`; dropping it releases
    // the shape in its own session rather than leaking it.
    assert_eq!(first.shape_count().unwrap(), 1);
    assert_eq!(second.shape_count().unwrap(), 1);
    assert!(first.is_valid(&first_shape).unwrap());
    assert!(second.is_valid(&second_shape).unwrap());
}

#[test]
fn clear_invalidates_shapes_for_every_shape_operation() {
    let session = Session::new().unwrap();
    let old_shape = unit_box(&session, 0.0);
    let old_shape_to_remove = unit_box(&session, 2.0);

    session.clear().unwrap();

    let new_shape = unit_box(&session, 10.0);
    assert_cleared(session.create_compound(&[&old_shape]).unwrap_err());
    assert_cleared(session.fuse(&old_shape, &new_shape).unwrap_err());
    assert_cleared(session.cut(&new_shape, &old_shape).unwrap_err());
    assert_cleared(session.common(&new_shape, &old_shape).unwrap_err());
    assert_cleared(session.fillet(&old_shape, &[], 1.0).unwrap_err());
    assert_cleared(session.chamfer(&old_shape, &[], 1.0).unwrap_err());
    assert_cleared(session.offset(&old_shape, 1.0, 1e-6).unwrap_err());
    assert_cleared(session.hollow(&old_shape, &[], -1.0, 1e-6).unwrap_err());
    assert_cleared(session.shape_type(&old_shape).unwrap_err());
    assert_cleared(
        session
            .subshape_count(&old_shape, ShapeType::Face)
            .unwrap_err(),
    );
    assert_cleared(
        session
            .subshape(&old_shape, ShapeType::Face, 0)
            .unwrap_err(),
    );
    assert_cleared(session.surface_area(&old_shape).unwrap_err());
    assert_cleared(session.volume(&old_shape).unwrap_err());
    assert_cleared(session.center_of_mass(&old_shape).unwrap_err());
    assert_cleared(
        session
            .history_count(&new_shape, &old_shape, HistoryRelation::Modified)
            .unwrap_err(),
    );
    assert_cleared(
        session
            .history(&new_shape, &old_shape, HistoryRelation::Modified, 0)
            .unwrap_err(),
    );
    assert_cleared(
        session
            .history_is_deleted(&new_shape, &old_shape)
            .unwrap_err(),
    );
    assert_cleared(session.create_face_from_wire(&old_shape).unwrap_err());
    assert_cleared(
        session
            .create_prism_from_face(&old_shape, Vec3::new(0.0, 0.0, 1.0))
            .unwrap_err(),
    );
    assert_cleared(
        session
            .translate(&old_shape, Vec3::new(1.0, 0.0, 0.0))
            .unwrap_err(),
    );
    assert_cleared(
        session
            .rotate(
                &old_shape,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                1.0,
            )
            .unwrap_err(),
    );
    assert_cleared(
        session
            .scale(&old_shape, Vec3::new(0.0, 0.0, 0.0), 2.0)
            .unwrap_err(),
    );
    assert_cleared(session.bounds(&old_shape).unwrap_err());
    assert_cleared(session.is_valid(&old_shape).unwrap_err());
    assert_cleared(
        session
            .save_brep(&old_shape, "cleared-shape-must-not-exist.brep")
            .unwrap_err(),
    );
    assert_cleared(
        session
            .save_step(&old_shape, "cleared-shape-must-not-exist.step")
            .unwrap_err(),
    );
    assert_cleared(
        session
            .save_stl(
                &old_shape,
                "cleared-shape-must-not-exist.stl",
                StlOptions::default(),
            )
            .unwrap_err(),
    );
    assert_cleared(session.remove(old_shape_to_remove).unwrap_err());

    assert_eq!(session.shape_count().unwrap(), 1);
    assert!(session.is_valid(&new_shape).unwrap());
}

#[test]
fn non_consuming_operations_leave_inputs_usable() {
    let session = Session::new().unwrap();
    let left = unit_box(&session, 0.0);
    let right = unit_box(&session, 0.5);

    let fused = session.fuse(&left, &right).unwrap();
    let compound = session.create_compound(&[&left, &right, &fused]).unwrap();

    assert!(session.is_valid(&left).unwrap());
    assert!(session.is_valid(&right).unwrap());
    assert!(session.is_valid(&fused).unwrap());
    assert!(session.is_valid(&compound).unwrap());
}

#[test]
fn remove_deletes_exactly_one_shape() {
    let session = Session::new().unwrap();
    let removed = unit_box(&session, 0.0);
    let retained = unit_box(&session, 2.0);

    session.remove(removed).unwrap();

    assert_eq!(session.shape_count().unwrap(), 1);
    assert!(session.is_valid(&retained).unwrap());
}
