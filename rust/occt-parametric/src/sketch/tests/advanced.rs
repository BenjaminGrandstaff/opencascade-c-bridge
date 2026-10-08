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
