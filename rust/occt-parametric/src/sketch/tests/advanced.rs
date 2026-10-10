use super::*;
fn number(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::scalar(v))
}
fn base() -> SketchDefinition {
    let mut s = rectangle();
    s.points.clear();
    s.lines.clear();
    s.constraints.clear();
    s.profile.clear();
    s
}
fn line(id: &str, a: &str, b: &str) -> SketchLine {
    SketchLine {
        id: id.into(),
        start: a.into(),
        end: b.into(),
    }
}
fn circle() -> SketchDefinition {
    let mut s = base();
    s.points = vec![point("c", 0., 0., true), point("r", 4., 0., false)];
    s.circles.push(SketchCircle {
        id: "circle".into(),
        center: "c".into(),
        rim: "r".into(),
    });
    s.profile = vec!["circle".into()];
    s
}

#[test]
fn angle_radius_diameter_and_symmetry_solve_and_report_true_residuals() {
    let params = HashMap::new();
    let mut s = base();
    s.points = vec![
        point("o", 0., 0., true),
        point("x", 10., 0., true),
        point("b", 8., 4., false),
        point("a", 3., 4., true),
        point("mirror", -2., 3., false),
        point("axis", 0., 10., true),
    ];
    s.lines = vec![
        line("first", "o", "x"),
        line("second", "o", "b"),
        line("axis", "o", "axis"),
    ];
    s.constraints = vec![
        SketchConstraint::Angle {
            first: "first".into(),
            second: "second".into(),
            value: number(std::f64::consts::FRAC_PI_3),
        },
        SketchConstraint::Distance {
            first: "o".into(),
            second: "b".into(),
            value: length(10.),
        },
        SketchConstraint::Symmetric {
            first: "a".into(),
            second: "mirror".into(),
            axis: "axis".into(),
        },
    ];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved, "{}", solved.max_residual);
    assert!((solved.points["b"].x - 5.).abs() < 1e-7);
    assert!((solved.points["b"].y - 5. * 3f64.sqrt()).abs() < 1e-7);
    assert!((solved.points["mirror"].x + 3.).abs() < 1e-7);
    assert!((solved.points["mirror"].y - 4.).abs() < 1e-7);
    assert!(
        s.constraint_checks(&params, &solved)
            .unwrap()
            .iter()
            .all(|c| c.satisfied)
    );
    let mut s = circle();
    s.constraints.push(SketchConstraint::Radius {
        curve: "circle".into(),
        value: length(6.),
    });
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved);
    assert!((line_length((solved.points["c"], solved.points["r"])) - 6.).abs() < 1e-8);
    s.constraints[0] = SketchConstraint::Diameter {
        curve: "circle".into(),
        value: length(14.),
    };
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved);
    assert!((line_length((solved.points["c"], solved.points["r"])) - 7.).abs() < 1e-8);
    s.points[1].fixed = true;
    let failed = s.solve(&params).unwrap();
    assert!(!failed.solved);
    assert!(
        (s.constraint_checks(&params, &failed).unwrap()[0]
            .max_residual
            .unwrap()
            - 6.)
            .abs()
            < 1e-8
    );
}
#[test]
fn point_on_lines_circles_arcs_ellipses_and_native_splines() {
    let params = HashMap::new();
    let mut s = circle();
    s.points[1].fixed = true;
    s.points.push(point("p", 2., 2., false));
    s.constraints = vec![SketchConstraint::PointOnCurve {
        point: "p".into(),
        curve: "circle".into(),
    }];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved);
    assert!((line_length((solved.points["c"], solved.points["p"])) - 4.).abs() < 1e-8);
    let mut s = base();
    s.points = vec![
        point("a", 0., 0., true),
        point("b", 10., 0., true),
        point("p", 3., 4., false),
    ];
    s.lines = vec![line("line", "a", "b")];
    s.constraints = vec![SketchConstraint::PointOnCurve {
        point: "p".into(),
        curve: "line".into(),
    }];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved);
    assert!(solved.points["p"].y.abs() < 1e-8);
    s.lines.clear();
    s.points.insert(1, point("m", 5., 3., true));
    s.splines = vec![SketchSpline {
        endpoints: None,
        basis: None,
        id: "spline".into(),
        points: vec!["a".into(), "m".into(), "b".into()],
    }];
    s.constraints[0] = SketchConstraint::PointOnCurve {
        point: "p".into(),
        curve: "spline".into(),
    };
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved, "{}", solved.max_residual);
    assert!(s.constraint_checks(&params, &solved).unwrap()[0].satisfied);
    assert_eq!(solved.free_degrees, 1);
    let mut s = base();
    s.points = vec![
        point("c", 0., 0., true),
        point("a", 6., 0., true),
        point("b", 0., 3., true),
        point("p", 4., 4., false),
    ];
    s.ellipses = vec![SketchEllipse {
        id: "e".into(),
        center: "c".into(),
        major: "a".into(),
        minor: "b".into(),
    }];
    s.constraints = vec![SketchConstraint::PointOnCurve {
        point: "p".into(),
        curve: "e".into(),
    }];
    let solved = s.solve(&params).unwrap();
    assert!(
        solved.solved,
        "ellipse residual {} at {:?}",
        solved.max_residual, solved.points["p"]
    );
    let p = solved.points["p"];
    assert!(((p.x / 6.).powi(2) + (p.y / 3.).powi(2) - 1.).abs() < 1e-8);
    let mut s = arc_profile(false);
    for p in &mut s.points {
        p.fixed = true;
    }
    let arc = s.arcs[0].clone();
    let c = s.points.iter().find(|p| p.id == arc.center).unwrap();
    let _ = c;
    s.points.push(point("on", 8., 5., false));
    s.constraints = vec![SketchConstraint::PointOnCurve {
        point: "on".into(),
        curve: arc.id.clone(),
    }];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved, "arc residual {}", solved.max_residual);
}
#[test]
fn exact_ellipse_profiles_regenerate_on_rotated_planes_and_migrate() {
    let mut s = base();
    s.points = vec![
        point("c", 0., 0., true),
        point("a", 8., 0., true),
        point("b", 0., 3., false),
    ];
    s.ellipses = vec![SketchEllipse {
        id: "ellipse".into(),
        center: "c".into(),
        major: "a".into(),
        minor: "b".into(),
    }];
    s.constraints = vec![SketchConstraint::Distance {
        first: "c".into(),
        second: "b".into(),
        value: length(4.),
    }];
    let session = Session::new().unwrap();
    let params = HashMap::new();
    let face = s.face(&session, &params).unwrap();
    assert!((session.surface_area(&face).unwrap() - 32. * std::f64::consts::PI).abs() < 1e-7);
    drop(face);
    s.x_axis = VectorExpr::Literal(VectorQuantity::scalars(0., 0., 1.));
    s.y_axis = VectorExpr::Literal(VectorQuantity::scalars(1., 0., 0.));
    let face = s.face(&session, &params).unwrap();
    assert!((session.surface_area(&face).unwrap() - 32. * std::f64::consts::PI).abs() < 1e-7);
    drop(face);
    assert_eq!(session.shape_count().unwrap(), 0);
    let encoded = serde_json::to_value(&s).unwrap();
    assert_eq!(
        serde_json::from_value::<SketchDefinition>(encoded).unwrap(),
        s
    );
    let mut old = serde_json::to_value(rectangle()).unwrap();
    old.as_object_mut().unwrap().remove("ellipses");
    old.as_object_mut().unwrap().remove("profile_operations");
    let old: SketchDefinition = serde_json::from_value(old).unwrap();
    assert!(old.ellipses.is_empty() && old.profile_operations.is_empty());
    assert!(old.solve(&params).unwrap().solved);
}
#[test]
fn trim_extend_and_offset_profiles_are_native_and_leave_source_geometry_unchanged() {
    let params = HashMap::new();
    let session = Session::new().unwrap();
    let mut s = base();
    s.points = vec![point("a", 0., 0., true), point("b", 10., 0., true)];
    s.lines = vec![line("line", "a", "b")];
    s.profile_operations = vec![
        SketchProfileOperation::Trim {
            entity: "line".into(),
            first: number(0.2),
            last: number(0.8),
        },
        SketchProfileOperation::Extend {
            entity: "line".into(),
            start: length(1.),
            end: length(2.),
        },
    ];
    let wire = s.open_wire(&session, &params, None).unwrap();
    let bounds = session.exact_bounds(&wire).unwrap();
    assert!((bounds.min.x - 1.).abs() < 1e-7 && (bounds.max.x - 10.).abs() < 1e-7);
    drop(wire);
    assert_eq!(s.points[0].x, length(0.));
    s.profile_operations.push(SketchProfileOperation::Offset {
        distance: length(2.),
        join: SketchOffsetJoin::Intersection,
    });
    let wire = s.open_wire(&session, &params, None).unwrap();
    let bounds = session.exact_bounds(&wire).unwrap();
    assert!((bounds.min.y + 2.).abs() < 1e-7);
    drop(wire);
    let mut s = circle();
    s.points[1].fixed = true;
    s.profile_operations = vec![
        SketchProfileOperation::Trim {
            entity: "circle".into(),
            first: number(0.),
            last: number(0.25),
        },
        SketchProfileOperation::Extend {
            entity: "circle".into(),
            start: length(0.),
            end: length(std::f64::consts::PI * 2.),
        },
    ];
    let wire = s.open_wire(&session, &params, None).unwrap();
    let edge = session
        .subshapes(&wire, ShapeType::Edge)
        .unwrap()
        .pop()
        .unwrap();
    let samples = session.edge_sample_points(&edge, 5).unwrap();
    assert!((samples.last().unwrap().x + 4.).abs() < 1e-6);
    drop(edge);
    drop(wire);
    let mut s = base();
    s.points = vec![
        point("c", 0., 0., true),
        point("a", 6., 0., true),
        point("b", 0., 3., true),
    ];
    s.ellipses = vec![SketchEllipse {
        id: "ellipse".into(),
        center: "c".into(),
        major: "a".into(),
        minor: "b".into(),
    }];
    s.profile_operations = vec![SketchProfileOperation::Trim {
        entity: "ellipse".into(),
        first: number(0.),
        last: number(0.5),
    }];
    let wire = s.open_wire(&session, &params, None).unwrap();
    let bounds = session.exact_bounds(&wire).unwrap();
    assert!(
        (bounds.min.x + 6.).abs() < 1e-7
            && bounds.min.y.abs() < 1e-7
            && (bounds.max.y - 3.).abs() < 1e-7
    );
    drop(wire);
    let mut s = base();
    s.points = vec![
        point("a", 0., 0., true),
        point("b", 10., 0., true),
        point("c", 10., 10., true),
        point("d", 0., 10., true),
    ];
    s.lines = vec![
        line("ab", "a", "b"),
        line("bc", "b", "c"),
        line("cd", "c", "d"),
        line("da", "d", "a"),
    ];
    s.profile_operations = vec![SketchProfileOperation::Offset {
        distance: length(1.),
        join: SketchOffsetJoin::Intersection,
    }];
    let face = s.face(&session, &params).unwrap();
    assert!(
        (session.surface_area(&face).unwrap() - 144.).abs() < 1e-6,
        "offset area {}",
        session.surface_area(&face).unwrap()
    );
    drop(face);
    s.profile_operations[0] = SketchProfileOperation::Offset {
        distance: length(1.),
        join: SketchOffsetJoin::Arc,
    };
    let face = s.face(&session, &params).unwrap();
    assert!((session.surface_area(&face).unwrap() - (140. + std::f64::consts::PI)).abs() < 1e-6);
    drop(face);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn edited_splines_keep_native_curve_geometry_and_extend_to_tangent_targets() {
    let mut s = base();
    s.points = vec![
        point("a", 0., 0., true),
        point("m", 5., 3., true),
        point("b", 10., 0., true),
    ];
    s.splines = vec![SketchSpline {
        endpoints: None,
        basis: None,
        id: "s".into(),
        points: vec!["a".into(), "m".into(), "b".into()],
    }];
    s.profile_operations = vec![
        SketchProfileOperation::Trim {
            entity: "s".into(),
            first: number(0.1),
            last: number(0.9),
        },
        SketchProfileOperation::Extend {
            entity: "s".into(),
            start: length(1.),
            end: length(2.),
        },
    ];
    let session = Session::new().unwrap();
    let wire = s.open_wire(&session, &HashMap::new(), None).unwrap();
    assert!(session.is_valid(&wire).unwrap());
    assert!(!session.wire_is_closed(&wire).unwrap());
    drop(wire);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn advanced_references_units_degenerate_axes_and_bad_operation_order_are_rejected() {
    let params = HashMap::new();
    let mut s = circle();
    s.constraints = vec![SketchConstraint::Radius {
        curve: "missing".into(),
        value: length(2.),
    }];
    assert!(s.solve(&params).is_err());
    s.constraints[0] = SketchConstraint::Radius {
        curve: "circle".into(),
        value: number(2.),
    };
    assert!(s.solve(&params).is_err());
    s.constraints[0] = SketchConstraint::Radius {
        curve: "circle".into(),
        value: length(-2.),
    };
    assert!(s.solve(&params).is_err());
    s.constraints.clear();
    s.profile_operations = vec![SketchProfileOperation::Trim {
        entity: "circle".into(),
        first: number(0.8),
        last: number(0.2),
    }];
    assert!(s.solve(&params).is_err());
    s.profile_operations = vec![
        SketchProfileOperation::Offset {
            distance: length(1.),
            join: SketchOffsetJoin::Arc,
        },
        SketchProfileOperation::Trim {
            entity: "circle".into(),
            first: number(0.),
            last: number(0.5),
        },
    ];
    assert!(s.solve(&params).is_err());
    let mut s = base();
    s.points = vec![
        point("c", 0., 0., true),
        point("a", 2., 0., true),
        point("b", 0., 3., true),
    ];
    s.ellipses = vec![SketchEllipse {
        id: "e".into(),
        center: "c".into(),
        major: "a".into(),
        minor: "b".into(),
    }];
    assert!(s.solve(&params).is_err());
}

#[test]
fn equal_radius_solves_circle_and_arc_pairs_with_real_millimetre_diagnostics() {
    let parameters = HashMap::new();
    let mut s = circle();
    s.points[1].fixed = true; // The first circle drives the common 4 mm radius.
    s.points.extend([
        point("other-center", 10., 0., true),
        point("other-rim", 16., 0., false),
    ]);
    s.circles.push(SketchCircle {
        id: "other".into(),
        center: "other-center".into(),
        rim: "other-rim".into(),
    });
    s.lines.push(line("axis", "other-center", "other-rim"));
    s.constraints = vec![
        SketchConstraint::Horizontal {
            line: "axis".into(),
        },
        SketchConstraint::EqualRadius {
            first: "circle".into(),
            second: "other".into(),
        },
    ];
    let solution = s.solve(&parameters).unwrap();
    assert!(solution.solved);
    assert_eq!(solution.free_degrees, 0);
    assert!((solution.points["other-rim"].x - 14.).abs() < 1e-7);
    let checks = s.constraint_checks(&parameters, &solution).unwrap();
    assert!(checks[1].satisfied && !checks[1].by_construction);
    assert!(checks[1].max_residual.unwrap() < 1e-7);
    s.circles.pop();
    s.lines.clear();
    s.points.pop();
    s.points.extend([
        point("arc-start", 16., 0., false),
        point("arc-end", 10., 6., false),
    ]);
    s.arcs.push(SketchArc {
        id: "arc".into(),
        center: "other-center".into(),
        start: "arc-start".into(),
        end: "arc-end".into(),
        clockwise: false,
    });
    s.constraints = vec![SketchConstraint::EqualRadius {
        first: "circle".into(),
        second: "arc".into(),
    }];
    let solution = s.solve(&parameters).unwrap();
    assert!(solution.solved);
    for id in ["arc-start", "arc-end"] {
        let p = solution.points[id];
        assert!(((p.x - 10.).hypot(p.y) - 4.).abs() < 1e-7);
    }
    let session = Session::new().unwrap();
    s.profile = vec!["arc".into()];
    let wire = s.open_wire(&session, &parameters, None).unwrap();
    let edge = session
        .subshapes(&wire, occt_bridge::ShapeType::Edge)
        .unwrap()
        .pop()
        .unwrap();
    assert!((session.edge_circle_radius(&edge).unwrap().unwrap() - 4.).abs() < 1e-7);
    drop((edge, wire));
    assert_eq!(session.shape_count().unwrap(), 0);
    // The same supporting-radius equation also works between two arcs.
    s.circles.clear();
    s.points.push(point("first-end", 0.0, 4.0, true));
    s.arcs.push(SketchArc {
        id: "first-arc".into(),
        center: "c".into(),
        start: "r".into(),
        end: "first-end".into(),
        clockwise: false,
    });
    s.constraints = vec![SketchConstraint::EqualRadius {
        first: "first-arc".into(),
        second: "arc".into(),
    }];
    let solution = s.solve(&parameters).unwrap();
    assert!(solution.solved);
    assert!(s.constraint_checks(&parameters, &solution).unwrap()[0].satisfied);
}

#[test]
fn equal_radius_rejects_wrong_references_and_reports_fixed_radius_conflicts() {
    let mut s = circle();
    s.points[1].fixed = true;
    s.points.extend([
        point("other-center", 10., 0., true),
        point("other-rim", 16., 0., true),
    ]);
    s.circles.push(SketchCircle {
        id: "other".into(),
        center: "other-center".into(),
        rim: "other-rim".into(),
    });
    s.lines.push(line("axis", "other-center", "other-rim"));
    s.constraints = vec![SketchConstraint::EqualRadius {
        first: "circle".into(),
        second: "other".into(),
    }];
    let parameters = HashMap::new();
    let solution = s.solve(&parameters).unwrap();
    assert!(!solution.solved);
    let check = &s.constraint_checks(&parameters, &solution).unwrap()[0];
    assert_eq!(check.max_residual, Some(2.0));
    assert!(!check.satisfied && !check.by_construction);
    s.points.extend([
        point("major", 5.0, 0.0, true),
        point("minor", 0.0, 3.0, true),
    ]);
    s.ellipses.push(SketchEllipse {
        id: "ellipse".into(),
        center: "c".into(),
        major: "major".into(),
        minor: "minor".into(),
    });
    for second in ["missing", "axis", "circle", "ellipse"] {
        s.constraints = vec![SketchConstraint::EqualRadius {
            first: "circle".into(),
            second: second.into(),
        }];
        assert!(s.solve(&parameters).is_err());
    }
}

#[test]
fn midpoint_solves_both_coordinates_and_reports_conflicting_fixed_points() {
    let mut s = base();
    s.points = vec![
        point("a", -4., 2., true),
        point("b", 8., 6., true),
        point("mid", 7., -3., false),
    ];
    s.lines = vec![line("ab", "a", "b")];
    s.constraints = vec![SketchConstraint::Midpoint {
        point: "mid".into(),
        line: "ab".into(),
    }];
    let params = HashMap::new();
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved);
    assert_eq!(solved.free_degrees, 0);
    assert!((solved.points["mid"].x - 2.).abs() < 1e-7);
    assert!((solved.points["mid"].y - 4.).abs() < 1e-7);
    let checks = s.constraint_checks(&params, &solved).unwrap();
    assert!(checks[0].satisfied && !checks[0].by_construction);
    s.points[2].fixed = true;
    let failed = s.solve(&params).unwrap();
    assert!(!failed.solved);
    assert!(!s.constraint_checks(&params, &failed).unwrap()[0].satisfied);
    s.constraints[0] = SketchConstraint::Midpoint {
        point: "missing".into(),
        line: "ab".into(),
    };
    assert!(s.solve(&params).is_err());
    s.constraints[0] = SketchConstraint::Midpoint {
        point: "mid".into(),
        line: "missing".into(),
    };
    assert!(s.solve(&params).is_err());
}

#[test]
fn concentric_supports_circles_arcs_and_ellipses_without_equalizing_radii() {
    let params = HashMap::new();
    for kind in 0..3 {
        let mut s = circle();
        s.points[1].fixed = true;
        s.points.extend([
            point("other-center", 1., 1., false),
            point("a", 7., 0., true),
            point("b", 0., 3., true),
        ]);
        match kind {
            0 => s.circles.push(SketchCircle {
                id: "other".into(),
                center: "other-center".into(),
                rim: "a".into(),
            }),
            1 => {
                s.points.last_mut().unwrap().y = length(7.);
                s.arcs.push(SketchArc {
                    id: "other".into(),
                    center: "other-center".into(),
                    start: "a".into(),
                    end: "b".into(),
                    clockwise: false,
                });
            }
            _ => s.ellipses.push(SketchEllipse {
                id: "other".into(),
                center: "other-center".into(),
                major: "a".into(),
                minor: "b".into(),
            }),
        }
        s.constraints = vec![SketchConstraint::Concentric {
            first: "circle".into(),
            second: "other".into(),
        }];
        let solved = s.solve(&params).unwrap();
        assert!(solved.solved);
        assert_eq!(solved.free_degrees, 0);
        assert!(solved.points["other-center"].x.abs() < 1e-7);
        assert!(solved.points["other-center"].y.abs() < 1e-7);
        assert_eq!(solved.points["a"].x, 7.);
        assert!(s.constraint_checks(&params, &solved).unwrap()[0].satisfied);
    }
}

#[test]
fn concentric_rejects_missing_self_and_line_references_and_reports_conflicts() {
    let mut s = circle();
    s.points.extend([
        point("other-center", 1., 2., true),
        point("other-rim", 8., 2., true),
    ]);
    s.circles.push(SketchCircle {
        id: "other".into(),
        center: "other-center".into(),
        rim: "other-rim".into(),
    });
    s.lines.push(line("axis", "other-center", "other-rim"));
    let params = HashMap::new();
    for second in ["missing", "circle", "axis"] {
        s.constraints = vec![SketchConstraint::Concentric {
            first: "circle".into(),
            second: second.into(),
        }];
        assert!(s.solve(&params).is_err());
    }
    s.constraints = vec![SketchConstraint::Concentric {
        first: "circle".into(),
        second: "other".into(),
    }];
    let failed = s.solve(&params).unwrap();
    assert!(!failed.solved);
    assert!(!s.constraint_checks(&params, &failed).unwrap()[0].satisfied);
}

#[test]
fn midpoint_and_concentric_solve_when_all_referenced_centers_are_free() {
    let params = HashMap::new();
    let mut s = base();
    s.points = vec![
        point("a", -4., 2., false),
        point("b", 8., 6., false),
        point("mid", 7., -3., false),
    ];
    s.lines = vec![line("ab", "a", "b")];
    s.constraints = vec![SketchConstraint::Midpoint {
        point: "mid".into(),
        line: "ab".into(),
    }];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved);
    assert_eq!(solved.free_degrees, 4);
    assert!(s.constraint_checks(&params, &solved).unwrap()[0].satisfied);
    let mut s = circle();
    s.points[0].fixed = false;
    s.points[1].fixed = true;
    s.points.extend([
        point("other-center", 2., 2., false),
        point("other-rim", 8., 2., true),
    ]);
    s.circles.push(SketchCircle {
        id: "other".into(),
        center: "other-center".into(),
        rim: "other-rim".into(),
    });
    s.constraints = vec![SketchConstraint::Concentric {
        first: "circle".into(),
        second: "other".into(),
    }];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved);
    assert_eq!(solved.free_degrees, 2);
    assert!(s.constraint_checks(&params, &solved).unwrap()[0].satisfied);
}

