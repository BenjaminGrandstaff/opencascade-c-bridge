//! Composition follows generated/modified ancestry without mutating inputs.
use super::*;

fn assert_member(session: &Session, result: &Shape<'_>, target: &Shape<'_>) {
    let kind = session.shape_type(target).unwrap();
    assert!(
        (0..session.subshape_count(result, kind).unwrap()).any(|index| {
            let member = session.subshape(result, kind, index).unwrap();
            session.is_same(target, &member).unwrap()
        })
    );
}

#[test]
fn composed_rigid_history_shares_geometry_and_outlives_intermediates() {
    let session = Session::new().unwrap();
    let source = unit_box(&session, 0.0);
    let edge = session.subshape(&source, ShapeType::Edge, 0).unwrap();
    let first = session
        .translate(&source, Vec3::new(10.0, 0.0, 0.0))
        .unwrap();
    let second = session
        .translate(&first, Vec3::new(0.0, 20.0, 0.0))
        .unwrap();
    let composed = session.compose_history(&second, &first).unwrap();
    let repeated = session.compose_history(&composed, &first).unwrap();
    assert!(session.is_same(&repeated, &composed).unwrap());
    assert_eq!(
        session
            .history_count(&repeated, &edge, HistoryRelation::Modified)
            .unwrap(),
        1
    );
    assert!(session.is_same(&composed, &second).unwrap());
    assert!(
        session
            .history_count(&second, &edge, HistoryRelation::Modified)
            .is_err()
    );
    assert_eq!(
        session
            .history_count(&composed, &edge, HistoryRelation::Modified)
            .unwrap(),
        1
    );
    assert_eq!(
        session
            .history_count(&composed, &edge, HistoryRelation::Generated)
            .unwrap(),
        0
    );
    assert!(!session.history_is_deleted(&composed, &edge).unwrap());
    let target = session
        .history(&composed, &edge, HistoryRelation::Modified, 0)
        .unwrap();
    assert_member(&session, &composed, &target);
    let before = session.exact_bounds(&edge).unwrap();
    let after = session.exact_bounds(&target).unwrap();
    assert!((after.min.x - before.min.x - 10.0).abs() < 1e-9);
    assert!((after.min.y - before.min.y - 20.0).abs() < 1e-9);
    drop((first, second));
    assert_eq!(
        session
            .history_count(&composed, &edge, HistoryRelation::Modified)
            .unwrap(),
        1
    );
    drop((source, edge, composed, repeated, target));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn composed_extrusion_tracks_generated_faces_after_modification_and_deletion() {
    let session = Session::new().unwrap();
    let wire = session
        .create_polyline_wire(
            &[
                Vec3::new(2.0, 4.0, 1.0),
                Vec3::new(8.0, 4.0, 1.0),
                Vec3::new(2.0, 4.0, 7.0),
            ],
            true,
        )
        .unwrap();
    let profile = session.create_face_from_wire(&wire).unwrap();
    let prism = session
        .create_prism_from_face(&profile, Vec3::new(0.0, 2.0, 0.0))
        .unwrap();
    let placed = session
        .translate(&prism, Vec3::new(0.0, -1.0, 0.0))
        .unwrap();
    let wall = session.compose_history(&placed, &prism).unwrap();
    let body = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 1.0))
        .unwrap();
    let fused = session.fuse(&body, &wall).unwrap();
    let result = session.compose_history(&fused, &wall).unwrap();
    assert!((session.volume(&result).unwrap() - 136.0).abs() < 1e-8);
    for index in 0..3 {
        let edge = session.subshape(&profile, ShapeType::Edge, index).unwrap();
        let expected = expected_generated_targets(&session, &prism, &placed, &fused, &edge);
        let count = session
            .history_count(&result, &edge, HistoryRelation::Generated)
            .unwrap();
        assert_eq!(count, expected.len());
        for index in 0..count {
            let actual = session
                .history(&result, &edge, HistoryRelation::Generated, index)
                .unwrap();
            assert_member(&session, &result, &actual);
            assert!(
                expected
                    .iter()
                    .any(|target| session.is_same(target, &actual).unwrap())
            );
        }
    }
    drop((wire, profile, prism, placed, wall, body, fused, result));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn composition_maps_modified_edges_into_generated_faces() {
    let session = Session::new().unwrap();
    let wire = session
        .create_polyline_wire(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
                Vec3::new(0.0, 3.0, 0.0),
            ],
            true,
        )
        .unwrap();
    let face = session.create_face_from_wire(&wire).unwrap();
    let moved = session.translate(&face, Vec3::new(0.0, 0.0, 10.0)).unwrap();
    let prism = session
        .create_prism_from_face(&moved, Vec3::new(0.0, 0.0, 2.0))
        .unwrap();
    let result = session.compose_history(&prism, &moved).unwrap();
    for index in 0..3 {
        let source = session.subshape(&face, ShapeType::Edge, index).unwrap();
        assert_eq!(
            session
                .history_count(&result, &source, HistoryRelation::Generated)
                .unwrap(),
            1
        );
        let generated = session
            .history(&result, &source, HistoryRelation::Generated, 0)
            .unwrap();
        assert_eq!(session.shape_type(&generated).unwrap(), ShapeType::Face);
        assert_member(&session, &result, &generated);
    }
    drop((wire, face, moved, prism, result));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn composition_rejects_missing_history_unrelated_and_foreign_handles() {
    let session = Session::new().unwrap();
    let other = Session::new().unwrap();
    let source = unit_box(&session, 0.0);
    let first = session
        .translate(&source, Vec3::new(2.0, 0.0, 0.0))
        .unwrap();
    let second = session.translate(&first, Vec3::new(2.0, 0.0, 0.0)).unwrap();
    let unrelated = session
        .translate(&source, Vec3::new(20.0, 0.0, 0.0))
        .unwrap();
    let foreign = unit_box(&other, 0.0);
    for (result, intermediate) in [
        (&second, &source),
        (&source, &first),
        (&second, &unrelated),
        (&second, &second),
    ] {
        let before = session.shape_count().unwrap();
        assert_eq!(
            session
                .compose_history(result, intermediate)
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(session.shape_count().unwrap(), before);
    }
    assert_wrong_session(session.compose_history(&foreign, &first).unwrap_err());
    assert_wrong_session(session.compose_history(&second, &foreign).unwrap_err());
    let stale = session.duplicate(&first).unwrap();
    session.clear().unwrap();
    assert_eq!(
        session.compose_history(&second, &stale).unwrap_err().status,
        3
    );
}

fn expected_generated_targets<'a>(
    session: &'a Session,
    prism: &Shape<'_>,
    placed: &Shape<'_>,
    fused: &Shape<'_>,
    edge: &Shape<'_>,
) -> Vec<Shape<'a>> {
    let mut expected = Vec::new();
    for index in 0..session
        .history_count(prism, edge, HistoryRelation::Generated)
        .unwrap()
    {
        let generated = session
            .history(prism, edge, HistoryRelation::Generated, index)
            .unwrap();
        let moved = session
            .history(placed, &generated, HistoryRelation::Modified, 0)
            .unwrap();
        for relation in [HistoryRelation::Generated, HistoryRelation::Modified] {
            for index in 0..session.history_count(fused, &moved, relation).unwrap() {
                expected.push(session.history(fused, &moved, relation, index).unwrap());
            }
        }
        if !session.history_is_deleted(fused, &moved).unwrap()
            && session
                .history_count(fused, &moved, HistoryRelation::Modified)
                .unwrap()
                == 0
        {
            expected.push(moved);
        }
    }
    expected
}

