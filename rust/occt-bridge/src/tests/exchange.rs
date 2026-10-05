//! BREP, STEP, and STL exchange.

use super::*;

fn step_test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "occt-bridge-{name}-{}-{}.step",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

fn binary_stl_triangle_count(path: &Path) -> u32 {
    let bytes = fs::read(path).unwrap();
    assert!(bytes.len() >= 84);
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap());
    assert_eq!(bytes.len(), 84 + 50 * count as usize);
    count
}

#[test]
fn step_round_trip_preserves_geometry_and_topology() {
    let session = Session::new().unwrap();
    let source = session
        .create_box(Vec3::new(-2.0, 3.0, 5.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let path = step_test_path("round-trip");
    session.save_step(&source, &path).unwrap();
    let loaded = session.load_step(&path).unwrap();

    assert!(session.is_valid(&loaded).unwrap());
    assert_eq!(session.shape_type(&loaded).unwrap(), ShapeType::Solid);
    assert_eq!(
        session.subshape_count(&loaded, ShapeType::Face).unwrap(),
        session.subshape_count(&source, ShapeType::Face).unwrap()
    );
    assert_eq!(
        session.subshape_count(&loaded, ShapeType::Edge).unwrap(),
        session.subshape_count(&source, ShapeType::Edge).unwrap()
    );
    let source_bounds = session.bounds(&source).unwrap();
    let loaded_bounds = session.bounds(&loaded).unwrap();
    for (actual, expected) in [
        (loaded_bounds.min.x, source_bounds.min.x),
        (loaded_bounds.min.y, source_bounds.min.y),
        (loaded_bounds.min.z, source_bounds.min.z),
        (loaded_bounds.max.x, source_bounds.max.x),
        (loaded_bounds.max.y, source_bounds.max.y),
        (loaded_bounds.max.z, source_bounds.max.z),
    ] {
        assert!((actual - expected).abs() < 1e-6);
    }
    assert!((session.volume(&loaded).unwrap() - session.volume(&source).unwrap()).abs() < 1e-6);
    assert_eq!(session.shape_count().unwrap(), 2);

    session.remove(loaded).unwrap();
    session.remove(source).unwrap();
    assert_eq!(session.shape_count().unwrap(), 0);
    fs::remove_file(path).unwrap();
}

#[test]
fn step_exchange_reports_io_and_session_errors_without_leaking_handles() {
    let session = Session::new().unwrap();
    let missing = step_test_path("missing");
    let _ = fs::remove_file(&missing);
    let error = session.load_step(&missing).unwrap_err();
    assert_eq!(error.status, 5);
    assert_eq!(error.category, "I/O error");
    assert_eq!(session.shape_count().unwrap(), 0);

    let malformed = step_test_path("malformed");
    fs::write(&malformed, b"not a STEP file\n").unwrap();
    let error = session.load_step(&malformed).unwrap_err();
    assert_eq!(error.status, 5);
    assert_eq!(session.shape_count().unwrap(), 0);
    fs::remove_file(malformed).unwrap();

    let first = Session::new().unwrap();
    let second = Session::new().unwrap();
    let shape = unit_box(&first, 0.0);
    let output = step_test_path("wrong-session");
    assert_wrong_session(second.save_step(&shape, &output).unwrap_err());
    assert!(!output.exists());
}

#[test]
fn stl_export_writes_verified_ascii_and_tessellated_binary_meshes() {
    let session = Session::new().unwrap();
    let box_shape = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let ascii_path = step_test_path("ascii-stl").with_extension("stl");
    session
        .save_stl(
            &box_shape,
            &ascii_path,
            StlOptions {
                format: StlFormat::Ascii,
                ..StlOptions::default()
            },
        )
        .unwrap();
    let ascii = fs::read_to_string(&ascii_path).unwrap();
    assert!(ascii.starts_with("solid"));
    assert_eq!(ascii.matches("facet normal").count(), 12);

    let sphere = session
        .create_sphere(Vec3::new(0.0, 0.0, 0.0), 10.0)
        .unwrap();
    let coarse_path = step_test_path("coarse-stl").with_extension("stl");
    let fine_path = step_test_path("fine-stl").with_extension("stl");
    session
        .save_stl(
            &sphere,
            &coarse_path,
            StlOptions {
                linear_deflection: 2.0,
                angular_deflection_radians: 1.0,
                format: StlFormat::Binary,
            },
        )
        .unwrap();
    session
        .save_stl(
            &sphere,
            &fine_path,
            StlOptions {
                linear_deflection: 0.1,
                angular_deflection_radians: 0.2,
                format: StlFormat::Binary,
            },
        )
        .unwrap();
    let coarse_triangles = binary_stl_triangle_count(&coarse_path);
    let fine_triangles = binary_stl_triangle_count(&fine_path);
    assert!(coarse_triangles > 0);
    assert!(fine_triangles > coarse_triangles);
    assert_eq!(session.shape_count().unwrap(), 2);

    // Each export meshes independently: a coarse export after a fine one
    // must not reuse the finer triangulation.
    let recoarse_path = step_test_path("recoarse-stl").with_extension("stl");
    session
        .save_stl(
            &sphere,
            &recoarse_path,
            StlOptions {
                linear_deflection: 2.0,
                angular_deflection_radians: 1.0,
                format: StlFormat::Binary,
            },
        )
        .unwrap();
    assert_eq!(binary_stl_triangle_count(&recoarse_path), coarse_triangles);

    // Exporting leaves no triangulation on the session's shape.
    let brep_path = step_test_path("after-stl").with_extension("brep");
    session.save_brep(&sphere, &brep_path).unwrap();
    let brep = fs::read_to_string(&brep_path).unwrap();
    assert!(
        brep.lines()
            .filter(|line| line.starts_with("Triangulations"))
            .all(|line| line == "Triangulations 0")
    );

    fs::remove_file(ascii_path).unwrap();
    fs::remove_file(coarse_path).unwrap();
    fs::remove_file(fine_path).unwrap();
    fs::remove_file(recoarse_path).unwrap();
    fs::remove_file(brep_path).unwrap();
}

#[test]
fn stl_export_rejects_invalid_options_paths_and_sessions() {
    let first = Session::new().unwrap();
    let second = Session::new().unwrap();
    let shape = unit_box(&first, 0.0);
    let output = step_test_path("invalid-stl").with_extension("stl");
    assert_wrong_session(
        second
            .save_stl(&shape, &output, StlOptions::default())
            .unwrap_err(),
    );
    assert!(!output.exists());

    for options in [
        StlOptions {
            linear_deflection: 0.0,
            ..StlOptions::default()
        },
        StlOptions {
            linear_deflection: f64::NAN,
            ..StlOptions::default()
        },
        StlOptions {
            angular_deflection_radians: -1.0,
            ..StlOptions::default()
        },
    ] {
        let error = first.save_stl(&shape, &output, options).unwrap_err();
        assert_eq!(error.status, 1);
        assert!(!output.exists());
    }
    let error = first
        .save_stl(
            &shape,
            "/nonexistent-directory/shape.stl",
            StlOptions::default(),
        )
        .unwrap_err();
    assert_eq!(error.status, 5);
    assert_eq!(first.shape_count().unwrap(), 1);
}

#[test]
fn step_assemblies_share_placed_copies_as_one_part() {
    let session = Session::new().unwrap();
    let block = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let copies =
        [-50.0, 0.0, 50.0].map(|x| session.translate(&block, Vec3::new(x, 0.0, 0.0)).unwrap());
    let pin = session
        .create_cylinder(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            2.0,
            40.0,
        )
        .unwrap();
    let names = ["a", "b", "c"];
    let mut components = copies
        .iter()
        .zip(names)
        .map(|(shape, name)| StepComponent {
            shape,
            name,
            part_name: "block",
            color: Some([0.8, 0.2, 0.1]),
        })
        .collect::<Vec<_>>();
    components.push(StepComponent {
        shape: &pin,
        name: "pin",
        part_name: "pin",
        color: None,
    });
    let path = step_test_path("assembly");
    assert_eq!(
        session
            .save_step_assembly(&path, "rack", &components)
            .unwrap(),
        2
    );
    // The flat reader sees every placed component.
    let loaded = session.load_step(&path).unwrap();
    let expected = 3.0 * 6000.0 + std::f64::consts::PI * 4.0 * 40.0;
    assert!((session.volume(&loaded).unwrap() - expected).abs() < 1e-6 * expected);
    std::fs::remove_file(&path).unwrap();

    components[0].name = "bad\0name";
    assert!(
        session
            .save_step_assembly(&path, "rack", &components)
            .is_err()
    );
    assert!(session.save_step_assembly(&path, "rack", &[]).is_err());
    assert!(!path.exists(), "argument errors write nothing");
}

#[test]
fn step_assembly_trees_keep_model_space_geometry() {
    let session = Session::new().unwrap();
    let block = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let moved = session
        .translate(&block, Vec3::new(120.0, 0.0, 0.0))
        .unwrap();
    let turned = session
        .rotate(
            &block,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            std::f64::consts::FRAC_PI_2,
        )
        .unwrap();
    let component = |shape, name| StepComponent {
        shape,
        name,
        part_name: "block",
        color: None,
    };
    let components = [
        component(&block, "outer"),
        component(&moved, "inner"),
        component(&turned, "turned"),
    ];
    let (sine, cosine) = 0.3f64.sin_cos();
    let nodes = [
        StepNode {
            name: "wing",
            parent: None,
            transform: [1.0, 0.0, 0.0, 100.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        },
        // A 0.3 rad turn about z, then 50 along y.
        StepNode {
            name: "flap",
            parent: Some(0),
            transform: [
                cosine, -sine, 0.0, 0.0, sine, cosine, 0.0, 50.0, 0.0, 0.0, 1.0, 0.0,
            ],
        },
    ];
    let face_colors = [StepFaceColor {
        component: 2,
        face: 0,
        color: [0.1, 0.6, 0.3],
    }];
    let path = step_test_path("tree");
    let parts = session
        .save_step_assembly_tree(
            &path,
            "plane",
            &nodes,
            &components,
            &[None, Some(0), Some(1)],
            &face_colors,
        )
        .unwrap();
    assert_eq!(parts, 1, "all three share the block");
    // Model space matches the shapes as placed, whatever the nesting.
    let loaded = session.load_step(&path).unwrap();
    let flat = session.create_compound(&[&block, &moved, &turned]).unwrap();
    let (got, want) = (
        session.bounds(&loaded).unwrap(),
        session.bounds(&flat).unwrap(),
    );
    for (a, b) in [
        (got.min.x, want.min.x),
        (got.min.y, want.min.y),
        (got.min.z, want.min.z),
        (got.max.x, want.max.x),
        (got.max.y, want.max.y),
        (got.max.z, want.max.z),
    ] {
        assert!((a - b).abs() < 1e-6, "{got:?} vs {want:?}");
    }
    assert!((session.volume(&loaded).unwrap() - 18_000.0).abs() < 1e-6);
    std::fs::remove_file(&path).unwrap();

    // Memberships must match the components, nodes must be nonempty, and
    // face colors must name real faces.
    let beyond = [StepFaceColor {
        face: 6,
        ..face_colors[0]
    }];
    assert!(
        session
            .save_step_assembly_tree(
                &path,
                "plane",
                &nodes,
                &components,
                &[None, Some(0), Some(1)],
                &beyond
            )
            .is_err()
    );
    assert!(
        session
            .save_step_assembly_tree(&path, "plane", &nodes, &components, &[None], &[])
            .is_err()
    );
    assert!(
        session
            .save_step_assembly_tree(
                &path,
                "plane",
                &nodes,
                &components,
                &[None, None, Some(0)],
                &[]
            )
            .is_err(),
        "the flap would be empty"
    );
    assert!(!path.exists(), "argument errors write nothing");
}
