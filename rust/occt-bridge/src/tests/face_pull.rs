//! Exact per-face pull ranges, checked against fine tessellations.

use super::*;

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn unit_facet_normal(triangle: &MeshTriangle) -> Vec3 {
    let [a, b, c] = triangle.points;
    let (u, w) = (
        v(b.x - a.x, b.y - a.y, b.z - a.z),
        v(c.x - a.x, c.y - a.y, c.z - a.z),
    );
    let n = v(
        u.y * w.z - u.z * w.y,
        u.z * w.x - u.x * w.z,
        u.x * w.y - u.y * w.x,
    );
    let length = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
    v(n.x / length, n.y / length, n.z / length)
}

/// Every exact range contains each of its face's facet projections and is
/// approached by them, within the tessellation's angular deflection.
/// Returns how many faces were exact.
fn agrees_with_tessellation(session: &Session, shape: &Shape<'_>, pull: Vec3) -> usize {
    let ranges = session.face_pull_ranges(shape, pull).unwrap();
    let options = MeshOptions {
        linear_deflection: 0.02,
        angular_deflection_radians: 0.05,
        maximum_triangles: 1_000_000,
    };
    let triangles = session.surface_mesh(shape, options).unwrap();
    let norm = (pull.x * pull.x + pull.y * pull.y + pull.z * pull.z).sqrt();
    // Facet normals stray from the surface's by about the angular deflection.
    let slack = 0.06;
    let mut seen = vec![(f64::INFINITY, f64::NEG_INFINITY); ranges.len()];
    for triangle in &triangles {
        let n = unit_facet_normal(triangle);
        let projection = (n.x * pull.x + n.y * pull.y + n.z * pull.z) / norm;
        let entry = &mut seen[triangle.face_index];
        *entry = (entry.0.min(projection), entry.1.max(projection));
    }
    let mut exact = 0;
    for (face, (range, (low, high))) in ranges.iter().zip(seen).enumerate() {
        let Some(range) = range else { continue };
        exact += 1;
        let (minimum, maximum) = (range.minimum.0, range.maximum.0);
        assert!(minimum <= maximum, "face {face}: {range:?}");
        assert!(
            low >= minimum - slack && high <= maximum + slack,
            "face {face}: facets {low}..{high} outside {minimum}..{maximum}"
        );
        assert!(
            (low - minimum).abs() <= slack && (high - maximum).abs() <= slack,
            "face {face}: facets {low}..{high} do not reach {minimum}..{maximum}"
        );
    }
    exact
}

#[test]
fn analytic_faces_have_exact_pull_ranges() {
    let session = Session::new().unwrap();
    let up = v(0.0, 0.0, 1.0);

    // A cylinder lying along x: its side spans the whole range, lowest at
    // its underside.
    let lying = session
        .create_cylinder(v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 2.0, 10.0)
        .unwrap();
    let ranges = session.face_pull_ranges(&lying, up).unwrap();
    assert!(ranges.iter().all(Option::is_some));
    let side = ranges
        .iter()
        .flatten()
        .find(|range| range.minimum.0 < -0.5 && range.maximum.0 > 0.5)
        .unwrap();
    assert!((side.minimum.0 + 1.0).abs() < 1e-12 && (side.maximum.0 - 1.0).abs() < 1e-12);
    assert!(
        (side.minimum.1.z + 2.0).abs() < 1e-9,
        "{:?}",
        side.minimum.1
    );
    assert!(
        (side.maximum.1.z - 2.0).abs() < 1e-9,
        "{:?}",
        side.maximum.1
    );
    assert_eq!(agrees_with_tessellation(&session, &lying, up), 3);

    // A cone narrowing upward leans out of the pull by atan(3 / 6).
    let cone = session
        .create_cone(v(0.0, 0.0, 0.0), up, 5.0, 2.0, 6.0)
        .unwrap();
    let ranges = session.face_pull_ranges(&cone, up).unwrap();
    let expected = 3.0 / 45.0_f64.sqrt();
    let flank = ranges
        .iter()
        .flatten()
        .find(|range| range.minimum.0.abs() < 0.99)
        .unwrap();
    assert!((flank.minimum.0 - expected).abs() < 1e-12, "{flank:?}");
    assert!((flank.maximum.0 - expected).abs() < 1e-12, "{flank:?}");
    assert_eq!(agrees_with_tessellation(&session, &cone, up), 3);

    // A sphere spans the whole range, lowest at its south pole.
    let sphere = session.create_sphere(v(1.0, 2.0, 3.0), 3.0).unwrap();
    let ranges = session.face_pull_ranges(&sphere, up).unwrap();
    let ball = ranges[0].unwrap();
    assert!((ball.minimum.0 + 1.0).abs() < 1e-12 && (ball.maximum.0 - 1.0).abs() < 1e-12);
    assert!(
        (ball.minimum.1.z - 0.0).abs() < 1e-9,
        "{:?}",
        ball.minimum.1
    );
    assert_eq!(agrees_with_tessellation(&session, &sphere, up), 1);

    // Pull directions need not be unit length or axis aligned.
    let slanted = v(0.0, 3.0, 4.0);
    assert_eq!(agrees_with_tessellation(&session, &lying, slanted), 3);
    assert_eq!(agrees_with_tessellation(&session, &sphere, slanted), 1);
}

