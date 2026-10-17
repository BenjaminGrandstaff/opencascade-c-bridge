//! Advanced sweeps (ABI 82): twist and scale laws, guide rails and
//! intermediate sections, checked against exact volumes and end faces.
use super::*;

const PI: f64 = std::f64::consts::PI;

fn straight(session: &Session, length: f64) -> Shape<'_> {
    session
        .create_polyline_wire(
            &[Vec3::new(0.0, 0.0, 0.0), Vec3::new(length, 0.0, 0.0)],
            false,
        )
        .unwrap()
}

/// A square face of side `side` in the plane x = `x`, centered on the x axis.
fn square(session: &Session, side: f64, x: f64) -> Shape<'_> {
    let h = side / 2.0;
    let wire = session
        .create_polyline_wire(
            &[
                Vec3::new(x, -h, -h),
                Vec3::new(x, h, -h),
                Vec3::new(x, h, h),
                Vec3::new(x, -h, h),
            ],
            true,
        )
        .unwrap();
    session.create_face_from_wire(&wire).unwrap()
}

fn disc(session: &Session, radius: f64) -> Shape<'_> {
    let wire = session
        .create_circle_wire(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), radius)
        .unwrap();
    session.create_face_from_wire(&wire).unwrap()
}

fn close(actual: f64, expected: f64, relative: f64) -> bool {
    (actual - expected).abs() <= relative * expected.abs()
}

/// The largest |y| of the planar end face lying in the plane x = `x`.
fn end_reach(session: &Session, shape: &Shape<'_>, x: f64) -> f64 {
    session
        .subshapes(shape, ShapeType::Face)
        .unwrap()
        .iter()
        .map(|face| session.exact_bounds(face).unwrap())
        .find(|b| (b.min.x - x).abs() < 1e-6 && (b.max.x - x).abs() < 1e-6)
        .map(|b| b.max.y.max(-b.min.y))
        .expect("an end face")
}

#[test]
fn twists_turn_the_profile_and_keep_its_area() {
    let session = Session::new().unwrap();
    let path = straight(&session, 10.0);
    let profile = square(&session, 2.0, 0.0);
    // An eighth turn leaves the end square on its corner: reach √2.
    for law in [SweepLaw::Linear, SweepLaw::Smooth] {
        let twisted = session
            .sweep_advanced(
                &profile,
                &path,
                &SweepOptions {
                    twist_radians: PI / 4.0,
                    twist_law: law,
                    ..SweepOptions::default()
                },
            )
            .unwrap();
        assert!(session.is_valid(&twisted).unwrap());
        assert!(
            close(session.volume(&twisted).unwrap(), 40.0, 1e-5),
            "{law:?}"
        );
        assert!((end_reach(&session, &twisted, 10.0) - 2f64.sqrt()).abs() < 1e-4);
        assert!((end_reach(&session, &twisted, 0.0) - 1.0).abs() < 1e-6);
    }
    // Several turns, and a twist along a bend, keep area × length.
    let many = SweepOptions {
        twist_radians: 6.0 * PI,
        ..SweepOptions::default()
    };
    let spun = session.sweep_advanced(&profile, &path, &many).unwrap();
    let spun_volume = session.volume(&spun).unwrap();
    assert!(
        close(spun_volume, 40.0, 1e-5),
        "{spun_volume} valid {}",
        session.is_valid(&spun).unwrap()
    );
    let s = 20.0 * std::f64::consts::FRAC_1_SQRT_2;
    let arc = session
        .create_segment_wire(
            &[WireSegment::Arc {
                start: Vec3::new(0.0, 0.0, 0.0),
                middle: Vec3::new(s, 20.0 - s, 0.0),
                end: Vec3::new(20.0, 20.0, 0.0),
            }],
            false,
        )
        .unwrap();
    let half = SweepOptions {
        twist_radians: PI,
        ..SweepOptions::default()
    };
    let bent = session.sweep_advanced(&profile, &arc, &half).unwrap();
    assert!(session.is_valid(&bent).unwrap());
    assert!(close(session.volume(&bent).unwrap(), 4.0 * 10.0 * PI, 1e-5));
}

