use super::*;
use std::f64::consts::TAU;

fn helix(turns: f64, left_handed: bool) -> HelixOptions {
    HelixOptions {
        origin: Vec3::new(100.0, -50.0, 20.0),
        axis: Vec3::new(0.0, 0.0, 3.0),
        // Not perpendicular to the axis: only its perpendicular part counts.
        start_direction: Vec3::new(2.0, 0.0, 5.0),
        radius: 10.0,
        pitch: 4.0,
        turns,
        left_handed,
    }
}

#[test]
fn helix_length_endpoints_and_handedness_match_the_exact_curve() {
    let session = Session::new().unwrap();
    for (turns, left_handed) in [(3.25, false), (3.25, true), (0.5, false)] {
        let wire = session
            .create_helix_wire(helix(turns, left_handed))
            .unwrap();
        assert_eq!(session.shape_type(&wire).unwrap(), ShapeType::Wire);
        // One edge per (possibly partial) turn, in order along the helix.
        let edges = session.subshape_count(&wire, ShapeType::Edge).unwrap();
        assert_eq!(edges, (turns - 1e-9_f64).ceil() as usize);
        let exact = turns * (TAU * 10.0).hypot(4.0);
        let mut length = 0.0;
        let mut points = Vec::new();
        for index in 0..edges {
            let edge = session.subshape(&wire, ShapeType::Edge, index).unwrap();
            length += session.edge_length(&edge).unwrap();
            points.extend(session.edge_sample_points(&edge, 41).unwrap());
        }
        assert!((length - exact).abs() < 1e-6 * exact, "{length} vs {exact}");
        let first = points[0];
        let last = *points.last().unwrap();
        assert!((first.x - 110.0).abs() < 1e-6 && (first.y + 50.0).abs() < 1e-6);
        assert!((first.z - 20.0).abs() < 1e-6);
        // Every sample lies on the cylinder.
        for p in &points {
            let r = (p.x - 100.0).hypot(p.y + 50.0);
            assert!((r - 10.0).abs() < 1e-6, "radius {r}");
        }
        assert!((last.z - (20.0 + 4.0 * turns)).abs() < 1e-6);
        let angle = turns * TAU * if left_handed { -1.0 } else { 1.0 };
        assert!((last.x - (100.0 + 10.0 * angle.cos())).abs() < 1e-6);
        assert!((last.y - (-50.0 + 10.0 * angle.sin())).abs() < 1e-6);
        // Right-handed helices turn counterclockwise looking down the axis.
        let second = points[1];
        assert_eq!(second.y > first.y, !left_handed);
    }
    for bad in [
        HelixOptions {
            radius: 0.0,
            ..helix(1.0, false)
        },
        HelixOptions {
            pitch: -1.0,
            ..helix(1.0, false)
        },
        HelixOptions {
            turns: 0.0,
            ..helix(1.0, false)
        },
        HelixOptions {
            turns: 20_000.0,
            ..helix(1.0, false)
        },
        HelixOptions {
            start_direction: Vec3::new(0.0, 0.0, 1.0),
            ..helix(1.0, false)
        },
        HelixOptions {
            axis: Vec3::new(0.0, 0.0, 0.0),
            ..helix(1.0, false)
        },
        HelixOptions {
            origin: Vec3::new(f64::NAN, 0.0, 0.0),
            ..helix(1.0, false)
        },
    ] {
        assert!(session.create_helix_wire(bad).is_err());
    }
}

#[test]
fn a_circle_swept_along_a_helix_makes_a_valid_spring_of_tube_volume() {
    let session = Session::new().unwrap();
    for left_handed in [false, true] {
        let options = helix(5.0, left_handed);
        let path = session.create_helix_wire(options).unwrap();
        // Profile at the start, across the start tangent.
        let tangent = Vec3::new(0.0, if left_handed { -TAU * 10.0 } else { TAU * 10.0 }, 4.0);
        let profile = session
            .create_face_from_wire(
                &session
                    .create_circle_wire(Vec3::new(110.0, -50.0, 20.0), tangent, 1.5)
                    .unwrap(),
            )
            .unwrap();
        let spring = session
            .sweep(&profile, &path, SweepOrientation::Binormal(options.axis))
            .unwrap();
        assert!(session.is_valid(&spring).unwrap());
        assert_eq!(session.shape_type(&spring).unwrap(), ShapeType::Solid);
        // Tube formula: cross-section area times centerline length.
        let expected = std::f64::consts::PI * 1.5 * 1.5 * 5.0 * (TAU * 10.0).hypot(4.0);
        let volume = session.volume(&spring).unwrap();
        assert!(
            (volume - expected).abs() < 1e-3 * expected,
            "{volume} vs {expected}"
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
