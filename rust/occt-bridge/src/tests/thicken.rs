use super::*;
use std::f64::consts::PI;

fn plate(session: &Session) -> Shape<'_> {
    let wire = session
        .create_polyline_wire(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(40.0, 0.0, 0.0),
                Vec3::new(40.0, 20.0, 0.0),
                Vec3::new(0.0, 20.0, 0.0),
            ],
            true,
        )
        .unwrap();
    session.create_face_from_wire(&wire).unwrap()
}

/// Sweeps an open profile 25 mm (L) or 30 mm (arc) along +y into a sheet.
fn sheet<'a>(session: &'a Session, profile: &Shape<'_>, length: f64) -> Shape<'a> {
    let path = session
        .create_polyline_wire(
            &[Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, length, 0.0)],
            false,
        )
        .unwrap();
    session
        .sweep(profile, &path, SweepOrientation::CorrectedFrenet)
        .unwrap()
}

fn solid_volume(session: &Session, shape: &Shape<'_>) -> f64 {
    assert!(session.is_valid(shape).unwrap());
    assert_eq!(session.subshape_count(shape, ShapeType::Solid).unwrap(), 1);
    session.volume(shape).unwrap()
}

#[test]
fn planar_sheets_thicken_along_reverse_or_across_their_normal() {
    let session = Session::new().unwrap();
    let face = plate(&session);
    let up = session.face_normal(&face).unwrap().z;
    assert!((up.abs() - 1.0).abs() < 1e-12);
    for (thickness, symmetric, low, high) in [
        (2.0, false, 0.0f64, 2.0 * up),
        (-2.0, false, 0.0, -2.0 * up),
        (2.0, true, -1.0, 1.0),
    ] {
        let solid = session.thicken(&face, thickness, symmetric, 1e-6).unwrap();
        assert!((solid_volume(&session, &solid) - 1600.0).abs() < 1e-7);
        let bounds = session.exact_bounds(&solid).unwrap();
        let (low, high) = (low.min(high), low.max(high));
        assert!(
            (bounds.min.z - low).abs() < 1e-7 && (bounds.max.z - high).abs() < 1e-7,
            "{thickness} {symmetric}: {bounds:?}"
        );
    }
}

#[test]
fn sharp_and_curved_sheets_thicken_exactly() {
    let session = Session::new().unwrap();
    // L sheet: 30 x 25 floor and 20 x 25 wall meeting at a sharp edge. The
    // mitred corner adds or removes a t x t x 25 block.
    let l = session
        .create_polyline_wire(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(30.0, 0.0, 0.0),
                Vec3::new(30.0, 0.0, 20.0),
            ],
            false,
        )
        .unwrap();
    let l = sheet(&session, &l, 25.0);
    let mut volumes = [2.0, -2.0].map(|t| {
        let solid = session.thicken(&l, t, false, 1e-6).unwrap();
        assert_eq!(session.subshape_count(&solid, ShapeType::Face).unwrap(), 10);
        solid_volume(&session, &solid)
    });
    volumes.sort_by(f64::total_cmp);
    assert!((volumes[0] - 2400.0).abs() < 1e-6 && (volumes[1] - 2600.0).abs() < 1e-6);
    let both = session.thicken(&l, 4.0, true, 1e-6).unwrap();
    assert!((solid_volume(&session, &both) - 5000.0).abs() < 1e-6);
    assert_eq!(session.subshape_count(&both, ShapeType::Face).unwrap(), 10);
    // Half-cylinder sheet of radius 20, 30 long.
    let arc = session
        .create_curve_wire(
            &[CurveSegment::Arc {
                start: Vec3::new(-20.0, 0.0, 0.0),
                middle: Vec3::new(0.0, 0.0, 20.0),
                end: Vec3::new(20.0, 0.0, 0.0),
            }],
            false,
        )
        .unwrap();
    let arc = sheet(&session, &arc, 30.0);
    let ring = |outer: f64, inner: f64| PI / 2.0 * (outer * outer - inner * inner) * 30.0;
    let mut volumes = [2.0, -2.0].map(|t| {
        let solid = session.thicken(&arc, t, false, 1e-6).unwrap();
        solid_volume(&session, &solid)
    });
    volumes.sort_by(f64::total_cmp);
    assert!((volumes[0] - ring(20.0, 18.0)).abs() < 1e-6, "{volumes:?}");
    assert!((volumes[1] - ring(22.0, 20.0)).abs() < 1e-6, "{volumes:?}");
    let both = session.thicken(&arc, 4.0, true, 1e-6).unwrap();
    assert!((solid_volume(&session, &both) - ring(22.0, 18.0)).abs() < 1e-6);
    let bounds = session.exact_bounds(&both).unwrap();
    assert!((bounds.max.z - 22.0).abs() < 1e-7 && (bounds.max.x - 22.0).abs() < 1e-7);
}

