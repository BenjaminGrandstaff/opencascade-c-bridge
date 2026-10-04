//! Semantic edge and face selectors and kernel failure diagnostics.

use super::*;

#[test]
fn curvature_bounds_classify_only_proven_decisions() {
    let bounds = |minimum_lower_bound, minimum, maximum, maximum_upper_bound| CurvatureExtrema {
        minimum,
        minimum_lower_bound,
        maximum,
        maximum_upper_bound,
        is_exact: false,
    };
    // Curvature range [0.4, 0.6] is radius range [1.667, 2.5].
    let inside = bounds(0.45, 0.46, 0.54, 0.55);
    assert_eq!(
        classify_curvature_bounds(&inside, 0.4, 0.6, true),
        Some(true)
    );
    assert_eq!(
        classify_curvature_bounds(&inside, 0.4, 0.6, false),
        Some(true)
    );
    let straddling_top = bounds(0.45, 0.46, 0.59, 0.61);
    assert_eq!(
        classify_curvature_bounds(&straddling_top, 0.4, 0.6, true),
        None
    );
    assert_eq!(
        classify_curvature_bounds(&straddling_top, 0.4, 0.6, false),
        Some(true)
    );
    let exceeds = bounds(0.45, 0.46, 0.61, 0.62);
    assert_eq!(
        classify_curvature_bounds(&exceeds, 0.4, 0.6, true),
        Some(false)
    );
    let near_below = bounds(0.35, 0.36, 0.39, 0.41);
    assert_eq!(
        classify_curvature_bounds(&near_below, 0.4, 0.6, false),
        None
    );
    let below = bounds(0.35, 0.36, 0.38, 0.39);
    assert_eq!(
        classify_curvature_bounds(&below, 0.4, 0.6, false),
        Some(false)
    );
    let straight = bounds(0.0, 0.0, 0.0, 0.0);
    assert_eq!(
        classify_curvature_bounds(&straight, 0.4, 0.6, false),
        Some(false)
    );
}

#[test]
fn bounded_curvature_selector_matches_exact_and_spline_edges() {
    let session = Session::new().unwrap();
    let cylinder = session
        .create_cylinder(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 2.0, 5.0)
        .unwrap();
    let rims = select_edges_by_bounded_curvature_radius(&session, &cylinder, 1.9, 2.1, 1e-9, true)
        .unwrap();
    assert_eq!(rims.len(), 2);
    cleanup_shapes(&session, rims);

    let compound = session
        .load_brep(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/curvature_edges.brep"
        ))
        .unwrap();
    let before = session.shape_count().unwrap();
    let arcs = select_edges_by_bounded_curvature_radius(&session, &compound, 1.9, 2.1, 1e-9, true)
        .unwrap();
    let rational_circle = session.subshape(&compound, ShapeType::Edge, 1).unwrap();
    assert_eq!(arcs.len(), 1);
    assert!(session.is_same(&arcs[0], &rational_circle).unwrap());
    cleanup_shapes(&session, arcs);
    let _ = session.remove(rational_circle);

    // The parabola spans radius 0.5..5.59, so it overlaps but is not contained.
    let overlapping =
        select_edges_by_bounded_curvature_radius(&session, &compound, 4.0, 4.5, 1e-9, false)
            .unwrap();
    let parabola = session.subshape(&compound, ShapeType::Edge, 0).unwrap();
    assert!(
        overlapping
            .iter()
            .any(|edge| session.is_same(edge, &parabola).unwrap())
    );
    cleanup_shapes(&session, overlapping);

    let _ = session.remove(parabola);

    // Put the lower radius bound strictly inside the spline's loose
    // maximum-curvature gap: undecidable at 0.5, decided at 1e-6.
    let spline = session.subshape(&compound, ShapeType::Edge, 2).unwrap();
    let loose = session.edge_curvature_extrema(&spline, 0.5).unwrap();
    let tight = session.edge_curvature_extrema(&spline, 1e-6).unwrap();
    let _ = session.remove(spline);
    let boundary_curvature = 0.5 * (loose.maximum + loose.maximum_upper_bound);
    assert!(loose.maximum < boundary_curvature && boundary_curvature < loose.maximum_upper_bound);
    assert!(tight.maximum_upper_bound < boundary_curvature);
    let boundary = 1.0 / boundary_curvature;
    let error =
        select_edges_by_bounded_curvature_radius(&session, &compound, boundary, 100.0, 0.5, true)
            .unwrap_err();
    assert!(error.message.contains("edge 2"), "{error}");
    assert!(error.message.contains("tighten relative_tolerance"));
    assert_eq!(session.shape_count().unwrap(), before);
    // Rational circle, spline, ellipse arc, and the conic parabola
    // (radius 1..11.2) and hyperbola (radius 1.33..17.1); not the Bezier
    // parabola (radius 0.5 at its vertex) or the line.
    let resolved =
        select_edges_by_bounded_curvature_radius(&session, &compound, boundary, 100.0, 1e-6, true)
            .unwrap();
    assert_eq!(resolved.len(), 5);
    cleanup_shapes(&session, resolved);
    assert_eq!(session.shape_count().unwrap(), before);

    for (minimum, maximum, tolerance) in [
        (0.0, 1.0, 1e-6),
        (2.0, 1.0, 1e-6),
        (1.0, 2.0, 0.0),
        (1.0, 2.0, 2.0),
    ] {
        assert!(
            select_edges_by_bounded_curvature_radius(
                &session, &compound, minimum, maximum, tolerance, true
            )
            .is_err()
        );
    }
}

