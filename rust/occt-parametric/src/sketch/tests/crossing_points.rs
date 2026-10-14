use super::*;
fn line(id: &str, start: &str, end: &str) -> SketchLine {
    SketchLine {
        id: id.into(),
        start: start.into(),
        end: end.into(),
    }
}
fn selected(
    point_id: &str,
    first: &str,
    second: &str,
    x: f64,
    y: f64,
    radius: f64,
) -> SketchConstraint {
    SketchConstraint::CrossingPoint {
        point: point_id.into(),
        first: first.into(),
        second: second.into(),
        near: VectorExpr::Literal(VectorQuantity::lengths(x, y, 0., LengthUnit::Millimeter)),
        maximum_distance: length(radius),
    }
}
fn crossing() -> SketchDefinition {
    let mut s = rectangle();
    s.points = vec![
        point("center", 0., 0., true),
        point("rim", 10., 0., true),
        point("a", -12., 5., true),
        point("b", 12., 5., true),
        point("hit", -8., 5., false),
    ];
    s.lines = vec![line("guide", "a", "b")];
    s.circles = vec![SketchCircle {
        id: "circle".into(),
        center: "center".into(),
        rim: "rim".into(),
    }];
    s.profile = vec!["circle".into()];
    s.constraints = vec![selected("hit", "circle", "guide", 8.5, 5., 3.)];
    s
}
#[test]
fn selected_crossings_follow_the_window_instead_of_a_solver_seed_and_reject_loss_ambiguity_and_overlap()
 {
    let mut s = crossing();
    let solution = s.solve(&HashMap::new()).unwrap();
    assert!(solution.solved);
    assert_eq!(solution.free_degrees, 0);
    assert!((solution.points["hit"].x - 75_f64.sqrt()).abs() < 1e-8);
    assert!((solution.points["hit"].y - 5.).abs() < 1e-8);
    for point in &mut s.points {
        if ["a", "b"].contains(&point.id.as_str()) {
            point.y = length(6.);
        }
    }
    let moved = s.solve(&HashMap::new()).unwrap();
    assert!(moved.solved);
    assert!((moved.points["hit"].x - 8.).abs() < 1e-8);
    let mut bad = s.clone();
    bad.constraints = vec![selected("hit", "circle", "guide", 0., 6., 20.)];
    assert!(
        bad.solve(&HashMap::new())
            .unwrap_err()
            .to_string()
            .contains("multiple crossings")
    );
    let mut bad = s.clone();
    bad.constraints = vec![selected("hit", "circle", "guide", 8.5, 5., 0.1)];
    assert!(
        bad.solve(&HashMap::new())
            .unwrap_err()
            .to_string()
            .contains("no crossing")
    );
    let mut bad = s.clone();
    bad.points[2].y = length(12.);
    bad.points[3].y = length(12.);
    assert!(
        bad.solve(&HashMap::new())
            .unwrap_err()
            .to_string()
            .contains("no crossing")
    );
    // Supporting-line intersections outside a finite segment do not count.
    let mut bad = s.clone();
    bad.points[2].x = length(-2.);
    bad.points[3].x = length(2.);
    assert!(
        bad.solve(&HashMap::new())
            .unwrap_err()
            .to_string()
            .contains("no crossing")
    );
    let mut bad = s.clone();
    bad.lines.push(line("overlap", "a", "b"));
    bad.constraints = vec![selected("hit", "guide", "overlap", 0., 6., 20.)];
    assert!(bad.solve(&HashMap::new()).is_err());
}
#[test]
fn selected_crossings_validate_target_independence_and_hint_units() {
    let s = crossing();
    let mut bad = s.clone();
    bad.points[4].fixed = true;
    assert!(bad.validate_structure().is_err());
    let mut bad = s.clone();
    bad.constraints
        .push(selected("hit", "circle", "guide", 8., 5., 3.));
    assert!(bad.validate_structure().is_err());
    let mut bad = s.clone();
    bad.constraints = vec![selected("a", "circle", "guide", 8., 5., 3.)];
    bad.points[2].fixed = false;
    assert!(
        bad.validate_structure()
            .unwrap_err()
            .to_string()
            .contains("cannot define")
    );
    let mut bad = s.clone();
    bad.points[3].fixed = false;
    bad.constraints.push(SketchConstraint::WeightedPoint {
        point: "b".into(),
        terms: vec![SketchWeightedPointTerm {
            point: "hit".into(),
            weight: ScalarExpr::Literal(Quantity::scalar(1.)),
        }],
    });
    assert!(
        bad.validate_structure()
            .unwrap_err()
            .to_string()
            .contains("cannot define")
    );
    let mut bad = s.clone();
    bad.constraints = vec![selected("hit", "circle", "circle", 8., 5., 3.)];
    assert!(bad.validate_structure().is_err());
    let mut bad = s.clone();
    if let SketchConstraint::CrossingPoint { near, .. } = &mut bad.constraints[0] {
        *near = VectorExpr::Literal(VectorQuantity::lengths(8., 5., 1., LengthUnit::Millimeter));
    }
    assert!(
        bad.solve(&HashMap::new())
            .unwrap_err()
            .to_string()
            .contains("z equal")
    );
    let mut bad = s.clone();
    bad.constraints = vec![selected("hit", "circle", "guide", 8., 5., 0.)];
    assert!(
        bad.solve(&HashMap::new())
            .unwrap_err()
            .to_string()
            .contains("positive")
    );
}

