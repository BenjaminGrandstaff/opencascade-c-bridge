use super::*;
use crate::selection::{Features, resolve_edge_selector, resolve_face_selector};
use serde_json::json;

fn part(family: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "test".into(),
        definition: family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
fn example() -> FamilyDefinition {
    ModelDocument::from_json(
        &serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../../tools/model/edited-regions.request.json"
        ))
        .unwrap()["model"]
            .to_string(),
    )
    .unwrap()
    .family
}
fn region(witnesses: &[&str], excludes: &[&str]) -> FaceSelector {
    FaceSelector::SketchRegion {
        witnesses: witnesses.iter().map(|s| (*s).into()).collect(),
        excludes: excludes.iter().map(|s| (*s).into()).collect(),
    }
}
fn curve(entity: &str, require_complete: bool) -> EdgeSelector {
    EdgeSelector::SketchCurve {
        entity: entity.into(),
        require_complete,
    }
}

#[test]
fn sketch_curve_references_follow_split_lineage_and_generated_side_faces_across_edits() {
    let session = Session::new().unwrap();
    let f = example();
    let definitions = Features::new(&f);
    let mut part = part(&f);
    let first = part.regenerate(&session).unwrap();
    for width in [20., 24., 18.] {
        part.overrides.insert(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(width, LengthUnit::Millimeter)),
        );
        let built = part.regenerate_incremental(&session, &first).unwrap();
        let parameters = resolve_parameters(&f, &part.overrides).unwrap();
        let source = built.shape("profile").unwrap();
        let all = resolve_edge_selector(
            &session,
            source,
            &curve("bottom", false),
            &parameters,
            &built.shapes,
            &definitions,
        )
        .unwrap();
        assert_eq!(all.len(), 2);
        let piece = resolve_edge_selector(
            &session,
            source,
            &curve("bottom-right", true),
            &parameters,
            &built.shapes,
            &definitions,
        )
        .unwrap();
        assert_eq!(piece.len(), 1);
        assert!((session.edge_length(&piece[0]).unwrap() - width * 0.5).abs() < 1e-7);
        assert!(
            resolve_edge_selector(
                &session,
                source,
                &curve("bottom", true),
                &parameters,
                &built.shapes,
                &definitions
            )
            .unwrap_err()
            .message
            .contains("incomplete")
        );
        let persistent = EdgeSelector::Persistent {
            feature: "profile".into(),
            select: Box::new(curve("bottom-right", true)),
        };
        let base_edges = resolve_edge_selector(
            &session,
            built.shape("pads").unwrap(),
            &persistent,
            &parameters,
            &built.shapes,
            &definitions,
        )
        .unwrap();
        assert_eq!(base_edges.len(), 1);
        assert!(session.exact_bounds(&base_edges[0]).unwrap().max.z.abs() < 1e-7);
        let side = FaceSelector::GeneratedFromEdges {
            source_feature: "profile".into(),
            source: Box::new(curve("bottom-right", true)),
        };
        let faces = resolve_face_selector(
            &session,
            built.shape("pads").unwrap(),
            &side,
            &parameters,
            &built.shapes,
            &definitions,
        )
        .unwrap();
        assert_eq!(faces.len(), 1);
        assert!((session.surface_area(&faces[0]).unwrap() - width).abs() < 1e-6);
        assert!(
            resolve_edge_selector(
                &session,
                source,
                &curve("deleted", false),
                &parameters,
                &built.shapes,
                &definitions
            )
            .unwrap_err()
            .message
            .contains("lost sketch curve reference")
        );
    }
}

#[test]
fn witnessed_region_references_select_named_material_and_reject_lost_boundary_and_merge() {
    let session = Session::new().unwrap();
    let f = example();
    let d = Features::new(&f);
    let built = part(&f).regenerate(&session).unwrap();
    let parameters = resolve_parameters(&f, &HashMap::new()).unwrap();
    let source = built.shape("profile").unwrap();
    let selected = resolve_face_selector(
        &session,
        source,
        &region(&["main-pick"], &["extra-pick"]),
        &parameters,
        &built.shapes,
        &d,
    )
    .unwrap();
    assert_eq!(selected.len(), 1);
    assert!(session.surface_area(&selected[0]).unwrap() > 190.);
    for (selector, message) in [
        (
            region(&["main-pick", "extra-pick"], &[]),
            "split sketch region reference",
        ),
        (region(&["deleted"], &[]), "unknown point"),
        (region(&["short-end"], &[]), "boundary"),
        (region(&["a"], &[]), "outside material"),
        (region(&["main-pick"], &["cut"]), "merged or ambiguous"),
        (region(&[], &[]), "unique witnesses"),
    ] {
        assert!(
            resolve_face_selector(&session, source, &selector, &parameters, &built.shapes, &d)
                .unwrap_err()
                .message
                .contains(message),
            "{message}"
        );
    }
}

