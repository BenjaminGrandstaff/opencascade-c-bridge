use super::*;

fn options(angle: f64) -> DraftOptions {
    DraftOptions {
        neutral_origin: Vec3::new(0.0, 0.0, 0.0),
        neutral_normal: Vec3::new(0.0, 0.0, 2.0),
        pull_direction: Vec3::new(0.0, 0.0, 3.0),
        angle_radians: angle,
    }
}

#[test]
fn draft_signed_planar_faces_preserves_history_and_input() {
    let session = Session::new().unwrap();
    let body = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
        .unwrap();
    let face = (0..6)
        .map(|index| session.subshape(&body, ShapeType::Face, index).unwrap())
        .find(|face| session.center_of_mass(face).unwrap().x > 9.9)
        .unwrap();
    for angle in [0.1_f64, -0.1] {
        let result = session.draft(&body, &[&face], options(angle)).unwrap();
        assert!((session.volume(&result).unwrap() - (1000.0 - 500.0 * angle.tan())).abs() < 1e-7);
        assert!(session.is_valid(&result).unwrap());
        assert!(
            session
                .history_count(&result, &face, HistoryRelation::Modified)
                .unwrap()
                > 0
        );
        assert!((session.volume(&body).unwrap() - 1000.0).abs() < 1e-7);
    }
    drop(face);
    drop(body);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn draft_cylindrical_face_makes_exact_frustum() {
    let session = Session::new().unwrap();
    let body = session
        .create_cylinder(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            2.0,
            10.0,
        )
        .unwrap();
    let face = (0..3)
        .map(|index| session.subshape(&body, ShapeType::Face, index).unwrap())
        .find(|face| !session.face_is_planar(face).unwrap())
        .unwrap();
    let angle = 0.05_f64;
    let result = session.draft(&body, &[&face], options(angle)).unwrap();
    let top = 2.0 - 10.0 * angle.tan();
    assert!(
        (session.volume(&result).unwrap()
            - std::f64::consts::PI * 10.0 * (4.0 + 2.0 * top + top * top) / 3.0)
            .abs()
            < 1e-7
    );
    assert!(session.is_valid(&result).unwrap());
    drop(result);
    drop(face);
    drop(body);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn draft_invalid_selection_parameters_and_sessions_do_not_leak() {
    let session = Session::new().unwrap();
    let body = unit_box(&session, 0.0);
    let face = session.subshape(&body, ShapeType::Face, 0).unwrap();
    let edge = session.subshape(&body, ShapeType::Edge, 0).unwrap();
    let other = unit_box(&session, 5.0);
    let stranger = session.subshape(&other, ShapeType::Face, 0).unwrap();
    let count = session.shape_count().unwrap();
    for faces in [vec![], vec![&face, &face], vec![&edge], vec![&stranger]] {
        assert!(session.draft(&body, &faces, options(0.1)).is_err());
        assert_eq!(session.shape_count().unwrap(), count);
    }
    for angle in [0.0, f64::NAN, f64::INFINITY, std::f64::consts::FRAC_PI_2] {
        assert!(session.draft(&body, &[&face], options(angle)).is_err());
    }
    for case in 0..3 {
        let mut invalid = options(0.1);
        match case {
            0 => invalid.neutral_origin.x = f64::NAN,
            1 => invalid.neutral_normal = Vec3::new(0.0, 0.0, 0.0),
            _ => invalid.pull_direction = Vec3::new(0.0, 0.0, 0.0),
        }
        assert!(session.draft(&body, &[&face], invalid).is_err());
    }
    let second = Session::new().unwrap();
    assert_wrong_session(second.draft(&body, &[&face], options(0.1)).unwrap_err());
    assert_eq!(session.shape_count().unwrap(), count);
}

#[test]
fn draft_failures_report_occt_status_and_problematic_shape() {
    let session = Session::new().unwrap();
    let body = unit_box(&session, 0.0);
    let face = session.subshape(&body, ShapeType::Face, 0).unwrap();
    let mut invalid = options(0.1);
    invalid.neutral_normal = Vec3::new(1.0, 0.0, 0.0);
    let error = session.draft(&body, &[&face], invalid).unwrap_err();
    assert_eq!(error.status, 6);
    assert!(error.message.contains("Draft_"));
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == DiagnosticKind::Draft)
    );
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.has_shape)
    );
    let diagnostic_shape = session.last_diagnostic_shape(0).unwrap();
    drop(diagnostic_shape);
    let sphere = session
        .create_sphere(Vec3::new(0.0, 0.0, 0.0), 1.0)
        .unwrap();
    let curved = session.subshape(&sphere, ShapeType::Face, 0).unwrap();
    assert_eq!(
        session
            .draft(&sphere, &[&curved], options(0.1))
            .unwrap_err()
            .status,
        4
    );
}
