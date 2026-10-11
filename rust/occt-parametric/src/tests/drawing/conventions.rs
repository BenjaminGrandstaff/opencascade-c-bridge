//! Sheet lettering, reference zones and line-weight presets (schema 134).

use super::*;

fn graph_for(definition: &FamilyDefinition) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    graph
}

fn conventional(size: DrawingSheetSize) -> DrawingDefinition {
    let mut page = drawing();
    page.sheet = Some(standard_sheet(size, Some(ProjectionConvention::ThirdAngle)));
    let sheet = page.sheet.as_mut().unwrap();
    sheet.lettering = Some(SheetLettering::Standard);
    sheet.line_weights = Some(SheetLineWeights::Y14_2m);
    page
}

fn label<'a>(generated: &'a GeneratedDrawing, prefix: &str) -> &'a DrawingLabel {
    generated
        .sheet_labels
        .iter()
        .chain(&generated.labels)
        .find(|label| label.text.starts_with(prefix))
        .unwrap_or_else(|| panic!("no label starting with {prefix}"))
}

#[test]
fn standard_lettering_sets_role_heights_from_sheet_size() {
    let definition = family_with_datums();
    let graph = graph_for(&definition);
    let session = Session::new().unwrap();
    let inch = 25.4;
    for (size, number, title, notes, block) in [
        (
            DrawingSheetSize::AnsiB,
            0.24 * inch,
            0.19 * inch,
            0.14 * inch,
            0.19 * inch,
        ),
        (
            DrawingSheetSize::AnsiD,
            0.24 * inch,
            0.24 * inch,
            0.16 * inch,
            0.24 * inch,
        ),
        (DrawingSheetSize::IsoA1, 6.0, 6.0, 3.0, 3.0),
        (DrawingSheetSize::IsoA3, 3.0, 3.0, 3.0, 3.0),
    ] {
        let page = conventional(size);
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        assert_eq!(label(&generated, "DRAWING: ").height_mm, number);
        assert_eq!(label(&generated, "Block").height_mm, title);
        assert_eq!(label(&generated, "SCALE: ").height_mm, block);
        assert_eq!(label(&generated, "Width: ").height_mm, notes);
        // The dimension label is the only other annotation label.
        assert!(
            generated
                .labels
                .iter()
                .all(|label| label.height_mm == notes)
        );
        let svg = generated.to_svg();
        assert!(svg.contains(&format!("font-size=\"{number}\"")));
        assert!(
            generated
                .to_dxf()
                .contains(&format!("\n40\n{number}\n1\nDRAWING: "))
        );
    }
    // Without a preset every label keeps the legacy 3 mm height.
    let mut page = conventional(DrawingSheetSize::AnsiD);
    page.sheet.as_mut().unwrap().lettering = None;
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(
        generated
            .sheet_labels
            .iter()
            .chain(&generated.labels)
            .all(|label| label.height_mm == 3.0)
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn standard_lettering_wraps_titles_and_rejects_text_that_cannot_fit() {
    let definition = family_with_datums();
    let graph = graph_for(&definition);
    let session = Session::new().unwrap();
    let mut page = conventional(DrawingSheetSize::AnsiD);
    page.title = "MOUNTING BRACKET ASSEMBLY WITH BOLT CIRCLE".into();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let rows: Vec<_> = generated
        .sheet_labels
        .iter()
        .filter(|label| label.text.starts_with("MOUNTING") || label.text == "CIRCLE")
        .collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows[0].position_mm[1] - rows[1].position_mm[1] >= 1.5 * rows[0].height_mm - 1e-9);
    for title in [
        "W".repeat(60),
        "WIDE WORDS ".repeat(9).trim_end().to_owned(),
    ] {
        page.title = title;
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .unwrap_err()
                .message
                .contains("does not fit")
        );
    }
    // The same title fits at the legacy height.
    page.sheet.as_mut().unwrap().lettering = None;
    page.generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn iso_zones_follow_field_counts_fifty_millimetre_fields_and_labels() {
    for (size, long, short) in [
        (DrawingSheetSize::IsoA0, 24, 16),
        (DrawingSheetSize::IsoA1, 16, 12),
        (DrawingSheetSize::IsoA2, 12, 8),
        (DrawingSheetSize::IsoA3, 8, 6),
        (DrawingSheetSize::IsoA4, 6, 4),
    ] {
        for orientation in [
            DrawingSheetOrientation::Landscape,
            DrawingSheetOrientation::Portrait,
        ] {
            let mut sheet = standard_sheet(size, None);
            sheet.orientation = orientation;
            sheet.zones = Some(SheetZones::Iso5457);
            let grid = sheet.zone_grid().unwrap();
            let (columns, rows) = if orientation == DrawingSheetOrientation::Landscape {
                (long, short)
            } else {
                (short, long)
            };
            assert_eq!(grid.columns_mm.len(), columns + 1);
            assert_eq!(grid.rows_mm.len(), rows + 1);
            let [w, h] = sheet.paper_size_mm();
            let centre = 0.5 * w;
            for boundary in &grid.columns_mm[1..columns] {
                let steps = (boundary - centre) / 50.0;
                assert!((steps - steps.round()).abs() < 1e-9);
            }
            assert!(grid.columns_mm.windows(2).all(|pair| pair[1] > pair[0]));
            assert!(grid.rows_mm.windows(2).all(|pair| pair[1] < pair[0]));
            assert_eq!(grid.zone_at([13.0, h - 13.0]).as_deref(), Some("A1"));
            let last = format!(
                "{}{}",
                DrawingZoneGrid::row_label(rows - 1),
                DrawingZoneGrid::column_label(columns - 1)
            );
            assert_eq!(grid.zone_at([w - 13.0, 13.0]), Some(last));
            assert_eq!(grid.zone_at([1.0, 1.0]), None);
        }
    }
    let letters: String = (0..24).map(DrawingZoneGrid::row_label).collect();
    assert_eq!(letters, "ABCDEFGHJKLMNPQRSTUVWXYZ");
}

#[test]
fn zones_draw_margin_labels_centring_marks_and_validate_sizes() {
    let definition = family_with_datums();
    let graph = graph_for(&definition);
    let session = Session::new().unwrap();
    for (size, zones, expected_labels) in [
        (DrawingSheetSize::IsoA3, SheetZones::Iso5457, 2 * (8 + 6)),
        // ISO A4 is zoned on the top and right only.
        (DrawingSheetSize::IsoA4, SheetZones::Iso5457, 6 + 4),
        (
            DrawingSheetSize::AnsiD,
            SheetZones::Grid {
                columns: 8,
                rows: 4,
            },
            2 * (8 + 4),
        ),
    ] {
        let mut page = conventional(size);
        page.sheet.as_mut().unwrap().zones = Some(zones);
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        let grid = generated.zones.as_ref().unwrap();
        let zone_labels = generated
            .sheet_labels
            .iter()
            .filter(|label| label.height_mm == generated.lettering.zone)
            .count();
        assert_eq!(zone_labels, expected_labels, "{size:?}");
        let [w, h] = generated.paper_size_mm;
        assert!(
            generated
                .sheet_labels
                .iter()
                .flat_map(|label| [label.position_mm])
                .all(|[x, y]| (0.0..=w).contains(&x) && (0.0..=h).contains(&y))
        );
        if let SheetZones::Grid { .. } = zones {
            let widths: Vec<f64> = grid.columns_mm.windows(2).map(|p| p[1] - p[0]).collect();
            assert!(widths.iter().all(|width| (width - widths[0]).abs() < 1e-9));
        }
        // The title block sits in the lower-right zone.
        let rows = grid.rows_mm.len() - 1;
        let columns = grid.columns_mm.len() - 1;
        assert_eq!(
            grid.zone_at([w - 20.0, 20.0]),
            Some(format!(
                "{}{}",
                DrawingZoneGrid::row_label(rows - 1),
                DrawingZoneGrid::column_label(columns - 1)
            ))
        );
    }
    for (size, zones) in [
        (DrawingSheetSize::AnsiB, SheetZones::Iso5457),
        (
            DrawingSheetSize::IsoA3,
            SheetZones::Grid {
                columns: 0,
                rows: 4,
            },
        ),
        (
            DrawingSheetSize::IsoA3,
            SheetZones::Grid {
                columns: 4,
                rows: 25,
            },
        ),
    ] {
        let mut page = conventional(size);
        page.sheet.as_mut().unwrap().zones = Some(zones);
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn y14_2m_line_weights_export_thick_outlines_and_thin_annotations() {
    let definition = family_with_datums();
    let graph = graph_for(&definition);
    let session = Session::new().unwrap();
    let page = conventional(DrawingSheetSize::AnsiB);
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let svg = generated.to_svg();
    assert!(svg.contains("stroke-width=\"0.7\""));
    assert!(svg.contains("stroke-width=\"0.35\""));
    assert!(!svg.contains("stroke-width=\"0.25\""));
    let dxf = generated.to_dxf();
    assert!(dxf.contains("\n370\n70\n"));
    assert!(dxf.contains("\n370\n35\n"));
    // Dimension lines and arrows are thin; the frame is thick.
    assert!(generated.polylines.iter().any(|line| line.thin));
    assert!(generated.sheet_lines.iter().any(|line| !line.thin));
    let mut legacy = page.clone();
    legacy.sheet.as_mut().unwrap().line_weights = None;
    let generated = legacy
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(!generated.to_svg().contains("stroke-width=\"0.7\""));
    assert!(!generated.to_dxf().contains("\n370\n70\n"));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn sheet_conventions_round_trip_and_older_documents_default_to_none() {
    let definition = family_with_datums();
    let graph = graph_for(&definition);
    let mut page = conventional(DrawingSheetSize::AnsiD);
    page.sheet.as_mut().unwrap().zones = Some(SheetZones::Grid {
        columns: 8,
        rows: 4,
    });
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"lettering\": \"standard\""));
    assert!(json.contains("\"line_weights\": \"y14_2m\""));
    assert!(json.contains("\"columns\": 8"));
    assert_eq!(
        ModelDocument::from_json(&json).unwrap().drawings,
        document.drawings
    );
    let mut iso = document.clone();
    let sheet = iso.drawings[0].sheet.as_mut().unwrap();
    sheet.size = DrawingSheetSize::IsoA3;
    sheet.zones = Some(SheetZones::Iso5457);
    let iso_json = iso.to_json_pretty().unwrap();
    assert!(iso_json.contains("\"zones\": \"iso5457\""), "{iso_json}");
    // Schema 133 documents have no convention fields.
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["schema_version"] = 133.into();
    let sheet = value["drawings"][0]["sheet"].as_object_mut().unwrap();
    for key in ["lettering", "zones", "line_weights"] {
        sheet.remove(key);
    }
    let loaded = ModelDocument::from_json(&value.to_string()).unwrap();
    let sheet = loaded.drawings[0].sheet.as_ref().unwrap();
    assert_eq!(
        (sheet.lettering, sheet.zones, sheet.line_weights),
        (None, None, None)
    );
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
}
