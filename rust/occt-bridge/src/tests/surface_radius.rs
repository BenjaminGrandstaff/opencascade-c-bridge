use super::*;

const Z: Vec3 = Vec3 {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};

/// The edge whose bounds are the segment from `start` to `end`.
fn edge_between<'a>(session: &'a Session, shape: &Shape<'_>, start: Vec3, end: Vec3) -> Shape<'a> {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    session
        .subshapes(shape, ShapeType::Edge)
        .unwrap()
        .into_iter()
        .find(|edge| {
            let bounds = session.bounds(edge).unwrap();
            near(bounds.min.x, start.x.min(end.x))
                && near(bounds.max.x, start.x.max(end.x))
                && near(bounds.min.y, start.y.min(end.y))
                && near(bounds.max.y, start.y.max(end.y))
                && near(bounds.min.z, start.z.min(end.z))
                && near(bounds.max.z, start.z.max(end.z))
        })
        .expect("edge exists")
}

/// Smallest exact convex and concave radii over all faces.
fn extremes(bounds: &[FaceRadiusBounds]) -> (Option<f64>, Option<f64>) {
    let smallest = |side: fn(&FaceRadiusBounds) -> Option<(f64, Vec3)>| {
        bounds
            .iter()
            .filter_map(side)
            .map(|(radius, _)| radius)
            .reduce(f64::min)
    };
    (smallest(|face| face.convex), smallest(|face| face.concave))
}

fn assert_close(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("radius present");
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn analytic_faces_report_exact_signed_radii() {
    let session = Session::new().unwrap();
    let origin = Vec3::new(0.0, 0.0, 0.0);

    let cylinder = session.create_cylinder(origin, Z, 5.0, 10.0).unwrap();
    let bounds = session.face_radius_bounds(&cylinder, 8).unwrap();
    assert_eq!(bounds.len(), 3);
    assert!(bounds.iter().all(|face| face.exact && face.samples == 0));
    let (convex, concave) = extremes(&bounds);
    assert_close(convex, 5.0);
    assert_eq!(concave, None);
    let (_, point) = bounds.iter().find_map(|face| face.convex).unwrap();
    assert!(
        (point.x.hypot(point.y) - 5.0).abs() < 1e-9,
        "witness on the wall"
    );

    let sphere = session.create_sphere(origin, 4.0).unwrap();
    assert_close(
        extremes(&session.face_radius_bounds(&sphere, 8).unwrap()).0,
        4.0,
    );

    // Frustum half-angle a has tan a = (6 - 2) / 8; the smallest
    // circumferential radius is at the top: 2 / cos a.
    let cone = session.create_cone(origin, Z, 6.0, 2.0, 8.0).unwrap();
    let (convex, concave) = extremes(&session.face_radius_bounds(&cone, 8).unwrap());
    assert_close(convex, 2.0 * 1.25_f64.sqrt());
    assert_eq!(concave, None);

    // A through bore is concave; reversing the face orientation is what flips it.
    let plate = session
        .create_box(Vec3::new(-10.0, -10.0, 0.0), Vec3::new(20.0, 20.0, 5.0))
        .unwrap();
    let bore = session
        .create_cylinder(Vec3::new(0.0, 0.0, -1.0), Z, 2.0, 7.0)
        .unwrap();
    let drilled = session.cut(&plate, &bore).unwrap();
    let (convex, concave) = extremes(&session.face_radius_bounds(&drilled, 8).unwrap());
    assert_eq!(convex, None);
    assert_close(concave, 2.0);

    // A rounded cylinder rim is a torus: minor radius 1 is the smallest, and
    // the other principal radius only grows toward the cap.
    let rim = edge_between(
        &session,
        &cylinder,
        Vec3::new(-5.0, -5.0, 10.0),
        Vec3::new(5.0, 5.0, 10.0),
    );
    let rounded = session.fillet(&cylinder, &[&rim], 1.0).unwrap();
    let bounds = session.face_radius_bounds(&rounded, 8).unwrap();
    assert!(bounds.iter().all(|face| face.exact));
    let (convex, concave) = extremes(&bounds);
    assert_close(convex, 1.0);
    assert_eq!(concave, None);
}

#[test]
fn edge_concavity_separates_inside_and_outside_corners() {
    let session = Session::new().unwrap();
    // An L in plan: the vertical edge at x = y = 10 is an inside corner.
    let long = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(30.0, 10.0, 10.0))
        .unwrap();
    let tall = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 30.0, 10.0))
        .unwrap();
    let l_shape = session.fuse(&long, &tall).unwrap();
    let edges = session.subshapes(&l_shape, ShapeType::Edge).unwrap();
    let concavities = session.edge_concavities(&l_shape, 1e-6).unwrap();
    assert_eq!(concavities.len(), edges.len());
    let corner = edge_between(
        &session,
        &l_shape,
        Vec3::new(10.0, 10.0, 0.0),
        Vec3::new(10.0, 10.0, 10.0),
    );
    // Edges between coplanar faces the fuse left split are smooth.
    for (edge, concavity) in edges.iter().zip(&concavities) {
        if session.is_same(edge, &corner).unwrap() {
            assert_eq!(*concavity, EdgeConcavity::Concave);
        } else {
            assert!(
                matches!(concavity, EdgeConcavity::Convex | EdgeConcavity::Smooth),
                "{concavity:?}"
            );
        }
    }

    // Rounding the inside corner leaves a concave face and smooth edges.
    let rounded = session.fillet(&l_shape, &[&corner], 3.0).unwrap();
    let (convex, concave) = extremes(&session.face_radius_bounds(&rounded, 8).unwrap());
    assert_eq!(convex, None);
    assert_close(concave, 3.0);
    let concavities = session.edge_concavities(&rounded, 1e-6).unwrap();
    assert!(!concavities.contains(&EdgeConcavity::Concave));
    assert!(
        concavities
            .iter()
            .filter(|concavity| **concavity == EdgeConcavity::Smooth)
            .count()
            >= 2,
        "the fillet meets both walls tangentially"
    );

    // An open face has free boundary edges.
    let face = session.subshape(&l_shape, ShapeType::Face, 0).unwrap();
    assert!(
        session
            .edge_concavities(&face, 1e-6)
            .unwrap()
            .iter()
            .all(|concavity| *concavity == EdgeConcavity::Other)
    );
}

