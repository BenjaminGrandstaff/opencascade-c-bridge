use super::*;

fn block(session: &Session) -> Shape<'_> {
    session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
        .unwrap()
}

fn vertical_edge<'a>(session: &'a Session, body: &Shape<'_>) -> Shape<'a> {
    (0..session.subshape_count(body, ShapeType::Edge).unwrap())
        .map(|i| session.subshape(body, ShapeType::Edge, i).unwrap())
        .find(|edge| {
            let center = session.center_of_mass(edge).unwrap();
            center.x.abs() < 1e-6 && center.y.abs() < 1e-6 && (center.z - 5.0).abs() < 1e-6
        })
        .unwrap()
}

#[test]
fn linear_fillet_geometry_history_and_input_preservation() {
    let session = Session::new().unwrap();
    let body = block(&session);
    let edge = vertical_edge(&session, &body);
    let constant_volume =
        |radius: f64| 1000.0 - (1.0 - std::f64::consts::FRAC_PI_4) * 10.0 * radius * radius;
    let mut measurements = Vec::new();
    for (start, end) in [(1.0, 2.0), (2.0, 1.0), (1.5, 1.5)] {
        let result = session
            .variable_fillet(&body, &[&edge], start, end)
            .unwrap();
        let volume = session.volume(&result).unwrap();
        assert!(volume >= constant_volume(start.max(end)) - 1e-6);
        assert!(volume <= constant_volume(start.min(end)) + 1e-6);
        measurements.push((volume, session.center_of_mass(&result).unwrap().z));
        assert!(session.is_valid(&result).unwrap());
        assert!(
            session
                .history_count(&result, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
        if start == end {
            let constant = session.fillet(&body, &[&edge], start).unwrap();
            assert!((volume - session.volume(&constant).unwrap()).abs() < 1e-6);
            assert!((volume - constant_volume(start)).abs() < 1e-6);
        }
        assert!((session.volume(&body).unwrap() - 1000.0).abs() < 1e-7);
    }
    // Endpoint reversal mirrors the blend. A constant or averaged radius law
    // cannot satisfy the volume difference and noncentral mass checks.
    assert!((measurements[0].0 - measurements[1].0).abs() < 1e-6);
    assert!((measurements[0].1 + measurements[1].1 - 10.0).abs() < 1e-6);
    assert!((measurements[0].1 - 5.0).abs() > 1e-4);
    assert!((measurements[0].0 - measurements[2].0).abs() > 1e-3);
    drop((body, edge));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn variable_fillet_uses_one_law_for_selected_tangent_neighbors() {
    let session = Session::new().unwrap();
    let body = session
        .create_polygon_prism(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(5.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(10.0, 10.0, 0.0),
                Vec3::new(0.0, 10.0, 0.0),
            ],
            Vec3::new(0.0, 0.0, 10.0),
        )
        .unwrap();
    let edges = (0..session.subshape_count(&body, ShapeType::Edge).unwrap())
        .map(|i| session.subshape(&body, ShapeType::Edge, i).unwrap())
        .filter(|e| {
            let c = session.center_of_mass(e).unwrap();
            c.y.abs() < 1e-6 && (c.z - 10.0).abs() < 1e-6
        })
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), 2);
    let single = session
        .variable_fillet(&body, &[&edges[0]], 1.0, 2.0)
        .unwrap();
    let both = session
        .variable_fillet(&body, &[&edges[0], &edges[1]], 1.0, 2.0)
        .unwrap();
    assert!((session.volume(&single).unwrap() - session.volume(&both).unwrap()).abs() < 1e-6);
    assert!(session.is_valid(&both).unwrap());
    for edge in &edges {
        assert!(
            session
                .history_count(&both, edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
    }
    drop((single, both, edges, body));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn variable_fillet_rejects_invalid_radii_selections_and_closed_contours() {
    let session = Session::new().unwrap();
    let body = block(&session);
    let edge = vertical_edge(&session, &body);
    let face = session.subshape(&body, ShapeType::Face, 0).unwrap();
    let other = unit_box(&session, 20.0);
    let stranger = session.subshape(&other, ShapeType::Edge, 0).unwrap();
    let count = session.shape_count().unwrap();
    for (a, b) in [
        (0.0, 1.0),
        (1.0, 0.0),
        (-1.0, 1.0),
        (1.0, -1.0),
        (f64::NAN, 1.0),
        (1.0, f64::NAN),
        (f64::INFINITY, 1.0),
        (1.0, f64::INFINITY),
    ] {
        assert_eq!(
            session
                .variable_fillet(&body, &[&edge], a, b)
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(session.shape_count().unwrap(), count);
    }
    for edges in [vec![], vec![&edge, &edge], vec![&face], vec![&stranger]] {
        assert!(session.variable_fillet(&body, &edges, 1.0, 2.0).is_err());
        assert_eq!(session.shape_count().unwrap(), count);
    }
    let cylinder = session
        .create_cylinder(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            5.0,
            10.0,
        )
        .unwrap();
    let circle = (0..session.subshape_count(&cylinder, ShapeType::Edge).unwrap())
        .map(|i| session.subshape(&cylinder, ShapeType::Edge, i).unwrap())
        .find(|edge| session.edge_circle_radius(edge).unwrap().is_some())
        .unwrap();
    let count = session.shape_count().unwrap();
    let error = session
        .variable_fillet(&cylinder, &[&circle], 1.0, 2.0)
        .unwrap_err();
    assert_eq!(error.status, 4);
    assert!(error.message.contains("open tangent contours"));
    assert_eq!(session.shape_count().unwrap(), count);
}

#[test]
fn variable_fillet_sessions_stale_handles_and_failure_diagnostics() {
    let session = Session::new().unwrap();
    let body = block(&session);
    let edge = vertical_edge(&session, &body);
    let second = Session::new().unwrap();
    let foreign = block(&second);
    let foreign_edge = vertical_edge(&second, &foreign);
    assert_wrong_session(
        session
            .variable_fillet(&foreign, &[&edge], 1.0, 2.0)
            .unwrap_err(),
    );
    assert_wrong_session(
        session
            .variable_fillet(&body, &[&foreign_edge], 1.0, 2.0)
            .unwrap_err(),
    );
    let stale = session.duplicate(&edge).unwrap();
    session.remove(session.shape(stale.id)).unwrap();
    assert!(session.variable_fillet(&body, &[&stale], 1.0, 2.0).is_err());
    let count = session.shape_count().unwrap();
    let error = session
        .variable_fillet(&body, &[&edge], 20.0, 30.0)
        .unwrap_err();
    assert_eq!(error.status, 6);
    assert!(!error.diagnostics.is_empty());
    assert!(
        error
            .diagnostics
            .iter()
            .any(|d| d.input_index == Some(0) && d.has_shape)
    );
    let diagnostic = session.last_diagnostic_shape(0).unwrap().unwrap();
    assert!(session.is_same(&diagnostic, &edge).unwrap());
    drop(diagnostic);
    assert_eq!(session.shape_count().unwrap(), count);
}

#[test]
fn simultaneous_independent_sessions_build_stable_edge_treatments() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads = (0..8)
        .map(|worker| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let session = Session::new().unwrap();
                let body = block(&session);
                let edge = vertical_edge(&session, &body);
                barrier.wait();
                for _ in 0..8 {
                    let result = match worker % 3 {
                        0 => session.variable_fillet(&body, &[&edge], 1.0, 2.0),
                        1 => session.fillet(&body, &[&edge], 1.5),
                        _ => session.chamfer(&body, &[&edge], 1.0),
                    }
                    .unwrap();
                    assert!(session.is_valid(&result).unwrap());
                    let volume = session.volume(&result).unwrap();
                    assert!(volume > 990.0 && volume < 1000.0);
                    drop(result);
                    assert_eq!(session.shape_count().unwrap(), 2);
                }
                drop((body, edge));
                assert_eq!(session.shape_count().unwrap(), 0);
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn station_fillet_controls_interior_radius_and_spine_start() {
    let session = Session::new().unwrap();
    let body = block(&session);
    let edge = vertical_edge(&session, &body);
    let stations = [
        FilletRadiusStation {
            position: 0.0,
            radius: 1.0,
        },
        FilletRadiusStation {
            position: 0.25,
            radius: 2.5,
        },
        FilletRadiusStation {
            position: 1.0,
            radius: 2.0,
        },
    ];
    let linear = session.variable_fillet(&body, &[&edge], 1.0, 2.0).unwrap();
    let mut results = Vec::new();
    for direction in [
        FilletSpineDirection::Kernel,
        FilletSpineDirection::Reversed,
        FilletSpineDirection::FromPoint(Vec3::new(0.0, 0.0, 0.0)),
        FilletSpineDirection::FromPoint(Vec3::new(0.0, 0.0, 10.0)),
    ] {
        let shape = session
            .variable_fillet_stations(&body, &[&edge], &stations, direction)
            .unwrap();
        assert!(session.is_valid(&shape).unwrap());
        assert_eq!(session.subshape_count(&shape, ShapeType::Solid).unwrap(), 1);
        assert!(
            session
                .history_count(&shape, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
        assert!((session.volume(&shape).unwrap() - session.volume(&linear).unwrap()).abs() > 0.1);
        results.push((
            session.volume(&shape).unwrap(),
            session.center_of_mass(&shape).unwrap().z,
        ));
    }
    for (first, last) in [(results[0], results[1]), (results[2], results[3])] {
        // The default kernel volume quadrature is approximate on spline
        // blends; mirror centers are a tighter orientation invariant.
        assert!((first.0 - last.0).abs() < 0.1, "{first:?} {last:?}");
        assert!((first.1 + last.1 - 10.0).abs() < 1e-5, "{first:?} {last:?}");
        assert!((first.1 - last.1).abs() > 1e-4);
    }
    let constant = [
        FilletRadiusStation {
            position: 0.0,
            radius: 1.5,
        },
        FilletRadiusStation {
            position: 0.5,
            radius: 1.5,
        },
        FilletRadiusStation {
            position: 1.0,
            radius: 1.5,
        },
    ];
    let shape = session
        .variable_fillet_stations(&body, &[&edge], &constant, FilletSpineDirection::Kernel)
        .unwrap();
    let standard = session.fillet(&body, &[&edge], 1.5).unwrap();
    assert!((session.volume(&shape).unwrap() - session.volume(&standard).unwrap()).abs() < 1e-6);
    assert!((session.volume(&body).unwrap() - 1000.0).abs() < 1e-7);
    drop((shape, standard, linear, edge, body));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn station_fillet_rejects_bad_laws_and_ambiguous_start_without_leaks() {
    let session = Session::new().unwrap();
    let body = block(&session);
    let edge = vertical_edge(&session, &body);
    let handles = session.shape_count().unwrap();
    let good = [
        FilletRadiusStation {
            position: 0.0,
            radius: 1.0,
        },
        FilletRadiusStation {
            position: 1.0,
            radius: 2.0,
        },
    ];
    for stations in [
        Vec::new(),
        vec![good[0]],
        vec![
            FilletRadiusStation {
                position: 0.1,
                ..good[0]
            },
            good[1],
        ],
        vec![
            good[0],
            FilletRadiusStation {
                position: 0.9,
                ..good[1]
            },
        ],
        vec![
            good[0],
            FilletRadiusStation {
                position: 0.0,
                radius: 1.5,
            },
            good[1],
        ],
        vec![
            good[0],
            FilletRadiusStation {
                position: 0.8,
                radius: 1.5,
            },
            FilletRadiusStation {
                position: 0.2,
                radius: 1.8,
            },
            good[1],
        ],
        vec![
            FilletRadiusStation {
                position: 0.0,
                radius: 0.0,
            },
            good[1],
        ],
        vec![
            good[0],
            FilletRadiusStation {
                position: 1.0,
                radius: f64::NAN,
            },
        ],
        vec![
            good[0],
            FilletRadiusStation {
                position: f64::NAN,
                radius: 2.0,
            },
        ],
    ] {
        assert!(
            session
                .variable_fillet_stations(&body, &[&edge], &stations, FilletSpineDirection::Kernel)
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), handles);
    }
    for point in [Vec3::new(0.0, 0.0, 5.0), Vec3::new(f64::NAN, 0.0, 0.0)] {
        assert!(
            session
                .variable_fillet_stations(
                    &body,
                    &[&edge],
                    &good,
                    FilletSpineDirection::FromPoint(point)
                )
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), handles);
    }
    assert!(
        session
            .variable_fillet_stations(&body, &[&edge, &edge], &good, FilletSpineDirection::Kernel)
            .is_err()
    );
    let oversized = [
        FilletRadiusStation {
            position: 0.0,
            radius: 20.0,
        },
        FilletRadiusStation {
            position: 0.5,
            radius: 25.0,
        },
        FilletRadiusStation {
            position: 1.0,
            radius: 20.0,
        },
    ];
    let error = session
        .variable_fillet_stations(&body, &[&edge], &oversized, FilletSpineDirection::Kernel)
        .unwrap_err();
    assert!(!error.diagnostics.is_empty());
    assert_eq!(session.shape_count().unwrap(), handles);
    drop((edge, body));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn station_fillet_applies_one_law_to_tangent_neighbors_and_scaled_models() {
    let session = Session::new().unwrap();
    let body = session
        .create_polygon_prism(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(5.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(10.0, 10.0, 0.0),
                Vec3::new(0.0, 10.0, 0.0),
            ],
            Vec3::new(0.0, 0.0, 10.0),
        )
        .unwrap();
    let edges = (0..session.subshape_count(&body, ShapeType::Edge).unwrap())
        .map(|i| session.subshape(&body, ShapeType::Edge, i).unwrap())
        .filter(|edge| {
            let center = session.center_of_mass(edge).unwrap();
            center.y.abs() < 1e-6 && (center.z - 10.0).abs() < 1e-6
        })
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), 2);
    let stations = [
        FilletRadiusStation {
            position: 0.0,
            radius: 1.0,
        },
        FilletRadiusStation {
            position: 0.5,
            radius: 2.0,
        },
        FilletRadiusStation {
            position: 1.0,
            radius: 1.5,
        },
    ];
    let single = session
        .variable_fillet_stations(&body, &[&edges[0]], &stations, FilletSpineDirection::Kernel)
        .unwrap();
    let references = edges.iter().collect::<Vec<_>>();
    let both = session
        .variable_fillet_stations(&body, &references, &stations, FilletSpineDirection::Kernel)
        .unwrap();
    assert!((session.volume(&single).unwrap() - session.volume(&both).unwrap()).abs() < 1e-6);
    drop((single, both, edges, body));
    for (scale, offset) in [(0.1, 0.0), (100_000.0, 0.0), (1.0, 1_000_000.0)] {
        let body = session
            .create_box(
                Vec3::new(offset, offset, offset),
                Vec3::new(10.0 * scale, 10.0 * scale, 10.0 * scale),
            )
            .unwrap();
        let edge = (0..session.subshape_count(&body, ShapeType::Edge).unwrap())
            .map(|i| session.subshape(&body, ShapeType::Edge, i).unwrap())
            .find(|edge| {
                let center = session.center_of_mass(edge).unwrap();
                (center.x - offset).abs() < scale * 1e-6
                    && (center.y - offset).abs() < scale * 1e-6
                    && (center.z - offset - 5.0 * scale).abs() < scale * 1e-6
            })
            .unwrap();
        let law = stations.map(|station| FilletRadiusStation {
            radius: station.radius * scale,
            ..station
        });
        let result = session
            .variable_fillet_stations(
                &body,
                &[&edge],
                &law,
                FilletSpineDirection::FromPoint(Vec3::new(offset, offset, offset)),
            )
            .unwrap();
        assert!(session.is_valid(&result).unwrap());
        let normalized = session.volume(&result).unwrap() / scale.powi(3);
        assert!((950.0..1000.0).contains(&normalized), "{normalized}");
        drop((result, edge, body));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
