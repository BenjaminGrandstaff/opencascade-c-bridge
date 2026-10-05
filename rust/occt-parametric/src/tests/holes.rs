//! Parametric bores: geometry, history, incremental reuse, and failures.

use super::*;

fn position(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn direction(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::scalars(x, y, z))
}

fn bore(extent: HoleExtent) -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "Bore".into(),
        version: 1,
        parameters: vec![
            length_parameter("diameter", 2.0),
            length_parameter("depth", 4.0),
        ],
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        datums: Vec::new(),
        requirements: Vec::new(),
        features: vec![
            FeatureDefinition {
                id: "hole".into(),
                operation: FeatureOperation::Hole {
                    input: "body".into(),
                    position: position(5.0, 5.0, 10.0),
                    axis: direction(0.0, 0.0, -3.0),
                    diameter: ScalarExpr::Parameter("diameter".into()),
                    extent,
                    finish: HoleFinish::Plain,
                    thread: None,
                },
            },
            FeatureDefinition {
                id: "body".into(),
                operation: FeatureOperation::Box {
                    origin: position(0.0, 0.0, 0.0),
                    size: position(10.0, 10.0, 10.0),
                },
            },
            FeatureDefinition {
                id: "placed".into(),
                operation: FeatureOperation::Translate {
                    input: "hole".into(),
                    offset: position(20.0, 0.0, 0.0),
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

fn length_expr(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

fn finished_bore(extent: HoleExtent, finish: HoleFinish) -> FamilyDefinition {
    let mut definition = bore(extent);
    if let FeatureOperation::Hole { finish: entry, .. } = &mut definition.features[0].operation {
        *entry = finish;
    }
    definition
}

#[test]
fn entry_recesses_remove_exact_volume_with_history_and_cleanup() {
    let session = Session::new().unwrap();
    for (finish, extra) in [
        (
            HoleFinish::Counterbore {
                diameter: length_expr(4.0),
                depth: length_expr(2.0),
            },
            6.0 * std::f64::consts::PI,
        ),
        (
            HoleFinish::Countersink {
                diameter: length_expr(4.0),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
            },
            4.0 * std::f64::consts::PI / 3.0,
        ),
    ] {
        for (extent, bore_volume) in [
            (HoleExtent::ThroughAll, 10.0 * std::f64::consts::PI),
            (
                HoleExtent::Blind {
                    depth: length_expr(4.0),
                },
                4.0 * std::f64::consts::PI,
            ),
        ] {
            let definition = finished_bore(extent, finish.clone());
            let generated = part(&definition).regenerate(&session).unwrap();
            let result = generated.shape("hole").unwrap();
            assert!(
                (session.volume(result).unwrap() - (1000.0 - bore_volume - extra)).abs() < 1e-7
            );
            assert!(session.is_valid(result).unwrap());
            let body = generated.shape("body").unwrap();
            let mut modified = 0;
            for index in 0..session.subshape_count(body, ShapeType::Face).unwrap() {
                let face = session.subshape(body, ShapeType::Face, index).unwrap();
                modified += session
                    .history_count(result, &face, occt_bridge::HistoryRelation::Modified)
                    .unwrap();
            }
            assert!(modified > 0);
            assert_eq!(session.shape_count().unwrap(), 3);
            drop(generated);
            assert_eq!(session.shape_count().unwrap(), 0);
        }
    }
}

#[test]
fn recess_parameter_edits_rebuild_and_invalid_edits_preserve_prior_output() {
    let session = Session::new().unwrap();
    for sink in [false, true] {
        let finish = if sink {
            HoleFinish::Countersink {
                diameter: ScalarExpr::Parameter("entry_diameter".into()),
                angle_radians: ScalarExpr::Parameter("entry_angle".into()),
            }
        } else {
            HoleFinish::Counterbore {
                diameter: ScalarExpr::Parameter("entry_diameter".into()),
                depth: ScalarExpr::Parameter("entry_depth".into()),
            }
        };
        let mut definition = finished_bore(
            HoleExtent::Blind {
                depth: length_expr(4.0),
            },
            finish,
        );
        definition.parameters.extend([
            length_parameter("entry_diameter", 4.0),
            length_parameter("entry_depth", 2.0),
            ParameterDefinition {
                id: "entry_angle".into(),
                parameter_type: ParameterType::Scalar(Dimension::Scalar),
                default: ParameterValue::Scalar(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
                minimum: None,
                maximum: None,
            },
        ]);
        let first = part(&definition).regenerate(&session).unwrap();
        for (name, value) in [
            (
                "entry_diameter",
                ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
            ),
            (
                if sink { "entry_angle" } else { "entry_depth" },
                ParameterValue::Scalar(if sink {
                    Quantity::scalar(std::f64::consts::PI / 3.0)
                } else {
                    Quantity::length(3.0, LengthUnit::Millimeter)
                }),
            ),
        ] {
            let mut edited = part(&definition);
            edited.overrides.insert(name.into(), value);
            let next = edited.regenerate_incremental(&session, &first).unwrap();
            assert_eq!(next.regeneration.reused, ["body"]);
            assert_eq!(next.regeneration.rebuilt, ["hole", "placed"]);
            assert!(
                session.volume(next.shape("hole").unwrap()).unwrap()
                    < session.volume(first.shape("hole").unwrap()).unwrap()
            );
        }
        let count = session.shape_count().unwrap();
        let mut invalid = part(&definition);
        invalid.overrides.insert(
            "entry_diameter".into(),
            ParameterValue::Scalar(Quantity::length(1.0, LengthUnit::Millimeter)),
        );
        assert!(invalid.regenerate_incremental(&session, &first).is_err());
        assert_eq!(session.shape_count().unwrap(), count);
        assert!(session.is_valid(first.shape("hole").unwrap()).unwrap());
        drop(first);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn invalid_recess_dimensions_angles_and_units_fail_without_leaks() {
    let session = Session::new().unwrap();
    let scalar = |v| ScalarExpr::Literal(Quantity::scalar(v));
    for finish in [
        HoleFinish::Counterbore {
            diameter: length_expr(2.0),
            depth: length_expr(1.0),
        },
        HoleFinish::Counterbore {
            diameter: length_expr(4.0),
            depth: length_expr(0.0),
        },
        HoleFinish::Counterbore {
            diameter: length_expr(4.0),
            depth: length_expr(4.0),
        },
        HoleFinish::Counterbore {
            diameter: scalar(4.0),
            depth: length_expr(1.0),
        },
        HoleFinish::Counterbore {
            diameter: length_expr(4.0),
            depth: scalar(1.0),
        },
        HoleFinish::Countersink {
            diameter: length_expr(1.0),
            angle_radians: scalar(1.0),
        },
        HoleFinish::Countersink {
            diameter: length_expr(4.0),
            angle_radians: scalar(0.0),
        },
        HoleFinish::Countersink {
            diameter: length_expr(4.0),
            angle_radians: scalar(-1.0),
        },
        HoleFinish::Countersink {
            diameter: length_expr(4.0),
            angle_radians: scalar(std::f64::consts::PI),
        },
        HoleFinish::Countersink {
            diameter: length_expr(4.0),
            angle_radians: scalar(0.1),
        },
        HoleFinish::Countersink {
            diameter: scalar(4.0),
            angle_radians: scalar(1.0),
        },
        HoleFinish::Countersink {
            diameter: length_expr(4.0),
            angle_radians: length_expr(1.0),
        },
    ] {
        let definition = finished_bore(
            HoleExtent::Blind {
                depth: length_expr(4.0),
            },
            finish,
        );
        let error = part(&definition).regenerate(&session).err().unwrap();
        assert!(error.message.contains("hole"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn schema_thirty_round_trips_recesses_and_defaults_old_holes_to_plain() {
    for finish in [
        HoleFinish::Counterbore {
            diameter: length_expr(4.0),
            depth: length_expr(2.0),
        },
        HoleFinish::Countersink {
            diameter: length_expr(4.0),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
        },
    ] {
        let definition = finished_bore(HoleExtent::ThroughAll, finish);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let document = ModelDocument::from_graph(&graph);
        assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(
            ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
            document
        );
        let plain = bore(HoleExtent::ThroughAll);
        let mut graph = InstanceGraph::new(&plain);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let mut old = serde_json::to_value(ModelDocument::from_graph(&graph)).unwrap();
        old["schema_version"] = serde_json::json!(29);
        // Externally tagged operation enum: hole fields are nested under "hole".
        old["family"]["features"][0]["operation"]["hole"]
            .as_object_mut()
            .unwrap()
            .remove("finish");
        let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.family, plain);
    }
}

#[test]
fn blind_and_through_holes_remove_exact_volume_and_preserve_history() {
    let session = Session::new().unwrap();
    for (extent, removed) in [
        (
            HoleExtent::Blind {
                depth: ScalarExpr::Parameter("depth".into()),
            },
            4.0 * std::f64::consts::PI,
        ),
        (HoleExtent::ThroughAll, 10.0 * std::f64::consts::PI),
    ] {
        let definition = bore(extent);
        let generated = part(&definition).regenerate(&session).unwrap();
        let result = generated.shape("hole").unwrap();
        assert!((session.volume(result).unwrap() - (1000.0 - removed)).abs() < 1e-7);
        assert_eq!(session.subshape_count(result, ShapeType::Solid).unwrap(), 1);
        assert!(session.is_valid(result).unwrap());
        assert!((session.volume(generated.shape("body").unwrap()).unwrap() - 1000.0).abs() < 1e-7);
        let body = generated.shape("body").unwrap();
        let mut modified = 0;
        for index in 0..session.subshape_count(body, ShapeType::Face).unwrap() {
            let face = session.subshape(body, ShapeType::Face, index).unwrap();
            modified += session
                .history_count(result, &face, occt_bridge::HistoryRelation::Modified)
                .unwrap();
        }
        assert!(modified > 0);
        assert_eq!(
            session.shape_count().unwrap(),
            3,
            "temporary cylinder retained"
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn through_all_covers_distant_positions_and_rotated_axes() {
    let session = Session::new().unwrap();
    let mut definition = bore(HoleExtent::ThroughAll);
    if let FeatureOperation::Hole {
        position: point,
        axis,
        ..
    } = &mut definition.features[0].operation
    {
        *point = position(1_000_100.0, 5.0, 5.0);
        *axis = direction(-7.0, 0.0, 0.0);
    }
    if let FeatureOperation::Box { origin, .. } = &mut definition.features[1].operation {
        *origin = position(1_000_000.0, 0.0, 0.0);
    }
    let generated = part(&definition).regenerate(&session).unwrap();
    assert!(
        (session.volume(generated.shape("hole").unwrap()).unwrap()
            - (1000.0 - 10.0 * std::f64::consts::PI))
            .abs()
            < 1e-7
    );
    drop(generated);
    let mut definition = bore(HoleExtent::ThroughAll);
    definition.features[1].operation = FeatureOperation::Cylinder {
        origin: position(0.0, 0.0, 0.0),
        axis: direction(1.0, 0.0, 1.0),
        radius: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
        height: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
    };
    if let FeatureOperation::Hole {
        position: point,
        axis,
        ..
    } = &mut definition.features[0].operation
    {
        *point = position(100.0, 0.0, 100.0);
        *axis = direction(1.0, 0.0, 1.0);
    }
    let generated = part(&definition).regenerate(&session).unwrap();
    assert!(
        (session.volume(generated.shape("hole").unwrap()).unwrap() - 30.0 * std::f64::consts::PI)
            .abs()
            < 1e-7
    );
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn hole_diameter_depth_position_and_axis_edits_rebuild_the_affected_branch() {
    let session = Session::new().unwrap();
    let mut definition = bore(HoleExtent::Blind {
        depth: ScalarExpr::Parameter("depth".into()),
    });
    definition.parameters.push(ParameterDefinition {
        id: "position".into(),
        parameter_type: ParameterType::Vector(Dimension::Length),
        default: ParameterValue::Vector(VectorQuantity::lengths(
            5.0,
            5.0,
            10.0,
            LengthUnit::Millimeter,
        )),
        minimum: None,
        maximum: None,
    });
    definition.parameters.push(ParameterDefinition {
        id: "axis".into(),
        parameter_type: ParameterType::Vector(Dimension::Scalar),
        default: ParameterValue::Vector(VectorQuantity::scalars(0.0, 0.0, -1.0)),
        minimum: None,
        maximum: None,
    });
    if let FeatureOperation::Hole { position, axis, .. } = &mut definition.features[0].operation {
        *position = VectorExpr::Parameter("position".into());
        *axis = VectorExpr::Parameter("axis".into());
    }
    let first = part(&definition).regenerate(&session).unwrap();
    for (name, value, removed) in [
        (
            "diameter",
            ParameterValue::Scalar(Quantity::length(4.0, LengthUnit::Millimeter)),
            16.0 * std::f64::consts::PI,
        ),
        (
            "depth",
            ParameterValue::Scalar(Quantity::length(6.0, LengthUnit::Millimeter)),
            6.0 * std::f64::consts::PI,
        ),
        (
            "position",
            ParameterValue::Vector(VectorQuantity::lengths(
                3.0,
                5.0,
                10.0,
                LengthUnit::Millimeter,
            )),
            4.0 * std::f64::consts::PI,
        ),
        (
            "axis",
            ParameterValue::Vector(VectorQuantity::scalars(0.0, 0.0, -2.0)),
            4.0 * std::f64::consts::PI,
        ),
    ] {
        let mut edited = part(&definition);
        edited.overrides.insert(name.into(), value);
        let generated = edited.regenerate_incremental(&session, &first).unwrap();
        assert_eq!(generated.regeneration.reused, ["body"]);
        assert_eq!(generated.regeneration.rebuilt, ["hole", "placed"]);
        assert!(
            (session.volume(generated.shape("hole").unwrap()).unwrap() - (1000.0 - removed)).abs()
                < 1e-7
        );
    }
    let count = session.shape_count().unwrap();
    let mut failed = part(&definition);
    failed.overrides.insert(
        "position".into(),
        ParameterValue::Vector(VectorQuantity::lengths(
            100.0,
            100.0,
            10.0,
            LengthUnit::Millimeter,
        )),
    );
    let error = failed
        .regenerate_incremental(&session, &first)
        .err()
        .unwrap();
    assert!(error.message.contains("does not remove material"));
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(first.shape("hole").unwrap()).unwrap());
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn invalid_holes_fail_with_feature_context_and_release_all_handles() {
    let session = Session::new().unwrap();
    for case in 0..10 {
        let mut definition = bore(HoleExtent::Blind {
            depth: ScalarExpr::Parameter("depth".into()),
        });
        if let FeatureOperation::Hole {
            input,
            position: point,
            axis,
            diameter,
            extent,
            ..
        } = &mut definition.features[0].operation
        {
            match case {
                0 => *diameter = ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
                1 => *diameter = ScalarExpr::Literal(Quantity::scalar(2.0)),
                2 => *axis = direction(0.0, 0.0, 0.0),
                3 => *axis = position(0.0, 0.0, -1.0),
                4 => *point = direction(5.0, 5.0, 10.0),
                5 => {
                    *extent = HoleExtent::Blind {
                        depth: ScalarExpr::Literal(Quantity::length(-1.0, LengthUnit::Millimeter)),
                    }
                }
                6 => {
                    *extent = HoleExtent::Blind {
                        depth: ScalarExpr::Literal(Quantity::scalar(4.0)),
                    }
                }
                7 => *input = "missing".into(),
                8 => {
                    *diameter =
                        ScalarExpr::Literal(Quantity::length(100.0, LengthUnit::Millimeter));
                    *extent = HoleExtent::ThroughAll;
                }
                _ => {
                    *point = position(100.0, 100.0, 10.0);
                    *extent = HoleExtent::ThroughAll;
                }
            }
        }
        let error = part(&definition)
            .regenerate(&session)
            .err()
            .expect("invalid hole must fail");
        assert!(error.message.contains("hole"), "{error}");
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut definition = bore(HoleExtent::ThroughAll);
    definition.features[0].operation = FeatureOperation::Hole {
        input: "body".into(),
        position: position(5.0, 5.0, 10.0),
        axis: direction(0.0, 0.0, -1.0),
        diameter: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
        extent: HoleExtent::ThroughAll,
        finish: HoleFinish::Plain,
        thread: None,
    };
    definition.features[1].operation = FeatureOperation::SketchWire {
        sketch: Box::new(SketchDefinition {
            id: "circle".into(),
            datum_plane: None,
            origin: position(0.0, 0.0, 0.0),
            x_axis: direction(1.0, 0.0, 0.0),
            y_axis: direction(0.0, 1.0, 0.0),
            points: vec![
                SketchPoint {
                    id: "c".into(),
                    x: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
                    y: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
                    fixed: true,
                },
                SketchPoint {
                    id: "r".into(),
                    x: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
                    y: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
                    fixed: true,
                },
            ],
            lines: Vec::new(),
            circles: vec![SketchCircle {
                id: "circle".into(),
                center: "c".into(),
                rim: "r".into(),
            }],
            arcs: Vec::new(),
            splines: Vec::new(),
            profile: Vec::new(),
            constraints: Vec::new(),
        }),
    };
    assert!(part(&definition).regenerate(&session).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn schema_twenty_nine_round_trips_holes_and_migrates_existing_features() {
    for extent in [
        HoleExtent::ThroughAll,
        HoleExtent::Blind {
            depth: ScalarExpr::Parameter("depth".into()),
        },
    ] {
        let definition = bore(extent);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        let session = Session::new().unwrap();
        let result = loaded
            .instance_graph()
            .unwrap()
            .regenerate_all(&session)
            .unwrap();
        drop(result);
        assert_eq!(session.shape_count().unwrap(), 0);
        let mut old = serde_json::to_value(&document).unwrap();
        old["schema_version"] = serde_json::json!(28);
        old["family"]["features"]
            .as_array_mut()
            .unwrap()
            .retain(|feature| feature["id"] == "body");
        let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.family.features[0], definition.features[1]);
    }
}

fn thread_spec() -> ThreadSpecification {
    ThreadSpecification {
        designation: "custom internal thread".into(),
        nominal_diameter: length_expr(3.0),
        pitch: length_expr(0.5),
        handedness: ThreadHandedness::Right,
    }
}

fn threaded_bore(specification: ThreadSpecification) -> FamilyDefinition {
    let mut definition = bore(HoleExtent::ThroughAll);
    if let FeatureOperation::Hole { thread, .. } = &mut definition.features[0].operation {
        *thread = Some(Box::new(specification));
    }
    definition
}

#[test]
fn clearance_catalog_checks_every_frozen_row_and_converts_units() {
    let rows = [
        [1.6, 1.7, 1.8, 2.0],
        [2.0, 2.2, 2.4, 2.6],
        [2.5, 2.7, 2.9, 3.1],
        [3.0, 3.2, 3.4, 3.6],
        [4.0, 4.3, 4.5, 4.8],
        [5.0, 5.3, 5.5, 5.8],
        [6.0, 6.4, 6.6, 7.0],
        [8.0, 8.4, 9.0, 10.0],
        [10.0, 10.5, 11.0, 12.0],
        [12.0, 13.0, 13.5, 14.5],
        [14.0, 15.0, 15.5, 16.5],
        [16.0, 17.0, 17.5, 18.5],
        [18.0, 19.0, 20.0, 21.0],
        [20.0, 21.0, 22.0, 24.0],
        [22.0, 23.0, 24.0, 26.0],
        [24.0, 25.0, 26.0, 28.0],
        [27.0, 28.0, 30.0, 32.0],
        [30.0, 31.0, 33.0, 35.0],
        [36.0, 37.0, 39.0, 42.0],
    ];
    for row in rows {
        for (column, series) in [
            ClearanceSeries::Fine,
            ClearanceSeries::Medium,
            ClearanceSeries::Coarse,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                iso273_clearance_v1(Quantity::length(row[0], LengthUnit::Millimeter), series)
                    .unwrap(),
                Quantity::length(row[column + 1], LengthUnit::Millimeter)
            );
        }
    }
    for (value, unit) in [
        (0.6, LengthUnit::Centimeter),
        (0.006, LengthUnit::Meter),
        (6.0 / 25.4, LengthUnit::Inch),
    ] {
        assert_eq!(
            iso273_clearance_v1(Quantity::length(value, unit), ClearanceSeries::Medium)
                .unwrap()
                .value,
            6.6
        );
    }
}

#[test]
fn clearance_catalog_rejects_unsupported_and_invalid_quantities() {
    for quantity in [
        Quantity::scalar(6.0),
        Quantity::length(0.0, LengthUnit::Millimeter),
        Quantity::length(-6.0, LengthUnit::Millimeter),
        Quantity::length(7.0, LengthUnit::Millimeter),
        Quantity::length(6.001, LengthUnit::Millimeter),
        Quantity::length(100.0, LengthUnit::Millimeter),
        Quantity::length(f64::INFINITY, LengthUnit::Millimeter),
        Quantity::length(f64::NAN, LengthUnit::Millimeter),
        Quantity {
            value: 6.0,
            dimension: Dimension::Length,
            unit: None,
        },
    ] {
        assert!(iso273_clearance_v1(quantity, ClearanceSeries::Fine).is_err());
    }
    let expression = ScalarExpr::Iso273ClearanceV1 {
        nominal_diameter: Box::new(ScalarExpr::Literal(Quantity::scalar(6.0))),
        series: ClearanceSeries::Medium,
    };
    assert!(evaluate_resolved_expression(&expression, &HashMap::new()).is_err());
    assert!(
        evaluate_derived_expression(
            &expression,
            &HashMap::new(),
            &mut HashMap::new(),
            &mut Vec::new()
        )
        .is_err()
    );
}

fn catalog_bore(series: ClearanceSeries) -> FamilyDefinition {
    let mut definition = bore(HoleExtent::ThroughAll);
    definition
        .parameters
        .push(length_parameter("fastener", 2.0));
    if let FeatureOperation::Hole { diameter, .. } = &mut definition.features[0].operation {
        *diameter = ScalarExpr::Iso273ClearanceV1 {
            nominal_diameter: Box::new(ScalarExpr::Parameter("fastener".into())),
            series,
        };
    }
    definition
}

#[test]
fn catalog_holes_rebuild_for_size_and_series_edits_and_rollback_bad_sizes() {
    let session = Session::new().unwrap();
    let definition = catalog_bore(ClearanceSeries::Fine);
    let first = part(&definition).regenerate(&session).unwrap();
    assert!(
        (session.volume(first.shape("hole").unwrap()).unwrap()
            - (1000.0 - 10.0 * std::f64::consts::PI * 1.1 * 1.1))
            .abs()
            < 1e-7
    );
    let mut changed_size = part(&definition);
    changed_size.overrides.insert(
        "fastener".into(),
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
    );
    let next = changed_size
        .regenerate_incremental(&session, &first)
        .unwrap();
    assert_eq!(next.regeneration.reused, ["body"]);
    assert_eq!(next.regeneration.rebuilt, ["hole", "placed"]);
    assert!(
        (session.volume(next.shape("hole").unwrap()).unwrap()
            - (1000.0 - 10.0 * std::f64::consts::PI * 1.6 * 1.6))
            .abs()
            < 1e-7
    );
    drop(next);
    let revised = catalog_bore(ClearanceSeries::Coarse);
    let next = part(&revised)
        .regenerate_incremental(&session, &first)
        .unwrap();
    assert_eq!(next.regeneration.reused, ["body"]);
    assert_eq!(next.regeneration.rebuilt, ["hole", "placed"]);
    assert!(
        (session.volume(next.shape("hole").unwrap()).unwrap()
            - (1000.0 - 10.0 * std::f64::consts::PI * 1.3 * 1.3))
            .abs()
            < 1e-7
    );
    drop(next);
    let count = session.shape_count().unwrap();
    changed_size.overrides.insert(
        "fastener".into(),
        ParameterValue::Scalar(Quantity::length(7.0, LengthUnit::Millimeter)),
    );
    let error = changed_size
        .regenerate_incremental(&session, &first)
        .err()
        .unwrap();
    assert!(error.message.contains("unsupported ISO 273"));
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(first.shape("hole").unwrap()).unwrap());
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn catalog_expressions_work_in_derived_parameters_and_schema_thirty_two() {
    let mut definition = catalog_bore(ClearanceSeries::Medium);
    let expression =
        if let FeatureOperation::Hole { diameter, .. } = &definition.features[0].operation {
            diameter.clone()
        } else {
            unreachable!()
        };
    definition
        .derived_parameters
        .push(DerivedParameterDefinition {
            id: "clearance".into(),
            dimension: Dimension::Length,
            expression,
        });
    if let FeatureOperation::Hole { diameter, .. } = &mut definition.features[0].operation {
        *diameter = ScalarExpr::Parameter("clearance".into());
    }
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    let session = Session::new().unwrap();
    let generated = part(&loaded.family).regenerate(&session).unwrap();
    assert!(
        (session.volume(generated.shape("hole").unwrap()).unwrap()
            - (1000.0 - 10.0 * std::f64::consts::PI * 1.2 * 1.2))
            .abs()
            < 1e-7
    );
    drop(generated);
    let plain = bore(HoleExtent::ThroughAll);
    let mut graph = InstanceGraph::new(&plain);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut old = serde_json::to_value(ModelDocument::from_graph(&graph)).unwrap();
    old["schema_version"] = serde_json::json!(31);
    let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(migrated.family, plain);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn thread_metadata_round_trips_without_changing_geometry_and_old_holes_migrate() {
    let session = Session::new().unwrap();
    for handedness in [ThreadHandedness::Right, ThreadHandedness::Left] {
        let mut spec = thread_spec();
        spec.handedness = handedness;
        let definition = threaded_bore(spec);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let document = ModelDocument::from_graph(&graph);
        assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
        let generated = part(&loaded.family).regenerate(&session).unwrap();
        assert!(
            (session.volume(generated.shape("hole").unwrap()).unwrap()
                - (1000.0 - 10.0 * std::f64::consts::PI))
                .abs()
                < 1e-7
        );
        assert_eq!(session.shape_count().unwrap(), 3);
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), 0);
        let mut old = serde_json::to_value(&document).unwrap();
        old["schema_version"] = serde_json::json!(30);
        old["family"]["features"][0]["operation"]["hole"]
            .as_object_mut()
            .unwrap()
            .remove("thread");
        let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.family, bore(HoleExtent::ThroughAll));
    }
}

#[test]
fn invalid_thread_metadata_rejects_units_and_dimensions_without_leaks() {
    let session = Session::new().unwrap();
    for case in 0..7 {
        let mut spec = thread_spec();
        match case {
            0 => spec.designation = " \t\n".into(),
            1 => spec.nominal_diameter = length_expr(2.0),
            2 => spec.nominal_diameter = length_expr(-1.0),
            3 => spec.pitch = length_expr(0.0),
            4 => spec.pitch = length_expr(-0.5),
            5 => spec.nominal_diameter = ScalarExpr::Literal(Quantity::scalar(3.0)),
            _ => spec.pitch = ScalarExpr::Literal(Quantity::scalar(0.5)),
        }
        let definition = threaded_bore(spec);
        let error = part(&definition).regenerate(&session).err().unwrap();
        assert!(error.message.contains("hole"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn thread_parameter_and_literal_edits_invalidate_only_the_hole_branch() {
    let session = Session::new().unwrap();
    let mut spec = thread_spec();
    spec.nominal_diameter = ScalarExpr::Parameter("nominal".into());
    spec.pitch = ScalarExpr::Parameter("pitch".into());
    let mut definition = threaded_bore(spec);
    definition.parameters.extend([
        length_parameter("nominal", 3.0),
        length_parameter("pitch", 0.5),
    ]);
    definition
        .parameters
        .iter_mut()
        .find(|parameter| parameter.id == "pitch")
        .unwrap()
        .minimum = None;
    let first = part(&definition).regenerate(&session).unwrap();
    for name in ["nominal", "pitch"] {
        let mut edited = part(&definition);
        edited.overrides.insert(
            name.into(),
            ParameterValue::Scalar(Quantity::length(
                if name == "nominal" { 4.0 } else { 0.75 },
                LengthUnit::Millimeter,
            )),
        );
        let next = edited.regenerate_incremental(&session, &first).unwrap();
        assert_eq!(next.regeneration.reused, ["body"]);
        assert_eq!(next.regeneration.rebuilt, ["hole", "placed"]);
        assert!(
            (session.volume(next.shape("hole").unwrap()).unwrap()
                - session.volume(first.shape("hole").unwrap()).unwrap())
            .abs()
                < 1e-7
        );
    }
    let mut revised = definition.clone();
    if let FeatureOperation::Hole {
        thread: Some(thread),
        ..
    } = &mut revised.features[0].operation
    {
        thread.designation = "revised custom thread".into();
        thread.handedness = ThreadHandedness::Left;
    }
    let next = part(&revised)
        .regenerate_incremental(&session, &first)
        .unwrap();
    assert_eq!(next.regeneration.reused, ["body"]);
    assert_eq!(next.regeneration.rebuilt, ["hole", "placed"]);
    drop(next);
    let count = session.shape_count().unwrap();
    let mut invalid = part(&definition);
    invalid.overrides.insert(
        "pitch".into(),
        ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
    );
    assert!(invalid.regenerate_incremental(&session, &first).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(first.shape("hole").unwrap()).unwrap());
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
}