#[test]
fn crossing_hints_reject_ties_instead_of_switching_split_branch() {
    let mut f = example();
    let FeatureOperation::SketchFace { sketch } = &mut f.features[0].operation else {
        panic!()
    };
    // Two intersections of a full circle and its diameter are equidistant
    // from the named center; the split must reject this ambiguous hint.
    sketch.points.clear();
    sketch.lines.clear();
    sketch.shapes.clear();
    sketch.regions.clear();
    sketch.region_edits.clear();
    let mm = |v| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
    for (id, x, y) in [
        ("center", 0., 0.),
        ("rim", 10., 0.),
        ("a", -12., 0.),
        ("b", 12., 0.),
    ] {
        sketch.points.push(
            serde_json::from_value(json!({"id":id,"x":mm(x),"y":mm(y),"fixed":true})).unwrap(),
        );
    }
    sketch.circles =
        serde_json::from_value(json!([{"id":"circle","center":"center","rim":"rim"}])).unwrap();
    sketch.lines = serde_json::from_value(json!([{"id":"divider","start":"a","end":"b"}])).unwrap();
    sketch.curve_operations = serde_json::from_value(json!([{"split":{"entity":"circle","at":{"crossing":{"curve":"divider","near":"center"}},"before":"one","after":"two"}}])).unwrap();
    sketch.profile = vec!["one".into(), "two".into()];
    let session = Session::new().unwrap();
    assert!(
        part(&f)
            .regenerate(&session)
            .err()
            .unwrap()
            .message
            .contains("ambiguous crossing reference")
    );
}

#[test]
fn discovered_region_references_detect_topology_splits_merges_and_removed_internal_edges() {
    let session = Session::new().unwrap();
    for merge in [false, true] {
        let mut f = example();
        let FeatureOperation::SketchFace { sketch } = &mut f.features[0].operation else {
            panic!()
        };
        let mm = |v| json!({"literal":{"value":v,"dimension":"length","unit":"millimeter"}});
        let mut value = serde_json::to_value(sketch.as_ref()).unwrap();
        value["points"] = json!(
            [
                ("center", 0., 0.),
                ("rim", 10., 0.),
                ("left", -12., 0.),
                ("right", 12., 0.),
                ("upper", 0., 5.),
                ("lower", 0., -5.)
            ]
            .iter()
            .map(|(id, x, y)| json!({"id":id,"x":mm(*x),"y":mm(*y),"fixed":true}))
            .collect::<Vec<_>>()
        );
        value["lines"] = json!([{"id":"divider","start":"left","end":"right"}]);
        value["circles"] = json!([{"id":"disc","center":"center","rim":"rim"}]);
        for field in [
            "shapes",
            "regions",
            "region_edits",
            "curve_operations",
            "profile",
        ] {
            value[field] = json!([]);
        }
        value["region_discovery"] = json!({"merge_adjacent":merge});
        **sketch = serde_json::from_value(value).unwrap();
        f.features.truncate(1);
        f.requirements.clear();
        let d = Features::new(&f);
        let built = part(&f).regenerate(&session).unwrap();
        let parameters = resolve_parameters(&f, &HashMap::new()).unwrap();
        let source = built.shape("profile").unwrap();
        let both = resolve_face_selector(
            &session,
            source,
            &region(&["upper", "lower"], &[]),
            &parameters,
            &built.shapes,
            &d,
        );
        let guarded = resolve_face_selector(
            &session,
            source,
            &region(&["upper"], &["lower"]),
            &parameters,
            &built.shapes,
            &d,
        );
        let divider = resolve_edge_selector(
            &session,
            source,
            &curve("divider", false),
            &parameters,
            &built.shapes,
            &d,
        );
        if merge {
            assert_eq!(both.unwrap().len(), 1);
            assert!(guarded.unwrap_err().message.contains("merged or ambiguous"));
            assert!(
                divider
                    .unwrap_err()
                    .message
                    .contains("no surviving boundary")
            );
        } else {
            assert!(
                both.unwrap_err()
                    .message
                    .contains("split sketch region reference")
            );
            assert_eq!(guarded.unwrap().len(), 1);
            assert_eq!(divider.unwrap().len(), 1);
        }
        let disc = resolve_edge_selector(
            &session,
            source,
            &curve("disc", true),
            &parameters,
            &built.shapes,
            &d,
        )
        .unwrap();
        let circumference = disc
            .iter()
            .map(|e| session.edge_length(e).unwrap())
            .sum::<f64>();
        assert!((circumference - 20. * std::f64::consts::PI).abs() < 1e-6);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn named_sketch_references_drive_attached_boss_and_hollow_through_fusion_and_reject_bad_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/stable-sketch-references.request.json"
    ))
    .unwrap();
    let f = ModelDocument::from_json(&request["model"].to_string())
        .unwrap()
        .family;
    let session = Session::new().unwrap();
    let mut part = part(&f);
    let original = part.regenerate(&session).unwrap();
    let volume = session.volume(original.shape("body").unwrap()).unwrap();
    for width in [16., 24., 28.] {
        part.overrides.insert(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(width, LengthUnit::Millimeter)),
        );
        let built = part.regenerate_incremental(&session, &original).unwrap();
        assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
        assert_eq!(
            session
                .subshape_count(built.shape("body").unwrap(), ShapeType::Solid)
                .unwrap(),
            1
        );
        assert!(
            session.volume(built.shape("body").unwrap()).unwrap()
                < session.volume(built.shape("joined").unwrap()).unwrap()
        );
    }
    let mut broken = f.clone();
    let ReferenceTarget::Faces(FaceSelector::GeneratedFromEdges { source, .. }) =
        &mut broken.references[1].target
    else {
        panic!()
    };
    **source = curve("deleted-piece", true);
    part.definition = &broken;
    assert!(
        part.regenerate_incremental(&session, &original)
            .err()
            .unwrap()
            .message
            .contains("lost sketch curve reference")
    );
    assert!((session.volume(original.shape("body").unwrap()).unwrap() - volume).abs() < 1e-9);
    drop(original);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn persistent_sketch_references_keep_descendants_in_both_contributing_branches() {
    let mut f = example();
    f.features.truncate(2);
    f.requirements.clear();
    for (id, x) in [("copy-a", 0.), ("copy-b", 30.)] {
        f.features.push(FeatureDefinition {
            id: id.into(),
            operation: FeatureOperation::Translate {
                input: "pads".into(),
                offset: VectorExpr::Literal(VectorQuantity::lengths(
                    x,
                    0.,
                    0.,
                    LengthUnit::Millimeter,
                )),
            },
        });
    }
    f.features.push(FeatureDefinition {
        id: "copies".into(),
        operation: FeatureOperation::Compound {
            inputs: vec!["copy-a".into(), "copy-b".into()],
        },
    });
    let session = Session::new().unwrap();
    let built = part(&f).regenerate(&session).unwrap();
    let parameters = resolve_parameters(&f, &HashMap::new()).unwrap();
    let selected = resolve_edge_selector(
        &session,
        built.shape("copies").unwrap(),
        &EdgeSelector::Persistent {
            feature: "profile".into(),
            select: Box::new(curve("bottom-right", true)),
        },
        &parameters,
        &built.shapes,
        &Features::new(&f),
    )
    .unwrap();
    assert_eq!(selected.len(), 2);
    let mut centers = selected
        .iter()
        .map(|e| session.center_of_mass(e).unwrap().x)
        .collect::<Vec<_>>();
    centers.sort_by(f64::total_cmp);
    assert!((centers[1] - centers[0] - 30.).abs() < 1e-7);
}

