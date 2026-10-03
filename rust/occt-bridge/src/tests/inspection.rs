//! Measurements, curvature, topology, adjacency, tangency, and history queries.

use super::*;

#[test]
fn bulk_subshapes_match_indexed_traversal_and_release_independent_handles() {
    let session = Session::new().unwrap();
    let body = unit_box(&session, 0.0);
    let repeated = session.create_compound(&[&body, &body]).unwrap();
    for kind in [ShapeType::Face, ShapeType::Edge, ShapeType::Vertex] {
        let subshapes = session.subshapes(&repeated, kind).unwrap();
        assert_eq!(
            subshapes.len(),
            session.subshape_count(&body, kind).unwrap()
        );
        for (index, shape) in subshapes.iter().enumerate() {
            let indexed = session.subshape(&body, kind, index).unwrap();
            assert!(session.is_same(shape, &indexed).unwrap());
        }
        drop(subshapes);
        assert_eq!(session.shape_count().unwrap(), 2);
    }
    let face = session.subshape(&body, ShapeType::Face, 0).unwrap();
    assert!(
        session
            .subshapes(&face, ShapeType::Solid)
            .unwrap()
            .is_empty()
    );
    let other = Session::new().unwrap();
    assert_wrong_session(other.subshapes(&body, ShapeType::Edge).unwrap_err());
    drop(face);
    drop(repeated);
    drop(body);
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn curvature_fixture_edge<'a>(session: &'a Session, index: usize) -> Shape<'a> {
    let compound = session
        .load_brep(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/curvature_edges.brep"
        ))
        .unwrap();
    session.subshape(&compound, ShapeType::Edge, index).unwrap()
}

fn assert_bounded(extrema: CurvatureExtrema, tolerance: f64) {
    assert!(!extrema.is_exact);
    assert!(extrema.minimum_lower_bound <= extrema.minimum);
    assert!(extrema.maximum <= extrema.maximum_upper_bound);
    let gap = tolerance * extrema.maximum;
    assert!(
        extrema.minimum - extrema.minimum_lower_bound <= gap,
        "{extrema:?}"
    );
    assert!(
        extrema.maximum_upper_bound - extrema.maximum <= gap,
        "{extrema:?}"
    );
}

#[test]
fn curvature_extrema_bound_polynomial_and_rational_edges() {
    let session = Session::new().unwrap();
    let tolerance = 1e-6;

    let parabola = session
        .edge_curvature_extrema(&curvature_fixture_edge(&session, 0), tolerance)
        .unwrap();
    assert_bounded(parabola, tolerance);
    // y = x^2 on [-1, 1]: vertex curvature 2 is attained at the midpoint split.
    assert!((parabola.maximum - 2.0).abs() < 1e-12);
    let end_curvature = 2.0 / 5.0_f64.powf(1.5);
    assert!((parabola.minimum - end_curvature).abs() < 1e-12);

    let circle = session
        .edge_curvature_extrema(&curvature_fixture_edge(&session, 1), tolerance)
        .unwrap();
    assert_bounded(circle, tolerance);
    assert!((circle.minimum_lower_bound - 0.5).abs() <= 0.5 * tolerance);
    assert!((circle.maximum_upper_bound - 0.5).abs() <= 0.5 * tolerance);

    let spline_edge = curvature_fixture_edge(&session, 2);
    let spline = session
        .edge_curvature_extrema(&spline_edge, tolerance)
        .unwrap();
    assert_bounded(spline, tolerance);
    let (sampled_minimum, sampled_maximum) =
        session.edge_curvature_range(&spline_edge, 100_000).unwrap();
    assert!(spline.minimum_lower_bound <= sampled_minimum + 1e-12);
    assert!(sampled_maximum <= spline.maximum_upper_bound + 1e-12);
    assert!(spline.minimum <= sampled_minimum + tolerance * spline.maximum);
    assert!(spline.maximum >= sampled_maximum - tolerance * spline.maximum);

    let arc = session
        .edge_curvature_extrema(&curvature_fixture_edge(&session, 3), tolerance)
        .unwrap();
    assert!(arc.is_exact);
    let ellipse = |t: f64| 8.0 / (16.0 * t.sin().powi(2) + 4.0 * t.cos().powi(2)).powf(1.5);
    assert!((arc.minimum - 0.125).abs() < 1e-12);
    assert!((arc.maximum - ellipse(0.3).max(ellipse(2.0))).abs() < 1e-12);

    let straight = session
        .edge_curvature_extrema(&curvature_fixture_edge(&session, 4), tolerance)
        .unwrap();
    assert!(!straight.is_exact);
    assert!(straight.maximum_upper_bound <= 1e-10);
}