/// Images of `source` under `relation` that are faces, with their centroid z.
fn face_levels(
    session: &Session,
    solid: &Shape<'_>,
    source: &Shape<'_>,
    relation: HistoryRelation,
) -> Vec<f64> {
    let count = session.history_count(solid, source, relation).unwrap();
    let mut levels = (0..count)
        .map(|i| session.history(solid, source, relation, i).unwrap())
        .filter(|image| session.shape_type(image).unwrap() == ShapeType::Face)
        .map(|image| {
            assert!(session.face_normal(&image).unwrap().z.abs() > 1.0 - 1e-12);
            session.center_of_mass(&image).unwrap().z
        })
        .collect::<Vec<_>>();
    levels.sort_by(f64::total_cmp);
    levels
}

#[test]
fn thickened_faces_and_edges_keep_sheet_history() {
    let session = Session::new().unwrap();
    let face = plate(&session);
    let up = session.face_normal(&face).unwrap().z;
    let edges = session.subshapes(&face, ShapeType::Edge).unwrap();
    for symmetric in [false, true] {
        let solid = session.thicken(&face, 3.0, symmetric, 1e-6).unwrap();
        let faces = session.subshapes(&solid, ShapeType::Face).unwrap();
        // The sheet face generates its offset copy; one-sided, the sheet
        // face itself stays in the solid, symmetric, its half offset does.
        let generated = face_levels(&session, &solid, &face, HistoryRelation::Generated);
        let modified = face_levels(&session, &solid, &face, HistoryRelation::Modified);
        if symmetric {
            assert_eq!((generated, modified), (vec![1.5 * up], vec![-1.5 * up]));
        } else {
            assert_eq!((generated, modified), (vec![3.0 * up], vec![]));
            assert!(faces.iter().any(|f| session.is_same(f, &face).unwrap()));
        }
        // Each boundary edge generates exactly one side wall face.
        for edge in &edges {
            let count = session
                .history_count(&solid, edge, HistoryRelation::Generated)
                .unwrap();
            let walls = (0..count)
                .map(|i| {
                    session
                        .history(&solid, edge, HistoryRelation::Generated, i)
                        .unwrap()
                })
                .filter(|image| session.shape_type(image).unwrap() == ShapeType::Face)
                .collect::<Vec<_>>();
            assert_eq!(walls.len(), 1);
            assert!(faces.iter().any(|f| session.is_same(f, &walls[0]).unwrap()));
            let area = session.surface_area(&walls[0]).unwrap();
            assert!((area - 3.0 * session.edge_length(edge).unwrap()).abs() < 1e-7);
            assert!(session.face_normal(&walls[0]).unwrap().z.abs() < 1e-12);
        }
    }
}

#[test]
fn invalid_sheets_and_thicknesses_fail_without_leaking_shapes() {
    let session = Session::new().unwrap();
    let face = plate(&session);
    let block = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
        .unwrap();
    let edge = session.subshape(&face, ShapeType::Edge, 0).unwrap();
    let count = session.shape_count().unwrap();
    for thickness in [0.0, f64::NAN, f64::INFINITY] {
        let error = session.thicken(&face, thickness, false, 1e-6).unwrap_err();
        assert_eq!(error.status, 1);
    }
    assert_eq!(
        session.thicken(&face, 1.0, false, 0.0).unwrap_err().status,
        1
    );
    for input in [&block, &edge] {
        let error = session.thicken(input, 1.0, false, 1e-6).unwrap_err();
        assert_eq!(error.status, 4, "{}", error.message);
        assert!(error.message.contains("without solids"));
    }
    assert_eq!(session.shape_count().unwrap(), count);
    // Thickening a radius-20 half cylinder 25 mm toward its axis collapses it.
    let arc = session
        .create_curve_wire(
            &[CurveSegment::Arc {
                start: Vec3::new(-20.0, 0.0, 0.0),
                middle: Vec3::new(0.0, 0.0, 20.0),
                end: Vec3::new(20.0, 0.0, 0.0),
            }],
            false,
        )
        .unwrap();
    let arc = sheet(&session, &arc, 30.0);
    let count = session.shape_count().unwrap();
    let inward = [25.0, -25.0]
        .into_iter()
        .filter(|&t| session.thicken(&arc, t, false, 1e-6).is_err())
        .count();
    assert_eq!(inward, 1, "exactly the inward 25 mm thicken must fail");
    assert_eq!(session.shape_count().unwrap(), count);
}

