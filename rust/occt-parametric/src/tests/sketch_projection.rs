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
    assert!(error.message.contains("spline projection is not supported"));
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
