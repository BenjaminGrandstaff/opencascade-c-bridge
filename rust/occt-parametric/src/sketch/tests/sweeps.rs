//! Extrude and revolve features from sketch profiles.

use super::*;

fn sweep_family(
    sketch: SketchDefinition,
    wire: bool,
    operation: FeatureOperation,
) -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
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
            input: "profile".into(),
            direction: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        },
        FeatureOperation::Extrude {
            input: "profile".into(),
            direction: VectorExpr::Literal(VectorQuantity::lengths(
                1.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        },
        FeatureOperation::Extrude {
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
