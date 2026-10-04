//! Primitives, wires, faces, sweeps, lofts, sewing, solids, and recipes.

use super::*;

#[test]
fn sewing_independent_faces_builds_a_closed_solid() {
    let session = Session::new().unwrap();
    let corner = |x, y, z| Vec3::new(x, y, z);
    let square = |points: [Vec3; 4]| {
        let wire = session.create_polyline_wire(&points, true).unwrap();
        session.create_face_from_wire(&wire).unwrap()
    };
    let (p000, p100, p110, p010) = (
        corner(0.0, 0.0, 0.0),
        corner(2.0, 0.0, 0.0),
        corner(2.0, 3.0, 0.0),
        corner(0.0, 3.0, 0.0),
    );
    let (p001, p101, p111, p011) = (
        corner(0.0, 0.0, 4.0),
        corner(2.0, 0.0, 4.0),
        corner(2.0, 3.0, 4.0),
        corner(0.0, 3.0, 4.0),
    );
    let faces = [
        square([p000, p010, p110, p100]),
        square([p001, p101, p111, p011]),
        square([p000, p100, p101, p001]),
        square([p010, p011, p111, p110]),
        square([p000, p001, p011, p010]),
        square([p100, p110, p111, p101]),
    ];
    let face_refs = faces.iter().collect::<Vec<_>>();

    let shell = session.sew(&face_refs, 1e-6).unwrap();
    assert_eq!(session.shape_type(&shell).unwrap(), ShapeType::Shell);
    assert_eq!(session.subshape_count(&shell, ShapeType::Edge).unwrap(), 12);
    assert_eq!(
        session
            .history_count(&shell, &faces[2], HistoryRelation::Modified)
            .unwrap(),
        1
    );
    let solid = session.make_solid(&shell).unwrap();
    assert!((session.volume(&solid).unwrap() - 24.0).abs() < 1e-9);
    assert!(session.is_valid(&solid).unwrap());

    // Without the last face the shell stays open; far-apart faces do not join.
    let open = session.sew(&face_refs[..5], 1e-6).unwrap();
    assert_eq!(session.make_solid(&open).unwrap_err().status, 4);
    let apart = session
        .translate(&faces[1], Vec3::new(0.0, 0.0, 10.0))
        .unwrap();
    let loose = session.sew(&[&faces[0], &apart], 1e-6).unwrap();
    assert_eq!(session.make_solid(&loose).unwrap_err().status, 4);
    assert_eq!(session.sew(&[], 1e-6).unwrap_err().status, 1);
    assert_eq!(session.sew(&face_refs, 0.0).unwrap_err().status, 1);
}

#[test]
fn multiple_shells_build_a_solid_with_an_internal_void() {
    let session = Session::new().unwrap();
    let outer = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
        .unwrap();
    let void = session
        .create_box(Vec3::new(2.0, 2.0, 2.0), Vec3::new(2.0, 2.0, 2.0))
        .unwrap();
    let solid = session.make_solid_from_shells(&[&void, &outer]).unwrap();
    assert_eq!(session.shape_type(&solid).unwrap(), ShapeType::Solid);
    assert_eq!(session.subshape_count(&solid, ShapeType::Shell).unwrap(), 2);
    assert!((session.volume(&solid).unwrap() - 992.0).abs() < 1e-9);
    assert!(session.is_valid(&solid).unwrap());

    let crossing = session
        .create_box(Vec3::new(9.0, 9.0, 9.0), Vec3::new(2.0, 2.0, 2.0))
        .unwrap();
    assert_eq!(
        session
            .make_solid_from_shells(&[&outer, &crossing])
            .unwrap_err()
            .status,
        4
    );
    assert_eq!(session.make_solid_from_shells(&[]).unwrap_err().status, 1);
}