#[test]
fn selected_crossing_cache_refreshes_for_free_geometry_and_tangencies() {
    let mut s = crossing();
    s.points[2].fixed = false;
    s.points[3].fixed = false;
    s.constraints.extend([
        SketchConstraint::Horizontal {
            line: "guide".into(),
        },
        SketchConstraint::HorizontalDistance {
            first: "a".into(),
            second: "b".into(),
            value: length(24.),
        },
        SketchConstraint::HorizontalDistance {
            first: "center".into(),
            second: "a".into(),
            value: length(-12.),
        },
        SketchConstraint::VerticalDistance {
            first: "center".into(),
            second: "a".into(),
            value: length(6.),
        },
    ]);
    let solved = s.solve(&HashMap::new()).unwrap();
    assert!(solved.solved, "{solved:?}");
    assert_eq!(solved.free_degrees, 0);
    assert!((solved.points["hit"].x - 8.).abs() < 1e-8);
    assert!((solved.points["hit"].y - 6.).abs() < 1e-8);
    let mut tangent = crossing();
    tangent.points[2].y = length(10.);
    tangent.points[3].y = length(10.);
    tangent.constraints = vec![selected("hit", "circle", "guide", 0., 10., 2.)];
    let solved = tangent.solve(&HashMap::new()).unwrap();
    assert!(solved.solved);
    assert!(solved.points["hit"].x.abs() < 1e-8);
    assert!((solved.points["hit"].y - 10.).abs() < 1e-8);
}
#[test]
fn selected_crossings_support_exact_ellipse_arcs_and_spline_basis_modes() {
    for mode in 0..5 {
        let mut s = crossing();
        s.circles.clear();
        s.profile.clear();
        s.points[2].y = length(0.);
        s.points[3].y = length(0.);
        match mode {
            0 | 1 => {
                s.points.push(point("minor", 0., 8., true));
                s.ellipses = vec![SketchEllipse {
                    id: "source".into(),
                    center: "center".into(),
                    major: "rim".into(),
                    minor: "minor".into(),
                    start: None,
                    end: None,
                    clockwise: false,
                }];
                if mode == 1 {
                    s.points.push(point("arc-end", 0., -8., true));
                    s.ellipses[0].start = Some("minor".into());
                    s.ellipses[0].end = Some("arc-end".into());
                    s.ellipses[0].clockwise = true;
                }
            }
            _ => {
                s.points.push(point("top", 0., 10., true));
                s.points.push(point("bottom", 0., -10., true));
                s.splines = vec![SketchSpline {
                    id: "source".into(),
                    points: vec!["rim".into(), "top".into(), "center".into(), "bottom".into()],
                    basis: None,
                    endpoints: None,
                }];
                if mode == 3 {
                    s.splines[0].basis = Some(SketchBSplineBasis {
                        degree: 3,
                        periodic: true,
                        non_clamped: false,
                        knots: vec![0., 1., 2., 3., 4.],
                        multiplicities: vec![1; 5],
                        weights: vec![],
                    });
                }
                if mode == 4 {
                    s.points[0].x = length(8.);
                    s.points.iter_mut().find(|p| p.id == "top").unwrap().x = length(8.);
                    s.points.iter_mut().find(|p| p.id == "bottom").unwrap().x = length(10.);
                    s.splines[0].basis = Some(SketchBSplineBasis {
                        degree: 2,
                        periodic: false,
                        non_clamped: true,
                        knots: vec![0., 1., 2., 3., 4., 5., 6.],
                        multiplicities: vec![1; 7],
                        weights: vec![],
                    });
                    s.points.push(point("start", 5., 5., false));
                    s.points.push(point("end", 0., -5., false));
                    s.splines[0].endpoints = Some(SketchSplineEndpoints {
                        start: "start".into(),
                        end: "end".into(),
                    });
                }
            }
        }
        s.constraints = vec![selected("hit", "source", "guide", 9., 0., 5.)];
        let solved = s
            .solve(&HashMap::new())
            .unwrap_or_else(|e| panic!("mode {mode}: {e}"));
        assert!(solved.solved, "mode {mode}: {solved:?}");
        assert!((solved.points["hit"].y).abs() < 1e-8);
        assert!(solved.points["hit"].x > 0.);
    }
}
#[test]
fn one_thousand_selected_crossings_share_numeric_native_candidates_within_budget() {
    let mut s = crossing();
    s.points.pop();
    s.constraints.clear();
    for i in 0..1000 {
        let id = format!("hit-{i}");
        s.points.push(point(&id, -8., 5., false));
        s.constraints
            .push(selected(&id, "circle", "guide", 8.5, 5., 3.));
    }
    let start = std::time::Instant::now();
    let solved = s.solve(&HashMap::new()).unwrap();
    assert!(solved.solved);
    assert_eq!(solved.free_degrees, 0);
    assert!((solved.points["hit-999"].x - 75_f64.sqrt()).abs() < 1e-8);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn selected_crossings_respect_circular_arc_direction_and_span() {
    let mut s = crossing();
    s.circles.clear();
    s.profile.clear();
    s.points[2].y = length(0.);
    s.points[3].y = length(0.);
    s.points
        .extend([point("top", 0., 10., true), point("bottom", 0., -10., true)]);
    s.arcs = vec![SketchArc {
        id: "source".into(),
        center: "center".into(),
        start: "top".into(),
        end: "bottom".into(),
        clockwise: true,
    }];
    s.constraints = vec![selected("hit", "source", "guide", 9., 0., 3.)];
    let right = s.solve(&HashMap::new()).unwrap();
    assert!(right.solved);
    assert!((right.points["hit"].x - 10.).abs() < 1e-8);
    s.arcs[0].clockwise = false;
    assert!(
        s.solve(&HashMap::new())
            .unwrap_err()
            .to_string()
            .contains("no crossing")
    );
    s.constraints = vec![selected("hit", "source", "guide", -9., 0., 3.)];
    let left = s.solve(&HashMap::new()).unwrap();
    assert!(left.solved);
    assert!((left.points["hit"].x + 10.).abs() < 1e-8);
}

#[test]
fn selected_crossings_reject_excessive_pairs_and_candidates_without_partial_selection() {
    let mut s = crossing();
    s.points.pop();
    s.constraints.clear();
    s.circles = (0..257)
        .map(|i| SketchCircle {
            id: format!("circle-{i}"),
            center: "center".into(),
            rim: "rim".into(),
        })
        .collect();
    s.profile = vec!["circle-0".into()];
    for i in 0..257 {
        let id = format!("hit-{i}");
        s.points.push(point(&id, 0., 0., false));
        s.constraints
            .push(selected(&id, &format!("circle-{i}"), "guide", 8.5, 5., 3.));
    }
    assert!(
        s.solve(&HashMap::new())
            .unwrap_err()
            .to_string()
            .contains("256 distinct")
    );
    let mut s = crossing();
    s.circles.clear();
    s.profile.clear();
    s.points = (0..1003)
        .map(|i| {
            point(
                &format!("pole-{i}"),
                i as f64,
                if i % 2 == 0 { -1. } else { 1. },
                true,
            )
        })
        .collect();
    s.points.extend([
        point("a", -1., 0., true),
        point("b", 1004., 0., true),
        point("hit", 0., 0., false),
    ]);
    s.splines = vec![SketchSpline {
        id: "zigzag".into(),
        points: (0..1003).map(|i| format!("pole-{i}")).collect(),
        basis: Some(SketchBSplineBasis {
            degree: 1,
            periodic: false,
            non_clamped: false,
            knots: (0..1003).map(f64::from).collect(),
            multiplicities: (0..1003)
                .map(|i| if i == 0 || i == 1002 { 2 } else { 1 })
                .collect(),
            weights: vec![],
        }),
        endpoints: None,
    }];
    s.constraints = vec![selected("hit", "zigzag", "guide", 0.5, 0., 0.1)];
    assert!(s.solve(&HashMap::new()).is_err());
}

#[test]
fn finite_crossing_queries_share_current_native_curve_semantics_without_editing_the_sketch() {
    let mut s = crossing();
    let before = serde_json::to_value(&s).unwrap();
    let params = HashMap::new();
    let mut points = s
        .finite_curve_intersections(&params, "circle", "guide")
        .unwrap();
    points.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(points.len(), 2);
    assert!((points[0].x + 75_f64.sqrt()).abs() < 1e-8);
    assert!((points[1].x - 75_f64.sqrt()).abs() < 1e-8);
    assert_eq!(serde_json::to_value(&s).unwrap(), before);
    for (a, b) in [("circle", "circle"), ("missing", "guide")] {
        assert!(s.finite_curve_intersections(&params, a, b).is_err());
    }
    s.constraints.clear();
    s.points.pop();
    s.points[2].x = length(-1.);
    s.points[3].x = length(1.);
    assert!(
        s.finite_curve_intersections(&params, "circle", "guide")
            .unwrap()
            .is_empty()
    );
    s.lines.push(line("duplicate", "a", "b"));
    assert!(
        s.finite_curve_intersections(&params, "guide", "duplicate")
            .is_err()
    );
}

#[test]
fn finite_crossing_queries_handle_interpolation_and_explicit_periodic_splines() {
    for mode in 0..3 {
        let mut s = crossing();
        s.constraints.clear();
        s.points.pop();
        s.circles.clear();
        s.profile.clear();
        s.points.extend([
            point("left", -10., 0., true),
            point("top", 0., 10., true),
            point("bottom", 0., -10., true),
        ]);
        let mut curve = SketchSpline {
            id: "source".into(),
            points: vec!["left".into(), "top".into(), "rim".into()],
            basis: None,
            endpoints: None,
        };
        if mode == 1 {
            curve.points.push("left".into());
        }
        if mode == 2 {
            curve.points.push("bottom".into());
            curve.basis = Some(SketchBSplineBasis {
                degree: 3,
                periodic: true,
                non_clamped: false,
                knots: vec![0., 1., 2., 3., 4.],
                multiplicities: vec![1; 5],
                weights: vec![],
            });
        }
        s.splines.push(curve);
        let points = s
            .finite_curve_intersections(&HashMap::new(), "source", "guide")
            .unwrap();
        assert_eq!(points.len(), 2, "mode {mode}");
        for p in points {
            assert!((p.y - 5.).abs() < 1e-7);
        }
    }
}
