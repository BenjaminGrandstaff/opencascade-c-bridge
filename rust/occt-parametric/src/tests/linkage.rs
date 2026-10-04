use super::*;
fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn vector(x: f64, y: f64) -> VectorQuantity {
    VectorQuantity::lengths(x, y, 0.0, LengthUnit::Millimeter)
}
fn scalar(value: Quantity) -> JointScalar {
    JointScalar {
        value,
        minimum: None,
        maximum: None,
    }
}
fn free(frame: &str, coordinate: JointDof) -> JointVariable {
    JointVariable {
        frame: frame.into(),
        coordinate,
    }
}
pub(super) fn definition(size: f64) -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Required, 100000.0);
    definition.requirements.clear();
    definition.datums = vec![
        DatumDefinition {
            id: "origin".into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(vector(0.0, 0.0)),
            },
        },
        DatumDefinition {
            id: "end".into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(vector(3.0 * size, 0.0)),
            },
        },
    ];
    definition
}
pub(super) fn mechanism(definition: &FamilyDefinition, size: f64, world: f64) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(definition);
    graph
        .add_frame(
            "world",
            None,
            Placement::translated(vector(world, world)),
            "test",
        )
        .unwrap();
    for (frame, parent, offset, origin, kind) in [
        (
            "crank",
            "world",
            0.0,
            0.0,
            JointKind::Revolute {
                angle: scalar(Quantity::scalar(std::f64::consts::FRAC_PI_3)),
            },
        ),
        (
            "rod",
            "crank",
            2.0 * size,
            2.0 * size,
            JointKind::Revolute {
                angle: scalar(Quantity::scalar(-1.5)),
            },
        ),
        (
            "slider",
            "world",
            0.0,
            0.0,
            JointKind::Prismatic {
                distance: scalar(mm(3.0 * size)),
            },
        ),
    ] {
        graph
            .add_frame(
                frame,
                Some(parent),
                Placement::translated(vector(offset, 0.0)),
                "test",
            )
            .unwrap();
        graph
            .add_joint(AssemblyJoint {
                id: frame.into(),
                frame: frame.into(),
                origin: vector(origin, 0.0),
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
        graph.add_base(id, HashMap::new(), "test").unwrap();
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
    graph
}
pub(super) fn variables() -> Vec<JointVariable> {
    vec![
        free("rod", JointDof::Angle),
        free("slider", JointDof::Axial),
    ]
}

#[test]
fn linkage_closes_crank_slider_and_preserves_driver_across_scales() {
    for (size, world) in [(0.01, 0.0), (1.0, 0.0), (1000.0, 0.0), (1.0, 1_000_000.0)] {
        let definition = definition(size);
        let mut graph = mechanism(&definition, size, world);
        let driver = graph.assembly.joints["crank"].clone();
        let result = graph
            .solve_joint_coordinates(
                &variables(),
                JointSolveOptions {
                    characteristic_length: mm(size),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(result.solved, "{result:?}");
        assert_eq!(result.free_degrees, 0);
        assert_eq!(graph.assembly.joints["crank"], driver);
        let x = graph.datum("slider", "origin").unwrap();
        if let ResolvedDatum::Point { origin } = x {
            assert!((origin.x - world - size * (1.0 + 6.0_f64.sqrt())).abs() < 1e-6);
        } else {
            panic!()
        }
        graph
            .set_joint_coordinate("crank", JointDof::Angle, Quantity::scalar(0.8))
            .unwrap();
        let result = graph
            .solve_joint_coordinates(
                &variables(),
                JointSolveOptions {
                    characteristic_length: mm(size),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(result.solved, "{result:?}");
        assert_eq!(
            graph.assembly.joints["crank"].kind,
            JointKind::Revolute {
                angle: scalar(Quantity::scalar(0.8))
            }
        );
        assert!(
            graph
                .check_relationships()
                .unwrap()
                .iter()
                .all(|check| check.satisfied)
        );
    }
}

#[test]
fn linkage_limits_and_conflicts_leave_graph_unchanged() {
    let definition = definition(1.0);
    let mut graph = mechanism(&definition, 1.0, 0.0);
    if let JointKind::Prismatic { distance } =
        &mut graph.assembly.joints.get_mut("slider").unwrap().kind
    {
        distance.maximum = Some(mm(3.0));
    }
    let accepted = ModelDocument::from_graph(&graph);
    let result = graph
        .solve_joint_coordinates(&variables(), Default::default())
        .unwrap();
    assert!(!result.solved);
    assert_eq!(ModelDocument::from_graph(&graph), accepted);
    assert!(
        result
            .positions
            .iter()
            .find(|position| position.frame == "slider")
            .unwrap()
            .value
            .value
            <= 3.0
    );
    assert!(
        result
            .active_limits
            .contains(&free("slider", JointDof::Axial))
    );
    // A fixed endpoint conflict is also part of the closure contract.
    graph.assembly.joints.get_mut("slider").unwrap().kind = JointKind::Prismatic {
        distance: scalar(mm(3.0)),
    };
    graph
        .add_relationship(AssemblyRelationship {
            id: "conflict".into(),
            kind: RelationKind::Distance(mm(2.0)),
            first: DatumRef::new("rod", "end"),
            second: DatumRef::new("slider", "origin"),
        })
        .unwrap();
    let accepted = ModelDocument::from_graph(&graph);
    let result = graph
        .solve_joint_coordinates(&variables(), Default::default())
        .unwrap();
    assert!(!result.solved);
    assert_eq!(ModelDocument::from_graph(&graph), accepted);
}

#[test]
fn linkage_invalid_inputs_and_iteration_exhaustion_are_atomic() {
    let definition = definition(1.0);
    let mut graph = mechanism(&definition, 1.0, 0.0);
    let accepted = ModelDocument::from_graph(&graph);
    for variables in [
        vec![],
        vec![free("missing", JointDof::Angle)],
        vec![free("rod", JointDof::Axial)],
        vec![free("rod", JointDof::Angle); 2],
        vec![free("rod", JointDof::Angle); 10_001],
    ] {
        assert!(
            graph
                .solve_joint_coordinates(&variables, Default::default())
                .is_err()
        );
    }
    for options in [
        JointSolveOptions {
            maximum_iterations: 0,
            ..Default::default()
        },
        JointSolveOptions {
            maximum_iterations: 1001,
            ..Default::default()
        },
        JointSolveOptions {
            characteristic_length: Quantity::scalar(1.0),
            ..Default::default()
        },
        JointSolveOptions {
            characteristic_length: mm(f64::NAN),
            ..Default::default()
        },
        JointSolveOptions {
            characteristic_length: mm(0.0),
            ..Default::default()
        },
    ] {
        assert!(
            graph
                .solve_joint_coordinates(&variables(), options)
                .is_err()
        );
    }
    assert!(
        !graph
            .solve_joint_coordinates(
                &variables(),
                JointSolveOptions {
                    maximum_iterations: 1,
                    ..Default::default()
                }
            )
            .unwrap()
            .solved
    );
    assert_eq!(ModelDocument::from_graph(&graph), accepted);
    graph.assembly.joints.get_mut("slider").unwrap().kind = JointKind::Prismatic {
        distance: JointScalar {
            value: mm(3.0),
            minimum: Some(mm(3.0)),
            maximum: Some(mm(3.0)),
        },
    };
    assert!(
        graph
            .solve_joint_coordinates(&variables(), Default::default())
            .is_err()
    );
    graph.assembly.relationships.clear();
    assert!(
        graph
            .solve_joint_coordinates(&variables(), Default::default())
            .is_err()
    );
}

#[test]
fn linkage_preserves_units_reports_nullity_and_round_trips() {
    let definition = definition(1.0);
    let mut graph = mechanism(&definition, 1.0, 0.0);
    graph.assembly.joints.get_mut("slider").unwrap().kind = JointKind::Cylindrical {
        distance: scalar(Quantity::length(0.3, LengthUnit::Centimeter)),
        angle: scalar(Quantity::scalar(0.42)),
    };
    let mut unknowns = variables();
    unknowns.push(free("slider", JointDof::Angle));
    let result = graph
        .solve_joint_coordinates(&unknowns, Default::default())
        .unwrap();
    assert!(result.solved, "{result:?}");
    assert_eq!(result.free_degrees, 1);
    assert_eq!(result.positions[1].value.unit, Some(LengthUnit::Centimeter));
    assert_eq!(result.positions[2].value, Quantity::scalar(0.42));
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    let restored = ModelDocument::from_json(&json).unwrap();
    let restored_graph = restored.instance_graph().unwrap();
    assert!(
        restored_graph
            .check_relationships()
            .unwrap()
            .iter()
            .all(|check| check.satisfied)
    );
    assert_eq!(
        restored_graph.assembly.joints["slider"].kind,
        graph.assembly.joints["slider"].kind
    );
}

#[test]
fn linkage_four_bar_retains_the_seeded_assembly_branch() {
    let definition = definition(1.0);
    let mut graph = mechanism(&definition, 1.0, 0.0);
    graph
        .set_placement("slider", Placement::identity())
        .unwrap();
    graph.frames.get_mut("slider").unwrap().placement = Placement::translated(vector(4.0, 0.0));
    let joint = graph.assembly.joints.get_mut("slider").unwrap();
    joint.origin = vector(4.0, 0.0);
    joint.axis = VectorQuantity::scalars(0.0, 0.0, 1.0);
    joint.kind = JointKind::Revolute {
        angle: scalar(Quantity::scalar(1.5)),
    };
    graph
        .set_joint_coordinate("rod", JointDof::Angle, Quantity::scalar(-0.5))
        .unwrap();
    graph.assembly.relationships[0].second.datum = "end".into();
    let result = graph
        .solve_joint_coordinates(
            &[
                free("rod", JointDof::Angle),
                free("slider", JointDof::Angle),
            ],
            Default::default(),
        )
        .unwrap();
    assert!(result.solved, "{result:?}");
    // Independent circle intersection: both moving links have radius 3.
    let b: (f64, f64) = (1.0, 3.0_f64.sqrt());
    let d = ((4.0 - b.0).powi(2) + b.1 * b.1).sqrt();
    let height = (9.0 - d * d / 4.0).sqrt();
    let expected = (
        (b.0 + 4.0) / 2.0 + b.1 * height / d,
        b.1 / 2.0 + (4.0 - b.0) * height / d,
    );
    if let ResolvedDatum::Point { origin } = graph.datum("rod", "end").unwrap() {
        assert!(
            (origin.x - expected.0).abs() < 1e-6 && (origin.y - expected.1).abs() < 1e-6,
            "{origin:?} vs {expected:?}"
        );
    } else {
        panic!()
    }
}

#[test]
fn linkage_planar_coordinates_direction_and_mixed_unit_bounds() {
    let mut definition = definition(1.0);
    definition.datums.push(DatumDefinition {
        id: "axis".into(),
        kind: DatumKind::Axis {
            origin: VectorExpr::Literal(vector(0.0, 0.0)),
            direction: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        },
    });
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame("plane", None, Placement::identity(), "test")
        .unwrap();
    graph
        .add_joint(AssemblyJoint {
            id: "plane".into(),
            frame: "plane".into(),
            origin: vector(0.0, 0.0),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            kind: JointKind::Planar {
                x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                x: JointScalar {
                    value: mm(0.0),
                    minimum: Some(Quantity::length(0.0, LengthUnit::Meter)),
                    maximum: Some(Quantity::length(0.027, LengthUnit::Meter)),
                },
                y: scalar(mm(0.0)),
                angle: scalar(Quantity::scalar(0.4)),
            },
        })
        .unwrap();
    for id in ["moving", "fixed"] {
        graph.add_base(id, HashMap::new(), "test").unwrap();
    }
    graph.set_instance_frame("moving", Some("plane")).unwrap();
    graph
        .set_placement("fixed", Placement::translated(vector(27.0, -2.0)))
        .unwrap();
    for (id, datum, kind) in [
        ("position", "origin", RelationKind::Coincident),
        ("direction", "axis", RelationKind::Parallel),
    ] {
        graph
            .add_relationship(AssemblyRelationship {
                id: id.into(),
                kind,
                first: DatumRef::new("moving", datum),
                second: DatumRef::new("fixed", datum),
            })
            .unwrap();
    }
    let result = graph
        .solve_joint_coordinates(
            &[
                free("plane", JointDof::PlanarX),
                free("plane", JointDof::PlanarY),
                free("plane", JointDof::Angle),
            ],
            Default::default(),
        )
        .unwrap();
    assert!(result.solved, "{result:?}");
    assert_eq!(result.free_degrees, 0);
    assert!(
        result
            .active_limits
            .contains(&free("plane", JointDof::PlanarX))
    );
    graph.assembly.joints.get_mut("plane").unwrap().kind = JointKind::Fixed;
    assert!(
        graph
            .solve_joint_coordinates(&[free("plane", JointDof::Angle)], Default::default())
            .is_err()
    );
}

pub(super) fn slider_array(
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
        variables.push(free(&frame, JointDof::Axial));
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

#[test]
fn linkage_solves_1000_independent_mounted_sliders_and_preserves_limits() {
    let definition = definition(1.0);
    let (mut graph, variables) = slider_array(&definition, 1000, false);
    let result = graph
        .solve_joint_coordinates(&variables, Default::default())
        .unwrap();
    assert!(result.solved, "{result:?}");
    assert_eq!(result.free_degrees, 0);
    assert_eq!(result.redundant_equations, 2000);
    assert_eq!(result.positions.len(), 1000);
    assert!(
        result
            .positions
            .iter()
            .all(|position| position.value.normalized().unwrap() <= 1.0)
    );
    assert!(
        result
            .positions
            .iter()
            .all(|position| (position.value.normalized().unwrap() - 1.0).abs() < 1e-6)
    );
    assert_eq!(
        ModelDocument::from_json(&ModelDocument::from_graph(&graph).to_json_pretty().unwrap())
            .unwrap()
            .schema_version,
        CURRENT_SCHEMA_VERSION
    );
    assert!(
        graph
            .check_relationships()
            .unwrap()
            .iter()
            .all(|check| check.satisfied)
    );
    let study = MotionStudy {
        samples: vec![MotionSample { positions: vec![] }; 2],
        outputs: vec![InstanceOutputRef {
            instance: "prototype".into(),
            output: "body".into(),
        }],
        excluded_pairs: Vec::new(),
        collision_options: CollisionOptions::default(),
    };
    let closed = graph
        .solve_motion_study(&study, &variables, Default::default())
        .unwrap();
    assert_eq!(closed.status, JointMotionStatus::Complete);
    assert!(
        closed
            .solutions
            .iter()
            .all(|solution| solution.positions.len() == 1000 && solution.solved)
    );
    assert_eq!(
        closed.closed_study.unwrap().samples[0].positions.len(),
        1000
    );
    graph
        .set_placement("fixed-0", Placement::translated(vector(2.0, 0.0)))
        .unwrap();
    let before = ModelDocument::from_graph(&graph);
    let failed = graph
        .solve_joint_coordinates(&variables, Default::default())
        .unwrap();
    assert!(!failed.solved);
    assert!(failed.active_limits.contains(&variables[0]));
    assert_eq!(ModelDocument::from_graph(&graph), before);
}

#[test]
fn linkage_solves_coupled_64_frame_chain_through_ancestor_coordinates() {
    let definition = definition(1.0);
    let (mut graph, variables) = slider_array(&definition, 64, true);
    let result = graph
        .solve_joint_coordinates(&variables, Default::default())
        .unwrap();
    assert!(result.solved, "{result:?}");
    assert_eq!(result.free_degrees, 0);
    assert_eq!(result.redundant_equations, 128);
    assert!(
        result
            .positions
            .iter()
            .all(|position| (position.value.normalized().unwrap() - 1.0).abs() < 1e-6)
    );
    assert!(
        graph
            .check_relationships()
            .unwrap()
            .iter()
            .all(|check| check.satisfied)
    );
}

#[test]
fn linkage_dense_influence_budget_rejects_before_changing_graph() {
    let definition = definition(1.0);
    let (mut graph, variables) = slider_array(&definition, 100, true);
    let before = ModelDocument::from_graph(&graph);
    let error = graph
        .solve_joint_coordinates(&variables, Default::default())
        .unwrap_err();
    assert!(error.to_string().contains("sparse work budget"), "{error}");
    assert_eq!(ModelDocument::from_graph(&graph), before);
}

#[test]
fn linkage_large_closed_reports_reject_before_solving_or_mutating() {
    let definition = definition(1.0);
    let (graph, variables) = slider_array(&definition, 1000, false);
    let before = ModelDocument::from_graph(&graph);
    let study = MotionStudy {
        samples: vec![MotionSample { positions: vec![] }; 2001],
        outputs: vec![InstanceOutputRef {
            instance: "prototype".into(),
            output: "body".into(),
        }],
        excluded_pairs: Vec::new(),
        collision_options: CollisionOptions::default(),
    };
    let error = graph
        .solve_motion_study(&study, &variables, Default::default())
        .unwrap_err();
    assert!(error.to_string().contains("report entry budget"), "{error}");
    assert_eq!(ModelDocument::from_graph(&graph), before);
}