#[test]
fn constructs_and_inspects_shapes() {
    let session = Session::new().unwrap();
    let block = session
        .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    assert!(session.is_valid(&block).unwrap());
    let bounds = session.bounds(&block).unwrap();
    let close = |left: f64, right: f64| (left - right).abs() < 1e-6;
    assert!(close(bounds.min.x, 1.0));
    assert!(close(bounds.min.y, 2.0));
    assert!(close(bounds.min.z, 3.0));
    assert!(close(bounds.max.x, 11.0));
    assert!(close(bounds.max.y, 22.0));
    assert!(close(bounds.max.z, 33.0));

    let stone = session
        .create_polygon_prism(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(8.0, -1.0, 0.0),
                Vec3::new(11.0, 5.0, 0.0),
                Vec3::new(5.0, 10.0, 0.0),
                Vec3::new(-2.0, 6.0, 0.0),
            ],
            Vec3::new(0.0, 0.0, 4.0),
        )
        .unwrap();
    assert!(session.is_valid(&stone).unwrap());
    assert_eq!(session.shape_count().unwrap(), 2);
}

#[test]
fn constructs_generic_round_primitives() {
    let session = Session::new().unwrap();
    let cylinder = session
        .create_cylinder(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 2.0, 5.0)
        .unwrap();
    let cone = session
        .create_cone(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            3.0,
            1.0,
            8.0,
        )
        .unwrap();
    let sphere = session
        .create_sphere(Vec3::new(5.0, 6.0, 7.0), 4.0)
        .unwrap();

    assert!(session.is_valid(&cylinder).unwrap());
    assert!(session.is_valid(&cone).unwrap());
    assert!(session.is_valid(&sphere).unwrap());
    let bounds = session.bounds(&sphere).unwrap();
    assert!((bounds.min.x - 1.0).abs() < 1e-6);
    assert!((bounds.max.z - 11.0).abs() < 1e-6);

    let error = session
        .create_cylinder(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0), 1.0, 1.0)
        .unwrap_err();
    assert_eq!(error.status, 1);
    assert_eq!(error.message, "cylinder axis must be nonzero");
}

#[test]
fn rejects_invalid_generic_primitive_and_transform_parameters() {
    let session = Session::new().unwrap();
    let source = unit_box(&session, 0.0);

    assert_eq!(
        session
            .create_cone(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                0.0,
                0.0,
                1.0,
            )
            .unwrap_err()
            .status,
        1
    );
    assert_eq!(
        session
            .create_sphere(Vec3::new(0.0, 0.0, 0.0), 0.0)
            .unwrap_err()
            .status,
        1
    );
    assert_eq!(
        session
            .translate(&source, Vec3::new(f64::NAN, 0.0, 0.0))
            .unwrap_err()
            .status,
        1
    );
    assert_eq!(
        session
            .rotate(
                &source,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 0.0),
                1.0,
            )
            .unwrap_err()
            .status,
        1
    );
    assert_eq!(
        session
            .scale(&source, Vec3::new(0.0, 0.0, 0.0), 0.0)
            .unwrap_err()
            .status,
        1
    );
    assert_eq!(session.shape_count().unwrap(), 1);
    assert!(session.is_valid(&source).unwrap());
}

