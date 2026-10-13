//! Drawing GD&T exported as semantic AP242 PMI on persistent faces.

use super::inspection::{attachment, control, page};
use super::*;

fn at(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn mm(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

/// The plate's face at one end of an axis.
fn facing(name: &str, axis: CoordinateAxis, extremum: Extremum) -> NamedReference {
    NamedReference {
        name: name.into(),
        target: ReferenceTarget::Faces(FaceSelector::AtExtreme {
            axis,
            extremum,
            tolerance: mm(1e-6),
        }),
    }
}

/// A 60 × 40 × 10 mm plate with Ø10 bores at x = 20 and 40, and a named
/// reference for each face.
fn drilled_plate() -> FamilyDefinition {
    let mut definition = super::inspection::plate();
    let bore = |id: &str, x: f64| FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::Cylinder {
            origin: at(x, 20.0, -1.0),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            radius: mm(5.0),
            height: mm(12.0),
        },
    };
    definition.features = vec![
        FeatureDefinition {
            id: "block".into(),
            operation: FeatureOperation::Box {
                origin: at(0.0, 0.0, 0.0),
                size: at(60.0, 40.0, 10.0),
            },
        },
        bore("first_bore", 20.0),
        bore("second_bore", 40.0),
        FeatureDefinition {
            id: "drilled".into(),
            operation: FeatureOperation::Cut {
                object: "block".into(),
                tool: "first_bore".into(),
            },
        },
        FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Cut {
                object: "drilled".into(),
                tool: "second_bore".into(),
            },
        },
    ];
    let wall = |name: &str, x: f64| NamedReference {
        name: name.into(),
        target: ReferenceTarget::Faces(FaceSelector::NearestCenter {
            target: at(x, 20.0, 5.0),
            maximum_distance: mm(1.0),
        }),
    };
    definition.references = vec![
        facing("bottom_face", CoordinateAxis::Z, Extremum::Minimum),
        facing("top_face", CoordinateAxis::Z, Extremum::Maximum),
        facing("left_face", CoordinateAxis::X, Extremum::Minimum),
        facing("right_face", CoordinateAxis::X, Extremum::Maximum),
        facing("front_face", CoordinateAxis::Y, Extremum::Minimum),
        wall("first_wall", 20.0),
        wall("second_wall", 40.0),
    ];
    // The chamfer and hole datums of the inspection plate do not apply.
    definition
        .datums
        .retain(|d| !matches!(d.id.as_str(), "chamfer" | "hole"));
    definition.datums.push(DatumDefinition {
        id: "hole".into(),
        kind: DatumKind::Axis {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                20.0,
                20.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
        },
    });
    definition
}

fn on(anchor: &str, faces: FaceSelector) -> DrawingGdtAttachment {
    DrawingGdtAttachment {
        faces: Some(faces),
        ..attachment(anchor)
    }
}

fn named(name: &str) -> FaceSelector {
    FaceSelector::Named(name.into())
}

fn bore_position(id: &str, wall: &str) -> DrawingFeatureControlFrame {
    let mut f = control(id, "hole", GeometricCharacteristic::Position, 0.2, &[]);
    f.attachment = on("hole", named(wall));
    f.zone = GeometricToleranceZone::Diameter;
    f.material = ToleranceMaterialCondition::Maximum;
    f.feature_of_size = true;
    f.pattern = Some("bores".into());
    f.size_limits = Some(DrawingSizeLimits {
        kind: FeatureOfSizeKind::Internal,
        lower: Quantity::length(9.9, LengthUnit::Millimeter),
        upper: Quantity::length(10.1, LengthUnit::Millimeter),
    });
    f.datums = vec![
        DrawingDatumReference {
            datum_feature: "A".into(),
            boundary: DatumMaterialBoundary::Regardless,
            common_with: Vec::new(),
            translation: false,
        },
        DrawingDatumReference {
            datum_feature: "W".into(),
            boundary: DatumMaterialBoundary::Maximum,
            common_with: Vec::new(),
            translation: false,
        },
    ];
    f.refinement = Some(DrawingCompositeRefinement {
        tolerance: Quantity::length(0.05, LengthUnit::Millimeter),
        datums: f.datums[..1].to_vec(),
        refinement: None,
    });
    f
}

