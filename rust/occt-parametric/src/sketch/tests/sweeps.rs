//! Extrude and revolve features from sketch profiles.

use super::*;

fn sweep_family(
    sketch: SketchDefinition,
    wire: bool,
    operation: FeatureOperation,
) -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "Sweep".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        requirements: Vec::new(),
        datums: Vec::new(),
        // Deliberately declared before its profile to exercise dependencies.
        features: vec![
            FeatureDefinition {
                id: "solid".into(),
                operation,
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
        ],
    }
}

fn circular_profile() -> SketchDefinition {
    let mut sketch = rectangle();
    sketch.points = vec![
        point("center", 0.0, 0.0, true),
        point("rim", 2.0, 0.0, true),
    ];
    sketch.lines.clear();
    sketch.constraints.clear();
    sketch.circles = vec![SketchCircle {
        id: "circle".into(),
        center: "center".into(),
        rim: "rim".into(),
    }];
    sketch
}

#[test]
fn extrudes_line_arc_and_circle_faces_and_wires_to_exact_solids() {
    let session = Session::new().unwrap();
    for wire in [false, true] {
        for (sketch, area) in [
            (rectangle(), 50.0),
            (arc_profile(false), std::f64::consts::PI - 2.0),
            (circular_profile(), 4.0 * std::f64::consts::PI),
        ] {
            for height in [3.0, -3.0] {
                let definition = sweep_family(
                    sketch.clone(),
                    wire,
                    FeatureOperation::Extrude {
                        extent: ExtrudeExtent::Distance,
                        input: "profile".into(),
                        direction: VectorExpr::Literal(VectorQuantity::lengths(
                            1.0,
                            0.0,
                            height,
                            LengthUnit::Millimeter,
                        )),
                    },
                );
                let generated = PartInstance {
                    id: "part".into(),
                    definition: &definition,
                    overrides: HashMap::new(),
                    provenance: "test".into(),
                }
                .regenerate(&session)
                .unwrap();
                let solid = generated.shape("solid").unwrap();
                assert_eq!(session.shape_type(solid).unwrap(), ShapeType::Solid);
                assert!(session.is_valid(solid).unwrap());
                assert!((session.volume(solid).unwrap() - area * height.abs()).abs() < 1e-7);
                let edge = session
                    .subshape(generated.shape("profile").unwrap(), ShapeType::Edge, 0)
                    .unwrap();
                assert!(
                    session
                        .history_count(solid, &edge, occt_bridge::HistoryRelation::Generated)
                        .unwrap()
                        > 0
                );
            }
        }
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn revolves_offset_circle_profiles_into_full_and_signed_partial_tori() {
    let session = Session::new().unwrap();
    let mut sketch = circular_profile();
    sketch.points = vec![
        point("center", 3.0, 0.0, true),
        point("rim", 4.0, 0.0, true),
    ];
    sketch.origin = VectorExpr::Literal(VectorQuantity::lengths(
        10.0,
        20.0,
        30.0,
        LengthUnit::Millimeter,
    ));
    sketch.y_axis = VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0));
    for wire in [false, true] {
        for angle in [
            std::f64::consts::TAU,
            std::f64::consts::PI,
            -std::f64::consts::PI / 2.0,
        ] {
            let definition = sweep_family(
                sketch.clone(),
                wire,
                FeatureOperation::Revolve {
                    input: "profile".into(),
                    origin: sketch.origin.clone(),
                    axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 2.0)),
                    angle_radians: ScalarExpr::Literal(Quantity::scalar(angle)),
                },
            );
            let generated = PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "test".into(),
            }
            .regenerate(&session)
            .unwrap();
            let solid = generated.shape("solid").unwrap();
            assert!(session.is_valid(solid).unwrap());
            assert!(
                (session.volume(solid).unwrap() - 3.0 * std::f64::consts::PI * angle.abs()).abs()
                    < 1e-7
            );
            let edge = session
                .subshape(generated.shape("profile").unwrap(), ShapeType::Edge, 0)
                .unwrap();
            assert!(
                session
                    .history_count(solid, &edge, occt_bridge::HistoryRelation::Generated)
                    .unwrap()
                    > 0
            );
        }
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn sweep_parameters_rebuild_solids_and_reuse_unchanged_profiles() {
    let session = Session::new().unwrap();
    for revolve in [false, true] {
        let mut sketch = circular_profile();
        let operation = if revolve {
            sketch.points[0].x = length(3.0);
            sketch.points[1].x = length(4.0);
            sketch.y_axis = VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0));
            FeatureOperation::Revolve {
                input: "profile".into(),
                origin: sketch.origin.clone(),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                angle_radians: ScalarExpr::Parameter("amount".into()),
            }
        } else {
            FeatureOperation::Extrude {
                extent: ExtrudeExtent::Distance,
                input: "profile".into(),
                direction: VectorExpr::Components {
                    x: length(0.0),
                    y: length(0.0),
                    z: ScalarExpr::Parameter("amount".into()),
                },
            }
        };
        let mut definition = sweep_family(sketch, true, operation);
        let value = |amount| {
            if revolve {
                Quantity::scalar(amount)
            } else {
                Quantity::length(amount, LengthUnit::Millimeter)
            }
        };
        definition.parameters.push(ParameterDefinition {
            id: "amount".into(),
            parameter_type: ParameterType::Scalar(if revolve {
                Dimension::Scalar
            } else {
                Dimension::Length
            }),
            default: ParameterValue::Scalar(value(1.0)),
            minimum: None,
            maximum: None,
        });
        let part = |amount| PartInstance {
            id: "part".into(),
            definition: &definition,
            overrides: HashMap::from([("amount".into(), ParameterValue::Scalar(value(amount)))]),
            provenance: "test".into(),
        };
        let first = part(1.0).regenerate(&session).unwrap();
        let second = part(2.0).regenerate_incremental(&session, &first).unwrap();
        assert_eq!(second.regeneration.rebuilt, ["solid"]);
        assert_eq!(second.regeneration.reused, ["profile"]);
        assert!(
            (session.volume(second.shape("solid").unwrap()).unwrap()
                - 2.0 * session.volume(first.shape("solid").unwrap()).unwrap())
            .abs()
                < 1e-7
        );
        let count = session.shape_count().unwrap();
        assert!(part(0.0).regenerate_incremental(&session, &second).is_err());
        assert_eq!(session.shape_count().unwrap(), count);
        assert!(session.is_valid(second.shape("solid").unwrap()).unwrap());
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn invalid_sweep_values_and_inputs_fail_without_leaking_profiles() {
    let session = Session::new().unwrap();
    for operation in [
        FeatureOperation::Extrude {
            extent: ExtrudeExtent::Distance,
            input: "profile".into(),
            direction: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        },
        FeatureOperation::Extrude {
            extent: ExtrudeExtent::Distance,
            input: "profile".into(),
            direction: VectorExpr::Literal(VectorQuantity::lengths(
                1.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        },
        FeatureOperation::Extrude {
            extent: ExtrudeExtent::Distance,
            input: "profile".into(),
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        },
        FeatureOperation::Revolve {
            input: "profile".into(),
            origin: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0)),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(1.0)),
        },
        FeatureOperation::Revolve {
            input: "profile".into(),
            origin: rectangle().origin,
            axis: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                1.0,
                LengthUnit::Millimeter,
            )),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(1.0)),
        },
        FeatureOperation::Revolve {
            input: "profile".into(),
            origin: rectangle().origin,
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            angle_radians: length(1.0),
        },
        FeatureOperation::Revolve {
            input: "missing".into(),
            origin: rectangle().origin,
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(1.0)),
        },
    ] {
        let definition = sweep_family(rectangle(), true, operation);
        assert!(
            PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "test".into()
            }
            .regenerate(&session)
            .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut definition = sweep_family(
        rectangle(),
        true,
        FeatureOperation::Extrude {
            extent: ExtrudeExtent::Distance,
            input: "profile".into(),
            direction: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                1.0,
                LengthUnit::Millimeter,
            )),
        },
    );
    definition.features[1].operation = FeatureOperation::Box {
        origin: rectangle().origin,
        size: VectorExpr::Literal(VectorQuantity::lengths(
            1.0,
            1.0,
            1.0,
            LengthUnit::Millimeter,
        )),
    };
    let error = PartInstance {
        id: "part".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
    .regenerate(&session)
    .err()
    .unwrap();
    assert!(error.message.contains("profile 'profile'"));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn schema_twenty_eight_round_trips_sweeps_and_migrates_sketch_documents() {
    for operation in [
        FeatureOperation::Extrude {
            extent: ExtrudeExtent::Distance,
            input: "profile".into(),
            direction: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                2.0,
                LengthUnit::Millimeter,
            )),
        },
        FeatureOperation::Revolve {
            input: "profile".into(),
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                -3.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::TAU)),
        },
    ] {
        let definition = sweep_family(circular_profile(), true, operation);
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
        let mut old = serde_json::to_value(&document).unwrap();
        old["schema_version"] = serde_json::json!(27);
        old["family"]["features"].as_array_mut().unwrap().remove(0);
        let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.family.features.len(), 1);
    }
}

