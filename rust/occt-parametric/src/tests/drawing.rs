use super::*;

fn slice_definition(
    output: &str,
    origin: VectorQuantity,
    direction: VectorQuantity,
) -> DrawingDefinition {
    DrawingDefinition {
        id: "cutting-template".into(),
        title: "True plane profile".into(),
        paper_size_mm: [1000.0, 1000.0],
        views: vec![DrawingView {
            id: "slice".into(),
            outputs: vec![InstanceOutputRef {
                instance: "part".into(),
                output: output.into(),
            }],
            origin,
            direction,
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [0.0, 0.0],
            scale: 1.0,
            show_hidden: false,
            kind: DrawingViewKind::Slice,
            detail: None,
        }],
        dimensions: vec![],
        notes: vec![],
        metadata: BTreeMap::new(),
    }
}

#[test]
fn drawing_batches_share_variants_and_enforce_global_budgets_and_unique_ids() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let first = slice_definition(
        "body",
        VectorQuantity::lengths(0.0, 0.0, 5.0, LengthUnit::Millimeter),
        VectorQuantity::scalars(0.0, 0.0, 1.0),
    );
    let mut second = first.clone();
    second.id = "second".into();
    second.views[0].origin.z.value = 25.0;
    let definitions = vec![first, second];
    let session = Session::new().unwrap();
    let options = DrawingRenderOptions {
        curve_samples: 8,
        maximum_vertices: 1000,
    };
    let drawings =
        DrawingDefinition::generate_many(&definitions, &graph, &session, options).unwrap();
    assert_eq!(drawings.len(), 2);
    assert!(
        drawings
            .iter()
            .all(|drawing| drawing.generated_variants == 1)
    );
    assert!(
        DrawingDefinition::generate_many(
            &definitions,
            &graph,
            &session,
            DrawingRenderOptions {
                maximum_vertices: 70,
                ..options
            }
        )
        .is_err()
    );
    assert!(
        DrawingDefinition::generate_many(
            &[definitions[0].clone(), definitions[0].clone()],
            &graph,
            &session,
            options
        )
        .is_err()
    );
    assert!(DrawingDefinition::generate_many(&[], &graph, &session, options).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn slice_planes_follow_nested_rotated_frames_far_from_origin_and_reject_shells() {
    let mut definition = family_with_datums();
    definition.features.push(FeatureDefinition {
        id: "shell".into(),
        operation: FeatureOperation::Sew {
            inputs: vec!["body".into()],
            tolerance: ScalarExpr::Literal(Quantity::length(1e-6, LengthUnit::Millimeter)),
        },
    });
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    graph
        .add_frame(
            "mounted",
            None,
            Placement {
                translation: VectorQuantity::lengths(1e6, 1e6, 1e6, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: std::f64::consts::FRAC_PI_4,
                }),
            },
            "test",
        )
        .unwrap();
    graph.set_instance_frame("part", Some("mounted")).unwrap();
    let d = std::f64::consts::FRAC_1_SQRT_2;
    let mut drawing = slice_definition(
        "body",
        VectorQuantity::lengths(1e6 - 10.0 * d, 1e6 + 10.0 * d, 1e6, LengthUnit::Millimeter),
        VectorQuantity::scalars(d, -d, 0.0),
    );
    drawing.views[0].x_axis = VectorQuantity::scalars(d, d, 0.0);
    let session = Session::new().unwrap();
    let generated = drawing
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let points: Vec<_> = generated
        .polylines
        .iter()
        .flat_map(|line| &line.points_mm)
        .collect();
    assert!(points.iter().all(|point| point[0] >= -1e-5
        && point[0] <= 10.0 + 1e-5
        && point[1] >= -1e-5
        && point[1] <= 30.0 + 1e-5));
    assert_eq!(generated.polylines.len(), 4);
    drawing.views[0].outputs[0].output = "shell".into();
    assert!(
        drawing
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap_err()
            .message
            .contains("solid geometry")
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn slices_export_only_swept_section_geometry_and_handle_root_tip_and_empty_planes() {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    let section = |y, x, chord| LoftSection {
        profile: vec![[0.0, 0.0], [chord, 0.0], [chord, 2.0], [0.0, 2.0]],
        origin: VectorExpr::Literal(VectorQuantity::lengths(x, y, 0.0, LengthUnit::Millimeter)),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        scale: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
        rotation_radians: None,
        pivot: [0.0, 0.0],
    };
    family.features = vec![FeatureDefinition {
        id: "loft".into(),
        operation: FeatureOperation::Loft {
            sections: vec![section(0.0, 0.0, 10.0), section(10.0, 20.0, 4.0)],
            smooth: false,
            ruled: true,
        },
    }];
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let accepted = ModelDocument::from_graph(&graph);
    let session = Session::new().unwrap();
    for (y, expected) in [
        (0.0, Some((0.0, 10.0))),
        (5.0, Some((10.0, 17.0))),
        (10.0, Some((20.0, 24.0))),
        (11.0, None),
    ] {
        let drawing = slice_definition(
            "loft",
            VectorQuantity::lengths(0.0, y, 0.0, LengthUnit::Millimeter),
            VectorQuantity::scalars(0.0, -1.0, 0.0),
        );
        let generated = drawing
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        assert_eq!(generated.generated_variants, 1);
        match expected {
            Some((minimum, maximum)) => {
                let points: Vec<_> = generated
                    .polylines
                    .iter()
                    .flat_map(|line| &line.points_mm)
                    .collect();
                let found_minimum = points
                    .iter()
                    .map(|point| point[0])
                    .fold(f64::INFINITY, f64::min);
                let found_maximum = points
                    .iter()
                    .map(|point| point[0])
                    .fold(f64::NEG_INFINITY, f64::max);
                assert!(
                    (found_minimum - minimum).abs() < 1e-6,
                    "{y}: {found_minimum}"
                );
                assert!(
                    (found_maximum - maximum).abs() < 1e-6,
                    "{y}: {found_maximum}"
                );
                assert_eq!(generated.polylines.len(), 4);
                assert!(generated.polylines.iter().all(|line| !line.hidden));
            }
            None => assert!(generated.polylines.is_empty()),
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    assert_eq!(ModelDocument::from_graph(&graph), accepted);
}

#[test]
fn slices_retain_hole_boundaries_persist_in_schema_51_and_honor_vertex_budgets() {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.features.push(FeatureDefinition {
        id: "hole".into(),
        operation: FeatureOperation::Cylinder {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                5.0,
                10.0,
                -1.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            radius: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
            height: ScalarExpr::Literal(Quantity::length(40.0, LengthUnit::Millimeter)),
        },
    });
    family.features.push(FeatureDefinition {
        id: "drilled".into(),
        operation: FeatureOperation::Cut {
            object: "body".into(),
            tool: "hole".into(),
        },
    });
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let drawing = slice_definition(
        "drilled",
        VectorQuantity::lengths(0.0, 0.0, 15.0, LengthUnit::Millimeter),
        VectorQuantity::scalars(0.0, 0.0, 1.0),
    );
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(drawing.clone());
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    let session = Session::new().unwrap();
    let generated = drawing
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let round = generated
        .polylines
        .iter()
        .find(|line| {
            line.points_mm
                .iter()
                .all(|p| ((p[0] - 5.0).hypot(p[1] - 10.0) - 2.0).abs() < 1e-6)
        })
        .unwrap();
    assert!((round.points_mm[0][0] - round.points_mm.last().unwrap()[0]).abs() < 1e-6);
    assert_eq!(generated.polylines.len(), 5);
    assert!(generated.to_dxf().contains("$INSUNITS"));
    assert!(
        drawing
            .generate(
                &graph,
                &session,
                DrawingRenderOptions {
                    curve_samples: 64,
                    maximum_vertices: 10
                }
            )
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut old = serde_json::to_value(ModelDocument::from_graph(&graph)).unwrap();
    old["schema_version"] = serde_json::json!(50);
    assert_eq!(
        ModelDocument::from_json(&old.to_string())
            .unwrap()
            .schema_version,
        51
    );
}

fn family_with_datums() -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Required, 100000.0);
    definition.requirements.clear();
    definition.datums = vec![
        DatumDefinition {
            id: "origin".into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
        },
        DatumDefinition {
            id: "corner".into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Components {
                    x: ScalarExpr::Parameter("width".into()),
                    y: ScalarExpr::Parameter("depth".into()),
                    z: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
                },
            },
        },
    ];
    definition
}
fn drawing() -> DrawingDefinition {
    DrawingDefinition {
        id: "drawing".into(),
        title: "Block <assembly> & drawing".into(),
        paper_size_mm: [297.0, 210.0],
        views: vec![DrawingView {
            id: "top".into(),
            outputs: vec![InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            }],
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [40.0, 100.0],
            scale: 3.0,
            show_hidden: true,
            kind: DrawingViewKind::Orthographic,
            detail: None,
        }],
        dimensions: vec![DrawingDimension {
            id: "width".into(),
            view: "top".into(),
            first: DatumRef::new("part", "origin"),
            second: DatumRef::new("part", "corner"),
            direction: DimensionDirection::Horizontal,
            offset_mm: -10.0,
            precision: 1,
        }],
        notes: vec![DrawingNote {
            id: "width-note".into(),
            position_mm: [100.0, 150.0],
            text: DrawingText::Parameter {
                instance: "part".into(),
                parameter: "width".into(),
                prefix: "Width: ".into(),
                suffix: " mm".into(),
                precision: 1,
            },
        }],
        metadata: BTreeMap::from([
            ("Revision".into(), "A".into()),
            ("Author".into(), "Engineering".into()),
        ]),
    }
}

#[test]
fn drawings_regenerate_geometry_dimensions_and_notes_and_round_trip() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let drawing = drawing();
    let session = Session::new().unwrap();
    let generated = drawing
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(generated.generated_variants, 1);
    assert!(generated.polylines.iter().any(|line| line.hidden));
    assert!(generated.labels.iter().any(|label| label.text == "10.0 mm"));
    assert!(
        generated
            .labels
            .iter()
            .any(|label| label.text == "Width: 10.0 mm")
    );
    let svg = generated.to_svg();
    assert!(svg.contains("width=\"297mm\""));
    assert!(svg.contains("Block &lt;assembly&gt; &amp; drawing"));
    assert!(svg.contains("stroke-dasharray=\"2 1\""));
    let dxf = generated.to_dxf();
    assert!(dxf.contains("9\n$INSUNITS\n70\n4\n"));
    assert!(dxf.contains("8\nHIDDEN\n"));
    assert!(dxf.ends_with("0\nEOF\n"));
    if std::env::var_os("OCCT_DRAWING_QA").is_some() {
        std::fs::write("/tmp/occb-generated-drawing.svg", svg).unwrap();
        std::fs::write("/tmp/occb-generated-drawing.dxf", dxf).unwrap();
    }
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(drawing.clone());
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    assert_eq!(
        loaded.drawings[0]
            .generate(
                &loaded.instance_graph().unwrap(),
                &session,
                DrawingRenderOptions::default()
            )
            .unwrap(),
        generated
    );
    graph
        .set_override(
            "part",
            "width",
            ParameterValue::Scalar(Quantity::length(15.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    let edited = drawing
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(edited.labels.iter().any(|label| label.text == "15.0 mm"));
    assert!(
        edited
            .labels
            .iter()
            .any(|label| label.text == "Width: 15.0 mm")
    );
    assert_ne!(edited.polylines, generated.polylines);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn drawing_failures_release_projection_geometry_and_preserve_document() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let accepted = ModelDocument::from_graph(&graph);
    let session = Session::new().unwrap();
    assert!(
        drawing()
            .generate(
                &graph,
                &session,
                DrawingRenderOptions {
                    curve_samples: 64,
                    maximum_vertices: 2
                }
            )
            .unwrap_err()
            .message
            .contains("vertex budget")
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    for mutate in [
        |drawing: &mut DrawingDefinition| drawing.views[0].scale = 0.0,
        |drawing: &mut DrawingDefinition| {
            drawing.views[0].direction = VectorQuantity::scalars(1.0, 0.0, 0.0)
        },
        |drawing: &mut DrawingDefinition| drawing.views[0].outputs[0].output = "missing".into(),
        |drawing: &mut DrawingDefinition| drawing.dimensions[0].view = "missing".into(),
        |drawing: &mut DrawingDefinition| drawing.dimensions[0].precision = 13,
        |drawing: &mut DrawingDefinition| {
            drawing.dimensions[0].second = drawing.dimensions[0].first.clone()
        },
        |drawing: &mut DrawingDefinition| {
            drawing.notes[0].text = DrawingText::Parameter {
                instance: "part".into(),
                parameter: "missing".into(),
                prefix: String::new(),
                suffix: String::new(),
                precision: 1,
            }
        },
    ] {
        let mut invalid = drawing();
        mutate(&mut invalid);
        assert!(
            invalid
                .generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    assert_eq!(ModelDocument::from_graph(&graph), accepted);
    let mut old = serde_json::to_value(&accepted).unwrap();
    old["schema_version"] = serde_json::json!(41);
    old.as_object_mut().unwrap().remove("drawings");
    let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
    assert!(migrated.drawings.is_empty());
    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
}

#[test]
fn drawing_dimension_directions_share_geometry_and_changes_have_stable_ids() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut definition = drawing();
    for (id, direction) in [
        ("height", DimensionDirection::Vertical),
        ("diagonal", DimensionDirection::Aligned),
    ] {
        let mut dimension = definition.dimensions[0].clone();
        dimension.id = id.into();
        dimension.direction = direction;
        definition.dimensions.push(dimension);
    }
    let session = Session::new().unwrap();
    let generated = definition
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(generated.labels.iter().any(|label| label.text == "20.0 mm"));
    assert!(generated.labels.iter().any(|label| label.text == "22.4 mm"));
    let mut before = ModelDocument::from_graph(&graph);
    before.drawings.push(definition);
    let mut after = before.clone();
    after.drawings[0].dimensions.reverse();
    assert!(before.semantic_diff(&after).unwrap().is_empty());
    after.drawings[0].dimensions[0].offset_mm = 12.0;
    let changes = before.semantic_diff(&after).unwrap();
    assert_eq!(changes.len(), 1);
    assert!(
        changes[0]
            .path
            .contains(&DocumentPathSegment::Entity("diagonal".into()))
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn section_and_detail_views_clip_geometry_and_keep_current_annotations() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut drawing = drawing();
    drawing.dimensions.clear();
    drawing.notes.clear();
    drawing.views[0].show_hidden = false;
    drawing.views[0].kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(5.0, 0.0, 0.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(1.0, 0.0, 0.0),
        keep_positive: true,
    };
    let session = Session::new().unwrap();
    let generated = drawing
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(
        generated
            .polylines
            .iter()
            .flat_map(|line| &line.points_mm)
            .all(|point| point[0] >= 55.0 - 1e-7 && point[0] <= 70.0 + 1e-7)
    );
    assert!(!generated.polylines.is_empty());
    drawing.views[0].kind = DrawingViewKind::Orthographic;
    drawing.views[0].detail = Some(DrawingDetail {
        minimum_mm: [2.0, 4.0],
        maximum_mm: [10.0, 16.0],
    });
    let generated = drawing
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(!generated.polylines.is_empty());
    for point in generated.polylines.iter().flat_map(|line| &line.points_mm) {
        assert!(point[0] >= 40.0 - 1e-7 && point[0] <= 64.0 + 1e-7);
        assert!(point[1] >= 100.0 - 1e-7 && point[1] <= 136.0 + 1e-7);
    }
    drawing.views[0].kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(100.0, 0.0, 0.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(1.0, 0.0, 0.0),
        keep_positive: true,
    };
    let generated = drawing
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(generated.polylines.is_empty());
    assert_eq!(session.shape_count().unwrap(), 0);
    drawing.views[0].detail = Some(DrawingDetail {
        minimum_mm: [2.0, 4.0],
        maximum_mm: [1.0, 16.0],
    });
    assert!(
        drawing
            .generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut document = ModelDocument::from_graph(&graph);
    drawing.views[0].detail = None;
    document.drawings.push(drawing);
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
}

#[test]
fn parameter_notes_and_control_characters_export_as_plain_text() {
    let mut definition = family_with_datums();
    for (id, parameter_type, default) in [
        (
            "enabled",
            ParameterType::Boolean,
            ParameterValue::Boolean(true),
        ),
        ("count", ParameterType::Integer, ParameterValue::Integer(4)),
        (
            "code",
            ParameterType::Choice(vec!["M6\n0\nSECTION".into()]),
            ParameterValue::Choice("M6\n0\nSECTION".into()),
        ),
        (
            "vector",
            ParameterType::Vector(Dimension::Length),
            ParameterValue::Vector(VectorQuantity::lengths(
                1.0,
                2.0,
                3.0,
                LengthUnit::Millimeter,
            )),
        ),
    ] {
        definition.parameters.push(ParameterDefinition {
            id: id.into(),
            parameter_type,
            default,
            minimum: None,
            maximum: None,
        });
    }
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut drawing = drawing();
    for (index, parameter) in ["enabled", "count", "code"].iter().enumerate() {
        drawing.notes.push(DrawingNote {
            id: parameter.to_string(),
            position_mm: [20.0, 50.0 + 5.0 * index as f64],
            text: DrawingText::Parameter {
                instance: "part".into(),
                parameter: parameter.to_string(),
                prefix: String::new(),
                suffix: String::new(),
                precision: 1,
            },
        });
    }
    drawing.notes.push(DrawingNote {
        id: "literal".into(),
        position_mm: [20.0, 70.0],
        text: DrawingText::Literal("A\0B\u{1}C<&>".into()),
    });
    let session = Session::new().unwrap();
    let generated = drawing
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(generated.labels.iter().any(|label| label.text == "true"));
    assert!(generated.labels.iter().any(|label| label.text == "4"));
    let svg = generated.to_svg();
    assert!(svg.contains("ABC&lt;&amp;&gt;"));
    assert!(!svg.contains('\0') && !svg.contains('\u{1}'));
    let dxf = generated.to_dxf();
    assert!(dxf.contains("M6 0 SECTION"));
    assert!(dxf.contains("A B C<&>"));
    assert_eq!(dxf.matches("0\nSECTION\n").count(), 3);
    drawing.notes[0].text = DrawingText::Parameter {
        instance: "part".into(),
        parameter: "vector".into(),
        prefix: String::new(),
        suffix: String::new(),
        precision: 1,
    };
    assert!(
        drawing
            .generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn drawing_text_limits_reject_unrepresentable_dxf_fields_before_generation() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    for mutate in [
        |drawing: &mut DrawingDefinition| drawing.title = "x".repeat(2050),
        |drawing: &mut DrawingDefinition| {
            drawing.notes[0].text = DrawingText::Literal("x".repeat(2050))
        },
        |drawing: &mut DrawingDefinition| {
            drawing.metadata.insert("field".into(), "x".repeat(2050));
        },
    ] {
        let mut invalid = drawing();
        mutate(&mut invalid);
        assert!(
            invalid
                .generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut drawing = drawing();
    drawing.notes[0].text = DrawingText::Literal("x".repeat(2049));
    assert!(
        drawing
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap()
            .to_dxf()
            .contains(&"x".repeat(2049))
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
