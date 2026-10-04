use super::*;

const PI: f64 = std::f64::consts::PI;

fn line_path<'a>(session: &'a Session, points: &[Vec3]) -> Shape<'a> {
    session.create_polyline_wire(points, false).unwrap()
}

/// A quarter circle of radius 20 from the origin, leaving along +x and
/// ending at (20, 20, 0) heading +y.
fn quarter_arc(session: &Session) -> Shape<'_> {
    let s = 20.0 * std::f64::consts::FRAC_1_SQRT_2;
    session
        .create_segment_wire(
            &[WireSegment::Arc {
                start: Vec3::new(0.0, 0.0, 0.0),
                middle: Vec3::new(s, 20.0 - s, 0.0),
                end: Vec3::new(20.0, 20.0, 0.0),
            }],
            false,
        )
        .unwrap()
}

/// A circular face in the YZ plane, centered at the origin: across a path
/// that starts at the origin heading +x.
fn disc(session: &Session, radius: f64) -> Shape<'_> {
    let wire = session
        .create_circle_wire(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), radius)
        .unwrap();
    session.create_face_from_wire(&wire).unwrap()
}

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= 1e-6 * expected.abs()
}

#[test]
fn sweeps_follow_straight_and_curved_paths_with_exact_volumes() {
    let session = Session::new().unwrap();
    let straight = line_path(
        &session,
        &[Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0)],
    );
    let profile = disc(&session, 1.0);
    for orientation in [
        SweepOrientation::CorrectedFrenet,
        SweepOrientation::Frenet,
        SweepOrientation::Binormal(Vec3::new(0.0, 0.0, 1.0)),
        SweepOrientation::Fixed,
    ] {
        let rod = session.sweep(&profile, &straight, orientation).unwrap();
        assert_eq!(session.shape_type(&rod).unwrap(), ShapeType::Solid);
        assert!(
            close(session.volume(&rod).unwrap(), PI * 10.0),
            "{orientation:?}"
        );
    }

    // Following the arc, the volume is area x centerline length (Pappus).
    let arc = quarter_arc(&session);
    let tube = disc(&session, 2.0);
    let bent = session
        .sweep(&tube, &arc, SweepOrientation::CorrectedFrenet)
        .unwrap();
    assert!(session.is_valid(&bent).unwrap());
    assert!(close(
        session.volume(&bent).unwrap(),
        4.0 * PI * (PI / 2.0 * 20.0)
    ));
    let end = session.exact_bounds(&bent).unwrap();
    assert!((end.max.y - 20.0).abs() < 1e-6 && (end.max.x - 22.0).abs() < 1e-6);
    // Without rotation every section stays parallel to the first, so the
    // volume is area x the 20 mm the path advances along the profile normal.
    let sheared = session.sweep(&tube, &arc, SweepOrientation::Fixed).unwrap();
    assert!(close(session.volume(&sheared).unwrap(), 4.0 * PI * 20.0));

    // A mitered right-angle corner keeps area x centerline length.
    let square = session
        .create_polyline_wire(
            &[
                Vec3::new(0.0, -1.0, -1.0),
                Vec3::new(0.0, 1.0, -1.0),
                Vec3::new(0.0, 1.0, 1.0),
                Vec3::new(0.0, -1.0, 1.0),
            ],
            true,
        )
        .unwrap();
    let corner = line_path(
        &session,
        &[
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(10.0, 10.0, 0.0),
        ],
    );
    let elbow = session
        .sweep(&square, &corner, SweepOrientation::CorrectedFrenet)
        .unwrap();
    assert!(session.is_valid(&elbow).unwrap());
    assert!(close(session.volume(&elbow).unwrap(), 4.0 * 20.0));
}

#[test]
fn open_profiles_make_surfaces_and_history_names_generated_faces() {
    let session = Session::new().unwrap();
    let path = line_path(
        &session,
        &[Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0)],
    );
    let strip = session
        .create_polyline_wire(&[Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 3.0, 0.0)], false)
        .unwrap();
    let surface = session
        .sweep(&strip, &path, SweepOrientation::CorrectedFrenet)
        .unwrap();
    assert_ne!(session.shape_type(&surface).unwrap(), ShapeType::Solid);
    assert!(close(session.surface_area(&surface).unwrap(), 30.0));

    let profile = disc(&session, 1.0);
    let rod = session
        .sweep(&profile, &path, SweepOrientation::CorrectedFrenet)
        .unwrap();
    let rim = session.subshape(&profile, ShapeType::Edge, 0).unwrap();
    assert!(
        session
            .history_count(&rod, &rim, HistoryRelation::Generated)
            .unwrap()
            > 0
    );
}

#[test]
fn invalid_sweeps_fail_without_partial_results() {
    let session = Session::new().unwrap();
    let path = quarter_arc(&session);
    // A 25 mm tube cannot bend around a 20 mm radius.
    let too_wide = disc(&session, 25.0);
    let error = session
        .sweep(&too_wide, &path, SweepOrientation::CorrectedFrenet)
        .unwrap_err();
    assert!(
        error.message.contains("intersect itself"),
        "{}",
        error.message
    );
    let profile = disc(&session, 1.0);
    let zero = SweepOrientation::Binormal(Vec3::new(0.0, 0.0, 0.0));
    assert!(session.sweep(&profile, &path, zero).is_err());
    let block = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
        .unwrap();
    assert!(
        session
            .sweep(&profile, &block, SweepOrientation::CorrectedFrenet)
            .is_err()
    );
    assert!(
        session
            .sweep(&block, &path, SweepOrientation::CorrectedFrenet)
            .is_err()
    );
    // A face with a hole has two boundaries.
    let plate = session
        .create_box(Vec3::new(-5.0, -5.0, 0.0), Vec3::new(10.0, 10.0, 1.0))
        .unwrap();
    let bore = session
        .create_cylinder(
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, 0.0, 1.0),
            1.0,
            3.0,
        )
        .unwrap();
    let drilled = session.cut(&plate, &bore).unwrap();
    let holed = session
        .subshapes(&drilled, ShapeType::Face)
        .unwrap()
        .into_iter()
        .find(|face| session.subshape_count(face, ShapeType::Wire).unwrap() == 2)
        .unwrap();
    let error = session
        .sweep(&holed, &path, SweepOrientation::CorrectedFrenet)
        .unwrap_err();
    assert!(error.message.contains("no holes"), "{}", error.message);
}