#[test]
fn curvature_extrema_are_exact_for_parabola_and_hyperbola_edges() {
    let session = Session::new().unwrap();
    let assert_exact = |extrema: CurvatureExtrema, minimum: f64, maximum: f64| {
        assert!(extrema.is_exact, "{extrema:?}");
        assert_eq!(extrema.minimum, extrema.minimum_lower_bound);
        assert_eq!(extrema.maximum, extrema.maximum_upper_bound);
        assert!((extrema.minimum - minimum).abs() < 1e-12, "{extrema:?}");
        assert!((extrema.maximum - maximum).abs() < 1e-12, "{extrema:?}");
    };

    // Focal 0.5 on u in [-1, 2]: the vertex (u = 0) is interior and the
    // flattest point is the far end u = 2.
    let parabola = session
        .edge_curvature_extrema(&curvature_fixture_edge(&session, 5), 1e-6)
        .unwrap();
    assert_exact(parabola, 1.0 / 5.0_f64.powf(1.5), 1.0);

    // a = 3, b = 2 on u in [-0.5, 1]: vertex curvature a / b^2, flattest at u = 1.
    let hyperbola = session
        .edge_curvature_extrema(&curvature_fixture_edge(&session, 6), 1e-6)
        .unwrap();
    let curvature = |u: f64| 6.0 / (9.0 * u.sinh().powi(2) + 4.0 * u.cosh().powi(2)).powf(1.5);
    assert_exact(hyperbola, curvature(1.0), 0.75);
}

#[test]
fn exact_bounds_follow_geometry_without_tolerance_padding() {
    let session = Session::new().unwrap();
    let box_shape = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(120.0, 20.0, 30.0))
        .unwrap();
    let padded = session.bounds(&box_shape).unwrap();
    let exact = session.exact_bounds(&box_shape).unwrap();
    assert!(padded.max.x - padded.min.x > 120.0);
    assert!((exact.max.x - exact.min.x - 120.0).abs() < 1e-12);
    assert!((exact.max.z - exact.min.z - 30.0).abs() < 1e-12);

    let cylinder = session
        .create_cylinder(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 2.0, 5.0)
        .unwrap();
    let exact = session.exact_bounds(&cylinder).unwrap();
    assert!((exact.max.z - exact.min.z - 5.0).abs() < 1e-9);
    assert!((exact.max.x - exact.min.x - 4.0).abs() < 1e-9);
}

#[test]
fn box_face_and_edge_adjacency_follows_shared_topology() {
    let session = Session::new().unwrap();
    let box_shape = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 3.0))
        .unwrap();
    let subshapes = |kind| {
        (0..session.subshape_count(&box_shape, kind).unwrap())
            .map(|index| session.subshape(&box_shape, kind, index).unwrap())
            .collect::<Vec<_>>()
    };
    let adjacent_pairs = |shapes: &[Shape<'_>]| {
        let mut count = 0;
        for (index, first) in shapes.iter().enumerate() {
            assert!(!session.is_adjacent(&box_shape, first, first).unwrap());
            for second in &shapes[index + 1..] {
                let forward = session.is_adjacent(&box_shape, first, second).unwrap();
                assert_eq!(
                    forward,
                    session.is_adjacent(&box_shape, second, first).unwrap()
                );
                count += usize::from(forward);
            }
        }
        count
    };

    // Each of the 12 edges joins exactly two faces; only the 3 opposite
    // face pairs share nothing.
    let faces = subshapes(ShapeType::Face);
    assert_eq!(faces.len(), 6);
    assert_eq!(adjacent_pairs(&faces), 12);

    // Three edges meet at each of the 8 corners: 8 * C(3, 2) pairs.
    let edges = subshapes(ShapeType::Edge);
    assert_eq!(edges.len(), 12);
    assert_eq!(adjacent_pairs(&edges), 24);

    let vertex = session.subshape(&box_shape, ShapeType::Vertex, 0).unwrap();
    assert_eq!(
        session
            .is_adjacent(&box_shape, &faces[0], &vertex)
            .unwrap_err()
            .status,
        1
    );
}

