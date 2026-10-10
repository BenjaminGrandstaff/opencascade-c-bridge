//! Exact conic and spline export, detail trimming, and exact hatching.

use super::*;

#[test]
fn exact_drawing_conics_export_without_sampling_and_keep_legacy_and_detail_paths() {
    let mut family = family_with_datums();
    family.features.push(FeatureDefinition {
        id: "round".into(),
        operation: FeatureOperation::Cylinder {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            radius: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
            height: ScalarExpr::Literal(Quantity::length(30.0, LengthUnit::Millimeter)),
        },
    });
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mut drawing = slice_definition(
        "round",
        VectorQuantity::lengths(0.0, 0.0, 15.0, LengthUnit::Millimeter),
        VectorQuantity::scalars(0.0, 0.0, 1.0),
    );
    drawing.views[0].scale = 2.0;
    drawing.views[0].paper_origin_mm = [20.0, 30.0];
    let options = DrawingRenderOptions {
        curve_tolerance_mm: 0.01,
        exact_curves: true,
        curve_samples: 2,
        maximum_vertices: 13,
    };
    let generated = drawing.generate(&graph, &session, options).unwrap();
    assert!(generated.polylines.is_empty());
    assert_eq!(generated.curves.len(), 1);
    let DrawingCurveGeometry::Ellipse {
        center_mm,
        major_axis_mm,
        minor_radius_mm,
        start_parameter,
        end_parameter,
    } = generated.curves[0].geometry
    else {
        panic!("expected circle")
    };
    assert_eq!(center_mm, [20.0, 30.0]);
    assert!((major_axis_mm[0].hypot(major_axis_mm[1]) - 10.0).abs() < 1e-9);
    assert!((minor_radius_mm - 10.0).abs() < 1e-9);
    assert_eq!(start_parameter, 0.0);
    assert_eq!(end_parameter, std::f64::consts::TAU);
    assert!(generated.to_dxf().contains("0\nCIRCLE\n"));
    assert_eq!(generated.to_svg().matches(" A ").count(), 2);
    assert!(
        drawing
            .generate(
                &graph,
                &session,
                DrawingRenderOptions {
                    maximum_vertices: 12,
                    ..options
                }
            )
            .is_err()
    );
    let sampled = drawing
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(sampled.polylines.len(), 1);
    assert!(sampled.curves.is_empty());
    drawing.views[0].detail = Some(DrawingDetail {
        minimum_mm: [-2.0, -2.0],
        maximum_mm: [2.0, 2.0],
    });
    let detail = drawing
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                exact_curves: true,
                ..DrawingRenderOptions::default()
            },
        )
        .unwrap();
    assert!(detail.curves.is_empty());
    drawing.views[0].detail = None;
    drawing.views[0].kind = DrawingViewKind::Orthographic;
    drawing.views[0].direction = VectorQuantity::scalars(0.0, 1.0, 1.0);
    drawing.views[0].show_hidden = true;
    let tilted = drawing
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                maximum_vertices: 100,
                ..options
            },
        )
        .unwrap();
    assert!(tilted.to_dxf().contains("0\nELLIPSE\n"));
    assert!(tilted.curves.iter().any(|c| c.hidden));
    assert!(tilted.polylines.is_empty());
    assert_eq!(session.shape_count().unwrap(), 0);
    let old: DrawingRenderOptions = serde_json::from_str("{}").unwrap();
    assert!(!old.exact_curves);
}