#[test]
fn signed_point_line_distance_solves_tilted_lines_both_sides_zero_and_line_extensions() {
    let params = HashMap::new();
    for x in [3.8, 20.] {
        for target in [-3., 0., 3.] {
            for reversed in [false, true] {
                let mut s = base();
                s.points = vec![
                    point("a", 2., -1., true),
                    point("b", 5., 3., true),
                    point("p", x, 2., false),
                    point("axis", x, -20., true),
                ];
                s.lines = vec![
                    if reversed {
                        line("reference", "b", "a")
                    } else {
                        line("reference", "a", "b")
                    },
                    line("vertical", "axis", "p"),
                ];
                s.constraints = vec![
                    SketchConstraint::Vertical {
                        line: "vertical".into(),
                    },
                    SketchConstraint::PointLineDistance {
                        point: "p".into(),
                        line: "reference".into(),
                        value: length(target),
                    },
                ];
                let solution = s.solve(&params).unwrap();
                assert!(solution.solved);
                assert_eq!(solution.free_degrees, 0);
                let distance = if reversed { -target } else { target };
                let expected = -1. + (distance + (x - 2.) * 0.8) / 0.6;
                assert!((solution.points["p"].y - expected).abs() < 1e-7);
                let check = &s.constraint_checks(&params, &solution).unwrap()[1];
                assert!(check.satisfied && !check.by_construction);
                assert!(check.max_residual.unwrap() < 1e-7);
            }
        }
    }
}

