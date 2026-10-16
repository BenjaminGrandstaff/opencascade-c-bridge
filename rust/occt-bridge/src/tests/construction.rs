//! Primitives, wires, faces, sweeps, lofts, sewing, solids, and recipes.

use super::*;
use std::f64::consts::PI;

#[test]
fn discovered_regions_split_crossings_and_select_cells() {
    let session = Session::new().unwrap();
    let origin = Vec3::new(0., 0., 0.);
    let normal = Vec3::new(0., 0., 1.);
    let x_axis = Vec3::new(1., 0., 0.);
    let circle = session.create_circle_wire(origin, normal, 10.).unwrap();
    let divider = session
        .create_polyline_wire(&[Vec3::new(-12., 0., 0.), Vec3::new(12., 0., 0.)], false)
        .unwrap();
    let inputs = [&circle, &divider];
    let all = session
        .discover_planar_regions(&inputs, origin, normal, x_axis, &[])
        .unwrap();
    assert_eq!(session.subshape_count(&all, ShapeType::Face).unwrap(), 2);
    assert!((session.surface_area(&all).unwrap() - 100. * PI).abs() < 1e-7);
    let merged = session.unify_same_domain(&all, 1e-7, 1e-9).unwrap();
    assert_eq!(session.subshape_count(&merged, ShapeType::Face).unwrap(), 1);
    assert!((session.surface_area(&merged).unwrap() - 100. * PI).abs() < 1e-7);
    let half = session
        .discover_planar_regions(&inputs, origin, normal, x_axis, &[Vec3::new(0., 5., 0.)])
        .unwrap();
    assert_eq!(session.subshape_count(&half, ShapeType::Face).unwrap(), 1);
    assert!((session.surface_area(&half).unwrap() - 50. * PI).abs() < 1e-7);
    assert!(session.is_valid(&half).unwrap());
    let edge = session.subshapes(&circle, ShapeType::Edge).unwrap();
    assert!((session.edge_length(&edge[0]).unwrap() - 20. * PI).abs() < 1e-7);
}

#[test]
fn discovered_regions_classify_holes_and_reject_bad_seeds() {
    let session = Session::new().unwrap();
    let origin = Vec3::new(0., 0., 0.);
    let normal = Vec3::new(0., 0., 1.);
    let x_axis = Vec3::new(1., 0., 0.);
    let outer = session.create_circle_wire(origin, normal, 10.).unwrap();
    let inner = session.create_circle_wire(origin, normal, 3.).unwrap();
    let annulus = session
        .discover_planar_regions(
            &[&outer, &inner],
            origin,
            normal,
            x_axis,
            &[Vec3::new(5., 0., 0.), Vec3::new(6., 0., 0.)],
        )
        .unwrap();
    assert_eq!(
        session.subshape_count(&annulus, ShapeType::Face).unwrap(),
        1
    );
    assert!((session.surface_area(&annulus).unwrap() - 91. * PI).abs() < 1e-7);
    let before = session.shape_count().unwrap();
    for (seed, message) in [
        (Vec3::new(10., 0., 0.), "boundary"),
        (Vec3::new(20., 0., 0.), "outside"),
        (Vec3::new(5., 0., 1.), "plane"),
    ] {
        let error = session
            .discover_planar_regions(&[&outer, &inner], origin, normal, x_axis, &[seed])
            .unwrap_err();
        assert!(error.message.contains(message), "{}", error.message);
    }
    assert_eq!(session.shape_count().unwrap(), before);
}

