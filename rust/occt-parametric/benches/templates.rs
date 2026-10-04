use occt_bridge::Session;
use occt_parametric::*;
use std::{
    collections::{BTreeMap, HashMap},
    time::Instant,
};

fn fixture() -> (ModelDocument, DrawingDefinition) {
    let family = FamilyDefinition {
        references: Vec::new(),
        id: "part".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        requirements: vec![],
        datums: vec![],
        features: vec![FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                size: VectorExpr::Literal(VectorQuantity::lengths(
                    10.0,
                    20.0,
                    30.0,
                    LengthUnit::Millimeter,
                )),
            },
        }],
    };
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let drawing = DrawingDefinition {
        id: "template".into(),
        title: "Section template".into(),
        paper_size_mm: [100.0, 100.0],
        views: vec![DrawingView {
            id: "profile".into(),
            outputs: vec![InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            }],
            origin: VectorQuantity::lengths(0.0, 0.0, 15.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [20.0, 40.0],
            scale: 1.0,
            show_hidden: false,
            kind: DrawingViewKind::Slice,
            detail: None,
        }],
        dimensions: vec![],
        notes: vec![],
        metadata: BTreeMap::new(),
    };
    (ModelDocument::from_graph(&graph), drawing)
}
fn main() {
    let (document, template) = fixture();
    let graph = document.instance_graph().unwrap();
    let definitions: Vec<_> = (0..1000)
        .map(|index| {
            let mut drawing = template.clone();
            drawing.id = format!("template-{index}");
            drawing.views[0].origin.z.value = (index as f64 + 0.5) * 30.0 / 1000.0;
            drawing
        })
        .collect();
    let session = Session::new().unwrap();
    let started = Instant::now();
    let drawings = DrawingDefinition::generate_many(
        &definitions,
        &graph,
        &session,
        DrawingRenderOptions {
            curve_samples: 8,
            maximum_vertices: 100000,
        },
    )
    .unwrap();
    assert_eq!(drawings.len(), 1000);
    assert!(
        drawings
            .iter()
            .all(|drawing| drawing.generated_variants == 1 && drawing.polylines.len() == 4)
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 30.0);
    println!(
        "1000 planar cutting templates: {:?} (30s budget), one shared variant",
        started.elapsed()
    );
}