#[test]
fn mixed_segment_wires_preserve_exact_arcs() {
    let session = Session::new().unwrap();
    // Both orientations and a major arc: area of the circular segment.
    for (middle, end, area) in [
        (
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            std::f64::consts::PI / 2.0,
        ),
        (
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            std::f64::consts::PI / 2.0,
        ),
        (
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            3.0 * std::f64::consts::PI / 4.0 + 0.5,
        ),
    ] {
        let start = Vec3::new(1.0, 0.0, 0.0);
        let wire = session
            .create_segment_wire(
                &[
                    WireSegment::Arc { start, middle, end },
                    WireSegment::Line {
                        start: end,
                        end: start,
                    },
                ],
                true,
            )
            .unwrap();
        let face = session.create_face_from_wire(&wire).unwrap();
        assert!(session.is_valid(&face).unwrap());
        assert!((session.surface_area(&face).unwrap() - area).abs() < 1e-9);
        let prism = session
            .create_prism_from_face(&face, Vec3::new(0.0, 0.0, 3.0))
            .unwrap();
        assert!((session.volume(&prism).unwrap() - 3.0 * area).abs() < 1e-9);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn invalid_segment_wires_do_not_leak_handles() {
    let session = Session::new().unwrap();
    assert!(session.create_segment_wire(&[], false).is_err());
    let start = Vec3::new(1.0, 0.0, 0.0);
    let end = Vec3::new(-1.0, 0.0, 0.0);
    let arc = WireSegment::Arc {
        start,
        middle: Vec3::new(0.0, 1.0, 0.0),
        end,
    };
    assert!(session.create_segment_wire(&[arc], true).is_err());
    assert!(
        session
            .create_segment_wire(&[arc, WireSegment::Line { start, end }], false)
            .is_err()
    );
    assert!(
        session
            .create_segment_wire(
                &[WireSegment::Arc {
                    start,
                    middle: Vec3::new(0.0, 0.0, 0.0),
                    end
                }],
                false
            )
            .is_err()
    );
    let open = session.create_segment_wire(&[arc], false).unwrap();
    assert!(session.is_valid(&open).unwrap());
    drop(open);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn revolves_faces_with_signed_partial_and_full_sweeps_and_history() {
    let session = Session::new().unwrap();
    let wire = session
        .create_polyline_wire(
            &[
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 2.0),
                Vec3::new(1.0, 0.0, 2.0),
            ],
            true,
        )
        .unwrap();
    let face = session.create_face_from_wire(&wire).unwrap();
    let edge = session.subshape(&face, ShapeType::Edge, 1).unwrap();
    for angle in [
        std::f64::consts::TAU,
        std::f64::consts::PI,
        -std::f64::consts::PI / 2.0,
        -std::f64::consts::TAU,
    ] {
        let solid = session
            .create_revolve_from_face(
                &face,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 7.0),
                angle,
            )
            .unwrap();
        assert_eq!(session.shape_type(&solid).unwrap(), ShapeType::Solid);
        assert!(session.is_valid(&solid).unwrap());
        assert!((session.volume(&solid).unwrap() - 8.0 * angle.abs()).abs() < 1e-7);
        assert!(
            session
                .history_count(&solid, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
        if angle < 0.0 && angle.abs() < std::f64::consts::PI {
            let bounds = session.bounds(&solid).unwrap();
            assert!((bounds.min.y + 3.0).abs() < 1e-6);
            assert!(bounds.max.y.abs() < 1e-6);
        }
    }
    drop((face, wire, edge));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn revolve_rejects_invalid_values_and_foreign_or_stale_faces() {
    let session = Session::new().unwrap();
    let other = Session::new().unwrap();
    let wire = session
        .create_circle_wire(Vec3::new(3.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), 1.0)
        .unwrap();
    let face = session.create_face_from_wire(&wire).unwrap();
    let zero = Vec3::new(0.0, 0.0, 0.0);
    let up = Vec3::new(0.0, 0.0, 1.0);
    for (origin, axis, angle) in [
        (zero, zero, 1.0),
        (Vec3::new(f64::NAN, 0.0, 0.0), up, 1.0),
        (zero, Vec3::new(0.0, f64::INFINITY, 0.0), 1.0),
        (zero, up, 0.0),
        (zero, up, f64::NAN),
        (zero, up, 7.0),
    ] {
        assert!(
            session
                .create_revolve_from_face(&face, origin, axis, angle)
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 2);
    }
    assert!(
        session
            .create_revolve_from_face(&wire, zero, up, 1.0)
            .is_err()
    );
    assert!(
        other
            .create_revolve_from_face(&face, zero, up, 1.0)
            .is_err()
    );
    assert_eq!(other.shape_count().unwrap(), 0);
    session.clear().unwrap();
    assert!(
        session
            .create_revolve_from_face(&face, zero, up, 1.0)
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn reuses_wire_and_face_handles_to_build_geometry() {
    let session = Session::new().unwrap();
    let rectangle = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(6.0, 0.0, 0.0),
        Vec3::new(6.0, 4.0, 0.0),
        Vec3::new(0.0, 4.0, 0.0),
    ];
    let wire = session.create_polyline_wire(&rectangle, true).unwrap();
    let face = session.create_face_from_wire(&wire).unwrap();
    let source_edge = session.subshape(&face, ShapeType::Edge, 0).unwrap();
    let prism = session
        .create_prism_from_face(&face, Vec3::new(0.0, 0.0, 3.0))
        .unwrap();
    let circle = session
        .create_circle_wire(Vec3::new(10.0, 20.0, 30.0), Vec3::new(0.0, 0.0, 1.0), 5.0)
        .unwrap();
    let disk = session.create_face_from_wire(&circle).unwrap();

    assert!(session.is_valid(&wire).unwrap());
    assert!(session.is_valid(&face).unwrap());
    assert!(session.is_valid(&prism).unwrap());
    assert!(session.is_valid(&circle).unwrap());
    assert!(session.is_valid(&disk).unwrap());
    assert_eq!(
        session
            .history_count(&prism, &source_edge, HistoryRelation::Generated)
            .unwrap(),
        1
    );
    let generated_face = session
        .history(&prism, &source_edge, HistoryRelation::Generated, 0)
        .unwrap();
    assert_eq!(
        session.shape_type(&generated_face).unwrap(),
        ShapeType::Face
    );
    let prism_bounds = session.bounds(&prism).unwrap();
    assert!((prism_bounds.max.x - 6.0).abs() < 1e-6);
    assert!((prism_bounds.max.y - 4.0).abs() < 1e-6);
    assert!((prism_bounds.max.z - 3.0).abs() < 1e-6);
    let disk_bounds = session.bounds(&disk).unwrap();
    assert!((disk_bounds.min.x - 5.0).abs() < 1e-6);
    assert!((disk_bounds.max.y - 25.0).abs() < 1e-6);
    assert_eq!(session.shape_count().unwrap(), 7);
}

#[test]
fn rejects_invalid_wire_and_face_inputs_without_creating_shapes() {
    let session = Session::new().unwrap();
    let box_shape = unit_box(&session, 0.0);
    let repeated_endpoint = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
    ];

    assert_eq!(
        session
            .create_polyline_wire(&repeated_endpoint, true)
            .unwrap_err()
            .status,
        1
    );
    assert_eq!(
        session
            .create_face_from_wire(&box_shape)
            .unwrap_err()
            .status,
        4
    );
    assert_eq!(
        session
            .create_prism_from_face(&box_shape, Vec3::new(0.0, 0.0, 1.0))
            .unwrap_err()
            .status,
        4
    );
    assert_eq!(session.shape_count().unwrap(), 1);
}

#[test]
#[allow(deprecated)]
fn constructs_a_chamfered_filleted_faceted_stone() {
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
    let stone = session
        .create_faceted_stone(&bottom, &top, Vec3::new(0.0, 0.5, 9.0), 0.8, 0.7)
        .unwrap();
    assert!(session.is_valid(&stone).unwrap());
}

#[test]
#[allow(deprecated)]
fn creates_wall_torch_with_flame_anchored_light() {
    let session = Session::new().unwrap();
    let torch = session
        .create_wall_torch(Vec3::new(0.0, 120.0, 130.0), Vec3::new(1.0, 0.0, 0.0), 1.0)
        .unwrap();
    assert!(session.is_valid(&torch.fixture).unwrap());
    assert!(session.is_valid(&torch.flame).unwrap());
    assert!(torch.light.position.x > 40.0);
    assert!(torch.light.position.z > 160.0);
    assert_eq!(torch.light.light_type, LightType::Positional);
    assert_eq!(torch.light.direction, Vec3::new(0.0, 0.0, 0.0));
    assert!(!torch.light.cast_shadows);
}

#[test]
fn sweeps_a_round_tube_along_a_polyline() {
    let session = Session::new().unwrap();
    let tube = session
        .create_polyline_tube(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(20.0, 0.0, 0.0),
                Vec3::new(30.0, 10.0, 0.0),
                Vec3::new(30.0, 25.0, 5.0),
            ],
            2.0,
        )
        .unwrap();
    assert!(session.is_valid(&tube).unwrap());
}

#[test]
fn lofts_sections_and_builds_a_compound() {
    let session = Session::new().unwrap();
    let root = [
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(10.0, 0.0, -1.0),
        Vec3::new(10.0, 0.0, 1.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let tip = [
        Vec3::new(2.0, 20.0, -0.5),
        Vec3::new(8.0, 20.0, -0.5),
        Vec3::new(8.0, 20.0, 0.5),
        Vec3::new(2.0, 20.0, 0.5),
    ];
    let loft = session.create_loft(&[&root, &tip], true, false).unwrap();
    assert!(session.is_valid(&loft).unwrap());
    let marker = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
        .unwrap();
    let compound = session.create_compound(&[&loft, &marker]).unwrap();
    assert!(session.is_valid(&compound).unwrap());
    assert!(session.is_valid(&loft).unwrap());
    assert!(session.is_valid(&marker).unwrap());
}

#[test]
fn spline_lofts_interpolate_smooth_sections_with_one_corner() {
    let session = Session::new().unwrap();
    let circle = |y: f64| {
        (0..16)
            .map(|index| {
                let angle = std::f64::consts::TAU * index as f64 / 16.0;
                Vec3::new(10.0 * angle.cos(), y, 10.0 * angle.sin())
            })
            .collect::<Vec<_>>()
    };
    let (root, tip) = (circle(0.0), circle(20.0));
    let exact = std::f64::consts::PI * 100.0 * 20.0;
    let polygon = session.create_loft(&[&root, &tip], true, true).unwrap();
    let smooth = session
        .create_spline_loft(&[&root, &tip], true, true)
        .unwrap();
    assert!(session.is_valid(&smooth).unwrap());
    // A 16-gon falls 2.5% short of the circle; the interpolated section does
    // not, apart from the small corner kept at the first point.
    let polygon_error = (session.volume(&polygon).unwrap() - exact).abs() / exact;
    let smooth_error = (session.volume(&smooth).unwrap() - exact).abs() / exact;
    assert!(polygon_error > 0.02, "{polygon_error}");
    assert!(smooth_error < 2e-3, "{smooth_error}");
    // One smooth side face per span plus two caps, instead of one per segment.
    assert_eq!(session.subshape_count(&smooth, ShapeType::Face).unwrap(), 3);
    assert_eq!(
        session.subshape_count(&polygon, ShapeType::Face).unwrap(),
        18
    );

    // Smoothing across three sections still yields a valid solid.
    let middle = circle(10.0)
        .into_iter()
        .map(|point| Vec3::new(point.x * 0.5, point.y, point.z * 0.5))
        .collect::<Vec<_>>();
    let waisted = session
        .create_spline_loft(&[&root, &middle, &tip], true, false)
        .unwrap();
    assert!(session.is_valid(&waisted).unwrap());
    assert!(session.volume(&waisted).unwrap() < exact);

    let mut repeated = root.clone();
    repeated[3] = repeated[2];
    assert!(
        session
            .create_spline_loft(&[&repeated, &tip], true, true)
            .is_err()
    );
    assert!(session.create_spline_loft(&[&root], true, true).is_err());
}

/// NACA 0012 at cosine spacing: trailing edge, upper surface, leading edge,
/// lower surface, without repeating the trailing edge.
fn naca_0012(chord: f64, y: f64) -> Vec<Vec3> {
    let thickness = |x: f64| {
        0.6 * (0.2969 * x.sqrt() - 0.126 * x - 0.3516 * x * x + 0.2843 * x.powi(3)
            - 0.1036 * x.powi(4))
    };
    let x = |i: usize| (1.0 + (std::f64::consts::PI * i as f64 / 40.0).cos()) / 2.0;
    let upper = (0..=40).map(|i| (x(i), thickness(x(i))));
    let lower = (1..40).rev().map(|i| (x(i), -thickness(x(i))));
    upper
        .chain(lower)
        .map(|(x, z)| Vec3::new(chord * x, y, chord * z))
        .collect()
}

#[test]
fn volumes_of_freeform_lofts_use_adaptive_integration() {
    let session = Session::new().unwrap();
    let (root, tip) = (naca_0012(100.0, 0.0), naca_0012(100.0, 1000.0));
    let polygon = session.create_loft(&[&root, &tip], true, true).unwrap();
    let smooth = session
        .create_spline_loft(&[&root, &tip], true, true)
        .unwrap();
    // The smooth section circumscribes the inscribed polygon. Fixed-order
    // integration once reported the smooth wing about 20% too small.
    let polygon_volume = session.volume(&polygon).unwrap();
    let smooth_volume = session.volume(&smooth).unwrap();
    assert!(
        smooth_volume > polygon_volume,
        "{smooth_volume} <= {polygon_volume}"
    );
    assert!((smooth_volume - polygon_volume) / polygon_volume < 5e-3);
    let adaptive = session.mass_properties(&smooth).unwrap().volume;
    assert!((smooth_volume - adaptive).abs() / adaptive < 1e-8);
    let center = session.center_of_mass(&smooth).unwrap();
    assert!((center.y - 500.0).abs() < 1e-6, "{center:?}");
}