#[test]
fn discovered_regions_respect_tilted_planes_and_reject_nonplanar_curves() {
    let session = Session::new().unwrap();
    let origin = Vec3::new(5., 0., 2.);
    let normal = Vec3::new(0., 1., 0.);
    let x_axis = Vec3::new(0., 0., 1.);
    let circle = session.create_circle_wire(origin, normal, 4.).unwrap();
    let cells = session
        .discover_planar_regions(&[&circle], origin, normal, x_axis, &[origin])
        .unwrap();
    assert!((session.surface_area(&cells).unwrap() - 16. * PI).abs() < 1e-7);
    assert!(session.is_valid(&cells).unwrap());
    let out_of_plane = session
        .create_polyline_wire(&[origin, Vec3::new(5., 1., 2.)], false)
        .unwrap();
    let before = session.shape_count().unwrap();
    let error = session
        .discover_planar_regions(&[&circle, &out_of_plane], origin, normal, x_axis, &[])
        .unwrap_err();
    assert!(error.message.contains("coplanar"));
    assert_eq!(session.shape_count().unwrap(), before);
    let other = Session::new().unwrap();
    assert_wrong_session(
        other
            .discover_planar_regions(&[&circle], origin, normal, x_axis, &[])
            .unwrap_err(),
    );
}

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

fn on_circle(radius: f64, angle: f64) -> Vec3 {
    Vec3::new(radius * angle.cos(), radius * angle.sin(), 0.0)
}

#[test]
fn curve_wires_mix_lines_and_interpolated_splines() {
    let session = Session::new().unwrap();
    let pi = std::f64::consts::PI;
    // A semicircle sampled at nine points, with vertical end tangents, closed
    // by its diameter, encloses very nearly half the circle's area.
    let arch = (0..=8)
        .map(|index| on_circle(5.0, pi * index as f64 / 8.0))
        .collect::<Vec<_>>();
    let half_disc = session
        .create_curve_wire(
            &[
                CurveSegment::Spline {
                    points: arch,
                    start_tangent: Some(Vec3::new(0.0, 1.0, 0.0)),
                    end_tangent: Some(Vec3::new(0.0, -1.0, 0.0)),
                    periodic: false,
                },
                CurveSegment::Line {
                    start: Vec3::new(-5.0, 0.0, 0.0),
                    end: Vec3::new(5.0, 0.0, 0.0),
                },
            ],
            true,
        )
        .unwrap();
    let face = session.create_face_from_wire(&half_disc).unwrap();
    assert!(session.is_valid(&face).unwrap());
    let area = session.surface_area(&face).unwrap();
    assert!((area - pi * 12.5).abs() / (pi * 12.5) < 5e-3, "{area}");
    assert_eq!(
        session.subshape_count(&half_disc, ShapeType::Edge).unwrap(),
        2
    );

    // Eight points on a circle make one smooth periodic loop.
    let ring = (0..8)
        .map(|index| on_circle(5.0, 2.0 * pi * index as f64 / 8.0))
        .collect::<Vec<_>>();
    let loop_wire = session
        .create_curve_wire(
            &[CurveSegment::Spline {
                points: ring,
                start_tangent: None,
                end_tangent: None,
                periodic: true,
            }],
            true,
        )
        .unwrap();
    assert_eq!(
        session.subshape_count(&loop_wire, ShapeType::Edge).unwrap(),
        1
    );
    let disc = session.create_face_from_wire(&loop_wire).unwrap();
    let area = session.surface_area(&disc).unwrap();
    assert!((area - pi * 25.0).abs() / (pi * 25.0) < 1e-2, "{area}");
}

#[test]
fn spline_end_tangents_set_the_departure_direction() {
    let session = Session::new().unwrap();
    let points = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(5.0, 3.0, 0.0),
        Vec3::new(10.0, 0.0, 0.0),
    ];
    // Slope just after the start: free, the curve heads up toward (5, 3);
    // with a horizontal start tangent it leaves flat.
    let departure = |start_tangent| {
        let wire = session
            .create_curve_wire(
                &[CurveSegment::Spline {
                    points: points.clone(),
                    start_tangent,
                    end_tangent: None,
                    periodic: false,
                }],
                false,
            )
            .unwrap();
        let edge = session.subshape(&wire, ShapeType::Edge, 0).unwrap();
        let samples = session.edge_sample_points(&edge, 1001).unwrap();
        samples[1].y / samples[1].x
    };
    assert!(departure(None) > 0.5);
    assert!(departure(Some(Vec3::new(1.0, 0.0, 0.0))).abs() < 0.05);

    let spline = |points: Vec<Vec3>, start_tangent, periodic| CurveSegment::Spline {
        points,
        start_tangent,
        end_tangent: None,
        periodic,
    };
    let line = |a: Vec3, b: Vec3| CurveSegment::Line { start: a, end: b };
    let origin = Vec3::new(0.0, 0.0, 0.0);
    for (segments, closed) in [
        (vec![spline(vec![origin], None, false)], false),
        (
            vec![spline(
                vec![origin, origin, Vec3::new(1.0, 0.0, 0.0)],
                None,
                false,
            )],
            false,
        ),
        (vec![spline(points.clone(), Some(origin), false)], false),
        (vec![spline(points[..2].to_vec(), None, true)], true),
        (
            vec![
                spline(points.clone(), None, false),
                line(Vec3::new(20.0, 0.0, 0.0), origin),
            ],
            false,
        ),
        (vec![spline(points.clone(), None, false)], true),
    ] {
        assert!(session.create_curve_wire(&segments, closed).is_err());
    }
}

