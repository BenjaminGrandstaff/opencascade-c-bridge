//! Lofts through parameter-placed planar outlines.

use super::*;

fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn direction(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::scalars(x, y, z))
}

/// A section in the XZ plane at span position `y`.
fn section(profile: Vec<[f64; 2]>, y: f64, scale: ScalarExpr) -> LoftSection {
    LoftSection {
        profile,
        origin: point(0.0, y, 0.0),
        x_axis: direction(1.0, 0.0, 0.0),
        y_axis: direction(0.0, 0.0, 1.0),
        scale,
        rotation_radians: None,
        pivot: [0.0, 0.0],
    }
}

fn unit_square() -> Vec<[f64; 2]> {
    vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
}

fn circle(points: usize) -> Vec<[f64; 2]> {
    (0..points)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / points as f64;
            [angle.cos(), angle.sin()]
        })
        .collect()
}

fn loft_family(sections: Vec<LoftSection>, smooth: bool, ruled: bool) -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.parameters.push(length_parameter("size", 10.0));
    let mut turn = length_parameter("turn", 0.0);
    turn.parameter_type = ParameterType::Scalar(Dimension::Scalar);
    turn.default = ParameterValue::Scalar(Quantity::scalar(0.0));
    turn.minimum = None;
    family.parameters.push(turn);
    family.features = vec![FeatureDefinition {
        id: "loft".into(),
        operation: FeatureOperation::Loft {
            sections,
            smooth,
            ruled,
        },
    }];
    family
}

