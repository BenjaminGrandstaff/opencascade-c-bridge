//! Release records at scale: 10,000 releases validated with their approvals.

use super::*;
use occt_parametric::{
    ApprovalDigestStatus, DrawingApproval, DrawingApprovalWorkflow, DrawingChangeLeader,
    DrawingChangeMarker, DrawingDefinition, DrawingRelease, DrawingReleaseStatus,
    DrawingRenderOptions, DrawingRevisionTable, DrawingSheet, DrawingSheetOrientation,
    DrawingSheetSize, DrawingView, DrawingViewKind, InstanceOutputRef, SheetRevisionMethod,
    SheetRevisions, SheetZones,
};

/// 10,000 approved releases, each signed by one APPROVED role.
fn released(count: usize) -> Vec<DrawingRelease> {
    (0..count)
        .map(|i| {
            let date = format!(
                "{:04}-{:02}-{:02}",
                2000 + i / 336,
                1 + (i / 28) % 12,
                1 + i % 28
            );
            DrawingRelease {
                revision: format!("R{i}"),
                description: format!("Release {i}"),
                date: date.clone(),
                approvals: vec![DrawingApproval {
                    role: "APPROVED".into(),
                    name: "Approver".into(),
                    date,
                    digest: None,
                }],
                model_revision: None,
                changes: Vec::new(),
            }
        })
        .collect()
}

/// A three-role workflow signs the next release of a drawing with 10,000
/// released revisions, each sign-off digesting the whole drawing, and the
/// three digests are verified.
pub(crate) fn workflow_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "releases: 3-role sign-off and verification after 10000 releases".into(),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let mut releases = released(10_000);
            let roles = ["DRAWN", "CHECKED", "APPROVED"];
            for release in &mut releases {
                release.approvals = roles
                    .iter()
                    .map(|role| DrawingApproval {
                        role: (*role).into(),
                        name: "Signer".into(),
                        date: release.date.clone(),
                        digest: None,
                    })
                    .collect();
            }
            releases.push(DrawingRelease {
                revision: "NEXT".into(),
                description: "Next release".into(),
                date: "2031-01-01".into(),
                approvals: Vec::new(),
                model_revision: None,
                changes: Vec::new(),
            });
            let mut drawing = DrawingDefinition {
                approval_workflow: Some(DrawingApprovalWorkflow {
                    roles: roles.iter().map(|r| (*r).to_owned()).collect(),
                }),
                releases,
                ..release_drawing()
            };
            drawing.validate(&graph)?;
            for role in roles {
                drawing.sign_release(role, "Signer", "2031-01-01")?;
            }
            let verified = drawing.verify_approvals()?;
            if verified.len() != 3
                || verified
                    .iter()
                    .any(|v| v.status != ApprovalDigestStatus::Valid)
                || drawing.latest_released().map(|r| r.revision.as_str()) != Some("NEXT")
            {
                return Err(failure(format!("{verified:?}")));
            }
            Ok("3 sign-offs digested and verified over 10001 releases".into())
        },
    )
}

pub(crate) fn release_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "releases: 10000 release records with approvals".into(),
        Duration::from_millis(50),
        Expectation::Required,
        || {
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let releases = released(10_000);
            let mut document = ModelDocument::from_graph(&graph);
            document.drawings.push(DrawingDefinition {
                releases,
                ..release_drawing()
            });
            document.instance_graph()?;
            let status = document.drawing_release_status("released")?;
            if status
                != (DrawingReleaseStatus::Untracked {
                    revision: "R9999".into(),
                })
            {
                return Err(failure(format!("{status:?}")));
            }
            Ok("10000 releases validated; latest release untracked".into())
        },
    )
}

