use super::*;
use occt_parametric::{
    DrawingDefinition, DrawingDetail, DrawingRenderOptions, DrawingView, DrawingViewKind,
    InstanceOutputRef,
};

pub(crate) fn assembly_drawing_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "drawing 1000-part assembly: indexed projected edges".into(),
        ms(20_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let members = graph.add_linear_pattern(
                "row",
                "member",
                "part",
                1000,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "bench",
            )?;
            let drawing = DrawingDefinition {
                id: "assembly-drawing".into(),
                title: "Assembly drawing".into(),
                paper_size_mm: [1000.0, 1000.0],
                dimensions: Vec::new(),
                notes: Vec::new(),
                metadata: Default::default(),
                views: vec![DrawingView {
                    id: "top".into(),
                    outputs: members
                        .into_iter()
                        .map(|instance| InstanceOutputRef {
                            instance,
                            output: "body".into(),
                        })
                        .collect(),
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                    paper_origin_mm: [10.0, 100.0],
                    scale: 0.01,
                    show_hidden: false,
                    kind: DrawingViewKind::Orthographic,
                    detail: None,
                }],
            };
            let generated = drawing.generate(
                &graph,
                &session,
                DrawingRenderOptions {
                    curve_samples: 4,
                    maximum_vertices: 1_000_000,
                },
            )?;
            if generated.generated_variants != 1
                || generated.polylines.len() < 4000
                || session
                    .shape_count()
                    .map_err(|error| failure(error.to_string()))?
                    != 0
            {
                return Err(failure(
                    "assembly drawing lost geometry or leaked handles".into(),
                ));
            }
            Ok(format!(
                "1000 parts, {} projected polylines, one variant, indexed traversal, no retained handles",
                generated.polylines.len()
            ))
        },
    )
}

pub(crate) fn drawing_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "drawings 1000: orthographic, section, detail, exports".into(),
        ms(10_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let drawing = DrawingDefinition {
                id: "scale-drawing".into(),
                title: "Scale drawing".into(),
                paper_size_mm: [1000.0, 1000.0],
                dimensions: Vec::new(),
                notes: Vec::new(),
                metadata: Default::default(),
                views: (0..1000)
                    .map(|index| DrawingView {
                        id: format!("view{index}"),
                        outputs: vec![InstanceOutputRef {
                            instance: "part".into(),
                            output: "body".into(),
                        }],
                        origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                        direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
                        x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                        paper_origin_mm: [(index % 32) as f64 * 30.0, (index / 32) as f64 * 30.0],
                        scale: 1.0,
                        show_hidden: false,
                        kind: if index % 3 == 1 {
                            DrawingViewKind::Section {
                                origin: VectorQuantity::lengths(
                                    5.0,
                                    0.0,
                                    0.0,
                                    LengthUnit::Millimeter,
                                ),
                                normal: VectorQuantity::scalars(1.0, 0.0, 0.0),
                                keep_positive: true,
                            }
                        } else {
                            DrawingViewKind::Orthographic
                        },
                        detail: if index % 3 == 2 {
                            Some(DrawingDetail {
                                minimum_mm: [2.0, 2.0],
                                maximum_mm: [10.0, 18.0],
                            })
                        } else {
                            None
                        },
                    })
                    .collect(),
            };
            let generated = drawing.generate(
                &graph,
                &session,
                DrawingRenderOptions {
                    curve_samples: 4,
                    maximum_vertices: 1_000_000,
                },
            )?;
            if generated.generated_variants != 1
                || generated.polylines.len() < 1000
                || !generated.to_svg().ends_with("</svg>\n")
                || !generated.to_dxf().ends_with("0\nEOF\n")
                || session
                    .shape_count()
                    .map_err(|error| failure(error.to_string()))?
                    != 0
            {
                return Err(failure("drawing outputs, reuse, or cleanup differs".into()));
            }
            Ok(format!(
                "1000 views, {} polylines, one variant, SVG/DXF exports, no retained handles",
                generated.polylines.len()
            ))
        },
    )
}