#[test]
fn explicit_rational_bspline_preserves_quarter_circle_and_internal_knots() {
    let session = Session::new().unwrap();
    let wire = session
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: false,
                poles: vec![
                    Vec3::new(10., 0., 0.),
                    Vec3::new(10., 10., 0.),
                    Vec3::new(0., 10., 0.),
                ],
                degree: 2,
                knots: vec![0., 1.],
                multiplicities: vec![3, 3],
                weights: vec![1., 0.5_f64.sqrt(), 1.],
            }],
            false,
        )
        .unwrap();
    let p = 10. / 2_f64.sqrt();
    let near = session
        .curve_closest_point(&wire, Vec3::new(p, p, 0.))
        .unwrap();
    assert!((near.x - p).hypot(near.y - p) < 1e-7);
    let edge = session
        .subshapes(&wire, ShapeType::Edge)
        .unwrap()
        .pop()
        .unwrap();
    let measured = session.edge_length(&edge).unwrap();
    assert!(
        (measured - 5. * std::f64::consts::PI).abs() < 1e-5,
        "length {measured}"
    );
    // BRepGProp integrates B-spline length numerically; native samples and
    // rational poles verify the exact circular geometry independently.
    for p in session.edge_sample_points(&edge, 17).unwrap() {
        assert!((p.x.hypot(p.y) - 10.).abs() < 1e-8);
    }
    let spans = session.edge_bezier_spans(&edge, 10).unwrap();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].poles.len(), 3);
    assert!((spans[0].weights[1] - 0.5_f64.sqrt()).abs() < 1e-12);
    let wire = session
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: false,
                poles: (0..7)
                    .map(|i| Vec3::new(i as f64, (i % 2) as f64, 0.))
                    .collect(),
                degree: 3,
                knots: vec![0., 0.4, 1.],
                multiplicities: vec![4, 3, 4],
                weights: vec![],
            }],
            false,
        )
        .unwrap();
    let edge = session
        .subshapes(&wire, ShapeType::Edge)
        .unwrap()
        .pop()
        .unwrap();
    let spans = session.edge_bezier_spans(&edge, 20).unwrap();
    assert_eq!(spans.len(), 2);
    assert!(spans.iter().all(|s| s.poles.len() == 4));
}