#[test]
fn holed_sheet_history_keeps_each_boundary_wall_across_signs_and_symmetric_modes() {
    let s = Session::new().unwrap();
    {
        let outer = s
            .create_polyline_wire(
                &[
                    Vec3::new(0., 0., 0.),
                    Vec3::new(40., 0., 0.),
                    Vec3::new(40., 20., 0.),
                    Vec3::new(0., 20., 0.),
                ],
                true,
            )
            .unwrap();
        let hole = s
            .create_circle_wire(Vec3::new(10., 10., 0.), Vec3::new(0., 0., 1.), 3.)
            .unwrap();
        let face = s.create_face_with_holes(&outer, &[&hole]).unwrap();
        let face = s
            .rotate(&face, Vec3::new(0., 0., 0.), Vec3::new(1., 0., 0.), 0.4)
            .unwrap();
        let face = s.translate(&face, Vec3::new(500., -200., 70.)).unwrap();
        let before = s.exact_bounds(&face).unwrap();
        for thickness in [-2.5_f64, 2.5] {
            for symmetric in [false, true] {
                let solid = s.thicken(&face, thickness, symmetric, 1e-6).unwrap();
                assert!(
                    (solid_volume(&s, &solid) - (800. - 9. * PI) * thickness.abs()).abs() < 1e-6
                );
                let result_faces = s.subshapes(&solid, ShapeType::Face).unwrap();
                assert_eq!(result_faces.len(), 7);
                assert_eq!(
                    s.history_count(&solid, &face, HistoryRelation::Generated)
                        .unwrap(),
                    1
                );
                assert_eq!(
                    s.history_count(&solid, &face, HistoryRelation::Modified)
                        .unwrap(),
                    usize::from(symmetric)
                );
                assert!(!s.history_is_deleted(&solid, &face).unwrap());
                for edge in s.subshapes(&face, ShapeType::Edge).unwrap() {
                    let mut walls = Vec::new();
                    for i in 0..s
                        .history_count(&solid, &edge, HistoryRelation::Generated)
                        .unwrap()
                    {
                        let image = s
                            .history(&solid, &edge, HistoryRelation::Generated, i)
                            .unwrap();
                        if s.shape_type(&image).unwrap() == ShapeType::Face {
                            walls.push(image);
                        }
                    }
                    assert_eq!(walls.len(), 1);
                    assert!(
                        result_faces
                            .iter()
                            .any(|f| s.is_same(f, &walls[0]).unwrap())
                    );
                    assert!(
                        (s.surface_area(&walls[0]).unwrap()
                            - thickness.abs() * s.edge_length(&edge).unwrap())
                        .abs()
                            < 1e-6
                    );
                    assert!(!s.history_is_deleted(&solid, &edge).unwrap());
                }
                assert_eq!(s.exact_bounds(&face).unwrap(), before);
            }
        }
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}

#[test]
fn joined_sheet_internal_seams_do_not_generate_boundary_wall_faces() {
    let s = Session::new().unwrap();
    {
        let profile = s
            .create_polyline_wire(
                &[
                    Vec3::new(0., 0., 0.),
                    Vec3::new(30., 0., 0.),
                    Vec3::new(30., 0., 20.),
                ],
                false,
            )
            .unwrap();
        let source = sheet(&s, &profile, 25.);
        let edges = s.subshapes(&source, ShapeType::Edge).unwrap();
        let hinge = edges
            .iter()
            .find(|edge| {
                let b = s.exact_bounds(edge).unwrap();
                (b.min.x - 30.).abs() < 1e-7
                    && (b.max.x - 30.).abs() < 1e-7
                    && b.min.z.abs() < 1e-7
                    && b.max.z.abs() < 1e-7
            })
            .unwrap();
        for symmetric in [false, true] {
            let solid = s.thicken(&source, 2., symmetric, 1e-6).unwrap();
            let count = s
                .history_count(&solid, hinge, HistoryRelation::Generated)
                .unwrap();
            for i in 0..count {
                let image = s
                    .history(&solid, hinge, HistoryRelation::Generated, i)
                    .unwrap();
                assert_ne!(s.shape_type(&image).unwrap(), ShapeType::Face);
            }
            assert!(!s.history_is_deleted(&solid, hinge).unwrap());
            assert_eq!(s.subshape_count(&solid, ShapeType::Face).unwrap(), 10);
        }
    }
    assert_eq!(s.shape_count().unwrap(), 0);
}