#[test]
fn point_line_distance_rejects_bad_references_units_and_collapsed_lines() {
    let params = HashMap::new();
    let mut s = base();
    s.points = vec![
        point("a", 0., 0., true),
        point("b", 10., 0., true),
        point("p", 2., 2., false),
    ];
    s.lines = vec![line("reference", "a", "b")];
    for (point_id, line_id, value) in [
        ("missing", "reference", length(2.)),
        ("p", "missing", length(2.)),
        ("p", "reference", number(2.)),
    ] {
        s.constraints = vec![SketchConstraint::PointLineDistance {
            point: point_id.into(),
            line: line_id.into(),
            value,
        }];
        assert!(s.solve(&params).is_err());
    }
    s.constraints = vec![SketchConstraint::PointLineDistance {
        point: "p".into(),
        line: "reference".into(),
        value: length(2.),
    }];
    s.points[1].x = length(0.);
    assert!(
        s.solve(&params)
            .unwrap_err()
            .message
            .contains("zero length")
    );
}

#[test]
fn point_line_distance_solves_free_line_endpoints_and_reports_fixed_conflicts() {
    let params = HashMap::new();
    let mut s = base();
    s.points = vec![
        point("a", 0., 0., true),
        point("b", 10., 0., true),
        point("p", 2., 0., false),
    ];
    s.lines = vec![line("reference", "a", "b")];
    s.constraints = vec![SketchConstraint::PointLineDistance {
        point: "p".into(),
        line: "reference".into(),
        value: length(3.),
    }];
    let solution = s.solve(&params).unwrap();
    assert!(solution.solved);
    assert_eq!(solution.free_degrees, 1);
    assert!((solution.points["p"].y - 3.).abs() < 1e-7);
    s.points[2].fixed = true;
    let solution = s.solve(&params).unwrap();
    assert!(!solution.solved);
    assert_eq!(
        s.constraint_checks(&params, &solution).unwrap()[0].max_residual,
        Some(3.)
    );
    s.points[0].fixed = false;
    s.points[1].fixed = false;
    let solution = s.solve(&params).unwrap();
    assert!(solution.solved);
    assert_eq!(solution.free_degrees, 3);
    assert!(s.constraint_checks(&params, &solution).unwrap()[0].satisfied);
}

#[test]
fn independent_line_circle_tangency_solves_both_sides_of_a_tilted_supporting_line() {
    let params = HashMap::new();
    for side in [SketchLineSide::Left, SketchLineSide::Right] {
        let mut s = base();
        s.points = vec![
            point("a", 0., 0., true),
            point("b", 3., 4., true),
            point("c", 20., 0., false),
            point("r", 22., 0., false),
            point("axis", 20., -20., true),
        ];
        s.lines = vec![
            line("reference", "a", "b"),
            line("vertical", "axis", "c"),
            line("radius", "c", "r"),
        ];
        s.circles.push(SketchCircle {
            id: "circle".into(),
            center: "c".into(),
            rim: "r".into(),
        });
        s.constraints = vec![
            SketchConstraint::LineCircleTangent {
                line: "reference".into(),
                circle: "circle".into(),
                side,
            },
            SketchConstraint::Vertical {
                line: "vertical".into(),
            },
            SketchConstraint::Radius {
                curve: "circle".into(),
                value: length(2.),
            },
            SketchConstraint::Horizontal {
                line: "radius".into(),
            },
        ];
        let solution = s.solve(&params).unwrap();
        assert!(solution.solved);
        assert_eq!(solution.free_degrees, 0);
        let sign = if side == SketchLineSide::Left {
            1.
        } else {
            -1.
        };
        assert!((solution.points["c"].y - (16. + sign * 2.) / 0.6).abs() < 1e-7);
        let check = &s.constraint_checks(&params, &solution).unwrap()[0];
        assert!(check.satisfied && !check.by_construction);
        assert!(check.max_residual.unwrap() < 1e-7);
    }
}

#[test]
fn independent_circle_tangency_solves_external_and_ordered_internal_contacts() {
    let params = HashMap::new();
    for mode in [
        SketchCircleTangency::External,
        SketchCircleTangency::Internal,
    ] {
        let mut s = circle();
        s.points[1].fixed = true;
        s.points
            .extend([point("c2", 5., 0., false), point("r2", 7., 0., false)]);
        s.circles.push(SketchCircle {
            id: "second".into(),
            center: "c2".into(),
            rim: "r2".into(),
        });
        s.lines = vec![line("centers", "c", "c2"), line("radius", "c2", "r2")];
        s.constraints = vec![
            SketchConstraint::CircleCircleTangent {
                first: "circle".into(),
                second: "second".into(),
                mode,
            },
            SketchConstraint::Horizontal {
                line: "centers".into(),
            },
            SketchConstraint::Horizontal {
                line: "radius".into(),
            },
            SketchConstraint::Radius {
                curve: "second".into(),
                value: length(2.),
            },
        ];
        let solution = s.solve(&params).unwrap();
        assert!(solution.solved);
        assert_eq!(solution.free_degrees, 0);
        let target = if mode == SketchCircleTangency::External {
            6.
        } else {
            2.
        };
        assert!((solution.points["c2"].x - target).abs() < 1e-7);
        assert!(s.constraint_checks(&params, &solution).unwrap()[0].satisfied);
    }
}

#[test]
fn independent_tangency_rejects_wrong_types_and_degenerate_geometry_and_reports_fixed_conflicts() {
    let params = HashMap::new();
    let mut s = circle();
    s.points[1].fixed = true;
    s.points.extend([
        point("a", -10., -6., true),
        point("b", 10., -6., true),
        point("c2", 10., 0., true),
        point("r2", 12., 0., true),
    ]);
    s.lines.push(line("reference", "a", "b"));
    s.circles.push(SketchCircle {
        id: "second".into(),
        center: "c2".into(),
        rim: "r2".into(),
    });
    for circle in ["missing", "reference"] {
        s.constraints = vec![SketchConstraint::LineCircleTangent {
            line: "reference".into(),
            circle: circle.into(),
            side: SketchLineSide::Left,
        }];
        assert!(s.solve(&params).is_err());
    }
    for second in ["missing", "reference", "circle"] {
        s.constraints = vec![SketchConstraint::CircleCircleTangent {
            first: "circle".into(),
            second: second.into(),
            mode: SketchCircleTangency::External,
        }];
        assert!(s.solve(&params).is_err());
    }
    s.constraints = vec![SketchConstraint::LineCircleTangent {
        line: "reference".into(),
        circle: "circle".into(),
        side: SketchLineSide::Left,
    }];
    let solution = s.solve(&params).unwrap();
    assert!(!solution.solved);
    assert_eq!(
        s.constraint_checks(&params, &solution).unwrap()[0].max_residual,
        Some(2.)
    );
    s.points[3].x = s.points[2].x.clone();
    assert!(s.solve(&params).is_err());
    s.constraints = vec![SketchConstraint::CircleCircleTangent {
        first: "second".into(),
        second: "circle".into(),
        mode: SketchCircleTangency::Internal,
    }];
    assert!(
        s.solve(&params)
            .unwrap_err()
            .message
            .contains("larger radius")
    );
    s.constraints = vec![SketchConstraint::CircleCircleTangent {
        first: "circle".into(),
        second: "second".into(),
        mode: SketchCircleTangency::External,
    }];
    let solution = s.solve(&params).unwrap();
    assert!(!solution.solved);
    s.points[4].x = length(0.);
    assert!(
        s.solve(&params)
            .unwrap_err()
            .message
            .contains("distinct centres")
    );
}

#[test]
fn independent_tangency_solves_underconstrained_centres_and_rejects_equal_internal_radii() {
    let params = HashMap::new();
    let mut s = circle();
    s.points[1].fixed = true;
    s.points
        .extend([point("c2", 8., 2., false), point("r2", 10., 2., true)]);
    s.circles.push(SketchCircle {
        id: "second".into(),
        center: "c2".into(),
        rim: "r2".into(),
    });
    s.constraints = vec![SketchConstraint::CircleCircleTangent {
        first: "circle".into(),
        second: "second".into(),
        mode: SketchCircleTangency::External,
    }];
    let solution = s.solve(&params).unwrap();
    assert!(solution.solved);
    assert_eq!(solution.free_degrees, 1);
    assert!(s.constraint_checks(&params, &solution).unwrap()[0].satisfied);
    s.points[2] = point("c2", 10., 0., true);
    s.points[3] = point("r2", 14., 0., true);
    s.constraints = vec![SketchConstraint::CircleCircleTangent {
        first: "circle".into(),
        second: "second".into(),
        mode: SketchCircleTangency::Internal,
    }];
    assert!(
        s.solve(&params)
            .unwrap_err()
            .message
            .contains("larger radius")
    );
    s.points[2] = point("c2", 8., 2., false);
    s.points[3] = point("r2", 10., 2., true);
    s.points
        .extend([point("a", -20., 0., true), point("b", 20., 0., true)]);
    s.lines.push(line("reference", "a", "b"));
    s.constraints = vec![SketchConstraint::LineCircleTangent {
        line: "reference".into(),
        circle: "second".into(),
        side: SketchLineSide::Left,
    }];
    let solution = s.solve(&params).unwrap();
    assert!(solution.solved);
    assert_eq!(solution.free_degrees, 1);
}

