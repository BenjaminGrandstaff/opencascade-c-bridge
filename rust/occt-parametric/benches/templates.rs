use occt_bridge::Session;
use occt_parametric::*;
use std::{
    collections::{BTreeMap, HashMap},
    time::Instant,
};

fn fixture() -> (ModelDocument, DrawingDefinition) {
    let mut family = FamilyDefinition {
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
    for (id, x, y) in [("center", 0.0, 0.0), ("x", 5.0, 0.0), ("y", 0.0, 5.0)] {
        family.datums.push(DatumDefinition {
            id: id.into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    x,
                    y,
                    15.0,
                    LengthUnit::Millimeter,
                )),
            },
        });
    }
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
    let mut annotated = template.clone();
    let graph = document.instance_graph().unwrap();
    let definitions: Vec<_> = (0..1000)
        .map(|index| {
            let mut drawing = template.clone();
            drawing.id = format!("template-{index}");
            drawing.views[0].origin.z.value = (index as f64 + 0.5) * 30.0 / 1000.0;
            drawing
        })
        .collect();
    annotated.dimensions = (0..10_000)
        .map(|index| {
            let (direction, first, second, tolerance) = match index % 5 {
                0 => (
                    DimensionDirection::Horizontal,
                    "center",
                    "x",
                    DimensionTolerance::Symmetric {
                        deviation: Quantity::length(0.01, LengthUnit::Millimeter),
                    },
                ),
                1 => (
                    DimensionDirection::Radius,
                    "center",
                    "x",
                    DimensionTolerance::Reference,
                ),
                2 => (
                    DimensionDirection::Diameter,
                    "center",
                    "x",
                    DimensionTolerance::Limits {
                        lower: Quantity::length(9.9, LengthUnit::Millimeter),
                        upper: Quantity::length(10.1, LengthUnit::Millimeter),
                    },
                ),
                3 => (
                    DimensionDirection::Angular {
                        vertex: DatumRef::new("part", "center"),
                    },
                    "x",
                    "y",
                    DimensionTolerance::Deviations {
                        lower: Quantity::scalar(-0.001),
                        upper: Quantity::scalar(0.001),
                    },
                ),
                _ => (
                    DimensionDirection::Vertical,
                    "center",
                    "y",
                    DimensionTolerance::Basic,
                ),
            };
            DrawingDimension {
                id: format!("dimension-{index}"),
                view: "profile".into(),
                first: DatumRef::new("part", first),
                second: DatumRef::new("part", second),
                direction,
                offset_mm: 10.0,
                precision: 3,
                presentation: DimensionPresentation {
                    tolerance,
                    ..Default::default()
                },
            }
        })
        .collect();
    let session = Session::new().unwrap();
    let started = Instant::now();
    let generated = annotated
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_samples: 8,
                maximum_vertices: 1_000_000,
            },
        )
        .unwrap();
    assert_eq!(generated.labels.len(), 10_000);
    assert_eq!(generated.generated_variants, 1);
    assert!(
        generated
            .labels
            .iter()
            .any(|l| l.text == "90.000 +0.057/-0.057 °")
    );
    assert!(generated.to_svg().contains("(R5.000 mm)"));
    assert!(generated.to_dxf().contains("10.100/9.900 mm"));
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "10000 mixed manufacturing dimensions: {:?} (10s budget), one shared variant",
        started.elapsed()
    );
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
    let mut hole_document = document.clone();
    hole_document.family.features.push(FeatureDefinition {
        id: "hole".into(),
        operation: FeatureOperation::Hole {
            input: "body".into(),
            position: VectorExpr::Literal(VectorQuantity::lengths(
                5.0,
                5.0,
                30.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
            diameter: ScalarExpr::Literal(Quantity::length(4.0, LengthUnit::Millimeter)),
            extent: HoleExtent::ThroughAll,
            finish: HoleFinish::Plain,
            thread: None,
        },
    });
    let hole_graph = hole_document.instance_graph().unwrap();
    annotated.views[0].outputs[0].output = "hole".into();
    for dimension in &mut annotated.dimensions {
        dimension.direction = DimensionDirection::Diameter;
        dimension.first = DatumRef::new("part", "center");
        dimension.second = DatumRef::new("part", "x");
        dimension.presentation = DimensionPresentation {
            hole: Some(InstanceOutputRef {
                instance: "part".into(),
                output: "hole".into(),
            }),
            ..Default::default()
        };
    }
    let started = Instant::now();
    let generated = annotated
        .generate(
            &hole_graph,
            &session,
            DrawingRenderOptions {
                curve_samples: 8,
                maximum_vertices: 1_000_000,
            },
        )
        .unwrap();
    assert_eq!(generated.labels.len(), 10_000);
    assert!(generated.labels.iter().all(|l| l.text == "Ø4.000 mm THRU"));
    assert_eq!(generated.generated_variants, 1);
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "10000 feature-linked hole callouts: {:?} (10s budget), one shared variant",
        started.elapsed()
    );
}
