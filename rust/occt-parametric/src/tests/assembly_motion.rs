use super::*;

fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn scalar(value: Quantity) -> JointScalar {
    JointScalar {
        value,
        minimum: None,
        maximum: None,
    }
}
fn joint(frame: &str, kind: JointKind) -> AssemblyJoint {
    AssemblyJoint {
        id: format!("{frame}.joint"),
        frame: frame.into(),
        origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
        kind,
    }
}
fn output(instance: &str) -> InstanceOutputRef {
    InstanceOutputRef {
        instance: instance.into(),
        output: "body".into(),
    }
}
fn definition() -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition
}
fn bounds(graph: &InstanceGraph<'_>) -> occt_bridge::Bounds {
    let session = Session::new().unwrap();
    let result = graph
        .resolve_with_placement("moving")
        .unwrap()
        .regenerate(&session)
        .unwrap();
    session.exact_bounds(result.shape("body").unwrap()).unwrap()
}

#[test]
fn joints_cover_all_kinds_parent_coordinates_and_persistence() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame(
            "parent",
            None,
            Placement::translated(VectorQuantity::lengths(
                100.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            "test",
        )
        .unwrap();
    graph
        .add_frame("moving", Some("parent"), Placement::identity(), "test")
        .unwrap();
    graph.add_base("moving", HashMap::new(), "test").unwrap();
    graph.set_instance_frame("moving", Some("moving")).unwrap();
    let cases = [
        (JointKind::Fixed, Vec3::new(100.0, 0.0, 0.0)),
        (
            JointKind::Prismatic {
                distance: scalar(Quantity::length(2.0, LengthUnit::Centimeter)),
            },
            Vec3::new(120.0, 0.0, 0.0),
        ),
        (
            JointKind::Revolute {
                angle: scalar(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
            },
            Vec3::new(100.0, -30.0, 0.0),
        ),
        (
            JointKind::Cylindrical {
                angle: scalar(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
                distance: scalar(mm(20.0)),
            },
            Vec3::new(120.0, -30.0, 0.0),
        ),
        (
            JointKind::Planar {
                x_axis: VectorQuantity::scalars(0.0, 1.0, 0.0),
                x: scalar(mm(3.0)),
                y: scalar(mm(4.0)),
                angle: scalar(Quantity::scalar(0.0)),
            },
            Vec3::new(100.0, 3.0, 4.0),
        ),
    ];
    for (kind, minimum) in cases {
        graph.add_joint(joint("moving", kind)).unwrap();
        let result = bounds(&graph);
        assert!((result.min.x - minimum.x).abs() < 1e-6, "{result:?}");
        assert!((result.min.y - minimum.y).abs() < 1e-6, "{result:?}");
        assert!((result.min.z - minimum.z).abs() < 1e-6, "{result:?}");
        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
        assert_eq!(loaded.instance_graph().unwrap().joints().count(), 1);
        graph.remove_joint("moving").unwrap();
    }
    // Child axial translation is rotated by the parent joint.
    let mut parent = joint(
        "parent",
        JointKind::Revolute {
            angle: scalar(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
        },
    );
    parent.axis = VectorQuantity::scalars(0.0, 0.0, 1.0);
    graph
        .add_joints([
            parent,
            joint(
                "moving",
                JointKind::Prismatic {
                    distance: scalar(mm(20.0)),
                },
            ),
        ])
        .unwrap();
    let result = bounds(&graph);
    assert!((result.min.x + 20.0).abs() < 1e-6);
    assert!((result.min.y - 120.0).abs() < 1e-6);
}

#[test]
fn invalid_joint_edits_and_batches_preserve_the_accepted_state() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame("slide", None, Placement::identity(), "test")
        .unwrap();
    let bounded = JointScalar {
        value: mm(5.0),
        minimum: Some(mm(0.0)),
        maximum: Some(mm(10.0)),
    };
    graph
        .add_joint(joint("slide", JointKind::Prismatic { distance: bounded }))
        .unwrap();
    let accepted = ModelDocument::from_graph(&graph);
    for value in [mm(-1.0), mm(11.0), mm(f64::NAN), Quantity::scalar(2.0)] {
        assert!(
            graph
                .set_joint_coordinate("slide", JointDof::Axial, value)
                .is_err()
        );
        assert_eq!(ModelDocument::from_graph(&graph), accepted);
    }
    assert!(
        graph
            .set_joint_coordinate("slide", JointDof::Angle, Quantity::scalar(1.0))
            .is_err()
    );
    assert!(
        graph
            .add_joints([joint("missing", JointKind::Fixed)])
            .is_err()
    );
    assert!(graph.add_joint(joint("slide", JointKind::Fixed)).is_err());
    graph
        .add_frame("bad", None, Placement::identity(), "test")
        .unwrap();
    let mut invalid = joint("bad", JointKind::Fixed);
    invalid.axis = VectorQuantity::scalars(0.0, 0.0, 0.0);
    assert!(graph.add_joint(invalid).is_err());
    let mut invalid = joint("bad", JointKind::Fixed);
    invalid.origin = VectorQuantity::lengths(f64::INFINITY, 0.0, 0.0, LengthUnit::Millimeter);
    assert!(graph.add_joint(invalid).is_err());
    let invalid = joint(
        "bad",
        JointKind::Planar {
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            x: scalar(mm(0.0)),
            y: scalar(mm(0.0)),
            angle: scalar(Quantity::scalar(0.0)),
        },
    );
    assert!(graph.add_joint(invalid).is_err());
    graph
        .set_joint_coordinate("slide", JointDof::Axial, mm(10.0))
        .unwrap();
    assert_eq!(graph.joints().count(), 1);
}

#[test]
fn collision_bvh_matches_all_pairs_and_releases_query_temporaries() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    let locations = [0.0, 5.0, 10.0, 12.0, 30.0, 100.0, -10.0];
    let mut outputs = Vec::new();
    for (index, x) in locations.iter().enumerate() {
        let id = format!("part{index}");
        graph.add_base(&id, HashMap::new(), "test").unwrap();
        graph
            .set_placement(
                &id,
                Placement::translated(VectorQuantity::lengths(
                    *x,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            )
            .unwrap();
        outputs.push(output(&id));
    }
    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    let handles = session.shape_count().unwrap();
    let options = CollisionOptions {
        minimum_clearance: Quantity::length(0.3, LengthUnit::Centimeter),
        ..Default::default()
    };
    let indexed = generation
        .check_collisions(&session, &outputs, options)
        .unwrap();
    let mut exhaustive = Vec::new();
    for first in 0..outputs.len() {
        for second in first + 1..outputs.len() {
            let check = generation
                .check_pair(&session, &outputs[first], &outputs[second], options)
                .unwrap();
            if check.status != PairStatus::Clear {
                exhaustive.push(check);
            }
        }
    }
    assert_eq!(indexed, exhaustive);
    assert!(
        indexed
            .iter()
            .any(|check| check.status == PairStatus::Interference)
    );
    assert!(
        indexed
            .iter()
            .any(|check| check.status == PairStatus::InsufficientClearance)
    );
    let touching = generation
        .check_pair(
            &session,
            &outputs[0],
            &outputs[2],
            CollisionOptions::default(),
        )
        .unwrap();
    assert_eq!(touching.status, PairStatus::Touching);
    assert_eq!(touching.overlap_volume_mm3, 0.0);
    let overlap = generation
        .check_pair(
            &session,
            &outputs[0],
            &outputs[1],
            CollisionOptions::default(),
        )
        .unwrap();
    assert!((overlap.overlap_volume_mm3 - 3000.0).abs() < 1e-6);
    assert!(
        generation
            .check_collisions(&session, &[outputs[0].clone(), outputs[0].clone()], options)
            .is_err()
    );
    assert!(
        generation
            .check_collisions(&session, &[output("missing")], options)
            .is_err()
    );
    assert!(
        generation
            .check_collisions(
                &session,
                &outputs,
                CollisionOptions {
                    contact_tolerance: Quantity::scalar(0.0),
                    ..options
                }
            )
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), handles);
    drop(generation);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn motion_samples_reuse_local_geometry_report_crossings_and_preserve_graph() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("moving", HashMap::new(), "test").unwrap();
    graph
        .add_clone("obstacle", "moving", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "obstacle",
            Placement::translated(VectorQuantity::lengths(
                20.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph
        .add_frame("slide", None, Placement::identity(), "test")
        .unwrap();
    graph.set_instance_frame("moving", Some("slide")).unwrap();
    graph
        .add_joint(joint(
            "slide",
            JointKind::Prismatic {
                distance: JointScalar {
                    value: mm(0.0),
                    minimum: Some(mm(0.0)),
                    maximum: Some(mm(40.0)),
                },
            },
        ))
        .unwrap();
    let accepted = ModelDocument::from_graph(&graph);
    let session = Session::new().unwrap();
    let study = MotionStudy::linear(
        "slide",
        JointDof::Axial,
        mm(0.0),
        Quantity::length(4.0, LengthUnit::Centimeter),
        5,
        vec![output("moving"), output("obstacle")],
        CollisionOptions::default(),
    )
    .unwrap();
    let result = graph.run_motion_study(&session, &study).unwrap();
    assert_eq!(result.generated_variants, 1);
    assert_eq!(result.samples.len(), 5);
    assert!(result.samples[0].collisions.is_empty());
    assert_eq!(result.samples[1].collisions[0].status, PairStatus::Touching);
    assert_eq!(
        result.samples[2].collisions[0].status,
        PairStatus::Interference
    );
    assert_eq!(result.samples[3].collisions[0].status, PairStatus::Touching);
    assert!(result.samples[4].collisions.is_empty());
    assert_eq!(ModelDocument::from_graph(&graph), accepted);
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut invalid = study.clone();
    invalid.samples[4].positions[0].value = mm(41.0);
    assert!(
        graph
            .run_motion_study(&session, &invalid)
            .unwrap_err()
            .message
            .contains("sample 4")
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    invalid.samples[4].positions[0].value = mm(40.0);
    let repeated = invalid.samples[0].positions[0].clone();
    invalid.samples[0].positions.push(repeated);
    assert!(graph.run_motion_study(&session, &invalid).is_err());
    invalid = study.clone();
    invalid.outputs[0].output = "missing".into();
    assert!(graph.run_motion_study(&session, &invalid).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(
        MotionStudy::linear(
            "slide",
            JointDof::Axial,
            mm(0.0),
            mm(1.0),
            MAX_MOTION_SAMPLES + 1,
            study.outputs,
            CollisionOptions::default()
        )
        .is_err()
    );
}