#[test]
fn semantic_history_selector_survives_serialization_and_drives_fillet() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition.features.push(FeatureDefinition {
        id: "filleted".into(),
        operation: FeatureOperation::Fillet {
            input: "placed".into(),
            edges: vec![EdgeSelector::History {
                source_feature: "body".into(),
                source: Box::new(EdgeSelector::NearestCenter {
                    target: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        15.0,
                        LengthUnit::Millimeter,
                    )),
                    maximum_distance: ScalarExpr::Literal(Quantity::length(
                        0.01,
                        LengthUnit::Millimeter,
                    )),
                }),
                relation: SemanticHistoryRelation::Modified,
            }],
            radius: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
        },
    });
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let json = ModelDocument::from_graph(&graph).to_json_pretty().unwrap();
    let loaded = ModelDocument::from_json(&json).unwrap();
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
    assert!(json.contains("source_feature"));

    let loaded_graph = loaded.instance_graph().unwrap();
    let instance = loaded_graph.resolve("part").unwrap();
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();
    let filleted = result.shape("filleted").unwrap();
    assert!(session.is_valid(filleted).unwrap());
    assert!(session.volume(filleted).unwrap() < 6_000.0);
    assert_eq!(session.shape_count().unwrap(), 3);
}

fn nearest_edge(x: f64, y: f64, z: f64) -> EdgeSelector {
    EdgeSelector::NearestCenter {
        target: VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter)),
        maximum_distance: ScalarExpr::Literal(Quantity::length(0.01, LengthUnit::Millimeter)),
    }
}

