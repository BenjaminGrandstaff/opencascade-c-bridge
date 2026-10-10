use super::*;

mod advanced;
mod profiles;
mod solving;
mod splines;
mod sweeps;

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

fn point(id: &str, x: f64, y: f64, fixed: bool) -> SketchPoint {
    SketchPoint {
        id: id.into(),
        x: length(x),
        y: length(y),
        fixed,
    }
}

fn rectangle() -> SketchDefinition {
    SketchDefinition {
        id: "rectangle".into(),
        datum_plane: None,
        face_support: None,
        circles: Vec::new(),
        arcs: Vec::new(),
        ellipses: Vec::new(),
        profile_operations: Vec::new(),
        splines: Vec::new(),
        profile: Vec::new(),
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        points: vec![
            point("p0", 0.0, 0.0, true),
            point("p1", 9.0, 1.0, false),
            point("p2", 9.0, 8.0, false),
            point("p3", 1.0, 8.0, false),
            point("anchor", 1.0, 1.0, false),
        ],
        lines: vec![
            SketchLine {
                id: "bottom".into(),
                start: "p0".into(),
                end: "p1".into(),
            },
            SketchLine {
                id: "right".into(),
                start: "p1".into(),
                end: "p2".into(),
            },
            SketchLine {
                id: "top".into(),
                start: "p2".into(),
                end: "p3".into(),
            },
            SketchLine {
                id: "left".into(),
                start: "p3".into(),
                end: "p0".into(),
            },
        ],
        constraints: vec![
            SketchConstraint::Coincident {
                first: "anchor".into(),
                second: "p0".into(),
            },
            SketchConstraint::Horizontal {
                line: "bottom".into(),
            },
            SketchConstraint::Vertical {
                line: "right".into(),
            },
            SketchConstraint::Horizontal { line: "top".into() },
            SketchConstraint::Vertical {
                line: "left".into(),
            },
            SketchConstraint::Parallel {
                first: "bottom".into(),
                second: "top".into(),
            },
            SketchConstraint::Perpendicular {
                first: "bottom".into(),
                second: "right".into(),
            },
            SketchConstraint::EqualLength {
                first: "bottom".into(),
                second: "top".into(),
            },
            SketchConstraint::Distance {
                first: "p0".into(),
                second: "p1".into(),
                value: length(10.0),
            },
            SketchConstraint::Distance {
                first: "p1".into(),
                second: "p2".into(),
                value: length(5.0),
            },
        ],
    }
}

fn arc_profile(clockwise: bool) -> SketchDefinition {
    let mut sketch = rectangle();
    sketch.points = vec![
        point("c", 0.0, 0.0, true),
        point("a", 2.0, 0.0, true),
        point("b", 0.0, 2.0, true),
    ];
    sketch.lines = vec![SketchLine {
        id: "chord".into(),
        start: "b".into(),
        end: "a".into(),
    }];
    sketch.arcs = vec![SketchArc {
        id: "arc".into(),
        center: "c".into(),
        start: "a".into(),
        end: "b".into(),
        clockwise,
    }];
    sketch.constraints.clear();
    sketch.profile = vec!["arc".into(), "chord".into()];
    sketch
}