#[test]
fn scale_laws_and_scaling_guides_resize_about_the_path() {
    let session = Session::new().unwrap();
    let path = straight(&session, 10.0);
    let profile = disc(&session, 1.0);
    let linear = SweepOptions {
        end_scale: 2.0,
        ..SweepOptions::default()
    };
    let cone = session.sweep_advanced(&profile, &path, &linear).unwrap();
    assert!(close(session.volume(&cone).unwrap(), 70.0 * PI / 3.0, 1e-9));
    let smooth = SweepOptions {
        scale_law: SweepLaw::Smooth,
        ..linear
    };
    let flared = session.sweep_advanced(&profile, &path, &smooth).unwrap();
    assert!(close(
        session.volume(&flared).unwrap(),
        10.0 * PI * 83.0 / 35.0,
        1e-9
    ));
    // A rail from the square's edge to twice the distance doubles it about
    // the path, so the end stays centered.
    let square = square(&session, 2.0, 0.0);
    let rail = session
        .create_polyline_wire(
            &[Vec3::new(0.0, 1.0, 0.0), Vec3::new(10.0, 2.0, 0.0)],
            false,
        )
        .unwrap();
    let guided = SweepOptions {
        guide: Some(SweepGuide {
            path: &rail,
            scales: true,
        }),
        ..SweepOptions::default()
    };
    let tapered = session.sweep_advanced(&square, &path, &guided).unwrap();
    assert!(close(session.volume(&tapered).unwrap(), 280.0 / 3.0, 1e-7));
    let bounds = session.exact_bounds(&tapered).unwrap();
    assert!((bounds.max.y - 2.0).abs() < 1e-6 && (bounds.min.y + 2.0).abs() < 1e-6);
}

#[test]
fn turning_guides_set_the_profile_angle() {
    let session = Session::new().unwrap();
    let path = straight(&session, 5.0);
    let profile = square(&session, 2.0, 0.0);
    // A helix about the path turning an eighth turn over its 5 mm.
    let helix = session
        .create_helix_wire(HelixOptions {
            origin: Vec3::new(0.0, 0.0, 0.0),
            axis: Vec3::new(1.0, 0.0, 0.0),
            start_direction: Vec3::new(0.0, 0.0, 1.0),
            radius: 3.0,
            pitch: 40.0,
            turns: 0.125,
            left_handed: false,
        })
        .unwrap();
    let options = SweepOptions {
        guide: Some(SweepGuide {
            path: &helix,
            scales: false,
        }),
        ..SweepOptions::default()
    };
    let turned = session.sweep_advanced(&profile, &path, &options).unwrap();
    assert!(close(session.volume(&turned).unwrap(), 20.0, 1e-5));
    assert!((end_reach(&session, &turned, 5.0) - 2f64.sqrt()).abs() < 1e-4);
}

#[test]
fn sections_blend_along_the_path() {
    let session = Session::new().unwrap();
    let path = straight(&session, 10.0);
    let start = square(&session, 2.0, 0.0);
    let wide = square(&session, 4.0, 10.0);
    let options = SweepOptions {
        sections: &[(&wide, 1.0)],
        ..SweepOptions::default()
    };
    let flared = session.sweep_advanced(&start, &path, &options).unwrap();
    assert!(close(session.volume(&flared).unwrap(), 280.0 / 3.0, 1e-9));
    // Through a wider middle and back: side 2 + 8t − 8t², area integrated.
    let middle = square(&session, 4.0, 5.0);
    let end = square(&session, 2.0, 10.0);
    let options = SweepOptions {
        sections: &[(&middle, 0.5), (&end, 1.0)],
        ..SweepOptions::default()
    };
    let bulged = session.sweep_advanced(&start, &path, &options).unwrap();
    assert!(session.is_valid(&bulged).unwrap());
    assert!(close(session.volume(&bulged).unwrap(), 344.0 / 3.0, 1e-9));
}

