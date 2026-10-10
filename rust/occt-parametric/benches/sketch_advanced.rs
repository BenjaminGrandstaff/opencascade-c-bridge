//! Scale gates for schema-72 sketch equations and native profile edits.
//! Independent components keep sparse storage/fill local. A point-on-spline
//! touches its defining points; interpolation/projection cost scales with that
//! curve rather than all sketch points. Native handles are released each loop.
use occt_bridge::Session;
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn mm(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn scalar(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::scalar(v))
}
fn point(id: String, x: f64, y: f64, fixed: bool) -> SketchPoint {
    SketchPoint {
        id,
        x: mm(x),
        y: mm(y),
        fixed,
    }
}
fn sketch() -> SketchDefinition {
    SketchDefinition {
        id: "scale".into(),
        datum_plane: None,
        face_support: None,
        projections: Vec::new(),
        origin: VectorExpr::Literal(VectorQuantity::lengths(0., 0., 0., LengthUnit::Millimeter)),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1., 0., 0.)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0., 1., 0.)),
        points: vec![],
        lines: vec![],
        circles: vec![],
        ellipses: vec![],
        arcs: vec![],
        splines: vec![],
        profile: vec![],
        constraints: vec![],
        profile_operations: vec![],
    }
}
fn gate(name: &str, budget: u64, run: impl FnOnce()) {
    let start = Instant::now();
    run();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(budget),
        "{name} exceeded its {budget}s budget: {elapsed:?}"
    );
    println!("PASS {name}: {:.3}s / {budget}s", elapsed.as_secs_f64());
}
fn main() {
    gate(
        "1000 native periodic B-spline pads with rational weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/periodic-spline-pad.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "corner_weight".into(),
                    ParameterValue::Scalar(Quantity::scalar(if i % 2 == 0 { 1. } else { 2. })),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                if i % 2 == 0 {
                    assert!(
                        (session.volume(built.shape("body").unwrap()).unwrap()
                            - 122. / 45. * 10. * 8. * 8.)
                            .abs()
                            < 1e-5
                    );
                }
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 explicit rational B-spline profiles with native weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/rational-spline-cap.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                let weight = if i % 2 == 0 { 0.5_f64.sqrt() } else { 1. };
                part.overrides.insert(
                    "middle_weight".into(),
                    ParameterValue::Scalar(Quantity::scalar(weight)),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                let area = if i % 2 == 0 {
                    (std::f64::consts::PI / 4. - 0.5) * 100.
                } else {
                    100. / 3.
                };
                assert!(
                    (session.volume(built.shape("body").unwrap()).unwrap() - area * 8.).abs()
                        < 1e-5
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 arc-span-aware line and circular tangent components",
        10,
        || {
            let mut s = sketch();
            for i in 0..1000 {
                let id = |name: &str| format!("{name}-{i}");
                let x = i as f64 * 30.;
                s.points.extend([
                    point(id("c"), x, 0., true),
                    point(id("start"), x, 4., true),
                    point(id("end"), x, -4., true),
                    point(id("a"), x - 10., -4., true),
                    point(id("b"), x + 10., -4., true),
                    point(id("c2"), x + 6., -1., false),
                    point(id("start2"), x + 6., 1., false),
                    point(id("end2"), x + 8., -1., false),
                ]);
                s.lines.extend([
                    SketchLine {
                        id: id("line"),
                        start: id("a"),
                        end: id("b"),
                    },
                    SketchLine {
                        id: id("end-axis"),
                        start: id("c2"),
                        end: id("end2"),
                    },
                    SketchLine {
                        id: id("start-axis"),
                        start: id("c2"),
                        end: id("start2"),
                    },
                ]);
                s.arcs.extend([
                    SketchArc {
                        id: id("first"),
                        center: id("c"),
                        start: id("start"),
                        end: id("end"),
                        clockwise: true,
                    },
                    SketchArc {
                        id: id("second"),
                        center: id("c2"),
                        start: id("start2"),
                        end: id("end2"),
                        clockwise: false,
                    },
                ]);
                s.constraints.extend([
                    SketchConstraint::LineCircleTangent {
                        line: id("line"),
                        circle: id("second"),
                        side: SketchLineSide::Left,
                    },
                    SketchConstraint::CircleCircleTangent {
                        first: id("first"),
                        second: id("second"),
                        mode: SketchCircleTangency::External,
                    },
                    SketchConstraint::Radius {
                        curve: id("second"),
                        value: mm(2.),
                    },
                    SketchConstraint::Horizontal {
                        line: id("end-axis"),
                    },
                    SketchConstraint::Vertical {
                        line: id("start-axis"),
                    },
                ]);
            }
            let params = HashMap::new();
            let solved = s.solve(&params).unwrap();
            assert!(solved.solved);
            assert_eq!(solved.free_degrees, 0);
            assert!(
                s.constraint_checks(&params, &solved)
                    .unwrap()
                    .iter()
                    .all(|c| c.satisfied)
            );
        },
    );
    gate(
        "1000 independent line-circle and circle-circle tangent components",
        10,
        || {
            let mut s = sketch();
            for i in 0..1000 {
                let id = |name: &str| format!("{name}-{i}");
                let x = i as f64 * 20.;
                s.points.extend([
                    point(id("c"), x, 0., true),
                    point(id("r"), x + 4., 0., true),
                    point(id("a"), x - 10., -4., true),
                    point(id("b"), x + 10., -4., true),
                    point(id("c2"), x + 6., -1., false),
                    point(id("r2"), x + 8., -1., false),
                ]);
                s.lines.extend([
                    SketchLine {
                        id: id("line"),
                        start: id("a"),
                        end: id("b"),
                    },
                    SketchLine {
                        id: id("radius"),
                        start: id("c2"),
                        end: id("r2"),
                    },
                ]);
                s.circles.extend([
                    SketchCircle {
                        id: id("first"),
                        center: id("c"),
                        rim: id("r"),
                    },
                    SketchCircle {
                        id: id("second"),
                        center: id("c2"),
                        rim: id("r2"),
                    },
                ]);
                s.constraints.extend([
                    SketchConstraint::LineCircleTangent {
                        line: id("line"),
                        circle: id("second"),
                        side: SketchLineSide::Left,
                    },
                    SketchConstraint::CircleCircleTangent {
                        first: id("first"),
                        second: id("second"),
                        mode: SketchCircleTangency::External,
                    },
                    SketchConstraint::Radius {
                        curve: id("second"),
                        value: mm(2.),
                    },
                    SketchConstraint::Horizontal { line: id("radius") },
                ]);
            }
            let params = HashMap::new();
            let solved = s.solve(&params).unwrap();
            assert!(solved.solved);
            assert_eq!(solved.free_degrees, 0);
            assert!(
                s.constraint_checks(&params, &solved)
                    .unwrap()
                    .iter()
                    .all(|c| c.satisfied)
            );
        },
    );
    gate(
        "1000 signed point-line dimensions on tilted references",
        10,
        || {
            let mut s = sketch();
            for i in 0..1000 {
                let id = |name: &str| format!("{name}-{i}");
                let x = i as f64 * 20.;
                s.points.extend([
                    point(id("a"), x, 0., true),
                    point(id("b"), x + 3., 4., true),
                    point(id("p"), x + 2., 0., false),
                    point(id("axis"), x + 2., -20., true),
                ]);
                s.lines.extend([
                    SketchLine {
                        id: id("line"),
                        start: id("a"),
                        end: id("b"),
                    },
                    SketchLine {
                        id: id("vertical"),
                        start: id("axis"),
                        end: id("p"),
                    },
                ]);
                s.constraints.extend([
                    SketchConstraint::Vertical {
                        line: id("vertical"),
                    },
                    SketchConstraint::PointLineDistance {
                        point: id("p"),
                        line: id("line"),
                        value: mm(if i % 2 == 0 { 3. } else { -3. }),
                    },
                ]);
            }
            let params = HashMap::new();
            let solved = s.solve(&params).unwrap();
            assert!(solved.solved);
            assert_eq!(solved.free_degrees, 0);
            assert!(
                s.constraint_checks(&params, &solved)
                    .unwrap()
                    .iter()
                    .all(|c| c.satisfied)
            );
        },
    );
    gate(
        "1000 midpoint and concentric components with sparse equations",
        10,
        || {
            let mut s = sketch();
            for i in 0..1000 {
                let id = |name: &str| format!("{name}-{i}");
                let x = i as f64 * 20.;
                s.points.extend([
                    point(id("a"), x, 2., true),
                    point(id("b"), x + 8., 6., true),
                    point(id("mid"), x + 1., 1., false),
                    point(id("c"), x + 2., 3., false),
                    point(id("r"), x + 10., 3., true),
                    point(id("fixed-rim"), x + 10., 4., true),
                ]);
                s.lines.push(SketchLine {
                    id: id("line"),
                    start: id("a"),
                    end: id("b"),
                });
                s.circles.extend([
                    SketchCircle {
                        id: id("first"),
                        center: id("mid"),
                        rim: id("fixed-rim"),
                    },
                    SketchCircle {
                        id: id("second"),
                        center: id("c"),
                        rim: id("r"),
                    },
                ]);
                s.constraints.extend([
                    SketchConstraint::Midpoint {
                        point: id("mid"),
                        line: id("line"),
                    },
                    SketchConstraint::Concentric {
                        first: id("first"),
                        second: id("second"),
                    },
                ]);
            }
            let params = HashMap::new();
            let solved = s.solve(&params).unwrap();
            assert!(solved.solved);
            assert_eq!(solved.free_degrees, 0);
            assert!(
                s.constraint_checks(&params, &solved)
                    .unwrap()
                    .iter()
                    .all(|c| c.satisfied)
            );
        },
    );
    gate(
        "1000 linked edge-projection sketches, runtime snapshots and source-depth edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-pocket.request.json"
            ))
            .unwrap();
            let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let family = &mut document.family;
            family.requirements.clear();
            let profile = family
                .features
                .iter()
                .find(|f| f.id == "profile")
                .unwrap()
                .clone();
            family.features.retain(|f| f.id == "block");
            for i in 0..1000 {
                let mut f = profile.clone();
                f.id = format!("profile-{i}");
                family.features.push(f);
            }
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: family,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session).unwrap();
            let count = session.shape_count().unwrap();
            let snapshots = part.resolved_sketches(&session, &first).unwrap();
            assert_eq!(snapshots.len(), 1000);
            for snapshot in snapshots.values() {
                let sketch = &snapshot.as_ref().unwrap().sketch;
                assert!(sketch.projections.is_empty());
                assert_eq!(sketch.points.len(), 10);
            }
            assert_eq!(session.shape_count().unwrap(), count);
            drop(snapshots);
            part.overrides.insert(
                "depth".into(),
                ParameterValue::Scalar(Quantity::length(50.0, LengthUnit::Millimeter)),
            );
            let edited = part.regenerate_incremental(&session, &first).unwrap();
            assert_eq!(edited.regeneration.rebuilt.len(), 1001);
            for i in 0..1000 {
                let bounds = session
                    .exact_bounds(edited.shape(&format!("profile-{i}")).unwrap())
                    .unwrap();
                assert!((bounds.min.y - 34.0).abs() < 1e-7 && (bounds.max.y - 46.0).abs() < 1e-7);
            }
            drop((first, edited));
            assert_eq!(session.shape_count().unwrap(), 0);
        },
    );
    gate(
        "1000 face-attached sketch profiles, plane queries and height edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/face-pocket.request.json"
            ))
            .unwrap();
            let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let family = &mut document.family;
            family.requirements.clear();
            let profile = family
                .features
                .iter()
                .find(|f| f.id == "profile")
                .unwrap()
                .clone();
            family.features.retain(|f| f.id == "block");
            for i in 0..1000 {
                let mut f = profile.clone();
                f.id = format!("profile-{i}");
                family.features.push(f);
            }
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: family,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session).unwrap();
            let count = session.shape_count().unwrap();
            let planes = part.sketch_support_planes(&session, &first).unwrap();
            assert_eq!(planes.len(), 1000);
            for plane in planes.values() {
                let ResolvedDatum::Plane { origin, .. } = plane.as_ref().unwrap() else {
                    panic!()
                };
                assert!((origin.z - 20.0).abs() < 1e-7);
            }
            assert_eq!(session.shape_count().unwrap(), count);
            part.overrides.insert(
                "height".into(),
                ParameterValue::Scalar(Quantity::length(30.0, LengthUnit::Millimeter)),
            );
            let edited = part.regenerate_incremental(&session, &first).unwrap();
            assert_eq!(edited.regeneration.rebuilt.len(), 1001);
            for i in 0..1000 {
                assert!(
                    (session
                        .exact_bounds(edited.shape(&format!("profile-{i}")).unwrap())
                        .unwrap()
                        .min
                        .z
                        - 30.0)
                        .abs()
                        < 1e-7
                );
            }
            drop((first, edited));
            assert_eq!(session.shape_count().unwrap(), 0);
        },
    );
    gate(
        "1000 equal-radius circle pairs with residual diagnostics",
        10,
        || {
            let mut s = sketch();
            for i in 0..1000 {
                let x = i as f64 * 30.0;
                let id = |p: &str| format!("{p}-{i}");
                for (name, dx, fixed) in [
                    ("a", 0.0, true),
                    ("ar", 3.0, true),
                    ("b", 10.0, true),
                    ("br", 14.0, false),
                ] {
                    s.points.push(point(id(name), x + dx, 0.0, fixed));
                }
                for (name, c, r) in [("first", "a", "ar"), ("second", "b", "br")] {
                    s.circles.push(SketchCircle {
                        id: id(name),
                        center: id(c),
                        rim: id(r),
                    });
                }
                s.lines.push(SketchLine {
                    id: id("axis"),
                    start: id("b"),
                    end: id("br"),
                });
                s.constraints
                    .push(SketchConstraint::Horizontal { line: id("axis") });
                s.constraints.push(SketchConstraint::EqualRadius {
                    first: id("first"),
                    second: id("second"),
                });
            }
            s.profile = vec!["first-0".into()];
            let parameters = HashMap::new();
            let solution = s.solve(&parameters).unwrap();
            assert!(solution.solved);
            assert_eq!(solution.free_degrees, 0);
            for i in 0..1000 {
                assert!(
                    (solution.points[&format!("br-{i}")].x - (i as f64 * 30.0 + 13.0)).abs() < 1e-7
                );
            }
            let checks = s.constraint_checks(&parameters, &solution).unwrap();
            assert_eq!(checks.len(), 2000);
            assert!(checks.iter().all(|c| c.satisfied && !c.by_construction));
        },
    );
    gate("1000 coupled advanced sketch components", 10, || {
        let mut s = sketch();
        for i in 0..1000 {
            let x = i as f64 * 30.;
            let id = |p: &str| format!("{p}{i}");
            for (p, dx, y, fixed) in [
                ("c", 0., 0., true),
                ("a", 8., 0., true),
                ("b", 0., 3., false),
                ("p", 4., 4., false),
                ("r", 3., 0., false),
                ("m", -5., 1., false),
                ("t", 5., 3., false),
            ] {
                s.points.push(point(id(p), x + dx, y, fixed));
            }
            for (l, a, b) in [
                ("x", "c", "a"),
                ("axis", "c", "b"),
                ("radial", "c", "r"),
                ("ray", "c", "t"),
            ] {
                s.lines.push(SketchLine {
                    id: id(l),
                    start: id(a),
                    end: id(b),
                });
            }
            s.circles.push(SketchCircle {
                id: id("circle"),
                center: id("c"),
                rim: id("r"),
            });
            s.ellipses.push(SketchEllipse {
                id: id("ellipse"),
                center: id("c"),
                major: id("a"),
                minor: id("b"),
            });
            s.constraints.extend([
                SketchConstraint::Distance {
                    first: id("c"),
                    second: id("b"),
                    value: mm(4.),
                },
                SketchConstraint::PointOnCurve {
                    point: id("p"),
                    curve: id("ellipse"),
                },
                SketchConstraint::Radius {
                    curve: id("circle"),
                    value: mm(6.),
                },
                SketchConstraint::Diameter {
                    curve: id("circle"),
                    value: mm(12.),
                },
                SketchConstraint::Horizontal { line: id("radial") },
                SketchConstraint::Symmetric {
                    first: id("a"),
                    second: id("m"),
                    axis: id("axis"),
                },
                SketchConstraint::Angle {
                    first: id("x"),
                    second: id("ray"),
                    value: scalar(std::f64::consts::FRAC_PI_3),
                },
                SketchConstraint::Distance {
                    first: id("c"),
                    second: id("t"),
                    value: mm(10.),
                },
            ]);
        }
        let result = s.solve(&HashMap::new()).unwrap();
        assert!(result.solved, "residual {}", result.max_residual);
        assert_eq!(result.free_degrees, 1000);
        for i in [0, 499, 999] {
            let x = i as f64 * 30.;
            assert!((result.points[&format!("m{i}")].x - (x - 8.)).abs() < 1e-7);
            assert!((result.points[&format!("r{i}")].x - (x + 6.)).abs() < 1e-7);
        }
    });
    gate("1000 native point-on-spline components", 10, || {
        let mut s = sketch();
        for i in 0..1000 {
            let x = i as f64 * 20.;
            let id = |p: &str| format!("{p}{i}");
            for (p, dx, y, fixed) in [
                ("a", 0., 0., true),
                ("b", 5., 3., true),
                ("c", 10., 0., true),
                ("p", 3., 4., false),
            ] {
                s.points.push(point(id(p), x + dx, y, fixed));
            }
            s.splines.push(SketchSpline {
                basis: None,
                id: id("s"),
                points: vec![id("a"), id("b"), id("c")],
            });
            s.constraints.push(SketchConstraint::PointOnCurve {
                point: id("p"),
                curve: id("s"),
            });
        }
        let result = s.solve(&HashMap::new()).unwrap();
        assert!(result.solved, "spline residual {}", result.max_residual);
        assert_eq!(result.free_degrees, 1000);
    });
    gate(
        "1000 native trim-extend-offset profile regenerations",
        10,
        || {
            let mut s = sketch();
            s.points = vec![
                point("c".into(), 0., 0., true),
                point("r".into(), 4., 0., true),
            ];
            s.circles.push(SketchCircle {
                id: "circle".into(),
                center: "c".into(),
                rim: "r".into(),
            });
            s.profile_operations = vec![
                SketchProfileOperation::Trim {
                    entity: "circle".into(),
                    first: scalar(0.),
                    last: scalar(0.25),
                },
                SketchProfileOperation::Extend {
                    entity: "circle".into(),
                    start: mm(0.),
                    end: mm(2. * std::f64::consts::PI),
                },
                SketchProfileOperation::Offset {
                    distance: mm(1.),
                    join: SketchOffsetJoin::Arc,
                },
            ];
            let mut family:FamilyDefinition=serde_json::from_value(serde_json::json!({"id":"advanced-profiles","version":1,"parameters":[],"features":[],"requirements":[]})).unwrap();
            family.features.push(FeatureDefinition {
                id: "profile".into(),
                operation: FeatureOperation::SketchOpenWire {
                    sketch: Box::new(s),
                },
            });
            let part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let session = Session::new().unwrap();
            for _ in 0..1000 {
                let generated = part.regenerate(&session).unwrap();
                let bounds = session
                    .exact_bounds(generated.shape("profile").unwrap())
                    .unwrap();
                assert!(
                    (bounds.min.x + 5.).abs() < 1e-6
                        && (bounds.max.x - 5.).abs() < 1e-6
                        && (bounds.max.y - 5.).abs() < 1e-6,
                    "offset half-circle bounds {bounds:?}"
                );
                drop(generated);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
}
