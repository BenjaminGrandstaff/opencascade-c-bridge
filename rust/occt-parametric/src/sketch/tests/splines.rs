use super::*;

fn line(id: &str, start: &str, end: &str) -> SketchLine {
    SketchLine {
        id: id.into(),
        start: start.into(),
        end: end.into(),
    }
}

fn spline(id: &str, points: &[&str]) -> SketchSpline {
    SketchSpline {
        id: id.into(),
        points: points.iter().map(|point| (*point).into()).collect(),
    }
}

fn tangent(first: &str, second: &str, point: &str) -> SketchConstraint {
    SketchConstraint::Tangent {
        first: first.into(),
        second: second.into(),
        point: point.into(),
    }
}

/// A 20 x 10 slot whose top is a spline through `crown`, which the solver
/// places at (10, 15) from two distance constraints.
fn arched_slot(tangents: bool) -> SketchDefinition {
    let mut sketch = rectangle();
    sketch.id = "arch".into();
    sketch.points = vec![
        point("p0", 0.0, 0.0, true),
        point("p1", 20.0, 0.0, true),
        point("p2", 20.0, 10.0, true),
        point("p3", 0.0, 10.0, true),
        point("crown", 9.0, 14.0, false),
        point("crown_foot", 10.0, 0.0, true),
        point("crown_side", 0.0, 15.0, true),
    ];
    sketch.lines = vec![
        line("bottom", "p0", "p1"),
        line("right", "p1", "p2"),
        line("left", "p3", "p0"),
    ];
    sketch.splines = vec![spline("top", &["p2", "crown", "p3"])];
    sketch.profile = vec!["bottom".into(), "right".into(), "top".into(), "left".into()];
    sketch.constraints = vec![
        SketchConstraint::Distance {
            first: "crown_foot".into(),
            second: "crown".into(),
            value: length(15.0),
        },
        // With 15 mm up from (10, 0), 10 mm from (0, 15) puts it at (10, 15).
        SketchConstraint::Distance {
            first: "crown_side".into(),
            second: "crown".into(),
            value: length(10.0),
        },
    ];
    if tangents {
        sketch
            .constraints
            .extend([tangent("right", "top", "p2"), tangent("top", "left", "p3")]);
    }
    sketch
}

#[test]
fn splines_join_profiles_with_lines_and_take_tangency_from_neighbors() {
    let session = Session::new().unwrap();
    let parameters = HashMap::new();
    let solution = arched_slot(true).solve(&parameters).unwrap();
    assert!(solution.solved);
    let crown = solution.points["crown"];
    assert!(
        (crown.x - 10.0).abs() < 1e-6 && (crown.y - 15.0).abs() < 1e-6,
        "{crown:?}"
    );

    let area = |tangents: bool| {
        let face = arched_slot(tangents).face(&session, &parameters).unwrap();
        assert!(session.is_valid(&face).unwrap());
        assert_eq!(
            session
                .subshape_count(&face, occt_bridge::ShapeType::Edge)
                .unwrap(),
            4
        );
        session.surface_area(&face).unwrap()
    };
    // Leaving the vertical sides straight up bulges the arch outward near the
    // corners, enclosing more than the free spline does.
    let (free, smooth) = (area(false), area(true));
    assert!(free > 200.0 && smooth > free + 1.0, "{free} {smooth}");

    let face = arched_slot(true).face(&session, &parameters).unwrap();
    let solid = session
        .create_prism_from_face(&face, Vec3::new(0.0, 0.0, 4.0))
        .unwrap();
    assert!(session.is_valid(&solid).unwrap());
    assert!((session.volume(&solid).unwrap() - 4.0 * smooth).abs() < 1e-6 * smooth);
}

#[test]
fn a_spline_closed_on_its_first_point_is_a_smooth_loop() {
    let session = Session::new().unwrap();
    let mut sketch = rectangle();
    sketch.id = "loop".into();
    sketch.lines.clear();
    sketch.constraints.clear();
    sketch.points = (0..6)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / 6.0;
            point(
                &format!("q{index}"),
                10.0 * angle.cos(),
                10.0 * angle.sin(),
                true,
            )
        })
        .collect();
    sketch.splines = vec![spline("ring", &["q0", "q1", "q2", "q3", "q4", "q5", "q0"])];
    let face = sketch.face(&session, &HashMap::new()).unwrap();
    assert_eq!(
        session
            .subshape_count(&face, occt_bridge::ShapeType::Edge)
            .unwrap(),
        1
    );
    let area = session.surface_area(&face).unwrap();
    let circle = std::f64::consts::PI * 100.0;
    assert!((area - circle).abs() / circle < 2e-2, "{area}");
}

#[test]
fn spline_sketches_validate_and_persist() {
    let parameters = HashMap::new();
    let failures = [
        (
            {
                let mut sketch = arched_slot(false);
                sketch.splines = vec![spline("top", &["p2"])];
                sketch
            },
            "2 or more distinct known points",
        ),
        (
            {
                let mut sketch = arched_slot(false);
                sketch.splines = vec![spline("top", &["p2", "crown", "p2", "p3"])];
                sketch
            },
            "distinct known points",
        ),
        (
            {
                let mut sketch = arched_slot(false);
                sketch.splines.push(spline("extra", &["p3", "p0"]));
                sketch.constraints.push(tangent("top", "extra", "p3"));
                sketch
            },
            "only to a line or an arc",
        ),
        (
            {
                let mut sketch = arched_slot(false);
                sketch.splines = vec![spline("top", &["p2", "crown", "p3", "p2"])];
                sketch.constraints.push(tangent("right", "top", "p2"));
                sketch
            },
            "closed spline has no free ends",
        ),
        (
            {
                let mut sketch = arched_slot(false);
                sketch.profile.clear();
                sketch
            },
            "explicit profile",
        ),
    ];
    let session = Session::new().unwrap();
    for (sketch, message) in failures {
        let error = sketch.face(&session, &parameters).err().unwrap();
        assert!(error.message.contains(message), "{}", error.message);
    }
    let mut mixed = arched_slot(false);
    mixed.splines = vec![spline("top", &["p2", "crown", "p3", "p2"])];
    let error = mixed.face(&session, &parameters).err().unwrap();
    assert!(
        error.message.contains("only entity in its profile"),
        "{}",
        error.message
    );

    let arch = arched_slot(true);
    let json = serde_json::to_string(&arch).unwrap();
    assert_eq!(
        serde_json::from_str::<SketchDefinition>(&json).unwrap(),
        arch
    );
    let plain = serde_json::to_string(&rectangle()).unwrap();
    assert!(
        !plain.contains("splines"),
        "documents without splines are unchanged"
    );
}