#[test]
fn explicit_bspline_validation_is_bounded_and_failure_releases_handles() {
    let session = Session::new().unwrap();
    let valid = CurveSegment::BSpline {
        non_clamped: false,
        periodic: false,
        poles: vec![
            Vec3::new(0., 0., 0.),
            Vec3::new(1., 2., 0.),
            Vec3::new(2., 0., 0.),
        ],
        degree: 2,
        knots: vec![0., 1.],
        multiplicities: vec![3, 3],
        weights: vec![1.; 3],
    };
    for case in 0..8 {
        let mut segment = valid.clone();
        let CurveSegment::BSpline {
            degree,
            knots,
            multiplicities,
            weights,
            ..
        } = &mut segment
        else {
            unreachable!()
        };
        match case {
            0 => *degree = 0,
            1 => *degree = 26,
            2 => knots[1] = 0.,
            3 => knots[1] = f64::NAN,
            4 => multiplicities[0] = 2,
            5 => multiplicities.pop().map(|_| ()).unwrap(),
            6 => weights[1] = 0.,
            _ => weights.pop().map(|_| ()).unwrap(),
        }
        assert!(session.create_curve_wire(&[segment], false).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn periodic_explicit_bspline_closes_at_native_seam_and_preserves_tangency() {
    let session = Session::new().unwrap();
    for weight in [1., 2.] {
        let wire = session
            .create_curve_wire(
                &[CurveSegment::BSpline {
                    non_clamped: false,
                    periodic: true,
                    poles: vec![
                        Vec3::new(10., 10., 0.),
                        Vec3::new(-10., 10., 0.),
                        Vec3::new(-10., -10., 0.),
                        Vec3::new(10., -10., 0.),
                    ],
                    degree: 3,
                    knots: vec![0., 1., 2., 3., 4.],
                    multiplicities: vec![1; 5],
                    weights: vec![weight, 1., 1., 1.],
                }],
                true,
            )
            .unwrap();
        assert!(session.wire_is_closed(&wire).unwrap());
        let face = session.create_face_from_wire(&wire).unwrap();
        assert!(session.is_valid(&face).unwrap());
        if weight == 1. {
            assert!((session.surface_area(&face).unwrap() - 122. / 45. * 100.).abs() < 1e-5);
        }
        let edge = session
            .subshapes(&wire, ShapeType::Edge)
            .unwrap()
            .pop()
            .unwrap();
        let spans = session.edge_bezier_spans(&edge, 50).unwrap();
        assert_eq!(spans.len(), 4);
        let first = &spans[0];
        let last = spans.last().unwrap();
        let n = last.poles.len() - 1;
        assert!(
            (first.poles[0].x - last.poles[n].x).hypot(first.poles[0].y - last.poles[n].y) < 1e-7
        );
        assert!((first.poles[0].x - 10.).hypot(first.poles[0].y - 10.) > 1.); // Seam is not a control pole.
        for axis in 0..2 {
            let coord = |p: Vec3| if axis == 0 { p.x } else { p.y };
            let a = 3. * first.weights[1] / first.weights[0]
                * (coord(first.poles[1]) - coord(first.poles[0]));
            let b = 3. * last.weights[n - 1] / last.weights[n]
                * (coord(last.poles[n]) - coord(last.poles[n - 1]));
            assert!((a - b).abs() < 1e-7, "seam derivatives {a} {b}");
        }
    }
}

#[test]
fn periodic_explicit_bspline_rejects_bad_multiplicities_open_and_mixed_wires_without_leaks() {
    let session = Session::new().unwrap();
    let valid = CurveSegment::BSpline {
        non_clamped: false,
        periodic: true,
        poles: vec![
            Vec3::new(1., 1., 0.),
            Vec3::new(-1., 1., 0.),
            Vec3::new(-1., -1., 0.),
            Vec3::new(1., -1., 0.),
        ],
        degree: 3,
        knots: vec![0., 1., 2., 3., 4.],
        multiplicities: vec![1; 5],
        weights: vec![],
    };
    assert!(
        session
            .create_curve_wire(std::slice::from_ref(&valid), false)
            .is_err()
    );
    let line = CurveSegment::Line {
        start: Vec3::new(1., 1., 0.),
        end: Vec3::new(-1., 1., 0.),
    };
    assert!(
        session
            .create_curve_wire(&[valid.clone(), line], true)
            .is_err()
    );
    for case in 0..3 {
        let mut s = valid.clone();
        let CurveSegment::BSpline { multiplicities, .. } = &mut s else {
            unreachable!()
        };
        match case {
            0 => multiplicities[4] = 2,
            1 => multiplicities[1] = 2,
            _ => multiplicities[0] = 4,
        }
        assert!(session.create_curve_wire(&[s], true).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn non_clamped_bspline_mixes_with_chord_at_actual_endpoints_and_queries_native_derivatives() {
    let session = Session::new().unwrap();
    let curve = CurveSegment::BSpline {
        non_clamped: true,
        periodic: false,
        poles: vec![
            Vec3::new(0., 0., 0.),
            Vec3::new(10., 10., 0.),
            Vec3::new(20., 0., 0.),
        ],
        degree: 2,
        knots: vec![0., 1., 2., 3., 4., 5.],
        multiplicities: vec![1; 6],
        weights: vec![],
    };
    let wire = session
        .create_curve_wire(std::slice::from_ref(&curve), false)
        .unwrap();
    let (start, end, u, v) = session.curve_end_data(&wire).unwrap();
    assert!((start.x - 5.).abs() < 1e-7 && (start.y - 5.).abs() < 1e-7);
    assert!((end.x - 15.).abs() < 1e-7 && (end.y - 5.).abs() < 1e-7);
    assert!((u.x - 10.).abs() < 1e-7 && (u.y - 10.).abs() < 1e-7);
    assert!((v.x - 10.).abs() < 1e-7 && (v.y + 10.).abs() < 1e-7);
    let shifted = session.translate(&wire, Vec3::new(2., 3., 4.)).unwrap();
    let (a, _, a_tangent, _) = session.curve_end_data(&shifted).unwrap();
    assert!((a.x - 7.).abs() < 1e-7 && (a.y - 8.).abs() < 1e-7 && (a.z - 4.).abs() < 1e-7);
    assert_eq!(a_tangent, u);
    let closed = session
        .create_curve_wire(
            &[
                curve.clone(),
                CurveSegment::Line {
                    start: end,
                    end: start,
                },
            ],
            true,
        )
        .unwrap();
    assert!(session.wire_is_closed(&closed).unwrap());
    let face = session.create_face_from_wire(&closed).unwrap();
    assert!((session.surface_area(&face).unwrap() - 100. / 6.).abs() < 1e-5);
    assert!(session.curve_end_data(&closed).is_err()); // Multiple edges are not a single curve query.
    assert!(
        session
            .create_curve_wire(
                &[
                    curve,
                    CurveSegment::Line {
                        start: Vec3::new(20., 0., 0.),
                        end: Vec3::new(0., 0., 0.)
                    }
                ],
                true
            )
            .is_err()
    );
}

#[test]
fn non_clamped_bspline_rejects_conflicting_modes_and_bad_knot_cardinality_without_leaks() {
    let session = Session::new().unwrap();
    let valid = CurveSegment::BSpline {
        non_clamped: true,
        periodic: false,
        poles: vec![
            Vec3::new(0., 0., 0.),
            Vec3::new(10., 10., 0.),
            Vec3::new(20., 0., 0.),
        ],
        degree: 2,
        knots: vec![0., 1., 2., 3., 4., 5.],
        multiplicities: vec![1; 6],
        weights: vec![],
    };
    for case in 0..3 {
        let mut c = valid.clone();
        let CurveSegment::BSpline {
            periodic,
            multiplicities,
            non_clamped,
            ..
        } = &mut c
        else {
            unreachable!()
        };
        match case {
            0 => *periodic = true,
            1 => multiplicities[2] = 2,
            _ => *non_clamped = false,
        }
        assert!(session.create_curve_wire(&[c], false).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn curve_contact_data_tracks_native_interior_derivatives_bounds_and_locations() {
    let session = Session::new().unwrap();
    let segment = CurveSegment::BSpline {
        non_clamped: false,
        periodic: false,
        poles: vec![
            Vec3::new(-10., 0., 0.),
            Vec3::new(0., 10., 0.),
            Vec3::new(10., 0., 0.),
        ],
        degree: 2,
        knots: vec![0., 1.],
        multiplicities: vec![3, 3],
        weights: vec![],
    };
    let wire = session
        .create_curve_wire(std::slice::from_ref(&segment), false)
        .unwrap();
    let (p, d) = session
        .curve_closest_data(&wire, Vec3::new(0., 6., 0.))
        .unwrap();
    assert!(p.x.abs() < 1e-9 && (p.y - 5.).abs() < 1e-9);
    assert!((d.x - 20.).abs() < 1e-9 && d.y.abs() < 1e-9);
    let (p, d) = session
        .curve_closest_data(&wire, Vec3::new(-30., -30., 0.))
        .unwrap();
    assert!((p.x + 10.).abs() < 1e-9 && p.y.abs() < 1e-9 && (d.y - 20.).abs() < 1e-9);
    let moved = session.translate(&wire, Vec3::new(2., 3., 4.)).unwrap();
    let (p, d) = session
        .curve_closest_data(&moved, Vec3::new(2., 9., 4.))
        .unwrap();
    assert!((p.x - 2.).abs() < 1e-9 && (p.y - 8.).abs() < 1e-9 && (p.z - 4.).abs() < 1e-9);
    assert!((d.x - 20.).abs() < 1e-9 && d.y.abs() < 1e-9);
    let edge = session
        .subshapes(&wire, ShapeType::Edge)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        session
            .curve_closest_data(&edge, Vec3::new(0., 6., 0.))
            .unwrap()
            .0,
        Vec3::new(0., 5., 0.)
    );
    let mut stretched = segment;
    let CurveSegment::BSpline { knots, .. } = &mut stretched else {
        unreachable!()
    };
    knots[1] = 1e12;
    let rescaled = session.create_curve_wire(&[stretched], false).unwrap();
    let (p, d) = session
        .curve_closest_data(&rescaled, Vec3::new(0., 6., 0.))
        .unwrap();
    assert!((p.y - 5.).abs() < 1e-9 && (d.x / 2e-11 - 1.).abs() < 1e-9);
}

#[test]
fn curve_contact_data_rejects_c0_knots_multiple_edges_and_bad_handles_without_leaks() {
    let session = Session::new().unwrap();
    let wire = session
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: false,
                degree: 2,
                knots: vec![0., 1., 2.],
                multiplicities: vec![3, 2, 3],
                weights: vec![],
                poles: vec![
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 1., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(1., 2., 0.),
                    Vec3::new(2., 2., 0.),
                ],
            }],
            false,
        )
        .unwrap();
    assert!(
        session
            .curve_closest_data(&wire, Vec3::new(1., 1., 0.))
            .is_err()
    );
    assert!(
        session
            .curve_closest_data(&wire, Vec3::new(0.25, 0.75, 0.))
            .is_ok()
    ); // A smooth span on a globally C0 curve.
    let box_shape = session
        .create_box(Vec3::new(0., 0., 0.), Vec3::new(1., 1., 1.))
        .unwrap();
    assert!(
        session
            .curve_closest_data(&box_shape, Vec3::new(0., 0., 0.))
            .is_err()
    );
    let multi = session
        .create_polyline_wire(
            &[
                Vec3::new(0., 0., 0.),
                Vec3::new(1., 0., 0.),
                Vec3::new(1., 1., 0.),
            ],
            false,
        )
        .unwrap();
    assert!(
        session
            .curve_closest_data(&multi, Vec3::new(0., 0., 0.))
            .is_err()
    );
    assert!(
        session
            .curve_closest_data(&wire, Vec3::new(f64::NAN, 0., 0.))
            .is_err()
    );
    let other = Session::new().unwrap();
    assert!(
        other
            .curve_closest_data(&wire, Vec3::new(0., 0., 0.))
            .is_err()
    );
    let zero = session
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: false,
                degree: 3,
                knots: vec![0., 1.],
                multiplicities: vec![4, 4],
                weights: vec![],
                poles: vec![
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(2., 2., 0.),
                ],
            }],
            false,
        )
        .unwrap();
    assert!(
        session
            .curve_closest_data(&zero, Vec3::new(-1., -1., 0.))
            .is_err()
    );
    let periodic = session
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: true,
                degree: 1,
                knots: vec![0., 1., 2., 3.],
                multiplicities: vec![1; 4],
                weights: vec![],
                poles: vec![
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(0., 1., 0.),
                ],
            }],
            true,
        )
        .unwrap();
    let seam = session.curve_end_data(&periodic).unwrap().0;
    assert!(session.curve_closest_data(&periodic, seam).is_err());
    drop((wire, box_shape, multi, zero, periodic));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn elliptical_arc_segments_are_exact_in_either_sense_and_axis_order() {
    let session = Session::new().unwrap();
    let v = |x: f64, y: f64| Vec3::new(x, y, 0.0);
    // Half ellipse (a = 10, b = 4) above the major axis, closed by a chord:
    // counterclockwise from (10, 0) to (−10, 0) about +z.
    for (major, minor) in [(v(10., 0.), v(0., 4.)), (v(0., 4.), v(10., 0.))] {
        // Swapping the vertices flips the normal, so the same half needs
        // the clockwise flag.
        let clockwise = major.x == 0.0;
        let wire = session
            .create_curve_wire(
                &[
                    CurveSegment::EllipseArc {
                        center: v(0., 0.),
                        major,
                        minor,
                        start: v(10., 0.),
                        end: v(-10., 0.),
                        clockwise,
                    },
                    CurveSegment::Line {
                        start: v(-10., 0.),
                        end: v(10., 0.),
                    },
                ],
                true,
            )
            .unwrap();
        let face = session.create_face_from_wire(&wire).unwrap();
        let area = session.surface_area(&face).unwrap();
        assert!((area - std::f64::consts::PI * 20.).abs() < 1e-9, "{area}");
        let bounds = session.exact_bounds(&face).unwrap();
        assert!((bounds.max.y - 4.).abs() < 1e-9 && bounds.min.y.abs() < 1e-9);
        let edge = session.subshape(&wire, ShapeType::Edge, 0).unwrap();
        let half_perimeter = session.edge_length(&edge).unwrap();
        assert!(
            (half_perimeter - 23.013_112_595_664_847).abs() < 1e-9,
            "{half_perimeter}"
        );
    }
    // Rejected inputs fail without storing shapes.
    let count = session.shape_count().unwrap();
    for (minor, start, message) in [
        (v(1., 4.), v(10., 0.), "not perpendicular"),
        (v(0., 4.), v(10., 1.), "not on the ellipse"),
        (v(0., 0.), v(10., 0.), "positive semi-axes"),
    ] {
        let error = session
            .create_curve_wire(
                &[CurveSegment::EllipseArc {
                    center: v(0., 0.),
                    major: v(10., 0.),
                    minor,
                    start,
                    end: v(-10., 0.),
                    clockwise: false,
                }],
                false,
            )
            .unwrap_err();
        assert!(error.message.contains(message), "{}", error.message);
    }
    assert_eq!(session.shape_count().unwrap(), count);
}

