use super::linkage::{definition, mechanism, variables};
use super::*;
fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn output(id: &str) -> InstanceOutputRef {
    InstanceOutputRef {
        instance: id.into(),
        output: "body".into(),
    }
}
fn options() -> ClosedMotionOptions {
    ClosedMotionOptions {
        joint_solver: JointSolveOptions {
            characteristic_length: mm(10.0),
            ..Default::default()
        },
        ..Default::default()
    }
}
fn study(start: f64, end: f64) -> MotionStudy {
    MotionStudy::linear(
        "crank",
        JointDof::Angle,
        Quantity::scalar(start),
        Quantity::scalar(end),
        3,
        vec![output("slider")],
        Default::default(),
    )
    .unwrap()
}
fn slider(angle: f64) -> f64 {
    20.0 * angle.cos() + (900.0 - (20.0 * angle.sin()).powi(2)).sqrt()
}
fn solved_slider(solution: &JointSolution) -> f64 {
    solution
        .positions
        .iter()
        .find(|position| position.frame == "slider")
        .unwrap()
        .value
        .normalized()
        .unwrap()
}
fn motion_definition() -> FamilyDefinition {
    let mut definition = definition(10.0);
    for parameter in &mut definition.parameters {
        parameter.default = ParameterValue::Scalar(mm(1.0));
    }
    definition
}
#[test]
fn closed_motion_solves_analytic_poses_before_shared_geometry_collision_checks() {
    let definition = motion_definition();
    let mut graph = mechanism(&definition, 10.0, 0.0);
    graph
        .add_clone("obstacle", "slider", HashMap::new(), "test")
        .unwrap();
    // Remove the inherited moving frame, then park the obstacle at the middle pose.
    graph.set_instance_frame("obstacle", None).unwrap();
    graph
        .set_placement(
            "obstacle",
            Placement::translated(VectorQuantity::lengths(
                slider(0.8),
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    let mut study = study(0.4, 1.2);
    study.outputs.push(output("obstacle"));
    let session = Session::new().unwrap();
    let accepted = graph
        .regenerate_instances_current(&session, &["rod", "slider", "obstacle"])
        .unwrap();
    let handles = session.shape_count().unwrap();
    let document = ModelDocument::from_graph(&graph);
    let result = graph
        .run_closed_motion_study(&session, &study, &variables(), options())
        .unwrap();
    assert_eq!(result.closure.status, JointMotionStatus::Complete);
    assert_eq!(result.closure.failed_sample, None);
    assert_eq!(result.closure.solutions.len(), 3);
    let closed = result.closure.closed_study.as_ref().unwrap();
    for (index, solution) in result.closure.solutions.iter().enumerate() {
        let angle = 0.4 + 0.4 * index as f64;
        assert!((solved_slider(solution) - slider(angle)).abs() < 1e-6);
        assert!(solution.checks.iter().all(|check| check.satisfied));
        assert_eq!(
            closed.samples[index].positions[0],
            study.samples[index].positions[0]
        );
        assert_eq!(closed.samples[index].positions.len(), 3);
    }
    let motion = result.motion.unwrap();
    assert_eq!(motion.generated_variants, 1);
    assert!(motion.samples[0].collisions.is_empty());
    assert_eq!(motion.samples[1].collisions.len(), 1);
    assert_eq!(
        motion.samples[1].collisions[0].status,
        PairStatus::Interference
    );
    assert!(motion.samples[2].collisions.is_empty());
    assert!(
        motion
            .samples
            .iter()
            .flat_map(|sample| &sample.relationships)
            .all(|check| check.satisfied)
    );
    assert_eq!(session.shape_count().unwrap(), handles);
    assert_eq!(ModelDocument::from_graph(&graph), document);
    assert!(
        session
            .is_valid(accepted.result("slider").unwrap().shape("body").unwrap())
            .unwrap()
    );
    drop(accepted);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn closed_motion_reports_late_limit_failure_without_generating_geometry() {
    let mut definition = motion_definition();
    definition
        .features
        .iter_mut()
        .find(|feature| feature.id == "body")
        .unwrap()
        .operation = FeatureOperation::Box {
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        size: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
    };
    let mut graph = mechanism(&definition, 10.0, 0.0);
    if let JointKind::Prismatic { distance } =
        &mut graph.assembly.joints.get_mut("slider").unwrap().kind
    {
        distance.maximum = Some(mm(45.0));
    }
    let session = Session::new().unwrap();
    let document = ModelDocument::from_graph(&graph);
    let result = graph
        .run_closed_motion_study(&session, &study(1.2, 0.4), &variables(), options())
        .unwrap();
    assert_eq!(result.closure.status, JointMotionStatus::ClosureFailed);
    assert_eq!(result.closure.failed_sample, Some(2));
    assert_eq!(result.closure.solutions.len(), 3);
    assert!(
        result.closure.solutions[..2]
            .iter()
            .all(|solution| solution.solved)
    );
    assert!(!result.closure.solutions[2].solved);
    assert!(solved_slider(&result.closure.solutions[2]) <= 45.0);
    assert!(result.closure.closed_study.is_none());
    assert!(result.motion.is_none());
    assert_eq!(session.shape_count().unwrap(), 0);
    assert_eq!(ModelDocument::from_graph(&graph), document);
    // Had the runner generated this intentionally invalid body, it would fail.
    assert!(
        graph
            .regenerate_instances_current(&session, &["slider"])
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn closed_motion_budget_and_invalid_driver_roles_preserve_state() {
    let definition = motion_definition();
    let graph = mechanism(&definition, 10.0, 0.0);
    let session = Session::new().unwrap();
    let document = ModelDocument::from_graph(&graph);
    let study = study(0.4, 1.2);
    let result = graph
        .run_closed_motion_study(
            &session,
            &study,
            &variables(),
            ClosedMotionOptions {
                maximum_total_iterations: 1,
                ..options()
            },
        )
        .unwrap();
    assert_eq!(result.closure.status, JointMotionStatus::BudgetExceeded);
    assert_eq!(result.closure.failed_sample, Some(0));
    assert_eq!(result.closure.iterations, 1);
    assert!(result.motion.is_none());
    for invalid_options in [
        ClosedMotionOptions {
            maximum_total_iterations: 0,
            ..options()
        },
        ClosedMotionOptions {
            maximum_total_iterations: 1_000_001,
            ..options()
        },
        ClosedMotionOptions {
            joint_solver: JointSolveOptions {
                maximum_iterations: 1001,
                ..Default::default()
            },
            maximum_total_iterations: 1,
        },
    ] {
        assert!(
            graph
                .solve_motion_study(&study, &variables(), invalid_options)
                .is_err()
        );
    }
    let mut invalid = study.clone();
    invalid.samples[2].positions.push(JointPosition {
        frame: "slider".into(),
        coordinate: JointDof::Axial,
        value: mm(30.0),
    });
    assert!(
        graph
            .solve_motion_study(&invalid, &variables(), options())
            .is_err()
    );
    invalid = study.clone();
    invalid.samples[2].positions[0].value = mm(0.4);
    assert!(
        graph
            .solve_motion_study(&invalid, &variables(), options())
            .is_err()
    );
    assert!(graph.solve_motion_study(&study, &[], options()).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
    assert_eq!(ModelDocument::from_graph(&graph), document);
}
#[test]
fn closed_motion_omitted_drivers_reset_to_original_and_budget_stops_later_poses() {
    let definition = motion_definition();
    let mut graph = mechanism(&definition, 10.0, 0.0);
    let mut study = study(0.4, 1.2);
    study.samples[1].positions.clear();
    let result = graph
        .solve_motion_study(&study, &variables(), options())
        .unwrap();
    assert_eq!(result.status, JointMotionStatus::Complete);
    assert!(
        (solved_slider(&result.solutions[1]) - slider(std::f64::consts::FRAC_PI_3)).abs() < 1e-6
    );
    let first_iterations = result.solutions[0].iterations;
    assert!(first_iterations > 0);
    let limited = graph
        .solve_motion_study(
            &study,
            &variables(),
            ClosedMotionOptions {
                maximum_total_iterations: first_iterations,
                ..options()
            },
        )
        .unwrap();
    assert_eq!(limited.status, JointMotionStatus::BudgetExceeded);
    assert_eq!(limited.failed_sample, Some(1));
    assert_eq!(limited.solutions.len(), 1);
    assert!(limited.closed_study.is_none());
    assert!(
        graph
            .solve_joint_coordinates(&variables(), options().joint_solver)
            .unwrap()
            .solved
    );
    let one = MotionStudy {
        samples: vec![MotionSample { positions: vec![] }],
        ..study
    };
    let result = graph
        .solve_motion_study(
            &one,
            &variables(),
            ClosedMotionOptions {
                maximum_total_iterations: 1,
                ..options()
            },
        )
        .unwrap();
    assert_eq!(result.status, JointMotionStatus::Complete);
    assert_eq!(result.iterations, 0);
}
#[test]
fn closed_motion_kernel_errors_release_temporary_geometry() {
    let definition = motion_definition();
    let graph = mechanism(&definition, 10.0, 0.0);
    let session = Session::new().unwrap();
    let document = ModelDocument::from_graph(&graph);
    let mut study = study(0.4, 1.2);
    study.outputs[0].output = "missing".into();
    assert_eq!(
        graph
            .solve_motion_study(&study, &variables(), options())
            .unwrap()
            .status,
        JointMotionStatus::Complete
    );
    assert!(
        graph
            .run_closed_motion_study(&session, &study, &variables(), options())
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    assert_eq!(ModelDocument::from_graph(&graph), document);
}

#[test]
fn closed_motion_continues_the_initial_assembly_branch() {
    let definition = motion_definition();
    let session = Session::new().unwrap();
    let mut graph = mechanism(&definition, 10.0, 0.0);
    graph
        .set_joint_coordinate("crank", JointDof::Angle, Quantity::scalar(0.2))
        .unwrap();
    graph
        .set_joint_coordinate("rod", JointDof::Angle, Quantity::scalar(-3.2))
        .unwrap();
    graph
        .set_joint_coordinate("slider", JointDof::Axial, mm(-10.0))
        .unwrap();
    let study = study(0.2, 1.2);
    let result = graph
        .solve_motion_study(&study, &variables(), options())
        .unwrap();
    assert_eq!(result.status, JointMotionStatus::Complete);
    for (index, solution) in result.solutions.iter().enumerate() {
        let angle = 0.2 + 0.5 * index as f64;
        let expected = 20.0 * angle.cos() - (900.0 - (20.0 * angle.sin()).powi(2)).sqrt();
        assert!((solved_slider(solution) - expected).abs() < 1e-6);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn closed_linkage_studies_preserve_explicit_collision_exclusions() {
    let definition = motion_definition();
    let mut graph = mechanism(&definition, 10.0, 0.0);
    graph
        .add_clone("obstacle", "slider", HashMap::new(), "test")
        .unwrap();
    graph
        .set_instance_frame("obstacle", Some("slider"))
        .unwrap();
    let mut study = study(0.4, 1.2);
    study.outputs.push(output("obstacle"));
    study.excluded_pairs.push(CollisionPairRef {
        first: output("slider"),
        second: output("obstacle"),
    });
    let solved = graph
        .solve_motion_study(&study, &variables(), options())
        .unwrap();
    assert_eq!(solved.status, JointMotionStatus::Complete);
    let closed = solved.closed_study.unwrap();
    assert_eq!(closed.excluded_pairs, study.excluded_pairs);
    let session = Session::new().unwrap();
    let result = graph.run_motion_study(&session, &closed).unwrap();
    assert!(
        result
            .samples
            .iter()
            .all(|sample| sample.collisions.is_empty())
    );
    let mut unfiltered = closed;
    unfiltered.excluded_pairs.clear();
    assert!(
        graph
            .run_motion_study(&session, &unfiltered)
            .unwrap()
            .samples
            .iter()
            .all(|sample| !sample.collisions.is_empty())
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
