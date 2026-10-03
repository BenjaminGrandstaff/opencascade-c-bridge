use super::*;
use crate::assembly::tests::{assert_point, block, relationship, stacked, translated};
use std::f64::consts::FRAC_PI_6;

fn turned(angle: f64, x: f64, y: f64, z: f64) -> Placement {
    Placement {
        translation: VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter),
        rotation: Some(AxisAngle {
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            angle_radians: angle,
        }),
    }
}

/// Seats `b` on `a`: faces touch, axes align, and side faces stay parallel.
fn seat(graph: &mut InstanceGraph<'_>, upper: &str, lower: &str) {
    for (id, kind, first, second) in [
        ("seated", RelationKind::Coincident, "top", "bottom"),
        ("aligned", RelationKind::Coincident, "axis", "axis"),
        ("square", RelationKind::Parallel, "right", "right"),
    ] {
        graph
            .add_relationship(relationship(
                &format!("{upper}-{id}"),
                kind,
                (lower, first),
                (upper, second),
            ))
            .unwrap();
    }
}

#[test]
fn fully_constrained_instance_snaps_into_place() {
    let definition = block();
    let mut graph = stacked(&definition);
    graph
        .set_placement("b", turned(FRAC_PI_6, 40.0, -15.0, 70.0))
        .unwrap();
    seat(&mut graph, "b", "a");

    let solution = graph.solve_placements(&["b"]).unwrap();
    assert!(solution.solved, "{solution:?}");
    assert_eq!(solution.free_degrees, 0);
    assert!(solution.max_residual < 1e-9);
    assert_point(graph.datum("b", "top_center").unwrap(), (5.0, 10.0, 60.0));
    let ResolvedDatum::Plane { normal, .. } = graph.datum("b", "right").unwrap() else {
        panic!("right is a plane");
    };
    assert!(length(cross(normal, Vec3::new(1.0, 0.0, 0.0))) < 1e-9);
    assert!(
        graph
            .check_relationships()
            .unwrap()
            .iter()
            .all(|check| check.satisfied)
    );
}

#[test]
fn under_constrained_freedoms_keep_their_current_values() {
    let definition = block();
    let mut graph = stacked(&definition);
    graph
        .set_placement("b", translated(40.0, -15.0, 70.0))
        .unwrap();
    graph
        .add_relationship(relationship(
            "seated",
            RelationKind::Coincident,
            ("a", "top"),
            ("b", "bottom"),
        ))
        .unwrap();

    let solution = graph.solve_placements(&["b"]).unwrap();
    assert!(solution.solved, "{solution:?}");
    // Sliding in X and Y and turning about Z remain free.
    assert_eq!(solution.free_degrees, 3);
    assert_point(graph.datum("b", "top_center").unwrap(), (45.0, -5.0, 60.0));
}

#[test]
fn conflicting_relationships_leave_the_graph_unchanged() {
    let definition = block();
    let mut graph = stacked(&definition);
    let distance =
        |millimeters| RelationKind::Distance(Quantity::length(millimeters, LengthUnit::Millimeter));
    graph
        .add_relationship(relationship(
            "near",
            distance(30.0),
            ("a", "top"),
            ("b", "top"),
        ))
        .unwrap();
    graph
        .add_relationship(relationship(
            "far",
            distance(50.0),
            ("a", "top"),
            ("b", "top"),
        ))
        .unwrap();
    let before = graph.node("b").unwrap().placement();

    let solution = graph.solve_placements(&["b"]).unwrap();
    assert!(!solution.solved);
    assert!(solution.redundant_equations > 0);
    assert!(solution.checks.iter().any(|check| !check.satisfied));
    assert_eq!(graph.node("b").unwrap().placement(), before);
}

#[test]
fn chains_of_free_instances_solve_together() {
    let definition = block();
    let mut graph = stacked(&definition);
    graph.add_clone("c", "a", HashMap::new(), "test").unwrap();
    graph
        .set_placement("b", turned(0.4, -30.0, 12.0, 5.0))
        .unwrap();
    graph
        .set_placement("c", turned(-0.7, 25.0, 40.0, -8.0))
        .unwrap();
    seat(&mut graph, "b", "a");
    seat(&mut graph, "c", "b");

    let solution = graph.solve_placements(&["b", "c"]).unwrap();
    assert!(solution.solved, "{solution:?}");
    assert_eq!(solution.free_degrees, 0);
    assert_point(graph.datum("c", "top_center").unwrap(), (5.0, 10.0, 90.0));
}