#[test]
fn traverses_topology_and_measures_solid_geometry() {
    let session = Session::new().unwrap();
    let shape = session
        .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();

    assert_eq!(session.shape_type(&shape).unwrap(), ShapeType::Solid);
    assert_eq!(session.subshape_count(&shape, ShapeType::Face).unwrap(), 6);
    assert_eq!(session.subshape_count(&shape, ShapeType::Edge).unwrap(), 12);
    assert_eq!(
        session.subshape_count(&shape, ShapeType::Vertex).unwrap(),
        8
    );

    let face = session.subshape(&shape, ShapeType::Face, 0).unwrap();
    assert_eq!(session.shape_type(&face).unwrap(), ShapeType::Face);
    assert!(session.surface_area(&face).unwrap() > 0.0);
    assert!((session.surface_area(&shape).unwrap() - 2200.0).abs() < 1e-9);
    assert!((session.volume(&shape).unwrap() - 6000.0).abs() < 1e-9);
    let center = session.center_of_mass(&shape).unwrap();
    assert!((center.x - 6.0).abs() < 1e-9);
    assert!((center.y - 12.0).abs() < 1e-9);
    assert!((center.z - 18.0).abs() < 1e-9);
    assert_eq!(
        session
            .subshape(&shape, ShapeType::Face, 6)
            .unwrap_err()
            .status,
        1
    );
    assert_eq!(session.shape_count().unwrap(), 2);
}

#[test]
fn reports_oriented_face_normals_and_topology_adjacency() {
    let session = Session::new().unwrap();
    let shape = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let faces = (0..session.subshape_count(&shape, ShapeType::Face).unwrap())
        .map(|index| session.subshape(&shape, ShapeType::Face, index).unwrap())
        .collect::<Vec<_>>();
    let edges = (0..session.subshape_count(&shape, ShapeType::Edge).unwrap())
        .map(|index| session.subshape(&shape, ShapeType::Edge, index).unwrap())
        .collect::<Vec<_>>();
    let top = faces
        .iter()
        .find(|face| session.face_normal(face).unwrap().z > 0.999)
        .unwrap();

    let normal = session.face_normal(top).unwrap();
    assert!(normal.x.abs() < 1e-9);
    assert!(normal.y.abs() < 1e-9);
    assert!((normal.z - 1.0).abs() < 1e-9);
    assert_eq!(
        edges
            .iter()
            .filter(|edge| session.is_adjacent(&shape, top, edge).unwrap())
            .count(),
        4
    );
    assert_eq!(session.face_normal(&shape).unwrap_err().status, 4);
    assert!(session.is_same(top, top).unwrap());
    assert!(!session.is_same(&faces[0], &faces[1]).unwrap());
    assert_eq!(session.shape_count().unwrap(), 19);
}

#[test]
fn reports_planarity_edge_length_and_circular_radius() {
    let session = Session::new().unwrap();
    let box_shape = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let face = session.subshape(&box_shape, ShapeType::Face, 0).unwrap();
    let edge = session.subshape(&box_shape, ShapeType::Edge, 0).unwrap();
    assert!(session.face_is_planar(&face).unwrap());
    assert!(session.edge_length(&edge).unwrap() > 0.0);
    assert_eq!(session.edge_circle_radius(&edge).unwrap(), None);
    assert_eq!(session.edge_curvature(&edge).unwrap(), Some(0.0));

    let cylinder = session
        .create_cylinder(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 2.0, 5.0)
        .unwrap();
    let radii = (0..session.subshape_count(&cylinder, ShapeType::Edge).unwrap())
        .filter_map(|index| {
            let edge = session.subshape(&cylinder, ShapeType::Edge, index).unwrap();
            session.edge_circle_radius(&edge).unwrap()
        })
        .collect::<Vec<_>>();
    assert!(radii.len() >= 2);
    assert!(radii.iter().all(|radius| (*radius - 2.0).abs() < 1e-9));
    let circular_edge = (0..session.subshape_count(&cylinder, ShapeType::Edge).unwrap())
        .map(|index| session.subshape(&cylinder, ShapeType::Edge, index).unwrap())
        .find(|edge| session.edge_circle_radius(edge).unwrap().is_some())
        .unwrap();
    assert!((session.edge_curvature(&circular_edge).unwrap().unwrap() - 0.5).abs() < 1e-9);

    let ellipse = session
        .create_ellipse_wire(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 4.0, 2.0)
        .unwrap();
    let ellipse_edge = session.subshape(&ellipse, ShapeType::Edge, 0).unwrap();
    let (minimum, maximum) = session.edge_curvature_range(&ellipse_edge, 5).unwrap();
    assert!((minimum - 0.125).abs() < 1e-9);
    assert!((maximum - 1.0).abs() < 1e-9);
    assert_eq!(
        session
            .edge_curvature_range(&ellipse_edge, 1)
            .unwrap_err()
            .status,
        1
    );

    let exact = session.edge_curvature_extrema(&ellipse_edge, 1e-6).unwrap();
    assert!(exact.is_exact);
    assert_eq!(exact.minimum, exact.minimum_lower_bound);
    assert_eq!(exact.maximum, exact.maximum_upper_bound);
    assert!((exact.minimum - 0.125).abs() < 1e-12);
    assert!((exact.maximum - 1.0).abs() < 1e-12);
    let circle = session
        .edge_curvature_extrema(&circular_edge, 1e-6)
        .unwrap();
    assert!(circle.is_exact && (circle.maximum - 0.5).abs() < 1e-12);
    for tolerance in [0.0, -1.0, 1.5, f64::NAN] {
        assert_eq!(
            session
                .edge_curvature_extrema(&ellipse_edge, tolerance)
                .unwrap_err()
                .status,
            1
        );
    }
    assert_eq!(
        session
            .edge_curvature_extrema(&box_shape, 1e-6)
            .unwrap_err()
            .status,
        4
    );

    assert_eq!(session.face_is_planar(&box_shape).unwrap_err().status, 4);
    assert_eq!(session.edge_length(&box_shape).unwrap_err().status, 4);
    assert_eq!(
        session.edge_circle_radius(&box_shape).unwrap_err().status,
        4
    );
    assert_eq!(session.edge_curvature(&box_shape).unwrap_err().status, 4);
    assert_eq!(
        session
            .edge_curvature_range(&box_shape, 5)
            .unwrap_err()
            .status,
        4
    );
}

