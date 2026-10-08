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