/// Datums A (bottom), B (left), C (front) and a width W across the ends; a
/// flat top in its free state; an n× composite position of both bores;
/// a perpendicular right end on its tangent plane; a projected position;
/// and an angularity on the chamfer anchor, which names no faces.
fn pmi_page() -> DrawingDefinition {
    let mut page = page();
    for (feature, face) in
        page.datum_features
            .iter_mut()
            .zip(["bottom_face", "left_face", "front_face"])
    {
        feature.attachment.faces = Some(named(face));
    }
    page.datum_features.push(DrawingDatumFeature {
        id: "W".into(),
        label: "W".into(),
        feature_of_size: true,
        attachment: on(
            "right",
            FaceSelector::Union(vec![named("left_face"), named("right_face")]),
        ),
        size_kind: Some(FeatureOfSizeKind::External),
        size_limits: Some(DrawingSizeLimits {
            kind: FeatureOfSizeKind::External,
            lower: Quantity::length(59.9, LengthUnit::Millimeter),
            upper: Quantity::length(60.1, LengthUnit::Millimeter),
        }),
    });
    let mut flat = control("flat", "top", GeometricCharacteristic::Flatness, 0.05, &[]);
    flat.attachment = on("top", named("top_face"));
    flat.modifiers = vec![ToleranceModifier::FreeState];
    let mut square = control(
        "square",
        "right",
        GeometricCharacteristic::Perpendicularity,
        0.05,
        &["A"],
    );
    square.attachment = on("right", named("right_face"));
    square.tangent_plane = true;
    let mut projected = control(
        "projected",
        "hole",
        GeometricCharacteristic::Position,
        0.3,
        &["A", "B", "C"],
    );
    projected.attachment = on("hole", named("first_wall"));
    projected.zone = GeometricToleranceZone::Diameter;
    projected.feature_of_size = true;
    projected.projected = Some(DrawingProjectedZone {
        height: Quantity::length(15.0, LengthUnit::Millimeter),
        from: None,
    });
    let angular = control(
        "angular",
        "top",
        GeometricCharacteristic::Angularity,
        0.05,
        &["A", "B"],
    );
    page.feature_control_frames = vec![
        flat,
        bore_position("first", "first_wall"),
        bore_position("second", "second_wall"),
        square,
        projected,
        angular,
    ];
    page
}

fn item<'a>(report: &'a PmiExchangeReport, id: &str, segment: usize) -> &'a PmiExchangeItem {
    report
        .items
        .iter()
        .find(|i| i.id == id && i.segment == segment && i.kind != PmiItemKind::Size)
        .unwrap_or_else(|| panic!("no item {id}/{segment} in {report:#?}"))
}

