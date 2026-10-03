//! Sketch wires and faces, datum-plane links, and their document migration.

use super::*;

#[test]
fn sketch_wires_preserve_closed_line_arc_and_circle_geometry() {
    let session = Session::new().unwrap();
    let mut circle = arc_profile(false);
    circle.lines.clear();
    circle.arcs.clear();
    circle.profile.clear();
    circle.circles.push(SketchCircle {
        id: "circle".into(),
        center: "c".into(),
        rim: "a".into(),
    });
    for (sketch, perimeter) in [
        (rectangle(), 30.0),
        (arc_profile(false), std::f64::consts::PI + 8.0_f64.sqrt()),
        (circle, 4.0 * std::f64::consts::PI),
    ] {
        let wire = sketch.wire(&session, &HashMap::new(), None).unwrap();
        assert_eq!(session.shape_type(&wire).unwrap(), ShapeType::Wire);
        assert!(session.is_valid(&wire).unwrap());
        let mut length = 0.0;
        for index in 0..session.subshape_count(&wire, ShapeType::Edge).unwrap() {
            let edge = session.subshape(&wire, ShapeType::Edge, index).unwrap();
            length += session.edge_length(&edge).unwrap();
        }
        assert!((length - perimeter).abs() < 1e-8);
        let face = session.create_face_from_wire(&wire).unwrap();
        assert!(session.is_valid(&face).unwrap());
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn linked_family() -> FamilyDefinition {
    let mut sketch = rectangle();
    sketch.datum_plane = Some("mount".into());
    // Unused inline origin/y expressions must not become dependencies.
    sketch.origin = VectorExpr::Parameter("unused_origin".into());
    sketch.y_axis = VectorExpr::Parameter("unused_y_axis".into());
    FamilyDefinition {
        id: "LinkedSketch".into(),
        version: 1,
        parameters: vec![ParameterDefinition {
            id: "height".into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
            minimum: None,
            maximum: None,
        }],
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        requirements: Vec::new(),
        datums: vec![DatumDefinition {
            id: "mount".into(),
            kind: DatumKind::Plane {
                origin: VectorExpr::Components {
                    x: length(10.0),
                    y: length(20.0),
                    z: ScalarExpr::Parameter("height".into()),
                },
                normal: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            },
        }],
        features: vec![
            FeatureDefinition {
                id: "wire".into(),
                operation: FeatureOperation::SketchWire {
                    sketch: Box::new(sketch.clone()),
                },
            },
            FeatureDefinition {
                id: "face".into(),
                operation: FeatureOperation::SketchFace {
                    sketch: Box::new(sketch),
                },
            },
            FeatureDefinition {
                id: "placed_wire".into(),
                operation: FeatureOperation::Translate {
                    input: "wire".into(),
                    offset: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        2.0,
                        LengthUnit::Millimeter,
                    )),
                },
            },
            FeatureDefinition {
                id: "unrelated".into(),
                operation: FeatureOperation::Box {
                    origin: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                    size: VectorExpr::Literal(VectorQuantity::lengths(
                        1.0,
                        1.0,
                        1.0,
                        LengthUnit::Millimeter,
                    )),
                },
            },
        ],
    }
}

#[test]
fn linked_sketches_follow_datum_parameters_and_definition_edits_incrementally() {
    let session = Session::new().unwrap();
    let mut family = linked_family();
    fn generate<'session>(
        session: &'session Session,
        definition: &FamilyDefinition,
        height: f64,
        previous: Option<&GeneratedResult<'session>>,
    ) -> GeneratedResult<'session> {
        let part = PartInstance {
            id: "part".into(),
            definition,
            overrides: HashMap::from([(
                "height".into(),
                ParameterValue::Scalar(Quantity::length(height, LengthUnit::Millimeter)),
            )]),
            provenance: "test".into(),
        };
        match previous {
            Some(previous) => part.regenerate_incremental(session, previous),
            None => part.regenerate(session),
        }
        .unwrap()
    }
    let original = generate(&session, &family, 3.0, None);
    assert_eq!(
        session.shape_type(original.shape("wire").unwrap()).unwrap(),
        ShapeType::Wire
    );
    assert!(
        (session
            .bounds(original.shape("wire").unwrap())
            .unwrap()
            .min
            .z
            - 3.0)
            .abs()
            < 1e-6
    );
    let unchanged = generate(&session, &family, 3.0, Some(&original));
    assert!(unchanged.regeneration.rebuilt.is_empty());
    assert_eq!(unchanged.regeneration.reused.len(), 4);
    let moved = generate(&session, &family, 8.0, Some(&unchanged));
    assert_eq!(moved.regeneration.rebuilt, ["wire", "face", "placed_wire"]);
    assert_eq!(moved.regeneration.reused, ["unrelated"]);
    assert!(
        (session
            .bounds(moved.shape("placed_wire").unwrap())
            .unwrap()
            .min
            .z
            - 10.0)
            .abs()
            < 1e-6
    );
    assert!((session.surface_area(moved.shape("face").unwrap()).unwrap() - 50.0).abs() < 1e-8);
    if let DatumKind::Plane { normal, .. } = &mut family.datums[0].kind {
        *normal = VectorExpr::Literal(VectorQuantity::scalars(0.0, -1.0, 0.0));
    }
    let rotated = generate(&session, &family, 8.0, Some(&moved));
    assert_eq!(
        rotated.regeneration.rebuilt,
        ["wire", "face", "placed_wire"]
    );
    let bounds = session.bounds(rotated.shape("face").unwrap()).unwrap();
    assert!((bounds.min.y - 20.0).abs() < 1e-6);
    assert!((bounds.max.z - 13.0).abs() < 1e-6);
    drop((original, unchanged, moved, rotated));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn linked_sketches_reject_missing_nonplane_and_incompatible_datums() {
    let session = Session::new().unwrap();
    for case in 0..4 {
        let mut definition = linked_family();
        match case {
            0 => definition.datums.clear(),
            1 => {
                definition.datums[0].kind = DatumKind::Point {
                    origin: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                }
            }
            2 => {
                definition.datums[0].kind = DatumKind::Plane {
                    origin: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                    normal: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                }
            }
            _ => {
                definition.datums[0].kind = DatumKind::Plane {
                    origin: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0)),
                    normal: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                }
            }
        }
        let error = PartInstance {
            id: "part".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        }
        .regenerate(&session)
        .err()
        .expect("invalid linked sketch must fail");
        assert!(
            error.message.contains(if case == 0 {
                "unknown sketch plane"
            } else if case == 1 {
                "must be a plane"
            } else if case == 2 {
                "x axis must lie"
            } else {
                "datum"
            }),
            "{error}"
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn schema_twenty_seven_preserves_datum_links_and_wire_outputs() {
    let definition = linked_family();
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
    assert!(session.shape_count().unwrap() > 0);
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut inline = rectangle();
    inline.datum_plane = None;
    let mut legacy = serde_json::to_value(&inline).unwrap();
    legacy.as_object_mut().unwrap().remove("datum_plane");
    assert_eq!(
        serde_json::from_value::<SketchDefinition>(legacy).unwrap(),
        inline
    );
}