#[test]
fn kernel_failures_name_the_feature_and_selector_at_fault() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    // A 12 mm round fits the 10 mm edge between the 20 mm and 30 mm faces
    // but not the 30 mm edge beside the 10 mm face.
    definition.features.push(FeatureDefinition {
        id: "round".into(),
        operation: FeatureOperation::Fillet {
            input: "body".into(),
            edges: vec![nearest_edge(5.0, 0.0, 30.0), nearest_edge(10.0, 20.0, 15.0)],
            radius: ScalarExpr::Literal(Quantity::length(12.0, LengthUnit::Millimeter)),
        },
    });
    let instance = PartInstance {
        id: "rounded".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let error = instance.regenerate(&session).err().unwrap();
    assert!(!error.diagnostics.is_empty(), "{}", error.message);
    for diagnostic in &error.diagnostics {
        assert_eq!(diagnostic.feature, "round");
        assert_eq!(diagnostic.kind, DiagnosticKind::FilletEdge);
        assert_eq!(diagnostic.selection, Some(1));
        assert_eq!(diagnostic.selector, Some(1));
    }
    assert!(error.message.starts_with("feature 'round': "));
    assert!(
        error.message.ends_with("; at fault: edge selector 1"),
        "{}",
        error.message
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn boolean_diagnostics_name_the_input_at_fault() {
    let diagnostic = |operand| FeatureDiagnostic {
        feature: String::new(),
        kind: DiagnosticKind::BooleanAlert,
        code: 0,
        name: "BOPAlgo_AlertSelfInterferingShape".into(),
        selection: operand,
        selector: None,
        input: None,
    };
    let error = ModelError {
        message: "kernel error: cut operation failed".into(),
        diagnostics: vec![diagnostic(Some(1)), diagnostic(None), diagnostic(Some(1))],
    }
    .locate_operands(["stock", "cutter"])
    .in_feature("pocket");
    assert_eq!(
        error.message,
        "feature 'pocket': kernel error: cut operation failed; at fault: input 'cutter'"
    );
    let inputs = error
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.feature.as_str(),
                diagnostic.input.as_deref(),
                diagnostic.selection,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        inputs,
        vec![
            ("pocket", Some("cutter"), None),
            ("pocket", None, None),
            ("pocket", Some("cutter"), None),
        ]
    );
}

#[test]
fn ambiguous_semantic_selector_fails_and_cleans_temporary_shapes() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition.features.push(FeatureDefinition {
        id: "ambiguous-fillet".into(),
        operation: FeatureOperation::Fillet {
            input: "body".into(),
            edges: vec![EdgeSelector::NearestCenter {
                target: VectorExpr::Literal(VectorQuantity::lengths(
                    5.0,
                    10.0,
                    15.0,
                    LengthUnit::Millimeter,
                )),
                maximum_distance: ScalarExpr::Literal(Quantity::length(
                    20.0,
                    LengthUnit::Millimeter,
                )),
            }],
            radius: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
        },
    });
    let instance = PartInstance {
        id: "ambiguous".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();

    let error = instance.regenerate(&session).err().unwrap();
    assert!(error.message.contains("ambiguous"));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn extremum_selector_chamfers_multiple_edges_without_topology_indices() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition.features.push(FeatureDefinition {
        id: "top-chamfer".into(),
        operation: FeatureOperation::Chamfer {
            input: "body".into(),
            edges: vec![EdgeSelector::AtExtreme {
                axis: CoordinateAxis::Z,
                extremum: Extremum::Maximum,
                tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
            }],
            distance: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
        },
    });
    let instance = PartInstance {
        id: "multi-edge".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();

    let chamfered = result.shape("top-chamfer").unwrap();
    assert!(session.is_valid(chamfered).unwrap());
    assert!(session.volume(chamfered).unwrap() < 6_000.0);
    assert_eq!(session.shape_count().unwrap(), 3);
}

#[test]
fn face_extremum_selector_opens_the_top_of_a_hollow_part() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition.features.push(FeatureDefinition {
        id: "open-shell".into(),
        operation: FeatureOperation::Hollow {
            input: "body".into(),
            faces: vec![FaceSelector::AtExtreme {
                axis: CoordinateAxis::Z,
                extremum: Extremum::Maximum,
                tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
            }],
            thickness: ScalarExpr::Literal(Quantity::length(-1.0, LengthUnit::Millimeter)),
            tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
        },
    });
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("hollow", HashMap::new(), "test").unwrap();
    let json = ModelDocument::from_graph(&graph).to_json_pretty().unwrap();
    let loaded = ModelDocument::from_json(&json).unwrap();
    let loaded_graph = loaded.instance_graph().unwrap();
    let instance = loaded_graph.resolve("hollow").unwrap();
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();

    let hollow = result.shape("open-shell").unwrap();
    assert!(session.is_valid(hollow).unwrap());
    assert!(session.volume(hollow).unwrap() < 6_000.0);
    assert_eq!(session.shape_count().unwrap(), 3);
}

#[test]
fn orientation_selector_tracks_a_face_after_feature_rotation() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition.features.push(FeatureDefinition {
        id: "rotated".into(),
        operation: FeatureOperation::Rotate {
            input: "body".into(),
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                0.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
            axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
        },
    });
    definition.features.push(FeatureDefinition {
        id: "oriented-shell".into(),
        operation: FeatureOperation::Hollow {
            input: "rotated".into(),
            faces: vec![FaceSelector::NormalAligned {
                direction: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999)),
            }],
            thickness: ScalarExpr::Literal(Quantity::length(-1.0, LengthUnit::Millimeter)),
            tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
        },
    });
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("oriented", HashMap::new(), "test").unwrap();
    let json = ModelDocument::from_graph(&graph).to_json_pretty().unwrap();
    let loaded = ModelDocument::from_json(&json).unwrap();
    let loaded_graph = loaded.instance_graph().unwrap();
    let instance = loaded_graph.resolve("oriented").unwrap();
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();

    assert!(
        session
            .is_valid(result.shape("oriented-shell").unwrap())
            .unwrap()
    );
    assert_eq!(session.shape_count().unwrap(), 4);
}

