//! Helix features as sweep paths for springs, driven by parameters.

use super::*;
use std::f64::consts::TAU;

const COIL_RADIUS: f64 = 10.0;
const PITCH: f64 = 4.0;
const WIRE_RADIUS: f64 = 1.5;

fn scalar(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::scalar(value))
}

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

fn fixed(id: &str, x: f64, y: f64) -> SketchPoint {
    SketchPoint {
        id: id.into(),
        x: length(x),
        y: length(y),
        fixed: true,
    }
}

/// A wire circle at the helix start, across its starting tangent
/// (0, 2πr, pitch): the sketch's x axis is radial and its y axis completes
/// the plane perpendicular to the tangent.
fn wire_section() -> SketchDefinition {
    let lead = TAU * COIL_RADIUS;
    let norm = lead.hypot(PITCH);
    SketchDefinition {
        id: "section".into(),
        datum_plane: None,
        face_support: None,
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            COIL_RADIUS,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, -PITCH / norm, lead / norm)),
        points: vec![fixed("center", 0.0, 0.0), fixed("rim", WIRE_RADIUS, 0.0)],
        lines: Vec::new(),
        circles: vec![SketchCircle {
            id: "circle".into(),
            center: "center".into(),
            rim: "rim".into(),
        }],
        ellipses: Vec::new(),
        profile_operations: Vec::new(),
        arcs: Vec::new(),
        splines: Vec::new(),
        profile: Vec::new(),
        constraints: Vec::new(),
    }
}

