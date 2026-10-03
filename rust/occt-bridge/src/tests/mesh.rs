use super::*;

#[test]
fn meshes_have_source_face_indices_outward_winding_and_preserve_brep() {
    let session = Session::new().unwrap();
    let shape = session
        .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let directory = std::env::temp_dir();
    let before = directory.join(format!("occt-mesh-before-{}.brep", std::process::id()));
    let after = directory.join(format!("occt-mesh-after-{}.brep", std::process::id()));
    session.save_brep(&shape, &before).unwrap();
    let mesh = session
        .surface_mesh(&shape, MeshOptions::default())
        .unwrap();
    assert_eq!(mesh.len(), 12);
    let faces = session.subshapes(&shape, ShapeType::Face).unwrap();
    assert_eq!(faces.len(), 6);
    let mut counts = [0; 6];
    let center = Vec3::new(6.0, 12.0, 18.0);
    for triangle in &mesh {
        counts[triangle.face_index] += 1;
        let [a, b, c] = triangle.points;
        let ab = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
        let ac = Vec3::new(c.x - a.x, c.y - a.y, c.z - a.z);
        let normal = Vec3::new(
            ab.y * ac.z - ab.z * ac.y,
            ab.z * ac.x - ab.x * ac.z,
            ab.x * ac.y - ab.y * ac.x,
        );
        assert!(
            normal.x * (a.x - center.x) + normal.y * (a.y - center.y) + normal.z * (a.z - center.z)
                > 0.0
        );
        let face_center = session.center_of_mass(&faces[triangle.face_index]).unwrap();
        let face_normal = session.face_normal(&faces[triangle.face_index]).unwrap();
        assert!(
            ((a.x - face_center.x) * face_normal.x
                + (a.y - face_center.y) * face_normal.y
                + (a.z - face_center.z) * face_normal.z)
                .abs()
                < 1e-7
        );
    }
    assert_eq!(counts, [2; 6]);
    session.save_brep(&shape, &after).unwrap();
    assert_eq!(
        std::fs::read(&before).unwrap(),
        std::fs::read(&after).unwrap()
    );
    std::fs::remove_file(before).unwrap();
    std::fs::remove_file(after).unwrap();
    drop(faces);
    assert_eq!(session.shape_count().unwrap(), 1);
}

#[test]
fn mesh_resolution_budget_invalid_geometry_and_foreign_sessions_are_checked() {
    let session = Session::new().unwrap();
    let shape = unit_box(&session, 0.0);
    let defaults = MeshOptions::default();
    for options in [
        MeshOptions {
            maximum_triangles: 1,
            ..defaults
        },
        MeshOptions {
            maximum_triangles: 0,
            ..defaults
        },
        MeshOptions {
            maximum_triangles: usize::MAX,
            ..defaults
        },
        MeshOptions {
            linear_deflection: f64::NAN,
            ..defaults
        },
        MeshOptions {
            linear_deflection: 1e-12,
            ..defaults
        },
        MeshOptions {
            angular_deflection_radians: 0.0,
            ..defaults
        },
        MeshOptions {
            angular_deflection_radians: 4.0,
            ..defaults
        },
    ] {
        assert!(session.surface_mesh(&shape, options).is_err());
    }
    let edge = session.subshape(&shape, ShapeType::Edge, 0).unwrap();
    assert!(session.surface_mesh(&edge, defaults).is_err());
    let foreign = Session::new().unwrap();
    assert!(foreign.surface_mesh(&shape, defaults).is_err());
    let cylinder = session
        .create_cylinder(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            10.0,
            20.0,
        )
        .unwrap();
    let coarse = session
        .surface_mesh(
            &cylinder,
            MeshOptions {
                linear_deflection: 1.0,
                angular_deflection_radians: 1.0,
                ..defaults
            },
        )
        .unwrap();
    let fine = session.surface_mesh(&cylinder, defaults).unwrap();
    assert!(fine.len() > coarse.len());
    assert!(fine.iter().all(|triangle| triangle.face_index < 3));
    assert!((session.volume(&cylinder).unwrap() - std::f64::consts::PI * 2000.0).abs() < 1e-7);
}