#[test]
fn drawing_gdt_exports_as_verified_ap242_pmi_on_its_faces() {
    let definition = drilled_plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    // A turned and moved part: selectors work in its own coordinates and the
    // faces they find are carried to the placed output.
    graph
        .set_placement(
            "part",
            Placement {
                translation: VectorQuantity::lengths(100.0, 50.0, 0.0, LengthUnit::Millimeter),
                rotation: Some(AxisAngle {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(1.0, 1.0, 0.0),
                    angle_radians: 0.7,
                }),
            },
        )
        .unwrap();
    let page = pmi_page();
    page.validate(&graph).unwrap();
    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    let path = std::env::temp_dir().join(format!("occb-pmi-{}.step", std::process::id()));
    let report = page
        .export_step_pmi(
            &graph,
            &session,
            &generation,
            &path,
            "plate",
            &OutputSet::AllWithOutput("body".into()),
        )
        .unwrap();
    assert_eq!(report.parts, 1);
    for id in ["A", "B", "C", "W"] {
        let datum = item(&report, id, 0);
        assert_eq!(datum.kind, PmiItemKind::Datum);
        assert!(datum.lost.is_empty(), "{datum:?}");
    }
    assert_eq!(item(&report, "W", 0).faces, 2);
    let flat = item(&report, "flat", 0);
    assert!(
        flat.lost.is_empty() && flat.unrepresented.is_empty(),
        "{flat:?}"
    );
    assert_eq!(item(&report, "square", 0).lost, Vec::new());
    // Both bores are one n× tolerance; its lower composite segment is a
    // tolerance of its own whose relation to the top is not representable.
    let pattern = item(&report, "first", 0);
    assert_eq!(pattern.faces, 2);
    assert!(pattern.lost.is_empty() && pattern.unrepresented.is_empty());
    assert!(!report.items.iter().any(|i| i.id == "second"));
    let lower = item(&report, "first", 1);
    assert_eq!(lower.unrepresented, vec![PmiProperty::CompositeRelation]);
    assert!(lower.lost.is_empty(), "{lower:?}");
    // OCCT's AP242 writer drops projected zones; the read-back shows it.
    assert_eq!(
        item(&report, "projected", 0).lost,
        vec![PmiProperty::ProjectedZone]
    );
    // The bore diameter and the width W, each read back.
    let sizes: Vec<_> = report
        .items
        .iter()
        .filter(|i| i.kind == PmiItemKind::Size)
        .collect();
    assert_eq!(sizes.len(), 2, "{sizes:?}");
    assert!(sizes.iter().all(|s| s.lost.is_empty()), "{sizes:?}");
    assert_eq!(
        report.skipped,
        vec![PmiSkipped {
            id: "angular".into(),
            reason: "no persistent faces".into(),
        }]
    );
    assert!(!report.verified() && !report.complete());
    std::fs::remove_file(&path).unwrap();

    // Without the projected zone, every exported item reads back.
    let mut clean = page.clone();
    clean.feature_control_frames.retain(|f| f.id != "projected");
    let report = clean
        .export_step_pmi(
            &graph,
            &session,
            &generation,
            &path,
            "plate",
            &OutputSet::AllWithOutput("body".into()),
        )
        .unwrap();
    assert!(report.verified(), "{report:#?}");
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn pmi_skips_what_it_cannot_attach_and_refuses_broken_faces() {
    let definition = drilled_plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    let path = std::env::temp_dir().join(format!("occb-pmi-skip-{}.step", std::process::id()));
    let export = |page: &DrawingDefinition| {
        page.export_step_pmi(
            &graph,
            &session,
            &generation,
            &path,
            "plate",
            &OutputSet::AllWithOutput("body".into()),
        )
    };
    // Datum W without faces is skipped, and so is everything referencing it;
    // a pattern with one member lacking faces is skipped whole.
    let mut page = pmi_page();
    page.datum_features[3].attachment.faces = None;
    page.feature_control_frames.retain(|f| f.id != "projected");
    let report = export(&page).unwrap();
    let skipped: Vec<_> = report
        .skipped
        .iter()
        .map(|s| (s.id.as_str(), s.reason.as_str()))
        .collect();
    assert_eq!(
        skipped,
        vec![
            ("W", "no persistent faces"),
            ("first", "datum feature 'W' is not exported"),
            ("second", "datum feature 'W' is not exported"),
            ("angular", "no persistent faces"),
        ]
    );
    let mut page = pmi_page();
    page.feature_control_frames[2].attachment.faces = None;
    let report = export(&page).unwrap();
    assert!(
        report
            .skipped
            .iter()
            .any(|s| s.id == "first"
                && s.reason == "pattern member 'second' has no persistent faces"),
        "{report:#?}"
    );
    // Faces that name no reference fail validation; faces that select
    // nothing fail the export.
    let mut page = pmi_page();
    page.feature_control_frames[0].attachment.faces = Some(named("missing"));
    let error = page.validate(&graph).unwrap_err().message;
    assert!(error.contains("attachment faces names unknown face reference 'missing'"));
    let mut page = pmi_page();
    page.feature_control_frames[0].attachment.faces = Some(FaceSelector::NearestCenter {
        target: at(500.0, 0.0, 0.0),
        maximum_distance: mm(1.0),
    });
    let error = export(&page).unwrap_err().message;
    assert!(error.contains("flat"), "{error}");
    let _ = std::fs::remove_file(&path);
}

fn annotation_view(
    id: &str,
    direction: (f64, f64, f64),
    datum_features: &[&str],
    frames: &[&str],
) -> DrawingAnnotationView {
    DrawingAnnotationView {
        id: id.into(),
        name: id.to_uppercase(),
        direction: VectorQuantity::scalars(direction.0, direction.1, direction.2),
        x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
        scale: 1.0,
        datum_features: datum_features.iter().map(|s| (*s).into()).collect(),
        feature_control_frames: frames.iter().map(|s| (*s).into()).collect(),
    }
}

/// A top view of datums A and B, the flat top, the bore pattern and the
/// square end, and a front view at twice the size of W, the second bore
/// (which shows its pattern), the flat top again and the angularity, which
/// names no faces.
fn viewed_page() -> DrawingDefinition {
    let mut page = pmi_page();
    page.feature_control_frames.retain(|f| f.id != "projected");
    let mut front = annotation_view(
        "front",
        (0.0, -1.0, 0.0),
        &["W"],
        &["second", "flat", "angular"],
    );
    front.scale = 2.0;
    page.annotation_views = vec![
        annotation_view(
            "top",
            (0.0, 0.0, 1.0),
            &["A", "B"],
            &["flat", "first", "square"],
        ),
        front,
    ];
    page
}

#[test]
fn annotation_views_export_as_saved_views_of_3d_presentations() {
    let definition = drilled_plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = viewed_page();
    page.validate(&graph).unwrap();

    // Each annotation lies in the plane of the first view showing it, at
    // that view's scale; C appears in no view and has no presentation.
    let plan = page.pmi_plan(&graph, |_| true).unwrap();
    let datum = |id: &str| plan.datums.iter().find(|d| d.id == id).unwrap();
    let a = datum("A").presentation.as_ref().unwrap();
    assert_eq!(a.name, "datum A");
    assert_eq!(
        (a.normal, a.x_direction),
        (Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0))
    );
    assert!(datum("C").presentation.is_none());
    let w = datum("W").presentation.as_ref().unwrap();
    assert_eq!(w.normal, Vec3::new(0.0, -1.0, 0.0));
    // The label box starts at the anchor plus twice the sheet offset
    // [20, 30]: right is +x and up is +z when looking along +y.
    let corner = w.polylines[0][0];
    let expected = Vec3::new(w.origin.x + 40.0, w.origin.y, w.origin.z + 60.0);
    assert!(
        (corner.x - expected.x).abs() < 1e-9
            && (corner.y - expected.y).abs() < 1e-9
            && (corner.z - expected.z).abs() < 1e-9,
        "{corner:?} vs {expected:?}"
    );
    // The composite pattern: its frame with 2X over the top segment, and the
    // lower segment's own row.
    let segments: Vec<_> = plan
        .tolerances
        .iter()
        .filter(|t| t.id == "first")
        .map(|t| t.presentation.as_ref().unwrap())
        .collect();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].name, "frame first");
    assert_eq!(segments[1].name, "frame first segment 2");
    assert!(segments[0].polylines.len() > segments[1].polylines.len());
    let top = &plan.views[0];
    assert_eq!(top.annotations, ["A", "B", "flat", "first", "square"]);
    assert_eq!((top.datums.len(), top.tolerances.len()), (2, 4));
    let front = &plan.views[1];
    assert_eq!(front.annotations, ["W", "second", "flat"]);
    assert_eq!(front.omitted, ["angular"]);
    assert_eq!(front.axes.up, Vec3::new(0.0, 0.0, 1.0));

    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    let path = std::env::temp_dir().join(format!("occb-pmi-views-{}.step", std::process::id()));
    let report = page
        .export_step_pmi(
            &graph,
            &session,
            &generation,
            &path,
            "plate",
            &OutputSet::AllWithOutput("body".into()),
        )
        .unwrap();
    assert!(report.verified(), "{report:#?}");
    assert!(!report.complete());
    assert_eq!(report.views.len(), 2);
    assert_eq!(report.views[0].name, "TOP");
    assert_eq!(report.views[1].omitted, ["angular"]);
    assert!(
        report
            .views
            .iter()
            .all(|v| v.lost.is_none() && v.lost_annotations.is_empty())
    );
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.matches("CAMERA_MODEL_D3(").count(), 2);
    assert!(text.contains("DRAUGHTING_CALLOUT('frame first segment 2'"));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn annotation_views_need_known_annotations_and_square_axes() {
    let definition = drilled_plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let error = |page: DrawingDefinition| page.validate(&graph).unwrap_err().message;
    let mut page = viewed_page();
    page.annotation_views[0]
        .feature_control_frames
        .push("missing".into());
    assert!(error(page).contains("shows unknown feature control frame 'missing'"));
    let mut page = viewed_page();
    page.annotation_views[1].name = "TOP".into();
    assert!(error(page).contains("unique name"));
    let mut page = viewed_page();
    page.annotation_views[0].x_axis = VectorQuantity::scalars(1.0, 0.0, 0.5);
    assert!(error(page).contains("perpendicular"));
    let mut page = viewed_page();
    page.annotation_views[0].datum_features.push("A".into());
    assert!(error(page).contains("twice"));
}
