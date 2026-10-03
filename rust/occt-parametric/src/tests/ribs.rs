use super::*;

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}
fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn direction(y: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::scalars(0.0, y, 0.0))
}

fn rib_family(wire: bool) -> FamilyDefinition {
    let sketch = SketchDefinition {
        id: "brace".into(),
        datum_plane: None,
        origin: point(2.0, 4.0, 1.0),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        points: vec![
            SketchPoint {
                id: "a".into(),
                x: length(0.0),
                y: length(0.0),
                fixed: true,
            },
            SketchPoint {
                id: "b".into(),
                x: length(6.0),
                y: length(0.0),
                fixed: true,
            },
            SketchPoint {
                id: "c".into(),
                x: length(0.0),
                y: ScalarExpr::Parameter("height".into()),
                fixed: true,
            },
        ],
        lines: vec![
            SketchLine {
                id: "ab".into(),
                start: "a".into(),
                end: "b".into(),
            },
            SketchLine {
                id: "bc".into(),
                start: "b".into(),
                end: "c".into(),
            },
            SketchLine {
                id: "ca".into(),
                start: "c".into(),
                end: "a".into(),
            },
        ],
        circles: Vec::new(),
        arcs: Vec::new(),
        profile: Vec::new(),
        constraints: Vec::new(),
    };
    FamilyDefinition {
        id: "Rib".into(),
        version: 1,
        parameters: vec![
            length_parameter("thickness", 2.0),
            length_parameter("height", 6.0),
        ],
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        datums: Vec::new(),
        requirements: Vec::new(),
        features: vec![
            FeatureDefinition {
                id: "rib".into(),
                operation: FeatureOperation::Rib {
                    input: "body".into(),
                    profile: "profile".into(),
                    thickness: ScalarExpr::Parameter("thickness".into()),
                    direction: direction(3.0),
                    profile_mode: RibProfileMode::Closed,
                    thickness_mode: RibThicknessMode::OneSided,
                },
            },
            FeatureDefinition {
                id: "body".into(),
                operation: FeatureOperation::Box {
                    origin: point(0.0, 0.0, 0.0),
                    size: point(10.0, 10.0, 1.0),
                },
            },
            FeatureDefinition {
                id: "profile".into(),
                operation: if wire {
                    FeatureOperation::SketchWire {
                        sketch: Box::new(sketch),
                    }
                } else {
                    FeatureOperation::SketchFace {
                        sketch: Box::new(sketch),
                    }
                },
            },
            FeatureDefinition {
                id: "placed".into(),
                operation: FeatureOperation::Translate {
                    input: "rib".into(),
                    offset: point(20.0, 0.0, 0.0),
                },
            },
        ],
    }
}
fn part(definition: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

#[test]
fn triangular_ribs_join_faces_and_wires_with_exact_volume_and_body_history() {
    let session = Session::new().unwrap();
    for wire in [false, true] {
        for sign in [3.0, -3.0] {
            let mut definition = rib_family(wire);
            if let FeatureOperation::Rib {
                direction: axis, ..
            } = &mut definition.features[0].operation
            {
                *axis = direction(sign);
            }
            let generated = part(&definition).regenerate(&session).unwrap();
            let result = generated.shape("rib").unwrap();
            let body = generated.shape("body").unwrap();
            assert!((session.volume(result).unwrap() - 136.0).abs() < 1e-7);
            assert!((session.volume(body).unwrap() - 100.0).abs() < 1e-7);
            assert!(session.is_valid(result).unwrap());
            assert_eq!(session.subshape_count(result, ShapeType::Solid).unwrap(), 1);
            let mut modified = 0;
            for index in 0..6 {
                let face = session.subshape(body, ShapeType::Face, index).unwrap();
                modified += session
                    .history_count(result, &face, occt_bridge::HistoryRelation::Modified)
                    .unwrap();
            }
            assert!(modified > 0);
            assert_eq!(session.shape_count().unwrap(), 4);
            drop(generated);
            assert_eq!(session.shape_count().unwrap(), 0);
        }
    }
    let mut overlapping = rib_family(true);
    if let FeatureOperation::SketchWire { sketch } = &mut overlapping.features[2].operation {
        sketch.origin = point(2.0, 4.0, 0.5);
    }
    let generated = part(&overlapping).regenerate(&session).unwrap();
    // The part already contains 5.75 mm^3 of the 36 mm^3 wall.
    assert!((session.volume(generated.shape("rib").unwrap()).unwrap() - 130.25).abs() < 1e-7);
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn rib_thickness_profile_and_direction_edits_rebuild_only_affected_features() {
    let session = Session::new().unwrap();
    let mut definition = rib_family(true);
    definition.parameters.push(ParameterDefinition {
        id: "direction".into(),
        parameter_type: ParameterType::Vector(Dimension::Scalar),
        default: ParameterValue::Vector(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        minimum: None,
        maximum: None,
    });
    if let FeatureOperation::Rib { direction, .. } = &mut definition.features[0].operation {
        *direction = VectorExpr::Parameter("direction".into());
    }
    let first = part(&definition).regenerate(&session).unwrap();
    for (name, value, expected) in [
        (
            "thickness",
            ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
            154.0,
        ),
        (
            "height",
            ParameterValue::Scalar(Quantity::length(8.0, LengthUnit::Millimeter)),
            148.0,
        ),
        (
            "direction",
            ParameterValue::Vector(VectorQuantity::scalars(0.0, -2.0, 0.0)),
            136.0,
        ),
    ] {
        let mut edited = part(&definition);
        edited.overrides.insert(name.into(), value);
        let next = edited.regenerate_incremental(&session, &first).unwrap();
        assert!(next.regeneration.reused.contains(&"body".into()));
        assert_eq!(
            next.regeneration.rebuilt.contains(&"profile".into()),
            name == "height"
        );
        assert!(next.regeneration.rebuilt.contains(&"rib".into()));
        assert!(next.regeneration.rebuilt.contains(&"placed".into()));
        assert!((session.volume(next.shape("rib").unwrap()).unwrap() - expected).abs() < 1e-7);
    }
    let count = session.shape_count().unwrap();
    let mut failed = part(&definition);
    failed.overrides.insert(
        "direction".into(),
        ParameterValue::Vector(VectorQuantity::scalars(0.0, 0.0, 0.0)),
    );
    assert!(failed.regenerate_incremental(&session, &first).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(first.shape("rib").unwrap()).unwrap());
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn ribs_reject_disconnected_edge_only_contained_and_invalid_inputs() {
    let session = Session::new().unwrap();
    for case in 0..10 {
        let mut definition = rib_family(true);
        if let FeatureOperation::SketchWire { sketch } = &mut definition.features[2].operation {
            match case {
                0 => sketch.origin = point(2.0, 4.0, 3.0),
                1 => sketch.origin = point(2.0, 10.0, 1.0),
                2 => {
                    sketch.origin = point(2.0, 4.0, 0.0);
                    sketch.points[2].y = length(0.5);
                }
                _ => {}
            }
        }
        if let FeatureOperation::Rib {
            input,
            profile,
            thickness,
            direction,
            ..
        } = &mut definition.features[0].operation
        {
            match case {
                3 => *thickness = length(0.0),
                4 => *thickness = ScalarExpr::Literal(Quantity::scalar(2.0)),
                5 => *direction = point(0.0, 1.0, 0.0),
                6 => *direction = VectorExpr::Literal(VectorQuantity::scalars(1.0, 1.0, 0.0)),
                7 => *profile = "body".into(),
                8 => *input = "profile".into(),
                9 => *profile = "missing".into(),
                _ => {}
            }
        }
        assert!(
            part(&definition).regenerate(&session).is_err(),
            "case {case}"
        );
        assert_eq!(session.shape_count().unwrap(), 0, "case {case}");
    }
}

#[test]
fn rib_schema_thirty_four_round_trips_and_migrates_existing_features() {
    for wire in [false, true] {
        let definition = rib_family(wire);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let document = ModelDocument::from_graph(&graph);
        assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
        let session = Session::new().unwrap();
        let generated = part(&loaded.family).regenerate(&session).unwrap();
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
        let mut old = serde_json::to_value(&document).unwrap();
        old["schema_version"] = serde_json::json!(33);
        old["family"]["features"]
            .as_array_mut()
            .unwrap()
            .retain(|feature| feature["id"] == "body");
        let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.family.features[0], definition.features[1]);
    }
}

fn set_thickness_mode(definition: &mut FamilyDefinition, mode: RibThicknessMode) {
    let FeatureOperation::Rib { thickness_mode, .. } = &mut definition.features[0].operation else {
        unreachable!()
    };
    *thickness_mode = mode;
}

#[test]
fn centered_ribs_have_total_thickness_and_are_invariant_to_direction_reversal() {
    let session = Session::new().unwrap();
    for wire in [false, true] {
        for (offset, scale) in [(0.0, 1.0), (1_000_000.0, 1.0), (0.0, 0.01)] {
            for sign in [3.0, -3.0] {
                let mut definition = rib_family(wire);
                set_thickness_mode(&mut definition, RibThicknessMode::Centered);
                let FeatureOperation::Rib {
                    direction: axis, ..
                } = &mut definition.features[0].operation
                else {
                    unreachable!()
                };
                *axis = direction(sign);
                let FeatureOperation::Box { origin, size } = &mut definition.features[1].operation
                else {
                    unreachable!()
                };
                *origin = point(0.0, offset, 0.0);
                *size = point(10.0 * scale, 10.0 * scale, scale);
                for parameter in &mut definition.parameters {
                    parameter.minimum = None;
                    let ParameterValue::Scalar(value) = &mut parameter.default else {
                        unreachable!()
                    };
                    value.value *= scale;
                }
                let sketch = match &mut definition.features[2].operation {
                    FeatureOperation::SketchWire { sketch }
                    | FeatureOperation::SketchFace { sketch } => sketch,
                    _ => unreachable!(),
                };
                sketch.origin = point(2.0 * scale, offset + 4.0 * scale, scale);
                sketch.points[1].x = length(6.0 * scale);
                let generated = part(&definition).regenerate(&session).unwrap();
                let rib = generated.shape("rib").unwrap();
                let expected_y = offset + scale * (500.0 + 36.0 * 4.0) / 136.0;
                assert!(
                    (session.volume(rib).unwrap() - 136.0 * scale.powi(3)).abs()
                        < 1e-7 * scale.powi(3)
                );
                assert!((session.center_of_mass(rib).unwrap().y - expected_y).abs() < 1e-7);
                assert_eq!(session.subshape_count(rib, ShapeType::Solid).unwrap(), 1);
                assert!(session.is_valid(rib).unwrap());
                assert!(
                    (session
                        .center_of_mass(generated.shape("profile").unwrap())
                        .unwrap()
                        .y
                        - (offset + 4.0 * scale))
                        .abs()
                        < 1e-7
                );
                assert!(
                    (session.volume(generated.shape("body").unwrap()).unwrap()
                        - 100.0 * scale.powi(3))
                    .abs()
                        < 1e-7 * scale.powi(3)
                );
                let body = generated.shape("body").unwrap();
                let history = (0..6)
                    .map(|index| {
                        let face = session.subshape(body, ShapeType::Face, index).unwrap();
                        session
                            .history_count(rib, &face, occt_bridge::HistoryRelation::Modified)
                            .unwrap()
                    })
                    .sum::<usize>();
                assert!(history > 0);
                assert_eq!(session.shape_count().unwrap(), 4);
                drop(generated);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        }
    }
}

#[test]
fn changing_rib_thickness_mode_reuses_inputs_and_rebuilds_dependents() {
    let session = Session::new().unwrap();
    let definition = rib_family(true);
    let first = part(&definition).regenerate(&session).unwrap();
    assert!(
        (session
            .center_of_mass(first.shape("rib").unwrap())
            .unwrap()
            .y
            - 5.0)
            .abs()
            < 1e-7
    );
    let mut centered = definition.clone();
    set_thickness_mode(&mut centered, RibThicknessMode::Centered);
    let mut edited = part(&centered);
    edited.overrides.insert(
        "thickness".into(),
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
    );
    let next = edited.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(next.regeneration.reused, ["body", "profile"]);
    assert_eq!(next.regeneration.rebuilt, ["rib", "placed"]);
    assert!((session.volume(next.shape("rib").unwrap()).unwrap() - 154.0).abs() < 1e-7);
    assert!(
        (session
            .center_of_mass(next.shape("rib").unwrap())
            .unwrap()
            .y
            - (500.0 + 54.0 * 4.0) / 154.0)
            .abs()
            < 1e-7
    );
    // Changing only the mode must also invalidate the feature signature.
    let mut one_sided = part(&definition);
    one_sided.overrides.insert(
        "thickness".into(),
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
    );
    let one_sided = one_sided.regenerate_incremental(&session, &next).unwrap();
    assert_eq!(one_sided.regeneration.reused, ["body", "profile"]);
    assert_eq!(one_sided.regeneration.rebuilt, ["rib", "placed"]);
    assert!(
        (session
            .center_of_mass(one_sided.shape("rib").unwrap())
            .unwrap()
            .y
            - (500.0 + 54.0 * 5.5) / 154.0)
            .abs()
            < 1e-7
    );
    drop((first, next, one_sided));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn centered_ribs_can_bridge_the_profile_plane_and_preserve_results_on_failed_mode_edits() {
    let session = Session::new().unwrap();
    let mut definition = rib_family(true);
    set_thickness_mode(&mut definition, RibThicknessMode::Centered);
    let FeatureOperation::SketchWire { sketch } = &mut definition.features[2].operation else {
        unreachable!()
    };
    sketch.origin = point(2.0, 10.0, 1.0);
    let first = part(&definition).regenerate(&session).unwrap();
    assert!((session.volume(first.shape("rib").unwrap()).unwrap() - 136.0).abs() < 1e-7);
    let mut failed = definition.clone();
    set_thickness_mode(&mut failed, RibThicknessMode::OneSided);
    assert!(
        part(&failed)
            .regenerate_incremental(&session, &first)
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 4);
    assert!(session.is_valid(first.shape("rib").unwrap()).unwrap());
    drop(first);
    for (y, z, height) in [(4.0, 3.0, 6.0), (11.0, 1.0, 6.0), (4.0, 0.0, 0.5)] {
        let mut invalid = definition.clone();
        let FeatureOperation::SketchWire { sketch } = &mut invalid.features[2].operation else {
            unreachable!()
        };
        sketch.origin = point(2.0, y, z);
        sketch.points[2].y = length(height);
        assert!(
            part(&invalid).regenerate(&session).is_err(),
            "y={y}, z={z}, height={height}"
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn centered_rib_mode_round_trips_and_legacy_ribs_default_to_one_sided() {
    let session = Session::new().unwrap();
    let mut definition = rib_family(true);
    set_thickness_mode(&mut definition, RibThicknessMode::Centered);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
    let result = part(&loaded.family).regenerate(&session).unwrap();
    assert!(
        (session
            .center_of_mass(result.shape("rib").unwrap())
            .unwrap()
            .y
            - (500.0 + 36.0 * 4.0) / 136.0)
            .abs()
            < 1e-7
    );
    drop(result);
    let mut old = serde_json::to_value(&document).unwrap();
    old["family"]["features"][0]["operation"]["rib"]
        .as_object_mut()
        .unwrap()
        .remove("thickness_mode");
    for version in [34, 35] {
        old["schema_version"] = serde_json::json!(version);
        let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
        let FeatureOperation::Rib { thickness_mode, .. } = migrated.family.features[0].operation
        else {
            unreachable!()
        };
        assert_eq!(thickness_mode, RibThicknessMode::OneSided);
        let generated = part(&migrated.family).regenerate(&session).unwrap();
        assert!(
            (session
                .center_of_mass(generated.shape("rib").unwrap())
                .unwrap()
                .y
                - 5.0)
                .abs()
                < 1e-7
        );
        drop(generated);
    }
    let mut unsupported = serde_json::to_value(&document).unwrap();
    unsupported["family"]["features"][0]["operation"]["rib"]["thickness_mode"] =
        serde_json::json!("unknown");
    assert!(ModelDocument::from_json(&unsupported.to_string()).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn profile_side_selector() -> FaceSelector {
    FaceSelector::GeneratedFromEdges {
        source_feature: "profile".into(),
        source: Box::new(EdgeSelector::Longest {
            allow_ties: false,
            relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-9)),
        }),
    }
}

fn assert_profile_side_history(wire: bool, mode: RibThicknessMode, sign: f64) {
    let session = Session::new().unwrap();
    let mut definition = rib_family(wire);
    if let FeatureOperation::Rib {
        thickness_mode,
        direction: axis,
        ..
    } = &mut definition.features[0].operation
    {
        *thickness_mode = mode;
        *axis = direction(sign);
    }
    let generated = part(&definition).regenerate(&session).unwrap();
    let faces = resolve_face_selector(
        &session,
        generated.shape("rib").unwrap(),
        &profile_side_selector(),
        &HashMap::new(),
        &generated.shapes,
    )
    .unwrap();
    assert_eq!(faces.len(), 1);
    let face = &faces[0];
    assert!((session.surface_area(face).unwrap() - 2.0 * 72.0_f64.sqrt()).abs() < 1e-8);
    let expected_y = if mode == RibThicknessMode::Centered {
        4.0
    } else {
        4.0 + sign.signum()
    };
    assert!((session.center_of_mass(face).unwrap().y - expected_y).abs() < 1e-8);
    let rib = generated.shape("rib").unwrap();
    assert!(
        (0..session.subshape_count(rib, ShapeType::Face).unwrap()).any(|index| {
            session
                .is_same(
                    face,
                    &session.subshape(rib, ShapeType::Face, index).unwrap(),
                )
                .unwrap()
        })
    );
    drop((faces, generated));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn rib_profile_edges_generate_selectable_faces_in_each_thickness_mode_and_direction() {
    use RibThicknessMode::{Centered, OneSided};
    for (wire, mode, sign) in [
        (false, OneSided, 3.0),
        (true, OneSided, 3.0),
        (false, OneSided, -3.0),
        (true, OneSided, -3.0),
        (false, Centered, 3.0),
        (true, Centered, 3.0),
        (false, Centered, -3.0),
        (true, Centered, -3.0),
    ] {
        assert_profile_side_history(wire, mode, sign);
    }
}

fn rib_with_history_draft() -> FamilyDefinition {
    let mut definition = rib_family(true);
    if let FeatureOperation::Rib { thickness_mode, .. } = &mut definition.features[0].operation {
        *thickness_mode = RibThicknessMode::Centered;
    }
    definition.features.insert(
        0,
        FeatureDefinition {
            id: "drafted".into(),
            operation: FeatureOperation::Draft {
                input: "rib".into(),
                faces: vec![profile_side_selector()],
                neutral_origin: point(0.0, 0.0, 1.0),
                neutral_normal: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                pull_direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(0.02)),
            },
        },
    );
    definition
}

#[test]
fn profile_history_drives_downstream_draft_round_trips_and_incremental_edits() {
    let definition = rib_with_history_draft();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
    let session = Session::new().unwrap();
    let mut instance = part(&loaded.family);
    let first = instance.regenerate(&session).unwrap();
    assert!(session.is_valid(first.shape("drafted").unwrap()).unwrap());
    assert!((session.volume(first.shape("drafted").unwrap()).unwrap() - 136.0).abs() > 1e-4);
    instance.overrides.insert(
        "thickness".into(),
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.reused, ["body", "profile"]);
    assert!(edited.regeneration.rebuilt.contains(&"drafted".into()));
    let faces = resolve_face_selector(
        &session,
        edited.shape("rib").unwrap(),
        &profile_side_selector(),
        &HashMap::new(),
        &edited.shapes,
    )
    .unwrap();
    assert!((session.surface_area(&faces[0]).unwrap() - 3.0 * 72.0_f64.sqrt()).abs() < 1e-8);
    drop((faces, first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn profile_history_selector_tracks_parameters_and_preserves_accepted_results_on_failure() {
    let mut definition = rib_with_history_draft();
    definition.parameters.push(ParameterDefinition {
        id: "edge_tolerance".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(1e-9)),
        minimum: None,
        maximum: None,
    });
    if let FeatureOperation::Draft { faces, .. } = &mut definition.features[0].operation {
        faces[0] = FaceSelector::GeneratedFromEdges {
            source_feature: "profile".into(),
            source: Box::new(EdgeSelector::Longest {
                allow_ties: false,
                relative_tolerance: ScalarExpr::Parameter("edge_tolerance".into()),
            }),
        };
    }
    let session = Session::new().unwrap();
    let mut instance = part(&definition);
    let first = instance.regenerate(&session).unwrap();
    let handles = session.shape_count().unwrap();
    instance.overrides.insert(
        "edge_tolerance".into(),
        ParameterValue::Scalar(Quantity::scalar(1e-8)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, ["drafted"]);
    assert_eq!(edited.regeneration.reused.len(), 4);
    drop(edited);
    assert_eq!(session.shape_count().unwrap(), handles);
    instance.overrides.insert(
        "edge_tolerance".into(),
        ParameterValue::Scalar(Quantity::scalar(1.0)),
    );
    assert!(instance.regenerate_incremental(&session, &first).is_err());
    assert_eq!(session.shape_count().unwrap(), handles);
    assert!(session.is_valid(first.shape("drafted").unwrap()).unwrap());
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn profile_history_rejects_removed_faces_and_unknown_source_features_without_leaks() {
    let session = Session::new().unwrap();
    let definition = rib_family(true);
    let generated = part(&definition).regenerate(&session).unwrap();
    let handles = session.shape_count().unwrap();
    let bottom = FaceSelector::GeneratedFromEdges {
        source_feature: "profile".into(),
        source: Box::new(EdgeSelector::NearestCenter {
            target: point(5.0, 4.0, 1.0),
            maximum_distance: length(0.001),
        }),
    };
    let error = resolve_face_selector(
        &session,
        generated.shape("rib").unwrap(),
        &bottom,
        &HashMap::new(),
        &generated.shapes,
    )
    .err()
    .unwrap();
    assert!(error.message.contains("resolved to no faces"), "{error:?}");
    assert_eq!(session.shape_count().unwrap(), handles);
    let mut invalid = rib_with_history_draft();
    if let FeatureOperation::Draft { faces, .. } = &mut invalid.features[0].operation
        && let FaceSelector::GeneratedFromEdges { source_feature, .. } = &mut faces[0]
    {
        *source_feature = "missing".into();
    }
    assert!(
        part(&invalid)
            .regenerate(&session)
            .err()
            .unwrap()
            .message
            .contains("unknown output 'missing'")
    );
    assert_eq!(session.shape_count().unwrap(), handles);
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn open_rib_family(mode: RibThicknessMode) -> FamilyDefinition {
    let mut definition = rib_family(true);
    let FeatureOperation::SketchWire { sketch } = &definition.features[2].operation else {
        unreachable!()
    };
    let mut sketch = (**sketch).clone();
    sketch.lines.truncate(1);
    sketch.points.truncate(2);
    definition.features[2].operation = FeatureOperation::SketchOpenWire {
        sketch: Box::new(sketch),
    };
    if let FeatureOperation::Rib {
        profile_mode,
        thickness_mode,
        ..
    } = &mut definition.features[0].operation
    {
        *profile_mode = RibProfileMode::OpenStrip {
            offset: VectorExpr::Components {
                x: length(0.0),
                y: length(0.0),
                z: ScalarExpr::Parameter("height".into()),
            },
        };
        *thickness_mode = mode;
    }
    definition
}

#[test]
fn open_sketch_ribs_have_exact_volume_and_composed_profile_history() {
    let session = Session::new().unwrap();
    for mode in [RibThicknessMode::OneSided, RibThicknessMode::Centered] {
        let definition = open_rib_family(mode);
        let generated = part(&definition).regenerate(&session).unwrap();
        let rib = generated.shape("rib").unwrap();
        assert!((session.volume(rib).unwrap() - 172.0).abs() < 1e-8);
        let faces = resolve_face_selector(
            &session,
            rib,
            &profile_side_selector(),
            &HashMap::new(),
            &generated.shapes,
        )
        .unwrap();
        assert_eq!(faces.len(), 1);
        assert!((session.surface_area(&faces[0]).unwrap() - 12.0).abs() < 1e-8);
        assert!((session.center_of_mass(&faces[0]).unwrap().z - 7.0).abs() < 1e-8);
        drop((faces, generated));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn open_rib_offset_edits_rebuild_dependents_and_failed_edits_preserve_geometry() {
    let mut definition = open_rib_family(RibThicknessMode::Centered);
    definition.parameters[1].minimum = None;
    let session = Session::new().unwrap();
    let mut instance = part(&definition);
    let first = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(8.0, LengthUnit::Millimeter)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    assert!(edited.regeneration.rebuilt.contains(&"rib".into()));
    assert_eq!(edited.regeneration.reused, ["body", "profile"]);
    assert!((session.volume(edited.shape("rib").unwrap()).unwrap() - 196.0).abs() < 1e-8);
    let handles = session.shape_count().unwrap();
    instance.overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
    );
    assert!(instance.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), handles);
    assert!((session.volume(edited.shape("rib").unwrap()).unwrap() - 196.0).abs() < 1e-8);
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn open_ribs_round_trip_and_previous_models_default_to_closed_profiles() {
    let definition = open_rib_family(RibThicknessMode::Centered);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    let session = Session::new().unwrap();
    let generated = part(&loaded.family).regenerate(&session).unwrap();
    drop(generated);
    let legacy = rib_family(true);
    let mut graph = InstanceGraph::new(&legacy);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut value = serde_json::to_value(ModelDocument::from_graph(&graph)).unwrap();
    value["schema_version"] = serde_json::json!(37);
    value["family"]["features"][0]["operation"]["rib"]
        .as_object_mut()
        .unwrap()
        .remove("profile_mode");
    let migrated = ModelDocument::from_json(&value.to_string()).unwrap();
    assert_eq!(migrated.family, legacy);
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn invalid_open_profile(case: usize) -> FamilyDefinition {
    let mut definition = open_rib_family(RibThicknessMode::Centered);
    if let FeatureOperation::SketchOpenWire { sketch } = &mut definition.features[2].operation {
        match case {
            0 => sketch.lines.push(SketchLine {
                id: "other".into(),
                start: "a".into(),
                end: "b".into(),
            }),
            1 => sketch.lines.push(SketchLine {
                id: "return".into(),
                start: "b".into(),
                end: "a".into(),
            }),
            2 => {
                sketch.lines.clear();
                sketch.circles.push(SketchCircle {
                    id: "circle".into(),
                    center: "a".into(),
                    rim: "b".into(),
                });
            }
            _ => {}
        }
    }
    if case >= 3
        && let FeatureOperation::Rib { profile_mode, .. } = &mut definition.features[0].operation
    {
        *profile_mode = RibProfileMode::OpenStrip {
            offset: if case == 3 {
                VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 6.0))
            } else {
                point(1.0, 0.0, 0.0)
            },
        };
    }
    definition
}

#[test]
fn open_profiles_reject_bad_chains_closure_geometry_and_units_without_leaks() {
    let session = Session::new().unwrap();
    for case in 0..5 {
        let definition = invalid_open_profile(case);
        assert!(
            part(&definition).regenerate(&session).is_err(),
            "case {case}"
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

fn scaled_open_rib(offset: f64, scale: f64, sign: f64) -> FamilyDefinition {
    let mut definition = open_rib_family(RibThicknessMode::Centered);
    if let FeatureOperation::Rib {
        direction: axis, ..
    } = &mut definition.features[0].operation
    {
        *axis = direction(sign);
    }
    if let FeatureOperation::Box { origin, size } = &mut definition.features[1].operation {
        *origin = point(0.0, offset, 0.0);
        *size = point(10.0 * scale, 10.0 * scale, scale);
    }
    for parameter in &mut definition.parameters {
        parameter.minimum = None;
        if let ParameterValue::Scalar(value) = &mut parameter.default {
            value.value *= scale;
        }
    }
    if let FeatureOperation::SketchOpenWire { sketch } = &mut definition.features[2].operation {
        sketch.origin = point(2.0 * scale, offset + 4.0 * scale, scale);
        sketch.points[1].x = length(6.0 * scale);
    }
    definition
}

#[test]
fn centered_open_ribs_handle_small_large_and_distant_models_with_reversed_directions() {
    let session = Session::new().unwrap();
    for (offset, scale, sign) in [
        (0.0, 0.01, 3.0),
        (0.0, 100_000.0, -3.0),
        (1_000_000.0, 1.0, 3.0),
        (1_000_000.0, 1.0, -3.0),
    ] {
        let definition = scaled_open_rib(offset, scale, sign);
        let generated = part(&definition).regenerate(&session).unwrap();
        let rib = generated.shape("rib").unwrap();
        assert!(
            (session.volume(rib).unwrap() - 172.0 * scale.powi(3)).abs() < 1e-7 * scale.powi(3)
        );
        assert!(
            (session.center_of_mass(rib).unwrap().y - (offset + scale * 788.0 / 172.0)).abs()
                < 1e-7 * scale.max(1.0)
        );
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn open_arc_sketch_profiles_generate_valid_reinforcing_solids() {
    let mut definition = open_rib_family(RibThicknessMode::Centered);
    if let FeatureOperation::SketchOpenWire { sketch } = &mut definition.features[2].operation {
        sketch.points.push(SketchPoint {
            id: "center".into(),
            x: length(3.0),
            y: length(0.0),
            fixed: true,
        });
        sketch.lines.clear();
        sketch.arcs.push(SketchArc {
            id: "curve".into(),
            center: "center".into(),
            start: "a".into(),
            end: "b".into(),
            clockwise: false,
        });
        sketch.profile = vec!["curve".into()];
    }
    let session = Session::new().unwrap();
    let generated = part(&definition).regenerate(&session).unwrap();
    let rib = generated.shape("rib").unwrap();
    assert!(session.is_valid(rib).unwrap());
    assert_eq!(session.subshape_count(rib, ShapeType::Solid).unwrap(), 1);
    assert!(session.volume(rib).unwrap() > 100.0);
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn next_rib_family(mode: RibThicknessMode) -> FamilyDefinition {
    let mut definition = open_rib_family(mode);
    definition.parameters.push(length_parameter("reach", 10.0));
    if let FeatureOperation::SketchOpenWire { sketch } = &mut definition.features[2].operation {
        sketch.origin = point(2.0, 4.0, 7.0);
    }
    if let FeatureOperation::Rib { profile_mode, .. } = &mut definition.features[0].operation {
        *profile_mode = RibProfileMode::OpenToNext {
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -3.0)),
            maximum_length: ScalarExpr::Parameter("reach".into()),
        };
    }
    definition
}

#[test]
fn extend_to_next_ribs_preserve_exact_geometry_history_and_thickness_placement() {
    let session = Session::new().unwrap();
    for mode in [RibThicknessMode::OneSided, RibThicknessMode::Centered] {
        for sign in [-1.0, 1.0] {
            let mut definition = next_rib_family(mode);
            if let FeatureOperation::Rib {
                direction: axis, ..
            } = &mut definition.features[0].operation
            {
                *axis = direction(sign);
            }
            let generated = part(&definition).regenerate(&session).unwrap();
            let rib = generated.shape("rib").unwrap();
            assert!((session.volume(rib).unwrap() - 172.0).abs() < 1e-8);
            let side = resolve_face_selector(
                &session,
                rib,
                &profile_side_selector(),
                &HashMap::new(),
                &generated.shapes,
            )
            .unwrap();
            assert_eq!(side.len(), 1);
            assert!((session.surface_area(&side[0]).unwrap() - 12.0).abs() < 1e-8);
            let center = session.center_of_mass(&side[0]).unwrap();
            assert!((center.z - 7.0).abs() < 1e-8);
            let y = 4.0
                + if mode == RibThicknessMode::Centered {
                    0.0
                } else {
                    sign
                };
            assert!((center.y - y).abs() < 1e-8);
            assert!(
                (session.volume(generated.shape("body").unwrap()).unwrap() - 100.0).abs() < 1e-8
            );
            drop((side, generated));
            assert_eq!(session.shape_count().unwrap(), 0);
        }
    }
}

#[test]
fn extend_to_next_reach_edits_rebuild_only_ribs_and_preserve_accepted_results() {
    let definition = next_rib_family(RibThicknessMode::Centered);
    let session = Session::new().unwrap();
    let mut instance = part(&definition);
    let first = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "reach".into(),
        ParameterValue::Scalar(Quantity::length(6.0, LengthUnit::Millimeter)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, ["rib", "placed"]);
    assert_eq!(edited.regeneration.reused, ["body", "profile"]);
    assert!((session.volume(edited.shape("rib").unwrap()).unwrap() - 172.0).abs() < 1e-8);
    let handles = session.shape_count().unwrap();
    instance.overrides.insert(
        "reach".into(),
        ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
    );
    let error = instance
        .regenerate_incremental(&session, &edited)
        .err()
        .unwrap();
    assert!(error.message.contains("no rib support"), "{error:?}");
    assert_eq!(session.shape_count().unwrap(), handles);
    assert!(session.is_valid(edited.shape("rib").unwrap()).unwrap());
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn extend_to_next_body_edits_recompute_support_without_rebuilding_profile() {
    let mut definition = next_rib_family(RibThicknessMode::Centered);
    definition
        .parameters
        .push(length_parameter("base_height", 1.0));
    if let FeatureOperation::Box { size, .. } = &mut definition.features[1].operation {
        *size = VectorExpr::Components {
            x: length(10.0),
            y: length(10.0),
            z: ScalarExpr::Parameter("base_height".into()),
        };
    }
    let session = Session::new().unwrap();
    let mut instance = part(&definition);
    let first = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "base_height".into(),
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, ["body", "rib", "placed"]);
    assert_eq!(edited.regeneration.reused, ["profile"]);
    assert!((session.volume(edited.shape("rib").unwrap()).unwrap() - 348.0).abs() < 1e-8);
    let handles = session.shape_count().unwrap();
    instance.overrides.insert(
        "base_height".into(),
        ParameterValue::Scalar(Quantity::length(8.0, LengthUnit::Millimeter)),
    );
    assert!(instance.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), handles);
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn extend_to_next_modes_round_trip_and_validate_units_and_unsupported_geometry() {
    let definition = next_rib_family(RibThicknessMode::Centered);
    let document = ModelDocument::from_graph(&InstanceGraph::new(&definition));
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded.family, definition);
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
    let session = Session::new().unwrap();
    let accepted = part(&loaded.family).regenerate(&session).unwrap();
    let handles = session.shape_count().unwrap();
    for case in 0..5 {
        let mut invalid = definition.clone();
        if let FeatureOperation::Rib { profile_mode, .. } = &mut invalid.features[0].operation {
            let (advance, reach) = match case {
                0 => (point(0.0, 0.0, -1.0), length(10.0)),
                1 => (
                    VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
                    ScalarExpr::Literal(Quantity::scalar(10.0)),
                ),
                2 => (
                    VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    length(10.0),
                ),
                3 => (
                    VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0)),
                    length(10.0),
                ),
                _ => (
                    VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
                    length(-1.0),
                ),
            };
            *profile_mode = RibProfileMode::OpenToNext {
                direction: advance,
                maximum_length: reach,
            };
        }
        assert!(part(&invalid).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), handles);
    }
    // A rising profile cannot be closed uniformly at its earliest contact.
    let mut invalid = definition.clone();
    if let FeatureOperation::SketchOpenWire { sketch } = &mut invalid.features[2].operation {
        sketch.points[1].y = length(1.0);
    }
    assert!(part(&invalid).regenerate(&session).is_err());
    assert_eq!(session.shape_count().unwrap(), handles);
    // Earlier bounded profiles retain their serialized mode and geometry.
    let bounded = open_rib_family(RibThicknessMode::Centered);
    let older = ModelDocument::from_graph(&InstanceGraph::new(&bounded));
    let mut json = serde_json::to_value(&older).unwrap();
    json["schema_version"] = serde_json::json!(38);
    let migrated = ModelDocument::from_json(&json.to_string()).unwrap();
    assert_eq!(migrated.family, older.family);
    let generated = part(&migrated.family).regenerate(&session).unwrap();
    assert!((session.volume(generated.shape("rib").unwrap()).unwrap() - 172.0).abs() < 1e-8);
    drop((generated, accepted));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn extend_to_next_ribs_work_at_small_large_and_distant_scales() {
    let session = Session::new().unwrap();
    for (scale, origin) in [(0.01, 0.0), (100_000.0, 0.0), (1.0, 1_000_000.0)] {
        let mut definition = next_rib_family(RibThicknessMode::Centered);
        definition.parameters[0].minimum = None;
        definition.parameters[0].maximum = None;
        definition.parameters[2].minimum = None;
        definition.parameters[2].maximum = None;
        definition.parameters[0].default =
            ParameterValue::Scalar(Quantity::length(2.0 * scale, LengthUnit::Millimeter));
        definition.parameters[2].default =
            ParameterValue::Scalar(Quantity::length(10.0 * scale, LengthUnit::Millimeter));
        if let FeatureOperation::Box { origin: base, size } = &mut definition.features[1].operation
        {
            *base = point(origin, origin, origin);
            *size = point(10.0 * scale, 10.0 * scale, scale);
        }
        if let FeatureOperation::SketchOpenWire { sketch } = &mut definition.features[2].operation {
            sketch.origin = point(
                origin + 2.0 * scale,
                origin + 4.0 * scale,
                origin + 7.0 * scale,
            );
            sketch.points[1].x = length(6.0 * scale);
        }
        let generated = part(&definition).regenerate(&session).unwrap();
        let rib = generated.shape("rib").unwrap();
        let expected = 172.0 * scale.powi(3);
        assert!((session.volume(rib).unwrap() - expected).abs() < expected * 1e-8);
        let center = session.center_of_mass(rib).unwrap();
        assert!((center.z - (origin + (50.0 + 72.0 * 4.0) / 172.0 * scale)).abs() < scale * 1e-7);
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn extend_to_next_advance_parameter_edits_participate_in_signatures() {
    let mut definition = next_rib_family(RibThicknessMode::Centered);
    definition.parameters.push(ParameterDefinition {
        id: "advance".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(-1.0)),
        minimum: None,
        maximum: None,
    });
    if let FeatureOperation::Rib { profile_mode, .. } = &mut definition.features[0].operation {
        *profile_mode = RibProfileMode::OpenToNext {
            direction: VectorExpr::Components {
                x: ScalarExpr::Literal(Quantity::scalar(0.0)),
                y: ScalarExpr::Literal(Quantity::scalar(0.0)),
                z: ScalarExpr::Parameter("advance".into()),
            },
            maximum_length: ScalarExpr::Parameter("reach".into()),
        };
    }
    let session = Session::new().unwrap();
    let mut instance = part(&definition);
    let first = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "advance".into(),
        ParameterValue::Scalar(Quantity::scalar(-2.0)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, ["rib", "placed"]);
    assert_eq!(edited.regeneration.reused, ["body", "profile"]);
    assert!((session.volume(edited.shape("rib").unwrap()).unwrap() - 172.0).abs() < 1e-8);
    let handles = session.shape_count().unwrap();
    for advance in [0.0, 1.0] {
        instance.overrides.insert(
            "advance".into(),
            ParameterValue::Scalar(Quantity::scalar(advance)),
        );
        assert!(instance.regenerate_incremental(&session, &edited).is_err());
        assert_eq!(session.shape_count().unwrap(), handles);
    }
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}
