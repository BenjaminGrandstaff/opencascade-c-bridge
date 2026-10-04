use super::*;
fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn vector(x: f64, y: f64, z: f64) -> VectorQuantity {
    VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter)
}
fn scalar(value: Quantity) -> JointScalar {
    JointScalar {
        value,
        minimum: None,
        maximum: None,
    }
}
fn output(id: &str) -> InstanceOutputRef {
    InstanceOutputRef {
        instance: id.into(),
        output: "body".into(),
    }
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
fn rotor(definition: &FamilyDefinition, obstacle: Vec3) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("moving", HashMap::new(), "test").unwrap();
    graph
        .add_clone("obstacle", "moving", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement("moving", Placement::translated(vector(5.0, -0.5, 0.0)))
        .unwrap();
    graph
        .set_placement(
            "obstacle",
            Placement::translated(vector(
                obstacle.x - 0.025,
                obstacle.y - 0.025,
                obstacle.z - 0.025,
            )),
        )
        .unwrap();
    for id in ["width", "depth", "height"] {
        graph
            .set_override("obstacle", id, ParameterValue::Scalar(mm(0.05)))
            .unwrap();
    }
    graph
        .add_frame("rotor", None, Placement::identity(), "test")
        .unwrap();
    graph
        .add_joint(AssemblyJoint {
            id: "rotor".into(),
            frame: "rotor".into(),
            origin: vector(0.0, 0.0, 0.0),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            kind: JointKind::Revolute {
                angle: scalar(Quantity::scalar(0.0)),
            },
        })
        .unwrap();
    graph.set_instance_frame("moving", Some("rotor")).unwrap();
    graph
}
fn study(start: f64, end: f64) -> MotionStudy {
    MotionStudy::linear(
        "rotor",
        JointDof::Angle,
        Quantity::scalar(start),
        Quantity::scalar(end),
        2,
        vec![output("moving"), output("obstacle")],
        CollisionOptions::default(),
    )
    .unwrap()
}
#[test]
fn rotating_full_turn_and_reverse_multiturn_find_unsampled_obstacles() {
    let definition = definition();
    let session = Session::new().unwrap();
    let phi = 0.713_f64;
    let graph = rotor(
        &definition,
        Vec3::new(5.5 * phi.cos(), 5.5 * phi.sin(), 0.5),
    );
    let accepted = graph
        .regenerate_instances_current(&session, &["moving", "obstacle"])
        .unwrap();
    let count = session.shape_count().unwrap();
    let document = ModelDocument::from_graph(&graph);
    for (start, end) in [
        (0.0, std::f64::consts::TAU),
        (0.2, 0.2 - 2.0 * std::f64::consts::TAU),
    ] {
        let study = study(start, end);
        assert!(
            graph
                .run_motion_study(&session, &study)
                .unwrap()
                .samples
                .iter()
                .all(|sample| sample.collisions.is_empty())
        );
        assert!(
            graph
                .check_translation_motion(&session, &study, Default::default())
                .is_err()
        );
        let result = graph
            .check_continuous_motion(&session, &study, Default::default())
            .unwrap();
        assert_eq!(result.status, ContinuousStatus::Collision, "{result:?}");
        let fraction = result.pairs[0].fraction.unwrap();
        assert!(fraction > 0.0 && fraction < 1.0);
        // Rebuild the witnessed pose through the independent sampled motion API.
        let witness = study_at_angle(start + (end - start) * fraction);
        assert!(
            !graph.run_motion_study(&session, &witness).unwrap().samples[0]
                .collisions
                .is_empty()
        );
        assert_eq!(session.shape_count().unwrap(), count);
        assert_eq!(ModelDocument::from_graph(&graph), document);
    }
    assert!(
        session
            .is_valid(accepted.result("moving").unwrap().shape("body").unwrap())
            .unwrap()
    );
    drop(accepted);
    assert_eq!(session.shape_count().unwrap(), 0);
}
fn study_at_angle(angle: f64) -> MotionStudy {
    MotionStudy {
        samples: vec![MotionSample {
            positions: vec![JointPosition {
                frame: "rotor".into(),
                coordinate: JointDof::Angle,
                value: Quantity::scalar(angle),
            }],
        }],
        outputs: vec![output("moving"), output("obstacle")],
        excluded_pairs: Vec::new(),
        collision_options: Default::default(),
    }
}

#[test]
fn rotating_clear_paths_and_exhausted_budgets_preserve_state() {
    let definition = definition();
    let session = Session::new().unwrap();
    let graph = rotor(&definition, Vec3::new(0.0, 0.0, 0.5));
    let study = study(0.0, std::f64::consts::TAU);
    let accepted = ModelDocument::from_graph(&graph);
    let result = graph
        .check_continuous_motion(&session, &study, Default::default())
        .unwrap();
    assert_eq!(result.status, ContinuousStatus::Clear, "{result:?}");
    assert!(result.exact_queries > 2);
    let result = graph
        .check_continuous_motion(
            &session,
            &study,
            ContinuousCollisionOptions {
                maximum_queries: 1,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.status, ContinuousStatus::Unresolved);
    assert_eq!(result.exact_queries, 1);
    assert_eq!(session.shape_count().unwrap(), 0);
    assert_eq!(ModelDocument::from_graph(&graph), accepted);
    let large_angles = graph
        .check_continuous_motion(
            &session,
            &study_at_large_angles(),
            ContinuousCollisionOptions {
                maximum_queries: 1,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(large_angles.status, ContinuousStatus::Unresolved);
    let far = rotor(&definition, Vec3::new(100.0, 100.0, 0.5));
    assert_eq!(
        far.check_continuous_motion(&session, &study, Default::default())
            .unwrap()
            .candidate_pairs,
        0
    );
}

#[test]
fn nested_planar_and_cylindrical_paths_match_an_independent_pose_oracle() {
    let definition = definition();
    let session = Session::new().unwrap();
    let fraction = 0.375_f64;
    let theta = 0.3 + 0.8 * fraction;
    let phi = std::f64::consts::PI * fraction;
    let local = (
        5.0 + 0.5 * theta.cos(),
        0.5 * theta.sin(),
        0.5 + 2.0 * fraction,
    );
    let parent = (
        local.0 * phi.cos() - local.1 * phi.sin() + 0.3 * fraction,
        local.0 * phi.sin() + local.1 * phi.cos() - 0.2 * fraction,
        local.2,
    );
    let world_angle = 0.4_f64;
    let far = 1_000_000.0;
    let obstacle = Vec3::new(
        parent.0 + far,
        parent.1 * world_angle.cos() - parent.2 * world_angle.sin() + far,
        parent.1 * world_angle.sin() + parent.2 * world_angle.cos() + far,
    );
    let mut graph = rotor(&definition, obstacle);
    graph
        .set_placement("moving", Placement::translated(vector(0.0, -0.5, 0.0)))
        .unwrap();
    graph
        .add_frame(
            "world",
            None,
            Placement {
                translation: vector(far, far, far),
                rotation: Some(AxisAngle {
                    origin: vector(0.0, 0.0, 0.0),
                    axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                    angle_radians: world_angle,
                }),
            },
            "test",
        )
        .unwrap();
    graph.frames.get_mut("rotor").unwrap().parent = Some("world".into());
    graph.assembly.joints.get_mut("rotor").unwrap().kind = JointKind::Planar {
        x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
        x: scalar(mm(0.0)),
        y: scalar(mm(0.0)),
        angle: scalar(Quantity::scalar(0.0)),
    };
    graph
        .add_frame(
            "child",
            Some("rotor"),
            Placement::translated(vector(5.0, 0.0, 0.0)),
            "test",
        )
        .unwrap();
    graph
        .add_joint(AssemblyJoint {
            id: "child".into(),
            frame: "child".into(),
            origin: vector(5.0, 0.0, 0.0),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            kind: JointKind::Cylindrical {
                angle: scalar(Quantity::scalar(0.3)),
                distance: scalar(mm(0.0)),
            },
        })
        .unwrap();
    graph.set_instance_frame("moving", Some("child")).unwrap();
    let sample = |f: f64| MotionSample {
        positions: vec![
            JointPosition {
                frame: "rotor".into(),
                coordinate: JointDof::Angle,
                value: Quantity::scalar(std::f64::consts::PI * f),
            },
            JointPosition {
                frame: "rotor".into(),
                coordinate: JointDof::PlanarX,
                value: mm(0.3 * f),
            },
            JointPosition {
                frame: "rotor".into(),
                coordinate: JointDof::PlanarY,
                value: mm(-0.2 * f),
            },
            JointPosition {
                frame: "child".into(),
                coordinate: JointDof::Angle,
                value: Quantity::scalar(0.3 + 0.8 * f),
            },
            JointPosition {
                frame: "child".into(),
                coordinate: JointDof::Axial,
                value: mm(2.0 * f),
            },
        ],
    };
    let study = MotionStudy {
        samples: vec![sample(0.0), sample(1.0)],
        outputs: vec![output("moving"), output("obstacle")],
        excluded_pairs: Vec::new(),
        collision_options: Default::default(),
    };
    assert!(
        graph
            .run_motion_study(&session, &study)
            .unwrap()
            .samples
            .iter()
            .all(|sample| sample.collisions.is_empty())
    );
    let result = graph
        .check_continuous_motion(&session, &study, Default::default())
        .unwrap();
    assert_eq!(result.status, ContinuousStatus::Collision, "{result:?}");
    let witness = MotionStudy {
        samples: vec![sample(result.pairs[0].fraction.unwrap())],
        ..study
    };
    assert!(
        !graph.run_motion_study(&session, &witness).unwrap().samples[0]
            .collisions
            .is_empty()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn rotating_invalid_and_overflowing_paths_release_geometry() {
    let definition = definition();
    let session = Session::new().unwrap();
    let mut graph = rotor(&definition, Vec3::new(10.0, 10.0, 0.5));
    let accepted = ModelDocument::from_graph(&graph);
    for study in [study(-1e308, 1e308), study(0.0, 1e308)] {
        assert!(
            graph
                .check_continuous_motion(&session, &study, Default::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
        assert_eq!(ModelDocument::from_graph(&graph), accepted);
    }
    if let JointKind::Revolute { angle } = &mut graph.assembly.joints.get_mut("rotor").unwrap().kind
    {
        angle.maximum = Some(Quantity::scalar(1.0));
    }
    assert!(
        graph
            .check_continuous_motion(&session, &study(0.0, 2.0), Default::default())
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut invalid = study(0.0, 0.5);
    invalid.samples[1].positions[0].value = mm(0.5);
    assert!(
        graph
            .check_continuous_motion(&session, &invalid, Default::default())
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn study_at_large_angles() -> MotionStudy {
    study(1e16, 1e16 + 2.0)
}

#[test]
fn opposite_rotating_bodies_collide_between_clear_endpoints() {
    let definition = definition();
    let session = Session::new().unwrap();
    let mut graph = rotor(&definition, Vec3::new(0.0, 0.0, 0.0));
    for id in ["width", "depth", "height"] {
        graph
            .set_override("obstacle", id, ParameterValue::Scalar(mm(1.0)))
            .unwrap();
    }
    graph
        .set_placement("obstacle", Placement::translated(vector(-6.0, -0.5, 0.0)))
        .unwrap();
    graph
        .add_frame(
            "opposite",
            None,
            Placement::translated(vector(8.0, 0.0, 0.0)),
            "test",
        )
        .unwrap();
    graph
        .add_joint(AssemblyJoint {
            id: "opposite".into(),
            frame: "opposite".into(),
            origin: vector(8.0, 0.0, 0.0),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            kind: JointKind::Revolute {
                angle: scalar(Quantity::scalar(0.0)),
            },
        })
        .unwrap();
    graph
        .set_instance_frame("obstacle", Some("opposite"))
        .unwrap();
    let sample = |fraction: f64| MotionSample {
        positions: vec![
            JointPosition {
                frame: "rotor".into(),
                coordinate: JointDof::Angle,
                value: Quantity::scalar(std::f64::consts::TAU * fraction),
            },
            JointPosition {
                frame: "opposite".into(),
                coordinate: JointDof::Angle,
                value: Quantity::scalar(-std::f64::consts::TAU * fraction),
            },
        ],
    };
    let study = MotionStudy {
        samples: vec![sample(0.0), sample(1.0)],
        outputs: vec![output("moving"), output("obstacle")],
        excluded_pairs: Vec::new(),
        collision_options: Default::default(),
    };
    assert!(
        graph
            .run_motion_study(&session, &study)
            .unwrap()
            .samples
            .iter()
            .all(|sample| sample.collisions.is_empty())
    );
    let result = graph
        .check_continuous_motion(&session, &study, Default::default())
        .unwrap();
    assert_eq!(result.status, ContinuousStatus::Collision);
    let witness = MotionStudy {
        samples: vec![sample(result.pairs[0].fraction.unwrap())],
        ..study
    };
    assert!(
        !graph.run_motion_study(&session, &witness).unwrap().samples[0]
            .collisions
            .is_empty()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn thin_rotating_plate_stack_rejects_false_dense_candidates_and_preserves_graph() {
    let mut definition = definition();
    definition.features[0].operation = FeatureOperation::Box {
        origin: VectorExpr::Literal(vector(0.0, 0.0, 0.0)),
        size: VectorExpr::Literal(vector(100.0, 100.0, 1.0)),
    };
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    let mut joints = vec![];
    let mut outputs = vec![];
    let mut first = vec![];
    let mut last = vec![];
    for index in 0..1000 {
        let id = format!("plate-{index}");
        graph
            .add_clone(&id, "source", HashMap::new(), "test")
            .unwrap();
        graph
            .add_frame(
                &id,
                None,
                Placement::translated(vector(0.0, 0.0, index as f64 * 3.0)),
                "test",
            )
            .unwrap();
        graph.set_instance_frame(&id, Some(&id)).unwrap();
        joints.push(AssemblyJoint {
            id: id.clone(),
            frame: id.clone(),
            origin: vector(0.0, 0.0, 0.0),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            kind: JointKind::Revolute {
                angle: scalar(Quantity::scalar(0.0)),
            },
        });
        outputs.push(output(&id));
        first.push(JointPosition {
            frame: id.clone(),
            coordinate: JointDof::Angle,
            value: Quantity::scalar(0.0),
        });
        last.push(JointPosition {
            frame: id,
            coordinate: JointDof::Angle,
            value: Quantity::scalar(
                if index % 2 == 0 { 3.0 } else { -3.0 } * std::f64::consts::TAU,
            ),
        });
    }
    graph.add_joints(joints).unwrap();
    let before = ModelDocument::from_graph(&graph);
    let session = Session::new().unwrap();
    let study = MotionStudy {
        samples: vec![
            MotionSample { positions: first },
            MotionSample { positions: last },
        ],
        outputs,
        excluded_pairs: Vec::new(),
        collision_options: Default::default(),
    };
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
    assert_eq!(ModelDocument::from_graph(&graph), before);
}

#[test]
fn interval_boxes_prune_clear_ring_interiors_and_detect_axial_crossings() {
    let definition = definition();
    let session = Session::new().unwrap();
    let graph = rotor(&definition, Vec3::new(0.0, 0.0, 0.5));
    let result = graph
        .check_continuous_motion(
            &session,
            &study(0.0, std::f64::consts::TAU),
            Default::default(),
        )
        .unwrap();
    assert_eq!(result.status, ContinuousStatus::Clear);
    assert_eq!(result.candidate_pairs, 1);
    assert!(result.bounds_rejected_intervals > 0, "{result:?}");
    let outside = rotor(&definition, Vec3::new(10.0, 10.0, 0.5));
    let bounded = outside
        .check_continuous_motion(
            &session,
            &study(0.0, std::f64::consts::TAU),
            ContinuousCollisionOptions {
                maximum_queries: 1,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(bounded.status, ContinuousStatus::Clear);
    assert_eq!(bounded.candidate_pairs, 0);
    assert_eq!(bounded.exact_queries, 0);
    assert!(result.exact_queries < 20, "{result:?}");
    // A cylinder's changing axial coordinate must remain in the swept box.
    let mut graph = rotor(&definition, Vec3::new(5.5, 0.0, 2.0));
    graph.assembly.joints.get_mut("rotor").unwrap().kind = JointKind::Cylindrical {
        angle: scalar(Quantity::scalar(0.0)),
        distance: scalar(mm(0.0)),
    };
    let mut travel = study(0.0, std::f64::consts::TAU);
    travel.samples[0].positions.push(JointPosition {
        frame: "rotor".into(),
        coordinate: JointDof::Axial,
        value: mm(0.0),
    });
    travel.samples[1].positions.push(JointPosition {
        frame: "rotor".into(),
        coordinate: JointDof::Axial,
        value: mm(3.0),
    });
    // At the half turn the body is on the opposite side of the rotation axis.
    graph
        .set_placement(
            "obstacle",
            Placement::translated(vector(-5.5 - 0.025, -0.025, 2.0 - 0.025)),
        )
        .unwrap();
    let result = graph
        .check_continuous_motion(&session, &travel, Default::default())
        .unwrap();
    assert_eq!(result.status, ContinuousStatus::Collision, "{result:?}");
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn shared_carrier<'a>(definition: &'a FamilyDefinition, spacing: f64) -> InstanceGraph<'a> {
    let mut graph = rotor(definition, Vec3::new(0.0, 0.0, 0.0));
    graph
        .set_placement("moving", Placement::identity())
        .unwrap();
    graph
        .set_placement("obstacle", Placement::identity())
        .unwrap();
    for id in ["width", "depth", "height"] {
        graph
            .set_override("obstacle", id, ParameterValue::Scalar(mm(1.0)))
            .unwrap();
    }
    graph
        .add_frame(
            "mount",
            Some("rotor"),
            Placement::translated(vector(spacing, 0.0, 0.0)),
            "test",
        )
        .unwrap();
    graph.set_instance_frame("obstacle", Some("mount")).unwrap();
    graph
}
#[test]
fn shared_rotating_carrier_uses_initial_bounds_through_fixed_child_mounts() {
    let definition = definition();
    let graph = shared_carrier(&definition, 3.0);
    let session = Session::new().unwrap();
    let before = session.shape_count().unwrap();
    let result = graph
        .check_continuous_motion(
            &session,
            &study(0.0, 4.0 * std::f64::consts::TAU),
            ContinuousCollisionOptions {
                maximum_queries: 1,
                maximum_candidate_pairs: 1,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.status, ContinuousStatus::Clear);
    assert_eq!(result.candidate_pairs, 0);
    assert_eq!(result.exact_queries, 0);
    assert_eq!(result.generated_variants, 1);
    assert_eq!(session.shape_count().unwrap(), before);
}
#[test]
fn shared_carrier_preserves_interference_contact_and_clearance_failures() {
    let definition = definition();
    let session = Session::new().unwrap();
    for (spacing, clearance, expected) in [
        (0.5, 0.0, PairStatus::Interference),
        (1.0, 0.0, PairStatus::Touching),
        (1.5, 1.0, PairStatus::InsufficientClearance),
    ] {
        let graph = shared_carrier(&definition, spacing);
        let mut study = study(0.0, std::f64::consts::TAU);
        study.collision_options.minimum_clearance = mm(clearance);
        let result = graph
            .check_continuous_motion(
                &session,
                &study,
                ContinuousCollisionOptions {
                    maximum_queries: 1,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.status, ContinuousStatus::Collision);
        assert_eq!(result.exact_queries, 1);
        assert_eq!(result.pairs[0].fraction, Some(0.0));
        assert_eq!(result.pairs[0].check.as_ref().unwrap().status, expected);
    }
}
#[test]
fn shared_carrier_one_query_certifies_separation_but_keeps_guard_uncertainty() {
    let definition = definition();
    let session = Session::new().unwrap();
    let graph = shared_carrier(&definition, 1.5);
    for (guard, expected) in [
        (0.6, ContinuousStatus::Unresolved),
        (0.4, ContinuousStatus::Clear),
    ] {
        let result = graph
            .check_continuous_motion(
                &session,
                &study(
                    std::f64::consts::FRAC_PI_4,
                    std::f64::consts::FRAC_PI_4 + std::f64::consts::TAU,
                ),
                ContinuousCollisionOptions {
                    distance_guard: mm(guard),
                    maximum_queries: 1,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.status, expected);
        assert_eq!(result.exact_queries, 1);
    }
}

#[test]
fn rotating_excluded_pair_remains_excluded_between_clear_samples() {
    let definition = definition();
    let phi = 0.713_f64;
    let graph = rotor(
        &definition,
        Vec3::new(5.5 * phi.cos(), 5.5 * phi.sin(), 0.5),
    );
    let session = Session::new().unwrap();
    let mut study = study(0.0, std::f64::consts::TAU);
    assert_eq!(
        graph
            .check_continuous_motion(&session, &study, Default::default())
            .unwrap()
            .status,
        ContinuousStatus::Collision
    );
    study.excluded_pairs = vec![CollisionPairRef {
        first: output("moving"),
        second: output("obstacle"),
    }];
    let result = graph
        .check_continuous_motion(&session, &study, Default::default())
        .unwrap();
    assert_eq!(result.status, ContinuousStatus::Clear);
    assert_eq!(result.exact_queries, 0);
    assert_eq!(result.candidate_pairs, 0);
    assert_eq!(session.shape_count().unwrap(), 0);
}
