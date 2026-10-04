//! Booleans, fillets, offsets, hollowing, transforms, and operation history.

use super::*;

#[test]
fn rigid_moves_share_geometry_and_keep_history() {
    let session = Session::new().unwrap();
    let block = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let turned = session
        .rotate(
            &block,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            std::f64::consts::FRAC_PI_2,
        )
        .unwrap();
    let placed = session
        .translate(&turned, Vec3::new(100.0, 0.0, 0.0))
        .unwrap();
    let bounds = session.exact_bounds(&placed).unwrap();
    assert!((bounds.min.x - 80.0).abs() < 1e-9 && (bounds.max.x - 100.0).abs() < 1e-9);

    // Each source face maps to exactly one moved face of the result.
    for index in 0..6 {
        let source = session.subshape(&turned, ShapeType::Face, index).unwrap();
        assert_eq!(
            session
                .history_count(&placed, &source, HistoryRelation::Modified)
                .unwrap(),
            1
        );
        let moved = session
            .history(&placed, &source, HistoryRelation::Modified, 0)
            .unwrap();
        assert!(session.is_adjacent(&placed, &moved, &moved).is_ok());
        assert!(!session.is_same(&moved, &source).unwrap());
        let source_area = session.surface_area(&source).unwrap();
        assert!((session.surface_area(&moved).unwrap() - source_area).abs() < 1e-9);
        assert!(!session.history_is_deleted(&placed, &source).unwrap());
    }
    let stranger = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
        .unwrap();
    assert_eq!(
        session
            .history_count(&placed, &stranger, HistoryRelation::Modified)
            .unwrap_err()
            .status,
        1
    );

    // Copies sharing geometry stay independent: cutting one leaves the
    // original and the other copy unchanged.
    let tool = session
        .create_box(Vec3::new(85.0, 5.0, -1.0), Vec3::new(5.0, 5.0, 40.0))
        .unwrap();
    let cut = session.cut(&placed, &tool).unwrap();
    assert!(session.volume(&cut).unwrap() < 6000.0 - 1e-6);
    assert!((session.volume(&placed).unwrap() - 6000.0).abs() < 1e-6);
    assert!((session.volume(&block).unwrap() - 6000.0).abs() < 1e-6);

    // Scaling cannot be a location; it copies geometry and keeps
    // explicit history.
    let scaled = session
        .scale(&block, Vec3::new(0.0, 0.0, 0.0), 2.0)
        .unwrap();
    assert!((session.volume(&scaled).unwrap() - 48_000.0).abs() < 1e-6);
    let face = session.subshape(&block, ShapeType::Face, 0).unwrap();
    assert_eq!(
        session
            .history_count(&scaled, &face, HistoryRelation::Modified)
            .unwrap(),
        1
    );
}

#[test]
fn transforms_create_new_shapes_without_modifying_the_source() {
    let session = Session::new().unwrap();
    let source = session
        .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let translated = session
        .translate(&source, Vec3::new(100.0, -2.0, 7.0))
        .unwrap();
    let rotated = session
        .rotate(
            &source,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            std::f64::consts::FRAC_PI_2,
        )
        .unwrap();
    let scaled = session
        .scale(&source, Vec3::new(0.0, 0.0, 0.0), 2.0)
        .unwrap();

    let source_bounds = session.bounds(&source).unwrap();
    let translated_bounds = session.bounds(&translated).unwrap();
    let rotated_bounds = session.bounds(&rotated).unwrap();
    let scaled_bounds = session.bounds(&scaled).unwrap();
    assert!((source_bounds.min.x - 1.0).abs() < 1e-6);
    assert!((source_bounds.max.z - 33.0).abs() < 1e-6);
    assert!((translated_bounds.min.x - 101.0).abs() < 1e-6);
    assert!((translated_bounds.min.y - 0.0).abs() < 1e-6);
    assert!((translated_bounds.min.z - 10.0).abs() < 1e-6);
    assert!((rotated_bounds.min.x - -22.0).abs() < 1e-6);
    assert!((rotated_bounds.max.y - 11.0).abs() < 1e-6);
    assert!((scaled_bounds.min.x - 2.0).abs() < 1e-6);
    assert!((scaled_bounds.max.z - 66.0).abs() < 1e-6);
    assert_eq!(session.shape_count().unwrap(), 4);
}

