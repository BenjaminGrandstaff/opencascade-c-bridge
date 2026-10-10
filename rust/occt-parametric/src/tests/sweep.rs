//! Sweeps of sketch profiles along sketch paths.

use super::*;

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

fn point(id: &str, x: ScalarExpr, y: ScalarExpr) -> SketchPoint {
    SketchPoint {
        id: id.into(),
        x,
        y,
        fixed: true,
    }
}

fn plane_sketch(id: &str, x_axis: (f64, f64, f64), y_axis: (f64, f64, f64)) -> SketchDefinition {
    SketchDefinition {
        id: id.into(),
        datum_plane: None,
        face_support: None,
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(x_axis.0, x_axis.1, x_axis.2)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(y_axis.0, y_axis.1, y_axis.2)),
        points: Vec::new(),
        lines: Vec::new(),
        circles: Vec::new(),
        arcs: Vec::new(),
        ellipses: Vec::new(),
        profile_operations: Vec::new(),
        splines: Vec::new(),
        profile: Vec::new(),
        constraints: Vec::new(),
    }
}

/// A disc of radius `radius` across the path start (the YZ plane), swept
/// along a 10 mm line and a tangent quarter arc of radius 10 in the XY plane.
fn sweep_family(orientation: SweepOrientation) -> FamilyDefinition {
    let mut profile = plane_sketch("disc", (0.0, 1.0, 0.0), (0.0, 0.0, 1.0));
    profile.points = vec![
        point("c", length(0.0), length(0.0)),
        point("r", ScalarExpr::Parameter("radius".into()), length(0.0)),
    ];
    profile.circles = vec![SketchCircle {
        id: "rim".into(),
        center: "c".into(),
        rim: "r".into(),
    }];
    let mut path = plane_sketch("route", (1.0, 0.0, 0.0), (0.0, 1.0, 0.0));
    path.points = vec![
        point("a", length(0.0), length(0.0)),
        point("b", length(10.0), length(0.0)),
        point("o", length(10.0), length(10.0)),
        point("e", length(20.0), length(10.0)),
    ];
    path.lines = vec![SketchLine {
        id: "run".into(),
        start: "a".into(),
        end: "b".into(),
    }];
    path.arcs = vec![SketchArc {
        id: "bend".into(),
        center: "o".into(),
        start: "b".into(),
        end: "e".into(),
        clockwise: false,
    }];
    path.profile = vec!["run".into(), "bend".into()];
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.parameters.push(length_parameter("radius", 2.0));
    family.features = vec![
        FeatureDefinition {
            id: "pipe".into(),
            operation: FeatureOperation::Sweep {
                profile: "profile".into(),
                path: "path".into(),
                orientation,
            },
        },
        FeatureDefinition {
            id: "profile".into(),
            operation: FeatureOperation::SketchFace {
                sketch: Box::new(profile),
            },
        },
        FeatureDefinition {
            id: "path".into(),
            operation: FeatureOperation::SketchOpenWire {
                sketch: Box::new(path),
            },
        },
    ];
    family
}

fn part(family: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition: family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

#[test]
fn sketch_profiles_sweep_along_sketch_paths_and_follow_parameters() {
    let session = Session::new().unwrap();
    let pi = std::f64::consts::PI;
    let path_length = 10.0 + pi / 2.0 * 10.0;
    let family = sweep_family(SweepOrientation::CorrectedFrenet);
    let mut instance = part(&family);
    let first = instance.regenerate(&session).unwrap();
    let pipe = first.shape("pipe").unwrap();
    assert!(session.is_valid(pipe).unwrap());
    let volume = session.volume(pipe).unwrap();
    assert!(
        (volume - pi * 4.0 * path_length).abs() < 1e-6 * volume,
        "{volume}"
    );

    instance.overrides.insert(
        "radius".into(),
        ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
    );
    let second = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.reused, vec!["path"]);
    assert_eq!(second.regeneration.rebuilt, vec!["profile", "pipe"]);
    let volume = session.volume(second.shape("pipe").unwrap()).unwrap();
    assert!(
        (volume - pi * 9.0 * path_length).abs() < 1e-6 * volume,
        "{volume}"
    );

    // A tube wider than the 10 mm bend fails and keeps the accepted result.
    instance.overrides.insert(
        "radius".into(),
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
    );
    let error = instance
        .regenerate_incremental(&session, &second)
        .err()
        .unwrap();
    assert!(
        error.message.contains("intersect itself"),
        "{}",
        error.message
    );
    assert!(session.is_valid(second.shape("pipe").unwrap()).unwrap());
    drop((first, second));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn sweep_orientations_validate_and_persist() {
    let session = Session::new().unwrap();
    let pi = std::f64::consts::PI;
    // A binormal along +z keeps the disc upright through the flat bend.
    let binormal = sweep_family(SweepOrientation::Binormal {
        direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
    });
    let generated = part(&binormal).regenerate(&session).unwrap();
    let volume = session.volume(generated.shape("pipe").unwrap()).unwrap();
    assert!((volume - pi * 4.0 * (10.0 + pi * 5.0)).abs() < 1e-6 * volume);

    let unitless = sweep_family(SweepOrientation::Binormal {
        direction: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            1.0,
            LengthUnit::Millimeter,
        )),
    });
    assert!(part(&unitless).regenerate(&session).is_err());

    let mut graph = InstanceGraph::new(&binormal);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"binormal\""));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    let plain = serde_json::to_string(&FeatureOperation::Sweep {
        profile: "a".into(),
        path: "b".into(),
        orientation: SweepOrientation::default(),
    })
    .unwrap();
    let defaulted: FeatureOperation =
        serde_json::from_str(&plain.replace(",\"orientation\":\"corrected_frenet\"", "")).unwrap();
    assert!(matches!(
        defaulted,
        FeatureOperation::Sweep {
            orientation: SweepOrientation::CorrectedFrenet,
            ..
        }
    ));
}
