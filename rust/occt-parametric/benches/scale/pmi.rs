//! Semantic AP242 PMI at scale: drawing GD&T on a 400-hole plate written on
//! persistent faces, drawn in two saved annotation views, and verified by
//! reading the file back.

use super::*;
use occt_parametric::{
    AnnotationClippingPlane, CoordinateAxis, DatasetAuthority, DatumMaterialBoundary,
    DrawingAnnotationView, DrawingDataset, DrawingDatumFeature, DrawingDatumReference,
    DrawingDefinition, DrawingFeatureControlFrame, DrawingGdtAttachment, DrawingSizeLimits,
    DrawingView, DrawingViewKind, Extremum, FeatureOfSizeKind, GeometricCharacteristic,
    GeometricToleranceZone, InstanceOutputRef, NamedReference, OutputSet, PmiItemKind,
    ReferenceTarget, ToleranceMaterialCondition,
};
use std::collections::BTreeMap;

fn mm(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

fn quantity(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}

const SIDE: usize = 20;

/// A 410 × 410 × 10 plate cut by a 20 × 20 grid of Ø8 holes at 20 mm
/// pitch, with datum planes, a reference per hole wall and one per datum face.
fn drilled(definition: &FamilyDefinition) -> FamilyDefinition {
    let mut definition = definition.clone();
    let feature = |id: &str, operation| FeatureDefinition {
        id: id.into(),
        operation,
    };
    definition.features = vec![
        feature(
            "blank",
            FeatureOperation::Box {
                origin: mm(0.0, 0.0, 0.0),
                size: mm(410.0, 410.0, 10.0),
            },
        ),
        feature(
            "pin",
            FeatureOperation::Cylinder {
                origin: mm(15.0, 15.0, -1.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: length(4.0),
                height: length(12.0),
            },
        ),
        feature(
            "row",
            FeatureOperation::LinearPattern {
                input: "pin".into(),
                step: mm(20.0, 0.0, 0.0),
                count: ScalarExpr::Literal(Quantity::scalar(SIDE as f64)),
            },
        ),
        feature(
            "grid",
            FeatureOperation::LinearPattern {
                input: "row".into(),
                step: mm(0.0, 20.0, 0.0),
                count: ScalarExpr::Literal(Quantity::scalar(SIDE as f64)),
            },
        ),
        feature(
            "body",
            FeatureOperation::Cut {
                object: "blank".into(),
                tool: "grid".into(),
            },
        ),
    ];
    let plane = |id: &str, normal: [f64; 3]| DatumDefinition {
        id: id.into(),
        kind: DatumKind::Plane {
            origin: mm(0.0, 0.0, 0.0),
            normal: VectorExpr::Literal(VectorQuantity::scalars(normal[0], normal[1], normal[2])),
        },
    };
    definition.datums = vec![
        plane("bottom", [0.0, 0.0, -1.0]),
        plane("left", [-1.0, 0.0, 0.0]),
        plane("front", [0.0, -1.0, 0.0]),
    ];
    let extreme = |name: &str, axis, extremum| NamedReference {
        name: name.into(),
        target: ReferenceTarget::Faces(FaceSelector::AtExtreme {
            axis,
            extremum,
            tolerance: length(1e-6),
        }),
    };
    definition.references = vec![
        extreme("bottom_face", CoordinateAxis::Z, Extremum::Minimum),
        extreme("top_face", CoordinateAxis::Z, Extremum::Maximum),
        extreme("left_face", CoordinateAxis::X, Extremum::Minimum),
        extreme("front_face", CoordinateAxis::Y, Extremum::Minimum),
    ];
    for index in 0..SIDE * SIDE {
        let (x, y) = (
            15.0 + 20.0 * (index % SIDE) as f64,
            15.0 + 20.0 * (index / SIDE) as f64,
        );
        definition.references.push(NamedReference {
            name: format!("hole_{index}"),
            target: ReferenceTarget::Faces(FaceSelector::NearestCenter {
                target: mm(x, y, 5.0),
                maximum_distance: length(1.0),
            }),
        });
    }
    definition
}

fn attachment(anchor: &str, faces: &str) -> DrawingGdtAttachment {
    DrawingGdtAttachment {
        view: "top".into(),
        output: InstanceOutputRef {
            instance: "part".into(),
            output: "body".into(),
        },
        anchor: DatumRef::new("part", anchor),
        offset_mm: [20.0, 20.0],
        faces: Some(FaceSelector::Named(faces.into())),
    }
}

/// Datums A, B, C, a flat top and a position control with size limits on
/// every hole: one control each, or one 400× pattern. A top view shows them
/// all and an isometric section view the datums and the flat top.
fn drawing(pattern: bool) -> DrawingDefinition {
    let datum = |id: &str, anchor: &str, faces: &str| DrawingDatumFeature {
        id: id.into(),
        label: id.into(),
        feature_of_size: false,
        attachment: attachment(anchor, faces),
        size_kind: None,
        size_limits: None,
    };
    let reference = |id: &str| DrawingDatumReference {
        datum_feature: id.into(),
        boundary: DatumMaterialBoundary::Regardless,
        common_with: Vec::new(),
        translation: false,
    };
    let control =
        |id: String, faces: &str, characteristic, datums: &[&str]| DrawingFeatureControlFrame {
            size_limits: None,
            datum_reference_frame: None,
            refinement: None,
            pattern: None,
            unequal: None,
            surface: None,
            projected: None,
            tangent_plane: false,
            modifiers: Vec::new(),
            id,
            attachment: attachment("bottom", faces),
            characteristic,
            tolerance: quantity(0.2),
            display_unit: LengthUnit::Millimeter,
            precision: 2,
            zone: GeometricToleranceZone::Characteristic,
            material: ToleranceMaterialCondition::Regardless,
            feature_of_size: false,
            datums: datums.iter().map(|d| reference(d)).collect(),
        };
    let mut frames = vec![control(
        "flat".into(),
        "top_face",
        GeometricCharacteristic::Flatness,
        &[],
    )];
    for index in 0..SIDE * SIDE {
        let mut position = control(
            format!("position_{index}"),
            &format!("hole_{index}"),
            GeometricCharacteristic::Position,
            &["A", "B", "C"],
        );
        position.zone = GeometricToleranceZone::Diameter;
        position.material = ToleranceMaterialCondition::Maximum;
        position.feature_of_size = true;
        position.size_limits = Some(DrawingSizeLimits {
            kind: FeatureOfSizeKind::Internal,
            lower: quantity(7.9),
            upper: quantity(8.1),
        });
        if pattern {
            position.pattern = Some("holes".into());
        }
        frames.push(position);
    }
    let view = |id: &str, direction: [f64; 3], x_axis: [f64; 3], frames: Vec<String>| {
        DrawingAnnotationView {
            id: id.into(),
            name: id.to_uppercase(),
            direction: VectorQuantity::scalars(direction[0], direction[1], direction[2]),
            x_axis: VectorQuantity::scalars(x_axis[0], x_axis[1], x_axis[2]),
            scale: 1.0,
            datum_features: vec!["A".into(), "B".into(), "C".into()],
            feature_control_frames: frames,
            clipping_planes: Vec::new(),
        }
    };
    let mut annotation_views = vec![
        view(
            "top",
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            frames.iter().map(|f| f.id.clone()).collect(),
        ),
        view(
            "iso",
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 0.0],
            vec!["flat".into()],
        ),
    ];
    // The isometric view is a section through the middle row of holes.
    annotation_views[1].clipping_planes = vec![AnnotationClippingPlane {
        origin: VectorQuantity::lengths(0.0, 205.0, 0.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(0.0, 1.0, 0.0),
        keep_positive: true,
    }];
    DrawingDefinition {
        annotation_views,
        dataset: Some(DrawingDataset {
            identifier: "PLATE-400".into(),
            organization: "Bench".into(),
            authority: DatasetAuthority::Model,
        }),
        datum_reference_frames: Vec::new(),
        datum_features: vec![
            datum("A", "bottom", "bottom_face"),
            datum("B", "left", "left_face"),
            datum("C", "front", "front_face"),
        ],
        feature_control_frames: frames,
        surface_textures: Vec::new(),
        parts_list: None,
        balloons: Vec::new(),
        releases: Vec::new(),
        revision_table: None,
        sheet: None,
        id: "plate".into(),
        title: "Plate".into(),
        paper_size_mm: [841.0, 594.0],
        views: vec![DrawingView {
            id: "top".into(),
            outputs: vec![InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            }],
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [20.0, 20.0],
            scale: 1.0,
            show_hidden: false,
            kind: DrawingViewKind::Orthographic,
            detail: None,
            hatching: None,
            material_hatching: BTreeMap::new(),
            thread_representation: false,
        }],
        dimensions: Vec::new(),
        guides: Vec::new(),
        notes: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

/// The plate's GD&T written as AP242 PMI with its presentations and saved
/// views, and read back: 400 position controls and sizes on their own hole
/// walls, or one 400× pattern control and one size over all of them.
pub(crate) fn pmi_case(definition: &FamilyDefinition, pattern: bool) -> Outcome {
    let holes = SIDE * SIDE;
    let name = if pattern {
        format!("STEP PMI: one {holes}× position on a {holes}-hole plate, read back")
    } else {
        format!("STEP PMI: {holes} position controls and sizes on a {holes}-hole plate, read back")
    };
    // Measured about 1.9 s for either: two regenerations of the plate (the
    // graph's and the local one its selectors run on) take 1.2 s, and the
    // AP242 write and read-back 0.5 s.
    timed(name, ms(6_000), Expectation::Required, || {
        let definition = drilled(definition);
        let session = Session::new().map_err(|error| failure(error.to_string()))?;
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "bench")?;
        let generation = graph.regenerate_all(&session)?;
        let page = drawing(pattern);
        let path = std::env::temp_dir().join(format!(
            "occb-bench-pmi-{}-{pattern}.step",
            std::process::id()
        ));
        let start = Instant::now();
        let report = page.export_step_pmi(
            &graph,
            &session,
            &generation,
            &path,
            "plate",
            &OutputSet::AllWithOutput("body".into()),
        )?;
        let export = start.elapsed();
        std::fs::remove_file(&path).map_err(|error| failure(error.to_string()))?;
        let tolerances = report
            .items
            .iter()
            .filter(|i| i.kind == PmiItemKind::Tolerance)
            .count();
        let sizes = report
            .items
            .iter()
            .filter(|i| i.kind == PmiItemKind::Size)
            .count();
        let expected = if pattern { (2, 1) } else { (holes + 1, holes) };
        let shown = report
            .views
            .iter()
            .map(|v| v.annotations.len())
            .sum::<usize>();
        if !report.complete() || (tolerances, sizes) != expected || shown != holes + 8 {
            return Err(failure(format!(
                "{tolerances} tolerances, {sizes} sizes, {shown} shown, complete {}",
                report.complete()
            )));
        }
        Ok(format!(
            "{tolerances} tolerances, {sizes} sizes, 3 datums, 2 views and the dataset verified; export and read-back {:.3} s",
            export.as_secs_f64()
        ))
    })
}