fn fixed_contact_arc(start: f64, end: f64, clockwise: bool) -> SketchDefinition {
    let mut s = base();
    s.points = vec![
        point("c", 0., 0., true),
        point("a", 4. * start.cos(), 4. * start.sin(), true),
        point("b", 4. * end.cos(), 4. * end.sin(), true),
    ];
    s.arcs.push(SketchArc {
        id: "arc".into(),
        center: "c".into(),
        start: "a".into(),
        end: "b".into(),
        clockwise,
    });
    s
}

#[test]
fn independent_line_arc_tangency_enforces_direction_and_includes_span_endpoints() {
    let params = HashMap::new();
    let half_pi = std::f64::consts::FRAC_PI_2;
    let mut s = fixed_contact_arc(0., half_pi, false);
    s.points
        .extend([point("u", -10., -4., true), point("v", 10., -4., true)]);
    s.lines.push(line("reference", "u", "v"));
    s.constraints = vec![SketchConstraint::LineCircleTangent {
        line: "reference".into(),
        circle: "arc".into(),
        side: SketchLineSide::Left,
    }];
    let failed = s.solve(&params).unwrap();
    assert!(!failed.solved); // Supporting-circle distance is correct; arc excludes bottom.
    assert!(
        (s.constraint_checks(&params, &failed).unwrap()[0]
            .max_residual
            .unwrap()
            - 4. * half_pi)
            .abs()
            < 1e-7
    );
    s.arcs[0].clockwise = true;
    assert!(s.solve(&params).unwrap().solved);
    s.arcs[0].clockwise = false;
    s.points[3] = point("u", -10., 4., true);
    s.points[4] = point("v", 10., 4., true);
    s.constraints[0] = SketchConstraint::LineCircleTangent {
        line: "reference".into(),
        circle: "arc".into(),
        side: SketchLineSide::Right,
    };
    assert!(s.solve(&params).unwrap().solved); // Contact at end.
    s.points[3] = point("u", 4., -10., true);
    s.points[4] = point("v", 4., 10., true);
    s.constraints[0] = SketchConstraint::LineCircleTangent {
        line: "reference".into(),
        circle: "arc".into(),
        side: SketchLineSide::Left,
    };
    assert!(s.solve(&params).unwrap().solved); // Contact at start.
}

#[test]
fn independent_arc_contacts_handle_angle_wraparound_and_reversed_line_direction() {
    let params = HashMap::new();
    let q = std::f64::consts::FRAC_PI_4;
    let mut s = fixed_contact_arc(-q, q, false);
    s.points
        .extend([point("u", 4., -10., true), point("v", 4., 10., true)]);
    s.lines.push(line("reference", "u", "v"));
    s.constraints = vec![SketchConstraint::LineCircleTangent {
        line: "reference".into(),
        circle: "arc".into(),
        side: SketchLineSide::Left,
    }];
    assert!(s.solve(&params).unwrap().solved);
    s.lines[0] = line("reference", "v", "u");
    s.constraints[0] = SketchConstraint::LineCircleTangent {
        line: "reference".into(),
        circle: "arc".into(),
        side: SketchLineSide::Right,
    };
    assert!(s.solve(&params).unwrap().solved);
    s.arcs[0].clockwise = true;
    let failed = s.solve(&params).unwrap();
    assert!(!failed.solved);
    assert!(
        s.constraint_checks(&params, &failed).unwrap()[0]
            .max_residual
            .unwrap()
            > 3.
    );
}

#[test]
fn independent_arc_arc_tangency_checks_both_spans_in_external_and_internal_modes() {
    let params = HashMap::new();
    let half_pi = std::f64::consts::FRAC_PI_2;
    for mode in [
        SketchCircleTangency::External,
        SketchCircleTangency::Internal,
    ] {
        let mut s = fixed_contact_arc(0., half_pi, false);
        let d = if mode == SketchCircleTangency::External {
            6.
        } else {
            2.
        };
        let c = d / 2_f64.sqrt();
        let angle = if mode == SketchCircleTangency::External {
            std::f64::consts::PI
        } else {
            0.
        };
        s.points.extend([
            point("c2", c, c, true),
            point("a2", c + 2. * angle.cos(), c + 2. * angle.sin(), true),
            point(
                "b2",
                c + 2. * (angle + half_pi).cos(),
                c + 2. * (angle + half_pi).sin(),
                true,
            ),
        ]);
        s.arcs.push(SketchArc {
            id: "second".into(),
            center: "c2".into(),
            start: "a2".into(),
            end: "b2".into(),
            clockwise: false,
        });
        s.constraints = vec![SketchConstraint::CircleCircleTangent {
            first: "arc".into(),
            second: "second".into(),
            mode,
        }];
        assert!(s.solve(&params).unwrap().solved);
        for index in 0..2 {
            s.arcs[index].clockwise = true;
            let failed = s.solve(&params).unwrap();
            assert!(!failed.solved);
            assert!(
                s.constraint_checks(&params, &failed).unwrap()[0]
                    .max_residual
                    .unwrap()
                    > 1.
            );
            s.arcs[index].clockwise = false;
        }
    }
}

#[test]
fn arc_tangency_sparse_derivatives_include_the_free_end_point() {
    let params = HashMap::new();
    let mut s = fixed_contact_arc(0., std::f64::consts::FRAC_PI_2, false);
    s.points[2].fixed = false;
    s.points
        .extend([point("u", -4., -10., true), point("v", -4., 10., true)]);
    s.lines.push(line("reference", "u", "v"));
    s.constraints = vec![SketchConstraint::LineCircleTangent {
        line: "reference".into(),
        circle: "arc".into(),
        side: SketchLineSide::Right,
    }];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved, "residual {}", solved.max_residual);
    assert!(s.constraint_checks(&params, &solved).unwrap()[0].satisfied);
    assert!((solved.points["b"].x.hypot(solved.points["b"].y) - 4.).abs() < 1e-7);
    s.profile = vec!["arc".into()];
    let session = Session::new().unwrap();
    let wire = s.open_wire(&session, &params, None).unwrap();
    assert!(session.is_valid(&wire).unwrap());
}

fn explicit_quadratic() -> SketchDefinition {
    let mut s = base();
    s.points = vec![
        point("a", 10., 0., true),
        point("b", 10., 10., true),
        point("c", 0., 10., true),
    ];
    s.splines.push(SketchSpline {
        endpoints: None,
        id: "curve".into(),
        points: vec!["a".into(), "b".into(), "c".into()],
        basis: Some(SketchBSplineBasis {
            non_clamped: false,
            periodic: false,
            degree: 2,
            knots: vec![0., 1.],
            multiplicities: vec![3, 3],
            weights: vec![number(1.), number(0.5_f64.sqrt()), number(1.)],
        }),
    });
    s
}

#[test]
fn explicit_bspline_point_membership_uses_native_rational_curve_not_control_polygon() {
    let params = HashMap::new();
    let mut s = explicit_quadratic();
    s.points
        .extend([point("p", 7., 7., false), point("axis", 7., 0., true)]);
    s.lines.push(line("vertical", "axis", "p"));
    s.constraints = vec![
        SketchConstraint::PointOnCurve {
            point: "p".into(),
            curve: "curve".into(),
        },
        SketchConstraint::Vertical {
            line: "vertical".into(),
        },
    ];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved, "{}", solved.max_residual);
    assert!((solved.points["p"].y - 51_f64.sqrt()).abs() < 1e-7);
    assert!(s.constraint_checks(&params, &solved).unwrap()[0].satisfied);
    let session = Session::new().unwrap();
    s.profile = vec!["curve".into()];
    let wire = s.open_wire(&session, &params, None).unwrap();
    let p = 10. / 2_f64.sqrt();
    let near = session
        .curve_closest_point(&wire, Vec3::new(p, p, 0.))
        .unwrap();
    assert!((near.x - p).hypot(near.y - p) < 1e-7);
}

#[test]
fn explicit_bspline_endpoint_tangency_moves_control_pole_and_is_measured() {
    let params = HashMap::new();
    let mut s = explicit_quadratic();
    s.points[1] = point("b", 8., 5., false);
    s.points.push(point("neighbor", 10., -2., true));
    s.lines.push(line("line", "neighbor", "a"));
    s.constraints = vec![SketchConstraint::Tangent {
        first: "line".into(),
        second: "curve".into(),
        point: "a".into(),
    }];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved);
    assert_eq!(solved.free_degrees, 1);
    assert!((solved.points["b"].x - 10.).abs() < 1e-7);
    let check = &s.constraint_checks(&params, &solved).unwrap()[0];
    assert!(check.satisfied && !check.by_construction);
    assert!(check.max_residual.unwrap() < 1e-7);
    let session = Session::new().unwrap();
    s.profile = vec!["curve".into()];
    let wire = s.open_wire(&session, &params, None).unwrap();
    let edge = session
        .subshapes(&wire, occt_bridge::ShapeType::Edge)
        .unwrap()
        .pop()
        .unwrap();
    let spans = session.edge_bezier_spans(&edge, 10).unwrap();
    assert_eq!(spans[0].poles.len(), 3);
    assert!((spans[0].poles[1].x - 10.).abs() < 1e-7);
}

#[test]
fn explicit_bspline_rejects_invalid_basis_weights_and_preserves_legacy_interpolation() {
    let params = HashMap::new();
    for case in 0..8 {
        let mut s = explicit_quadratic();
        let b = s.splines[0].basis.as_mut().unwrap();
        match case {
            0 => b.degree = 0,
            1 => b.knots[1] = 0.,
            2 => b.multiplicities[0] = 2,
            3 => b.multiplicities.push(1),
            4 => b.weights[1] = number(0.),
            5 => b.weights[1] = length(1.),
            6 => {
                b.weights.pop();
            }
            _ => s.splines[0].points[1] = "missing".into(),
        }
        assert!(s.solve(&params).is_err());
    }
    let mut s = explicit_quadratic();
    s.splines[0].basis = None;
    let solution = s.solve(&params).unwrap();
    let session = Session::new().unwrap();
    s.profile = vec!["curve".into()];
    let wire = s.open_wire(&session, &params, None).unwrap();
    let near = session
        .curve_closest_point(&wire, Vec3::new(10., 10., 0.))
        .unwrap();
    assert!((near.x - 10.).hypot(near.y - 10.) < 1e-7);
    assert!(solution.spline_weights.is_empty());
}

#[test]
fn explicit_bspline_accepts_repeated_control_poles_and_closed_clamped_profiles() {
    let params = HashMap::new();
    let session = Session::new().unwrap();
    let mut s = explicit_quadratic();
    s.points[1].y = length(0.); // Coincident adjacent poles are valid for an explicit basis.
    s.splines[0].basis.as_mut().unwrap().weights.clear();
    s.profile = vec!["curve".into()];
    let wire = s.open_wire(&session, &params, None).unwrap();
    assert!(session.is_valid(&wire).unwrap());
    let mut s = explicit_quadratic();
    s.splines[0].points.push("a".into());
    let b = s.splines[0].basis.as_mut().unwrap();
    b.knots = vec![0., 0.5, 1.];
    b.multiplicities = vec![3, 1, 3];
    b.weights.clear();
    s.profile = vec!["curve".into()];
    let face = s.face_on_plane(&session, &params, None).unwrap();
    assert!(session.is_valid(&face).unwrap());
    assert!(session.surface_area(&face).unwrap() > 1.);
}

