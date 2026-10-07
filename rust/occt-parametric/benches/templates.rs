use occt_bridge::Session;
use occt_parametric::*;
use std::{
    collections::{BTreeMap, HashMap},
    time::Instant,
};

fn fixture() -> (ModelDocument, DrawingDefinition) {
    let mut family = FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
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
        datum_reference_frames: Vec::new(),
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
            material_hatching: Default::default(),
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
    exact_curve_export(&document, &template);
    exact_spline_details(&document, &template);
    material_hatch_case(&document, &template);
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
                curve_tolerance_mm: 0.01,
                exact_curves: false,
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
            curve_tolerance_mm: 0.01,
            exact_curves: false,
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
                curve_tolerance_mm: 0.01,
                exact_curves: false,
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
                curve_tolerance_mm: 0.01,
                exact_curves: false,
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
    for exact_curves in [false, true] {
        let started = Instant::now();
        let generated = hatched
            .generate(
                &assembly,
                &session,
                DrawingRenderOptions {
                    curve_tolerance_mm: 0.01,
                    exact_curves,
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
            "10000 section hatch segments across 1000 placed parts (exact={exact_curves}): {:?} (30s budget), one shared variant",
            started.elapsed()
        );
    }
    exact_curved_hatches(&document, &template);
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
            curve_tolerance_mm: 0.01,
            exact_curves: false,
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
            size_limits: None,
            datum_reference_frame: None,
            refinement: None,
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
                curve_tolerance_mm: 0.01,
                exact_curves: false,
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
    let references = gdt.feature_control_frames[0].datums.clone();
    gdt.datum_reference_frames.push(DrawingDatumReferenceFrame {
        id: "ABC".into(),
        datums: references.clone(),
    });
    for f in &mut gdt.feature_control_frames {
        f.datums.clear();
        f.datum_reference_frame = Some("ABC".into());
        f.refinement = Some(DrawingCompositeRefinement {
            tolerance: Quantity::length(0.05, LengthUnit::Millimeter),
            datums: vec![references[0].clone()],
        });
    }
    let started = Instant::now();
    let generated = gdt
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_tolerance_mm: 0.01,
                exact_curves: false,
                curve_samples: 8,
                maximum_vertices: 3_000_000,
            },
        )
        .unwrap();
    assert_eq!(generated.generated_variants, 1);
    assert_eq!(
        generated
            .gdt_lines
            .iter()
            .map(|l| l.points_mm.len())
            .sum::<usize>(),
        2_630_033
    );
    assert_eq!(
        generated
            .gdt_labels
            .iter()
            .filter(|l| l.text == "0.05 mm")
            .count(),
        10_000
    );
    assert!(generated.to_svg().contains("0.05 mm"));
    assert!(generated.to_dxf().contains("0.05 mm"));
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "10000 composite GD&T frames with a shared named datum frame and SVG/DXF exports: {:?} (10s budget), one shared variant",
        started.elapsed()
    );

    let mut planar_family = document.family.clone();
    for (datum, normal) in
        planar_family
            .datums
            .iter_mut()
            .zip([[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
    {
        let DatumKind::Point { origin } = datum.kind.clone() else {
            panic!("point fixture")
        };
        datum.kind = DatumKind::Plane {
            origin,
            normal: VectorExpr::Literal(VectorQuantity::scalars(normal[0], normal[1], normal[2])),
        };
    }
    let mut planar = InstanceGraph::new(&planar_family);
    planar.add_base("part", HashMap::new(), "test").unwrap();
    let mut datum_page = template.clone();
    datum_page.datum_features = gdt.datum_features.clone();
    datum_page.datum_reference_frames = (0..10_000)
        .map(|i| DrawingDatumReferenceFrame {
            id: format!("frame-{i}"),
            datums: references
                .iter()
                .map(|r| DrawingDatumReference {
                    datum_feature: r.datum_feature.clone(),
                    boundary: DatumMaterialBoundary::Regardless,
                })
                .collect(),
        })
        .collect();
    let started = Instant::now();
    let resolved = datum_page.resolve_datum_reference_frames(&planar).unwrap();
    assert_eq!(resolved.len(), 10_000);
    for frame in resolved {
        let nominal = frame.nominal_planar_321().unwrap();
        assert_eq!(nominal.origin_mm, occt_bridge::Vec3::new(5.0, 5.0, 15.0));
        assert_eq!(
            nominal
                .coordinates_mm(occt_bridge::Vec3::new(7.0, 8.0, 19.0))
                .unwrap(),
            occt_bridge::Vec3::new(2.0, 3.0, 4.0)
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "10000 named datum frames resolved and nominal planar 3-2-1 coordinates: {:?} (10s budget), no kernel handles",
        started.elapsed()
    );
    let mut control = gdt.feature_control_frames[0].clone();
    control.size_limits = Some(DrawingSizeLimits {
        kind: FeatureOfSizeKind::Internal,
        lower: Quantity::length(10.0, LengthUnit::Millimeter),
        upper: Quantity::length(12.0, LengthUnit::Millimeter),
    });
    let started = Instant::now();
    for _ in 0..10_000 {
        let result = control
            .tolerance_allowance(Quantity::length(11.0, LengthUnit::Millimeter))
            .unwrap();
        assert_eq!(result.bonus_mm, 1.0);
        assert_eq!(result.total_tolerance_mm, 1.1);
        assert_eq!(result.refinement_total_tolerance_mm, Some(1.05));
    }
    assert!(started.elapsed().as_secs_f64() < 10.0);
    assert_eq!(session.shape_count().unwrap(), 0);
    println!(
        "10000 composite feature-size allowances: {:?} (10s budget), no kernel handles",
        started.elapsed()
    );
    control.refinement = None;
    control.datum_reference_frame = None;
    control.datums = references;
    for reference in &mut control.datums {
        reference.boundary = DatumMaterialBoundary::Regardless;
    }
    let nominal_axis = PositionToleranceAxis {
        origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
    };
    let samples: Vec<_> = (0..100_000)
        .map(|i| VectorQuantity::lengths(0.03, 0.04, i as f64, LengthUnit::Millimeter))
        .collect();
    let started = Instant::now();
    let result = control
        .evaluate_position_samples(
            Quantity::length(10.0, LengthUnit::Millimeter),
            nominal_axis,
            &samples,
        )
        .unwrap();
    assert_eq!(result.sample_count, 100_000);
    assert_eq!(result.worst_sample_index, 0);
    assert_eq!(result.required_zone_diameter_mm, 0.1);
    assert!(result.samples_within_zone);
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "100000 fixed cylindrical position samples: {:?} (10s budget), no kernel handles",
        started.elapsed()
    );
    let mut measured_page = template.clone();
    measured_page.dimensions = (0..10_000)
        .map(|i| DrawingDimension {
            id: format!("width-{i}"),
            view: "profile".into(),
            first: DatumRef::new("part", "center"),
            second: DatumRef::new("part", "x"),
            direction: DimensionDirection::Horizontal,
            offset_mm: 10.0,
            precision: 3,
            presentation: DimensionPresentation {
                tolerance: DimensionTolerance::Symmetric {
                    deviation: Quantity::length(0.125, LengthUnit::Millimeter),
                },
                ..Default::default()
            },
        })
        .collect();
    let measurements: Vec<_> = (0..100_000)
        .map(|i| DrawingDimensionMeasurement {
            dimension: format!("width-{}", i % 10_000),
            value: Quantity::length(5.0 + (i % 3) as f64 * 0.125 - 0.125, LengthUnit::Millimeter),
        })
        .collect();
    let started = Instant::now();
    let results = measured_page
        .evaluate_dimension_measurements(&graph, &measurements)
        .unwrap();
    assert_eq!(results.len(), 100_000);
    for (input, result) in measurements.iter().zip(results.iter()) {
        assert_eq!(result.dimension, input.dimension);
        assert_eq!(result.evaluation.nominal.value, 5.0);
        assert_eq!(
            result.evaluation.disposition,
            DimensionMeasurementDisposition::WithinLimits
        );
    }
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "100000 dimensional measurements across 10000 saved dimensions: {:?} (10s budget), no kernel handles",
        started.elapsed()
    );
    for frame in &mut gdt.feature_control_frames {
        frame.refinement = None;
        frame.size_limits = Some(DrawingSizeLimits {
            kind: FeatureOfSizeKind::Internal,
            lower: Quantity::length(10.0, LengthUnit::Millimeter),
            upper: Quantity::length(12.0, LengthUnit::Millimeter),
        });
    }
    for reference in &mut gdt.datum_reference_frames[0].datums {
        reference.boundary = DatumMaterialBoundary::Regardless;
    }
    let positions: Vec<_> = (0..100_000)
        .map(|i| DrawingPositionMeasurement {
            control: format!("position-{}", i % 10_000),
            size: Quantity::length(10.0, LengthUnit::Millimeter),
            nominal_axis,
            samples: vec![VectorQuantity::lengths(
                0.03,
                0.04,
                5.0,
                LengthUnit::Millimeter,
            )],
        })
        .collect();
    let started = Instant::now();
    let results = gdt
        .evaluate_position_measurements(&graph, &positions)
        .unwrap();
    assert_eq!(results.len(), 100_000);
    for (input, result) in positions.iter().zip(&results) {
        assert_eq!(result.control, input.control);
        assert!(result.evaluation.samples_within_zone);
        assert_eq!(result.evaluation.required_zone_diameter_mm, 0.1);
    }
    assert!(started.elapsed().as_secs_f64() < 10.0);
    assert_eq!(session.shape_count().unwrap(), 0);
    println!(
        "100000 position measurements across 10000 named-frame controls: {:?} (10s budget), no kernel handles",
        started.elapsed()
    );
}

fn exact_curve_export(document: &ModelDocument, template: &DrawingDefinition) {
    let session = Session::new().unwrap();
    let graph = document.instance_graph().unwrap();
    let definitions: Vec<_> = (0..2500)
        .map(|i| {
            let mut d = template.clone();
            d.id = format!("analytic-{i}");
            d
        })
        .collect();
    let started = Instant::now();
    let drawings = DrawingDefinition::generate_many(
        &definitions,
        &graph,
        &session,
        DrawingRenderOptions {
            curve_tolerance_mm: 0.01,
            exact_curves: true,
            curve_samples: 100_000,
            maximum_vertices: 42_500,
        },
    )
    .unwrap();
    assert_eq!(
        drawings.iter().map(|d| d.curves.len()).sum::<usize>(),
        10_000
    );
    for drawing in &drawings {
        assert!(drawing.polylines.is_empty());
        assert_eq!(drawing.generated_variants, 1);
        assert_eq!(drawing.to_dxf().matches("0\nLINE\n").count(), 4);
        assert_eq!(drawing.to_svg().matches("<path ").count(), 4);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "10000 exact drawing lines, both exports and exact budget: {:?} (10s budget), one shared variant",
        started.elapsed()
    );
}

fn exact_spline_details(document: &ModelDocument, template: &DrawingDefinition) {
    let mut family = document.family.clone();
    let profile: Vec<_> = (0..12)
        .map(|i| {
            let angle = std::f64::consts::TAU * i as f64 / 12.0;
            [5.0 + 5.0 * angle.cos(), 5.0 + 5.0 * angle.sin()]
        })
        .collect();
    let section = |z| LoftSection {
        profile: profile.clone(),
        origin: VectorExpr::Literal(VectorQuantity::lengths(0.0, 0.0, z, LengthUnit::Millimeter)),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        scale: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
        rotation_radians: None,
        pivot: [0.0, 0.0],
    };
    family.features = vec![FeatureDefinition {
        id: "body".into(),
        operation: FeatureOperation::Loft {
            sections: vec![section(0.0), section(30.0)],
            smooth: true,
            ruled: true,
        },
    }];
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "bench").unwrap();
    let session = Session::new().unwrap();
    for detail in [
        None,
        Some(DrawingDetail {
            minimum_mm: [2.0, -10.0],
            maximum_mm: [6.0, 20.0],
        }),
    ] {
        let definitions: Vec<_> = (0..1000)
            .map(|i| {
                let mut d = template.clone();
                d.id = format!("spline-{i}");
                d.views[0].detail = detail;
                d
            })
            .collect();
        let started = Instant::now();
        let generated = DrawingDefinition::generate_many(
            &definitions,
            &graph,
            &session,
            DrawingRenderOptions {
                exact_curves: true,
                ..DrawingRenderOptions::default()
            },
        )
        .unwrap();
        let mut spans = 0;
        for d in &generated {
            assert_eq!(d.generated_variants, 1);
            assert!(d.polylines.is_empty());
            assert!(
                d.curves
                    .iter()
                    .all(|c| matches!(c.geometry, DrawingCurveGeometry::Bezier { .. }))
            );
            assert_eq!(d.to_dxf().matches("0\nSPLINE\n").count(), d.curves.len());
            assert_eq!(d.to_svg().matches("<path ").count(), d.curves.len());
            spans += d.curves.len();
        }
        assert!(spans >= if detail.is_none() { 10_000 } else { 2000 });
        assert_eq!(session.shape_count().unwrap(), 0);
        assert!(
            started.elapsed().as_secs_f64() < 30.0,
            "freeform view workload took {:?}",
            started.elapsed()
        );
        println!(
            "1000 exact spline views (detail={}): {spans} Bezier spans and both exports in {:?} (30s budget), one shared variant",
            detail.is_some(),
            started.elapsed()
        );
    }
}

