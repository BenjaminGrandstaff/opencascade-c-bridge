use super::*;
use occt_bridge::ShapeType;

#[test]
fn composes_wall_torch_from_generic_bridge_operations() {
    let session = Session::new().unwrap();
    let torch = create_wall_torch(
        &session,
        Vec3::new(0.0, 120.0, 130.0),
        Vec3::new(1.0, 0.0, 0.0),
        1.0,
    )
    .unwrap();

    assert!(session.is_valid(&torch.fixture).unwrap());
    assert!(session.is_valid(&torch.flame).unwrap());
    assert!(torch.light.position.x > 40.0);
    assert!(torch.light.position.z > 160.0);
    assert!(!torch.light.cast_shadows);
    assert_eq!(session.shape_count().unwrap(), 2);
}

#[test]
fn rejects_invalid_torch_parameters_before_creating_geometry() {
    let session = Session::new().unwrap();
    let error = create_wall_torch(
        &session,
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        1.0,
    )
    .unwrap_err();

    assert_eq!(error.status, 1);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn exposes_faceted_stone_as_a_recipe() {
    let session = Session::new().unwrap();
    let bottom = [
        Vec3::new(-20.0, -14.0, 0.0),
        Vec3::new(-7.0, -21.0, 0.0),
        Vec3::new(17.0, -18.0, 0.0),
        Vec3::new(23.0, -2.0, 0.0),
        Vec3::new(17.0, 17.0, 0.0),
        Vec3::new(-3.0, 22.0, 0.0),
        Vec3::new(-24.0, 9.0, 0.0),
    ];
    let top = [
        Vec3::new(-18.0, -12.5, 6.4),
        Vec3::new(-6.0, -19.0, 7.1),
        Vec3::new(15.0, -16.0, 6.7),
        Vec3::new(20.5, -1.5, 7.5),
        Vec3::new(15.0, 15.0, 6.8),
        Vec3::new(-2.5, 19.5, 7.8),
        Vec3::new(-21.0, 8.0, 6.6),
    ];
    let center = Vec3::new(0.0, 0.5, 9.0);
    for (chamfer, fillet) in [(0.8, 0.7), (0.0, 0.0), (1.2, 0.0), (0.0, 1.5)] {
        let stone = create_faceted_stone(&session, &bottom, &top, center, chamfer, fillet).unwrap();
        assert!(session.is_valid(&stone).unwrap());
        assert_eq!(session.shape_type(&stone).unwrap(), ShapeType::Solid);
        assert_eq!(
            session.shape_count().unwrap(),
            1,
            "intermediate handles leaked"
        );

        // Same algorithm as the legacy ABI constructor, now built from
        // generic faces, sewing, and shell-to-solid construction.
        #[allow(deprecated)]
        let legacy = session
            .create_faceted_stone(&bottom, &top, center, chamfer, fillet)
            .unwrap();
        let volume = session.volume(&stone).unwrap();
        assert!(volume > 0.0);
        assert!((volume - session.volume(&legacy).unwrap()).abs() <= 1e-9 * volume);
        session.remove(legacy).unwrap();
        session.remove(stone).unwrap();
    }

    let invalid = [
        create_faceted_stone(&session, &bottom[..2], &top[..2], center, 0.0, 0.0),
        create_faceted_stone(&session, &bottom, &top[..6], center, 0.0, 0.0),
        create_faceted_stone(
            &session,
            &bottom,
            &top,
            Vec3::new(f64::NAN, 0.0, 0.0),
            0.0,
            0.0,
        ),
        create_faceted_stone(&session, &bottom, &top, center, -1.0, 0.0),
        create_faceted_stone(&session, &bottom, &top, center, 0.0, f64::INFINITY),
    ];
    assert!(
        invalid
            .iter()
            .all(|result| result.as_ref().unwrap_err().status == 1)
    );
    for (chamfer, fillet) in [(50.0, 0.0), (0.0, 50.0)] {
        let error =
            create_faceted_stone(&session, &bottom, &top, center, chamfer, fillet).unwrap_err();
        assert_eq!(error.status, 4, "{error}");
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