#[test]
fn curve_parameter_fraction_is_exact_located_bounded_and_single_edge() {
    let s = Session::new().unwrap();
    let wire = s
        .create_curve_wire(
            &[CurveSegment::Spline {
                points: vec![
                    Vec3::new(0., 0., 0.),
                    Vec3::new(2., 10., 0.),
                    Vec3::new(10., 0., 0.),
                ],
                start_tangent: None,
                end_tangent: None,
                periodic: false,
            }],
            false,
        )
        .unwrap();
    let f = (104_f64).sqrt() / ((104_f64).sqrt() + (164_f64).sqrt());
    let p = s.curve_point_at_fraction(&wire, f).unwrap();
    assert!((p.x - 2.).abs() < 1e-9 && (p.y - 10.).abs() < 1e-9);
    let (a, b, _, _) = s.curve_end_data(&wire).unwrap();
    assert_eq!(s.curve_point_at_fraction(&wire, 0.).unwrap(), a);
    assert_eq!(s.curve_point_at_fraction(&wire, 1.).unwrap(), b);
    for f in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        assert!(s.curve_point_at_fraction(&wire, f).is_err());
    }
    let moved = s.translate(&wire, Vec3::new(3., -2., 7.)).unwrap();
    let q = s.curve_point_at_fraction(&moved, f).unwrap();
    assert!((q.x - 5.).abs() < 1e-9 && (q.y - 8.).abs() < 1e-9 && (q.z - 7.).abs() < 1e-9);
    let rectangle = s
        .create_curve_wire(
            &[
                CurveSegment::Line {
                    start: Vec3::new(0., 0., 0.),
                    end: Vec3::new(1., 0., 0.),
                },
                CurveSegment::Line {
                    start: Vec3::new(1., 0., 0.),
                    end: Vec3::new(1., 1., 0.),
                },
            ],
            false,
        )
        .unwrap();
    assert!(s.curve_point_at_fraction(&rectangle, 0.5).is_err());
}

