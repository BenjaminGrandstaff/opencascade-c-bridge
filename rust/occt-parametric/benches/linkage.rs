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
}
