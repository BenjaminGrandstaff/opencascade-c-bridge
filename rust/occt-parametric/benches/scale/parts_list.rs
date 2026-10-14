//! Parts lists at assembly scale: grouping 10,000 shown instances into items.

use super::*;
use occt_parametric::{
    DrawingBalloon, DrawingDefinition, DrawingPartsList, DrawingView, DrawingViewKind,
    InstanceOutputRef,
};

pub(crate) fn parts_list_case(definition: &'static FamilyDefinition, nested: bool) -> Outcome {
    timed(
        format!(
            "parts list{}: 10000 shown instances, 100 balloons",
            if nested {
                " (nested in a sub-assembly)"
            } else {
                ""
            }
        ),
        Duration::from_secs(2),
        Expectation::Required,
        || {
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let members = graph.add_linear_pattern(
                "row",
                "member",
                "part",
                10_000,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "bench",
            )?;
            if nested {
                graph.add_frame("sub", None, Placement::identity(), "bench")?;
                graph.set_pattern_frame("row", Some("sub"))?;
            }
            let outputs: Vec<_> = members
                .iter()
                .map(|instance| InstanceOutputRef {
                    instance: instance.clone(),
                    output: "body".into(),
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
                parts_list: Some(DrawingPartsList {
                    position_mm: [10.0, 990.0],
                    part_numbers: Default::default(),
                    nested,
                    merge_identical: false,
                }),
                balloons: members
                    .iter()
                    .take(100)
                    .enumerate()
                    .map(|(i, instance)| DrawingBalloon {
                        id: format!("b{i}"),
                        view: "top".into(),
                        instance: instance.clone(),
                        anchor: None,
                        offset_mm: [0.0, 12.0],
                    })
                    .collect(),
                releases: Vec::new(),
                revision_table: None,
                sheet: None,
                id: "bom".into(),
                title: "BOM".into(),
                paper_size_mm: [1000.0, 1000.0],
                guides: Vec::new(),
                dimensions: Vec::new(),
                notes: Vec::new(),
                metadata: Default::default(),
                views: vec![DrawingView {
                    id: "top".into(),
                    outputs,
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                    paper_origin_mm: [10.0, 100.0],
                    scale: 0.001,
                    show_hidden: false,
                    kind: DrawingViewKind::Orthographic,
                    detail: None,
                    material_hatching: Default::default(),
                    thread_representation: false,
                    hatching: None,
                }],
            };
            let items = drawing.parts_list_items(&graph)?;
            // Validation (including every balloon's leader) runs as for a saved file.
            let mut document = ModelDocument::from_graph(&graph);
            document.drawings.push(drawing);
            document.instance_graph()?;
            let part = &items[items.len() - 1];
            if items.len() != 1 + usize::from(nested) || part.quantity != 10_000 {
                return Err(failure(format!("{} items", items.len())));
            }
            Ok(format!(
                "10000 instances grouped into item {}; 100 balloons validated",
                part.label
            ))
        },
    )
}

/// 1,000 identical sub-assemblies, each with five parts and a hub frame of
/// five more (10,000 instances, 2,000 frames), merge into one row of 1,000.
pub(crate) fn merged_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "parts list: 1000 identical sub-assemblies of 10 parts merged".into(),
        Duration::from_secs(2),
        Expectation::Required,
        || {
            let mut graph = InstanceGraph::new(definition);
            let mut outputs = Vec::with_capacity(10_000);
            for a in 0..1_000 {
                graph.add_frame(format!("asm-{a}"), None, Placement::identity(), "bench")?;
                graph.add_frame(
                    format!("hub-{a}"),
                    Some(&format!("asm-{a}")),
                    Placement::identity(),
                    "bench",
                )?;
                for p in 0..10 {
                    let id = format!("part-{a}-{p}");
                    graph.add_base(id.clone(), HashMap::new(), "bench")?;
                    let frame = if p < 5 {
                        format!("asm-{a}")
                    } else {
                        format!("hub-{a}")
                    };
                    graph.set_instance_frame(&id, Some(&frame))?;
                    outputs.push(InstanceOutputRef {
                        instance: id,
                        output: "body".into(),
                    });
                }
            }
            let drawing = DrawingDefinition {
                annotation_views: Vec::new(),
                dataset: None,
                approval_workflow: None,
                datum_reference_frames: Vec::new(),
                datum_features: Vec::new(),
                feature_control_frames: Vec::new(),
                surface_textures: Vec::new(),
                parts_list: Some(DrawingPartsList {
                    position_mm: [10.0, 990.0],
                    part_numbers: Default::default(),
                    nested: true,
                    merge_identical: true,
                }),
                balloons: Vec::new(),
                releases: Vec::new(),
                revision_table: None,
                sheet: None,
                id: "bom".into(),
                title: "BOM".into(),
                paper_size_mm: [1000.0, 1000.0],
                guides: Vec::new(),
                dimensions: Vec::new(),
                notes: Vec::new(),
                metadata: Default::default(),
                views: vec![DrawingView {
                    id: "top".into(),
                    outputs,
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                    paper_origin_mm: [10.0, 100.0],
                    scale: 0.001,
                    show_hidden: false,
                    kind: DrawingViewKind::Orthographic,
                    detail: None,
                    material_hatching: Default::default(),
                    thread_representation: false,
                    hatching: None,
                }],
            };
            let start = Instant::now();
            let items = drawing.parts_list_items(&graph)?;
            let listed = start.elapsed();
            drawing.validate(&graph)?;
            let rows: Vec<_> = items
                .iter()
                .map(|i| (i.label.as_str(), i.quantity, i.instances.len()))
                .collect();
            if rows
                != [
                    ("1", 1_000, 0),
                    ("1.1", 5, 5_000),
                    ("1.2", 1, 0),
                    ("1.2.1", 5, 5_000),
                ]
            {
                return Err(failure(format!("{rows:?}")));
            }
            Ok(format!(
                "4 rows, asm quantity 1000; listed in {:.3} s and validated",
                listed.as_secs_f64()
            ))
        },
    )
}
