use super::*;

mod dimensions;
mod exact_geometry;
mod gdt;
mod guides;
mod hatching;
mod hole_callouts;
mod material_hatching;
mod sheets;
mod slices;
mod views;

fn family_with_datums() -> FamilyDefinition {
    let mut definition = family(RequirementPriority::Required, 100000.0);
    definition.requirements.clear();
    definition.datums = vec![
        DatumDefinition {
            id: "origin".into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
        },
        DatumDefinition {
            id: "corner".into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Components {
                    x: ScalarExpr::Parameter("width".into()),
                    y: ScalarExpr::Parameter("depth".into()),
                    z: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
                },
            },
        },
    ];
    definition
}
fn drawing() -> DrawingDefinition {
    DrawingDefinition {
        datum_reference_frames: Vec::new(),
        datum_features: Vec::new(),
        feature_control_frames: Vec::new(),
        surface_textures: Vec::new(),
        parts_list: None,
        balloons: Vec::new(),
        releases: Vec::new(),
        revision_table: None,
        sheet: None,
        id: "drawing".into(),
        title: "Block <assembly> & drawing".into(),
        paper_size_mm: [297.0, 210.0],
        views: vec![DrawingView {
            id: "top".into(),
            outputs: vec![InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            }],
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [40.0, 100.0],
            scale: 3.0,
            show_hidden: true,
            kind: DrawingViewKind::Orthographic,
            detail: None,
            material_hatching: Default::default(),
            hatching: None,
        }],
        guides: Vec::new(),
        dimensions: vec![DrawingDimension {
            id: "width".into(),
            view: "top".into(),
            first: DatumRef::new("part", "origin"),
            second: DatumRef::new("part", "corner"),
            direction: DimensionDirection::Horizontal,
            presentation: DimensionPresentation::default(),
            offset_mm: -10.0,
            precision: 1,
        }],
        notes: vec![DrawingNote {
            id: "width-note".into(),
            position_mm: [100.0, 150.0],
            text: DrawingText::Parameter {
                instance: "part".into(),
                parameter: "width".into(),
                prefix: "Width: ".into(),
                suffix: " mm".into(),
                precision: 1,
            },
        }],
        metadata: BTreeMap::from([
            ("Revision".into(), "A".into()),
            ("Author".into(), "Engineering".into()),
        ]),
    }
}

fn slice_definition(
    output: &str,
    origin: VectorQuantity,
    direction: VectorQuantity,
) -> DrawingDefinition {
    DrawingDefinition {
        datum_reference_frames: Vec::new(),
        datum_features: Vec::new(),
        feature_control_frames: Vec::new(),
        surface_textures: Vec::new(),
        parts_list: None,
        balloons: Vec::new(),
        releases: Vec::new(),
        revision_table: None,
        sheet: None,
        id: "cutting-template".into(),
        title: "True plane profile".into(),
        paper_size_mm: [1000.0, 1000.0],
        views: vec![DrawingView {
            id: "slice".into(),
            outputs: vec![InstanceOutputRef {
                instance: "part".into(),
                output: output.into(),
            }],
            origin,
            direction,
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [0.0, 0.0],
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
    }
}

fn annotated_family() -> FamilyDefinition {
    let mut definition = family_with_datums();
    for (id, x, y) in [("x", 5.0, 0.0), ("y", 0.0, 5.0)] {
        definition.datums.push(DatumDefinition {
            id: id.into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    x,
                    y,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            },
        });
    }
    definition
}

fn manufactured_dimension(
    direction: DimensionDirection,
    tolerance: DimensionTolerance,
) -> DrawingDimension {
    DrawingDimension {
        id: "manufactured".into(),
        view: "top".into(),
        first: DatumRef::new("part", "origin"),
        second: DatumRef::new("part", "x"),
        direction,
        offset_mm: 10.0,
        precision: 3,
        presentation: DimensionPresentation {
            tolerance,
            ..Default::default()
        },
    }
}

fn callout_family(finish: HoleFinish, extent: HoleExtent) -> FamilyDefinition {
    let mut definition = annotated_family();
    definition.parameters.push(length_parameter("bore", 4.0));
    definition.features.push(FeatureDefinition {
        id: "hole".into(),
        operation: FeatureOperation::Hole {
            bottom: HoleBottom::Flat,
            input: "body".into(),
            position: VectorExpr::Literal(VectorQuantity::lengths(
                5.0,
                5.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            diameter: ScalarExpr::Parameter("bore".into()),
            extent,
            finish,
            thread: Some(Box::new(ThreadSpecification {
                designation: "M5x0.8".into(),
                nominal_diameter: ScalarExpr::Literal(Quantity::length(
                    5.0,
                    LengthUnit::Millimeter,
                )),
                pitch: ScalarExpr::Literal(Quantity::length(0.8, LengthUnit::Millimeter)),
                handedness: ThreadHandedness::Left,
            })),
        },
    });
    definition
}

fn hatched_slice(output: &str) -> DrawingDefinition {
    let mut page = slice_definition(
        output,
        VectorQuantity::lengths(0.0, 0.0, 5.0, LengthUnit::Millimeter),
        VectorQuantity::scalars(0.0, 0.0, 1.0),
    );
    page.views[0].hatching = Some(SectionHatching {
        angle_radians: 0.0,
        spacing_mm: 1.0,
        phase_mm: 0.5,
    });
    page
}

fn standard_sheet(
    size: DrawingSheetSize,
    projection: Option<ProjectionConvention>,
) -> DrawingSheet {
    DrawingSheet {
        size,
        orientation: DrawingSheetOrientation::Landscape,
        drawing_number: "BRACKET-001".into(),
        revision: "A".into(),
        sheet_number: 1,
        sheet_count: 2,
        projection,
    }
}