#[test]
fn free_instances_inside_frames_are_solved_in_model_coordinates() {
    let definition = block();
    let mut graph = stacked(&definition);
    graph
        .add_frame("shelf", None, turned(FRAC_PI_6, 200.0, 0.0, 0.0), "layout")
        .unwrap();
    graph.set_instance_frame("b", Some("shelf")).unwrap();
    seat(&mut graph, "b", "a");

    let solution = graph.solve_placements(&["b"]).unwrap();
    assert!(solution.solved, "{solution:?}");
    assert_point(graph.datum("b", "top_center").unwrap(), (5.0, 10.0, 60.0));
    assert_eq!(graph.node("b").unwrap().frame(), Some("shelf"));
}

/// Far from the origin, positions are checked to the relationship
/// tolerance; doubles cannot hold 1e-9 mm at meter-scale coordinates.
fn assert_near(datum: ResolvedDatum, expected: (f64, f64, f64)) {
    let ResolvedDatum::Point { origin } = datum else {
        panic!("expected a point, got {datum:?}");
    };
    let error = length(subtract(
        origin,
        Vec3::new(expected.0, expected.1, expected.2),
    ));
    assert!(
        error < RELATIONSHIP_LINEAR_TOLERANCE,
        "{origin:?} != {expected:?}"
    );
}

/// Rotation about the model origin made rotation and translation nearly
/// interchangeable for distant parts; this stack used to stall tilted.
#[test]
fn coincident_stack_far_from_the_origin_converges() {
    let definition = block();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("i0", HashMap::new(), "test").unwrap();
    graph
        .set_placement("i0", translated(1000.0, -500.0, 0.0))
        .unwrap();
    let mut free = Vec::new();
    for index in 1..=5 {
        let id = format!("i{index}");
        graph
            .add_clone(id.clone(), "i0", HashMap::new(), "test")
            .unwrap();
        graph
            .set_placement(
                &id,
                translated(1000.0 + index as f64 * 3.0, -502.0, index as f64 * 25.0),
            )
            .unwrap();
        graph
            .add_relationship(relationship(
                &format!("r{index}"),
                RelationKind::Coincident,
                (&format!("i{}", index - 1), "top"),
                (&id, "bottom"),
            ))
            .unwrap();
        free.push(id);
    }
    let ids = free.iter().map(String::as_str).collect::<Vec<_>>();

    let solution = graph.solve_placements(&ids).unwrap();
    assert!(solution.solved, "{solution:?}");
    // Each block keeps its X/Y offset and rotation about Z free.
    assert_eq!(solution.free_degrees, 15);
    for index in 1..=5 {
        assert_near(
            graph.datum(&format!("i{index}"), "top_center").unwrap(),
            (
                1005.0 + index as f64 * 3.0,
                -492.0,
                30.0 * (index as f64 + 1.0),
            ),
        );
    }
}

#[test]
fn fully_constrained_seat_far_from_the_origin_converges() {
    let definition = block();
    let mut graph = stacked(&definition);
    graph
        .set_placement("a", translated(5000.0, -3000.0, 200.0))
        .unwrap();
    graph
        .set_placement("b", turned(FRAC_PI_6, 5040.0, -3015.0, 270.0))
        .unwrap();
    seat(&mut graph, "b", "a");

    let solution = graph.solve_placements(&["b"]).unwrap();
    assert!(solution.solved, "{solution:?}");
    assert_eq!(solution.free_degrees, 0);
    assert_near(
        graph.datum("b", "top_center").unwrap(),
        (5005.0, -2990.0, 260.0),
    );
}

#[test]
fn solving_rejects_invalid_free_sets() {
    let definition = block();
    let mut graph = stacked(&definition);
    assert!(graph.solve_placements(&[]).is_err());
    assert!(graph.solve_placements(&["b", "b"]).is_err());
    assert!(graph.solve_placements(&["missing"]).is_err());
    let error = graph.solve_placements(&["b"]).unwrap_err();
    assert!(error.message.contains("no relationship"), "{error}");
}