#[test]
fn tracks_modified_subshapes_through_operation_history() {
    let session = Session::new().unwrap();
    let source = session
        .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let source_face = session.subshape(&source, ShapeType::Face, 0).unwrap();
    let offset = Vec3::new(100.0, -2.0, 7.0);
    let translated = session.translate(&source, offset).unwrap();

    assert_eq!(
        session
            .history_count(&translated, &source_face, HistoryRelation::Generated)
            .unwrap(),
        0
    );
    assert_eq!(
        session
            .history_count(&translated, &source_face, HistoryRelation::Modified)
            .unwrap(),
        1
    );
    assert!(
        !session
            .history_is_deleted(&translated, &source_face)
            .unwrap()
    );
    let translated_face = session
        .history(&translated, &source_face, HistoryRelation::Modified, 0)
        .unwrap();
    assert_eq!(
        session.shape_type(&translated_face).unwrap(),
        ShapeType::Face
    );
    let before = session.bounds(&source_face).unwrap();
    let after = session.bounds(&translated_face).unwrap();
    assert!((after.min.x - before.min.x - offset.x).abs() < 1e-6);
    assert!((after.min.y - before.min.y - offset.y).abs() < 1e-6);
    assert!((after.min.z - before.min.z - offset.z).abs() < 1e-6);
    assert_eq!(
        session
            .history(&translated, &source_face, HistoryRelation::Modified, 1,)
            .unwrap_err()
            .status,
        1
    );
    assert_eq!(
        session
            .history_count(&source, &source_face, HistoryRelation::Modified)
            .unwrap_err()
            .status,
        1
    );
    assert_eq!(session.shape_count().unwrap(), 4);
}

#[test]
fn applies_selected_edge_offset_hollow_and_common_operations() {
    let session = Session::new().unwrap();
    let source = session
        .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let edge = session.subshape(&source, ShapeType::Edge, 0).unwrap();
    let face = session.subshape(&source, ShapeType::Face, 0).unwrap();

    let filleted = session.fillet(&source, &[&edge], 1.0).unwrap();
    let chamfered = session.chamfer(&source, &[&edge], 1.0).unwrap();
    let offset = session.offset(&source, 1.0, 1e-6).unwrap();
    let hollow = session.hollow(&source, &[&face], -1.0, 1e-6).unwrap();
    let overlap = session
        .create_box(Vec3::new(6.0, 12.0, 18.0), Vec3::new(10.0, 10.0, 20.0))
        .unwrap();
    let common = session.common(&source, &overlap).unwrap();

    assert!(session.is_valid(&filleted).unwrap());
    assert!(session.is_valid(&chamfered).unwrap());
    assert!(session.is_valid(&offset).unwrap());
    assert!(session.is_valid(&hollow).unwrap());
    assert!(session.is_valid(&common).unwrap());
    let offset_bounds = session.bounds(&offset).unwrap();
    assert!(offset_bounds.min.x < 1.0);
    assert!(offset_bounds.max.z > 33.0);
    assert!((session.volume(&common).unwrap() - 750.0).abs() < 1e-6);
    assert!(session.is_valid(&source).unwrap());
    assert_eq!(session.shape_count().unwrap(), 9);
}

#[test]
fn operation_history_marks_fully_removed_topology_as_deleted() {
    let session = Session::new().unwrap();
    let object = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
        .unwrap();
    let removed_face = session.subshape(&object, ShapeType::Face, 0).unwrap();
    let removed_bounds = session.bounds(&removed_face).unwrap();
    assert!(removed_bounds.max.x < 1e-6);
    let tool = session
        .create_box(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(5.0, 12.0, 12.0))
        .unwrap();
    let result = session.cut(&object, &tool).unwrap();

    assert!(session.history_is_deleted(&result, &removed_face).unwrap());
    assert_eq!(
        session
            .history_count(&result, &removed_face, HistoryRelation::Modified)
            .unwrap(),
        0
    );
    assert_eq!(session.shape_count().unwrap(), 4);
}

