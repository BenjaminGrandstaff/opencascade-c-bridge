use super::*;

fn slice_definition(
    output: &str,
    origin: VectorQuantity,
    direction: VectorQuantity,
) -> DrawingDefinition {
    DrawingDefinition {
        sheet: None,
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
            hatching: None,
        }],
        guides: Vec::new(),
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
fn slices_retain_hole_boundaries_persist_in_current_schema_and_honor_vertex_budgets() {
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
        CURRENT_SCHEMA_VERSION
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
        sheet: None,
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
            hatching: None,
        }],
        guides: Vec::new(),
        dimensions: vec![DrawingDimension {
            id: "width".into(),
            view: "top".into(),
            first: DatumRef::new("part", "origin"),
            second: DatumRef::new("part", "corner"),
            direction: DimensionDirection::Horizontal,
            presentation: DimensionPresentation::default(),
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

fn annotated_family() -> FamilyDefinition {
    let mut definition = family_with_datums();
    for (id, x, y) in [("x", 5.0, 0.0), ("y", 0.0, 5.0)] {
        definition.datums.push(DatumDefinition {
            id: id.into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    x,
                    y,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
        });
    }
    definition
}
fn manufactured_dimension(
    direction: DimensionDirection,
    tolerance: DimensionTolerance,
) -> DrawingDimension {
    DrawingDimension {
        id: "manufactured".into(),
        view: "top".into(),
        first: DatumRef::new("part", "origin"),
        second: DatumRef::new("part", "x"),
        direction,
        offset_mm: 10.0,
        precision: 3,
        presentation: DimensionPresentation {
            tolerance,
            ..Default::default()
        },
    }
}

fn assert_radial_leader(dimension: &DrawingDimension, generated: &GeneratedDrawing) {
    if matches!(
        dimension.direction,
        DimensionDirection::Radius | DimensionDirection::Diameter
    ) {
        let start = if matches!(dimension.direction, DimensionDirection::Diameter) {
            [25.0, 100.0]
        } else {
            [40.0, 100.0]
        };
        assert!(
            generated
                .polylines
                .iter()
                .any(|p| p.points_mm == vec![start, [55.0, 100.0]])
        );
        assert!(generated.labels[0].position_mm[0] < 100.0);
    }
}

#[test]
fn manufacturing_dimensions_export_tolerances_units_basic_and_reference_and_migrate() {
    let definition = annotated_family();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mm = |v| Quantity::length(v, LengthUnit::Millimeter);
    let cases = [
        (
            DimensionDirection::Radius,
            DimensionTolerance::Symmetric { deviation: mm(0.1) },
            "R5.000 ±0.100 mm",
        ),
        (
            DimensionDirection::Diameter,
            DimensionTolerance::Deviations {
                lower: mm(-0.05),
                upper: mm(0.1),
            },
            "Ø10.000 +0.100/-0.050 mm",
        ),
        (
            DimensionDirection::Horizontal,
            DimensionTolerance::Limits {
                lower: mm(4.9),
                upper: mm(5.2),
            },
            "5.200/4.900 mm",
        ),
        (
            DimensionDirection::Aligned,
            DimensionTolerance::Basic,
            "5.000 mm",
        ),
        (
            DimensionDirection::Vertical,
            DimensionTolerance::Reference,
            "(5.000 mm)",
        ),
    ];
    for (direction, tolerance, expected) in cases {
        let mut page = drawing();
        page.notes.clear();
        let mut dimension = manufactured_dimension(direction, tolerance);
        if matches!(dimension.direction, DimensionDirection::Vertical) {
            dimension.second = DatumRef::new("part", "y");
        }
        page.dimensions = vec![dimension];
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        assert_eq!(generated.labels[0].text, expected);
        assert_radial_leader(&page.dimensions[0], &generated);
        if matches!(
            page.dimensions[0].presentation.tolerance,
            DimensionTolerance::Deviations { .. } | DimensionTolerance::Limits { .. }
        ) {
            let stack = generated.labels[0].stack.as_ref().unwrap();
            assert!(!stack.upper.contains('/'));
            assert!(!stack.lower.contains('/'));
            assert!(generated.to_svg().contains("font-size=\"2.2\""));
            assert!(generated.to_dxf().contains("40\n2.2\n"));
        }
        if std::env::var_os("OCCT_DRAWING_QA").is_some() {
            std::fs::write(
                format!(
                    "/tmp/occb-dimension-{}.svg",
                    generated.labels[0].text.chars().next().unwrap()
                ),
                generated.to_svg(),
            )
            .unwrap();
        }
        assert!(generated.to_svg().contains(expected));
        assert!(generated.to_dxf().contains(expected));
        if matches!(
            page.dimensions[0].presentation.tolerance,
            DimensionTolerance::Basic
        ) {
            assert!(
                generated
                    .polylines
                    .iter()
                    .any(|p| p.points_mm.len() == 5 && p.points_mm[0] == p.points_mm[4])
            );
        }
        let mut document = ModelDocument::from_graph(&graph);
        document.drawings.push(page);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
    }
    let mut page = drawing();
    page.notes.clear();
    page.dimensions = vec![manufactured_dimension(
        DimensionDirection::Diameter,
        DimensionTolerance::Symmetric {
            deviation: Quantity::length(0.001, LengthUnit::Inch),
        },
    )];
    page.dimensions[0].presentation.length_unit = LengthUnit::Inch;
    page.dimensions[0].precision = 5;
    for scale in [0.001, 1.0, 1000.0] {
        page.views[0].scale = scale;
        assert_eq!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .unwrap()
                .labels[0]
                .text,
            "Ø0.39370 ±0.00100 in"
        );
    }
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(drawing());
    let mut old: serde_json::Value =
        serde_json::from_str(&document.to_json_pretty().unwrap()).unwrap();
    old["schema_version"] = 57.into();
    old["drawings"][0]["dimensions"][0]
        .as_object_mut()
        .unwrap()
        .remove("presentation");
    let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        migrated.drawings[0].dimensions[0].presentation,
        DimensionPresentation::default()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn angular_dimensions_use_minor_angles_arc_arrows_and_radian_tolerances() {
    let definition = annotated_family();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = drawing();
    page.notes.clear();
    let mut dimension = manufactured_dimension(
        DimensionDirection::Angular {
            vertex: DatumRef::new("part", "origin"),
        },
        DimensionTolerance::Symmetric {
            deviation: Quantity::scalar(0.5f64.to_radians()),
        },
    );
    dimension.first = DatumRef::new("part", "x");
    dimension.second = DatumRef::new("part", "y");
    page.dimensions = vec![dimension];
    let session = Session::new().unwrap();
    for reverse in [false, true] {
        if reverse {
            let d = &mut page.dimensions[0];
            std::mem::swap(&mut d.first, &mut d.second);
        }
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        assert_eq!(generated.labels[0].text, "90.000 ±0.500 °");
        let arc = generated
            .polylines
            .iter()
            .find(|p| p.points_mm.len() == 65)
            .unwrap();
        for p in &arc.points_mm {
            assert!(((p[0] - 40.0).hypot(p[1] - 100.0) - 10.0).abs() < 1e-10);
        }
        assert_eq!(
            arc.points_mm.first().unwrap(),
            if reverse {
                &[40.0, 110.0]
            } else {
                &[50.0, 100.0]
            }
        );
        assert!(generated.to_svg().contains("90.000 ±0.500 °"));
    }
    page.dimensions[0].second = DatumRef::new("part", "corner");
    let before = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    graph
        .set_override(
            "part",
            "depth",
            ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    let after = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_ne!(before.labels[0].text, after.labels[0].text);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn invalid_manufacturing_dimensions_fail_without_handles_or_document_mutation() {
    let definition = annotated_family();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mm = |v| Quantity::length(v, LengthUnit::Millimeter);
    let mut page = drawing();
    page.notes.clear();
    for tolerance in [
        DimensionTolerance::Symmetric {
            deviation: mm(-0.1),
        },
        DimensionTolerance::Symmetric {
            deviation: Quantity::scalar(0.1),
        },
        DimensionTolerance::Symmetric {
            deviation: mm(f64::MAX),
        },
        DimensionTolerance::Deviations {
            lower: mm(0.1),
            upper: mm(0.2),
        },
        DimensionTolerance::Deviations {
            lower: mm(-6.0),
            upper: mm(0.1),
        },
        DimensionTolerance::Limits {
            lower: mm(6.0),
            upper: mm(7.0),
        },
        DimensionTolerance::Limits {
            lower: mm(4.0),
            upper: mm(3.0),
        },
    ] {
        page.dimensions = vec![manufactured_dimension(
            DimensionDirection::Radius,
            tolerance,
        )];
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    page.dimensions = vec![manufactured_dimension(
        DimensionDirection::Angular {
            vertex: DatumRef::new("part", "origin"),
        },
        DimensionTolerance::None,
    )];
    page.dimensions[0].first = DatumRef::new("part", "x");
    page.dimensions[0].second = DatumRef::new("part", "x");
    assert!(
        page.generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    page.dimensions[0].second = DatumRef::new("part", "y");
    page.dimensions[0].offset_mm = 0.0;
    assert!(
        page.generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    page.dimensions[0].offset_mm = 10.0;
    page.views[0].origin.z.value = 1.0;
    assert!(
        page.generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    page.views[0].origin.z.value = 0.0;
    assert!(
        page.generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_samples: 2,
                maximum_vertices: 70
            }
        )
        .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn callout_family(finish: HoleFinish, extent: HoleExtent) -> FamilyDefinition {
    let mut definition = annotated_family();
    definition.parameters.push(length_parameter("bore", 4.0));
    definition.features.push(FeatureDefinition {
        id: "hole".into(),
        operation: FeatureOperation::Hole {
            input: "body".into(),
            position: VectorExpr::Literal(VectorQuantity::lengths(
                5.0,
                5.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            diameter: ScalarExpr::Parameter("bore".into()),
            extent,
            finish,
            thread: Some(Box::new(ThreadSpecification {
                designation: "M5x0.8".into(),
                nominal_diameter: ScalarExpr::Literal(Quantity::length(
                    5.0,
                    LengthUnit::Millimeter,
                )),
                pitch: ScalarExpr::Literal(Quantity::length(0.8, LengthUnit::Millimeter)),
                handedness: ThreadHandedness::Left,
            })),
        },
    });
    definition
}
fn callout_page() -> DrawingDefinition {
    let mut page = drawing();
    page.notes.clear();
    page.views[0].outputs[0].output = "hole".into();
    let mut dimension =
        manufactured_dimension(DimensionDirection::Diameter, DimensionTolerance::None);
    dimension.presentation.hole = Some(InstanceOutputRef {
        instance: "part".into(),
        output: "hole".into(),
    });
    page.dimensions = vec![dimension];
    page
}
#[test]
fn hole_callouts_follow_feature_edits_and_preserve_recess_depth_and_thread_intent() {
    let mm = |v| ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter));
    let definition = callout_family(
        HoleFinish::Counterbore {
            diameter: mm(6.0),
            depth: mm(2.0),
        },
        HoleExtent::Blind { depth: mm(8.0) },
    );
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = callout_page();
    let session = Session::new().unwrap();
    let mut limited = page.clone();
    limited.dimensions[0].presentation.tolerance = DimensionTolerance::Limits {
        lower: Quantity::length(3.9, LengthUnit::Millimeter),
        upper: Quantity::length(4.1, LengthUnit::Millimeter),
    };
    let limited = limited
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(limited.labels[0].text.starts_with("Ø4.100/3.900 mm"));
    assert_eq!(limited.labels[0].stack.as_ref().unwrap().upper, "4.100");
    let before = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(
        before.labels[0].text,
        "Ø4.000 mm DEPTH 8.000 mm; CBORE Ø6.000 DEPTH 2.000 mm; THREAD M5x0.8 (5.000 x 0.800 mm, LH)"
    );
    graph
        .set_override(
            "part",
            "bore",
            ParameterValue::Scalar(Quantity::length(4.2, LengthUnit::Millimeter)),
        )
        .unwrap();
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page.clone());
    let reloaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    let after = reloaded.drawings[0]
        .generate(
            &reloaded.instance_graph().unwrap(),
            &session,
            DrawingRenderOptions::default(),
        )
        .unwrap();
    assert!(after.labels[0].text.starts_with("Ø4.200 mm"));
    assert!(after.to_dxf().contains("THREAD M5x0.8"));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn hole_callouts_cover_through_plain_countersink_and_invalid_references() {
    let mm = |v| ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter));
    let session = Session::new().unwrap();
    for finish in [
        HoleFinish::Plain,
        HoleFinish::Countersink {
            diameter: mm(6.0),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
        },
    ] {
        let definition = callout_family(finish.clone(), HoleExtent::ThroughAll);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let mut page = callout_page();
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        assert!(generated.labels[0].text.contains("THRU"));
        if !matches!(finish, HoleFinish::Plain) {
            assert!(
                generated.labels[0]
                    .text
                    .contains("CSINK Ø6.000 mm x 90.000°")
            );
        }
        for reference in [
            InstanceOutputRef {
                instance: "missing".into(),
                output: "hole".into(),
            },
            InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            },
            InstanceOutputRef {
                instance: "part".into(),
                output: "missing".into(),
            },
        ] {
            page.dimensions[0].presentation.hole = Some(reference);
            assert!(
                page.generate(&graph, &session, DrawingRenderOptions::default())
                    .is_err()
            );
        }
        page = callout_page();
        page.dimensions[0].direction = DimensionDirection::Radius;
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn radial_and_angular_dimensions_preserve_far_rotated_placement_and_detail_coordinates() {
    let definition = annotated_family();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    // Rotate the part in XY by 90°, at a world origin one kilometer away.
    graph
        .set_placement(
            "part",
            Placement {
                translation: VectorQuantity::lengths(1e6, -1e6, 3e5, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: std::f64::consts::FRAC_PI_2,
                }),
            },
        )
        .unwrap();
    let mut page = drawing();
    page.notes.clear();
    page.views[0].origin = VectorQuantity::lengths(1e6, -1e6, 3e5, LengthUnit::Millimeter);
    page.views[0].x_axis = VectorQuantity::scalars(0.0, 1.0, 0.0);
    page.views[0].detail = Some(DrawingDetail {
        minimum_mm: [-10.0, -10.0],
        maximum_mm: [40.0, 40.0],
    });
    let mut angle = manufactured_dimension(
        DimensionDirection::Angular {
            vertex: DatumRef::new("part", "origin"),
        },
        DimensionTolerance::None,
    );
    angle.id = "angle".into();
    angle.first = DatumRef::new("part", "x");
    angle.second = DatumRef::new("part", "y");
    page.dimensions = vec![
        manufactured_dimension(DimensionDirection::Radius, DimensionTolerance::None),
        angle,
    ];
    let session = Session::new().unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(generated.labels[0].text, "R5.000 mm");
    assert_eq!(generated.labels[1].text, "90.000 °");
    let arc = generated
        .polylines
        .iter()
        .find(|p| p.points_mm.len() == 65)
        .unwrap();
    assert!((arc.points_mm[0][0] - 80.0).abs() < 1e-7);
    assert!((arc.points_mm[0][1] - 130.0).abs() < 1e-7);
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn guide_family() -> FamilyDefinition {
    let mut definition = annotated_family();
    for (id, x, y) in [("cut-first", 5.0, 0.0), ("center", 5.0, 5.0)] {
        definition.datums.push(DatumDefinition {
            id: id.into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    x,
                    y,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
        });
    }
    definition.datums.push(DatumDefinition {
        id: "cut-second".into(),
        kind: DatumKind::Point {
            origin: VectorExpr::Components {
                x: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
                y: ScalarExpr::Parameter("depth".into()),
                z: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
            },
        },
    });
    definition
}
fn guide_page() -> DrawingDefinition {
    let mut page = drawing();
    page.notes.clear();
    page.dimensions.clear();
    let mut section = page.views[0].clone();
    section.id = "section".into();
    section.paper_origin_mm = [160.0, 80.0];
    section.scale = 1.0;
    section.direction = VectorQuantity::scalars(1.0, 0.0, 0.0);
    section.x_axis = VectorQuantity::scalars(0.0, 1.0, 0.0);
    section.kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(5.0, 0.0, 0.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(1.0, 0.0, 0.0),
        keep_positive: false,
    };
    page.views.push(section);
    page.guides = vec![
        DrawingGuide {
            id: "center".into(),
            view: "top".into(),
            kind: DrawingGuideKind::CenterMark {
                center: DatumRef::new("part", "center"),
                half_length_mm: 3.0,
            },
        },
        DrawingGuide {
            id: "axis".into(),
            view: "top".into(),
            kind: DrawingGuideKind::Centerline {
                first: DatumRef::new("part", "origin"),
                second: DatumRef::new("part", "corner"),
                extension_mm: 2.0,
            },
        },
        DrawingGuide {
            id: "cut".into(),
            view: "top".into(),
            kind: DrawingGuideKind::CuttingPlane {
                first: DatumRef::new("part", "cut-first"),
                second: DatumRef::new("part", "cut-second"),
                section_view: "section".into(),
                label: "A".into(),
            },
        },
    ];
    page
}
#[test]
fn drawing_guides_export_line_styles_and_follow_current_datums_and_section_direction() {
    let definition = guide_family();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = guide_page();
    let session = Session::new().unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(generated.generated_variants, 1);
    assert_eq!(generated.guides.len(), 10);
    assert_eq!(
        generated.guides[0].points_mm,
        vec![[52.0, 115.0], [58.0, 115.0]]
    );
    assert_eq!(
        generated.guides[1].points_mm,
        vec![[55.0, 112.0], [55.0, 118.0]]
    );
    assert_eq!(
        generated.guides[3].points_mm,
        vec![[55.0, 100.0], [55.0, 160.0]]
    );
    assert_eq!(
        generated.guides[4].points_mm,
        vec![[59.0, 100.0], [55.0, 100.0]]
    );
    assert!(
        generated
            .labels
            .iter()
            .any(|l| l.text == "SECTION A-A" && l.position_mm == [160.0, 72.0])
    );
    let svg = generated.to_svg();
    let dxf = generated.to_dxf();
    assert!(svg.contains("stroke-dasharray=\"6 1 1 1\""));
    assert!(svg.contains("stroke-width=\"0.5\""));
    assert!(dxf.contains("8\nCENTER\n6\nCONTINUOUS\n370\n18"));
    assert!(dxf.contains("8\nCUTTING_PLANE\n6\nCENTER\n370\n50"));
    assert!(dxf.contains("73\n4\n40\n9\n"));
    if std::env::var_os("OCCT_DRAWING_QA").is_some() {
        std::fs::write("/tmp/occb-drawing-guides.svg", svg).unwrap();
        std::fs::write("/tmp/occb-drawing-guides.dxf", dxf).unwrap();
    }
    graph
        .set_override(
            "part",
            "depth",
            ParameterValue::Scalar(Quantity::length(25.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    let after = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(
        after.guides[3].points_mm,
        vec![[55.0, 100.0], [55.0, 175.0]]
    );
    assert_ne!(generated.guides[2], after.guides[2]);
    let mut reverse = page.clone();
    reverse.views[1].direction = VectorQuantity::scalars(-1.0, 0.0, 0.0);
    let reversed = reverse
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(
        reversed.guides[4].points_mm,
        vec![[51.0, 100.0], [55.0, 100.0]]
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn drawing_guides_persist_migrate_and_merge_by_stable_identity() {
    let definition = guide_family();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(guide_page());
    let reloaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(reloaded, document);
    let mut reordered = document.clone();
    reordered.drawings[0].guides.reverse();
    assert!(document.semantic_diff(&reordered).unwrap().is_empty());
    let mut edit = document.clone();
    edit.drawings[0].guides[0].kind = DrawingGuideKind::CenterMark {
        center: DatumRef::new("part", "center"),
        half_length_mm: 5.0,
    };
    let changes = document.semantic_diff(&edit).unwrap();
    assert_eq!(changes.len(), 1);
    assert!(
        changes[0]
            .path
            .contains(&DocumentPathSegment::Entity("center".into()))
    );
    let mut other = document.clone();
    other.drawings[0].guides[1].kind = DrawingGuideKind::Centerline {
        first: DatumRef::new("part", "origin"),
        second: DatumRef::new("part", "corner"),
        extension_mm: 4.0,
    };
    let merged = ModelDocument::three_way_merge(&document, &edit, &other).unwrap();
    let DocumentMerge::Merged(merged) = merged else {
        panic!("independent guide edits should merge")
    };
    assert_eq!(
        merged.drawings[0]
            .guides
            .iter()
            .find(|g| g.id == "center")
            .unwrap()
            .kind,
        edit.drawings[0].guides[0].kind
    );
    assert_eq!(
        merged.drawings[0]
            .guides
            .iter()
            .find(|g| g.id == "axis")
            .unwrap()
            .kind,
        other.drawings[0].guides[1].kind
    );
    let mut old: serde_json::Value =
        serde_json::from_str(&document.to_json_pretty().unwrap()).unwrap();
    old["schema_version"] = 58.into();
    old["drawings"][0].as_object_mut().unwrap().remove("guides");
    let migrated = ModelDocument::from_json(&old.to_string()).unwrap();
    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    assert!(migrated.drawings[0].guides.is_empty());
}
#[test]
fn invalid_drawing_guides_and_export_budgets_release_all_geometry() {
    let definition = guide_family();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let bad_kinds = vec![
        DrawingGuideKind::CenterMark {
            center: DatumRef::new("part", "center"),
            half_length_mm: 0.0,
        },
        DrawingGuideKind::CenterMark {
            center: DatumRef::new("missing", "center"),
            half_length_mm: 3.0,
        },
        DrawingGuideKind::Centerline {
            first: DatumRef::new("part", "origin"),
            second: DatumRef::new("part", "origin"),
            extension_mm: 2.0,
        },
        DrawingGuideKind::Centerline {
            first: DatumRef::new("part", "origin"),
            second: DatumRef::new("part", "corner"),
            extension_mm: -1.0,
        },
        DrawingGuideKind::CuttingPlane {
            first: DatumRef::new("part", "origin"),
            second: DatumRef::new("part", "corner"),
            section_view: "section".into(),
            label: "A".into(),
        },
        DrawingGuideKind::CuttingPlane {
            first: DatumRef::new("part", "cut-first"),
            second: DatumRef::new("part", "cut-second"),
            section_view: "top".into(),
            label: "A".into(),
        },
        DrawingGuideKind::CuttingPlane {
            first: DatumRef::new("part", "cut-first"),
            second: DatumRef::new("part", "cut-second"),
            section_view: "section".into(),
            label: "A\nB".into(),
        },
    ];
    for kind in bad_kinds {
        let mut page = guide_page();
        page.guides = vec![DrawingGuide {
            id: "invalid".into(),
            view: "top".into(),
            kind,
        }];
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut page = guide_page();
    page.guides.push(page.guides[0].clone());
    assert!(
        page.generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    page = guide_page();
    page.views[1].direction = VectorQuantity::scalars(0.0, 0.0, 1.0);
    page.views[1].x_axis = VectorQuantity::scalars(1.0, 0.0, 0.0);
    assert!(
        page.generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    page = guide_page();
    assert!(
        page.generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_samples: 2,
                maximum_vertices: 25
            }
        )
        .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn center_guides_keep_paper_sizes_across_rotated_far_placements_scales_and_detail_crops() {
    let definition = guide_family();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    graph
        .set_placement(
            "part",
            Placement {
                translation: VectorQuantity::lengths(1e6, -1e6, 3e5, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: std::f64::consts::FRAC_PI_2,
                }),
            },
        )
        .unwrap();
    let mut page = guide_page();
    page.views.pop();
    page.guides.pop();
    page.views[0].origin = VectorQuantity::lengths(1e6, -1e6, 3e5, LengthUnit::Millimeter);
    page.views[0].x_axis = VectorQuantity::scalars(0.0, 1.0, 0.0);
    page.views[0].detail = Some(DrawingDetail {
        minimum_mm: [-10.0, -10.0],
        maximum_mm: [40.0, 40.0],
    });
    let session = Session::new().unwrap();
    for scale in [0.001, 1.0, 1000.0] {
        page.views[0].scale = scale;
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        let center = [40.0 + 15.0 * scale, 100.0 + 15.0 * scale];
        assert!((generated.guides[0].points_mm[0][0] - (center[0] - 3.0)).abs() < 1e-7);
        assert!((generated.guides[0].points_mm[1][0] - (center[0] + 3.0)).abs() < 1e-7);
        assert!(
            (generated.guides[0].points_mm[1][0] - generated.guides[0].points_mm[0][0] - 6.0).abs()
                < 1e-7
        );
        let direction = [10.0f64 / 10.0f64.hypot(20.0), 20.0f64 / 10.0f64.hypot(20.0)];
        assert!(
            (generated.guides[2].points_mm[0][0] - (40.0 + 10.0 * scale - 2.0 * direction[0]))
                .abs()
                < 1e-7
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

fn hatched_slice(output: &str) -> DrawingDefinition {
    let mut page = slice_definition(
        output,
        VectorQuantity::lengths(0.0, 0.0, 5.0, LengthUnit::Millimeter),
        VectorQuantity::scalars(0.0, 0.0, 1.0),
    );
    page.views[0].hatching = Some(SectionHatching {
        angle_radians: 0.0,
        spacing_mm: 1.0,
        phase_mm: 0.5,
    });
    page
}
#[test]
fn section_hatching_follows_real_cut_faces_and_circular_holes_with_safe_exports() {
    let definition = callout_family(HoleFinish::Plain, HoleExtent::ThroughAll);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = hatched_slice("hole");
    let session = Session::new().unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(generated.hatches.len() > 20);
    for line in &generated.hatches {
        let [a, b] = [line.points_mm[0], line.points_mm[1]];
        assert!((a[1] - b[1]).abs() < 1e-10);
        assert!(a[0] >= -1e-7 && b[0] <= 10.0 + 1e-7);
        for i in 1..10 {
            let x = a[0] + (b[0] - a[0]) * f64::from(i) / 10.0;
            // Polygonal chord approximation can enter the circle slightly;
            // 64 samples on a radius-2 circle bound this observed fixture error.
            assert!((x - 5.0).hypot(a[1] - 5.0) >= 1.99);
        }
    }
    let svg = generated.to_svg();
    let dxf = generated.to_dxf();
    assert!(svg.contains("stroke-width=\"0.13\""));
    assert!(dxf.contains("8\nSECTION_HATCH\n370\n13"));
    if std::env::var_os("OCCT_DRAWING_QA").is_some() {
        let mut qa = page.clone();
        qa.paper_size_mm = [100.0, 120.0];
        qa.views[0].paper_origin_mm = [20.0, 50.0];
        qa.views[0].scale = 2.0;
        qa.views[0].hatching = Some(SectionHatching::default());
        let qa = qa
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        std::fs::write("/tmp/occb-hatched-hole.svg", qa.to_svg()).unwrap();
        std::fs::write("/tmp/occb-hatched-hole.dxf", qa.to_dxf()).unwrap();
    }
    let mut section = page.clone();
    section.views[0].kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(0.0, 0.0, 5.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(0.0, 0.0, 1.0),
        keep_positive: false,
    };
    let section = section
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(!section.hatches.is_empty());
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page.clone());
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    graph
        .set_override(
            "part",
            "bore",
            ParameterValue::Scalar(Quantity::length(4.5, LengthUnit::Millimeter)),
        )
        .unwrap();
    let edited = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_ne!(edited.hatches, generated.hatches);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn hatching_unions_overlapping_components_and_keeps_disconnected_islands() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    graph
        .add_clone("overlap", "part", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "overlap",
            Placement::translated(VectorQuantity::lengths(
                5.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph
        .add_clone("island", "part", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "island",
            Placement::translated(VectorQuantity::lengths(
                30.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    let mut page = hatched_slice("body");
    page.views[0].outputs.extend([
        InstanceOutputRef {
            instance: "overlap".into(),
            output: "body".into(),
        },
        InstanceOutputRef {
            instance: "island".into(),
            output: "body".into(),
        },
    ]);
    let session = Session::new().unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(generated.hatches.len(), 40);
    let mut section = page.clone();
    section.views[0].kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(0.0, 0.0, 5.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(0.0, 0.0, 1.0),
        keep_positive: false,
    };
    let section = section
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(section.hatches, generated.hatches);
    assert!(
        section
            .polylines
            .iter()
            .flat_map(|l| &l.points_mm)
            .any(|p| (p[0] - 40.0).abs() < 1e-7)
    );
    for y in [0.5, 5.5, 19.5] {
        let lines: Vec<_> = generated
            .hatches
            .iter()
            .filter(|l| (l.points_mm[0][1] - y).abs() < 1e-7)
            .collect();
        assert_eq!(lines.len(), 2);
        assert!((lines[0].points_mm[0][0]).abs() < 1e-7);
        assert!((lines[0].points_mm[1][0] - 15.0).abs() < 1e-7);
        assert!((lines[1].points_mm[0][0] - 30.0).abs() < 1e-7);
        assert!((lines[1].points_mm[1][0] - 40.0).abs() < 1e-7);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn hatching_clips_details_uses_paper_spacing_and_handles_empty_or_boundary_planes() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mut page = hatched_slice("body");
    page.views[0].scale = 2.0;
    page.views[0].paper_origin_mm = [10.0, 20.0];
    page.views[0].detail = Some(DrawingDetail {
        minimum_mm: [2.0, 3.0],
        maximum_mm: [8.0, 7.0],
    });
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(generated.hatches.len(), 8);
    for line in &generated.hatches {
        assert!((line.points_mm[0][0] - 10.0).abs() < 1e-7);
        assert!((line.points_mm[1][0] - 22.0).abs() < 1e-7);
        assert!(line.points_mm.iter().all(|p| p[1] >= 20.0 && p[1] <= 28.0));
    }
    for z in [0.0, 30.0] {
        let mut boundary = hatched_slice("body");
        boundary.views[0].origin.z.value = z;
        assert_eq!(
            boundary
                .generate(&graph, &session, DrawingRenderOptions::default())
                .unwrap()
                .hatches
                .len(),
            20
        );
    }
    let mut outside = hatched_slice("body");
    outside.views[0].origin.z.value = 40.0;
    assert!(
        outside
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap()
            .hatches
            .is_empty()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn hatching_rejects_invalid_patterns_projection_and_work_or_export_exhaustion() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    for pattern in [
        SectionHatching {
            spacing_mm: 0.0,
            ..Default::default()
        },
        SectionHatching {
            angle_radians: f64::NAN,
            ..Default::default()
        },
        SectionHatching {
            phase_mm: f64::INFINITY,
            ..Default::default()
        },
        SectionHatching {
            spacing_mm: 1e-12,
            ..Default::default()
        },
    ] {
        let mut page = hatched_slice("body");
        page.views[0].hatching = Some(pattern);
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut page = hatched_slice("body");
    page.views[0].kind = DrawingViewKind::Orthographic;
    assert!(
        page.generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    page.views[0].kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(5.0, 0.0, 0.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(1.0, 0.0, 0.0),
        keep_positive: true,
    };
    assert!(
        page.generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    page = hatched_slice("body");
    assert!(
        page.generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_samples: 2,
                maximum_vertices: 25
            }
        )
        .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page);
    let mut old: serde_json::Value =
        serde_json::from_str(&document.to_json_pretty().unwrap()).unwrap();
    old["schema_version"] = 59.into();
    old["drawings"][0]["views"][0]
        .as_object_mut()
        .unwrap()
        .remove("hatching");
    assert!(
        ModelDocument::from_json(&old.to_string()).unwrap().drawings[0].views[0]
            .hatching
            .is_none()
    );
}

fn standard_sheet(
    size: DrawingSheetSize,
    projection: Option<ProjectionConvention>,
) -> DrawingSheet {
    DrawingSheet {
        size,
        orientation: DrawingSheetOrientation::Landscape,
        drawing_number: "BRACKET-001".into(),
        revision: "A".into(),
        sheet_number: 1,
        sheet_count: 2,
        projection,
    }
}
#[test]
fn standard_sheet_sizes_orientations_and_projection_exports_persist() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    for (size, expected) in [
        (DrawingSheetSize::AnsiA, [215.9, 279.4]),
        (DrawingSheetSize::AnsiB, [279.4, 431.8]),
        (DrawingSheetSize::AnsiC, [431.8, 558.8]),
        (DrawingSheetSize::AnsiD, [558.8, 863.6]),
        (DrawingSheetSize::AnsiE, [863.6, 1117.6]),
        (DrawingSheetSize::IsoA0, [841.0, 1189.0]),
        (DrawingSheetSize::IsoA1, [594.0, 841.0]),
        (DrawingSheetSize::IsoA2, [420.0, 594.0]),
        (DrawingSheetSize::IsoA3, [297.0, 420.0]),
        (DrawingSheetSize::IsoA4, [210.0, 297.0]),
    ] {
        assert_eq!(size.dimensions_mm(), expected);
        for orientation in [
            DrawingSheetOrientation::Portrait,
            DrawingSheetOrientation::Landscape,
        ] {
            let mut page = hatched_slice("body");
            page.sheet = Some(standard_sheet(size, Some(ProjectionConvention::ThirdAngle)));
            page.sheet.as_mut().unwrap().orientation = orientation;
            page.metadata
                .insert("Material".into(), "Aluminum & steel".into());
            let generated = page
                .generate(&graph, &session, DrawingRenderOptions::default())
                .unwrap();
            assert_eq!(
                generated.paper_size_mm,
                if orientation == DrawingSheetOrientation::Portrait {
                    expected
                } else {
                    [expected[1], expected[0]]
                }
            );
            assert!(
                generated
                    .sheet_lines
                    .iter()
                    .flat_map(|l| &l.points_mm)
                    .all(|p| p[0] >= 0.0
                        && p[0] <= generated.paper_size_mm[0]
                        && p[1] >= 0.0
                        && p[1] <= generated.paper_size_mm[1])
            );
            assert!(generated.to_svg().contains("Aluminum &amp; steel"));
            assert!(generated.to_dxf().contains("SHEET 1 OF 2"));
            assert!(
                generated
                    .sheet_labels
                    .iter()
                    .any(|l| l.text == "SCALE: 1:1")
            );
            let mut document = ModelDocument::from_graph(&graph);
            document.drawings.push(page);
            assert_eq!(
                ModelDocument::from_json(&document.to_json_pretty().unwrap())
                    .unwrap()
                    .drawings,
                document.drawings
            );
        }
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn projection_symbols_put_end_views_next_to_correct_cone_end_and_ignore_view_scale() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mut page = hatched_slice("body");
    let mut symbols = Vec::new();
    for convention in [
        ProjectionConvention::FirstAngle,
        ProjectionConvention::ThirdAngle,
    ] {
        page.sheet = Some(standard_sheet(DrawingSheetSize::IsoA3, Some(convention)));
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        let circles: Vec<_> = generated
            .sheet_lines
            .iter()
            .filter(|l| l.points_mm.len() == 65)
            .collect();
        assert_eq!(circles.len(), 2);
        let cone = generated
            .sheet_lines
            .iter()
            .find(|l| {
                l.points_mm.len() == 5
                    && l.points_mm[1][1] != l.points_mm[0][1]
                    && l.points_mm[1][0] != l.points_mm[0][0]
            })
            .unwrap();
        let center = circles[0].points_mm[0][0] - 5.0;
        assert_eq!(
            center > cone.points_mm[1][0],
            convention == ProjectionConvention::FirstAngle
        );
        assert!((cone.points_mm[2][1] - cone.points_mm[1][1] - 10.0).abs() < 1e-8);
        assert!((cone.points_mm[3][1] - cone.points_mm[0][1] - 5.0).abs() < 1e-8);
        page.views[0].scale = 2.0;
        let scaled = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        assert_eq!(generated.sheet_lines, scaled.sheet_lines);
        page.views[0].scale = 1.0;
        symbols.push(generated.sheet_lines);
    }
    assert_ne!(symbols[0], symbols[1]);
    page.views[0].scale = 1e-8;
    let tiny = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(
        tiny.sheet_labels
            .iter()
            .any(|l| l.text.contains("1.000000e-8"))
    );
    page.views[0].scale = 1.0;
    let mut second = page.views[0].clone();
    second.id = "enlarged".into();
    second.scale = 2.0;
    page.views.push(second);
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(
        generated
            .sheet_labels
            .iter()
            .any(|l| l.text == "SCALE: AS SHOWN")
    );
    if std::env::var_os("OCCT_DRAWING_QA").is_some() {
        page.views.truncate(1);
        page.views[0].paper_origin_mm = [60.0, 130.0];
        page.views[0].scale = 3.0;
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        std::fs::write("/tmp/occb-standard-sheet.svg", generated.to_svg()).unwrap();
        std::fs::write("/tmp/occb-standard-sheet.dxf", generated.to_dxf()).unwrap();
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn standard_sheet_validation_budgets_and_legacy_migration_preserve_document() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mut page = hatched_slice("body");
    page.sheet = Some(standard_sheet(DrawingSheetSize::AnsiA, None));
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page.clone());
    let before = document.to_json_pretty().unwrap();
    for invalid in 0..7 {
        let mut bad = page.clone();
        let sheet = bad.sheet.as_mut().unwrap();
        match invalid {
            0 => sheet.sheet_number = 0,
            1 => sheet.sheet_number = 3,
            2 => sheet.drawing_number.clear(),
            3 => sheet.revision = "x".repeat(8),
            4 => bad.title = "x".repeat(117),
            5 => {
                bad.metadata = (0..9).map(|i| (i.to_string(), "x".into())).collect();
            }
            _ => sheet.drawing_number = "bad\nnumber".into(),
        }
        assert!(
            bad.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    assert!(
        page.generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_samples: 2,
                maximum_vertices: 10
            }
        )
        .is_err()
    );
    assert_eq!(document.to_json_pretty().unwrap(), before);
    let mut json: serde_json::Value = serde_json::from_str(&before).unwrap();
    json["schema_version"] = 60.into();
    json["drawings"][0].as_object_mut().unwrap().remove("sheet");
    let legacy = ModelDocument::from_json(&json.to_string()).unwrap();
    assert!(legacy.drawings[0].sheet.is_none());
    let generated = legacy.drawings[0]
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(generated.sheet_lines.is_empty());
    assert!(generated.to_svg().contains("True plane profile"));
    assert_eq!(session.shape_count().unwrap(), 0);
}