#[test]
fn freeform_faces_are_sampled_inside_the_face() {
    let session = Session::new().unwrap();
    let block = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(20.0, 10.0, 10.0))
        .unwrap();
    // A linear 1 -> 2 mm law along a 20 mm edge gives a freeform blend whose
    // smallest radius is at the start, on the sample grid's boundary.
    let edge = edge_between(
        &session,
        &block,
        Vec3::new(0.0, 0.0, 10.0),
        Vec3::new(20.0, 0.0, 10.0),
    );
    let blended = session.variable_fillet(&block, &[&edge], 1.0, 2.0).unwrap();
    let bounds = session.face_radius_bounds(&blended, 17).unwrap();
    let sampled = bounds.iter().filter(|face| !face.exact).collect::<Vec<_>>();
    assert_eq!(sampled.len(), 1);
    let blend = sampled[0];
    assert!(blend.samples > 0 && blend.samples <= 17 * 17);
    let (radius, point) = blend.convex.unwrap();
    assert!((radius - 1.0).abs() < 1e-2, "{radius}");
    assert!(point.x < 1.0, "the smallest radius is at the start");
    // Lengthwise, the approximated blend is nearly straight: any curvature it
    // has there is far gentler than its 1-2 mm cross-section.
    assert!(
        blend.concave.is_none_or(|(radius, _)| radius > 100.0),
        "{blend:?}"
    );

    assert!(session.face_radius_bounds(&blended, 1).is_err());
    assert!(session.face_radius_bounds(&blended, 1025).is_err());
    assert!(session.edge_concavities(&blended, 0.0).is_err());
    assert!(session.edge_concavities(&blended, f64::NAN).is_err());
}
