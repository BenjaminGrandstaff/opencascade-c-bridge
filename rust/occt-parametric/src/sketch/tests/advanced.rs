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
