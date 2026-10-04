//! Bounded closure workload with an independent crank-slider position oracle.
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn vector(x: f64) -> VectorQuantity {
    VectorQuantity::lengths(x, 0.0, 0.0, LengthUnit::Millimeter)
}
fn scalar(value: Quantity) -> JointScalar {
    JointScalar {
        value,
        minimum: None,
        maximum: None,
    }
}
fn main() {
    let definition = FamilyDefinition {
        references: Vec::new(),
        id: "link".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        features: vec![FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: VectorExpr::Literal(vector(0.0)),
                size: VectorExpr::Literal(VectorQuantity::lengths(
                    1.0,
                    1.0,
                    1.0,
                    LengthUnit::Millimeter,
                )),
            },
        }],
        requirements: vec![],
        datums: vec![
            DatumDefinition {
                id: "origin".into(),
                kind: DatumKind::Point {
                    origin: VectorExpr::Literal(vector(0.0)),
                },
            },
            DatumDefinition {
                id: "end".into(),
                kind: DatumKind::Point {
                    origin: VectorExpr::Literal(vector(3.0)),
                },
            },
        ],
    };
    let mut graph = InstanceGraph::new(&definition);
    for (frame, parent, offset, kind) in [
        (
            "crank",
            None,
            0.0,
            JointKind::Revolute {
                angle: scalar(Quantity::scalar(0.4)),
            },
        ),
        (
            "rod",
            Some("crank"),
            2.0,
            JointKind::Revolute {
                angle: scalar(Quantity::scalar(-0.8)),
            },
        ),
        (
            "slider",
            None,
            0.0,
            JointKind::Prismatic {
                distance: scalar(mm(4.0)),
            },
        ),
    ] {
        graph
            .add_frame(
                frame,
                parent,
                Placement::translated(vector(offset)),
                "bench",
            )
            .unwrap();
        graph
            .add_joint(AssemblyJoint {
                id: frame.into(),
                frame: frame.into(),
                origin: vector(offset),
                axis: if frame == "slider" {
                    VectorQuantity::scalars(1.0, 0.0, 0.0)
                } else {
                    VectorQuantity::scalars(0.0, 0.0, 1.0)
                },
                kind,
            })
            .unwrap();
    }
    for id in ["rod", "slider"] {
        graph.add_base(id, HashMap::new(), "bench").unwrap();
        graph.set_instance_frame(id, Some(id)).unwrap();
    }
    graph
        .add_relationship(AssemblyRelationship {
            id: "closure".into(),
            kind: RelationKind::Coincident,
            first: DatumRef::new("rod", "end"),
            second: DatumRef::new("slider", "origin"),
        })
        .unwrap();
    let variables = [
        JointVariable {
            frame: "rod".into(),
            coordinate: JointDof::Angle,
        },
        JointVariable {
            frame: "slider".into(),
            coordinate: JointDof::Axial,
        },
    ];
    let start = Instant::now();
    let mut iterations = 0;
    for index in 0..1000 {
        let angle = 0.2 + index as f64 / 1000.0;
        graph
            .set_joint_coordinate("crank", JointDof::Angle, Quantity::scalar(angle))
            .unwrap();
        let result = graph
            .solve_joint_coordinates(&variables, JointSolveOptions::default())
            .unwrap();
        assert!(result.solved, "closure {index}: {result:?}");
        let expected = 2.0 * angle.cos() + (9.0 - (2.0 * angle.sin()).powi(2)).sqrt();
        assert!((result.positions[1].value.value - expected).abs() < 1e-6);
        iterations += result.iterations;
    }
    let elapsed = start.elapsed();
    println!(
        "joint closure 1000 driven crank-slider poses: {:.3}s, {iterations} iterations (10s budget)",
        elapsed.as_secs_f64()
    );
    assert!(elapsed < Duration::from_secs(10));
    // Start the study from an assembled pose on the positive-slider branch.
    let mut initial = graph.clone();
    let angle = 0.2_f64;
    initial
        .set_joint_coordinate("crank", JointDof::Angle, Quantity::scalar(angle))
        .unwrap();
    initial
        .set_joint_coordinate(
            "rod",
            JointDof::Angle,
            Quantity::scalar(-(2.0 * angle.sin() / 3.0).asin() - angle),
        )
        .unwrap();
    initial
        .set_joint_coordinate(
            "slider",
            JointDof::Axial,
            Quantity::length(
                2.0 * angle.cos() + (9.0 - (2.0 * angle.sin()).powi(2)).sqrt(),
                LengthUnit::Millimeter,
            ),
        )
        .unwrap();
    closed_studies(&initial, &variables);
}
fn closed_studies(graph: &InstanceGraph<'_>, variables: &[JointVariable]) {
    let outputs = vec![
        InstanceOutputRef {
            instance: "rod".into(),
            output: "body".into(),
        },
        InstanceOutputRef {
            instance: "slider".into(),
            output: "body".into(),
        },
    ];
    let study = MotionStudy::linear(
        "crank",
        JointDof::Angle,
        Quantity::scalar(0.2),
        Quantity::scalar(1.2),
        10000,
        outputs,
        CollisionOptions::default(),
    )
    .unwrap();
    let start = Instant::now();
    let result = graph
        .solve_motion_study(&study, variables, ClosedMotionOptions::default())
        .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(result.status, JointMotionStatus::Complete);
    assert_eq!(result.solutions.len(), 10000);
    for (index, solution) in result.solutions.iter().enumerate() {
        let angle = study.samples[index].positions[0].value.value;
        let expected = 2.0 * angle.cos() + (9.0 - (2.0 * angle.sin()).powi(2)).sqrt();
        let distance = solution
            .positions
            .iter()
            .find(|position| position.frame == "slider")
            .unwrap()
            .value
            .value;
        assert!(
            (distance - expected).abs() < 1e-6,
            "sample {index}: distance {distance}, expected {expected}, positions {:?}",
            solution.positions
        );
    }
    println!(
        "closed motion 10000 analytic poses: {:.3}s (5s budget), {} iterations",
        elapsed.as_secs_f64(),
        result.iterations
    );
    assert!(elapsed < Duration::from_secs(5));
    let study = MotionStudy::linear(
        "crank",
        JointDof::Angle,
        Quantity::scalar(0.2),
        Quantity::scalar(1.2),
        1000,
        study.outputs,
        CollisionOptions::default(),
    )
    .unwrap();
    let session = occt_bridge::Session::new().unwrap();
    let start = Instant::now();
    let result = graph
        .run_closed_motion_study(&session, &study, variables, ClosedMotionOptions::default())
        .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(result.closure.status, JointMotionStatus::Complete);
    let motion = result.motion.unwrap();
    assert_eq!(motion.samples.len(), 1000);
    assert_eq!(motion.generated_variants, 1);
    assert!(
        motion
            .samples
            .iter()
            .flat_map(|sample| &sample.relationships)
            .all(|check| check.satisfied)
    );
    assert!(
        motion
            .samples
            .iter()
            .all(|sample| sample.collisions.is_empty())
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    println!(
        "closed motion 1000 poses with collision checks: {:.3}s (5s budget), one variant",
        elapsed.as_secs_f64()
    );
    assert!(elapsed < Duration::from_secs(5));
}