#[test]
fn reports_recorded_face_tangency_on_fillets() {
    let session = Session::new().unwrap();
    let box_shape = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let box_faces = (0..session.subshape_count(&box_shape, ShapeType::Face).unwrap())
        .map(|index| {
            session
                .subshape(&box_shape, ShapeType::Face, index)
                .unwrap()
        })
        .collect::<Vec<_>>();
    for first in 0..box_faces.len() {
        for second in first + 1..box_faces.len() {
            assert!(
                !session
                    .faces_are_tangent(&box_shape, &box_faces[first], &box_faces[second])
                    .unwrap()
            );
        }
    }

    let edge = session.subshape(&box_shape, ShapeType::Edge, 0).unwrap();
    let filleted = session.fillet(&box_shape, &[&edge], 1.0).unwrap();
    let faces = (0..session.subshape_count(&filleted, ShapeType::Face).unwrap())
        .map(|index| session.subshape(&filleted, ShapeType::Face, index).unwrap())
        .collect::<Vec<_>>();
    let tangent_pairs = (0..faces.len())
        .flat_map(|first| (first + 1..faces.len()).map(move |second| (first, second)))
        .filter(|(first, second)| {
            session
                .faces_are_tangent(&filleted, &faces[*first], &faces[*second])
                .unwrap()
        })
        .count();
    assert!(tangent_pairs >= 2);
    assert_eq!(
        session
            .faces_are_tangent(&box_shape, &box_faces[0], &edge)
            .unwrap_err()
            .status,
        1
    );
}

#[test]
fn duplicate_handles_preserve_operation_history() {
    let session = Session::new().unwrap();
    let source = unit_box(&session, 0.0);
    let source_face = session.subshape(&source, ShapeType::Face, 0).unwrap();
    let translated = session
        .translate(&source, Vec3::new(10.0, 0.0, 0.0))
        .unwrap();
    let duplicate = session.duplicate(&translated).unwrap();

    assert_eq!(
        session
            .history_count(&duplicate, &source_face, HistoryRelation::Modified)
            .unwrap(),
        1
    );
    assert_eq!(
        session.bounds(&duplicate).unwrap(),
        session.bounds(&translated).unwrap()
    );
    assert_eq!(session.shape_count().unwrap(), 4);
}

#[test]
fn rejects_measurements_unsupported_by_shape_dimension() {
    let session = Session::new().unwrap();
    let wire = session
        .create_polyline_wire(&[Vec3::new(0.0, 0.0, 0.0), Vec3::new(4.0, 0.0, 0.0)], false)
        .unwrap();

    assert_eq!(session.shape_type(&wire).unwrap(), ShapeType::Wire);
    assert_eq!(session.surface_area(&wire).unwrap_err().status, 4);
    assert_eq!(session.volume(&wire).unwrap_err().status, 4);
    let center = session.center_of_mass(&wire).unwrap();
    assert!((center.x - 2.0).abs() < 1e-9);
    assert!(center.y.abs() < 1e-9);
    assert!(center.z.abs() < 1e-9);
    assert_eq!(session.shape_count().unwrap(), 1);
}