#[test]
fn rejects_invalid_or_unrelated_operation_selections() {
    let session = Session::new().unwrap();
    let first = unit_box(&session, 0.0);
    let second = unit_box(&session, 5.0);
    let first_edge = session.subshape(&first, ShapeType::Edge, 0).unwrap();
    let first_face = session.subshape(&first, ShapeType::Face, 0).unwrap();

    assert_eq!(
        session
            .fillet(&second, &[&first_edge], 0.5)
            .unwrap_err()
            .status,
        4
    );
    assert_eq!(
        session
            .chamfer(&second, &[&first_edge], 0.5)
            .unwrap_err()
            .status,
        4
    );
    assert_eq!(
        session
            .hollow(&second, &[&first_face], -0.5, 1e-6)
            .unwrap_err()
            .status,
        4
    );
    assert_eq!(session.fillet(&first, &[], 0.5).unwrap_err().status, 1);
    assert_eq!(session.offset(&first, 0.0, 1e-6).unwrap_err().status, 1);
    assert_eq!(session.shape_count().unwrap(), 4);
}

#[test]
fn unifying_a_fused_stadium_lets_it_be_shelled() {
    let session = Session::new().unwrap();
    let up = Vec3::new(0.0, 0.0, 1.0);
    let round = session
        .create_cylinder(Vec3::new(0.0, 0.0, 0.0), up, 5.0, 5.0)
        .unwrap();
    let block = session
        .create_box(Vec3::new(0.0, -5.0, 0.0), Vec3::new(20.0, 10.0, 5.0))
        .unwrap();
    let stadium = session.fuse(&round, &block).unwrap();
    let top_of = |shape: &Shape<'_>| {
        (0..session.subshape_count(shape, ShapeType::Face).unwrap())
            .map(|index| session.subshape(shape, ShapeType::Face, index).unwrap())
            .filter(|face| {
                session.face_is_planar(face).unwrap()
                    && session.face_normal(face).unwrap().z > 0.999
            })
            .collect::<Vec<_>>()
    };
    // The fuse splits the top into the notched block face and two half-discs;
    // the offset cannot shell through them.
    let split_top = top_of(&stadium);
    assert_eq!(split_top.len(), 3);
    let refs = split_top.iter().collect::<Vec<_>>();
    assert!(session.hollow(&stadium, &refs, -1.0, 1e-4).is_err());

    let unified = session.unify_same_domain(&stadium, 1e-7, 1e-9).unwrap();
    assert_eq!(
        session.subshape_count(&unified, ShapeType::Face).unwrap(),
        6
    );
    let top = top_of(&unified);
    assert_eq!(top.len(), 1);
    // Each split piece's history leads to the merged top.
    for piece in &split_top {
        let merged = session
            .history(&unified, piece, HistoryRelation::Modified, 0)
            .unwrap();
        assert!(session.is_same(&merged, &top[0]).unwrap());
    }
    let tray = session.hollow(&unified, &[&top[0]], -1.0, 1e-4).unwrap();
    assert!(session.is_valid(&tray).unwrap());
    // The stadium less a 1 mm-inset stadium 4 mm deep.
    let pi = std::f64::consts::PI;
    let expected = 5.0 * (200.0 + pi * 12.5) - 4.0 * (152.0 + pi * 8.0);
    let volume = session.volume(&tray).unwrap();
    assert!((volume - expected).abs() < 1e-6 * expected, "{volume}");

    for (linear, angular) in [(0.0, 1e-9), (1e-7, 0.0), (f64::NAN, 1e-9), (1e-7, 2.0)] {
        assert!(
            session
                .unify_same_domain(&stadium, linear, angular)
                .is_err()
        );
    }
}
