//! Release records at scale: 10,000 releases validated with their approvals.

use super::*;
use occt_parametric::{
    DrawingApproval, DrawingDefinition, DrawingRelease, DrawingReleaseStatus, DrawingView,
    DrawingViewKind, InstanceOutputRef,
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
                    }
                })
                .collect();
            let mut document = ModelDocument::from_graph(&graph);
            document.drawings.push(DrawingDefinition {
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