/// 10,000 revision symbols, 100 on each of 100 releases with every tenth
/// leading to a datum, on a zoned ANSI E sheet whose revision table lists
/// each release's zones. The drawing generates and exports.
pub(crate) fn change_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        "releases: 10000 change markers in 100 releases with a zone column".into(),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let mut definition = definition.clone();
            definition.datums.push(DatumDefinition {
                id: "corner".into(),
                kind: DatumKind::Point {
                    origin: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                },
            });
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let releases: Vec<_> = (0..100)
                .map(|r| DrawingRelease {
                    revision: format!("R{r}"),
                    description: format!("Release {r}"),
                    date: format!("2026-{:02}-{:02}", 1 + r / 28, 1 + r % 28),
                    approvals: Vec::new(),
                    model_revision: None,
                    changes: (0..100)
                        .map(|m| DrawingChangeMarker {
                            position_mm: [
                                200.0 + 8.0 * (m % 75) as f64,
                                100.0 + 7.0 * r as f64 + (m / 75) as f64 * 3.0,
                            ],
                            leader: (m % 10 == 0).then(|| DrawingChangeLeader {
                                view: "top".into(),
                                anchor: DatumRef::new("part", "corner"),
                            }),
                        })
                        .collect(),
                })
                .collect();
            let drawing = DrawingDefinition {
                annotation_views: Vec::new(),
                dataset: None,
                approval_workflow: None,
                datum_reference_frames: Vec::new(),
                datum_features: Vec::new(),
                feature_control_frames: Vec::new(),
                surface_textures: Vec::new(),
                parts_list: None,
                balloons: Vec::new(),
                releases,
                revision_table: Some(DrawingRevisionTable {
                    position_mm: [20.0, 830.0],
                    zone_column: true,
                }),
                sheet: Some(DrawingSheet {
                    size: DrawingSheetSize::AnsiE,
                    orientation: DrawingSheetOrientation::Landscape,
                    drawing_number: "CHG-1".into(),
                    revision: "R99".into(),
                    sheet_number: 1,
                    sheet_count: 1,
                    projection: None,
                    lettering: None,
                    zones: Some(SheetZones::Grid {
                        columns: 16,
                        rows: 12,
                    }),
                    line_weights: None,
                    revisions: None,
                }),
                id: "changed".into(),
                title: "Changed".into(),
                paper_size_mm: [1117.6, 863.6],
                guides: Vec::new(),
                dimensions: Vec::new(),
                notes: Vec::new(),
                metadata: Default::default(),
                views: vec![DrawingView {
                    id: "top".into(),
                    outputs: vec![InstanceOutputRef {
                        instance: "part".into(),
                        output: "body".into(),
                    }],
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                    paper_origin_mm: [60.0, 60.0],
                    scale: 1.0,
                    show_hidden: false,
                    kind: DrawingViewKind::Orthographic,
                    detail: None,
                    material_hatching: Default::default(),
                    thread_representation: false,
                    hatching: None,
                }],
            };
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let generated = drawing.generate(
                &graph,
                &session,
                DrawingRenderOptions {
                    exact_curves: false,
                    curve_tolerance_mm: 0.01,
                    curve_samples: 8,
                    maximum_vertices: 2_000_000,
                },
            )?;
            let symbols = generated
                .gdt_labels
                .iter()
                .filter(|l| l.text.starts_with('R'))
                .count();
            let svg = generated.to_svg();
            if symbols != 10_000 || !svg.contains("ZONE") {
                return Err(failure(format!("{symbols} revision symbols drawn")));
            }
            Ok(format!("{symbols} symbols, {} kB SVG", svg.len() / 1024))
        },
    )
}

/// A one-view drawing with no releases.
fn release_drawing() -> DrawingDefinition {
    DrawingDefinition {
        annotation_views: Vec::new(),
        dataset: None,
        approval_workflow: None,
        datum_reference_frames: Vec::new(),
        datum_features: Vec::new(),
        feature_control_frames: Vec::new(),
        surface_textures: Vec::new(),
        parts_list: None,
        balloons: Vec::new(),
        releases: Vec::new(),
        revision_table: None,
        sheet: None,
        id: "released".into(),
        title: "Released".into(),
        paper_size_mm: [420.0, 297.0],
        guides: Vec::new(),
        dimensions: Vec::new(),
        notes: Vec::new(),
        metadata: Default::default(),
        views: vec![DrawingView {
            id: "top".into(),
            outputs: vec![InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            }],
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [50.0, 50.0],
            scale: 1.0,
            show_hidden: false,
            kind: DrawingViewKind::Orthographic,
            detail: None,
            material_hatching: Default::default(),
            thread_representation: false,
            hatching: None,
        }],
    }
}

/// Sheet `n` of `count` of drawing MS-1, revised by `method`.
fn sheet(n: u32, count: u32, method: SheetRevisionMethod, revision: &str) -> DrawingDefinition {
    DrawingDefinition {
        id: format!("sheet-{n}"),
        paper_size_mm: [1117.6, 863.6],
        sheet: Some(DrawingSheet {
            size: DrawingSheetSize::AnsiE,
            orientation: DrawingSheetOrientation::Landscape,
            drawing_number: "MS-1".into(),
            revision: revision.into(),
            sheet_number: n,
            sheet_count: count,
            projection: None,
            lettering: None,
            zones: None,
            line_weights: None,
            revisions: Some(SheetRevisions {
                method,
                status_table_mm: None,
            }),
        }),
        ..release_drawing()
    }
}

/// 1,000 sheets sharing one revision validate as a document, and a 30-sheet
/// per-sheet drawing generates with sheet 1's status table.
pub(crate) fn sheet_set_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "releases: 1000 same-revision sheets validated, 30-sheet status table".into(),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let same = SheetRevisionMethod::SameRevision;
            let mut sheets: Vec<_> = (1..=1_000).map(|n| sheet(n, 1_000, same, "R9")).collect();
            sheets[0].releases = released(10);
            let mut document = ModelDocument::from_graph(&graph);
            document.drawings = sheets;
            document.instance_graph()?;
            let per = SheetRevisionMethod::PerSheet;
            let mut batch: Vec<_> = (1..=30)
                .map(|n| {
                    let mut drawing = sheet(n, 30, per, &format!("R{}", n - 1));
                    drawing.releases = released(n as usize);
                    drawing
                })
                .collect();
            batch[0]
                .sheet
                .as_mut()
                .and_then(|s| s.revisions.as_mut())
                .expect("sheet revisions")
                .status_table_mm = Some([20.0, 830.0]);
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let generated = DrawingDefinition::generate_many(
                &batch,
                &graph,
                &session,
                DrawingRenderOptions::default(),
            )?;
            let labels = &generated[0].sheet_labels;
            if labels.last().map(|l| l.text.as_str()) != Some("R29") {
                return Err(failure("status table missing its last sheet".into()));
            }
            Ok("1000 sheets validated; 30-row status table drawn".into())
        },
    )
}
