use super::*;

fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn dir(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::scalars(x, y, z))
}
fn definition() -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Advisory, 100_000.0);
    definition.requirements.clear();
    definition.parameters.push(ParameterDefinition {
        id: "angle".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(0.1)),
        minimum: None,
        maximum: None,
    });
    definition.features = vec![
        FeatureDefinition {
            id: "draft".into(),
            operation: FeatureOperation::Draft {
                input: "body".into(),
                faces: vec![FaceSelector::AtExtreme {
                    axis: CoordinateAxis::X,
                    extremum: Extremum::Maximum,
                    tolerance: ScalarExpr::Literal(Quantity::length(1e-6, LengthUnit::Millimeter)),
                }],
                neutral_origin: point(0.0, 0.0, 0.0),
                neutral_normal: dir(0.0, 0.0, 2.0),
                pull_direction: dir(0.0, 0.0, 3.0),
                angle_radians: ScalarExpr::Parameter("angle".into()),
            },
        },
        FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: point(0.0, 0.0, 0.0),
                size: point(10.0, 10.0, 10.0),
            },
        },
        FeatureDefinition {
            id: "placed".into(),
            operation: FeatureOperation::Translate {
                input: "draft".into(),
                offset: point(20.0, 0.0, 0.0),
            },
        },
    ];
    definition
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
fn draft_edits_reuse_input_and_preserve_previous_generation_on_failure() {
    let session = Session::new().unwrap();
    let definition = definition();
    let first = part(&definition).regenerate(&session).unwrap();
    assert!(
        (session.volume(first.shape("draft").unwrap()).unwrap() - (1000.0 - 500.0 * 0.1_f64.tan()))
            .abs()
            < 1e-7
    );
    assert_eq!(session.shape_count().unwrap(), 3);
    let mut edited = part(&definition);
    edited.overrides.insert(
        "angle".into(),
        ParameterValue::Scalar(Quantity::scalar(-0.1)),
    );
    let next = edited.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(next.regeneration.reused, ["body"]);
    assert_eq!(next.regeneration.rebuilt, ["draft", "placed"]);
    assert!(
        (session.volume(next.shape("draft").unwrap()).unwrap() - (1000.0 + 500.0 * 0.1_f64.tan()))
            .abs()
            < 1e-7
    );
    drop(next);
    let count = session.shape_count().unwrap();
    edited.overrides.insert(
        "angle".into(),
        ParameterValue::Scalar(Quantity::scalar(0.0)),
    );
    assert!(edited.regenerate_incremental(&session, &first).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(first.shape("draft").unwrap()).unwrap());
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn draft_rejects_units_empty_missing_and_duplicate_selections_without_leaks() {
    let session = Session::new().unwrap();
    for case in 0..8 {
        let mut definition = definition();
        if let FeatureOperation::Draft {
            faces,
            neutral_origin,
            neutral_normal,
            pull_direction,
            angle_radians,
            ..
        } = &mut definition.features[0].operation
        {
            match case {
                0 => faces.clear(),
                1 => faces.push(faces[0].clone()),
                2 => *neutral_origin = dir(0.0, 0.0, 0.0),
                3 => *neutral_normal = point(0.0, 0.0, 1.0),
                4 => *pull_direction = dir(0.0, 0.0, 0.0),
                5 => {
                    *angle_radians =
                        ScalarExpr::Literal(Quantity::length(0.1, LengthUnit::Millimeter))
                }
                6 => faces.push(FaceSelector::NearestCenter {
                    target: point(100.0, 100.0, 100.0),
                    maximum_distance: ScalarExpr::Literal(Quantity::length(
                        0.1,
                        LengthUnit::Millimeter,
                    )),
                }),
                _ => *angle_radians = ScalarExpr::Literal(Quantity::scalar(1.5)),
            }
        }
        let error = part(&definition).regenerate(&session).err().unwrap();
        assert!(error.message.contains("draft"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn draft_schema_thirty_three_round_trips_and_migrates_previous_models() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    let mut old = serde_json::to_value(&document).unwrap();
    old["schema_version"] = serde_json::json!(32);
    old["family"]["features"]
        .as_array_mut()
        .unwrap()
        .retain(|feature| feature["id"] == "body");
    let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(migrated.family.features[0], definition.features[1]);
}

#[test]
fn draft_plane_and_direction_parameter_edits_are_tracked() {
    let session = Session::new().unwrap();
    let mut definition = definition();
    for (id, dimension, default) in [
        (
            "origin",
            Dimension::Length,
            VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        ),
        (
            "normal",
            Dimension::Scalar,
            VectorQuantity::scalars(0.0, 0.0, 1.0),
        ),
        (
            "pull",
            Dimension::Scalar,
            VectorQuantity::scalars(0.0, 0.0, 1.0),
        ),
    ] {
        definition.parameters.push(ParameterDefinition {
            id: id.into(),
            parameter_type: ParameterType::Vector(dimension),
            default: ParameterValue::Vector(default),
            minimum: None,
            maximum: None,
        });
    }
    if let FeatureOperation::Draft {
        neutral_origin,
        neutral_normal,
        pull_direction,
        ..
    } = &mut definition.features[0].operation
    {
        *neutral_origin = VectorExpr::Parameter("origin".into());
        *neutral_normal = VectorExpr::Parameter("normal".into());
        *pull_direction = VectorExpr::Parameter("pull".into());
    }
    let first = part(&definition).regenerate(&session).unwrap();
    for (id, value, removed) in [
        (
            "origin",
            VectorQuantity::lengths(0.0, 0.0, 2.0, LengthUnit::Millimeter),
            300.0 * 0.1_f64.tan(),
        ),
        (
            "normal",
            VectorQuantity::scalars(0.0, 0.0, 3.0),
            500.0 * 0.1_f64.tan(),
        ),
        (
            "pull",
            VectorQuantity::scalars(0.0, 0.0, 4.0),
            500.0 * 0.1_f64.tan(),
        ),
    ] {
        let mut edited = part(&definition);
        edited
            .overrides
            .insert(id.into(), ParameterValue::Vector(value));
        let next = edited.regenerate_incremental(&session, &first).unwrap();
        assert_eq!(next.regeneration.reused, ["body"]);
        assert_eq!(next.regeneration.rebuilt, ["draft", "placed"]);
        assert!(
            (session.volume(next.shape("draft").unwrap()).unwrap() - (1000.0 - removed)).abs()
                < 1e-7
        );
    }
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
}