#[test]
fn subset_extraction_filters_ancestry_to_the_selected_solid_and_outlives_parents() {
    let session = Session::new().unwrap();
    let a = session
        .create_circle_wire(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 1.0)
        .unwrap();
    let b = session
        .create_circle_wire(Vec3::new(5.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 1.0)
        .unwrap();
    let edge_a = session.subshape(&a, ShapeType::Edge, 0).unwrap();
    let edge_b = session.subshape(&b, ShapeType::Edge, 0).unwrap();
    let face_a = session.create_face_from_wire(&a).unwrap();
    let face_b = session.create_face_from_wire(&b).unwrap();
    let prism_a = session
        .create_prism_from_face(&face_a, Vec3::new(0.0, 0.0, 2.0))
        .unwrap();
    let prism_b = session
        .create_prism_from_face(&face_b, Vec3::new(0.0, 0.0, 2.0))
        .unwrap();
    let fused = session.fuse(&prism_a, &prism_b).unwrap();
    let first = session.compose_history(&fused, &prism_a).unwrap();
    let parent = session.compose_history(&first, &prism_b).unwrap();
    assert_eq!(
        session.subshape_count(&parent, ShapeType::Solid).unwrap(),
        2
    );
    let mut extracted = Vec::new();
    for i in 0..2 {
        let selected = session
            .subshape_with_history(&parent, ShapeType::Solid, i)
            .unwrap();
        let original = session.subshape(&parent, ShapeType::Solid, i).unwrap();
        assert!(session.is_same(&selected, &original).unwrap());
        let (own, other) = if session.exact_bounds(&selected).unwrap().min.x < 2.0 {
            (&edge_a, &edge_b)
        } else {
            (&edge_b, &edge_a)
        };
        let count = session
            .history_count(&selected, own, HistoryRelation::Generated)
            .unwrap();
        assert!(count > 0);
        assert_eq!(
            session
                .history_count(&selected, other, HistoryRelation::Generated)
                .unwrap(),
            0
        );
        assert!(session.history_is_deleted(&selected, other).unwrap());
        for n in 0..count {
            let face = session
                .history(&selected, own, HistoryRelation::Generated, n)
                .unwrap();
            assert_member(&session, &selected, &face);
        }
        extracted.push(selected);
    }
    drop((a, b, face_a, face_b, prism_a, prism_b, fused, first, parent));
    assert!(extracted.iter().all(|s| session.is_valid(s).unwrap()));
    assert!(
        session
            .history_count(&extracted[0], &edge_a, HistoryRelation::Generated)
            .unwrap()
            + session
                .history_count(&extracted[0], &edge_b, HistoryRelation::Generated)
                .unwrap()
            > 0
    );
    drop((extracted, edge_a, edge_b));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn subset_extraction_expands_located_history_and_checks_ownership_and_index() {
    let session = Session::new().unwrap();
    let source = unit_box(&session, 0.0);
    let original = session.subshape(&source, ShapeType::Face, 0).unwrap();
    let placed = session
        .translate(&source, Vec3::new(10.0, 0.0, 0.0))
        .unwrap();
    let face = session
        .subshape_with_history(&placed, ShapeType::Face, 0)
        .unwrap();
    assert_eq!(
        session
            .history_count(&face, &original, HistoryRelation::Modified)
            .unwrap(),
        1
    );
    let mapped = session
        .history(&face, &original, HistoryRelation::Modified, 0)
        .unwrap();
    assert!(session.is_same(&face, &mapped).unwrap());
    let plain = session
        .subshape_with_history(&source, ShapeType::Face, 0)
        .unwrap();
    assert!(session.is_same(&plain, &original).unwrap());
    assert_eq!(
        session
            .subshape_with_history(&source, ShapeType::Face, 99)
            .unwrap_err()
            .status,
        1
    );
    let foreign_session = Session::new().unwrap();
    let foreign = unit_box(&foreign_session, 0.0);
    assert_eq!(
        session
            .subshape_with_history(&foreign, ShapeType::Face, 0)
            .unwrap_err()
            .status,
        1
    );
    drop((source, original, placed, face, mapped, plain));
    assert_eq!(session.shape_count().unwrap(), 0);
}
