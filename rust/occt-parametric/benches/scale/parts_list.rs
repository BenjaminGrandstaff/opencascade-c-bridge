//! Parts lists at assembly scale: grouping 10,000 shown instances into items.

use super::*;
use occt_parametric::{
    DrawingBalloon, DrawingDefinition, DrawingPartsList, DrawingView, DrawingViewKind,
    InstanceOutputRef,
};

pub(crate) fn parts_list_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "parts list: 10000 shown instances, 100 balloons".into(),
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
            let outputs: Vec<_> = members
                .iter()
                .map(|instance| InstanceOutputRef {
                    instance: instance.clone(),
                    output: "body".into(),
                })
                .collect();
            let drawing = DrawingDefinition {
                datum_reference_frames: Vec::new(),
                datum_features: Vec::new(),
                feature_control_frames: Vec::new(),
                surface_textures: Vec::new(),
                parts_list: Some(DrawingPartsList {
                    position_mm: [10.0, 990.0],
                    part_numbers: Default::default(),
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
                    hatching: None,
                }],
            };
            let items = drawing.parts_list_items(&graph)?;
            // Validation (including every balloon's leader) runs as for a saved file.
            let mut document = ModelDocument::from_graph(&graph);
            document.drawings.push(drawing);
            document.instance_graph()?;
            if items.len() != 1 || items[0].quantity != 10_000 {
                return Err(failure(format!("{} items", items.len())));
            }
            Ok("10000 instances grouped into one item; 100 balloons validated".into())
        },
    )
}