#[test]
fn advanced_sweeps_refuse_conflicting_or_impossible_options() {
    let session = Session::new().unwrap();
    let path = straight(&session, 10.0);
    let profile = square(&session, 2.0, 0.0);
    let end = square(&session, 2.0, 10.0);
    let rail = session
        .create_polyline_wire(&[Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 1.0, 0.0)], false)
        .unwrap();
    let corner = session
        .create_polyline_wire(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(10.0, 10.0, 0.0),
            ],
            false,
        )
        .unwrap();
    let ending = [(&end, 1.0)];
    let early = [(&end, 0.5)];
    let repeated = [(&end, 0.5), (&end, 0.5)];
    let twist = SweepOptions {
        twist_radians: 1.0,
        ..SweepOptions::default()
    };
    let guide = Some(SweepGuide {
        path: &rail,
        scales: true,
    });
    let cases: Vec<(&Shape<'_>, SweepOptions<'_, '_>, &str)> = vec![
        (
            &path,
            SweepOptions {
                orientation: SweepOrientation::Fixed,
                ..twist
            },
            "corrected-Frenet",
        ),
        (&path, SweepOptions { guide, ..twist }, "not a twist"),
        (&corner, twist, "without sharp corners"),
        (
            &path,
            SweepOptions {
                end_scale: 0.0,
                ..SweepOptions::default()
            },
            "finite and positive",
        ),
        (
            &path,
            SweepOptions {
                end_scale: 2.0,
                sections: &ending,
                ..SweepOptions::default()
            },
            "cannot be combined",
        ),
        (
            &path,
            SweepOptions {
                sections: &early,
                ..SweepOptions::default()
            },
            "position 1",
        ),
        (
            &path,
            SweepOptions {
                sections: &repeated,
                ..SweepOptions::default()
            },
            "must increase",
        ),
        (
            &path,
            SweepOptions {
                guide,
                ..SweepOptions::default()
            },
            "cross every plane",
        ),
    ];
    for (path, options, expected) in cases {
        let before = session.shape_count().unwrap();
        let error = session
            .sweep_advanced(&profile, path, &options)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{expected}: {error}");
        assert_eq!(session.shape_count().unwrap(), before);
    }
}

#[test]
fn path_stations_measure_along_the_whole_wire() {
    let session = Session::new().unwrap();
    // 10 mm along x, then 10 mm along y: half way is the corner's far side.
    let path = session
        .create_polyline_wire(
            &[
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(10.0, 10.0, 0.0),
            ],
            false,
        )
        .unwrap();
    let (point, tangent) = session.path_station(&path, 0.75).unwrap();
    assert!((point.x - 10.0).abs() < 1e-9 && (point.y - 5.0).abs() < 1e-9);
    assert!((tangent.y - 1.0).abs() < 1e-12);
    let (start, along) = session.path_station(&path, 0.0).unwrap();
    assert!(start.x.abs() < 1e-12 && (along.x - 1.0).abs() < 1e-12);
    assert!(session.path_station(&path, 1.5).is_err());
}

#[test]
fn glued_fuses_join_touching_solids_without_seams() {
    let session = Session::new().unwrap();
    // Three boxes side by side, and a smaller one on a part of a face.
    let boxes = [
        session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
            .unwrap(),
        session
            .create_box(Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
            .unwrap(),
        session
            .create_box(Vec3::new(20.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
            .unwrap(),
        session
            .create_box(Vec3::new(5.0, 10.0, 0.0), Vec3::new(4.0, 3.0, 10.0))
            .unwrap(),
    ];
    let fused = session
        .fuse_glued(&boxes.iter().collect::<Vec<_>>())
        .unwrap();
    assert_eq!(session.shape_type(&fused).unwrap(), ShapeType::Solid);
    assert!(session.is_valid(&fused).unwrap());
    assert!((session.volume(&fused).unwrap() - 3120.0).abs() < 1e-9);
    // The bar's faces merged: 6 for the bar, 4 more for the bump (its
    // base hides inside), the bar's side face split around it remains one.
    assert_eq!(session.subshape_count(&fused, ShapeType::Face).unwrap(), 10);
    assert!(session.fuse_glued(&[&boxes[0]]).is_err());
}
