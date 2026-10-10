//! Linear, radial, and angular manufacturing dimensions.

use super::*;

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
                curve_tolerance_mm: 0.01,
                exact_curves: false,
                curve_samples: 2,
                maximum_vertices: 70
            }
        )
        .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
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
