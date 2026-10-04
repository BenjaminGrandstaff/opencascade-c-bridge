//! Bounded closure workload with an independent crank-slider position oracle.
use occt_parametric::*;
use std::{collections::HashMap, time::Instant};
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

    let axes = vec![
        JointSeedAxis::linear(
            JointVariable {
                frame: "rod".into(),
                coordinate: JointDof::Angle,
            },
            Quantity::scalar(-std::f64::consts::PI),
            Quantity::scalar(std::f64::consts::PI),
            31,
        )
        .unwrap(),
        JointSeedAxis::linear(
            JointVariable {
                frame: "slider".into(),
                coordinate: JointDof::Axial,
            },
            mm(-5.0),
            mm(5.0),
            31,
        )
        .unwrap(),
    ];
    let before = ModelDocument::from_graph(&graph);
    let started = Instant::now();
    let result = graph
        .search_joint_branches(&axes, Default::default())
        .unwrap();
    assert_eq!(result.status, JointBranchSearchStatus::SeedsExhausted);
    assert_eq!(result.attempted_starts, 962);
    assert_eq!(result.branches.len(), 2);
    let center = 2.0 * 0.4_f64.cos();
    let radius = (9.0 - (2.0 * 0.4_f64.sin()).powi(2)).sqrt();
    for branch in &result.branches {
        let x = branch.positions[1].value.value;
        assert!((x - center - radius).abs() < 1e-6 || (x - center + radius).abs() < 1e-6);
        assert!(branch.checks.iter().all(|check| check.satisfied));
    }
    assert_eq!(ModelDocument::from_graph(&graph), before);
    println!(
        "joint branch discovery 962 starts: {:?} (10s budget), two analytic branches, {} iterations",
        started.elapsed(),
        result.iterations
    );
    assert!(started.elapsed().as_secs_f64() < 10.0);
}