#[test]
fn modeled_parts_agree_with_their_tessellation() {
    let session = Session::new().unwrap();
    let up = v(0.0, 0.0, 1.0);
    // A post whose top rim is rounded: a torus between side and top.
    let post = session
        .create_cylinder(v(0.0, 0.0, 0.0), up, 4.0, 8.0)
        .unwrap();
    let rim = (0..session.subshape_count(&post, ShapeType::Edge).unwrap())
        .map(|index| session.subshape(&post, ShapeType::Edge, index).unwrap())
        .find(|edge| {
            let bounds = session.bounds(edge).unwrap();
            bounds.min.z > 7.0 && session.edge_circle_radius(edge).unwrap().is_some()
        })
        .unwrap();
    let rounded = session.fillet(&post, &[&rim], 1.0).unwrap();
    let faces = session.subshape_count(&rounded, ShapeType::Face).unwrap();
    assert_eq!(agrees_with_tessellation(&session, &rounded, up), faces);

    // A fused stadium, a drilled block, and a sphere cut on a slant.
    let block = session
        .create_box(v(0.0, -5.0, 0.0), v(20.0, 10.0, 5.0))
        .unwrap();
    let round = session
        .create_cylinder(v(0.0, 0.0, 0.0), up, 5.0, 5.0)
        .unwrap();
    let stadium = session.fuse(&round, &block).unwrap();
    let faces = session.subshape_count(&stadium, ShapeType::Face).unwrap();
    assert_eq!(agrees_with_tessellation(&session, &stadium, up), faces);
    let bore = session
        .create_cylinder(v(10.0, 0.0, -1.0), v(0.3, 0.0, 1.0), 2.0, 10.0)
        .unwrap();
    let drilled = session.cut(&block, &bore).unwrap();
    let faces = session.subshape_count(&drilled, ShapeType::Face).unwrap();
    assert_eq!(
        agrees_with_tessellation(&session, &drilled, v(1.0, 1.0, 1.0)),
        faces
    );
    let ball = session.create_sphere(v(0.0, 0.0, 0.0), 3.0).unwrap();
    let cut = session
        .clip_by_plane(&ball, v(0.0, 0.0, 1.0), v(1.0, 0.0, 1.0), true)
        .unwrap();
    assert_eq!(agrees_with_tessellation(&session, &cut, up), 2);

    // A pocket under the sphere's south pole leaves its lowest point at one
    // corner of the trimmed face, which the closed form cannot place on the
    // face: the sphere is left unmeasured and the pocket's planes are exact.
    let under = session
        .create_box(v(-1.0, -4.0, -4.0), v(5.0, 8.0, 2.0))
        .unwrap();
    let pocketed = session.cut(&ball, &under).unwrap();
    let faces = session.subshape_count(&pocketed, ShapeType::Face).unwrap();
    assert_eq!(agrees_with_tessellation(&session, &pocketed, up), faces - 1);
    // Pulled along x, the extreme at the -x pole is cut away, so the sphere
    // is left unmeasured while the pocket's planes stay exact.
    let side = session
        .create_box(v(-4.0, -1.0, -1.0), v(2.0, 2.0, 2.0))
        .unwrap();
    let notched = session.cut(&ball, &side).unwrap();
    let ranges = session
        .face_pull_ranges(&notched, v(1.0, 0.0, 0.0))
        .unwrap();
    assert_eq!(ranges.iter().filter(|range| range.is_none()).count(), 1);
    assert_eq!(
        agrees_with_tessellation(&session, &notched, v(1.0, 0.0, 0.0)),
        ranges.len() - 1
    );

    assert!(session.face_pull_ranges(&ball, v(0.0, 0.0, 0.0)).is_err());
    assert!(
        session
            .face_pull_ranges(&ball, v(f64::NAN, 0.0, 1.0))
            .is_err()
    );
}