fn part(family: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition: family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

fn volume(session: &Session, result: &GeneratedResult<'_>) -> f64 {
    session.volume(result.shape("loft").unwrap()).unwrap()
}

#[test]
fn parameter_driven_sections_place_scale_and_rotate_profiles() {
    let session = Session::new().unwrap();
    let size = ScalarExpr::Parameter("size".into());
    // A square turning about its center is still a 20 mm prism of its area.
    let mut tip = section(unit_square(), 20.0, size.clone());
    tip.rotation_radians = Some(ScalarExpr::Parameter("turn".into()));
    tip.pivot = [0.5, 0.5];
    let family = loft_family(vec![section(unit_square(), 0.0, size), tip], false, true);
    let mut instance = part(&family);
    let first = instance.regenerate(&session).unwrap();
    assert!((volume(&session, &first) - 10.0 * 10.0 * 20.0).abs() < 1e-6);

    instance.overrides.insert(
        "size".into(),
        ParameterValue::Scalar(Quantity::length(2.0, LengthUnit::Centimeter)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.rebuilt, vec!["loft"]);
    assert!((volume(&session, &second) - 20.0 * 20.0 * 20.0).abs() < 1e-6);
    let bounds = session.bounds(second.shape("loft").unwrap()).unwrap();
    assert!((bounds.max.x - 20.0).abs() < 1e-6 && (bounds.max.z - 20.0).abs() < 1e-6);

    // Rotating the tip a quarter turn about its center moves its corners but
    // keeps the profile within the same 20 mm square.
    instance.overrides.insert(
        "turn".into(),
        ParameterValue::Scalar(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
    );
    let turned = instance.regenerate_incremental(&session, &second).unwrap();
    assert!(session.is_valid(turned.shape("loft").unwrap()).unwrap());
    drop((first, second, turned));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn smooth_sections_interpolate_curves_and_polygons_do_not() {
    let session = Session::new().unwrap();
    let radius = || ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter));
    let sections = || {
        vec![
            section(circle(24), 0.0, radius()),
            section(circle(24), 30.0, radius()),
        ]
    };
    let exact = std::f64::consts::PI * 25.0 * 30.0;
    let smooth = loft_family(sections(), true, true);
    let polygon = loft_family(sections(), false, true);
    let smooth_volume = volume(&session, &part(&smooth).regenerate(&session).unwrap());
    let polygon_volume = volume(&session, &part(&polygon).regenerate(&session).unwrap());
    assert!(
        (smooth_volume - exact).abs() / exact < 1e-3,
        "{smooth_volume}"
    );
    assert!(
        (polygon_volume - exact).abs() / exact > 1e-2,
        "{polygon_volume}"
    );
}

#[test]
fn invalid_sections_fail_without_partial_output_and_lofts_persist() {
    let session = Session::new().unwrap();
    let size = || ScalarExpr::Parameter("size".into());
    let good = || section(unit_square(), 0.0, size());
    let far = || section(unit_square(), 20.0, size());
    let mut three_points = far();
    three_points.profile.pop();
    let mut slanted = far();
    slanted.y_axis = direction(1.0, 0.0, 1.0);
    let mut flat = far();
    flat.scale = ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter));
    let mut unitless = far();
    unitless.scale = ScalarExpr::Literal(Quantity::scalar(1.0));
    let mut nan = far();
    nan.profile[1][0] = f64::NAN;
    for (sections, message) in [
        (vec![good()], "2-1000 sections"),
        (vec![good(), three_points], "same number"),
        (vec![good(), slanted], "perpendicular"),
        (vec![good(), flat], "positive length"),
        (vec![good(), unitless], "dimension"),
        (vec![good(), nan], "finite"),
    ] {
        let family = loft_family(sections, true, true);
        let error = part(&family).regenerate(&session).err().unwrap();
        assert!(error.message.contains(message), "{}", error.message);
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    let family = loft_family(vec![good(), far()], true, false);
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(!json.contains("rotation_radians") && !json.contains("pivot"));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
}

#[test]
fn schema_50_lofts_with_stored_rules_support_continuous_joint_motion() {
    let session = Session::new().unwrap();
    let scale = ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter));
    let mut family = loft_family(
        vec![
            section(unit_square(), 0.0, scale.clone()),
            section(unit_square(), 1.0, scale),
        ],
        false,
        true,
    );
    for (id, rule) in [
        (
            "single-solid",
            VerificationRule::Connectivity {
                output: "loft".into(),
                solids: 1,
                allow_voids: false,
            },
        ),
        (
            "envelope",
            VerificationRule::FitsWithin {
                output: "loft".into(),
                envelope: VectorQuantity::lengths(1.1, 1.1, 1.1, LengthUnit::Millimeter),
            },
        ),
    ] {
        family.requirements.push(Requirement {
            id: id.into(),
            version: 1,
            kind: RequirementKind::Validation,
            priority: RequirementPriority::Required,
            statement: id.into(),
            rule,
            provenance: "integration".into(),
            traces: Vec::new(),
        });
    }
    let mut graph = InstanceGraph::new(&family);
    graph
        .add_base("moving", HashMap::new(), "integration")
        .unwrap();
    graph
        .set_placement(
            "moving",
            Placement::translated(VectorQuantity::lengths(
                5.0,
                -0.5,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph
        .add_clone("obstacle", "moving", HashMap::new(), "integration")
        .unwrap();
    let center = 5.5 / std::f64::consts::SQRT_2;
    graph
        .set_placement(
            "obstacle",
            Placement::translated(VectorQuantity::lengths(
                center - 0.5,
                center - 0.5,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph
        .add_frame("rotor", None, Placement::identity(), "integration")
        .unwrap();
    graph
        .add_joint(AssemblyJoint {
            id: "rotor".into(),
            frame: "rotor".into(),
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            kind: JointKind::Revolute {
                angle: JointScalar {
                    value: Quantity::scalar(0.0),
                    minimum: None,
                    maximum: None,
                },
            },
        })
        .unwrap();
    graph.set_instance_frame("moving", Some("rotor")).unwrap();
    let document = ModelDocument::from_graph(&graph);
    assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
    let mut legacy: serde_json::Value =
        serde_json::from_str(&document.to_json_pretty().unwrap()).unwrap();
    legacy["schema_version"] = serde_json::json!(50);
    let restored = ModelDocument::from_json(&legacy.to_string()).unwrap();
    assert_eq!(restored.schema_version, CURRENT_SCHEMA_VERSION);
    let graph = restored.instance_graph().unwrap();
    let accepted = graph
        .regenerate_instances_current(&session, &["moving", "obstacle"])
        .unwrap();
    assert_eq!(accepted.generated_variants, 1);
    assert_eq!(accepted.result("moving").unwrap().verification.len(), 2);
    assert!(
        accepted
            .result("moving")
            .unwrap()
            .verification
            .iter()
            .all(|result| result.status == VerificationStatus::Passed)
    );
    let count = session.shape_count().unwrap();
    let outputs = vec![
        InstanceOutputRef {
            instance: "moving".into(),
            output: "loft".into(),
        },
        InstanceOutputRef {
            instance: "obstacle".into(),
            output: "loft".into(),
        },
    ];
    let study = MotionStudy::linear(
        "rotor",
        JointDof::Angle,
        Quantity::scalar(0.0),
        Quantity::scalar(std::f64::consts::TAU),
        2,
        outputs,
        Default::default(),
    )
    .unwrap();
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
    assert_eq!(result.generated_variants, 1);
    assert_eq!(session.shape_count().unwrap(), count);
    assert_eq!(ModelDocument::from_graph(&graph), restored);
    drop(accepted);
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn native_profile_family() -> FamilyDefinition {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/profile-loft.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string())
        .unwrap()
        .family
}

#[test]
fn saved_face_and_wire_profiles_loft_to_native_solids_with_history() {
    let session = Session::new().unwrap();
    for ruled in [false, true] {
        let mut definition = native_profile_family();
        let FeatureOperation::ProfileLoft { ruled: mode, .. } =
            &mut definition.features[0].operation
        else {
            panic!()
        };
        *mode = ruled;
        let result = part(&definition).regenerate(&session).unwrap();
        let body = result.shape("body").unwrap();
        let expected = 20.0 * std::f64::consts::PI * (64.0 + 32.0 + 16.0) / 3.0;
        assert!((session.volume(body).unwrap() - expected).abs() < 1e-5);
        assert!(session.is_valid(body).unwrap());
        let edge = session
            .subshape(result.shape("lower").unwrap(), ShapeType::Edge, 0)
            .unwrap();
        assert!(
            session
                .history_count(body, &edge, HistoryRelation::Generated)
                .unwrap()
                > 0
        );
        assert_eq!(
            session
                .subshape_count(result.shape("upper").unwrap(), ShapeType::Edge)
                .unwrap(),
            1
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn saved_profile_edits_reuse_unaffected_sections_and_roundtrip() {
    let session = Session::new().unwrap();
    let definition = native_profile_family();
    let first = part(&definition).regenerate(&session).unwrap();
    let mut edited = part(&definition);
    edited.overrides.insert(
        "upper_radius".into(),
        ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
    );
    let second = edited.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.reused, ["lower"]);
    assert!(second.regeneration.rebuilt.contains(&"upper".into()));
    assert!(second.regeneration.rebuilt.contains(&"body".into()));
    assert!(
        (session.volume(second.shape("body").unwrap()).unwrap()
            - 20.0 * std::f64::consts::PI * (64.0 + 40.0 + 25.0) / 3.0)
            .abs()
            < 1e-5
    );
    let document = ModelDocument::from_graph(&InstanceGraph::new(&definition));
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
}

#[test]
fn saved_profile_lofts_reject_bad_counts_duplicates_wrong_shapes_and_open_wires() {
    let session = Session::new().unwrap();
    for profiles in [
        vec!["lower"],
        vec!["lower", "lower"],
        vec!["lower", "absent"],
    ] {
        let mut definition = native_profile_family();
        let FeatureOperation::ProfileLoft {
            profiles: sections, ..
        } = &mut definition.features[0].operation
        else {
            panic!()
        };
        *sections = profiles.into_iter().map(str::to_owned).collect();
        assert!(part(&definition).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut definition = native_profile_family();
    definition.features[2].operation = FeatureOperation::Box {
        origin: point(0., 0., 20.),
        size: point(1., 1., 1.),
    };
    assert!(part(&definition).regenerate(&session).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut definition = native_profile_family();
    let FeatureOperation::SketchWire { sketch } = &definition.features[2].operation else {
        panic!()
    };
    let mut sketch = sketch.clone();
    sketch.circles.clear();
    sketch.constraints.clear();
    sketch.lines = vec![SketchLine {
        id: "line".into(),
        start: "center".into(),
        end: "edge".into(),
    }];
    sketch.profile = vec!["line".into()];
    definition.features[2].operation = FeatureOperation::SketchOpenWire { sketch };
    let error = part(&definition).regenerate(&session).err().unwrap();
    assert!(error.message.contains("closed wires"), "{}", error.message);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn loft_end_constraints_follow_profile_and_strength_parameters_and_roundtrip() {
    let session = Session::new().unwrap();
    let mut family = native_profile_family();
    let FeatureOperation::ProfileLoft {
        ruled,
        start_constraint,
        end_constraint,
        ..
    } = &mut family.features[0].operation
    else {
        panic!()
    };
    *ruled = false;
    let c = Box::new(LoftEndConstraint {
        direction: Some(direction(0., 0., 1.)),
        normal_to_profile: false,
        magnitude: ScalarExpr::Parameter("height".into()),
    });
    *start_constraint = Some(c.clone());
    *end_constraint = Some(c);
    let definition = family.clone();
    let mut p = part(&family);
    let result = p.regenerate(&session).unwrap();
    let expected = 20. * std::f64::consts::PI * (64. - 32. + 16. * 13. / 35.);
    assert!((session.volume(result.shape("body").unwrap()).unwrap() - expected).abs() < 1e-4);
    let (_, radius) = session
        .ray_first_hit(
            result.shape("body").unwrap(),
            Vec3::new(0., 0., 5.),
            Vec3::new(1., 0., 0.),
            20.,
        )
        .unwrap()
        .unwrap();
    assert!((radius - 7.375).abs() < 1e-6);
    drop(result);
    p.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(40., LengthUnit::Millimeter)),
    );
    p.overrides.insert(
        "upper_radius".into(),
        ParameterValue::Scalar(Quantity::length(6., LengthUnit::Millimeter)),
    );
    let changed = p.regenerate(&session).unwrap();
    let (_, radius) = session
        .ray_first_hit(
            changed.shape("body").unwrap(),
            Vec3::new(0., 0., 10.),
            Vec3::new(1., 0., 0.),
            20.,
        )
        .unwrap()
        .unwrap();
    assert!((radius - 7.6875).abs() < 1e-6);
    assert_eq!(family, definition);
    let json = serde_json::to_string(&family).unwrap();
    assert_eq!(
        serde_json::from_str::<FamilyDefinition>(&json).unwrap(),
        family
    );
    drop(changed);
    let FeatureOperation::ProfileLoft { ruled, .. } = &mut family.features[0].operation else {
        panic!()
    };
    *ruled = true;
    assert!(
        part(&family)
            .regenerate(&session)
            .err()
            .unwrap()
            .message
            .contains("ruled")
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn independent_loft_strength_rebuilds_only_the_loft_and_changes_geometry() {
    let session = Session::new().unwrap();
    let mut family = native_profile_family();
    family.parameters.push(length_parameter("strength", 20.));
    let FeatureOperation::ProfileLoft {
        ruled,
        start_constraint,
        end_constraint,
        ..
    } = &mut family.features[0].operation
    else {
        panic!()
    };
    *ruled = false;
    *start_constraint = Some(Box::new(LoftEndConstraint {
        direction: Some(direction(0., 0., 1.)),
        normal_to_profile: false,
        magnitude: ScalarExpr::Parameter("strength".into()),
    }));
    *end_constraint = Some(Box::new(LoftEndConstraint {
        direction: Some(direction(0., 0., 1.)),
        normal_to_profile: false,
        magnitude: ScalarExpr::Parameter("height".into()),
    }));
    let first = part(&family).regenerate(&session).unwrap();
    let mut edited = part(&family);
    edited.overrides.insert(
        "strength".into(),
        ParameterValue::Scalar(Quantity::length(40., LengthUnit::Millimeter)),
    );
    let changed = edited.regenerate_incremental(&session, &first).unwrap();
    assert!(changed.regeneration.reused.contains(&"lower".into()));
    assert!(changed.regeneration.reused.contains(&"upper".into()));
    assert_eq!(changed.regeneration.rebuilt, ["body"]);
    let (_, radius) = session
        .ray_first_hit(
            changed.shape("body").unwrap(),
            Vec3::new(0., 0., 10.),
            Vec3::new(1., 0., 0.),
            20.,
        )
        .unwrap()
        .unwrap();
    assert!(radius > 6.5);
}

#[test]
fn normal_loft_constraints_follow_parameter_driven_profile_plane_edits() {
    let session = Session::new().unwrap();
    let mut family = native_profile_family();
    family.parameters.push(ParameterDefinition {
        id: "tilt".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(0.)),
        minimum: None,
        maximum: None,
    });
    for feature in &mut family.features {
        match &mut feature.operation {
            FeatureOperation::SketchFace { sketch } | FeatureOperation::SketchWire { sketch } => {
                sketch.y_axis = VectorExpr::Components {
                    x: ScalarExpr::Literal(Quantity::scalar(0.)),
                    y: ScalarExpr::Literal(Quantity::scalar(1.)),
                    z: ScalarExpr::Parameter("tilt".into()),
                };
            }
            FeatureOperation::ProfileLoft {
                ruled,
                start_constraint,
                end_constraint,
                ..
            } => {
                *ruled = false;
                let c = Box::new(LoftEndConstraint {
                    direction: None,
                    normal_to_profile: true,
                    magnitude: ScalarExpr::Parameter("height".into()),
                });
                *start_constraint = Some(c.clone());
                *end_constraint = Some(c);
            }
            _ => {}
        }
    }
    let first = part(&family).regenerate(&session).unwrap();
    let volume = session.volume(first.shape("body").unwrap()).unwrap();
    let mut edited = part(&family);
    edited.overrides.insert(
        "tilt".into(),
        ParameterValue::Scalar(Quantity::scalar(0.25)),
    );
    let changed = edited.regenerate_incremental(&session, &first).unwrap();
    assert!(changed.regeneration.rebuilt.contains(&"body".into()));
    assert!(changed.regeneration.rebuilt.contains(&"lower".into()));
    assert!(
        session
            .face_normal(changed.shape("lower").unwrap())
            .unwrap()
            .y
            .abs()
            > 0.2
    );
    assert!((session.volume(changed.shape("body").unwrap()).unwrap() - volume).abs() > 1.);
    let count = session.shape_count().unwrap();
    let tangents = changed
        .loft_end_tangents(
            &session,
            &family.features[0].operation,
            &edited.resolved_parameters().unwrap(),
        )
        .unwrap();
    assert_eq!(tangents.len(), 2);
    assert!(!tangents[0].end && tangents[1].end);
    for t in &tangents {
        assert!(t.normal_to_profile);
        assert!(t.derivative.y < -4.);
        assert!((t.derivative.x.hypot(t.derivative.y.hypot(t.derivative.z)) - 20.).abs() < 1e-7);
    }
    assert!(tangents[0].origin.z.abs() < 1e-7);
    assert!((tangents[1].origin.z - 20.).abs() < 1e-7);
    assert_eq!(session.shape_count().unwrap(), count);
    let json = serde_json::to_string(&family).unwrap();
    assert_eq!(
        serde_json::from_str::<FamilyDefinition>(&json).unwrap(),
        family
    );
}

#[test]
fn profile_correspondence_tracks_parameters_keeps_history_and_rejects_stale_anchors() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/aligned-profile-loft.request.json"
    ))
    .unwrap();
    let mut family = ModelDocument::from_json(&request["model"].to_string())
        .unwrap()
        .family;
    let s = Session::new().unwrap();
    {
        let original = part(&family).regenerate(&s).unwrap();
        let edge = s
            .subshape(original.shape("lower").unwrap(), ShapeType::Edge, 0)
            .unwrap();
        assert!(
            s.history_count(
                original.shape("body").unwrap(),
                &edge,
                HistoryRelation::Generated
            )
            .unwrap()
                > 0
        );
        let mut changed = part(&family);
        changed.overrides.insert(
            "lower_radius".into(),
            ParameterValue::Scalar(Quantity::length(10., LengthUnit::Millimeter)),
        );
        changed.overrides.insert(
            "height".into(),
            ParameterValue::Scalar(Quantity::length(30., LengthUnit::Millimeter)),
        );
        let result = changed.regenerate_incremental(&s, &original).unwrap();
        assert!(result.regeneration.rebuilt.contains(&"body".into()));
        assert!(s.is_valid(result.shape("body").unwrap()).unwrap());
        let tangents = result
            .loft_end_tangents(
                &s,
                &family.features[0].operation,
                &changed.resolved_parameters().unwrap(),
            )
            .unwrap();
        assert_eq!(tangents.len(), 2);
        assert!(tangents.iter().all(|t| (t.derivative.z - 30.).abs() < 1e-7));
    }
    assert_eq!(s.shape_count().unwrap(), 0);
    let saved = family.clone();
    for invalid in ["stale", "off-boundary", "free", "ruled"] {
        family = saved.clone();
        let FeatureOperation::ProfileLoft {
            correspondence,
            start_constraint,
            end_constraint,
            ruled,
            ..
        } = &mut family.features[0].operation
        else {
            panic!()
        };
        match invalid {
            "stale" => {
                correspondence.insert("absent".into(), LoftProfileAlignment::default());
            }
            "off-boundary" => {
                correspondence.get_mut("lower").unwrap().start_point = Some(point(0., 9., 0.))
            }
            "free" => {
                *start_constraint = None;
                *end_constraint = None;
            }
            _ => *ruled = true,
        }
        assert!(part(&family).regenerate(&s).is_err(), "{invalid}");
        assert_eq!(s.shape_count().unwrap(), 0);
    }
    let encoded = serde_json::to_string(&saved).unwrap();
    assert_eq!(
        serde_json::from_str::<FamilyDefinition>(&encoded).unwrap(),
        saved
    );
}

#[test]
fn independent_correspondence_parameter_invalidates_loft_without_rebuilding_profiles() {
    let mut family = native_profile_family();
    let mut anchor = length_parameter("anchor", 8.);
    anchor.minimum = None;
    family.parameters.push(anchor);
    let FeatureOperation::ProfileLoft {
        correspondence,
        ruled,
        start_constraint,
        ..
    } = &mut family.features[0].operation
    else {
        panic!()
    };
    *ruled = false;
    *start_constraint = Some(Box::new(LoftEndConstraint {
        direction: None,
        normal_to_profile: true,
        magnitude: ScalarExpr::Parameter("height".into()),
    }));
    correspondence.insert(
        "lower".into(),
        LoftProfileAlignment {
            start_point: Some(VectorExpr::Components {
                x: ScalarExpr::Literal(Quantity::length(0., LengthUnit::Millimeter)),
                y: ScalarExpr::Parameter("anchor".into()),
                z: ScalarExpr::Literal(Quantity::length(0., LengthUnit::Millimeter)),
            }),
            reverse: false,
        },
    );
    correspondence.insert(
        "upper".into(),
        LoftProfileAlignment {
            start_point: Some(VectorExpr::Components {
                x: ScalarExpr::Literal(Quantity::length(0., LengthUnit::Millimeter)),
                y: ScalarExpr::Multiply(
                    Box::new(ScalarExpr::Parameter("anchor".into())),
                    Box::new(ScalarExpr::Literal(Quantity::scalar(0.5))),
                ),
                z: ScalarExpr::Parameter("height".into()),
            }),
            reverse: false,
        },
    );
    let s = Session::new().unwrap();
    let first = part(&family).regenerate(&s).unwrap();
    let mut changed = part(&family);
    // A different point on the same circle must rebuild the loft even though the sources do not change.
    changed.overrides.insert(
        "anchor".into(),
        ParameterValue::Scalar(Quantity::length(-8., LengthUnit::Millimeter)),
    );
    let result = changed.regenerate_incremental(&s, &first).unwrap();
    assert_eq!(result.regeneration.rebuilt, ["body"]);
    assert!(result.regeneration.reused.contains(&"lower".into()));
    assert!(result.regeneration.reused.contains(&"upper".into()));
    assert!(
        (s.volume(result.shape("body").unwrap()).unwrap()
            - s.volume(first.shape("body").unwrap()).unwrap())
        .abs()
            < 1e-5
    );
}