#[test]
fn adjacency_selector_finds_face_shared_by_semantic_edge_set() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition.features.push(FeatureDefinition {
        id: "adjacent-shell".into(),
        operation: FeatureOperation::Hollow {
            input: "body".into(),
            faces: vec![FaceSelector::AdjacentToEdges {
                edges: Box::new(EdgeSelector::AtExtreme {
                    axis: CoordinateAxis::Z,
                    extremum: Extremum::Maximum,
                    tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
                }),
                minimum_count: 4,
            }],
            thickness: ScalarExpr::Literal(Quantity::length(-1.0, LengthUnit::Millimeter)),
            tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
        },
    });
    let instance = PartInstance {
        id: "adjacent".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();

    assert!(
        session
            .is_valid(result.shape("adjacent-shell").unwrap())
            .unwrap()
    );
    assert_eq!(session.shape_count().unwrap(), 3);
}

#[test]
fn tangency_selector_finds_fillet_neighbors_without_topology_indices() {
    let session = Session::new().unwrap();
    let box_shape = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let edge = session.subshape(&box_shape, ShapeType::Edge, 0).unwrap();
    let filleted = session.fillet(&box_shape, &[&edge], 1.0).unwrap();
    let faces = (0..session.subshape_count(&filleted, ShapeType::Face).unwrap())
        .map(|index| session.subshape(&filleted, ShapeType::Face, index).unwrap())
        .collect::<Vec<_>>();
    let mut source_center = None;
    'pairs: for first in 0..faces.len() {
        for second in first + 1..faces.len() {
            if session
                .faces_are_tangent(&filleted, &faces[first], &faces[second])
                .unwrap()
            {
                source_center = Some(session.center_of_mass(&faces[first]).unwrap());
                break 'pairs;
            }
        }
    }
    let source_center = source_center.expect("fillet must record tangent face continuity");
    cleanup_shapes(&session, faces);

    let selector = FaceSelector::TangentTo {
        faces: Box::new(FaceSelector::NearestCenter {
            target: VectorExpr::Literal(VectorQuantity::lengths(
                source_center.x,
                source_center.y,
                source_center.z,
                LengthUnit::Millimeter,
            )),
            maximum_distance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
        }),
        minimum_count: 1,
    };
    let selected = resolve_face_selector(
        &session,
        &filleted,
        &selector,
        &HashMap::new(),
        &HashMap::new(),
        &Features::default(),
    )
    .unwrap();
    assert!(!selected.is_empty());
    cleanup_shapes(&session, selected);
    assert_eq!(session.shape_count().unwrap(), 3);
}

