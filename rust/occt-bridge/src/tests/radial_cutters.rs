//! Radial cutters through cylindrical bends (ABI 84).
use super::*;

const PI: f64 = std::f64::consts::PI;

/// A quarter of a tube about the z axis, inside radius 3 and outside 5,
/// 10 mm tall: a bend at midplane radius 4.
fn bend(session: &Session) -> Shape<'_> {
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let wire = session
        .create_segment_wire(
            &[
                WireSegment::Line {
                    start: Vec3::new(3.0, 0.0, 0.0),
                    end: Vec3::new(5.0, 0.0, 0.0),
                },
                WireSegment::Arc {
                    start: Vec3::new(5.0, 0.0, 0.0),
                    middle: Vec3::new(5.0 * s, 5.0 * s, 0.0),
                    end: Vec3::new(0.0, 5.0, 0.0),
                },
                WireSegment::Line {
                    start: Vec3::new(0.0, 5.0, 0.0),
                    end: Vec3::new(0.0, 3.0, 0.0),
                },
                WireSegment::Arc {
                    start: Vec3::new(0.0, 3.0, 0.0),
                    middle: Vec3::new(3.0 * s, 3.0 * s, 0.0),
                    end: Vec3::new(3.0, 0.0, 0.0),
                },
            ],
            true,
        )
        .unwrap();
    let face = session.create_face_from_wire(&wire).unwrap();
    session
        .create_prism_from_face(&face, Vec3::new(0.0, 0.0, 10.0))
        .unwrap()
}

fn circle(center: (f64, f64), r: f64) -> Vec<WireSegment> {
    let p = |x: f64, y: f64| Vec3::new(center.0 + x, center.1 + y, 0.0);
    vec![
        WireSegment::Arc {
            start: p(r, 0.0),
            middle: p(0.0, r),
            end: p(-r, 0.0),
        },
        WireSegment::Arc {
            start: p(-r, 0.0),
            middle: p(0.0, -r),
            end: p(r, 0.0),
        },
    ]
}

#[test]
fn radial_cutters_remove_their_developed_area_times_the_thickness() {
    let session = Session::new().unwrap();
    let solid = bend(&session);
    let full = session.volume(&solid).unwrap();
    // A Ø3 hole at the middle of the bend's midplane arc (π of its 2π mm),
    // half way up: the bend loses its developed area × 2 mm, exactly.
    let cutter = session
        .create_radial_cutter(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
            4.0,
            2.5,
            5.5,
            &circle((PI, 5.0), 1.5),
        )
        .unwrap();
    assert!(session.is_valid(&cutter).unwrap());
    let cut = session.cut(&solid, &cutter).unwrap();
    assert!(session.is_valid(&cut).unwrap());
    let removed = full - session.volume(&cut).unwrap();
    let expected = PI * 1.5 * 1.5 * 2.0;
    assert!(
        (removed - expected).abs() < 1e-6 * expected,
        "{removed} vs {expected}"
    );
    // A slot with straight sides along the circumference and across it.
    let p = |x: f64, y: f64| Vec3::new(x, y, 0.0);
    let rectangle = [
        WireSegment::Line {
            start: p(1.0, 2.0),
            end: p(5.0, 2.0),
        },
        WireSegment::Line {
            start: p(5.0, 2.0),
            end: p(5.0, 4.0),
        },
        WireSegment::Line {
            start: p(5.0, 4.0),
            end: p(1.0, 4.0),
        },
        WireSegment::Line {
            start: p(1.0, 4.0),
            end: p(1.0, 2.0),
        },
    ];
    let cutter = session
        .create_radial_cutter(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
            4.0,
            2.5,
            5.5,
            &rectangle,
        )
        .unwrap();
    let removed = full
        - session
            .volume(&session.cut(&solid, &cutter).unwrap())
            .unwrap();
    assert!((removed - 4.0 * 2.0 * 2.0).abs() < 1e-6 * 16.0, "{removed}");
    // Invalid radii and axes are refused.
    for (inner, outer, axis) in [
        (4.5, 5.5, Vec3::new(0.0, 0.0, 1.0)),
        (2.5, 3.5, Vec3::new(0.0, 0.0, 1.0)),
        (2.5, 5.5, Vec3::new(1.0, 0.0, 1.0)),
    ] {
        assert!(
            session
                .create_radial_cutter(
                    Vec3::new(0.0, 0.0, 0.0),
                    axis,
                    Vec3::new(1.0, 0.0, 0.0),
                    4.0,
                    inner,
                    outer,
                    &rectangle
                )
                .is_err()
        );
    }
}