fn spring(left_handed: bool) -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.parameters = vec![ParameterDefinition {
        id: "turns".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(5.0)),
        minimum: Some(Quantity::scalar(0.5)),
        maximum: Some(Quantity::scalar(50.0)),
    }];
    family.features = vec![
        FeatureDefinition {
            id: "coil".into(),
            operation: FeatureOperation::Helix {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                start: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                radius: length(COIL_RADIUS),
                pitch: length(PITCH),
                turns: ScalarExpr::Parameter("turns".into()),
                left_handed,
            },
        },
        FeatureDefinition {
            id: "section".into(),
            operation: FeatureOperation::SketchFace {
                sketch: Box::new(wire_section()),
            },
        },
        FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Sweep {
                profile: "section".into(),
                path: "coil".into(),
                orientation: SweepOrientation::Binormal {
                    direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                },
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

fn tube_volume(turns: f64) -> f64 {
    std::f64::consts::PI * WIRE_RADIUS * WIRE_RADIUS * turns * (TAU * COIL_RADIUS).hypot(PITCH)
}

#[test]
fn helix_paths_sweep_springs_that_follow_the_turns_parameter() {
    let family = spring(false);
    let session = Session::new().unwrap();
    let mut instance = part(&family);
    let first = instance.regenerate(&session).unwrap();
    let body = first.shape("body").unwrap();
    assert!(session.is_valid(body).unwrap());
    let volume = session.volume(body).unwrap();
    assert!(
        (volume - tube_volume(5.0)).abs() < 1e-3 * tube_volume(5.0),
        "{volume}"
    );
    let bounds = session.exact_bounds(body).unwrap();
    // The coil reaches its radius plus the wire, and spans pitch times turns
    // plus the tilted end sections.
    assert!(
        (bounds.max.x - (COIL_RADIUS + WIRE_RADIUS)).abs() < 1e-3,
        "{bounds:?}"
    );
    assert!(
        bounds.max.z > 5.0 * PITCH && bounds.max.z < 5.0 * PITCH + WIRE_RADIUS,
        "{bounds:?}"
    );
    instance.overrides.insert(
        "turns".into(),
        ParameterValue::Scalar(Quantity::scalar(8.0)),
    );
    let longer = instance.regenerate_incremental(&session, &first).unwrap();
    let volume = session.volume(longer.shape("body").unwrap()).unwrap();
    assert!(
        (volume - tube_volume(8.0)).abs() < 1e-3 * tube_volume(8.0),
        "{volume}"
    );
    // The section sketch does not depend on the turns and is reused.
    assert!(longer.regeneration.reused.contains(&"section".to_string()));
    assert!(longer.regeneration.rebuilt.contains(&"coil".to_string()));
}

#[test]
fn left_handed_springs_mirror_right_handed_ones_and_helices_persist() {
    let session = Session::new().unwrap();
    let right = spring(false);
    let left = spring(true);
    let a = part(&right).regenerate(&session).unwrap();
    // The left-handed section sits across the mirrored tangent.
    let mut left = left;
    if let FeatureOperation::SketchFace { sketch } = &mut left.features[1].operation {
        let lead = TAU * COIL_RADIUS;
        let norm = lead.hypot(PITCH);
        sketch.y_axis =
            VectorExpr::Literal(VectorQuantity::scalars(0.0, PITCH / norm, lead / norm));
    }
    let b = part(&left).regenerate(&session).unwrap();
    let (ra, rb) = (a.shape("body").unwrap(), b.shape("body").unwrap());
    assert!(session.is_valid(rb).unwrap());
    let (va, vb) = (session.volume(ra).unwrap(), session.volume(rb).unwrap());
    assert!((va - vb).abs() < 1e-6 * va);
    // Right-handed coils climb counterclockwise (+y first); left-handed, clockwise.
    let ba = session.bounds(a.shape("coil").unwrap()).unwrap();
    let bb = session.bounds(b.shape("coil").unwrap()).unwrap();
    assert!((ba.max.y - bb.max.y).abs() < 1e-6 && (ba.min.y - bb.min.y).abs() < 1e-6);

    let mut graph = InstanceGraph::new(&right);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let json = document.to_json_pretty().unwrap();
    if let Some(directory) = std::env::var_os("OCCB_HELIX_QA_DIR") {
        std::fs::write(std::path::Path::new(&directory).join("spring.json"), &json).unwrap();
    }
    assert!(json.contains("\"helix\""));
    assert!(!json.contains("left_handed\": true"));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);

    // Out-of-range turns fail at the helix without leaking handles.
    let mut bad = spring(false);
    if let FeatureOperation::Helix { turns, .. } = &mut bad.features[0].operation {
        *turns = scalar(0.0);
    }
    assert!(part(&bad).regenerate(&session).is_err());
    drop((a, b));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn ai_spring_example_reorients_its_profile_after_radius_and_pitch_edits() {
    let request: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tools/model/spring.request.json")).unwrap();
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let mut instance = part(&document.family);
    let first = instance.regenerate(&session).unwrap();
    instance.overrides.insert(
        "coil_radius".into(),
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
    );
    instance.overrides.insert(
        "pitch".into(),
        ParameterValue::Scalar(Quantity::length(6.0, LengthUnit::Millimeter)),
    );
    instance.overrides.insert(
        "turns".into(),
        ParameterValue::Scalar(Quantity::scalar(7.5)),
    );
    let edited = instance.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(edited.regeneration.rebuilt, vec!["coil", "profile", "body"]);
    let body = edited.shape("body").unwrap();
    assert!(session.is_valid(body).unwrap());
    let expected = std::f64::consts::PI * 7.5 * (TAU * 12.0).hypot(6.0);
    assert!((session.volume(body).unwrap() - expected).abs() < expected * 1e-3);
    let count = session.shape_count().unwrap();
    instance.overrides.insert(
        "wire_radius".into(),
        ParameterValue::Scalar(Quantity::length(4.0, LengthUnit::Millimeter)),
    );
    assert!(instance.regenerate_incremental(&session, &edited).is_err());
    assert_eq!(session.shape_count().unwrap(), count);
    assert!(session.is_valid(body).unwrap());
    drop((first, edited));
    assert_eq!(session.shape_count().unwrap(), 0);
}
