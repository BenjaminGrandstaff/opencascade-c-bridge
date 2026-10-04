//! Swept rotating assemblies and independently checked unsampled collisions.
use occt_bridge::Session;
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn vector(x: f64, y: f64, z: f64) -> VectorQuantity {
    VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter)
}
fn definition() -> FamilyDefinition {
    FamilyDefinition {
        id: "cube".into(),
        version: 1,
        parameters: vec![ParameterDefinition {
            id: "size".into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(mm(1.0)),
            minimum: None,
            maximum: None,
        }],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        requirements: vec![],
        datums: vec![],
        features: vec![FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: VectorExpr::Literal(vector(0.0, 0.0, 0.0)),
                size: VectorExpr::Components {
                    x: ScalarExpr::Parameter("size".into()),
                    y: ScalarExpr::Parameter("size".into()),
                    z: ScalarExpr::Parameter("size".into()),
                },
            },
        }],
    }
}
fn joint(frame: &str, x: f64) -> AssemblyJoint {
    AssemblyJoint {
        id: frame.into(),
        frame: frame.into(),
        origin: vector(x, 0.0, 0.0),
        axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
        kind: JointKind::Revolute {
            angle: JointScalar {
                value: Quantity::scalar(0.0),
                minimum: None,
                maximum: None,
            },
        },
    }
}
fn output(id: &str) -> InstanceOutputRef {
    InstanceOutputRef {
        instance: id.into(),
        output: "body".into(),
    }
}
fn position(frame: &str, angle: f64) -> JointPosition {
    JointPosition {
        frame: frame.into(),
        coordinate: JointDof::Angle,
        value: Quantity::scalar(angle),
    }
}
fn sparse_rotors(definition: &FamilyDefinition) {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("source", HashMap::new(), "bench").unwrap();
    graph
        .set_placement("source", Placement::translated(vector(5.0, -0.5, 0.0)))
        .unwrap();
    let mut joints = Vec::new();
    let mut outputs = Vec::new();
    let mut first = Vec::new();
    let mut last = Vec::new();
    for index in 0..10000 {
        let frame = format!("rotor{index}");
        let instance = format!("part{index}");
        let x = index as f64 * 50.0;
        graph
            .add_clone(&instance, "source", HashMap::new(), "bench")
            .unwrap();
        graph
            .add_frame(
                &frame,
                None,
                Placement::translated(vector(x, 0.0, 0.0)),
                "bench",
            )
            .unwrap();
        graph.set_instance_frame(&instance, Some(&frame)).unwrap();
        joints.push(joint(&frame, x));
        outputs.push(output(&instance));
        first.push(position(&frame, 0.0));
        last.push(position(&frame, std::f64::consts::TAU));
    }
    graph.add_joints(joints).unwrap();
    let session = Session::new().unwrap();
    let study = MotionStudy {
        samples: vec![
            MotionSample { positions: first },
            MotionSample { positions: last },
        ],
        outputs,
        collision_options: Default::default(),
    };
    let start = Instant::now();
    let result = graph
        .check_continuous_motion(&session, &study, Default::default())
        .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(result.status, ContinuousStatus::Clear);
    assert_eq!(result.candidate_pairs, 0);
    assert_eq!(result.exact_queries, 0);
    assert_eq!(result.generated_variants, 1);
    assert_eq!(session.shape_count().unwrap(), 0);
    println!(
        "continuous rotation 10000 sparse rotors: {:.3}s (10s budget), one variant, zero pair queries",
        elapsed.as_secs_f64()
    );
    assert!(elapsed < Duration::from_secs(10));
}
fn dense_plates(definition: &FamilyDefinition) {
    let mut definition = definition.clone();
    definition.features[0].operation = FeatureOperation::Box {
        origin: VectorExpr::Literal(vector(0.0, 0.0, 0.0)),
        size: VectorExpr::Literal(vector(100.0, 100.0, 1.0)),
    };
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "bench").unwrap();
    let mut joints = Vec::new();
    let mut outputs = Vec::new();
    let mut first = Vec::new();
    let mut last = Vec::new();
    for index in 0..10000 {
        let id = format!("plate-{index}");
        graph
            .add_clone(&id, "source", HashMap::new(), "bench")
            .unwrap();
        graph
            .add_frame(
                &id,
                None,
                Placement::translated(vector(0.0, 0.0, index as f64 * 3.0)),
                "bench",
            )
            .unwrap();
        graph.set_instance_frame(&id, Some(&id)).unwrap();
        joints.push(joint(&id, 0.0));
        outputs.push(output(&id));
        first.push(position(&id, 0.0));
        last.push(position(
            &id,
            if index % 2 == 0 { 3.0 } else { -3.0 } * std::f64::consts::TAU,
        ));
    }
    graph.add_joints(joints).unwrap();
    let study = MotionStudy {
        samples: vec![
            MotionSample { positions: first },
            MotionSample { positions: last },
        ],
        outputs,
        collision_options: Default::default(),
    };
    let session = Session::new().unwrap();
    let started = Instant::now();
    let result = graph
        .check_continuous_motion(
            &session,
            &study,
            ContinuousCollisionOptions {
                maximum_candidate_pairs: 1,
                maximum_queries: 1,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.status, ContinuousStatus::Clear);
    assert_eq!(result.candidate_pairs, 0);
    assert_eq!(result.exact_queries, 0);
    assert_eq!(result.generated_variants, 1);
    assert_eq!(session.shape_count().unwrap(), 0);
    println!(
        "continuous rotation 10000 thin stacked plates: {:?} (10s budget), one variant, zero pair queries",
        started.elapsed()
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}
fn crossings(definition: &FamilyDefinition) {
    let session = Session::new().unwrap();
    let start = Instant::now();
    let mut queries = 0;
    for index in 0..1000 {
        let phi = 0.3 + index as f64 / 1000.0;
        let mut graph = InstanceGraph::new(definition);
        graph.add_base("moving", HashMap::new(), "bench").unwrap();
        graph
            .set_placement("moving", Placement::translated(vector(5.0, -0.5, 0.0)))
            .unwrap();
        graph
            .add_clone(
                "obstacle",
                "moving",
                HashMap::from([("size".into(), ParameterValue::Scalar(mm(0.05)))]),
                "bench",
            )
            .unwrap();
        graph
            .set_placement(
                "obstacle",
                Placement::translated(vector(
                    5.5 * phi.cos() - 0.025,
                    5.5 * phi.sin() - 0.025,
                    0.475,
                )),
            )
            .unwrap();
        graph
            .add_frame("rotor", None, Placement::identity(), "bench")
            .unwrap();
        graph.add_joint(joint("rotor", 0.0)).unwrap();
        graph.set_instance_frame("moving", Some("rotor")).unwrap();
        let study = MotionStudy::linear(
            "rotor",
            JointDof::Angle,
            Quantity::scalar(0.0),
            Quantity::scalar(std::f64::consts::TAU),
            2,
            vec![output("moving"), output("obstacle")],
            Default::default(),
        )
        .unwrap();
        let result = graph
            .check_continuous_motion(&session, &study, Default::default())
            .unwrap();
        assert_eq!(result.status, ContinuousStatus::Collision);
        assert!(result.pairs[0].fraction.unwrap() > 0.0 && result.pairs[0].fraction.unwrap() < 1.0);
        // Independent separating-axis test for the two projected boxes.
        // A box can overlap while its center is outside the other box.
        let angle = result.pairs[0].fraction.unwrap() * std::f64::consts::TAU;
        let dx = 5.5 * (phi.cos() - angle.cos());
        let dy = 5.5 * (phi.sin() - angle.sin());
        let projection = angle.cos().abs() + angle.sin().abs();
        let tolerance = 1e-6;
        assert!(
            (dx * angle.cos() + dy * angle.sin()).abs() <= 0.5 + 0.025 * projection + tolerance
        );
        assert!(
            (-dx * angle.sin() + dy * angle.cos()).abs() <= 0.5 + 0.025 * projection + tolerance
        );
        assert!(dx.abs() <= 0.025 + 0.5 * projection + tolerance);
        assert!(dy.abs() <= 0.025 + 0.5 * projection + tolerance);
        queries += result.exact_queries;
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    println!(
        "continuous rotation 1000 obstacle crossings: {:.3}s (30s budget), {queries} pair queries",
        elapsed.as_secs_f64()
    );
    assert!(elapsed < Duration::from_secs(30));
}
fn main() {
    let definition = definition();
    sparse_rotors(&definition);
    dense_plates(&definition);
    crossings(&definition);
}