fn exact_curved_hatches(document: &ModelDocument, template: &DrawingDefinition) {
    let mut family = document.family.clone();
    family.features = vec![FeatureDefinition {
        id: "body".into(),
        operation: FeatureOperation::Cylinder {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            radius: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
            height: ScalarExpr::Literal(Quantity::length(30.0, LengthUnit::Millimeter)),
        },
    }];
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "bench").unwrap();
    let mut page = template.clone();
    page.views[0].outputs.clear();
    page.views[0].hatching = Some(SectionHatching {
        angle_radians: 0.0,
        spacing_mm: 0.25,
        phase_mm: 0.125,
    });
    for i in 0..1000 {
        let id = if i == 0 {
            "part".into()
        } else {
            format!("round-{i}")
        };
        if i > 0 {
            graph
                .add_clone(&id, "part", HashMap::new(), "bench")
                .unwrap();
            graph
                .set_placement(
                    &id,
                    Placement::translated(VectorQuantity::lengths(
                        15.0 * i as f64,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                )
                .unwrap();
        }
        page.views[0].outputs.push(InstanceOutputRef {
            instance: id,
            output: "body".into(),
        });
    }
    let session = Session::new().unwrap();
    let started = Instant::now();
    let generated = page
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                exact_curves: true,
                curve_samples: 2,
                ..DrawingRenderOptions::default()
            },
        )
        .unwrap();
    assert_eq!(generated.hatches.len(), 40_000);
    assert_eq!(generated.generated_variants, 1);
    for line in &generated.hatches {
        for p in &line.points_mm {
            let member = ((p[0] - 20.0) / 15.0).round();
            assert!(((p[0] - 20.0 - member * 15.0).hypot(p[1] - 40.0) - 5.0).abs() < 1e-7);
        }
    }
    assert!(generated.to_svg().contains("stroke-width=\"0.13\""));
    assert!(generated.to_dxf().contains("SECTION_HATCH"));
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(
        started.elapsed().as_secs_f64() < 30.0,
        "curved hatch workload took {:?}",
        started.elapsed()
    );
    println!(
        "40000 exact curved-boundary hatch segments across 1000 cylinders and both exports: {:?} (30s budget), one shared variant",
        started.elapsed()
    );
}

