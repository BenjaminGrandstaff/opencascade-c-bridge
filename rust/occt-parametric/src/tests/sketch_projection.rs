use super::*;
fn document() -> ModelDocument {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-pocket.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&r["model"].to_string()).unwrap()
}
fn part(f: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition: f,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
fn profile(f: &mut FamilyDefinition) -> &mut SketchDefinition {
    let FeatureOperation::SketchFace { sketch } = &mut f
        .features
        .iter_mut()
        .find(|f| f.id == "profile")
        .unwrap()
        .operation
    else {
        panic!()
    };
    sketch
}
fn mm(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn vector(x: f64, y: f64, z: f64, dim: Dimension) -> VectorExpr {
    VectorExpr::Literal(if dim == Dimension::Length {
        VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter)
    } else {
        VectorQuantity::scalars(x, y, z)
    })
}
fn point(id: &str, x: f64, y: f64) -> SketchPoint {
    SketchPoint {
        id: id.into(),
        x: mm(x),
        y: mm(y),
        fixed: true,
    }
}
fn blank() -> SketchDefinition {
    let mut d = document();
    let mut s = profile(&mut d.family).clone();
    s.face_support = None;
    s.projections.clear();
    s.points.clear();
    s.lines.clear();
    s.constraints.clear();
    s.profile.clear();
    s
}
fn fixture(source: FeatureOperation, sketch: SketchDefinition, open: bool) -> FamilyDefinition {
    let mut f = document().family;
    f.requirements.clear();
    f.features = vec![
        FeatureDefinition {
            id: "source".into(),
            operation: source,
        },
        FeatureDefinition {
            id: "projected".into(),
            operation: if open {
                FeatureOperation::SketchOpenWire {
                    sketch: Box::new(sketch),
                }
            } else {
                FeatureOperation::SketchFace {
                    sketch: Box::new(sketch),
                }
            },
        },
    ];
    f
}
fn projection(kind: SketchProjectionKind, edge: EdgeSelector) -> SketchProjection {
    SketchProjection {
        junctions: Vec::new(),
        intersections: Vec::new(),
        refinements: Vec::new(),
        id: "edge".into(),
        input: "source".into(),
        edge,
        kind,
    }
}
fn nearest(x: f64, y: f64, z: f64) -> EdgeSelector {
    EdgeSelector::NearestCenter {
        target: vector(x, y, z, Dimension::Length),
        maximum_distance: mm(1e-6),
    }
}

#[test]
fn projected_reference_moves_the_pocket_without_copying_source_depth_into_sketch_coordinates() {
    let d = document();
    let session = Session::new().unwrap();
    let mut p = part(&d.family);
    let first = p.regenerate(&session).unwrap();
    let bounds = session
        .exact_bounds(first.shape("profile").unwrap())
        .unwrap();
    assert!((bounds.min.y - 24.).abs() < 1e-7 && (bounds.max.y - 36.).abs() < 1e-7);
    p.overrides.insert(
        "depth".into(),
        ParameterValue::Scalar(Quantity::length(50., LengthUnit::Millimeter)),
    );
    let edited = p.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(
        edited.regeneration.rebuilt,
        vec!["block", "profile", "tool", "body"]
    );
    let bounds = session
        .exact_bounds(edited.shape("profile").unwrap())
        .unwrap();
    assert!((bounds.min.y - 34.).abs() < 1e-7 && (bounds.max.y - 46.).abs() < 1e-7);
    assert!((session.volume(edited.shape("body").unwrap()).unwrap() - 58800.).abs() < 1e-5);
    let count = session.shape_count().unwrap();
    let snapshots = p.resolved_sketches(&session, &edited).unwrap();
    let runtime = &snapshots["profile"].as_ref().unwrap().sketch;
    assert!(runtime.projections.is_empty());
    assert!(
        runtime
            .points
            .iter()
            .any(|p| p.id == "front-edge:start" && p.fixed)
    );
    let solved = runtime.solve(&p.resolved_parameters().unwrap()).unwrap();
    assert!(solved.solved);
    assert_eq!(solved.free_degrees, 0);
    let FeatureOperation::SketchFace { sketch: source } = &d.family.features[1].operation else {
        panic!()
    };
    let parameters = p.resolved_parameters().unwrap();
    assert!(source.solve(&parameters).is_err());
    assert!(source.constraint_checks(&parameters, &solved).is_err());
    assert!(source.preview_curves(&session, &solved, 32).is_err());
    assert!(
        source
            .preview_edited_profile(&session, &parameters, &solved, 32, true)
            .is_err()
    );
    assert!((solved.points["guide"].y - 25.).abs() < 1e-7);
    assert_eq!(session.shape_count().unwrap(), count);
    let mut bad = d.clone();
    profile(&mut bad.family).projections[0].edge = EdgeSelector::AtExtreme {
        axis: CoordinateAxis::Z,
        extremum: Extremum::Maximum,
        tolerance: mm(1e-6),
    };
    p.definition = &bad.family;
    assert!(p.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn linked_closed_circle_and_tilted_ellipse_preserve_native_projected_area() {
    let source = FeatureOperation::Cylinder {
        origin: vector(0., 0., 0., Dimension::Length),
        axis: vector(0., 0., 1., Dimension::Scalar),
        radius: mm(5.),
        height: mm(10.),
    };
    let mut s = blank();
    s.projections.push(projection(
        SketchProjectionKind::Circle,
        nearest(0., 0., 10.),
    ));
    s.profile = vec!["edge".into()];
    let mut f = fixture(source, s.clone(), false);
    let session = Session::new().unwrap();
    let first = part(&f).regenerate(&session).unwrap();
    assert!(
        (session
            .surface_area(first.shape("projected").unwrap())
            .unwrap()
            - 25. * std::f64::consts::PI)
            .abs()
            < 1e-7
    );
    s.projections[0].kind = SketchProjectionKind::Ellipse;
    s.y_axis = vector(0., 0.5, 3.0_f64.sqrt() * 0.5, Dimension::Scalar);
    f.features[1].operation = FeatureOperation::SketchFace {
        sketch: Box::new(s.clone()),
    };
    let tilted = part(&f).regenerate(&session).unwrap();
    assert!(
        (session
            .surface_area(tilted.shape("projected").unwrap())
            .unwrap()
            - 12.5 * std::f64::consts::PI)
            .abs()
            < 1e-7
    );
    assert!(
        session
            .is_valid(tilted.shape("projected").unwrap())
            .unwrap()
    );
    let count = session.shape_count().unwrap();
    s.projections[0].kind = SketchProjectionKind::Circle;
    f.features[1].operation = FeatureOperation::SketchFace {
        sketch: Box::new(s),
    };
    assert!(part(&f).regenerate_incremental(&session, &tilted).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    let FeatureOperation::SketchFace { sketch } = &mut f.features[1].operation else {
        panic!()
    };
    sketch.projections[0].kind = SketchProjectionKind::Ellipse;
    sketch.y_axis = vector(0., 0., 1., Dimension::Scalar);
    let error = part(&f).regenerate(&session).err().unwrap();
    assert!(error.message.contains("degenerate"));
    assert_eq!(session.shape_count().unwrap(), count);
    drop((first, tilted));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn projected_arcs_and_named_line_endpoints_can_form_native_profiles() {
    let mut source = blank();
    source.origin = vector(0., 0., 10., Dimension::Length);
    source.points = vec![point("c", 0., 0.), point("a", 5., 0.), point("b", 0., 5.)];
    source.arcs = vec![SketchArc {
        id: "arc".into(),
        center: "c".into(),
        start: "a".into(),
        end: "b".into(),
        clockwise: false,
    }];
    source.profile = vec!["arc".into()];
    let mut s = blank();
    s.projections.push(projection(
        SketchProjectionKind::Arc,
        EdgeSelector::AtExtreme {
            axis: CoordinateAxis::Z,
            extremum: Extremum::Maximum,
            tolerance: mm(1e-6),
        },
    ));
    s.profile = vec!["edge".into()];
    s.y_axis = vector(0., -1., 0., Dimension::Scalar);
    let f = fixture(
        FeatureOperation::SketchOpenWire {
            sketch: Box::new(source),
        },
        s,
        true,
    );
    let session = Session::new().unwrap();
    let arc = part(&f).regenerate(&session).unwrap();
    let edge = session
        .subshapes(
            arc.shape("projected").unwrap(),
            occt_bridge::ShapeType::Edge,
        )
        .unwrap()
        .pop()
        .unwrap();
    assert!((session.edge_length(&edge).unwrap() - 2.5 * std::f64::consts::PI).abs() < 1e-7);
    assert!((session.edge_circle_radius(&edge).unwrap().unwrap() - 5.).abs() < 1e-7);
    let runtime = part(&f).resolved_sketches(&session, &arc).unwrap();
    assert!(runtime["projected"].as_ref().unwrap().sketch.arcs[0].clockwise);
    drop((edge, arc));
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut source = blank();
    source.origin = vector(0., 0., 10., Dimension::Length);
    source.points = vec![point("a", 0., 0.), point("b", 10., 0.)];
    source.lines = vec![SketchLine {
        id: "line".into(),
        start: "a".into(),
        end: "b".into(),
    }];
    source.profile = vec!["line".into()];
    let mut s = blank();
    s.projections
        .push(projection(SketchProjectionKind::Line, nearest(5., 0., 10.)));
    s.points = vec![point("br", 10., -5.), point("bl", 0., -5.)];
    s.lines = vec![
        SketchLine {
            id: "right".into(),
            start: "edge:end".into(),
            end: "br".into(),
        },
        SketchLine {
            id: "bottom".into(),
            start: "br".into(),
            end: "bl".into(),
        },
        SketchLine {
            id: "left".into(),
            start: "bl".into(),
            end: "edge:start".into(),
        },
    ];
    s.profile = vec![
        "edge".into(),
        "right".into(),
        "bottom".into(),
        "left".into(),
    ];
    let f = fixture(
        FeatureOperation::SketchOpenWire {
            sketch: Box::new(source),
        },
        s,
        false,
    );
    let rectangle = part(&f).regenerate(&session).unwrap();
    assert!(
        (session
            .surface_area(rectangle.shape("projected").unwrap())
            .unwrap()
            - 50.)
            .abs()
            < 1e-7
    );
    drop(rectangle);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn ambiguous_collapsed_and_invalid_projection_definitions_fail_without_handle_growth() {
    let session = Session::new().unwrap();
    for case in 0..5 {
        let mut d = document();
        let s = profile(&mut d.family);
        match case {
            0 => s.projections[0].input = "missing".into(),
            1 => s.projections[0].input = "profile".into(),
            2 => s.projections[0].edge = nearest(0., 0., 10.),
            3 => s.points.push(point("front-edge:start", 0., 0.)),
            _ => s.projections[0].kind = SketchProjectionKind::Circle,
        }
        assert!(part(&d.family).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut source = blank();
    source.points = vec![point("a", 0., 0.), point("b", 5., 3.), point("c", 10., 0.)];
    source.splines = vec![SketchSpline {
        endpoints: None,
        basis: None,
        id: "spline".into(),
        points: vec!["a".into(), "b".into(), "c".into()],
    }];
    source.profile = vec!["spline".into()];
    let mut s = blank();
    s.projections.push(projection(
        SketchProjectionKind::Line,
        EdgeSelector::AtExtreme {
            axis: CoordinateAxis::Z,
            extremum: Extremum::Maximum,
            tolerance: mm(1e-6),
        },
    ));
    s.profile = vec!["edge".into()];
    let f = fixture(
        FeatureOperation::SketchOpenWire {
            sketch: Box::new(source),
        },
        s,
        true,
    );
    let error = part(&f).regenerate(&session).err().unwrap();
    assert!(error.message.contains("use kind spline"));
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut d = document();
    profile(&mut d.family).projections.resize(
        1001,
        projection(SketchProjectionKind::Line, nearest(0., 0., 0.)),
    );
    assert!(part(&d.family).regenerate(&session).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut old: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/face-pocket.request.json"
    ))
    .unwrap();
    old["model"]["schema_version"] = serde_json::json!(93);
    let migrated = ModelDocument::from_json(&old["model"].to_string()).unwrap();
    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    let old: Vec<FeatureDefinition> =
        serde_json::from_value(old["model"]["family"]["features"].clone()).unwrap();
    assert_eq!(migrated.family.features, old);
}

#[test]
fn named_persistent_projection_tracks_placed_edges_and_reference_edits_reject_conflicts() {
    let mut d = document();
    let query = profile(&mut d.family).projections[0].edge.clone();
    d.family.references.push(NamedReference {
        name: "front-reference".into(),
        target: ReferenceTarget::Edges(EdgeSelector::Persistent {
            feature: "block".into(),
            select: Box::new(query),
        }),
    });
    d.family.features.insert(
        1,
        FeatureDefinition {
            id: "placed".into(),
            operation: FeatureOperation::Translate {
                input: "block".into(),
                offset: vector(100., 0., 0., Dimension::Length),
            },
        },
    );
    let s = profile(&mut d.family);
    s.face_support.as_mut().unwrap().input = "placed".into();
    s.projections[0].input = "placed".into();
    s.projections[0].edge = EdgeSelector::Named("front-reference".into());
    if let FeatureOperation::Cut { object, .. } =
        &mut d.family.features.last_mut().unwrap().operation
    {
        *object = "placed".into();
    }
    let session = Session::new().unwrap();
    let first = part(&d.family).regenerate(&session).unwrap();
    let bounds = session
        .exact_bounds(first.shape("profile").unwrap())
        .unwrap();
    assert!((bounds.min.x - 120.).abs() < 1e-7);
    assert!((session.volume(first.shape("body").unwrap()).unwrap() - 46800.).abs() < 1e-5);
    let count = session.shape_count().unwrap();
    d.family.references[0].target = ReferenceTarget::Edges(EdgeSelector::Persistent {
        feature: "block".into(),
        select: Box::new(EdgeSelector::Intersection(vec![
            EdgeSelector::AtExtreme {
                axis: CoordinateAxis::Z,
                extremum: Extremum::Maximum,
                tolerance: mm(1e-6),
            },
            EdgeSelector::AtExtreme {
                axis: CoordinateAxis::X,
                extremum: Extremum::Maximum,
                tolerance: mm(1e-6),
            },
        ])),
    });
    assert!(
        part(&d.family)
            .regenerate_incremental(&session, &first)
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), count);
    drop(first);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn near_circular_conics_keep_their_actual_projected_type_at_large_scale() {
    let radius = 1e6;
    let angle = 3e-5_f64;
    let source = FeatureOperation::Cylinder {
        origin: vector(0., 0., 0., Dimension::Length),
        axis: vector(0., 0., 1., Dimension::Scalar),
        radius: mm(radius),
        height: mm(10.),
    };
    let mut s = blank();
    s.y_axis = vector(0., angle.cos(), angle.sin(), Dimension::Scalar);
    s.projections.push(projection(
        SketchProjectionKind::Circle,
        nearest(0., 0., 10.),
    ));
    s.profile = vec!["edge".into()];
    let mut f = fixture(source, s.clone(), false);
    let session = Session::new().unwrap();
    let error = part(&f).regenerate(&session).err().unwrap();
    assert!(error.message.contains("declared analytic kind"));
    assert_eq!(session.shape_count().unwrap(), 0);
    s.projections[0].kind = SketchProjectionKind::Ellipse;
    f.features[1].operation = FeatureOperation::SketchFace {
        sketch: Box::new(s),
    };
    let ellipse = part(&f).regenerate(&session).unwrap();
    assert!(
        (session
            .surface_area(ellipse.shape("projected").unwrap())
            .unwrap()
            - std::f64::consts::PI * radius * radius * angle.cos())
        .abs()
            < 1.0
    );
    drop(ellipse);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn point_line_margin_edits_reposition_native_pocket_and_reuse_source_block() {
    let d = document();
    let session = Session::new().unwrap();
    let mut p = part(&d.family);
    let first = p.regenerate(&session).unwrap();
    p.overrides.insert(
        "margin".into(),
        ParameterValue::Scalar(Quantity::length(6., LengthUnit::Millimeter)),
    );
    let edited = p.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.reused, vec!["block"]);
    assert_eq!(edited.regeneration.rebuilt, vec!["profile", "tool", "body"]);
    let bounds = session
        .exact_bounds(edited.shape("profile").unwrap())
        .unwrap();
    assert!((bounds.min.y - 22.).abs() < 1e-7);
    assert!((bounds.max.y - 34.).abs() < 1e-7);
    assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    assert!((session.volume(edited.shape("body").unwrap()).unwrap() - 46800.).abs() < 1e-5);
}

#[test]
fn exact_rational_spline_projection_tracks_weights_and_preserves_source_and_area() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-spline-pad.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let mut instance = part(&doc.family);
    for weight in [0.5, 0.7, 1.4] {
        instance.overrides.insert(
            "weight".into(),
            ParameterValue::Scalar(Quantity::scalar(weight)),
        );
        let built = instance.regenerate(&session).unwrap();
        let snapshot = instance.resolved_sketches(&session, &built).unwrap();
        let runtime = &snapshot["profile"].as_ref().unwrap().sketch;
        let spline = &runtime.splines[0];
        assert!(spline.basis.as_ref().unwrap().weights.len() > 5);
        assert_eq!(spline.points.first().unwrap(), "reference:start");
        assert_eq!(spline.points.last().unwrap(), "reference:end");
        assert!(runtime.points.iter().all(|p| p.fixed));
        let source = session
            .subshapes(built.shape("source").unwrap(), ShapeType::Edge)
            .unwrap();
        let solved = runtime
            .solve(&instance.resolved_parameters().unwrap())
            .unwrap();
        let samples = runtime.preview_curves(&session, &solved, 100).unwrap();
        let curve = &samples.iter().find(|(id, _)| id == "reference").unwrap().1;
        for p in curve {
            // Undo the affine projection into the tilted sketch; closest-point
            // queries verify the exact rational trace independently of sampling.
            let original = Vec3::new(p.x, (p.y + 2.) / (3f64.sqrt() / 2.), 0.);
            let closest = session.curve_closest_point(&source[0], original).unwrap();
            assert!((closest.x - original.x).hypot(closest.y - original.y) < 1e-6);
        }
        assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
        let area = session
            .surface_area(built.shape("profile").unwrap())
            .unwrap();
        let volume = session.volume(built.shape("body").unwrap()).unwrap();
        assert!(
            (volume - area * 10.).abs() < 1e-7 * volume,
            "weight {weight}: volume {volume}, area {area}"
        );
        drop(source);
        drop(built);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn exact_spline_projection_imports_closed_ellipses_and_trimmed_elliptic_arcs() {
    let mut source = blank();
    source.points = vec![point("c", 0., 0.), point("a", 10., 0.), point("b", 0., 5.)];
    source.ellipses = vec![SketchEllipse {
        id: "ellipse".into(),
        center: "c".into(),
        major: "a".into(),
        minor: "b".into(),
        start: None,
        end: None,
        clockwise: false,
    }];
    source.profile = vec!["ellipse".into()];
    let session = Session::new().unwrap();
    for trimmed in [false, true] {
        let mut source = source.clone();
        if trimmed {
            source.profile_operations = vec![SketchProfileOperation::Trim {
                entity: "ellipse".into(),
                first: ScalarExpr::Literal(Quantity::scalar(0.)),
                last: ScalarExpr::Literal(Quantity::scalar(0.5)),
            }];
        }
        let mut target = blank();
        target.projections = vec![projection(
            SketchProjectionKind::Spline,
            EdgeSelector::AtExtreme {
                axis: CoordinateAxis::Z,
                extremum: Extremum::Maximum,
                tolerance: mm(1e-6),
            },
        )];
        target.profile = vec!["edge".into()];
        if trimmed {
            target.lines = vec![SketchLine {
                id: "closing".into(),
                start: "edge:end".into(),
                end: "edge:start".into(),
            }];
            target.profile.push("closing".into());
        }
        let f = fixture(
            if trimmed {
                FeatureOperation::SketchOpenWire {
                    sketch: Box::new(source),
                }
            } else {
                FeatureOperation::SketchWire {
                    sketch: Box::new(source),
                }
            },
            target,
            false,
        );
        let built = part(&f).regenerate(&session).unwrap();
        let area = session
            .surface_area(built.shape("projected").unwrap())
            .unwrap();
        assert!((area - std::f64::consts::PI * 50. * if trimmed { 0.5 } else { 1. }).abs() < 1e-6);
        drop(built);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn spline_projection_respects_locations_rejects_collapses_and_keeps_prior_build() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-spline-pad.request.json"
    ))
    .unwrap();
    let mut doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    doc.family.features[0].id = "unplaced".into();
    doc.family.features.insert(
        1,
        FeatureDefinition {
            id: "source".into(),
            operation: FeatureOperation::Translate {
                input: "unplaced".into(),
                offset: vector(7., 8., 9., Dimension::Length),
            },
        },
    );
    let session = Session::new().unwrap();
    let built = part(&doc.family).regenerate(&session).unwrap();
    let snapshots = part(&doc.family)
        .resolved_sketches(&session, &built)
        .unwrap();
    let runtime = &snapshots["profile"].as_ref().unwrap().sketch;
    let solved = runtime
        .solve(&part(&doc.family).resolved_parameters().unwrap())
        .unwrap();
    let p = solved.points["reference:start"];
    assert!((p.x - 7.).abs() < 1e-7 && (p.y - (8. * 3f64.sqrt() / 2. + 2.5)).abs() < 1e-7);
    let count = session.shape_count().unwrap();
    let FeatureOperation::SketchFace { sketch } = &mut doc.family.features[2].operation else {
        panic!()
    };
    sketch.points.push(point("reference:pole:1", 0., 0.));
    assert!(
        part(&doc.family)
            .regenerate_incremental(&session, &built)
            .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), count);
    drop(built);
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut source = blank();
    source.points = vec![point("a", 0., 0.), point("b", 0., 5.), point("c", 0., 10.)];
    source.splines = vec![SketchSpline {
        id: "vertical".into(),
        points: vec!["a".into(), "b".into(), "c".into()],
        basis: None,
        endpoints: None,
    }];
    source.profile = vec!["vertical".into()];
    source.x_axis = vector(1., 0., 0., Dimension::Scalar);
    source.y_axis = vector(0., 0., 1., Dimension::Scalar);
    let mut target = blank();
    target.projections = vec![projection(
        SketchProjectionKind::Spline,
        EdgeSelector::AtExtreme {
            axis: CoordinateAxis::X,
            extremum: Extremum::Maximum,
            tolerance: mm(1e-6),
        },
    )];
    target.profile = vec!["edge".into()];
    let f = fixture(
        FeatureOperation::SketchOpenWire {
            sketch: Box::new(source),
        },
        target,
        true,
    );
    let error = part(&f).regenerate(&session).err().unwrap();
    assert!(
        error.message.contains("collapses to a point"),
        "{}",
        error.message
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn projected_periodic_and_nonclamped_splines_preserve_native_active_domains() {
    let session = Session::new().unwrap();
    for (file, closed) in [
        (
            include_str!("../../../../tools/model/periodic-spline-pad.request.json"),
            true,
        ),
        (
            include_str!("../../../../tools/model/non-clamped-spline-cap.request.json"),
            false,
        ),
    ] {
        let request: serde_json::Value = serde_json::from_str(file).unwrap();
        let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let original = part(&doc.family).regenerate(&session).unwrap();
        let expected = session
            .surface_area(original.shape("profile").unwrap())
            .unwrap();
        let FeatureOperation::SketchFace { sketch } = &doc.family.features[0].operation else {
            panic!()
        };
        let mut source = sketch.as_ref().clone();
        source.profile = vec!["curve".into()];
        let mut target = blank();
        target.projections = vec![projection(
            SketchProjectionKind::Spline,
            EdgeSelector::AtExtreme {
                axis: CoordinateAxis::Z,
                extremum: Extremum::Maximum,
                tolerance: mm(1e-6),
            },
        )];
        target.profile = vec!["edge".into()];
        if !closed {
            target.lines = vec![SketchLine {
                id: "closing".into(),
                start: "edge:end".into(),
                end: "edge:start".into(),
            }];
            target.profile.push("closing".into());
        }
        let mut family = doc.family.clone();
        family.requirements.clear();
        family.features = vec![
            FeatureDefinition {
                id: "source".into(),
                operation: if closed {
                    FeatureOperation::SketchWire {
                        sketch: Box::new(source),
                    }
                } else {
                    FeatureOperation::SketchOpenWire {
                        sketch: Box::new(source),
                    }
                },
            },
            FeatureDefinition {
                id: "projected".into(),
                operation: FeatureOperation::SketchFace {
                    sketch: Box::new(target),
                },
            },
        ];
        let built = part(&family).regenerate(&session).unwrap();
        let area = session
            .surface_area(built.shape("projected").unwrap())
            .unwrap();
        assert!(
            (area - expected).abs() < 1e-6 * expected,
            "closed {closed}: {area} vs {expected}"
        );
        drop(built);
        drop(original);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn projected_spline_drives_point_constraints_and_strict_acceptance() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-spline-pad.request.json"
    ))
    .unwrap();
    let mut doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let FeatureOperation::SketchFace { sketch } = &mut doc.family.features[1].operation else {
        panic!()
    };
    sketch.points.push(SketchPoint {
        id: "on_reference".into(),
        x: mm(5.),
        y: mm(5.),
        fixed: false,
    });
    sketch.constraints = vec![
        SketchConstraint::PointOnCurve {
            point: "on_reference".into(),
            curve: "reference".into(),
        },
        SketchConstraint::Distance {
            first: "reference:start".into(),
            second: "on_reference".into(),
            value: mm(10.),
        },
    ];
    let session = Session::new().unwrap();
    let built = part(&doc.family).regenerate(&session).unwrap();
    let snapshots = part(&doc.family)
        .resolved_sketches(&session, &built)
        .unwrap();
    let runtime = &snapshots["profile"].as_ref().unwrap().sketch;
    let solution = runtime
        .solve(&part(&doc.family).resolved_parameters().unwrap())
        .unwrap();
    assert!(solution.solved && solution.free_degrees == 0);
    runtime.check_acceptance(&solution).unwrap();
    let p = solution.points["on_reference"];
    let start = solution.points["reference:start"];
    assert!(((p.x - start.x).hypot(p.y - start.y) - 10.).abs() < 1e-8);
    let edge = session
        .subshapes(built.shape("source").unwrap(), ShapeType::Edge)
        .unwrap();
    let original = Vec3::new(p.x, (p.y + 2.) / (3f64.sqrt() / 2.), 0.);
    let closest = session.curve_closest_point(&edge[0], original).unwrap();
    assert!((closest.x - original.x).hypot(closest.y - original.y) < 1e-7);
    drop(edge);
    drop(built);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn mixed_degree_chain_projection_preserves_boundary_area_and_tracks_source_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-chain-pad.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let mut instance = part(&doc.family);
    for weight in [0.5, 0.7, 1.4] {
        instance.overrides.insert(
            "weight".into(),
            ParameterValue::Scalar(Quantity::scalar(weight)),
        );
        let built = instance.regenerate(&session).unwrap();
        let source_face = session
            .create_face_from_wire(built.shape("source").unwrap())
            .unwrap();
        let expected = session.surface_area(&source_face).unwrap() * 3f64.sqrt() / 2.;
        let area = session
            .surface_area(built.shape("profile").unwrap())
            .unwrap();
        assert!(
            (area - expected).abs() < 1e-7 * expected,
            "{area} vs {expected}"
        );
        let snapshots = instance.resolved_sketches(&session, &built).unwrap();
        let sk = &snapshots["profile"].as_ref().unwrap().sketch;
        assert_eq!(sk.splines.len(), 1);
        let s = &sk.splines[0];
        assert_eq!(s.points.first(), s.points.last());
        assert_eq!(s.basis.as_ref().unwrap().degree, 2);
        assert!(
            sk.solve(&instance.resolved_parameters().unwrap())
                .unwrap()
                .free_degrees
                == 0
        );
        assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
        drop(source_face);
        drop(built);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn disconnected_and_branched_chain_projections_reject_without_handle_growth() {
    let session = Session::new().unwrap();
    for branched in [false, true] {
        let mut source = blank();
        source.points = vec![
            point("a", 0., 0.),
            point("b", 5., 0.),
            point("c", 5., 5.),
            point("d", 10., 0.),
        ];
        source.lines = vec![SketchLine {
            id: "first".into(),
            start: "a".into(),
            end: "b".into(),
        }];
        source.profile = vec!["first".into()];
        let mut family = fixture(
            FeatureOperation::SketchOpenWire {
                sketch: Box::new(source.clone()),
            },
            blank(),
            true,
        );
        family.features.clear();
        family.features.push(FeatureDefinition {
            id: "first".into(),
            operation: FeatureOperation::SketchOpenWire {
                sketch: Box::new(source.clone()),
            },
        });
        source.lines = vec![SketchLine {
            id: "second".into(),
            start: if branched { "b".into() } else { "c".into() },
            end: "d".into(),
        }];
        source.profile = vec!["second".into()];
        family.features.push(FeatureDefinition {
            id: "second".into(),
            operation: FeatureOperation::SketchOpenWire {
                sketch: Box::new(source.clone()),
            },
        });
        let mut inputs = vec!["first".into(), "second".into()];
        if branched {
            source.lines = vec![SketchLine {
                id: "third".into(),
                start: "b".into(),
                end: "c".into(),
            }];
            source.profile = vec!["third".into()];
            family.features.push(FeatureDefinition {
                id: "third".into(),
                operation: FeatureOperation::SketchOpenWire {
                    sketch: Box::new(source),
                },
            });
            inputs.push("third".into());
        }
        family.features.push(FeatureDefinition {
            id: "source".into(),
            operation: FeatureOperation::Compound { inputs },
        });
        let mut target = blank();
        target.projections = vec![projection(
            SketchProjectionKind::Chain,
            EdgeSelector::AtExtreme {
                axis: CoordinateAxis::Z,
                extremum: Extremum::Maximum,
                tolerance: mm(1e-6),
            },
        )];
        target.profile = vec!["edge".into()];
        family.features.push(FeatureDefinition {
            id: "projected".into(),
            operation: FeatureOperation::SketchOpenWire {
                sketch: Box::new(target),
            },
        });
        let error = part(&family).regenerate(&session).err().unwrap();
        assert!(
            error
                .message
                .contains(if branched { "branched" } else { "disconnected" }),
            "{}",
            error.message
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn named_junctions_drive_constraints_and_invalidate_on_hint_only_edits() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-junction-pad.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let mut instance = part(&doc.family);
    let first = instance.regenerate(&session).unwrap();
    let snapshots = instance.resolved_sketches(&session, &first).unwrap();
    let sk = &snapshots["profile"].as_ref().unwrap().sketch;
    let solution = sk.solve(&instance.resolved_parameters().unwrap()).unwrap();
    assert!(solution.solved && solution.free_degrees == 0);
    let right = solution.points["reference:junction:right"];
    let marker = solution.points["marker"];
    assert!((right.x - 40.).abs() < 1e-8 && (right.y + 2.).abs() < 1e-8);
    assert!((marker.x - right.x).hypot(marker.y - right.y) < 1e-8);
    instance.overrides.insert(
        "width".into(),
        ParameterValue::Scalar(Quantity::length(60., LengthUnit::Millimeter)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    let snapshots = instance.resolved_sketches(&session, &edited).unwrap();
    let sk = &snapshots["profile"].as_ref().unwrap().sketch;
    let solution = sk.solve(&instance.resolved_parameters().unwrap()).unwrap();
    assert!((solution.points["reference:junction:right"].x - 60.).abs() < 1e-7);
    let count = session.shape_count().unwrap();
    instance.overrides.insert(
        "junction_shift".into(),
        ParameterValue::Scalar(Quantity::length(0.1, LengthUnit::Millimeter)),
    );
    let error = instance
        .regenerate_incremental(&session, &edited)
        .err()
        .unwrap();
    assert!(
        error.message.contains("found no source junction"),
        "{}",
        error.message
    );
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    drop(first);
    drop(edited);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn junction_references_reject_ambiguous_matches_bad_ids_and_non_chain_usage() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-junction-pad.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    for case in 0..5 {
        let mut f = doc.family.clone();
        let FeatureOperation::SketchFace { sketch } = &mut f.features[1].operation else {
            panic!()
        };
        let projection = &mut sketch.projections[0];
        let expected = match case {
            0 => {
                projection.junctions[0].maximum_distance = mm(100.);
                "multiple source junctions"
            }
            1 => {
                projection.junctions[0].maximum_distance = mm(0.);
                "must be positive"
            }
            2 => {
                projection.junctions[1].id = "left".into();
                "nonempty and unique"
            }
            3 => {
                projection.kind = SketchProjectionKind::Spline;
                "require a chain"
            }
            _ => {
                sketch.points.push(point("reference:junction:left", 0., 0.));
                "unique"
            }
        };
        let error = part(&f).regenerate(&session).err().unwrap();
        assert!(error.message.contains(expected), "{}", error.message);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

fn intersection_document() -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-intersection-pad.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}
#[test]
fn named_intersections_drive_native_parts_and_track_source_and_hint_edits() {
    let doc = intersection_document();
    let session = Session::new().unwrap();
    let mut instance = part(&doc.family);
    let first = instance.regenerate(&session).unwrap();
    let inspect = |instance: &PartInstance<'_>, built: &GeneratedResult<'_>| {
        let snapshots = instance.resolved_sketches(&session, built).unwrap();
        let sketch = &snapshots["profile"].as_ref().unwrap().sketch;
        let solution = sketch
            .solve(&instance.resolved_parameters().unwrap())
            .unwrap();
        assert!(solution.solved && solution.free_degrees == 0);
        let p = solution.points["reference:intersection:right"];
        let c = solution.points["center"];
        assert!((p.x - c.x).hypot(p.y - c.y) < 1e-8);
        assert!((p.y - 6.).abs() < 1e-8);
        for source in ["source", "guide-source"] {
            let closest = session
                .curve_closest_point(built.shape(source).unwrap(), Vec3::new(p.x, p.y, 0.))
                .unwrap();
            assert!((closest.x - p.x).hypot(closest.y - p.y).hypot(closest.z) < 1e-7);
        }
        assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
        assert!(
            (session.volume(built.shape("body").unwrap()).unwrap() - 16. * std::f64::consts::PI)
                .abs()
                < 1e-7
        );
        p.x
    };
    assert!((inspect(&instance, &first) - 8.).abs() < 1e-8);
    instance.overrides.insert(
        "radius".into(),
        ParameterValue::Scalar(Quantity::length(12., LengthUnit::Millimeter)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    assert!((inspect(&instance, &edited) - 108f64.sqrt()).abs() < 1e-8);
    assert!(edited.regeneration.rebuilt.contains(&"profile".to_owned()));
    let count = session.shape_count().unwrap();
    instance.overrides.insert(
        "intersection_shift".into(),
        ParameterValue::Scalar(Quantity::length(10., LengthUnit::Millimeter)),
    );
    let err = instance
        .regenerate_incremental(&session, &edited)
        .err()
        .unwrap();
    assert!(err.message.contains("found no crossing"), "{}", err.message);
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    drop(first);
    drop(edited);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn projected_intersection_guards_reject_ambiguity_invalid_hints_and_reference_budgets() {
    let doc = intersection_document();
    let session = Session::new().unwrap();
    for case in 0..10 {
        let mut f = doc.family.clone();
        let sk = profile(&mut f);
        let expected = match case {
            0 => {
                sk.projections[0].intersections[0].maximum_distance = mm(30.);
                "multiple crossings"
            }
            1 => {
                sk.projections[0].intersections[0].maximum_distance = mm(0.);
                "must be positive"
            }
            2 => {
                sk.projections[0].intersections[0].near = vector(8., 6., 1., Dimension::Length);
                "z equal to zero"
            }
            3 => {
                sk.projections[0].intersections[1].id = "right".into();
                "nonempty and unique"
            }
            4 => {
                sk.projections[0].intersections[0].other = "missing".into();
                "another declared projection"
            }
            5 => {
                sk.projections[0].intersections[0].other = "reference".into();
                "distinct projections"
            }
            6 => {
                sk.projections[0].intersections[0].id.clear();
                "nonempty and unique"
            }
            7 => {
                sk.points
                    .push(point("reference:intersection:right", 0., 0.));
                "unique"
            }
            8 => {
                let reference = sk.projections[0].intersections[0].clone();
                sk.projections[0].intersections = (0..1001)
                    .map(|i| SketchProjectionIntersection {
                        id: format!("ref-{i}"),
                        ..reference.clone()
                    })
                    .collect();
                "exceed 1000"
            }
            _ => {
                let mut projection = sk.projections[0].clone();
                let reference = projection.intersections[0].clone();
                projection.intersections = (0..1000)
                    .map(|i| SketchProjectionIntersection {
                        id: format!("ref-{i}"),
                        ..reference.clone()
                    })
                    .collect();
                sk.projections = (0..11)
                    .map(|i| SketchProjection {
                        id: format!("projected-{i}"),
                        ..projection.clone()
                    })
                    .chain(std::iter::once(sk.projections[1].clone()))
                    .collect();
                "exceed 1000"
            }
        };
        let err = part(&f).regenerate(&session).err().unwrap();
        assert!(err.message.contains(expected), "{}", err.message);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
#[test]
fn projected_intersection_hints_select_opposite_branch_without_rebuilding_sources() {
    let doc = intersection_document();
    let session = Session::new().unwrap();
    // Keep only right: the left reference's shared hint parameter would otherwise leave the curve.
    let mut family = doc.family.clone();
    profile(&mut family).projections[0]
        .intersections
        .truncate(1);
    let mut instance = part(&family);
    let first2 = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "intersection_shift".into(),
        ParameterValue::Scalar(Quantity::length(-18., LengthUnit::Millimeter)),
    );
    let edited = instance.regenerate_incremental(&session, &first2).unwrap();
    assert_eq!(edited.regeneration.rebuilt, vec!["profile", "body"]);
    let snapshots = instance.resolved_sketches(&session, &edited).unwrap();
    let solved = snapshots["profile"]
        .as_ref()
        .unwrap()
        .sketch
        .solve(&instance.resolved_parameters().unwrap())
        .unwrap();
    assert!((solved.points["center"].x + 8.).abs() < 1e-8);
    drop(first2);
    drop(edited);
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn native_intersection_document(ellipse: bool) -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(if ellipse {
        include_str!("../../../../tools/model/projected-ellipse-intersection-pad.request.json")
    } else {
        include_str!("../../../../tools/model/projected-spline-intersection-pad.request.json")
    })
    .unwrap();
    ModelDocument::from_json(&request["model"].to_string()).unwrap()
}
#[test]
fn native_spline_and_ellipse_intersections_drive_parts_and_track_source_edits() {
    let session = Session::new().unwrap();
    for ellipse in [false, true] {
        let doc = native_intersection_document(ellipse);
        let mut instance = part(&doc.family);
        let first = instance.regenerate(&session).unwrap();
        let inspect = |instance: &PartInstance<'_>, built: &GeneratedResult<'_>| {
            let snapshots = instance.resolved_sketches(&session, built).unwrap();
            let sk = &snapshots["profile"].as_ref().unwrap().sketch;
            let solved = sk.solve(&instance.resolved_parameters().unwrap()).unwrap();
            assert!(solved.solved && solved.free_degrees == 0);
            let p = solved.points["reference:intersection:right"];
            assert!(
                (p.x - solved.points["center"].x).hypot(p.y - solved.points["center"].y) < 1e-8
            );
            for id in ["source", "guide-source"] {
                let q = session
                    .curve_closest_point(built.shape(id).unwrap(), Vec3::new(p.x, p.y, 0.))
                    .unwrap();
                assert!((p.x - q.x).hypot(p.y - q.y).hypot(q.z) < 1e-7);
            }
            assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
            assert!(
                (session.volume(built.shape("body").unwrap()).unwrap()
                    - 16. * std::f64::consts::PI)
                    .abs()
                    < 1e-7
            );
            p
        };
        let a = inspect(&instance, &first);
        if ellipse {
            assert!((a.x - 5.).abs() < 1e-7 && (a.y - 27f64.sqrt()).abs() < 1e-7);
        } else {
            assert!((a.x - 20. - 160f64.sqrt()).abs() < 1e-7);
        }
        instance.overrides.insert(
            if ellipse { "radius" } else { "width" }.into(),
            ParameterValue::Scalar(Quantity::length(
                if ellipse { 12. } else { 50. },
                LengthUnit::Millimeter,
            )),
        );
        let mut edited = instance.regenerate_incremental(&session, &first).unwrap();
        let b = inspect(&instance, &edited);
        assert!((a.x - b.x).hypot(a.y - b.y) > 0.1);
        if !ellipse {
            instance.overrides.insert(
                "weight".into(),
                ParameterValue::Scalar(Quantity::scalar(0.75)),
            );
            let weighted = instance.regenerate_incremental(&session, &edited).unwrap();
            let c = inspect(&instance, &weighted);
            assert!((c.x - b.x).abs() > 0.1);
            drop(edited);
            edited = weighted;
        }
        let before = session.shape_count().unwrap();
        instance.overrides.insert(
            "match_radius".into(),
            ParameterValue::Scalar(Quantity::length(
                if ellipse { 30. } else { 50. },
                LengthUnit::Millimeter,
            )),
        );
        let error = instance
            .regenerate_incremental(&session, &edited)
            .err()
            .unwrap();
        assert!(
            error.message.contains("multiple crossings"),
            "{}",
            error.message
        );
        assert_eq!(session.shape_count().unwrap(), before);
        assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
        drop(first);
        drop(edited);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
#[test]
fn native_chain_and_spline_intersections_resolve_in_tilted_sketch_coordinates() {
    let session = Session::new().unwrap();
    for chain in [false, true] {
        let mut doc = native_intersection_document(false);
        if chain {
            let FeatureOperation::SketchOpenWire { sketch } = &doc.family.features[0].operation
            else {
                panic!()
            };
            let mut source = sketch.clone();
            source.lines.push(SketchLine {
                id: "closing".into(),
                start: "c".into(),
                end: "a".into(),
            });
            source.profile.push("closing".into());
            doc.family.features[0].operation = FeatureOperation::SketchWire { sketch: source };
        }
        let sk = profile(&mut doc.family);
        let tilt = 3f64.sqrt() / 2.;
        sk.y_axis = vector(0., tilt, 0.5, Dimension::Scalar);
        if chain {
            sk.projections[0].kind = SketchProjectionKind::Chain;
        }
        for reference in &mut sk.projections[0].intersections {
            reference.near = VectorExpr::Components {
                x: if reference.id == "right" {
                    ScalarExpr::Parameter("width".into())
                } else {
                    mm(0.)
                },
                y: ScalarExpr::Multiply(
                    Box::new(ScalarExpr::Parameter("offset".into())),
                    Box::new(ScalarExpr::Literal(Quantity::scalar(tilt))),
                ),
                z: mm(0.),
            };
        }
        let instance = part(&doc.family);
        let built = instance.regenerate(&session).unwrap();
        let snapshots = instance.resolved_sketches(&session, &built).unwrap();
        let solved = snapshots["profile"]
            .as_ref()
            .unwrap()
            .sketch
            .solve(&instance.resolved_parameters().unwrap())
            .unwrap();
        assert!(solved.solved && solved.free_degrees == 0);
        let p = solved.points["reference:intersection:right"];
        assert!((p.x - 20. - 160f64.sqrt()).abs() < 1e-7 && (p.y - 6. * tilt).abs() < 1e-7);
        assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
        drop(built);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
#[test]
fn overlapping_native_projection_curves_reject_without_leaking_handles() {
    let mut doc = native_intersection_document(false);
    let sk = profile(&mut doc.family);
    sk.projections[1].input = "source".into();
    sk.projections[1].kind = SketchProjectionKind::Spline;
    let session = Session::new().unwrap();
    let error = part(&doc.family).regenerate(&session).err().unwrap();
    assert!(error.message.contains("overlap"), "{}", error.message);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn projected_line_spline_touch_drives_a_solid_and_rejects_missing_or_double_contacts() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-tangent-intersection-pad.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let mut instance = part(&doc.family);
    let first = instance.regenerate(&session).unwrap();
    let inspect = |instance: &PartInstance<'_>, built: &GeneratedResult<'_>| {
        let snapshots = instance.resolved_sketches(&session, built).unwrap();
        let sk = &snapshots["profile"].as_ref().unwrap().sketch;
        let solution = sk.solve(&instance.resolved_parameters().unwrap()).unwrap();
        assert!(solution.solved && solution.free_degrees == 0);
        let p = solution.points["reference:intersection:touch"];
        let c = solution.points["center"];
        assert!((p.x - c.x).hypot(p.y - c.y) < 1e-8);
        for id in ["source", "guide-source"] {
            let q = session
                .curve_closest_point(built.shape(id).unwrap(), Vec3::new(p.x, p.y, 0.))
                .unwrap();
            assert!((p.x - q.x).hypot(p.y - q.y) < 1e-7);
        }
        assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
        assert!(
            (session.volume(built.shape("body").unwrap()).unwrap() - 16. * std::f64::consts::PI)
                .abs()
                < 1e-7
        );
        p
    };
    let p = inspect(&instance, &first);
    assert!((p.x - 20.).abs() < 1e-8 && (p.y - 10.).abs() < 1e-8);
    instance.overrides.insert(
        "width".into(),
        ParameterValue::Scalar(Quantity::length(50., LengthUnit::Millimeter)),
    );
    instance.overrides.insert(
        "rise".into(),
        ParameterValue::Scalar(Quantity::length(24., LengthUnit::Millimeter)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    let p = inspect(&instance, &edited);
    assert!((p.x - 25.).abs() < 1e-8 && (p.y - 12.).abs() < 1e-8);
    let before = session.shape_count().unwrap();
    for gap in [1e-6, -1e-6] {
        instance.overrides.insert(
            "offset".into(),
            ParameterValue::Scalar(Quantity::length(gap, LengthUnit::Millimeter)),
        );
        let error = instance
            .regenerate_incremental(&session, &edited)
            .err()
            .unwrap();
        assert!(
            error.message.contains(if gap > 0. {
                "found no crossing"
            } else {
                "multiple crossings"
            }),
            "{}",
            error.message
        );
        assert_eq!(session.shape_count().unwrap(), before);
        assert!(session.is_valid(edited.shape("body").unwrap()).unwrap());
    }
    drop(first);
    drop(edited);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn source_linked_projection_refinements_preserve_volumes_and_stable_references_after_edits() {
    for source in [
        include_str!("../../../../tools/model/projected-spline-pad.request.json"),
        include_str!("../../../../tools/model/projected-chain-pad.request.json"),
        include_str!("../../../../tools/model/projected-junction-pad.request.json"),
        include_str!("../../../../tools/model/projected-spline-intersection-pad.request.json"),
    ] {
        let request: serde_json::Value = serde_json::from_str(source).unwrap();
        let original = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let mut refined = original.clone();
        let target = profile(&mut refined.family);
        for p in &mut target.projections {
            if matches!(
                p.kind,
                SketchProjectionKind::Spline | SketchProjectionKind::Chain
            ) {
                p.refinements = vec![
                    SketchProjectionRefinement::ElevateDegree { degree: 4 },
                    SketchProjectionRefinement::InsertKnot {
                        fraction: 0.37,
                        multiplicity: 2,
                    },
                ];
                p.refinements.extend(p.refinements.clone());
            }
        }
        let session = Session::new().unwrap();
        for weight in [0.8, 1.4] {
            let mut before = part(&original.family);
            let mut after = part(&refined.family);
            for instance in [&mut before, &mut after] {
                instance.overrides.insert(
                    "weight".into(),
                    ParameterValue::Scalar(Quantity::scalar(weight)),
                );
                instance.overrides.insert(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(43., LengthUnit::Millimeter)),
                );
            }
            let a = before.regenerate(&session).unwrap();
            let b = after.regenerate(&session).unwrap();
            let volume = session.volume(a.shape("body").unwrap()).unwrap();
            let next = session.volume(b.shape("body").unwrap()).unwrap();
            assert!((volume - next).abs() < 1e-7 * volume, "{volume} != {next}");
            let old = before.resolved_sketches(&session, &a).unwrap();
            let new = after.resolved_sketches(&session, &b).unwrap();
            let old = &old["profile"].as_ref().unwrap().sketch;
            let new = &new["profile"].as_ref().unwrap().sketch;
            let parameters = after.resolved_parameters().unwrap();
            let x = old.solve(&parameters).unwrap();
            let y = new.solve(&parameters).unwrap();
            for (id, p) in &x.points {
                if !id.contains(":pole:") {
                    let q = y.points[id];
                    assert!((p.x - q.x).hypot(p.y - q.y) < 1e-7, "{id}");
                }
            }
            for spline in &new.splines {
                assert_eq!(spline.basis.as_ref().unwrap().degree, 4);
            }
            drop(a);
            drop(b);
            assert_eq!(session.shape_count().unwrap(), 0);
        }
    }
}

#[test]
fn projected_refinement_reimports_changed_source_basis_without_reducing_its_degree() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-spline-pad.request.json"
    ))
    .unwrap();
    let original = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let mut changed = original.clone();
    let FeatureOperation::SketchOpenWire { sketch } = &mut changed.family.features[0].operation
    else {
        panic!("source spline")
    };
    let basis = sketch.splines[0].basis.as_mut().unwrap();
    basis.degree = 4;
    basis.knots = vec![0., 3.];
    basis.multiplicities = vec![5, 5];
    let mut refined = changed.clone();
    profile(&mut refined.family).projections[0].refinements = vec![
        SketchProjectionRefinement::ElevateDegree { degree: 3 },
        SketchProjectionRefinement::InsertKnot {
            fraction: 0.5,
            multiplicity: 1,
        },
    ];
    let session = Session::new().unwrap();
    let old = part(&original.family).regenerate(&session).unwrap();
    let source = part(&changed.family).regenerate(&session).unwrap();
    let next = part(&refined.family).regenerate(&session).unwrap();
    let original_volume = session.volume(old.shape("body").unwrap()).unwrap();
    let expected = session.volume(source.shape("body").unwrap()).unwrap();
    let actual = session.volume(next.shape("body").unwrap()).unwrap();
    assert!((original_volume - expected).abs() > 1e-3);
    assert!((actual - expected).abs() < 1e-7 * expected);
    let snapshots = part(&refined.family)
        .resolved_sketches(&session, &next)
        .unwrap();
    let spline = &snapshots["profile"].as_ref().unwrap().sketch.splines[0];
    assert_eq!(spline.basis.as_ref().unwrap().degree, 4);
    assert_eq!(spline.points.len(), 6);
    assert_eq!(profile(&mut refined.family).projections[0].input, "source");
}
#[test]
fn projected_refinement_rejects_invalid_recipes_and_pole_budgets_without_handles() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/projected-spline-pad.request.json"
    ))
    .unwrap();
    let original = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    for operation in [
        SketchProjectionRefinement::InsertKnot {
            fraction: 0.,
            multiplicity: 1,
        },
        SketchProjectionRefinement::InsertKnot {
            fraction: 0.5,
            multiplicity: 0,
        },
        SketchProjectionRefinement::ElevateDegree { degree: 26 },
    ] {
        let mut invalid = original.clone();
        profile(&mut invalid.family).projections[0].refinements = vec![operation];
        assert!(profile(&mut invalid.family).validate_structure().is_err());
    }
    let mut invalid = original.clone();
    profile(&mut invalid.family).projections[0].refinements =
        vec![SketchProjectionRefinement::InsertKnot {
            fraction: 0.5,
            multiplicity: 3,
        }];
    let session = Session::new().unwrap();
    let error = part(&invalid.family).regenerate(&session).err().unwrap();
    assert!(
        error.message.contains("exceeds its current degree"),
        "{error}"
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    let target = profile(&mut invalid.family);
    target.projections[0].refinements =
        vec![SketchProjectionRefinement::ElevateDegree { degree: 25 }];
    target.projections[0].refinements.extend((1..32).map(|i| {
        SketchProjectionRefinement::InsertKnot {
            fraction: i as f64 / 32.,
            multiplicity: 25,
        }
    }));
    let error = part(&invalid.family).regenerate(&session).err().unwrap();
    assert!(error.message.contains("256"), "{error}");
    assert_eq!(session.shape_count().unwrap(), 0);
}
