//! Standard sheet sizes, orientations, and projection symbols.

use super::*;

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
                curve_tolerance_mm: 0.01,
                exact_curves: false,
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