#[test]
fn selector_composition_performs_topological_set_operations() {
    let session = Session::new().unwrap();
    let box_shape = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let longest = EdgeSelector::Longest {
        allow_ties: true,
        relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-9)),
    };
    let minimum_x = EdgeSelector::AtExtreme {
        axis: CoordinateAxis::X,
        extremum: Extremum::Minimum,
        tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
    };
    let intersection = EdgeSelector::Intersection(vec![longest.clone(), minimum_x.clone()]);
    let difference = EdgeSelector::Difference {
        base: Box::new(longest),
        subtract: Box::new(minimum_x),
    };
    let edges = resolve_edge_selector(
        &session,
        &box_shape,
        &EdgeSelector::Union(vec![intersection, difference]),
        &HashMap::new(),
        &HashMap::new(),
        &Features::default(),
    )
    .unwrap();
    assert_eq!(edges.len(), 4);
    cleanup_shapes(&session, edges);

    let largest = FaceSelector::LargestArea {
        planar_only: true,
        allow_ties: true,
        relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-9)),
    };
    let maximum_x = FaceSelector::AtExtreme {
        axis: CoordinateAxis::X,
        extremum: Extremum::Maximum,
        tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
    };
    let faces = resolve_face_selector(
        &session,
        &box_shape,
        &FaceSelector::Union(vec![
            FaceSelector::Intersection(vec![largest.clone(), maximum_x.clone()]),
            FaceSelector::Difference {
                base: Box::new(largest),
                subtract: Box::new(maximum_x),
            },
        ]),
        &HashMap::new(),
        &HashMap::new(),
        &Features::default(),
    )
    .unwrap();
    assert_eq!(faces.len(), 2);
    cleanup_shapes(&session, faces);

    let error = resolve_edge_selector(
        &session,
        &box_shape,
        &EdgeSelector::Union(Vec::new()),
        &HashMap::new(),
        &HashMap::new(),
        &Features::default(),
    )
    .err()
    .unwrap();
    assert!(error.message.contains("requires at least one"));
    assert_eq!(session.shape_count().unwrap(), 1);
}

#[test]
fn longest_edge_selector_drives_chamfer_and_rejects_disallowed_ties() {
    let mut definition = family(RequirementPriority::Required, 100_000.0);
    definition.requirements.clear();
    definition.features.push(FeatureDefinition {
        id: "long-edge-chamfer".into(),
        operation: FeatureOperation::Chamfer {
            input: "body".into(),
            edges: vec![EdgeSelector::Longest {
                allow_ties: true,
                relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-9)),
            }],
            distance: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
        },
    });
    let instance = PartInstance {
        id: "longest-edges".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();
    let chamfered = result.shape("long-edge-chamfer").unwrap();
    assert!(session.is_valid(chamfered).unwrap());
    assert!(session.volume(chamfered).unwrap() < 6_000.0);

    let direct_session = Session::new().unwrap();
    let box_shape = direct_session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let error = select_longest_edges(&direct_session, &box_shape, false, 1e-9)
        .err()
        .unwrap();
    assert!(error.message.contains("ambiguous across 4 edges"));
    assert_eq!(direct_session.shape_count().unwrap(), 1);
}

