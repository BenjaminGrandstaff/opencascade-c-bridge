use super::*;

fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn definition() -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Advisory, 100_000.0);
    definition.requirements.clear();
    for (id, value) in [("start", 1.0), ("end", 2.0)] {
        let mut parameter = length_parameter(id, value);
        parameter.minimum = None;
        definition.parameters.push(parameter);
    }
    definition.parameters.push(ParameterDefinition {
        id: "target".into(),
        parameter_type: ParameterType::Vector(Dimension::Length),
        default: ParameterValue::Vector(VectorQuantity::lengths(
            0.0,
            0.0,
            5.0,
            LengthUnit::Millimeter,
        )),
        minimum: None,
        maximum: None,
    });
    // Deliberately list dependents before their inputs.
    definition.features = vec![
        FeatureDefinition {
            id: "placed".into(),
            operation: FeatureOperation::Translate {
                input: "blend".into(),
                offset: point(20.0, 0.0, 0.0),
            },
        },
        FeatureDefinition {
            id: "blend".into(),
            operation: FeatureOperation::VariableFillet {
                input: "body".into(),
                edges: vec![EdgeSelector::NearestCenter {
                    target: VectorExpr::Parameter("target".into()),
                    maximum_distance: ScalarExpr::Literal(Quantity::length(
                        1e-6,
                        LengthUnit::Millimeter,
                    )),
                }],
                start_radius: ScalarExpr::Parameter("start".into()),
                end_radius: ScalarExpr::Parameter("end".into()),
                stations: Vec::new(),
                spine_direction: FilletSpineDirection::Kernel,
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
            id: "spare".into(),
            operation: FeatureOperation::Box {
                origin: point(30.0, 0.0, 0.0),
                size: point(1.0, 1.0, 1.0),
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
fn variable_fillet_tracks_both_radii_and_selectors_and_reuses_other_branches() {
    let session = Session::new().unwrap();
    let definition = definition();
    let first = part(&definition).regenerate(&session).unwrap();
    let original_volume = session.volume(first.shape("blend").unwrap()).unwrap();
    for (id, value) in [
        (
            "start",
            ParameterValue::Scalar(Quantity::length(0.5, LengthUnit::Millimeter)),
        ),
        (
            "end",
            ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
        ),
        (
            "target",
            ParameterValue::Vector(VectorQuantity::lengths(
                10.0,
                0.0,
                5.0,
                LengthUnit::Millimeter,
            )),
        ),
    ] {
        let mut edited = part(&definition);
        edited.overrides.insert(id.into(), value);
        let next = edited.regenerate_incremental(&session, &first).unwrap();
        assert_eq!(next.regeneration.reused, ["body", "spare"]);
        assert_eq!(next.regeneration.rebuilt, ["blend", "placed"]);
        assert!(session.is_valid(next.shape("blend").unwrap()).unwrap());
        if id != "target" {
            assert!(
                (session.volume(next.shape("blend").unwrap()).unwrap() - original_volume).abs()
                    > 0.1
            );
        } else {
            assert!(
                (session
                    .center_of_mass(next.shape("blend").unwrap())
                    .unwrap()
                    .x
                    - session
                        .center_of_mass(first.shape("blend").unwrap())
                        .unwrap()
                        .x)
                    .abs()
                    > 1e-3
            );
        }
        assert!((session.volume(first.shape("body").unwrap()).unwrap() - 1000.0).abs() < 1e-7);
        drop(next);
        assert_eq!(session.shape_count().unwrap(), 4);
    }
    // Equivalent lengths in other units preserve the geometric result.
    let mut converted = part(&definition);
    converted.overrides.insert(
        "start".into(),
        ParameterValue::Scalar(Quantity::length(0.1, LengthUnit::Centimeter)),
    );
    converted.overrides.insert(
        "end".into(),
        ParameterValue::Scalar(Quantity::length(0.002, LengthUnit::Meter)),
    );
    let converted = converted.regenerate_incremental(&session, &first).unwrap();
    assert!(
        (session.volume(converted.shape("blend").unwrap()).unwrap() - original_volume).abs() < 1e-6
    );
    drop((first, converted));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn variable_fillet_failed_edits_retain_accepted_results_and_selector_diagnostics() {
    let session = Session::new().unwrap();
    let definition = definition();
    let mut managed = ManagedPartInstance::new(&session, part(&definition));
    managed.regenerate().unwrap();
    let original = session
        .volume(managed.accepted().unwrap().shape("blend").unwrap())
        .unwrap();
    for (id, value) in [("start", 0.0), ("end", -1.0), ("start", 20.0)] {
        managed.instance_mut().overrides.clear();
        managed.instance_mut().overrides.insert(
            id.into(),
            ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
        );
        if value == 20.0 {
            managed.instance_mut().overrides.insert(
                "end".into(),
                ParameterValue::Scalar(Quantity::length(30.0, LengthUnit::Millimeter)),
            );
        }
        let error = managed.regenerate().unwrap_err();
        assert_eq!(managed.state(), RegenerationState::Stale);
        assert_eq!(managed.accepted_revision(), Some(1));
        assert_eq!(session.shape_count().unwrap(), 4);
        assert!(
            (session
                .volume(managed.accepted().unwrap().shape("blend").unwrap())
                .unwrap()
                - original)
                .abs()
                < 1e-7
        );
        if value == 20.0 {
            assert!(
                error
                    .diagnostics
                    .iter()
                    .any(|d| d.feature == "blend" && d.selector == Some(0)),
                "{error:?}"
            );
        }
    }
    managed.instance_mut().overrides.clear();
    managed.regenerate().unwrap();
    assert_eq!(managed.state(), RegenerationState::Current);
    assert_eq!(managed.accepted().unwrap().regeneration.reused.len(), 4);
    drop(managed);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn variable_fillet_invalid_units_selections_and_references_release_handles() {
    let session = Session::new().unwrap();
    for case in 0..6 {
        let mut definition = definition();
        let FeatureOperation::VariableFillet {
            input,
            edges,
            start_radius,
            end_radius,
            ..
        } = &mut definition.features[1].operation
        else {
            unreachable!()
        };
        match case {
            0 => *start_radius = ScalarExpr::Literal(Quantity::scalar(1.0)),
            1 => *end_radius = ScalarExpr::Literal(Quantity::scalar(2.0)),
            2 => edges.clear(),
            3 => edges.push(edges[0].clone()),
            4 => *input = "missing".into(),
            _ => *start_radius = ScalarExpr::Parameter("missing".into()),
        }
        assert!(part(&definition).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0, "case {case}");
    }
}

#[test]
fn variable_fillet_schema_round_trip_and_older_fillets_keep_their_operations() {
    let definition = definition();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded, document);
    let session = Session::new().unwrap();
    let generated = loaded
        .instance_graph()
        .unwrap()
        .regenerate_all(&session)
        .unwrap();
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut old = document.clone();
    old.family.features[1].operation = FeatureOperation::Fillet {
        input: "body".into(),
        edges: vec![EdgeSelector::NearestCenter {
            target: point(0.0, 0.0, 5.0),
            maximum_distance: ScalarExpr::Literal(Quantity::length(1e-6, LengthUnit::Millimeter)),
        }],
        radius: ScalarExpr::Parameter("start".into()),
    };
    let json = serde_json::to_value(&old).unwrap();
    for version in 1..CURRENT_SCHEMA_VERSION {
        let mut json = json.clone();
        json["schema_version"] = serde_json::json!(version);
        let migrated = ModelDocument::from_json(&json.to_string()).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.family.features, old.family.features);
    }
}

fn station_definition() -> FamilyDefinition {
    let mut definition = definition();
    definition.parameters.push(length_parameter("middle", 2.5));
    definition.parameters.push(ParameterDefinition {
        id: "position".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(0.25)),
        minimum: None,
        maximum: None,
    });
    definition.parameters.push(ParameterDefinition {
        id: "spine_start".into(),
        parameter_type: ParameterType::Vector(Dimension::Length),
        default: ParameterValue::Vector(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        minimum: None,
        maximum: None,
    });
    if let FeatureOperation::VariableFillet {
        stations,
        spine_direction,
        ..
    } = &mut definition.features[1].operation
    {
        stations.push(FilletRadiusStation {
            position: ScalarExpr::Parameter("position".into()),
            radius: ScalarExpr::Parameter("middle".into()),
        });
        *spine_direction = FilletSpineDirection::FromPoint {
            point: VectorExpr::Parameter("spine_start".into()),
        };
    }
    definition
}

#[test]
fn station_fillet_edits_track_samples_start_points_and_downstream_features() {
    let definition = station_definition();
    let session = Session::new().unwrap();
    let first = part(&definition).regenerate(&session).unwrap();
    let original = session.volume(first.shape("blend").unwrap()).unwrap();
    for (id, value) in [
        (
            "middle",
            ParameterValue::Scalar(Quantity::length(2.0, LengthUnit::Millimeter)),
        ),
        ("position", ParameterValue::Scalar(Quantity::scalar(0.5))),
        (
            "spine_start",
            ParameterValue::Vector(VectorQuantity::lengths(
                0.0,
                0.0,
                10.0,
                LengthUnit::Millimeter,
            )),
        ),
    ] {
        let mut instance = part(&definition);
        instance.overrides.insert(id.into(), value);
        let edited = instance.regenerate_incremental(&session, &first).unwrap();
        assert_eq!(edited.regeneration.rebuilt, ["blend", "placed"]);
        assert_eq!(edited.regeneration.reused, ["body", "spare"]);
        if id == "spine_start" {
            let sum = session
                .center_of_mass(first.shape("blend").unwrap())
                .unwrap()
                .z
                + session
                    .center_of_mass(edited.shape("blend").unwrap())
                    .unwrap()
                    .z;
            assert!((sum - 10.0).abs() < 1e-5);
        } else {
            assert!(
                (session.volume(edited.shape("blend").unwrap()).unwrap() - original).abs() > 0.1
            );
        }
        drop(edited);
        assert_eq!(session.shape_count().unwrap(), 4);
    }
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn station_fillet_bad_samples_and_ambiguous_spine_preserve_accepted_results() {
    let definition = station_definition();
    let session = Session::new().unwrap();
    let mut managed = ManagedPartInstance::new(&session, part(&definition));
    managed.regenerate().unwrap();
    for (id, value) in [
        ("position", ParameterValue::Scalar(Quantity::scalar(0.0))),
        ("position", ParameterValue::Scalar(Quantity::scalar(1.0))),
        (
            "middle",
            ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
        ),
        (
            "middle",
            ParameterValue::Scalar(Quantity::length(20.0, LengthUnit::Millimeter)),
        ),
        (
            "spine_start",
            ParameterValue::Vector(VectorQuantity::lengths(
                0.0,
                0.0,
                5.0,
                LengthUnit::Millimeter,
            )),
        ),
    ] {
        managed.instance_mut().overrides.clear();
        managed.instance_mut().overrides.insert(id.into(), value);
        assert!(managed.regenerate().is_err());
        assert_eq!(managed.accepted_revision(), Some(1));
        assert_eq!(session.shape_count().unwrap(), 4);
        assert!(
            session
                .is_valid(managed.accepted().unwrap().shape("blend").unwrap())
                .unwrap()
        );
    }
    drop(managed);
    for case in 0..3 {
        let mut invalid = definition.clone();
        if let FeatureOperation::VariableFillet {
            stations,
            spine_direction,
            ..
        } = &mut invalid.features[1].operation
        {
            match case {
                0 => {
                    stations[0].position =
                        ScalarExpr::Literal(Quantity::length(0.5, LengthUnit::Millimeter))
                }
                1 => stations[0].radius = ScalarExpr::Literal(Quantity::scalar(2.0)),
                _ => {
                    *spine_direction = FilletSpineDirection::FromPoint {
                        point: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0)),
                    }
                }
            }
        }
        assert!(part(&invalid).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn station_fillet_documents_round_trip_and_schema_39_preserves_linear_defaults() {
    let station_family = station_definition();
    let mut graph = InstanceGraph::new(&station_family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    let legacy = definition();
    let mut graph = InstanceGraph::new(&legacy);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut json = serde_json::to_value(ModelDocument::from_graph(&graph)).unwrap();
    json["schema_version"] = serde_json::json!(39);
    let operation = json["family"]["features"][1]["operation"]["variable_fillet"]
        .as_object_mut()
        .unwrap();
    operation.remove("stations");
    operation.remove("spine_direction");
    let migrated = ModelDocument::from_json(&json.to_string()).unwrap();
    assert_eq!(migrated.family, legacy);
    let session = Session::new().unwrap();
    let first = part(&legacy).regenerate(&session).unwrap();
    let second = part(&migrated.family).regenerate(&session).unwrap();
    assert!(
        (session.volume(first.shape("blend").unwrap()).unwrap()
            - session.volume(second.shape("blend").unwrap()).unwrap())
        .abs()
            < 1e-7
    );
    drop((first, second));
    assert_eq!(session.shape_count().unwrap(), 0);
}
