//! Center lines, center marks, and other drawing guides.

use super::*;

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
                curve_tolerance_mm: 0.01,
                exact_curves: false,
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