fn material_hatch_case(document: &ModelDocument, template: &DrawingDefinition) {
    let mut source = document.clone();
    source.assembly.materials = (0..10_000)
        .map(|i| Material {
            id: format!("m{i}"),
            name: format!("Material {i}"),
            density_kg_per_cubic_meter: 1000.0,
        })
        .collect();
    let mut graph = source.instance_graph().unwrap();
    graph.assign_material("part", Some("m9999")).unwrap();
    let mut page = template.clone();
    page.views[0].outputs.clear();
    page.views[0].material_hatching = graph
        .assembly()
        .materials
        .iter()
        .map(|m| (m.id.clone(), vec![]))
        .collect();
    page.views[0].material_hatching.insert(
        "m9999".into(),
        vec![
            SectionHatching {
                angle_radians: 0.0,
                spacing_mm: 2.0,
                phase_mm: 0.5,
            },
            SectionHatching {
                angle_radians: 0.0,
                spacing_mm: 2.0,
                phase_mm: 1.5,
            },
        ],
    );
    page.views[0].material_hatching.insert(
        "m9998".into(),
        vec![SectionHatching {
            angle_radians: std::f64::consts::FRAC_PI_2,
            spacing_mm: 2.0,
            phase_mm: 0.5,
        }],
    );
    for i in 0..1000 {
        let id = if i == 0 {
            "part".into()
        } else {
            format!("material-part-{i}")
        };
        if i > 0 {
            graph
                .add_clone(&id, "part", HashMap::new(), "bench")
                .unwrap();
            graph
                .set_placement(
                    &id,
                    Placement::translated(VectorQuantity::lengths(
                        i as f64 * 15.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                )
                .unwrap();
        }
        if i % 2 == 1 {
            graph.assign_material(&id, Some("m9998")).unwrap();
        }
        page.views[0].outputs.push(InstanceOutputRef {
            instance: id,
            output: "body".into(),
        });
    }
    let session = Session::new().unwrap();
    let started = Instant::now();
    let generated = page
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                exact_curves: true,
                ..DrawingRenderOptions::default()
            },
        )
        .unwrap();
    assert_eq!(generated.generated_variants, 1);
    assert_eq!(generated.hatches.len(), 12_500);
    let vertical = generated
        .hatches
        .iter()
        .filter(|l| (l.points_mm[0][0] - l.points_mm[1][0]).abs() < 1e-7)
        .count();
    assert_eq!(vertical, 2500);
    assert!(generated.to_svg().contains("stroke-width=\"0.13\""));
    assert!(generated.to_dxf().contains("SECTION_HATCH"));
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(
        started.elapsed().as_secs_f64() < 30.0,
        "material hatch workload took {:?}",
        started.elapsed()
    );
    println!(
        "1000 material-hatched parts, 10000 material mappings, 12500 hatch segments and both exports: {:?} (30s budget), one shared variant",
        started.elapsed()
    );
}