#[test]
fn circular_and_curvature_radius_selectors_drive_cylinder_chamfers() {
    let definition = FamilyDefinition {
        references: Vec::new(),
        id: "CylinderSelectorFamily".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        features: vec![
            FeatureDefinition {
                id: "cylinder".into(),
                operation: FeatureOperation::Cylinder {
                    origin: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                    axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    radius: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
                    height: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
                },
            },
            FeatureDefinition {
                id: "rim-chamfer".into(),
                operation: FeatureOperation::Chamfer {
                    input: "cylinder".into(),
                    edges: vec![EdgeSelector::CircularRadius {
                        minimum: ScalarExpr::Literal(Quantity::length(1.9, LengthUnit::Millimeter)),
                        maximum: ScalarExpr::Literal(Quantity::length(2.1, LengthUnit::Millimeter)),
                    }],
                    distance: ScalarExpr::Literal(Quantity::length(0.25, LengthUnit::Millimeter)),
                },
            },
            FeatureDefinition {
                id: "curvature-chamfer".into(),
                operation: FeatureOperation::Chamfer {
                    input: "cylinder".into(),
                    edges: vec![EdgeSelector::CurvatureRadius {
                        minimum: ScalarExpr::Literal(Quantity::length(1.9, LengthUnit::Millimeter)),
                        maximum: ScalarExpr::Literal(Quantity::length(2.1, LengthUnit::Millimeter)),
                    }],
                    distance: ScalarExpr::Literal(Quantity::length(0.25, LengthUnit::Millimeter)),
                },
            },
            FeatureDefinition {
                id: "full-curve-chamfer".into(),
                operation: FeatureOperation::Chamfer {
                    input: "cylinder".into(),
                    edges: vec![EdgeSelector::CurvatureRadiusRange {
                        minimum: ScalarExpr::Literal(Quantity::length(1.9, LengthUnit::Millimeter)),
                        maximum: ScalarExpr::Literal(Quantity::length(2.1, LengthUnit::Millimeter)),
                        sample_count: 9,
                        require_entire_edge: true,
                    }],
                    distance: ScalarExpr::Literal(Quantity::length(0.25, LengthUnit::Millimeter)),
                },
            },
        ],
        datums: Vec::new(),
        requirements: Vec::new(),
    };
    let instance = PartInstance {
        id: "circular-radius".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let session = Session::new().unwrap();
    let result = instance.regenerate(&session).unwrap();
    assert!(
        session
            .is_valid(result.shape("rim-chamfer").unwrap())
            .unwrap()
    );
    assert!(
        session
            .is_valid(result.shape("curvature-chamfer").unwrap())
            .unwrap()
    );
    assert!(
        session
            .is_valid(result.shape("full-curve-chamfer").unwrap())
            .unwrap()
    );

    let straight_session = Session::new().unwrap();
    let box_shape = straight_session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let error = select_edges_by_curvature_radius(&straight_session, &box_shape, 1.0, 10.0)
        .err()
        .unwrap();
    assert!(error.message.contains("no matches"));
    assert_eq!(straight_session.shape_count().unwrap(), 1);

    let ellipse_session = Session::new().unwrap();
    let ellipse = ellipse_session
        .create_ellipse_wire(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 4.0, 2.0)
        .unwrap();
    let entire =
        select_edges_by_curvature_radius_range(&ellipse_session, &ellipse, 0.9, 8.1, 5, true)
            .unwrap();
    assert_eq!(entire.len(), 1);
    cleanup_shapes(&ellipse_session, entire);
    let partial =
        select_edges_by_curvature_radius_range(&ellipse_session, &ellipse, 7.9, 8.1, 5, false)
            .unwrap();
    assert_eq!(partial.len(), 1);
    cleanup_shapes(&ellipse_session, partial);
    assert_eq!(ellipse_session.shape_count().unwrap(), 1);
}

#[test]
fn largest_planar_face_selector_finds_tied_box_faces() {
    let session = Session::new().unwrap();
    let box_shape = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let faces = select_largest_faces(&session, &box_shape, true, true, 1e-9).unwrap();
    assert_eq!(faces.len(), 2);
    for face in &faces {
        assert!(session.face_is_planar(face).unwrap());
        assert!((session.surface_area(face).unwrap() - 600.0).abs() < 1e-9);
    }
    cleanup_shapes(&session, faces);
    assert_eq!(session.shape_count().unwrap(), 1);
}
