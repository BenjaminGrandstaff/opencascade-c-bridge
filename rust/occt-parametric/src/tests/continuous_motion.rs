use super::*;
fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn definition() -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Advisory, 1e6);
    definition.requirements.clear();
    for parameter in &mut definition.parameters {
        parameter.default = ParameterValue::Scalar(mm(1.0));
        parameter.minimum = None;
    }
    definition
}
fn graph(definition: &FamilyDefinition, obstacle: Vec3) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("moving", HashMap::new(), "test").unwrap();
    graph
        .add_clone("obstacle", "moving", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "obstacle",
            Placement::translated(VectorQuantity::lengths(
                obstacle.x,
                obstacle.y,
                obstacle.z,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph
        .add_frame("slide", None, Placement::identity(), "test")
        .unwrap();
    graph.set_instance_frame("moving", Some("slide")).unwrap();
    graph
        .add_joint(AssemblyJoint {
            id: "joint".into(),
            frame: "slide".into(),
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            kind: JointKind::Prismatic {
                distance: JointScalar {
                    value: mm(0.0),
                    minimum: None,
                    maximum: None,
                },
            },
        })
        .unwrap();
    graph
}
fn output(id: &str) -> InstanceOutputRef {
    InstanceOutputRef {
        instance: id.into(),
        output: "body".into(),
    }
}
fn study(start: f64, end: f64) -> MotionStudy {
    MotionStudy::linear(
        "slide",
        JointDof::Axial,
        mm(start),
        mm(end),
        2,
        vec![output("moving"), output("obstacle")],
        CollisionOptions::default(),
    )
    .unwrap()
}
#[test]
fn continuous_check_catches_thin_obstacle_missed_by_endpoints() {
    let definition = definition();
    let mut graph = graph(&definition, Vec3::new(17.37, 0.0, 0.0));
    graph
        .set_override("obstacle", "width", ParameterValue::Scalar(mm(0.01)))
        .unwrap();
    let study = study(0.0, 40.0);
    let session = Session::new().unwrap();
    let before = ModelDocument::from_graph(&graph);
    let sampled = graph.run_motion_study(&session, &study).unwrap();
    assert!(
        sampled
            .samples
            .iter()
            .all(|sample| sample.collisions.is_empty())
    );
    let accepted = graph
        .regenerate_instances_current(&session, &["moving", "obstacle"])
        .unwrap();
    let handles = session.shape_count().unwrap();
    let report = graph
        .check_translation_motion(&session, &study, ContinuousCollisionOptions::default())
        .unwrap();
    assert_eq!(
        graph
            .check_continuous_motion(&session, &study, ContinuousCollisionOptions::default())
            .unwrap(),
        report
    );
    assert_eq!(report.status, ContinuousStatus::Collision);
    assert_eq!(report.generated_variants, 2);
    assert_eq!(report.unresolved_pairs, 0);
    assert_eq!(report.candidate_pairs, 1);
    let pair = &report.pairs[0];
    assert_eq!(pair.segment, 0);
    assert_eq!(
        pair.check.as_ref().unwrap().status,
        PairStatus::Interference
    );
    let x = pair.fraction.unwrap() * 40.0;
    assert!(x + 1.0 >= 17.37 && x <= 17.38);
    assert_eq!(session.shape_count().unwrap(), handles);
    assert_eq!(ModelDocument::from_graph(&graph), before);
    assert!(
        session
            .is_valid(accepted.result("moving").unwrap().shape("body").unwrap())
            .unwrap()
    );
    let mut strict = study.clone();
    strict.collision_options.contact_tolerance = mm(0.0);
    let strict_report = graph
        .check_translation_motion(&session, &strict, ContinuousCollisionOptions::default())
        .unwrap();
    assert_eq!(strict_report.status, ContinuousStatus::Collision);
    drop(accepted);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn analytic_translation_slab_oracle_matches_clear_and_colliding_paths() {
    let definition = definition();
    let session = Session::new().unwrap();
    // Independent interval oracle for moving and static axis-aligned unit boxes.
    for (start, end, obstacle, y) in [
        (0.0, 40.0, 17.37, 0.0),
        (40.0, 0.0, 7.13, 0.0),
        (-40.0, 0.0, -11.11, 0.0),
        (0.0, 40.0, 17.37, 3.0),
        (0.0, 4.0, 17.37, 0.0),
        (40.0, 0.0, 17.37, 2.0),
        (0.0, 40.0, 0.0, 0.0),
        (0.0, 40.0, 40.0, 0.0),
    ] {
        let graph = graph(&definition, Vec3::new(obstacle, y, 0.0));
        let velocity = end - start;
        let a = (obstacle - 1.0 - start) / velocity;
        let b = (obstacle + 1.0 - start) / velocity;
        let hits = a.min(b).max(0.0) <= a.max(b).min(1.0) && y.abs() <= 1.0;
        let report = graph
            .check_translation_motion(
                &session,
                &study(start, end),
                ContinuousCollisionOptions::default(),
            )
            .unwrap();
        assert_eq!(
            report.status,
            if hits {
                ContinuousStatus::Collision
            } else {
                ContinuousStatus::Clear
            },
            "{start} to {end}, obstacle {obstacle},{y}"
        );
        assert_eq!(report.unresolved_pairs, 0);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
#[test]
fn nested_rotated_frames_and_clearance_are_checked_in_world_coordinates() {
    let definition = definition();
    let mut graph = graph(&definition, Vec3::new(0.2, 17.37, 0.0));
    graph
        .add_frame(
            "parent",
            None,
            Placement {
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: std::f64::consts::FRAC_PI_2,
                }),
                translation: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            },
            "test",
        )
        .unwrap();
    graph.frames.get_mut("slide").unwrap().parent = Some("parent".into());
    let mut study = study(0.0, 40.0);
    study.collision_options.minimum_clearance = mm(0.5);
    let session = Session::new().unwrap();
    let report = graph
        .check_translation_motion(&session, &study, ContinuousCollisionOptions::default())
        .unwrap();
    assert_eq!(report.status, ContinuousStatus::Collision);
    let check = report.pairs[0].check.as_ref().unwrap();
    assert_eq!(check.status, PairStatus::InsufficientClearance);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn query_depth_guard_and_grazing_budgets_never_claim_clear() {
    let definition = definition();
    let graph = graph(&definition, Vec3::new(17.37, 0.0, 0.0));
    let session = Session::new().unwrap();
    for options in [
        ContinuousCollisionOptions {
            maximum_queries: 1,
            ..Default::default()
        },
        ContinuousCollisionOptions {
            maximum_depth: 1,
            ..Default::default()
        },
        ContinuousCollisionOptions {
            minimum_interval_fraction: 1.0,
            ..Default::default()
        },
        ContinuousCollisionOptions {
            distance_guard: mm(100.0),
            maximum_depth: 1,
            ..Default::default()
        },
    ] {
        let report = graph
            .check_translation_motion(&session, &study(0.0, 40.0), options)
            .unwrap();
        assert_eq!(report.status, ContinuousStatus::Unresolved);
        assert_eq!(report.unresolved_pairs, 1);
        assert!(report.pairs[0].check.is_none());
        assert!(report.pairs[0].unresolved_fraction_range.is_some());
        assert!(report.exact_queries <= options.maximum_queries);
    }
    let nearby = super::continuous_motion::graph(&definition, Vec3::new(1.0 + 2e-7, 0.0, 0.0));
    let uncertain = nearby
        .check_translation_motion(
            &session,
            &study(0.0, 0.0),
            ContinuousCollisionOptions::default(),
        )
        .unwrap();
    assert_eq!(uncertain.status, ContinuousStatus::Unresolved);
    // Corner grazing at fraction 0.4, with a diagonal translation.
    let mut grazing = graph.clone();
    grazing.assembly.joints.get_mut("slide").unwrap().axis = VectorQuantity::scalars(1.0, 1.0, 0.0);
    grazing
        .set_placement(
            "obstacle",
            Placement::translated(VectorQuantity::lengths(
                17.0,
                15.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    let report = grazing
        .check_translation_motion(
            &session,
            &study(0.0, 40.0 * 2.0_f64.sqrt()),
            ContinuousCollisionOptions {
                maximum_depth: 3,
                ..Default::default()
            },
        )
        .unwrap();
    assert_ne!(report.status, ContinuousStatus::Clear);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn comoving_static_and_piecewise_motion_preserve_relative_geometry() {
    let definition = definition();
    let mut graph = graph(&definition, Vec3::new(3.0, 0.0, 0.0));
    graph.set_instance_frame("obstacle", Some("slide")).unwrap();
    let session = Session::new().unwrap();
    let report = graph
        .check_translation_motion(
            &session,
            &study(0.0, 40.0),
            ContinuousCollisionOptions::default(),
        )
        .unwrap();
    assert_eq!(report.status, ContinuousStatus::Clear);
    assert_eq!(report.exact_queries, 0);
    assert_eq!(report.candidate_pairs, 0);
    assert_eq!(report.generated_variants, 1);
    graph.set_instance_frame("obstacle", None).unwrap();
    graph
        .set_placement(
            "obstacle",
            Placement::translated(VectorQuantity::lengths(
                17.37,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    let mut piecewise = study(0.0, 40.0);
    piecewise.samples.insert(
        1,
        MotionSample {
            positions: vec![JointPosition {
                frame: "slide".into(),
                coordinate: JointDof::Axial,
                value: mm(2.0),
            }],
        },
    );
    let report = graph
        .check_translation_motion(&session, &piecewise, ContinuousCollisionOptions::default())
        .unwrap();
    assert_eq!(report.segments, 2);
    assert_eq!(report.status, ContinuousStatus::Collision);
    assert_eq!(report.pairs[0].segment, 1);
    let static_clear = graph
        .check_translation_motion(
            &session,
            &study(0.0, 0.0),
            ContinuousCollisionOptions::default(),
        )
        .unwrap();
    assert_eq!(static_clear.status, ContinuousStatus::Clear);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn invalid_studies_rotation_limits_and_broadphase_budgets_release_handles() {
    let definition = definition();
    let mut graph = graph(&definition, Vec3::new(17.37, 0.0, 0.0));
    let session = Session::new().unwrap();
    for options in [
        ContinuousCollisionOptions {
            maximum_queries: 0,
            ..Default::default()
        },
        ContinuousCollisionOptions {
            maximum_candidate_pairs: 0,
            ..Default::default()
        },
        ContinuousCollisionOptions {
            maximum_depth: 53,
            ..Default::default()
        },
        ContinuousCollisionOptions {
            minimum_interval_fraction: 0.0,
            ..Default::default()
        },
        ContinuousCollisionOptions {
            minimum_interval_fraction: f64::NAN,
            ..Default::default()
        },
        ContinuousCollisionOptions {
            distance_guard: Quantity::scalar(1.0),
            ..Default::default()
        },
        ContinuousCollisionOptions {
            distance_guard: mm(-1.0),
            ..Default::default()
        },
    ] {
        assert!(
            graph
                .check_translation_motion(&session, &study(0.0, 40.0), options)
                .is_err()
        );
    }
    for index in 0..6 {
        let mut invalid = study(0.0, 40.0);
        match index {
            0 => invalid.samples.pop().map(|_| ()).unwrap(),
            1 => invalid.outputs.clear(),
            2 => invalid.outputs.push(output("moving")),
            3 => invalid.outputs[0].output = "missing".into(),
            4 => {
                let duplicate = invalid.samples[0].positions[0].clone();
                invalid.samples[0].positions.push(duplicate);
            }
            _ => invalid.collision_options.contact_tolerance = Quantity::scalar(1.0),
        };
        assert!(
            graph
                .check_translation_motion(&session, &invalid, ContinuousCollisionOptions::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    graph
        .add_clone("extra", "obstacle", HashMap::new(), "test")
        .unwrap();
    let mut dense = study(0.0, 40.0);
    dense.outputs.push(output("extra"));
    assert!(
        graph
            .check_translation_motion(
                &session,
                &dense,
                ContinuousCollisionOptions {
                    maximum_candidate_pairs: 1,
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    graph.assembly.joints.get_mut("slide").unwrap().kind = JointKind::Revolute {
        angle: JointScalar {
            value: Quantity::scalar(0.0),
            minimum: None,
            maximum: None,
        },
    };
    let rotating = MotionStudy::linear(
        "slide",
        JointDof::Angle,
        Quantity::scalar(0.0),
        Quantity::scalar(std::f64::consts::TAU),
        2,
        vec![output("moving"), output("obstacle")],
        CollisionOptions::default(),
    )
    .unwrap();
    assert!(
        graph
            .check_translation_motion(&session, &rotating, ContinuousCollisionOptions::default())
            .err()
            .unwrap()
            .message
            .contains("angular")
    );
    let mut constant = rotating.clone();
    constant.samples[1] = constant.samples[0].clone();
    assert!(
        graph
            .check_translation_motion(&session, &constant, ContinuousCollisionOptions::default())
            .is_ok()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
