use super::*;
#[test]
fn native_plane_mirrors_have_correct_geometry_mass_and_source_history() {
    let session = Session::new().unwrap();
    let source = session
        .create_box(Vec3::new(2.0, 1.0, 0.0), Vec3::new(3.0, 4.0, 5.0))
        .unwrap();
    let face = session.subshape(&source, ShapeType::Face, 0).unwrap();
    for normal in [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(1e300, 0.0, 0.0),
        Vec3::new(1e-300, 0.0, 0.0),
    ] {
        let mirrored = session
            .mirror(&source, Vec3::new(0.0, 0.0, 0.0), normal)
            .unwrap();
        assert!(session.is_valid(&mirrored).unwrap());
        assert!((session.volume(&mirrored).unwrap() - 60.0).abs() < 1e-7);
        let bounds = session.exact_bounds(&mirrored).unwrap();
        assert!((bounds.min.x + 5.0).abs() < 1e-7 && (bounds.max.x + 2.0).abs() < 1e-7);
        assert_eq!(
            session
                .history_count(&mirrored, &face, HistoryRelation::Modified)
                .unwrap(),
            1
        );
        let mapped = session
            .history(&mirrored, &face, HistoryRelation::Modified, 0)
            .unwrap();
        let actual = session
            .subshapes(&mirrored, ShapeType::Face)
            .unwrap()
            .into_iter()
            .find(|f| session.is_same(f, &mapped).unwrap())
            .unwrap();
        let actual_normal = session.face_normal(&actual).unwrap();
        let before = session.face_normal(&face).unwrap();
        let after = session.face_normal(&mapped).unwrap();
        assert!(
            (before.x + after.x).abs() < 1e-7
                && (before.y - after.y).abs() < 1e-7
                && (before.z - after.z).abs() < 1e-7,
            "source={before:?} history={after:?} actual={actual_normal:?}"
        );
        let before = session.center_of_mass(&source).unwrap();
        let after = session.center_of_mass(&mirrored).unwrap();
        assert!(
            (before.x + after.x).abs() < 1e-7
                && (before.y - after.y).abs() < 1e-7
                && (before.z - after.z).abs() < 1e-7
        );
        let twice = session
            .mirror(&mirrored, Vec3::new(0.0, 0.0, 0.0), normal)
            .unwrap();
        let common = session.common(&source, &twice).unwrap();
        assert!((session.volume(&common).unwrap() - 60.0).abs() < 1e-7);
    }
    let oblique = session
        .mirror(
            &source,
            Vec3::new(10.0, -2.0, 5.0),
            Vec3::new(1.0, 1.0, 0.0),
        )
        .unwrap();
    let b = session.exact_bounds(&oblique).unwrap();
    assert!(
        (b.min.x - 3.0).abs() < 1e-7
            && (b.max.x - 7.0).abs() < 1e-7
            && (b.min.y - 3.0).abs() < 1e-7
            && (b.max.y - 6.0).abs() < 1e-7
    );
    let distant = session
        .mirror(&source, Vec3::new(1e6, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0))
        .unwrap();
    assert!((session.volume(&distant).unwrap() - 60.0).abs() < 1e-7);
    assert!((session.exact_bounds(&distant).unwrap().min.x - (2e6 - 5.0)).abs() < 1e-7);
    let b = session.exact_bounds(&source).unwrap();
    assert!((b.min.x - 2.0).abs() < 1e-7 && (b.max.x - 5.0).abs() < 1e-7);
    drop((source, face, oblique, distant));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn mirror_rejects_invalid_planes_and_foreign_shapes_without_scratch_handles() {
    let session = Session::new().unwrap();
    let source = unit_box(&session, 0.0);
    let baseline = session.shape_count().unwrap();
    for (origin, normal) in [
        (Vec3::new(f64::NAN, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(f64::INFINITY, 0.0, 0.0)),
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0)),
    ] {
        assert_eq!(
            session.mirror(&source, origin, normal).unwrap_err().status,
            1
        );
        assert_eq!(session.shape_count().unwrap(), baseline);
    }
    let other = Session::new().unwrap();
    let foreign = unit_box(&other, 0.0);
    assert_eq!(
        session
            .mirror(&foreign, Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0))
            .unwrap_err()
            .status,
        1
    );
    drop(source);
    assert_eq!(session.shape_count().unwrap(), 0);
}
