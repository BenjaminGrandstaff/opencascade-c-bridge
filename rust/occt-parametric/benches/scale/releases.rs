//! Release records at scale: 10,000 releases validated with their approvals.

use super::*;
use occt_parametric::{
    DrawingApproval, DrawingChangeLeader, DrawingChangeMarker, DrawingDefinition, DrawingRelease,
    DrawingReleaseStatus, DrawingRenderOptions, DrawingRevisionTable, DrawingSheet,
    DrawingSheetOrientation, DrawingSheetSize, DrawingView, DrawingViewKind, InstanceOutputRef,
    SheetZones,
};

pub(crate) fn release_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "releases: 10000 release records with approvals".into(),
        Duration::from_millis(50),
        Expectation::Required,
        || {
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let releases: Vec<_> = (0..10_000)
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
                        }],
                        model_revision: None,
                        changes: Vec::new(),
                    }
                })
                .collect();
            let mut document = ModelDocument::from_graph(&graph);
            document.drawings.push(DrawingDefinition {
                annotation_views: Vec::new(),
                dataset: None,
                datum_reference_frames: Vec::new(),
                datum_features: Vec::new(),
                feature_control_frames: Vec::new(),
                surface_textures: Vec::new(),
                parts_list: None,
                balloons: Vec::new(),
                releases,
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