fn periodic_square() -> SketchDefinition {
    let mut s = base();
    s.points = vec![
        point("a", 10., 10., true),
        point("b", -10., 10., true),
        point("c", -10., -10., true),
        point("d", 10., -10., true),
    ];
    s.splines.push(SketchSpline {
        endpoints: None,
        id: "curve".into(),
        points: vec!["a".into(), "b".into(), "c".into(), "d".into()],
        basis: Some(SketchBSplineBasis {
            non_clamped: false,
            periodic: true,
            degree: 3,
            knots: vec![0., 1., 2., 3., 4.],
            multiplicities: vec![1; 5],
            weights: vec![],
        }),
    });
    s.profile = vec!["curve".into()];
    s
}

#[test]
fn periodic_explicit_sketch_point_membership_uses_the_closed_curve_not_the_poles() {
    let params = HashMap::new();
    let mut s = periodic_square();
    s.points
        .extend([point("p", 0., 10., false), point("axis", 0., 0., true)]);
    s.lines.push(line("vertical", "axis", "p"));
    s.constraints = vec![
        SketchConstraint::PointOnCurve {
            point: "p".into(),
            curve: "curve".into(),
        },
        SketchConstraint::Vertical {
            line: "vertical".into(),
        },
    ];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved, "{}", solved.max_residual);
    assert_eq!(solved.free_degrees, 0);
    assert!((solved.points["p"].y - 110. / 12.).abs() < 1e-7);
    let session = Session::new().unwrap();
    let face = s.face_on_plane(&session, &params, None).unwrap();
    assert!(session.is_valid(&face).unwrap());
    assert!((session.surface_area(&face).unwrap() - 122. / 45. * 100.).abs() < 1e-5);
    assert!(s.open_wire(&session, &params, None).is_err());
}

#[test]
fn periodic_explicit_sketch_rejects_invalid_basis_mixed_profiles_and_endpoint_tangency() {
    let params = HashMap::new();
    for case in 0..3 {
        let mut s = periodic_square();
        let b = s.splines[0].basis.as_mut().unwrap();
        match case {
            0 => b.multiplicities[4] = 2,
            1 => b.multiplicities[1] = 2,
            _ => b.multiplicities[0] = 4,
        }
        assert!(s.solve(&params).is_err());
    }
    let mut s = periodic_square();
    s.lines.push(line("line", "a", "b"));
    s.constraints = vec![SketchConstraint::Tangent {
        first: "curve".into(),
        second: "line".into(),
        point: "a".into(),
    }];
    assert!(
        s.solve(&params)
            .unwrap_err()
            .message
            .contains("no free ends")
    );
    s.constraints.clear();
    s.profile.push("line".into());
    let session = Session::new().unwrap();
    assert!(s.face_on_plane(&session, &params, None).is_err());
}

fn non_clamped_cap() -> SketchDefinition {
    let mut s = explicit_quadratic();
    s.points = vec![
        point("a", 0., 0., true),
        point("b", 10., 10., true),
        point("c", 20., 0., true),
        point("start", 4., 4., false),
        point("end", 16., 4., false),
    ];
    let b = s.splines[0].basis.as_mut().unwrap();
    b.non_clamped = true;
    b.knots = vec![0., 1., 2., 3., 4., 5.];
    b.multiplicities = vec![1; 6];
    b.weights.clear();
    s.splines[0].endpoints = Some(SketchSplineEndpoints {
        start: "start".into(),
        end: "end".into(),
    });
    s.lines = vec![line("chord", "end", "start")];
    s.profile = vec!["curve".into(), "chord".into()];
    s
}

#[test]
fn non_clamped_spline_endpoint_equations_drive_real_native_profile_connections() {
    let params = HashMap::new();
    let s = non_clamped_cap();
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved, "{}", solved.max_residual);
    assert_eq!(solved.free_degrees, 0);
    assert!(
        (solved.points["start"].x - 5.).abs() < 1e-7
            && (solved.points["start"].y - 5.).abs() < 1e-7
    );
    assert!(
        (solved.points["end"].x - 15.).abs() < 1e-7 && (solved.points["end"].y - 5.).abs() < 1e-7
    );
    let session = Session::new().unwrap();
    let face = s.face_on_plane(&session, &params, None).unwrap();
    assert!(session.is_valid(&face).unwrap());
    assert!((session.surface_area(&face).unwrap() - 100. / 6.).abs() < 1e-5);
    let mut arc_cap = s.clone();
    arc_cap.points.push(point("center", 10., 5., true));
    arc_cap.lines.clear();
    arc_cap.arcs.push(SketchArc {
        id: "arc".into(),
        center: "center".into(),
        start: "end".into(),
        end: "start".into(),
        clockwise: true,
    });
    arc_cap.profile = vec!["curve".into(), "arc".into()];
    let curved_face = arc_cap.face_on_plane(&session, &params, None).unwrap();
    assert!(session.is_valid(&curved_face).unwrap());
    assert!(
        (session.surface_area(&curved_face).unwrap()
            - (100. / 6. + 25. * std::f64::consts::PI / 2.))
            .abs()
            < 1e-5
    );
    let mut conflict = s.clone();
    conflict.points[3].fixed = true;
    let solved = conflict.solve(&params).unwrap();
    assert!(!solved.solved);
    assert!(solved.max_residual > 0.5);
    // A non-periodic native loop can share one named endpoint without using
    // the first/last poles as the closure point.
    let mut loop_sketch = s.clone();
    loop_sketch.points = vec![
        point("a", -1., 0., true),
        point("b", 1., 0., true),
        point("c", 3., 3., true),
        point("d", -3., 3., true),
        point("e", -1., 0., true),
        point("f", 1., 0., true),
        point("start", 0.1, 0.1, false),
    ];
    loop_sketch.splines[0].points = ["a", "b", "c", "d", "e", "f"].map(String::from).to_vec();
    loop_sketch.splines[0].endpoints.as_mut().unwrap().end = "start".into();
    let basis = loop_sketch.splines[0].basis.as_mut().unwrap();
    basis.knots = (0..9).map(f64::from).collect();
    basis.multiplicities = vec![1; 9];
    loop_sketch.lines.clear();
    loop_sketch.profile = vec!["curve".into()];
    let solution = loop_sketch.solve(&params).unwrap();
    assert!(solution.solved);
    let face = loop_sketch.face_on_plane(&session, &params, None).unwrap();
    assert!(session.is_valid(&face).unwrap());
    assert!(session.surface_area(&face).unwrap() > 1.);
    loop_sketch.points.last_mut().unwrap().fixed = true;
    let conflict = loop_sketch.solve(&params).unwrap();
    assert!(!conflict.solved);
    assert!(
        !loop_sketch
            .preview_curves(&session, &conflict, 32)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn non_clamped_endpoint_tangency_uses_native_derivative_and_moves_free_poles() {
    let params = HashMap::new();
    let mut s = non_clamped_cap();
    s.points[1] = point("b", 9., 10., false);
    s.points.push(point("neighbor", -5., -5., true));
    s.lines.push(line("line", "neighbor", "start"));
    s.constraints = vec![
        SketchConstraint::Tangent {
            first: "line".into(),
            second: "curve".into(),
            point: "start".into(),
        },
        SketchConstraint::Distance {
            first: "a".into(),
            second: "b".into(),
            value: length(200_f64.sqrt()),
        },
    ];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved, "{}", solved.max_residual);
    assert_eq!(solved.free_degrees, 0);
    assert!((solved.points["b"].x - 10.).abs() < 1e-7 && (solved.points["b"].y - 10.).abs() < 1e-7);
    let check = &s.constraint_checks(&params, &solved).unwrap()[0];
    assert!(check.satisfied && !check.by_construction);
    assert!(check.max_residual.unwrap() < 1e-7);
}

#[test]
fn non_clamped_endpoint_definitions_are_checked_and_old_splines_stay_unchanged() {
    let params = HashMap::new();
    for case in 0..5 {
        let mut s = non_clamped_cap();
        match case {
            0 => s.splines[0].endpoints = None,
            1 => s.splines[0].endpoints.as_mut().unwrap().start = "missing".into(),
            2 => s.splines[0].endpoints.as_mut().unwrap().start = "a".into(),
            3 => s.splines[0].basis.as_mut().unwrap().periodic = true,
            _ => s.splines[0].basis.as_mut().unwrap().non_clamped = false,
        }
        assert!(s.solve(&params).is_err());
    }
}

#[test]
fn non_clamped_cubic_tangency_is_not_the_first_control_polygon_edge() {
    let params = HashMap::new();
    let mut s = non_clamped_cap();
    s.points = vec![
        point("a", 0., 0., true),
        point("b", 10., 0., true),
        point("c", 10., 10., true),
        point("d", 20., 10., true),
        point("start", 8., 1., false),
        point("end", 12., 9., false),
        point("neighbor", 10. / 3., -10. / 3., true),
    ];
    s.splines[0].points = vec!["a".into(), "b".into(), "c".into(), "d".into()];
    let b = s.splines[0].basis.as_mut().unwrap();
    b.degree = 3;
    b.knots = (0..8).map(|i| i as f64).collect();
    b.multiplicities = vec![1; 8];
    s.lines = vec![line("line", "neighbor", "start")];
    s.profile = vec!["curve".into()];
    s.constraints = vec![SketchConstraint::Tangent {
        first: "line".into(),
        second: "curve".into(),
        point: "start".into(),
    }];
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved, "{}", solved.max_residual);
    assert!(s.constraint_checks(&params, &solved).unwrap()[0].satisfied);
    assert!((solved.points["start"].x - 25. / 3.).abs() < 1e-7);
    assert!((solved.points["start"].y - 5. / 3.).abs() < 1e-7);
}