fn extrusion_limit_family(extent: ExtrudeExtent, z: f64, size: f64, sign: f64) -> FamilyDefinition {
    let mut definition = sweep_family(
        circular_profile(),
        false,
        FeatureOperation::Extrude {
            input: "profile".into(),
            direction: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                sign,
                LengthUnit::Millimeter,
            )),
            extent,
        },
    );
    definition.features.push(FeatureDefinition {
        id: "limit".into(),
        operation: FeatureOperation::Box {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                -size / 2.0,
                -size / 2.0,
                z,
                LengthUnit::Millimeter,
            )),
            size: VectorExpr::Literal(VectorQuantity::lengths(
                size,
                size,
                2.0,
                LengthUnit::Millimeter,
            )),
        },
    });
    definition
}

fn limiting_face(extremum: Extremum) -> ExtrudeExtent {
    ExtrudeExtent::UpToFace {
        target: "limit".into(),
        face: Box::new(FaceSelector::AtExtreme {
            axis: CoordinateAxis::Z,
            extremum,
            tolerance: length(1e-6),
        }),
    }
}

#[test]
fn symmetric_extrusion_centers_full_length_and_preserves_history() {
    let session = Session::new().unwrap();
    for sign in [-8.0, 8.0] {
        let definition = extrusion_limit_family(ExtrudeExtent::Symmetric, 10.0, 20.0, sign);
        let result = PartInstance {
            id: "part".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        }
        .regenerate(&session)
        .unwrap();
        let solid = result.shape("solid").unwrap();
        let bounds = session.bounds(solid).unwrap();
        assert!((bounds.min.z + 4.0).abs() < 1e-6);
        assert!((bounds.max.z - 4.0).abs() < 1e-6);
        assert!((session.volume(solid).unwrap() - 32.0 * std::f64::consts::PI).abs() < 1e-7);
        let edge = session
            .subshape(result.shape("profile").unwrap(), ShapeType::Edge, 0)
            .unwrap();
        assert!(
            session
                .history_count(solid, &edge, occt_bridge::HistoryRelation::Generated)
                .unwrap()
                > 0
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn extrusion_limits_follow_selected_or_next_faces_in_both_directions() {
    let session = Session::new().unwrap();
    for (extent, z, sign, expected) in [
        (limiting_face(Extremum::Maximum), 10.0, 1.0, 12.0),
        (
            ExtrudeExtent::UpToNext {
                target: "limit".into(),
            },
            10.0,
            100.0,
            10.0,
        ),
        (limiting_face(Extremum::Minimum), -12.0, -1.0, 12.0),
        (
            ExtrudeExtent::UpToNext {
                target: "limit".into(),
            },
            -12.0,
            -100.0,
            10.0,
        ),
    ] {
        let definition = extrusion_limit_family(extent, z, 20.0, sign);
        let result = PartInstance {
            id: "part".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        }
        .regenerate(&session)
        .unwrap();
        let solid = result.shape("solid").unwrap();
        assert!(
            (session.volume(solid).unwrap() - 4.0 * std::f64::consts::PI * expected).abs() < 1e-7
        );
        let bounds = session.bounds(solid).unwrap();
        assert!(
            (if sign > 0.0 {
                bounds.max.z
            } else {
                -bounds.min.z
            } - expected)
                .abs()
                < 1e-6
        );
        let edge = session
            .subshape(result.shape("profile").unwrap(), ShapeType::Edge, 0)
            .unwrap();
        assert!(
            session
                .history_count(solid, &edge, occt_bridge::HistoryRelation::Generated)
                .unwrap()
                > 0
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn extrusion_limits_reject_partial_backward_ambiguous_and_missing_faces() {
    let session = Session::new().unwrap();
    for (extent, z, size) in [
        (limiting_face(Extremum::Minimum), 10.0, 2.0),
        (limiting_face(Extremum::Minimum), -12.0, 20.0),
        (
            ExtrudeExtent::UpToNext {
                target: "limit".into(),
            },
            10.0,
            2.0,
        ),
        (
            ExtrudeExtent::UpToFace {
                target: "limit".into(),
                face: Box::new(FaceSelector::NormalAligned {
                    direction: VectorExpr::Literal(VectorQuantity::scalars(1.0, 1.0, 1.0)),
                    minimum_dot: ScalarExpr::Literal(Quantity::scalar(-1.0)),
                }),
            },
            10.0,
            20.0,
        ),
        (
            ExtrudeExtent::UpToNext {
                target: "missing".into(),
            },
            10.0,
            20.0,
        ),
    ] {
        let definition = extrusion_limit_family(extent, z, size, 1.0);
        assert!(
            PartInstance {
                id: "part".into(),
                definition: &definition,
                overrides: HashMap::new(),
                provenance: "test".into()
            }
            .regenerate(&session)
            .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn extrusion_face_limit_tracks_parameters_and_named_reference() {
    let session = Session::new().unwrap();
    let mut definition = extrusion_limit_family(limiting_face(Extremum::Maximum), 0.0, 20.0, 1.0);
    let FeatureOperation::Extrude {
        extent: ExtrudeExtent::UpToFace { face, .. },
        ..
    } = &mut definition.features[0].operation
    else {
        panic!()
    };
    let selector = (**face).clone();
    **face = FaceSelector::Named("stop".into());
    definition.references.push(NamedReference {
        name: "stop".into(),
        target: ReferenceTarget::Faces(selector),
    });
    definition.parameters.push(ParameterDefinition {
        id: "height".into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
        minimum: None,
        maximum: None,
    });
    let FeatureOperation::Box { origin, .. } = &mut definition.features[2].operation else {
        panic!()
    };
    *origin = VectorExpr::Components {
        x: length(-10.0),
        y: length(-10.0),
        z: ScalarExpr::Parameter("height".into()),
    };
    for height in [5.0, 15.0] {
        let result = PartInstance {
            id: "part".into(),
            definition: &definition,
            overrides: HashMap::from([(
                "height".into(),
                ParameterValue::Scalar(Quantity::length(height, LengthUnit::Millimeter)),
            )]),
            provenance: "test".into(),
        }
        .regenerate(&session)
        .unwrap();
        assert!(
            (session.volume(result.shape("solid").unwrap()).unwrap()
                - 4.0 * std::f64::consts::PI * (height + 2.0))
                .abs()
                < 1e-7
        );
    }
    {
        let part = |height| PartInstance {
            id: "part".into(),
            definition: &definition,
            overrides: HashMap::from([(
                "height".into(),
                ParameterValue::Scalar(Quantity::length(height, LengthUnit::Millimeter)),
            )]),
            provenance: "test".into(),
        };
        let first = part(5.0).regenerate(&session).unwrap();
        let second = part(15.0).regenerate_incremental(&session, &first).unwrap();
        assert_eq!(second.regeneration.reused, ["profile"]);
        assert!(second.regeneration.rebuilt.contains(&"limit".to_string()));
        assert!(second.regeneration.rebuilt.contains(&"solid".to_string()));
        assert!(
            (session.volume(second.shape("solid").unwrap()).unwrap() - 68.0 * std::f64::consts::PI)
                .abs()
                < 1e-7
        );
    }
    let document = ModelDocument::from_graph(&InstanceGraph::new(&definition));
    let json = document.to_json_pretty().unwrap();
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn geometric_extrusion_accepts_oblique_travel_and_legacy_distance_defaults() {
    let session = Session::new().unwrap();
    let mut definition = extrusion_limit_family(limiting_face(Extremum::Minimum), 10.0, 40.0, 1.0);
    let FeatureOperation::Extrude { direction, .. } = &mut definition.features[0].operation else {
        panic!()
    };
    *direction = VectorExpr::Literal(VectorQuantity::lengths(
        1.0,
        0.0,
        1.0,
        LengthUnit::Millimeter,
    ));
    {
        let result = PartInstance {
            id: "part".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        }
        .regenerate(&session)
        .unwrap();
        assert!(
            (session.volume(result.shape("solid").unwrap()).unwrap() - 40.0 * std::f64::consts::PI)
                .abs()
                < 1e-7
        );
        assert!(
            (session
                .bounds(result.shape("solid").unwrap())
                .unwrap()
                .max
                .x
                - 12.0)
                .abs()
                < 1e-6
        );
    }
    let mut legacy = serde_json::to_value(&definition.features[0].operation).unwrap();
    legacy["extrude"].as_object_mut().unwrap().remove("extent");
    let restored: FeatureOperation = serde_json::from_value(legacy).unwrap();
    assert!(matches!(
        restored,
        FeatureOperation::Extrude {
            extent: ExtrudeExtent::Distance,
            ..
        }
    ));
    assert_eq!(session.shape_count().unwrap(), 0);
}
