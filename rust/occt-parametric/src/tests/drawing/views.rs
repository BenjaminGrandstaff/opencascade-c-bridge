//! Drawing regeneration, failures, sections, details, and notes.

use super::*;

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
                    curve_tolerance_mm: 0.01,
                    exact_curves: false,
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
