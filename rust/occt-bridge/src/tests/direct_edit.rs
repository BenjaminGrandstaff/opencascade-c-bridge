//! Direct face edits (ABI 86): removal by defeaturing and per-face offsets,
//! checked against closed-form volumes.

use super::*;
use std::f64::consts::PI;

/// A 40 × 30 × 20 block with a Ø6 through hole at (10, 15).
fn drilled(session: &Session) -> Shape<'_> {
    let block = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 30.0, 20.0))
        .unwrap();
    let rod = session
        .create_cylinder(
            Vec3::new(10.0, 15.0, -1.0),
            Vec3::new(0.0, 0.0, 1.0),
            3.0,
            22.0,
        )
        .unwrap();
    session.cut(&block, &rod).unwrap()
}

fn face_where<'a>(
    session: &'a Session,
    shape: &Shape<'_>,
    keep: impl Fn(&Session, &Shape<'_>) -> bool,
) -> Vec<Shape<'a>> {
    session
        .subshapes(shape, ShapeType::Face)
        .unwrap()
        .into_iter()
        .filter(|face| keep(session, face))
        .collect()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} vs {expected}");
}

#[test]
fn removed_holes_and_fillets_heal_to_the_original_block() {
    let session = Session::new().unwrap();
    let body = drilled(&session);
    let wall = face_where(&session, &body, |s, f| !s.face_is_planar(f).unwrap());
    assert_eq!(wall.len(), 1);
    let healed = session.remove_faces(&body, &[&wall[0]]).unwrap();
    close(session.volume(&healed).unwrap(), 24_000.0);
    assert_eq!(session.subshape_count(&healed, ShapeType::Face).unwrap(), 6);
    assert!(session.is_valid(&healed).unwrap());
    assert!(session.history_is_deleted(&healed, &wall[0]).unwrap());
    // The input is untouched.
    close(session.volume(&body).unwrap(), 24_000.0 - PI * 9.0 * 20.0);

    // Round the two vertical edges at x = 40, then remove the rounds.
    let edges: Vec<_> = session
        .subshapes(&body, ShapeType::Edge)
        .unwrap()
        .into_iter()
        .filter(|edge| {
            let b = session.exact_bounds(edge).unwrap();
            b.min.x > 39.99 && b.max.z - b.min.z > 19.0
        })
        .collect();
    assert_eq!(edges.len(), 2);
    let refs: Vec<_> = edges.iter().collect();
    let rounded = session.fillet(&body, &refs, 4.0).unwrap();
    let rounds = face_where(&session, &rounded, |s, f| {
        !s.face_is_planar(f).unwrap() && s.exact_bounds(f).unwrap().min.x > 30.0
    });
    assert_eq!(rounds.len(), 2);
    let refs: Vec<_> = rounds.iter().collect();
    let sharp = session.remove_faces(&rounded, &refs).unwrap();
    close(session.volume(&sharp).unwrap(), 24_000.0 - PI * 9.0 * 20.0);
    drop((wall, edges, rounds, healed, rounded, sharp, body));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn offset_faces_push_pull_and_resize_holes_exactly() {
    let session = Session::new().unwrap();
    let body = drilled(&session);
    let hole = PI * 9.0 * 20.0;
    let top = face_where(&session, &body, |s, f| {
        s.face_is_planar(f).unwrap() && s.exact_bounds(f).unwrap().min.z > 19.99
    });
    let end = face_where(&session, &body, |s, f| {
        s.face_is_planar(f).unwrap() && s.exact_bounds(f).unwrap().min.x > 39.99
    });
    let wall = face_where(&session, &body, |s, f| !s.face_is_planar(f).unwrap());
    // Pull the top up 5 and push the end in 3 together: the hole lengthens.
    let edited = session
        .offset_faces(&body, &[(&top[0], 5.0), (&end[0], -3.0)], 1e-6)
        .unwrap();
    close(
        session.volume(&edited).unwrap(),
        37.0 * 30.0 * 25.0 - PI * 9.0 * 25.0,
    );
    assert!(session.is_valid(&edited).unwrap());
    // Every face keeps its identity as a modification.
    for face in session.subshapes(&body, ShapeType::Face).unwrap() {
        assert_eq!(
            session
                .history_count(&edited, &face, HistoryRelation::Modified)
                .unwrap(),
            1
        );
        assert_eq!(
            session
                .history_count(&edited, &face, HistoryRelation::Generated)
                .unwrap(),
            0
        );
    }
    // The hole's outward normal points at its axis: -1 widens it to Ø8.
    let widened = session
        .offset_faces(&body, &[(&wall[0], -1.0)], 1e-6)
        .unwrap();
    close(
        session.volume(&widened).unwrap(),
        24_000.0 - PI * 16.0 * 20.0,
    );
    close(session.volume(&body).unwrap(), 24_000.0 - hole);
    drop((top, end, wall, edited, widened, body));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn offset_faces_extend_drafted_neighbours_along_their_draft() {
    let session = Session::new().unwrap();
    let block = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 30.0, 20.0))
        .unwrap();
    let sides = face_where(&session, &block, |s, f| {
        let b = s.exact_bounds(f).unwrap();
        b.max.z - b.min.z > 19.0
    });
    let refs: Vec<_> = sides.iter().collect();
    let angle = 0.1_f64;
    let drafted = session
        .draft(
            &block,
            &refs,
            DraftOptions {
                neutral_origin: Vec3::new(0.0, 0.0, 0.0),
                neutral_normal: Vec3::new(0.0, 0.0, 1.0),
                pull_direction: Vec3::new(0.0, 0.0, 1.0),
                angle_radians: angle,
            },
        )
        .unwrap();
    // Each side leans in by z tan(angle): the section at z is
    // (40 - 2zt)(30 - 2zt).
    let t = angle.tan();
    let volume = |h: f64| 1200.0 * h - 70.0 * t * h * h + 4.0 * t * t * h * h * h / 3.0;
    close(session.volume(&drafted).unwrap(), volume(20.0));
    let top = face_where(&session, &drafted, |s, f| {
        s.exact_bounds(f).unwrap().min.z > 19.99
    });
    let taller = session
        .offset_faces(&drafted, &[(&top[0], 5.0)], 1e-6)
        .unwrap();
    close(session.volume(&taller).unwrap(), volume(25.0));
    assert_eq!(session.subshape_count(&taller, ShapeType::Face).unwrap(), 6);
}

