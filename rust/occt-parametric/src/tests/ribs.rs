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
        assert_eq!(document.schema_version, 34);
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
        assert_eq!(migrated.schema_version, 34);
        assert_eq!(migrated.family.features[0], definition.features[1]);
    }
}