#[test]
fn independent_line_ellipse_tangency_solves_rotated_axes_sides_and_supporting_lines() {
    let params = HashMap::new();
    for (theta, phi, a, b, side) in [
        (0_f64, 0_f64, 5_f64, 3_f64, SketchLineSide::Left),
        (30., 0., 5., 3., SketchLineSide::Left),
        (30., 45., 5., 3., SketchLineSide::Left),
        (75., -30., 5., 3., SketchLineSide::Right),
        (20., 180., 3., 3., SketchLineSide::Left),
    ] {
        let mut s = base();
        let (st, ct) = theta.to_radians().sin_cos();
        let (sp, cp) = phi.to_radians().sin_cos();
        let world = |x, y| (2. + cp * x - sp * y, -3. + sp * x + cp * y);
        let support =
            (a * (theta - phi).to_radians().sin()).hypot(b * (theta - phi).to_radians().cos());
        let sign = if side == SketchLineSide::Left {
            1.
        } else {
            -1.
        };
        let (x0, y0) = world(-10., -sign * support + 0.7);
        let (x1, y1) = world(-8., -sign * support + 0.7);
        let (rx, ry) = world(1., 0.);
        let (vx, vy) = world(0., 1.);
        s.points = vec![
            point("c", 2., -3., true),
            point("major", 2. + a * ct, -3. + a * st, true),
            point("minor", 2. - b * st, -3. + b * ct, true),
            point("p", x0, y0, false),
            point("q", x1, y1, false),
            point("ref", rx, ry, true),
            point("up", vx, vy, true),
        ];
        s.lines = vec![
            line("line", "p", "q"),
            line("ref", "c", "ref"),
            line("vertical", "c", "up"),
        ];
        s.ellipses = vec![SketchEllipse {
            id: "ellipse".into(),
            center: "c".into(),
            major: "major".into(),
            minor: "minor".into(),
        }];
        s.constraints = vec![
            SketchConstraint::Parallel {
                first: "line".into(),
                second: "ref".into(),
            },
            SketchConstraint::Distance {
                first: "p".into(),
                second: "q".into(),
                value: length(2.),
            },
            SketchConstraint::PointLineDistance {
                point: "p".into(),
                line: "vertical".into(),
                value: length(10.),
            },
            SketchConstraint::LineEllipseTangent {
                line: "line".into(),
                ellipse: "ellipse".into(),
                side,
            },
        ];
        let solved = s.solve(&params).unwrap();
        assert!(
            solved.solved,
            "theta {theta}, phi {phi}, residual {}",
            solved.max_residual
        );
        assert_eq!(solved.free_degrees, 0);
        let expected = world(-10., -sign * support);
        assert!(
            (solved.points["p"].x - expected.0).hypot(solved.points["p"].y - expected.1) < 1e-7
        );
        let check = &s.constraint_checks(&params, &solved).unwrap()[3];
        assert!(check.satisfied && !check.by_construction);
        let [foot, contact] = s.line_ellipse_tangent_contacts(&solved).unwrap()[&3];
        assert!((foot.x - contact.x).hypot(foot.y - contact.y) < 1e-7);
        let local_x = (contact.x - 2.) * ct + (contact.y + 3.) * st;
        let local_y = -(contact.x - 2.) * st + (contact.y + 3.) * ct;
        assert!(((local_x / a).powi(2) + (local_y / b).powi(2) - 1.).abs() < 1e-10);
        let tangent_dot =
            local_x / a.powi(2) * (cp * ct + sp * st) + local_y / b.powi(2) * (-cp * st + sp * ct);
        assert!(tangent_dot.abs() < 1e-10);
        assert!((contact.x - 2.) * cp + (contact.y + 3.) * sp > -8.); // Contact outside the finite line segment.
        s.points[3].fixed = true;
        s.points[4].fixed = true;
        let conflict = s.solve(&params).unwrap();
        assert!(!conflict.solved);
        let check = &s.constraint_checks(&params, &conflict).unwrap()[3];
        assert!(!check.satisfied && check.max_residual.unwrap() > 0.6);
        let [foot, contact] = s.line_ellipse_tangent_contacts(&conflict).unwrap()[&3];
        assert!((foot.x - contact.x).hypot(foot.y - contact.y) > 0.6);
    }
}

#[test]
fn line_ellipse_tangency_rejects_wrong_entities_missing_lines_and_zero_axes() {
    let mut s = base();
    s.points = vec![
        point("c", 0., 0., true),
        point("a", 5., 0., true),
        point("b", 0., 3., true),
        point("p", -10., -3., true),
        point("q", 10., -3., true),
    ];
    s.lines = vec![line("line", "p", "q")];
    s.ellipses = vec![SketchEllipse {
        id: "ellipse".into(),
        center: "c".into(),
        major: "a".into(),
        minor: "b".into(),
    }];
    let relation = SketchConstraint::LineEllipseTangent {
        line: "line".into(),
        ellipse: "ellipse".into(),
        side: SketchLineSide::Left,
    };
    s.constraints = vec![relation.clone()];
    for case in 0..4 {
        let mut bad = s.clone();
        match case {
            0 => {
                let SketchConstraint::LineEllipseTangent { ellipse, .. } = &mut bad.constraints[0]
                else {
                    unreachable!()
                };
                *ellipse = "line".into();
            }
            1 => {
                let SketchConstraint::LineEllipseTangent { line, .. } = &mut bad.constraints[0]
                else {
                    unreachable!()
                };
                *line = "missing".into();
            }
            2 => bad.points[2] = point("b", 0., 0., true),
            _ => bad.points[4] = point("q", -10., -3., true),
        }
        assert!(bad.solve(&HashMap::new()).is_err());
    }
    assert!(s.solve(&HashMap::new()).unwrap().solved);
    let json = serde_json::to_string(&relation).unwrap();
    assert!(json.contains("line_ellipse_tangent"));
    assert_eq!(
        serde_json::from_str::<SketchConstraint>(&json).unwrap(),
        relation
    );
}

fn independent_spline_contact_sketch() -> (SketchDefinition, HashMap<String, ParameterValue>) {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/spline-tangent-cap.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&r["model"].to_string()).unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &doc.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let params = part.resolved_parameters().unwrap();
    let FeatureOperation::SketchFace { sketch } = &doc.family.features[0].operation else {
        unreachable!()
    };
    ((**sketch).clone(), params)
}

#[test]
fn independent_spline_contact_solves_native_interior_and_rational_weight_edits() {
    let (s, mut params) = independent_spline_contact_sketch();
    for weight in [1., 2.] {
        params.insert(
            "middle_weight".into(),
            ParameterValue::Scalar(Quantity::scalar(weight)),
        );
        let solved = s.solve(&params).unwrap();
        assert!(solved.solved, "residual {}", solved.max_residual);
        assert_eq!(solved.free_degrees, 0);
        assert!(solved.points["contact"].x.abs() < 1e-7);
        let height = 10. * weight / (1. + weight);
        assert!((solved.points["contact"].y - height).abs() < 1e-7);
        assert!((solved.points["line-start"].y - height).abs() < 1e-7);
        let checks = s.constraint_checks(&params, &solved).unwrap();
        assert!(checks[3].satisfied && !checks[3].by_construction);
        let contacts = s.line_spline_tangent_contacts(&params, &solved).unwrap();
        let (points, sine) = contacts[&3];
        assert!(sine.abs() < 1e-9);
        for p in points {
            assert!(p.x.abs() < 1e-7 && (p.y - height).abs() < 1e-7);
        }
    }
    let (mut free, free_params) = independent_spline_contact_sketch();
    for p in &mut free.points {
        if p.id == "line-start" || p.id == "line-end" {
            p.fixed = true;
            p.y = length(5.);
        }
        if p.id == "b" {
            p.fixed = false;
            p.y = length(8.);
        }
    }
    free.constraints = vec![
        SketchConstraint::PointLineDistance {
            point: "b".into(),
            line: "vertical-axis".into(),
            value: length(0.),
        },
        SketchConstraint::LineSplineTangent {
            line: "tangent-line".into(),
            spline: "curve".into(),
            point: "contact".into(),
        },
    ];
    let solved = free.solve(&free_params).unwrap();
    assert!(
        solved.solved && solved.free_degrees == 0,
        "{}",
        solved.max_residual
    );
    assert!((solved.points["b"].y - 10.).abs() < 1e-7);
    let mut conflict = s.clone();
    for p in &mut conflict.points {
        p.fixed = true;
    }
    let solved = conflict.solve(&params).unwrap();
    assert!(!solved.solved);
    let check = &conflict.constraint_checks(&params, &solved).unwrap()[3];
    assert!(!check.satisfied && !check.by_construction && check.max_residual.unwrap() > 0.1);
}

#[test]
fn independent_spline_contact_accepts_non_clamped_periodic_and_interpolated_curves() {
    let (mut s, params) = independent_spline_contact_sketch();
    s.splines[0].basis = None; // Interpolation through (-10,0), (0,10), (10,0).
    s.points.last_mut().unwrap().y = length(9.5);
    let solved = s.solve(&params).unwrap();
    assert!(solved.solved && (solved.points["contact"].y - 10.).abs() < 1e-7);
    assert!(!s.constraint_checks(&params, &solved).unwrap()[3].by_construction);
    let (mut s, params) = independent_spline_contact_sketch();
    let b = s.splines[0].basis.as_mut().unwrap();
    b.non_clamped = true;
    b.knots = (0..6).map(f64::from).collect();
    b.multiplicities = vec![1; 6];
    s.points
        .extend([point("start", -5., 5., false), point("end", 5., 5., false)]);
    s.points.iter_mut().find(|p| p.id == "contact").unwrap().y = length(7.3);
    s.splines[0].endpoints = Some(SketchSplineEndpoints {
        start: "start".into(),
        end: "end".into(),
    });
    s.lines[0] = line("chord", "end", "start");
    let solved = s.solve(&params).unwrap();
    assert!(
        solved.solved && (solved.points["contact"].y - 7.5).abs() < 1e-7,
        "{}",
        solved.max_residual
    );
    let session = Session::new().unwrap();
    let face = s.face_on_plane(&session, &params, None).unwrap();
    assert!(session.is_valid(&face).unwrap());
    let (mut s, params) = independent_spline_contact_sketch();
    s.points[0] = point("a", 10., 8., true);
    s.points[1] = point("b", -10., 8., true);
    s.points[2] = point("c", -10., -8., true);
    s.points.push(point("d", 10., -8., true));
    s.splines[0].points.push("d".into());
    let b = s.splines[0].basis.as_mut().unwrap();
    b.periodic = true;
    b.degree = 3;
    b.knots = (0..5).map(f64::from).collect();
    b.multiplicities = vec![1; 5];
    b.weights.clear();
    s.points.iter_mut().find(|p| p.id == "contact").unwrap().y = length(7.2);
    s.profile = vec!["curve".into()];
    let solved = s.solve(&params).unwrap();
    assert!(
        solved.solved && (solved.points["contact"].y - 22. / 3.).abs() < 1e-7,
        "{}",
        solved.max_residual
    );
    let face = s.face_on_plane(&session, &params, None).unwrap();
    assert!(session.is_valid(&face).unwrap());
}

#[test]
fn independent_spline_contact_rejects_missing_roles_and_contacts_outside_bounded_curve() {
    let (s, params) = independent_spline_contact_sketch();
    for role in 0..3 {
        let mut bad = s.clone();
        let SketchConstraint::LineSplineTangent {
            line,
            spline,
            point,
        } = &mut bad.constraints[3]
        else {
            unreachable!()
        };
        match role {
            0 => *line = "missing".into(),
            1 => *spline = "chord".into(),
            _ => *point = "missing".into(),
        };
        assert!(bad.solve(&params).is_err());
    }
    let mut invalid_solution = s.solve(&params).unwrap();
    invalid_solution.points.get_mut("line-start").unwrap().x = f64::NAN;
    assert!(
        s.line_spline_tangent_contacts(&params, &invalid_solution)
            .is_err()
    );
    let mut s = s;
    for p in &mut s.points {
        p.fixed = true;
    }
    s.points.iter_mut().find(|p| p.id == "contact").unwrap().x = length(30.);
    let solved = s.solve(&params).unwrap();
    assert!(!solved.solved);
    assert!(
        s.constraint_checks(&params, &solved).unwrap()[3]
            .max_residual
            .unwrap()
            > 10.
    );
}

fn spline_circle_contact_sketch() -> (SketchDefinition, HashMap<String, ParameterValue>) {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/spline-tangent-boss.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&r["model"].to_string()).unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &doc.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let params = part.resolved_parameters().unwrap();
    let FeatureOperation::SketchFace { sketch } = &doc.family.features[0].operation else {
        unreachable!()
    };
    ((**sketch).clone(), params)
}

