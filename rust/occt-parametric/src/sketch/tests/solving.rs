//! Constraint solving: curves, tangency, dimensions, conflicts, and free degrees.

use super::*;

#[test]
fn circle_radius_parameters_drive_exact_faces_in_a_rotated_plane() {
    let mut sketch = rectangle();
    sketch.lines.clear();
    sketch.points = vec![
        point("center", 0.0, 0.0, true),
        point("rim", 3.0, 0.0, false),
    ];
    sketch.circles = vec![SketchCircle {
        id: "circle".into(),
        center: "center".into(),
        rim: "rim".into(),
    }];
    sketch.constraints = vec![SketchConstraint::Distance {
        first: "center".into(),
        second: "rim".into(),
        value: ScalarExpr::Parameter("radius".into()),
    }];
    sketch.origin = VectorExpr::Literal(VectorQuantity::lengths(
        10.0,
        20.0,
        30.0,
        LengthUnit::Millimeter,
    ));
    sketch.x_axis = VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0));
    sketch.y_axis = VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0));
    let mut names = HashSet::new();
    sketch.collect_parameters(&mut names);
    assert!(names.contains("radius"));
    let session = Session::new().unwrap();
    for radius in [2.0, 5.0] {
        let parameters = HashMap::from([(
            "radius".into(),
            ParameterValue::Scalar(Quantity::length(radius, LengthUnit::Millimeter)),
        )]);
        let solution = sketch.solve(&parameters).unwrap();
        assert!(solution.solved);
        assert_eq!(solution.free_degrees, 1); // the rim can rotate
        let face = sketch.face(&session, &parameters).unwrap();
        assert!(
            (session.surface_area(&face).unwrap() - std::f64::consts::PI * radius * radius).abs()
                < 1e-7
        );
        let bounds = session.bounds(&face).unwrap();
        assert!((bounds.min.x - 10.0).abs() < 1e-6);
        assert!((bounds.max.z - (30.0 + radius)).abs() < 1e-6);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn mixed_profiles_keep_minor_and_major_arcs_exact() {
    let session = Session::new().unwrap();
    for clockwise in [false, true] {
        let mut sketch = arc_profile(clockwise);
        // Explicit profile excludes construction geometry.
        sketch.lines.push(SketchLine {
            id: "construction".into(),
            start: "c".into(),
            end: "a".into(),
        });
        let face = sketch.face(&session, &HashMap::new()).unwrap();
        let expected = if clockwise {
            3.0 * std::f64::consts::PI + 2.0
        } else {
            std::f64::consts::PI - 2.0
        };
        assert!((session.surface_area(&face).unwrap() - expected).abs() < 1e-8);
        assert!(session.is_valid(&face).unwrap());
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn implicit_arc_radius_equation_solves_and_reports_conflicts() {
    let mut sketch = arc_profile(false);
    sketch.points[2] = point("b", 0.0, 3.0, false);
    let solved = sketch.solve(&HashMap::new()).unwrap();
    assert!(solved.solved);
    assert_eq!(solved.free_degrees, 1);
    assert!((solved.points["b"].y - 2.0).abs() < 1e-8);
    sketch.points[2].fixed = true;
    let conflict = sketch.solve(&HashMap::new()).unwrap();
    assert!(!conflict.solved);
    assert!((conflict.max_residual - 1.0).abs() < 1e-9);
    let session = Session::new().unwrap();
    assert!(sketch.face(&session, &HashMap::new()).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn solves_line_arc_and_line_circle_tangency_at_contact() {
    for use_circle in [false, true] {
        let mut sketch = arc_profile(false);
        sketch.points.push(point("tip", 2.5, 3.0, false));
        sketch.lines = vec![SketchLine {
            id: "tangent".into(),
            start: "a".into(),
            end: "tip".into(),
        }];
        sketch.profile.clear();
        if use_circle {
            sketch.arcs.clear();
            sketch.circles.push(SketchCircle {
                id: "curve".into(),
                center: "c".into(),
                rim: "a".into(),
            });
        } else {
            sketch.arcs[0].id = "curve".into();
        }
        sketch.constraints = vec![
            SketchConstraint::Tangent {
                first: "curve".into(),
                second: "tangent".into(),
                point: "a".into(),
            },
            SketchConstraint::Distance {
                first: "a".into(),
                second: "tip".into(),
                value: length(3.0),
            },
        ];
        let solved = sketch.solve(&HashMap::new()).unwrap();
        assert!(solved.solved, "{solved:?}");
        assert_eq!(solved.free_degrees, 0);
        assert!((solved.points["tip"].x - 2.0).abs() < 1e-8);
        assert!((solved.points["tip"].y - 3.0).abs() < 1e-8);
        sketch.points.last_mut().unwrap().fixed = true;
        assert!(!sketch.solve(&HashMap::new()).unwrap().solved);
    }
}

#[test]
fn solves_circle_circle_and_arc_arc_tangency() {
    for circles in [false, true] {
        let mut sketch = arc_profile(false);
        sketch.lines.clear();
        sketch.profile.clear();
        sketch
            .points
            .extend([point("c2", 4.0, 0.3, false), point("b2", 4.0, 2.0, false)]);
        if circles {
            sketch.arcs.clear();
            sketch.circles = vec![
                SketchCircle {
                    id: "first".into(),
                    center: "c".into(),
                    rim: "a".into(),
                },
                SketchCircle {
                    id: "second".into(),
                    center: "c2".into(),
                    rim: "a".into(),
                },
            ];
        } else {
            sketch.arcs[0].id = "first".into();
            sketch.arcs.push(SketchArc {
                id: "second".into(),
                center: "c2".into(),
                start: "a".into(),
                end: "b2".into(),
                clockwise: true,
            });
        }
        sketch.constraints = vec![
            SketchConstraint::Tangent {
                first: "first".into(),
                second: "second".into(),
                point: "a".into(),
            },
            SketchConstraint::Distance {
                first: "c2".into(),
                second: "a".into(),
                value: length(2.0),
            },
        ];
        let solved = sketch.solve(&HashMap::new()).unwrap();
        assert!(solved.solved, "{solved:?}");
        assert!((solved.points["c2"].x - 4.0).abs() < 1e-8);
        assert!(solved.points["c2"].y.abs() < 1e-8);
    }
}

#[test]
fn invalid_curves_contacts_and_profiles_are_rejected_without_handles() {
    let session = Session::new().unwrap();
    let base = arc_profile(false);
    let mut cases = Vec::new();
    let mut crossing = rectangle();
    crossing.constraints.clear();
    crossing.points = vec![
        point("p0", 0.0, 0.0, true),
        point("p1", 2.0, 2.0, true),
        point("p2", 0.0, 2.0, true),
        point("p3", 2.0, 0.0, true),
    ];
    cases.push(crossing);
    let mut circle = base.clone();
    circle.circles.push(SketchCircle {
        id: "circle".into(),
        center: "c".into(),
        rim: "a".into(),
    });
    circle.profile = vec!["circle".into(), "arc".into()];
    cases.push(circle.clone());
    circle.profile = vec!["circle".into()];
    circle.arcs.clear();
    circle.points[1] = point("a", 0.0, 0.0, true);
    cases.push(circle);
    let mut sketch = base.clone();
    sketch.arcs[0].center = "missing".into();
    cases.push(sketch);
    let mut sketch = base.clone();
    sketch.arcs[0].end = "a".into();
    cases.push(sketch);
    let mut sketch = base.clone();
    sketch.arcs[0].id = "chord".into();
    cases.push(sketch);
    let mut sketch = base.clone();
    sketch.points[0] = point("c", 2.0, 0.0, true);
    cases.push(sketch);
    for profile in [vec![], vec!["missing"], vec!["arc", "arc"], vec!["arc"]] {
        let mut sketch = base.clone();
        sketch.profile = profile.into_iter().map(str::to_owned).collect();
        cases.push(sketch);
    }
    for (first, second, point) in [
        ("arc", "chord", "c"),
        ("arc", "arc", "a"),
        ("arc", "missing", "a"),
        ("arc", "chord", "missing"),
    ] {
        let mut sketch = base.clone();
        sketch.constraints.push(SketchConstraint::Tangent {
            first: first.into(),
            second: second.into(),
            point: point.into(),
        });
        cases.push(sketch);
    }
    for sketch in cases {
        assert!(
            sketch.face(&session, &HashMap::new()).is_err(),
            "{sketch:?}"
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn schema_twenty_six_round_trips_curves_and_migrates_line_sketches() {
    let mut curved = arc_profile(true);
    curved.circles.push(SketchCircle {
        id: "circle".into(),
        center: "c".into(),
        rim: "a".into(),
    });
    curved.constraints.push(SketchConstraint::Tangent {
        first: "arc".into(),
        second: "circle".into(),
        point: "a".into(),
    });
    let family = |sketch| FamilyDefinition {
        id: "SketchPart".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        features: vec![FeatureDefinition {
            id: "profile".into(),
            operation: FeatureOperation::SketchFace {
                sketch: Box::new(sketch),
            },
        }],
        requirements: Vec::new(),
        datums: Vec::new(),
    };
    for sketch in [rectangle(), curved] {
        let definition = family(sketch.clone());
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
        let session = Session::new().unwrap();
        let generated = PartInstance {
            id: "part".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        }
        .regenerate(&session)
        .unwrap();
        assert!(
            session
                .is_valid(generated.shape("profile").unwrap())
                .unwrap()
        );
        if sketch.arcs.is_empty() {
            let mut old = serde_json::to_value(&document).unwrap();
            old["schema_version"] = serde_json::json!(25);
            fn remove_new_fields(value: &mut serde_json::Value) {
                match value {
                    serde_json::Value::Object(fields) => {
                        if fields.contains_key("points") && fields.contains_key("lines") {
                            for field in ["circles", "arcs", "profile"] {
                                fields.remove(field);
                            }
                        }
                        for value in fields.values_mut() {
                            remove_new_fields(value);
                        }
                    }
                    serde_json::Value::Array(values) => {
                        for value in values {
                            remove_new_fields(value);
                        }
                    }
                    _ => {}
                }
            }
            remove_new_fields(&mut old);
            assert_eq!(
                ModelDocument::from_json(&old.to_string()).unwrap(),
                document
            );
        }
    }
}

#[test]
fn solves_dimensioned_rectangle_and_generates_face() {
    let sketch = rectangle();
    let solution = sketch.solve(&HashMap::new()).unwrap();
    assert!(solution.solved, "{solution:?}");
    assert_eq!(solution.free_degrees, 0);
    assert!(solution.redundant_equations > 0);
    assert!((solution.points["p2"].x - 10.0).abs() < 1e-8);
    assert!((solution.points["p2"].y - 5.0).abs() < 1e-8);

    let session = Session::new().unwrap();
    let face = sketch.face(&session, &HashMap::new()).unwrap();
    assert!((session.surface_area(&face).unwrap() - 50.0).abs() < 1e-7);
    session.remove(face).unwrap();

    let family = FamilyDefinition {
        id: "SketchPart".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        features: vec![FeatureDefinition {
            id: "profile".into(),
            operation: FeatureOperation::SketchFace {
                sketch: Box::new(sketch),
            },
        }],
        requirements: Vec::new(),
        datums: Vec::new(),
    };
    let instance = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let generated = instance.regenerate(&session).unwrap();
    assert!(
        (session
            .surface_area(generated.shape("profile").unwrap())
            .unwrap()
            - 50.0)
            .abs()
            < 1e-7
    );
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
}

#[test]
fn reports_conflicts_free_degrees_and_invalid_profiles() {
    let mut sketch = rectangle();
    sketch.points[1].fixed = true;
    let conflict = sketch.solve(&HashMap::new()).unwrap();
    assert!(!conflict.solved);
    assert!(conflict.max_residual > 0.1);

    let mut free = rectangle();
    free.constraints.clear();
    let solution = free.solve(&HashMap::new()).unwrap();
    assert!(solution.solved);
    assert_eq!(solution.free_degrees, 8);

    let mut open = rectangle();
    open.lines.pop();
    let session = Session::new().unwrap();
    assert!(open.face(&session, &HashMap::new()).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);

    let mut invalid = rectangle();
    invalid.constraints.push(SketchConstraint::Horizontal {
        line: "missing".into(),
    });
    assert!(invalid.solve(&HashMap::new()).is_err());
}

#[test]
fn sparse_solver_keeps_disconnected_and_unconstrained_points_independent() {
    let mut sketch = rectangle();
    sketch.points = vec![
        point("a", 0.0, 0.0, true),
        point("b", 9.0, 2.0, false),
        point("c", 100.0, 50.0, true),
        point("d", 110.0, 53.0, false),
        point("unused", 7.0, 8.0, false),
    ];
    sketch.lines = vec![
        SketchLine {
            id: "ab".into(),
            start: "a".into(),
            end: "b".into(),
        },
        SketchLine {
            id: "cd".into(),
            start: "c".into(),
            end: "d".into(),
        },
    ];
    sketch.constraints = vec![
        SketchConstraint::Horizontal { line: "ab".into() },
        SketchConstraint::Horizontal { line: "cd".into() },
        SketchConstraint::Distance {
            first: "a".into(),
            second: "b".into(),
            value: length(10.0),
        },
        SketchConstraint::Parallel {
            first: "ab".into(),
            second: "ab".into(),
        },
    ];
    let solution = sketch.solve(&HashMap::new()).unwrap();
    assert!(solution.solved, "{solution:?}");
    assert_eq!(solution.free_degrees, 3);
    assert_eq!(solution.redundant_equations, 1);
    assert_eq!(solution.points["unused"], SketchPoint2 { x: 7.0, y: 8.0 });
    assert_eq!(solution.points["d"].x, 110.0);
    assert!((solution.points["d"].y - 50.0).abs() < 1e-9);
    assert!((solution.points["b"].x - 10.0).abs() < 1e-9);
}

#[test]
fn nonfinite_residuals_are_errors_instead_of_successful_solves() {
    let mut sketch = rectangle();
    sketch.points = vec![point("a", -1e308, 0.0, true), point("b", 1e308, 0.0, true)];
    sketch.lines.clear();
    sketch.constraints = vec![SketchConstraint::Coincident {
        first: "a".into(),
        second: "b".into(),
    }];
    assert!(
        sketch
            .solve(&HashMap::new())
            .unwrap_err()
            .message
            .contains("not finite")
    );
}
