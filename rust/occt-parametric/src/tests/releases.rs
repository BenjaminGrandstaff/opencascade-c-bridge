//! Drawing release records, approvals, revision tables and release status.

use super::*;

fn approval(role: &str, name: &str, date: &str) -> DrawingApproval {
    DrawingApproval {
        role: role.into(),
        name: name.into(),
        date: date.into(),
    }
}

fn release(revision: &str, date: &str, model: Option<&str>) -> DrawingRelease {
    DrawingRelease {
        revision: revision.into(),
        description: format!("Release {revision}"),
        date: date.into(),
        approvals: vec![
            approval("CHECKED", "R. Checker", date),
            approval("APPROVED", "A. Approver", date),
        ],
        model_revision: model.map(Into::into),
    }
}

fn sheet_drawing(revision: &str) -> DrawingDefinition {
    DrawingDefinition {
        datum_reference_frames: Vec::new(),
        datum_features: Vec::new(),
        feature_control_frames: Vec::new(),
        surface_textures: Vec::new(),
        parts_list: None,
        balloons: Vec::new(),
        releases: Vec::new(),
        revision_table: Some(DrawingRevisionTable {
            position_mm: [240.0, 260.0],
        }),
        sheet: Some(DrawingSheet {
            size: DrawingSheetSize::AnsiB,
            orientation: DrawingSheetOrientation::Landscape,
            drawing_number: "BLK-100".into(),
            revision: revision.into(),
            sheet_number: 1,
            sheet_count: 1,
            projection: None,
        }),
        id: "block".into(),
        title: "Block".into(),
        paper_size_mm: [431.8, 279.4],
        views: vec![DrawingView {
            id: "top".into(),
            outputs: vec![InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            }],
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [60.0, 80.0],
            scale: 2.0,
            show_hidden: false,
            kind: DrawingViewKind::Orthographic,
            detail: None,
            material_hatching: Default::default(),
            hatching: None,
        }],
        guides: Vec::new(),
        dimensions: Vec::new(),
        notes: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

fn metadata(id: &str) -> RevisionMetadata {
    RevisionMetadata {
        id: id.into(),
        author: "test".into(),
        recorded_at: "2026-10-09T12:00:00Z".into(),
        message: id.into(),
    }
}

/// A document whose ledger records r1 and r2 (two width changes).
fn ledger_document(family: &FamilyDefinition) -> ModelDocument {
    let mut graph = InstanceGraph::new(family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(sheet_drawing(""));
    document.drawings[0].revision_table = None;
    for (id, width) in [("r1", 12.0), ("r2", 14.0)] {
        let previous = document.clone();
        document.family.parameters[0].default =
            ParameterValue::Scalar(Quantity::length(width, LengthUnit::Millimeter));
        document.record_revision(&previous, metadata(id)).unwrap();
    }
    document
}

fn block_family() -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family
}

#[test]
fn release_status_follows_the_model_revision_ledger() {
    let family = block_family();
    let mut document = ledger_document(&family);
    assert_eq!(
        document.drawing_release_status("block").unwrap(),
        DrawingReleaseStatus::Unreleased
    );
    let mut drawing = sheet_drawing("B");
    drawing.releases = vec![
        release("A", "2026-10-01", Some("r1")),
        release("B", "2026-10-08", Some("r2")),
    ];
    document.drawings[0] = drawing;
    document.instance_graph().unwrap();
    assert_eq!(
        document.drawing_release_status("block").unwrap(),
        DrawingReleaseStatus::Current {
            revision: "B".into()
        }
    );
    // A new model revision makes the released drawing stale.
    let previous = document.clone();
    document.family.parameters[0].default =
        ParameterValue::Scalar(Quantity::length(16.0, LengthUnit::Millimeter));
    document.record_revision(&previous, metadata("r3")).unwrap();
    assert_eq!(
        document.drawing_release_status("block").unwrap(),
        DrawingReleaseStatus::ModelChanged {
            revision: "B".into(),
            released_against: Some("r2".into()),
            latest_model_revision: "r3".into(),
        }
    );
    // Without a ledger, unlinked releases are untracked.
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut plain = ModelDocument::from_graph(&graph);
    let mut drawing = sheet_drawing("A");
    drawing.releases = vec![release("A", "2026-10-01", None)];
    plain.drawings.push(drawing);
    assert_eq!(
        plain.drawing_release_status("block").unwrap(),
        DrawingReleaseStatus::Untracked {
            revision: "A".into()
        }
    );
    assert!(plain.drawing_release_status("missing").is_err());
}

#[test]
fn releases_dates_approvals_and_links_are_validated() {
    let family = block_family();
    let document = ledger_document(&family);
    let good = || {
        let mut drawing = sheet_drawing("B");
        drawing.releases = vec![
            release("A", "2026-10-01", Some("r1")),
            release("B", "2026-10-08", Some("r2")),
        ];
        drawing
    };
    let check = |drawing: DrawingDefinition| {
        let mut document = document.clone();
        document.drawings[0] = drawing;
        document.instance_graph().map(|_| ())
    };
    assert!(check(good()).is_ok());
    for case in 0..13 {
        let mut drawing = good();
        match case {
            0 => drawing.releases[1].date = "2026-02-30".into(),
            1 => drawing.releases[1].date = "2026-09-30".into(),
            2 => drawing.releases[1].revision = "A".into(),
            3 => drawing.releases[1].revision = "B 2".into(),
            4 => drawing.releases[1].approvals[1].date = "2026-10-09".into(),
            5 => drawing.releases[1].approvals[0].name = " ".into(),
            6 => drawing.releases[1].model_revision = Some("r9".into()),
            7 => {
                drawing.releases[0].model_revision = Some("r2".into());
                drawing.releases[1].model_revision = Some("r1".into());
            }
            8 => drawing.sheet.as_mut().unwrap().revision = "A".into(),
            9 => drawing.releases.clear(),
            10 => drawing.releases[0].description = "x".repeat(201),
            11 => drawing.releases[0].date = "26-10-01".into(),
            _ => drawing.revision_table.as_mut().unwrap().position_mm = [400.0, 260.0],
        }
        assert!(check(drawing).is_err(), "case {case}");
    }
    // Leap days are valid dates.
    let mut drawing = good();
    drawing.releases[0].date = "2024-02-29".into();
    drawing.releases[0]
        .approvals
        .iter_mut()
        .for_each(|a| a.date = "2024-02-29".into());
    assert!(check(drawing).is_ok());
}

#[test]
fn revision_tables_render_with_exact_budgets_and_persist() {
    let family = block_family();
    let mut document = ledger_document(&family);
    let mut drawing = sheet_drawing("B");
    let mut first = release("A", "2026-10-01", Some("r1"));
    first.description =
        "Initial release for production tooling and first-article inspection".into();
    drawing.releases = vec![first, release("B", "2026-10-08", Some("r2"))];
    document.drawings[0] = drawing.clone();
    let graph = document.instance_graph().unwrap();
    let session = Session::new().unwrap();
    let without = {
        let mut plain = drawing.clone();
        plain.revision_table = None;
        plain
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap()
    };
    let generated = drawing
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let count = |d: &GeneratedDrawing| {
        d.sheet_lines
            .iter()
            .map(|l| l.points_mm.len())
            .sum::<usize>()
    };
    // Rectangle 5, two release rows 2 each, three column separators 2 each.
    assert_eq!(count(&generated) - count(&without), 5 + 2 * 2 + 2 * 3);
    let cells: Vec<&str> = generated
        .sheet_labels
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    for text in [
        "REV",
        "DESCRIPTION",
        "Release B",
        "2026-10-08",
        "A. Approver",
    ] {
        assert!(cells.contains(&text), "{text} not in {cells:?}");
    }
    // Long descriptions are shortened to the column in the drawing only.
    assert!(
        cells
            .iter()
            .any(|c| c.starts_with("Initial release") && c.ends_with('…'))
    );
    assert!(generated.to_dxf().contains("Release B") && generated.to_svg().contains("A. Approver"));
    if let Some(directory) = std::env::var_os("OCCB_RELEASE_QA_DIR") {
        std::fs::write(
            std::path::Path::new(&directory).join("releases.svg"),
            generated.to_svg(),
        )
        .unwrap();
    }

    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"releases\"") && json.contains("\"model_revision\": \"r2\""));
    let restored = ModelDocument::from_json(&json).unwrap();
    assert_eq!(restored, document);
    let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
    legacy["schema_version"] = serde_json::json!(89);
    let page = legacy["drawings"][0].as_object_mut().unwrap();
    page.remove("releases");
    page.remove("revision_table");
    page["sheet"]["revision"] = serde_json::json!("");
    let migrated = ModelDocument::from_json(&legacy.to_string()).unwrap();
    assert!(migrated.drawings[0].releases.is_empty());
    drop(graph);
    assert_eq!(session.shape_count().unwrap(), 0);
}