#[test]
fn closest_parameter_fractions_are_exact_located_and_oriented() {
    let session = Session::new().unwrap();
    let curve = session
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: false,
                degree: 2,
                knots: vec![3., 7.],
                multiplicities: vec![3, 3],
                weights: vec![1., 0.5_f64.sqrt(), 1.],
                poles: vec![
                    Vec3::new(10., 0., 0.),
                    Vec3::new(10., 10., 0.),
                    Vec3::new(0., 10., 0.),
                ],
            }],
            false,
        )
        .unwrap();
    for fraction in [0., 0.13, 0.5, 0.81, 1.] {
        let expected = session.curve_point_at_fraction(&curve, fraction).unwrap();
        let (f, p) = session.curve_closest_fraction(&curve, expected).unwrap();
        assert!((f - fraction).abs() < 1e-8);
        assert!((p.x - expected.x).hypot(p.y - expected.y) < 1e-7);
    }
    let moved = session.translate(&curve, Vec3::new(3., -2., 7.)).unwrap();
    let query = session.curve_point_at_fraction(&moved, 0.29).unwrap();
    let (fraction, point) = session.curve_closest_fraction(&moved, query).unwrap();
    assert!((fraction - 0.29).abs() < 1e-8 && (point.z - 7.).abs() < 1e-8);
    assert!(
        session
            .curve_closest_fraction(&curve, Vec3::new(f64::NAN, 0., 0.))
            .is_err()
    );
}
#[test]
fn closest_parameter_rejects_multiple_branches_and_allows_c0_knots() {
    let session = Session::new().unwrap();
    let curve = session
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: false,
                degree: 1,
                knots: vec![0., 1., 2., 3.],
                multiplicities: vec![2, 1, 1, 2],
                weights: vec![],
                poles: vec![
                    Vec3::new(-1., -1., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(-1., 1., 0.),
                    Vec3::new(1., -1., 0.),
                ],
            }],
            false,
        )
        .unwrap();
    assert!(
        session
            .curve_closest_fraction(&curve, Vec3::new(0., 0., 0.))
            .is_err()
    );
    assert!(
        session
            .curve_closest_fraction(&curve, Vec3::new(-0.8, -0.75, 0.))
            .is_ok()
    );
    let corner = session.curve_point_at_fraction(&curve, 1. / 3.).unwrap();
    assert!((session.curve_closest_fraction(&curve, corner).unwrap().0 - 1. / 3.).abs() < 1e-8);
}
#[test]
fn closest_parameter_canonicalizes_seams_and_rejects_nonunique_circle_center() {
    let session = Session::new().unwrap();
    let curve = session
        .create_curve_wire(
            &[CurveSegment::BSpline {
                non_clamped: false,
                periodic: true,
                degree: 1,
                knots: vec![0., 1., 2., 3.],
                multiplicities: vec![1; 4],
                weights: vec![],
                poles: vec![
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(0., 1., 0.),
                ],
            }],
            true,
        )
        .unwrap();
    let seam = session.curve_point_at_fraction(&curve, 1.).unwrap();
    assert_eq!(session.curve_closest_fraction(&curve, seam).unwrap().0, 0.);
    let circle = session
        .create_circle_wire(Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 10.)
        .unwrap();
    assert!(
        session
            .curve_closest_fraction(&circle, Vec3::new(0., 0., 0.))
            .is_err()
    );
    let multi = session
        .create_curve_wire(
            &[
                CurveSegment::Line {
                    start: Vec3::new(0., 0., 0.),
                    end: Vec3::new(1., 0., 0.),
                },
                CurveSegment::Line {
                    start: Vec3::new(1., 0., 0.),
                    end: Vec3::new(1., 1., 0.),
                },
            ],
            false,
        )
        .unwrap();
    assert!(
        session
            .curve_closest_fraction(&multi, Vec3::new(0., 0., 0.))
            .is_err()
    );
    drop((curve, circle, multi));
    assert_eq!(session.shape_count().unwrap(), 0);
}