#[test]
fn spline_circle_contact_solves_rational_weights_and_both_local_contact_branches() {
    for below in [false, true] {
        let (mut s, mut params) = spline_circle_contact_sketch();
        if below {
            for id in ["boss-center", "boss-rim"] {
                s.points.iter_mut().find(|p| p.id == id).unwrap().y = length(2.);
            }
        }
        for weight in [1., 2.] {
            params.insert(
                "middle_weight".into(),
                ParameterValue::Scalar(Quantity::scalar(weight)),
            );
            let solved = s.solve(&params).unwrap();
            assert!(solved.solved, "{}", solved.max_residual);
            assert_eq!(solved.free_degrees, 0);
            let peak = 10. * weight / (1. + weight);
            assert!(
                solved.points["contact"].x.abs() < 1e-7
                    && (solved.points["contact"].y - peak).abs() < 1e-7
            );
            let cy = peak + if below { -3. } else { 3. };
            assert!((solved.points["boss-center"].y - cy).abs() < 1e-7);
            let check = &s.constraint_checks(&params, &solved).unwrap()[3];
            assert!(check.satisfied && !check.by_construction);
            let c = s.spline_circle_tangent_contacts(&params, &solved).unwrap()[&3];
            assert!(c.tangent_sine.abs() < 1e-9 && c.arc_span_residual_mm.is_none());
            for p in c.points {
                assert!(p.x.abs() < 1e-7 && (p.y - peak).abs() < 1e-7);
            }
        }
    }
    let (mut s, params) = spline_circle_contact_sketch();
    // The circle is fixed; a free spline pole must move to create the contact.
    for p in &mut s.points {
        if p.id == "boss-center" || p.id == "boss-rim" {
            p.fixed = true;
        }
        if p.id == "b" {
            p.fixed = false;
            p.y = length(8.);
        }
    }
    s.constraints = vec![
        SketchConstraint::PointLineDistance {
            point: "b".into(),
            line: "vertical-axis".into(),
            value: length(0.),
        },
        SketchConstraint::SplineCircleTangent {
            spline: "curve".into(),
            circle: "boss".into(),
            point: "contact".into(),
        },
    ];
    let solved = s.solve(&params).unwrap();
    assert!(
        solved.solved && solved.free_degrees == 0,
        "{}",
        solved.max_residual
    );
    assert!((solved.points["b"].y - 10.).abs() < 1e-7);
    for mode in 0..3 {
        let (mut s, params) = spline_circle_contact_sketch();
        let peak = match mode {
            0 => {
                s.splines[0].basis = None;
                10.
            }
            1 => {
                let b = s.splines[0].basis.as_mut().unwrap();
                b.non_clamped = true;
                b.knots = (0..6).map(f64::from).collect();
                b.multiplicities = vec![1; 6];
                s.points
                    .extend([point("start", -5., 5., true), point("end", 5., 5., true)]);
                s.splines[0].endpoints = Some(SketchSplineEndpoints {
                    start: "start".into(),
                    end: "end".into(),
                });
                7.5
            }
            _ => {
                s.points[0] = point("a", 10., 8., true);
                s.points[1] = point("b", -10., 8., true);
                s.points[2] = point("c", -10., -8., true);
                s.points.push(point("d", 10., -8., true));
                s.splines[0].points.push("d".into());
                let b = s.splines[0].basis.as_mut().unwrap();
                b.periodic = true;
                b.degree = 3;
                b.knots = (0..5).map(f64::from).collect();
                b.multiplicities = vec![1; 5];
                b.weights.clear();
                22. / 3.
            }
        };
        s.points.iter_mut().find(|p| p.id == "contact").unwrap().y = length(peak - 0.2);
        for id in ["boss-center", "boss-rim"] {
            s.points.iter_mut().find(|p| p.id == id).unwrap().y = length(peak + 3.);
        }
        let solved = s.solve(&params).unwrap();
        assert!(
            solved.solved && solved.free_degrees == 0,
            "mode {mode}: {}",
            solved.max_residual
        );
        assert!((solved.points["contact"].y - peak).abs() < 1e-7);
        let session = Session::new().unwrap();
        let face = s.face_on_plane(&session, &params, None).unwrap();
        assert!(session.is_valid(&face).unwrap());
    }
}

#[test]
fn spline_circle_contact_checks_directed_arc_spans_and_reports_excluded_contacts() {
    let (mut s, params) = spline_circle_contact_sketch();
    let solved = s.solve(&params).unwrap();
    for p in &mut s.points {
        let q = solved.points[&p.id];
        p.x = length(q.x);
        p.y = length(q.y);
        p.fixed = true;
    }
    s.points.push(point("left-rim", -3., 8., true));
    s.circles.clear();
    s.arcs = vec![SketchArc {
        id: "boss".into(),
        center: "boss-center".into(),
        start: "boss-rim".into(),
        end: "left-rim".into(),
        clockwise: true,
    }];
    s.profile.clear();
    for clockwise in [true, false] {
        s.arcs[0].clockwise = clockwise;
        let solved = s.solve(&params).unwrap();
        assert_eq!(solved.solved, clockwise);
        let c = s.spline_circle_tangent_contacts(&params, &solved).unwrap()[&3];
        assert!(c.tangent_sine.abs() < 1e-9);
        if clockwise {
            assert!(c.arc_span_residual_mm.unwrap() < 1e-9);
        } else {
            assert!(
                (c.arc_span_residual_mm.unwrap() - 3. * std::f64::consts::FRAC_PI_2).abs() < 1e-7
            );
        }
        assert_eq!(
            s.constraint_checks(&params, &solved).unwrap()[3].satisfied,
            clockwise
        );
    }
    // Wrap through zero degrees, with the contact exactly at the start endpoint.
    s.arcs[0].start = "contact".into();
    s.arcs[0].end = "boss-rim".into();
    s.arcs[0].clockwise = false;
    assert!(s.solve(&params).unwrap().solved);
}

#[test]
fn spline_circle_contact_rejects_wrong_roles_degenerate_contacts_and_fixed_conflicts() {
    let (s, params) = spline_circle_contact_sketch();
    for role in 0..3 {
        let mut bad = s.clone();
        let SketchConstraint::SplineCircleTangent {
            spline,
            circle,
            point,
        } = &mut bad.constraints[3]
        else {
            unreachable!()
        };
        match role {
            0 => *spline = "boss".into(),
            1 => *circle = "curve".into(),
            _ => *point = "missing".into(),
        };
        assert!(bad.solve(&params).is_err());
    }
    let mut zero = s.clone();
    zero.points
        .iter_mut()
        .find(|p| p.id == "contact")
        .unwrap()
        .x = length(0.);
    zero.points
        .iter_mut()
        .find(|p| p.id == "contact")
        .unwrap()
        .y = length(8.);
    assert!(zero.solve(&params).is_err());
    let mut conflict = s;
    for p in &mut conflict.points {
        p.fixed = true;
    }
    let solved = conflict.solve(&params).unwrap();
    assert!(!solved.solved);
    let check = &conflict.constraint_checks(&params, &solved).unwrap()[3];
    assert!(!check.satisfied && !check.by_construction);
    let mut invalid = solved;
    invalid.points.get_mut("boss-center").unwrap().x = f64::NAN;
    assert!(
        conflict
            .spline_circle_tangent_contacts(&params, &invalid)
            .is_err()
    );
}

fn spline_pair_contact_sketch() -> (SketchDefinition, HashMap<String, ParameterValue>) {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/spline-tangent-pair.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&r["model"].to_string()).unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &doc.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let params = part.resolved_parameters().unwrap();
    let FeatureOperation::SketchFace { sketch } = &doc.family.features[0].operation else {
        unreachable!()
    };
    ((**sketch).clone(), params)
}

#[test]
fn spline_pair_contact_positions_free_poles_and_tracks_rational_weight_edits() {
    let (s, mut params) = spline_pair_contact_sketch();
    for weight in [1., 2.] {
        params.insert(
            "middle_weight".into(),
            ParameterValue::Scalar(Quantity::scalar(weight)),
        );
        let solved = s.solve(&params).unwrap();
        assert!(solved.solved, "{}", solved.max_residual);
        assert_eq!(solved.free_degrees, 0);
        let peak = 10. * weight / (1. + weight);
        assert!(
            solved.points["contact"].x.abs() < 1e-7
                && (solved.points["contact"].y - peak).abs() < 1e-7
        );
        assert!(
            (solved.points["upper-left"].y - peak - 4.).abs() < 1e-7
                && (solved.points["upper-pole"].y - peak + 4.).abs() < 1e-7
        );
        let check = &s.constraint_checks(&params, &solved).unwrap()[5];
        assert!(check.satisfied && !check.by_construction);
        let (points, sine) = s.spline_spline_tangent_contacts(&params, &solved).unwrap()[&5];
        assert!(sine.abs() < 1e-9);
        for p in points {
            assert!(p.x.abs() < 1e-7 && (p.y - peak).abs() < 1e-7);
        }
    }
    let mut reversed = s.clone();
    reversed.splines[1].points.reverse();
    reversed.profile.clear();
    assert!(reversed.solve(&params).unwrap().solved); // Unoriented tangency permits opposite native derivatives.
}

#[test]
fn spline_pair_contact_checks_bounded_endpoints_non_tangent_crossings_and_bad_roles() {
    let (s, params) = spline_pair_contact_sketch();
    for role in 0..3 {
        let mut bad = s.clone();
        let SketchConstraint::SplineSplineTangent {
            first,
            second,
            point,
        } = &mut bad.constraints[5]
        else {
            unreachable!()
        };
        match role {
            0 => *second = first.clone(),
            1 => *second = "chord".into(),
            _ => *point = "missing".into(),
        };
        assert!(bad.solve(&params).is_err());
    }
    let mut fixed = s.clone();
    for p in &mut fixed.points {
        p.fixed = true;
    }
    let solved = fixed.solve(&params).unwrap();
    assert!(!solved.solved && !fixed.constraint_checks(&params, &solved).unwrap()[5].satisfied);
    let mut invalid = solved;
    invalid.points.get_mut("contact").unwrap().x = f64::NAN;
    assert!(
        fixed
            .spline_spline_tangent_contacts(&params, &invalid)
            .is_err()
    );
    let mut s = base();
    s.points = vec![
        point("a", 0., 0., true),
        point("b", 1., 1., true),
        point("c", 2., 1., true),
        point("d", 2., 1., true),
        point("e", 3., 1., true),
        point("f", 4., 2., true),
        point("p", 2., 1., true),
    ];
    let basis = SketchBSplineBasis {
        non_clamped: false,
        periodic: false,
        degree: 2,
        knots: vec![0., 1.],
        multiplicities: vec![3, 3],
        weights: vec![],
    };
    s.splines = vec![
        SketchSpline {
            id: "first".into(),
            points: vec!["a".into(), "b".into(), "c".into()],
            basis: Some(basis.clone()),
            endpoints: None,
        },
        SketchSpline {
            id: "second".into(),
            points: vec!["d".into(), "e".into(), "f".into()],
            basis: Some(basis),
            endpoints: None,
        },
    ];
    s.constraints = vec![SketchConstraint::SplineSplineTangent {
        first: "first".into(),
        second: "second".into(),
        point: "p".into(),
    }];
    assert!(s.solve(&HashMap::new()).unwrap().solved); // Separate IDs, shared geometric endpoint, equal tangent.
    s.points[4] = point("e", 2., 2., true);
    let solved = s.solve(&HashMap::new()).unwrap();
    assert!(!solved.solved);
    let (points, sine) = s
        .spline_spline_tangent_contacts(&HashMap::new(), &solved)
        .unwrap()[&0];
    assert_eq!(points[0], points[1]);
    assert_eq!(points[0], points[2]);
    assert!(sine.abs() > 0.9); // Position alone cannot prove tangency.
    s.points[6] = point("p", 30., 1., true);
    assert!(s.solve(&HashMap::new()).unwrap().max_residual > 20.);
}

