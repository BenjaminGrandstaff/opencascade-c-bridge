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
        datum_features: Vec::new(),
        feature_control_frames: Vec::new(),
        sheet: None,
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
            hatching: None,
        }],
        guides: Vec::new(),
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
    let mut guide_page = template.clone();
    let mut section = template.views[0].clone();
    section.id = "section".into();
    section.direction = VectorQuantity::scalars(1.0, 0.0, 0.0);
    section.x_axis = VectorQuantity::scalars(0.0, 1.0, 0.0);
    section.kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(0.0, 0.0, 15.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(1.0, 0.0, 0.0),
        keep_positive: true,
    };
    guide_page.views.push(section);
    guide_page.guides = (0..10_000)
        .map(|i| DrawingGuide {
            id: format!("guide-{i}"),
            view: "profile".into(),
            kind: match i % 3 {
                0 => DrawingGuideKind::CenterMark {
                    center: DatumRef::new("part", "center"),
                    half_length_mm: 3.0,
                },
                1 => DrawingGuideKind::Centerline {
                    first: DatumRef::new("part", "center"),
                    second: DatumRef::new("part", "x"),
                    extension_mm: 2.0,
                },
                _ => DrawingGuideKind::CuttingPlane {
                    first: DatumRef::new("part", "center"),
                    second: DatumRef::new("part", "y"),
                    section_view: "section".into(),
                    label: "A".into(),
                },
            },
        })
        .collect();
    let started = Instant::now();
    let generated = guide_page
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_samples: 8,
                maximum_vertices: 1_000_000,
            },
        )
        .unwrap();
    assert_eq!(generated.generated_variants, 1);
    assert_eq!(generated.guides.len(), 33_332);
    assert_eq!(generated.labels.len(), 9_999);
    assert!(
        generated
            .guides
            .iter()
            .any(|g| g.kind == DrawingGuideLineKind::CuttingPlane)
    );
    assert!(generated.to_svg().contains("SECTION A-A"));
    assert!(generated.to_dxf().contains("8\nCENTER\n"));
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "10000 datum-linked drawing guides: {:?} (10s budget), one shared variant",
        started.elapsed()
    );
    // One shared geometry variant, 1,000 disconnected material regions.
    let family = document.family.clone();
    let mut assembly = InstanceGraph::new(&family);
    assembly.add_base("part", HashMap::new(), "test").unwrap();
    let mut hatched = template.clone();
    hatched.paper_size_mm = [15_100.0, 100.0];
    hatched.views[0].outputs.clear();
    hatched.views[0].hatching = Some(SectionHatching {
        angle_radians: 0.0,
        spacing_mm: 2.0,
        phase_mm: 1.0,
    });
    for index in 0..1000 {
        let id = if index == 0 {
            "part".into()
        } else {
            format!("part-{index}")
        };
        if index > 0 {
            assembly
                .add_clone(&id, "part", HashMap::new(), "test")
                .unwrap();
            assembly
                .set_placement(
                    &id,
                    Placement::translated(VectorQuantity::lengths(
                        index as f64 * 15.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                )
                .unwrap();
        }
        hatched.views[0].outputs.push(InstanceOutputRef {
            instance: id,
            output: "body".into(),
        });
    }
    let started = Instant::now();
    let generated = hatched
        .generate(
            &assembly,
            &session,
            DrawingRenderOptions {
                curve_samples: 4,
                maximum_vertices: 1_000_000,
            },
        )
        .unwrap();
    assert_eq!(generated.generated_variants, 1);
    assert_eq!(generated.hatches.len(), 10_000);
    for line in &generated.hatches {
        assert_eq!(line.points_mm.len(), 2);
        assert!((line.points_mm[1][0] - line.points_mm[0][0] - 10.0).abs() < 1e-8);
        assert!((line.points_mm[0][1] - line.points_mm[1][1]).abs() < 1e-8);
    }
    assert!(generated.to_svg().contains("stroke-width=\"0.13\""));
    assert!(generated.to_dxf().contains("8\nSECTION_HATCH\n"));
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 30.0);
    println!(
        "10000 section hatch segments across 1000 placed parts: {:?} (30s budget), one shared variant",
        started.elapsed()
    );
    let sheets: Vec<_> = definitions
        .into_iter()
        .enumerate()
        .map(|(i, mut page)| {
            page.sheet = Some(DrawingSheet {
                size: DrawingSheetSize::AnsiB,
                orientation: DrawingSheetOrientation::Landscape,
                drawing_number: "ASSEMBLY-SECTION".into(),
                revision: "B".into(),
                sheet_number: i as u32 + 1,
                sheet_count: 1000,
                projection: Some(if i % 2 == 0 {
                    ProjectionConvention::FirstAngle
                } else {
                    ProjectionConvention::ThirdAngle
                }),
            });
            page
        })
        .collect();
    let started = Instant::now();
    let generated = DrawingDefinition::generate_many(
        &sheets,
        &graph,
        &session,
        DrawingRenderOptions {
            curve_samples: 8,
            maximum_vertices: 1_000_000,
        },
    )
    .unwrap();
    assert_eq!(generated.len(), 1000);
    for (i, page) in generated.iter().enumerate() {
        assert_eq!(page.generated_variants, 1);
        assert_eq!(page.paper_size_mm, [431.8, 279.4]);
        assert!(
            page.sheet_labels
                .iter()
                .any(|l| l.text == format!("SHEET {} OF 1000", i + 1))
        );
        assert!(page.to_svg().contains("ASSEMBLY-SECTION"));
        assert!(page.to_dxf().contains("SCALE: 1:1"));
    }
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "1000 standard sheets with projection symbols and SVG/DXF exports: {:?} (10s budget), one shared variant",
        started.elapsed()
    );
    let mut gdt = template.clone();
    for (id, label, anchor, size) in [
        ("primary", "A", "center", false),
        ("secondary", "B", "x", true),
        ("tertiary", "C", "y", true),
    ] {
        gdt.datum_features.push(DrawingDatumFeature {
            id: id.into(),
            label: label.into(),
            feature_of_size: size,
            attachment: DrawingGdtAttachment {
                view: "profile".into(),
                output: InstanceOutputRef {
                    instance: "part".into(),
                    output: "body".into(),
                },
                anchor: DatumRef::new("part", anchor),
                offset_mm: [20.0, 30.0],
            },
        });
    }
    gdt.feature_control_frames = (0..10_000)
        .map(|i| DrawingFeatureControlFrame {
            id: format!("position-{i}"),
            attachment: DrawingGdtAttachment {
                view: "profile".into(),
                output: InstanceOutputRef {
                    instance: "part".into(),
                    output: "body".into(),
                },
                anchor: DatumRef::new("part", "center"),
                offset_mm: [30.0, 40.0],
            },
            characteristic: GeometricCharacteristic::Position,
            tolerance: Quantity::length(0.1, LengthUnit::Millimeter),
            display_unit: LengthUnit::Millimeter,
            precision: 2,
            zone: GeometricToleranceZone::Diameter,
            material: ToleranceMaterialCondition::Maximum,
            feature_of_size: true,
            datums: vec![
                DrawingDatumReference {
                    datum_feature: "primary".into(),
                    boundary: DatumMaterialBoundary::Regardless,
                },
                DrawingDatumReference {
                    datum_feature: "secondary".into(),
                    boundary: DatumMaterialBoundary::Maximum,
                },
                DrawingDatumReference {
                    datum_feature: "tertiary".into(),
                    boundary: DatumMaterialBoundary::Least,
                },
            ],
        })
        .collect();
    let started = Instant::now();
    let generated = gdt
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_samples: 8,
                maximum_vertices: 2_000_000,
            },
        )
        .unwrap();
    assert_eq!(generated.generated_variants, 1);
    assert_eq!(
        generated
            .gdt_labels
            .iter()
            .filter(|l| l.text == "0.10 mm")
            .count(),
        10_000
    );
    assert_eq!(
        generated
            .gdt_lines
            .iter()
            .map(|l| l.points_mm.len())
            .sum::<usize>(),
        1_890_033
    );
    assert!(generated.to_svg().contains("0.10 mm"));
    assert!(generated.to_dxf().contains("0.10 mm"));
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "10000 structured GD&T frames with datum/material modifiers and SVG/DXF exports: {:?} (10s budget), one shared variant",
        started.elapsed()
    );
}
