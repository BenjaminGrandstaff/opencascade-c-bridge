use occt_parametric::*;
use std::{collections::HashMap, time::Instant};
fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn vector(x: f64, y: f64) -> VectorQuantity {
    VectorQuantity::lengths(x, y, 0.0, LengthUnit::Millimeter)
}
fn slider_array(
    definition: &FamilyDefinition,
    count: usize,
    chain: bool,
) -> (InstanceGraph<'_>, Vec<JointVariable>) {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("prototype", HashMap::new(), "test").unwrap();
    graph
        .add_frame(
            "mounted",
            None,
            Placement {
                translation: vector(1e6, -1e6),
                rotation: Some(AxisAngle {
                    origin: vector(0.0, 0.0),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: 0.4,
                }),
            },
            "test",
        )
        .unwrap();
    let mut joints = Vec::new();
    let mut variables = Vec::new();
    for index in 0..count {
        let frame = format!("slide-{index}");
        let parent = if chain && index > 0 {
            format!("slide-{}", index - 1)
        } else {
            "mounted".into()
        };
        graph
            .add_frame(&frame, Some(&parent), Placement::identity(), "test")
            .unwrap();
        joints.push(AssemblyJoint {
            id: frame.clone(),
            frame: frame.clone(),
            origin: vector(0.0, 0.0),
            axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            kind: JointKind::Prismatic {
                distance: JointScalar {
                    value: Quantity::length(0.02, LengthUnit::Centimeter),
                    minimum: Some(mm(0.0)),
                    maximum: Some(mm(1.0)),
                },
            },
        });
        variables.push(JointVariable {
            frame: frame.clone(),
            coordinate: JointDof::Axial,
        });
        let fixed = format!("fixed-{index}");
        for id in [&frame, &fixed] {
            graph
                .add_clone(id, "prototype", HashMap::new(), "test")
                .unwrap();
        }
        graph.set_instance_frame(&frame, Some(&frame)).unwrap();
        graph.set_instance_frame(&fixed, Some("mounted")).unwrap();
        graph
            .set_placement(
                &fixed,
                Placement::translated(vector(if chain { index as f64 + 1.0 } else { 1.0 }, 0.0)),
            )
            .unwrap();
    }
    graph.add_joints(joints).unwrap();
    for index in 0..count {
        graph
            .add_relationship(AssemblyRelationship {
                id: format!("closure-{index}"),
                kind: RelationKind::Coincident,
                first: DatumRef::new(format!("slide-{index}"), "origin"),
                second: DatumRef::new(format!("fixed-{index}"), "origin"),
            })
            .unwrap();
    }
    (graph, variables)
}

fn main() {
    let definition = FamilyDefinition {
        id: "sliders".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        requirements: vec![],
        features: vec![],
        datums: vec![DatumDefinition {
            id: "origin".into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(vector(0.0, 0.0)),
            },
        }],
    };
    for (count, chain, budget) in [(10000, false, 10.0), (64, true, 10.0)] {
        let (mut graph, variables) = slider_array(&definition, count, chain);
        let started = Instant::now();
        let result = graph
            .solve_joint_coordinates(&variables, Default::default())
            .unwrap();
        assert!(result.solved);
        assert_eq!(result.free_degrees, 0);
        assert_eq!(result.redundant_equations, count * 2);
        assert!(
            result
                .positions
                .iter()
                .all(|position| match position.value.unit {
                    Some(LengthUnit::Millimeter) => (position.value.value - 1.0).abs() < 1e-6,
                    Some(LengthUnit::Centimeter) => (position.value.value - 0.1).abs() < 1e-7,
                    _ => false,
                })
        );
        println!(
            "sparse joint closure {count} coordinates, chain={chain}: {:?} ({budget}s budget), {} iterations",
            started.elapsed(),
            result.iterations
        );
        assert!(started.elapsed().as_secs_f64() < budget);
    }
}