#[test]
fn persistent_sketch_base_references_survive_symmetric_and_face_limited_extrusions() {
    let session = Session::new().unwrap();
    for symmetric in [true, false] {
        let mut f = example();
        f.features.truncate(2);
        f.requirements.clear();
        if !symmetric {
            f.features.push(FeatureDefinition {
                id: "limit".into(),
                operation: FeatureOperation::Box {
                    origin: VectorExpr::Literal(VectorQuantity::lengths(
                        -5.,
                        -5.,
                        3.,
                        LengthUnit::Millimeter,
                    )),
                    size: VectorExpr::Literal(VectorQuantity::lengths(
                        100.,
                        20.,
                        1.,
                        LengthUnit::Millimeter,
                    )),
                },
            });
        }
        let FeatureOperation::Extrude { extent, .. } = &mut f.features[1].operation else {
            panic!()
        };
        *extent = if symmetric {
            ExtrudeExtent::Symmetric
        } else {
            ExtrudeExtent::UpToFace {
                target: "limit".into(),
                face: Box::new(FaceSelector::NormalAligned {
                    direction: VectorExpr::Literal(VectorQuantity::scalars(0., 0., -1.)),
                    minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999)),
                }),
            }
        };
        let built = part(&f).regenerate(&session).unwrap();
        let parameters = resolve_parameters(&f, &HashMap::new()).unwrap();
        let selector = FaceSelector::Persistent {
            feature: "profile".into(),
            select: Box::new(region(&["main-pick"], &["extra-pick"])),
        };
        let selected = resolve_face_selector(
            &session,
            built.shape("pads").unwrap(),
            &selector,
            &parameters,
            &built.shapes,
            &Features::new(&f),
        )
        .unwrap();
        assert_eq!(selected.len(), 1);
        let expected_z = if symmetric { -1. } else { 0. };
        assert!((session.center_of_mass(&selected[0]).unwrap().z - expected_z).abs() < 1e-7);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