#[test]
fn central_inertia_matches_analytic_boxes_after_rotation_and_distant_placement() {
    let session = Session::new().unwrap();
    for (scale, offset) in [(0.01, 0.0), (100_000.0, 0.0), (1.0, 1_000_000.0)] {
        let body = session
            .create_box(
                Vec3::new(offset, offset, offset),
                Vec3::new(2.0 * scale, 3.0 * scale, 4.0 * scale),
            )
            .unwrap();
        let properties = session.mass_properties(&body).unwrap();
        assert!((properties.volume - 24.0 * scale.powi(3)).abs() < properties.volume * 1e-9);
        for (value, expected) in [
            (properties.center.x, offset + scale),
            (properties.center.y, offset + 1.5 * scale),
            (properties.center.z, offset + 2.0 * scale),
        ] {
            assert!((value - expected).abs() < scale * 1e-7);
        }
        for (row, factor) in [50.0, 40.0, 26.0].into_iter().enumerate() {
            assert!(
                (properties.inertia[row][row] - factor * scale.powi(5)).abs()
                    < factor * scale.powi(5) * 1e-8,
                "{:?}",
                properties.inertia
            );
            for column in 0..3 {
                if row != column {
                    assert!(properties.inertia[row][column].abs() < scale.powi(5) * 1e-6);
                }
            }
        }
        let rotated = session
            .rotate(
                &body,
                Vec3::new(offset, offset, offset),
                Vec3::new(0.0, 0.0, 1.0),
                std::f64::consts::FRAC_PI_4,
            )
            .unwrap();
        let properties = session.mass_properties(&rotated).unwrap();
        assert!(
            (properties.inertia[0][0] - 45.0 * scale.powi(5)).abs() < scale.powi(5) * 1e-6,
            "scale={scale} offset={offset} {:?}",
            properties.inertia
        );
        assert!((properties.inertia[1][1] - 45.0 * scale.powi(5)).abs() < scale.powi(5) * 1e-6);
        assert!((properties.inertia[0][1] - 5.0 * scale.powi(5)).abs() < scale.powi(5) * 1e-6);
        drop((rotated, body));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn exact_minimum_distance_distinguishes_separation_from_contact_and_containment() {
    let session = Session::new().unwrap();
    let first = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0))
        .unwrap();
    for (origin, size, expected) in [
        (Vec3::new(5.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0), 3.0),
        (
            Vec3::new(3.0, 3.0, 3.0),
            Vec3::new(2.0, 2.0, 2.0),
            3.0_f64.sqrt(),
        ),
        (Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0), 0.0),
        (Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0), 0.0),
        (Vec3::new(0.5, 0.5, 0.5), Vec3::new(0.5, 0.5, 0.5), 0.0),
    ] {
        let second = session.create_box(origin, size).unwrap();
        let handles = session.shape_count().unwrap();
        let distance = session.distance(&first, &second).unwrap();
        assert!((distance.distance - expected).abs() < 1e-8);
        let witness = (distance.first.x - distance.second.x).hypot(
            (distance.first.y - distance.second.y).hypot(distance.first.z - distance.second.z),
        );
        assert!((witness - expected).abs() < 1e-8);
        let overlap = session.overlap_volume(&first, &second).unwrap();
        let expected_overlap = if origin.x == 1.0 {
            4.0
        } else if origin.x == 0.5 {
            0.125
        } else {
            0.0
        };
        assert!((overlap - expected_overlap).abs() < 1e-9);
        assert_eq!(session.shape_count().unwrap(), handles);
    }
    let edge = session.subshape(&first, ShapeType::Edge, 0).unwrap();
    assert!(session.mass_properties(&edge).is_err());
    assert!(session.overlap_volume(&first, &edge).is_err());
    let empty = session
        .common(
            &first,
            &session
                .translate(&first, Vec3::new(10.0, 0.0, 0.0))
                .unwrap(),
        )
        .unwrap();
    assert!(session.distance(&first, &empty).is_err());
    let other = Session::new().unwrap();
    let foreign = unit_box(&other, 0.0);
    assert_wrong_session(session.distance(&first, &foreign).unwrap_err());
    assert_wrong_session(session.mass_properties(&foreign).unwrap_err());
    drop((first, edge, empty));
    assert_eq!(session.shape_count().unwrap(), 0);
}
