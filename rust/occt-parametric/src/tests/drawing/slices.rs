//! Planar slice exports.

use super::*;

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
        curve_tolerance_mm: 0.01,
        exact_curves: false,
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
                    curve_tolerance_mm: 0.01,
                    exact_curves: false,
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