#[test]
fn bad_selections_and_unhealable_removals_are_refused() {
    let session = Session::new().unwrap();
    let body = drilled(&session);
    let other = drilled(&session);
    let faces = session.subshapes(&body, ShapeType::Face).unwrap();
    let foreign = session.subshape(&other, ShapeType::Face, 0).unwrap();
    let count = session.shape_count().unwrap();
    assert!(session.remove_faces(&body, &[]).is_err());
    assert!(session.remove_faces(&body, &[&foreign]).is_err());
    assert!(
        session
            .offset_faces(&body, &[(&foreign, 1.0)], 1e-6)
            .is_err()
    );
    assert!(
        session
            .offset_faces(&body, &[(&faces[0], 0.0)], 1e-6)
            .is_err()
    );
    assert!(
        session
            .offset_faces(&body, &[(&faces[0], f64::NAN)], 1e-6)
            .is_err()
    );
    assert!(
        session
            .offset_faces(&body, &[(&faces[0], 1.0)], 0.0)
            .is_err()
    );
    let twice = session
        .offset_faces(&body, &[(&faces[0], 1.0), (&faces[0], 2.0)], 1e-6)
        .unwrap_err();
    assert!(twice.message.contains("twice"), "{}", twice.message);
    let sheet = session.subshape(&body, ShapeType::Face, 0).unwrap();
    assert!(session.remove_faces(&sheet, &[&sheet]).is_err());
    // Removing every face but one leaves nothing to heal from.
    let refs: Vec<_> = faces.iter().skip(1).collect();
    let error = session.remove_faces(&body, &refs).unwrap_err();
    assert!(error.message.contains("on selection"), "{}", error.message);
    assert!(
        error
            .diagnostics
            .iter()
            .all(|d| d.kind == DiagnosticKind::Defeaturing),
        "{:?}",
        error.diagnostics
    );
    assert_eq!(session.shape_count().unwrap(), count + 1);
}