#[test]
fn spline_pair_contact_accepts_interpolated_non_clamped_and_periodic_first_curves() {
    for mode in 0..3 {
        let (mut s, params) = spline_pair_contact_sketch();
        let peak = match mode {
            0 => {
                s.splines[0].basis = None;
                10.
            }
            1 => {
                let b = s.splines[0].basis.as_mut().unwrap();
                b.non_clamped = true;
                b.knots = (0..6).map(f64::from).collect();
                b.multiplicities = vec![1; 6];
                s.points
                    .extend([point("start", -5., 5., true), point("end", 5., 5., true)]);
                s.splines[0].endpoints = Some(SketchSplineEndpoints {
                    start: "start".into(),
                    end: "end".into(),
                });
                7.5
            }
            _ => {
                s.points[0] = point("a", 10., 8., true);
                s.points[1] = point("b", -10., 8., true);
                s.points[2] = point("c", -10., -8., true);
                s.points.push(point("d", 10., -8., true));
                s.splines[0].points.push("d".into());
                let b = s.splines[0].basis.as_mut().unwrap();
                b.periodic = true;
                b.degree = 3;
                b.knots = (0..5).map(f64::from).collect();
                b.multiplicities = vec![1; 5];
                b.weights.clear();
                22. / 3.
            }
        };
        s.points.iter_mut().find(|p| p.id == "contact").unwrap().y = length(peak - 0.2);
        let solved = s.solve(&params).unwrap();
        assert!(
            solved.solved && solved.free_degrees == 0,
            "mode {mode}: {}",
            solved.max_residual
        );
        assert!((solved.points["contact"].y - peak).abs() < 1e-7);
        let session = Session::new().unwrap();
        let face = s.face_on_plane(&session, &params, None).unwrap();
        assert!(session.is_valid(&face).unwrap());
    }
}

fn spline_ellipse_contact_sketch() -> (SketchDefinition, HashMap<String, ParameterValue>) {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/spline-tangent-ellipse.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&r["model"].to_string()).unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &doc.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let FeatureOperation::SketchFace { sketch } = &doc.family.features[0].operation else {
        unreachable!()
    };
    ((**sketch).clone(), part.resolved_parameters().unwrap())
}

#[test]
fn spline_ellipse_contact_solves_native_positions_rotation_and_rational_weight() {
    let (s, mut params) = spline_ellipse_contact_sketch();
    for (weight, tilt) in [(1., 0.), (2., 0.), (0.5, 0.4), (2., 1.0471975511965976)] {
        params.insert(
            "middle_weight".into(),
            ParameterValue::Scalar(Quantity::scalar(weight)),
        );
        params.insert(
            "tilt".into(),
            ParameterValue::Scalar(Quantity::scalar(tilt)),
        );
        let solved = s.solve(&params).unwrap();
        assert!(solved.solved, "{}", solved.max_residual);
        assert_eq!(solved.free_degrees, 0);
        let (p, sine) = s.spline_ellipse_tangent_contacts(&params, &solved).unwrap()[&4];
        assert!(sine.abs() < 1e-9);
        for q in &p[1..] {
            assert!((q.x - p[0].x).abs() < 1e-9 && (q.y - p[0].y).abs() < 1e-9);
        }
        let check = &s.constraint_checks(&params, &solved).unwrap()[4];
        assert!(check.satisfied && !check.by_construction);
        if tilt == 0. {
            assert!(p[0].x.abs() < 1e-7 && (p[0].y - 10. * weight / (1. + weight)).abs() < 1e-7);
        }
    }
}

#[test]
fn spline_ellipse_contact_rejects_bad_roles_and_reports_fixed_conflicts() {
    let (s, params) = spline_ellipse_contact_sketch();
    for role in 0..3 {
        let mut bad = s.clone();
        let SketchConstraint::SplineEllipseTangent {
            spline,
            ellipse,
            point,
        } = &mut bad.constraints[4]
        else {
            unreachable!()
        };
        match role {
            0 => *spline = "baseline".into(),
            1 => *ellipse = "curve".into(),
            _ => *point = "missing".into(),
        }
        assert!(bad.solve(&params).is_err());
    }
    let mut fixed = s.clone();
    for p in &mut fixed.points {
        p.fixed = true;
    }
    let solved = fixed.solve(&params).unwrap();
    assert!(!solved.solved);
    assert!(!fixed.constraint_checks(&params, &solved).unwrap()[4].satisfied);
    let (p, _) = fixed
        .spline_ellipse_tangent_contacts(&params, &solved)
        .unwrap()[&4];
    assert!((p[0].x - p[1].x).hypot(p[0].y - p[1].y) > 0.1);
    let mut invalid = solved.clone();
    invalid.points.remove("contact");
    assert!(
        fixed
            .spline_ellipse_tangent_contacts(&params, &invalid)
            .is_err()
    );
}

fn ellipse_circle_contact_sketch() -> (SketchDefinition, HashMap<String, ParameterValue>) {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/model/ellipse-tangent-boss.request.json"
    ))
    .unwrap();
    let doc = ModelDocument::from_json(&r["model"].to_string()).unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: &doc.family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let FeatureOperation::SketchFace { sketch } = &doc.family.features[0].operation else {
        unreachable!()
    };
    ((**sketch).clone(), part.resolved_parameters().unwrap())
}

#[test]
fn ellipse_circle_contact_solves_free_axes_radius_and_rotated_placement() {
    let (s, mut params) = ellipse_circle_contact_sketch();
    for (a, b, r, tilt) in [
        (5., 3., 2., 0.),
        (8., 4., 3., 0.7),
        (12., 2., 1., std::f64::consts::FRAC_PI_3),
    ] {
        for (id, value) in [("semi_major", a), ("semi_minor", b), ("boss_radius", r)] {
            params.insert(
                id.into(),
                ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
            );
        }
        params.insert(
            "tilt".into(),
            ParameterValue::Scalar(Quantity::scalar(tilt)),
        );
        let solved = s.solve(&params).unwrap();
        assert!(solved.solved, "{}", solved.max_residual);
        assert_eq!(solved.free_degrees, 0);
        let contact = s.ellipse_circle_tangent_contacts(&params, &solved).unwrap()[&6];
        assert!(contact.tangent_sine.abs() < 1e-9);
        assert!(contact.arc_span_residual_mm.is_none());
        for q in &contact.points[1..] {
            assert!(
                (q.x - contact.points[0].x).abs() < 1e-9
                    && (q.y - contact.points[0].y).abs() < 1e-9
            );
        }
        let check = &s.constraint_checks(&params, &solved).unwrap()[6];
        assert!(check.satisfied && !check.by_construction);
        if tilt == 0. {
            assert!((solved.points["boss-center"].y - b - r).abs() < 1e-7);
        }
    }
}

#[test]
fn ellipse_circle_contact_handles_internal_external_arcs_and_non_tangent_crossings() {
    let mut s = base();
    s.points = vec![
        point("c", 0., 0., true),
        point("a", 5., 0., true),
        point("b", 0., 3., true),
        point("cc", 0., 5., true),
        point("r", 2., 5., true),
        point("p", 0., 3., true),
        point("left", -2., 5., true),
    ];
    s.ellipses = vec![SketchEllipse {
        id: "ellipse".into(),
        center: "c".into(),
        major: "a".into(),
        minor: "b".into(),
    }];
    s.circles = vec![SketchCircle {
        id: "circle".into(),
        center: "cc".into(),
        rim: "r".into(),
    }];
    s.constraints = vec![SketchConstraint::EllipseCircleTangent {
        ellipse: "ellipse".into(),
        circle: "circle".into(),
        point: "p".into(),
    }];
    let params = HashMap::new();
    assert!(s.solve(&params).unwrap().solved);
    s.points[3] = point("cc", 0., 2., true);
    s.points[4] = point("r", 1., 2., true);
    assert!(s.solve(&params).unwrap().solved); // Internal tangency uses the opposite normal branch.
    s.points[3] = point("cc", 1., 3., true);
    s.points[4] = point("r", 2., 3., true);
    let crossing = s.solve(&params).unwrap();
    assert!(!crossing.solved);
    let c = s
        .ellipse_circle_tangent_contacts(&params, &crossing)
        .unwrap()[&0];
    assert!(
        (c.points[0].y - c.points[1].y).abs() < 1e-9
            && (c.points[0].x - c.points[2].x).abs() < 1e-9
    );
    assert!(c.tangent_sine.abs() > 0.9); // Coincident contacts alone do not prove tangency.
    s.points[3] = point("cc", 0., 5., true);
    s.points[4] = point("r", 2., 5., true);
    s.circles.clear();
    s.arcs = vec![SketchArc {
        id: "circle".into(),
        center: "cc".into(),
        start: "r".into(),
        end: "left".into(),
        clockwise: true,
    }];
    for clockwise in [true, false] {
        s.arcs[0].clockwise = clockwise;
        let solved = s.solve(&params).unwrap();
        assert_eq!(solved.solved, clockwise);
        let c = s.ellipse_circle_tangent_contacts(&params, &solved).unwrap()[&0];
        assert!(c.tangent_sine.abs() < 1e-9);
        assert!(
            (c.arc_span_residual_mm.unwrap() - if clockwise { 0. } else { std::f64::consts::PI })
                .abs()
                < 1e-7
        );
    }
    s.arcs[0].start = "p".into();
    s.arcs[0].end = "r".into();
    s.arcs[0].clockwise = false;
    assert!(s.solve(&params).unwrap().solved); // Contact exactly on an arc endpoint.
}

#[test]
fn ellipse_circle_contact_validates_roles_degenerate_geometry_and_diagnostics() {
    let (s, params) = ellipse_circle_contact_sketch();
    for role in 0..3 {
        let mut bad = s.clone();
        let SketchConstraint::EllipseCircleTangent {
            ellipse,
            circle,
            point,
        } = &mut bad.constraints[6]
        else {
            unreachable!()
        };
        match role {
            0 => *ellipse = "boss".into(),
            1 => *circle = "baseline".into(),
            _ => *point = "missing".into(),
        };
        assert!(bad.solve(&params).is_err());
    }
    let mut fixed = s.clone();
    for p in &mut fixed.points {
        p.fixed = true;
    }
    let solved = fixed.solve(&params).unwrap();
    assert!(!solved.solved);
    assert!(!fixed.constraint_checks(&params, &solved).unwrap()[6].satisfied);
    let mut missing = solved.clone();
    missing.points.remove("contact");
    assert!(
        fixed
            .ellipse_circle_tangent_contacts(&params, &missing)
            .is_err()
    );
    let mut invalid = solved;
    invalid.points.get_mut("contact").unwrap().x = f64::NAN;
    assert!(
        fixed
            .ellipse_circle_tangent_contacts(&params, &invalid)
            .is_err()
    );
    for id in ["major", "boss-rim", "contact"] {
        let mut bad = fixed.clone();
        let q = if id == "major" { (0., 0.) } else { (0., 5.) };
        let p = bad.points.iter_mut().find(|p| p.id == id).unwrap();
        p.x = length(q.0);
        p.y = length(q.1);
        assert!(bad.solve(&params).is_err());
    }
}