#[test]
fn exact_details_trim_curved_boundaries_and_splines_without_crop_perimeters() {
    let mut family = family_with_datums();
    let section = |z| LoftSection {
        profile: vec![
            [0.0, 0.0],
            [5.0, -1.0],
            [10.0, 0.0],
            [8.0, 5.0],
            [4.0, 7.0],
            [-1.0, 4.0],
        ],
        origin: VectorExpr::Literal(VectorQuantity::lengths(0.0, 0.0, z, LengthUnit::Millimeter)),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        scale: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
        rotation_radians: None,
        pivot: [0.0, 0.0],
    };
    family.features = vec![FeatureDefinition {
        id: "curved".into(),
        operation: FeatureOperation::Loft {
            sections: vec![section(0.0), section(30.0)],
            smooth: true,
            ruled: true,
        },
    }];
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mut drawing = slice_definition(
        "curved",
        VectorQuantity::lengths(0.0, 0.0, 15.0, LengthUnit::Millimeter),
        VectorQuantity::scalars(0.0, 0.0, 1.0),
    );
    drawing.views[0].detail = Some(DrawingDetail {
        minimum_mm: [2.0, -10.0],
        maximum_mm: [6.0, 20.0],
    });
    drawing.views[0].scale = 2.0;
    drawing.views[0].paper_origin_mm = [20.0, 40.0];
    let options = DrawingRenderOptions {
        exact_curves: true,
        curve_samples: 2,
        ..DrawingRenderOptions::default()
    };
    let generated = drawing.generate(&graph, &session, options).unwrap();
    assert!(generated.polylines.is_empty());
    assert!(generated.curves.len() >= 2);
    for curve in &generated.curves {
        let DrawingCurveGeometry::Bezier { poles_mm, .. } = &curve.geometry else {
            panic!("crop introduced a non-spline edge")
        };
        for p in [poles_mm[0], *poles_mm.last().unwrap()] {
            assert!(
                (p[0] - 20.0).abs() < 1e-6
                    || (p[0] - 28.0).abs() < 1e-6
                    || (20.0..=28.0).contains(&p[0])
            );
            assert!((40.0..=100.0).contains(&p[1]));
        }
    }
    assert!(generated.to_dxf().contains("0\nSPLINE\n"));
    assert_eq!(
        generated.to_svg().matches("<path ").count(),
        generated.curves.len()
    );
    let denser = drawing
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_samples: 100000,
                ..options
            },
        )
        .unwrap();
    assert_eq!(generated.curves, denser.curves);
    graph
        .add_frame(
            "mounted",
            None,
            Placement {
                translation: VectorQuantity::lengths(1e6, -1e6, 1e6, LengthUnit::Millimeter),
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
    drawing.views[0].origin =
        VectorQuantity::lengths(1e6, -1e6, 1e6 + 15.0, LengthUnit::Millimeter);
    let d = std::f64::consts::FRAC_1_SQRT_2;
    drawing.views[0].x_axis = VectorQuantity::scalars(d, d, 0.0);
    let mounted = drawing.generate(&graph, &session, options).unwrap();
    assert_eq!(mounted.curves.len(), generated.curves.len());
    for (a, b) in mounted.curves.iter().zip(&generated.curves) {
        let DrawingCurveGeometry::Bezier { poles_mm: a, .. } = &a.geometry else {
            panic!()
        };
        let DrawingCurveGeometry::Bezier { poles_mm: b, .. } = &b.geometry else {
            panic!()
        };
        for (a, b) in a.iter().zip(b) {
            assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 1e-4);
        }
    }
    drawing.views[0].detail = Some(DrawingDetail {
        minimum_mm: [100.0, 100.0],
        maximum_mm: [110.0, 110.0],
    });
    assert!(
        drawing
            .generate(&graph, &session, options)
            .unwrap()
            .curves
            .is_empty()
    );
    drawing.views[0].origin.z.value = 1e6 + 100.0;
    assert!(
        drawing
            .generate(&graph, &session, options)
            .unwrap()
            .curves
            .is_empty()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(
        drawing
            .generate(
                &graph,
                &session,
                DrawingRenderOptions {
                    curve_tolerance_mm: f64::NAN,
                    ..options
                }
            )
            .is_err()
    );
}

#[test]
fn exact_hatching_intersects_circle_boundaries_independently_of_curve_samples() {
    let definition = callout_family(HoleFinish::Plain, HoleExtent::ThroughAll);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = hatched_slice("hole");
    let session = Session::new().unwrap();
    let options = DrawingRenderOptions {
        exact_curves: true,
        curve_samples: 2,
        ..DrawingRenderOptions::default()
    };
    let generated = page.generate(&graph, &session, options).unwrap();
    for i in 0..20 {
        let y = i as f64 + 0.5;
        let row: Vec<_> = generated
            .hatches
            .iter()
            .filter(|l| (l.points_mm[0][1] - y).abs() < 1e-8)
            .collect();
        if (y - 5.0).abs() < 2.0 {
            assert_eq!(row.len(), 2);
            let half = (4.0 - (y - 5.0).powi(2)).sqrt();
            assert!((row[0].points_mm[1][0] - (5.0 - half)).abs() < 1e-7);
            assert!((row[1].points_mm[0][0] - (5.0 + half)).abs() < 1e-7);
        } else {
            assert_eq!(row.len(), 1);
            assert!((row[0].points_mm[0][0]).abs() < 1e-8);
            assert!((row[0].points_mm[1][0] - 10.0).abs() < 1e-8);
        }
    }
    let dense = page
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_samples: 100000,
                ..options
            },
        )
        .unwrap();
    assert_eq!(generated.hatches, dense.hatches);
    page.views[0].kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(0.0, 0.0, 5.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(0.0, 0.0, 1.0),
        keep_positive: false,
    };
    // A section's camera origin need not lie on its cut plane.
    page.views[0].origin.z.value = 100.0;
    let section = page.generate(&graph, &session, options).unwrap();
    assert_eq!(section.hatches, generated.hatches);
    page.views[0].kind = DrawingViewKind::Slice;
    page.views[0].origin.z.value = 5.0;
    page.views[0].detail = Some(DrawingDetail {
        minimum_mm: [3.0, 3.0],
        maximum_mm: [7.0, 7.0],
    });
    let detail = page.generate(&graph, &session, options).unwrap();
    assert!(!detail.hatches.is_empty());
    for line in &detail.hatches {
        for p in &line.points_mm {
            assert!(p[0] >= -1e-7 && p[0] <= 4.0 + 1e-7 && p[1] >= -1e-7 && p[1] <= 4.0 + 1e-7);
            assert!((p[0] - 2.0).hypot(p[1] - 2.0) >= 2.0 - 1e-7);
        }
    }
    assert!(
        page.generate(
            &graph,
            &session,
            DrawingRenderOptions {
                maximum_vertices: 10,
                ..options
            }
        )
        .is_err()
    );
    page.views[0].hatching.as_mut().unwrap().spacing_mm = 1e-8;
    assert!(
        page.generate(&graph, &session, options)
            .unwrap_err()
            .message
            .contains("resolution")
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn exact_hatching_unions_overlaps_preserves_islands_and_handles_far_frames() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = hatched_slice("body");
    for (id, x) in [("overlap", 5.0), ("island", 30.0)] {
        graph.add_clone(id, "part", HashMap::new(), "test").unwrap();
        graph
            .set_placement(
                id,
                Placement::translated(VectorQuantity::lengths(x, 0.0, 0.0, LengthUnit::Millimeter)),
            )
            .unwrap();
        page.views[0].outputs.push(InstanceOutputRef {
            instance: id.into(),
            output: "body".into(),
        });
    }
    let session = Session::new().unwrap();
    let options = DrawingRenderOptions {
        exact_curves: true,
        ..DrawingRenderOptions::default()
    };
    let generated = page.generate(&graph, &session, options).unwrap();
    assert_eq!(generated.hatches.len(), 40);
    for row in generated.hatches.as_chunks::<2>().0 {
        assert!((row[0].points_mm[0][0]).abs() < 1e-7);
        assert!((row[0].points_mm[1][0] - 15.0).abs() < 1e-7);
        assert!((row[1].points_mm[0][0] - 30.0).abs() < 1e-7);
        assert!((row[1].points_mm[1][0] - 40.0).abs() < 1e-7);
    }
    graph
        .add_frame(
            "mounted",
            None,
            Placement {
                translation: VectorQuantity::lengths(1e6, -1e6, 1e6, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: std::f64::consts::FRAC_PI_4,
                }),
            },
            "test",
        )
        .unwrap();
    for id in ["part", "overlap", "island"] {
        graph.set_instance_frame(id, Some("mounted")).unwrap();
    }
    let d = std::f64::consts::FRAC_1_SQRT_2;
    page.views[0].origin = VectorQuantity::lengths(1e6, -1e6, 1e6 + 5.0, LengthUnit::Millimeter);
    page.views[0].x_axis = VectorQuantity::scalars(d, d, 0.0);
    let mounted = page.generate(&graph, &session, options).unwrap();
    assert_eq!(mounted.hatches.len(), 40);
    for (a, b) in generated.hatches.iter().zip(&mounted.hatches) {
        for (a, b) in a.points_mm.iter().zip(&b.points_mm) {
            assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6);
        }
    }
    page.views[0].origin.z.value += 100.0;
    assert!(
        page.generate(&graph, &session, options)
            .unwrap()
            .hatches
            .is_empty()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
