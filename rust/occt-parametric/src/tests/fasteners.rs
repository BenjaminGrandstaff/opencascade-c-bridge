//! Fastened joints (schema 186): hardware placed from the measured stack,
//! holes that clear it exactly, the parts list, and refused joints.

use super::*;
use occt_parametric_core::fasteners::{JointFastening, JointHead};
use std::f64::consts::PI;

fn length(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn direction(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::scalars(x, y, z))
}
fn feature(id: &str, operation: FeatureOperation) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation,
    }
}
fn block(id: &str, origin: [f64; 3], size: [f64; 3]) -> FeatureDefinition {
    feature(
        id,
        FeatureOperation::Box {
            origin: point(origin[0], origin[1], origin[2]),
            size: point(size[0], size[1], size[2]),
        },
    )
}
#[allow(clippy::too_many_arguments)]
fn joint(
    stack: &[&str],
    positions: &[[f64; 3]],
    axis: [f64; 3],
    nominal: f64,
    fastening: JointFastening,
    head: JointHead,
    washers: bool,
    screw_length: Option<f64>,
) -> FeatureDefinition {
    feature(
        "joint",
        FeatureOperation::FastenedJoint {
            stack: stack.iter().map(|s| s.to_string()).collect(),
            positions: positions.iter().map(|p| point(p[0], p[1], p[2])).collect(),
            axis: direction(axis[0], axis[1], axis[2]),
            nominal_diameter: length(nominal),
            fastening,
            head,
            washers,
            clearance: None,
            length: screw_length.map(length),
            engagement: None,
        },
    )
}
fn holes(id: &str, input: &str) -> FeatureDefinition {
    feature(
        id,
        FeatureOperation::JointHoles {
            joint: "joint".into(),
            input: input.into(),
        },
    )
}
fn standard(
    id: &str,
    part: StandardPartKind,
    nominal: f64,
    size: Option<f64>,
) -> FeatureDefinition {
    feature(
        id,
        FeatureOperation::StandardPart {
            part,
            nominal_diameter: length(nominal),
            length: size.map(length),
            origin: point(500.0, 0.0, 0.0),
            axis: direction(0.0, 0.0, 1.0),
        },
    )
}
fn family(features: Vec<FeatureDefinition>) -> FamilyDefinition {
    FamilyDefinition {
        history: Default::default(),
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "joint".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        datums: Vec::new(),
        requirements: Vec::new(),
        features,
    }
}
fn instance(definition: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
fn close(actual: f64, expected: f64) {
    assert!(
        (actual / expected - 1.0).abs() < 1e-7,
        "{actual} vs {expected}"
    );
}
fn regeneration_error(features: Vec<FeatureDefinition>) -> String {
    let definition = family(features);
    let session = Session::new().unwrap();
    match instance(&definition).regenerate(&session) {
        Ok(_) => panic!("expected the joint to be refused"),
        Err(error) => error.message,
    }
}

#[test]
fn a_bolted_joint_places_its_hardware_and_clears_it_exactly() {
    use StandardPartKind::*;
    // M6 down through a 10 mm lid on an 8 mm base: 30 mm screw, washers,
    // nuts. Positions lie anywhere on each screw's line.
    let definition = family(vec![
        block("base", [0.0, 0.0, 0.0], [60.0, 40.0, 8.0]),
        block("lid", [0.0, 0.0, 8.0], [60.0, 40.0, 10.0]),
        joint(
            &["lid", "base"],
            &[[15.0, 20.0, 30.0], [45.0, 20.0, -4.0]],
            [0.0, 0.0, -1.0],
            6.0,
            JointFastening::Nut,
            JointHead::Surface,
            true,
            None,
        ),
        holes("lid_drilled", "lid"),
        holes("base_drilled", "base"),
        standard("screw", SocketHeadCapScrewIso4762, 6.0, Some(30.0)),
        standard("washer", PlainWasherIso7089, 6.0, None),
        standard("nut", HexNutIso4032, 6.0, None),
    ]);
    let session = Session::new().unwrap();
    let part = instance(&definition);
    let generated = part.regenerate(&session).unwrap();
    let shape = |id: &str| generated.shape(id).unwrap();
    let volume = |id: &str| session.volume(shape(id)).unwrap();
    let hardware = shape("joint");
    assert!(session.is_valid(hardware).unwrap());
    assert_eq!(
        session.subshape_count(hardware, ShapeType::Solid).unwrap(),
        8
    );
    close(
        volume("joint"),
        2.0 * volume("screw") + 4.0 * volume("washer") + 2.0 * volume("nut"),
    );
    // Head on a washer on the lid; screw tip 2 mm + 2 pitches past the nut.
    let bounds = session.exact_bounds(hardware).unwrap();
    close(bounds.max.z, 18.0 + 1.6 + 6.0);
    close(bounds.min.z, 18.0 + 1.6 - 30.0);
    // Medium ISO 273 clearance: 6.6 mm through each part.
    let hole = PI * 3.3 * 3.3;
    close(
        volume("lid_drilled"),
        60.0 * 40.0 * 10.0 - 2.0 * hole * 10.0,
    );
    close(volume("base_drilled"), 60.0 * 40.0 * 8.0 - 2.0 * hole * 8.0);
    for drilled in ["lid_drilled", "base_drilled"] {
        assert!(session.is_valid(shape(drilled)).unwrap());
        let overlap = session.overlap_volume(hardware, shape(drilled)).unwrap();
        assert!(
            overlap < 1e-6,
            "{drilled} overlaps the hardware by {overlap}"
        );
    }
    // The plan behind it, as reports read it.
    let joints = part.fastened_joints(&session, &generated, &[]).unwrap();
    let (id, placed) = &joints[0];
    assert_eq!(id, "joint");
    assert_eq!((placed.plan.length, placed.plan.chosen), (30.0, true));
    let lines: Vec<_> = placed
        .plan
        .hardware()
        .into_iter()
        .map(|item| (item.designation, item.quantity))
        .collect();
    assert_eq!(
        lines,
        [
            ("ISO 4762 M6 × 30".to_string(), 2),
            ("ISO 7089 6".to_string(), 4),
            ("ISO 4032 M6".to_string(), 2)
        ]
    );
}

#[test]
fn a_tapped_joint_sinks_its_head_and_drills_a_blind_tapped_hole() {
    // M5 along +x through a 10 mm lid into a 20 mm base, head counterbored:
    // Carr Lane M5 recess 9.75 × 5, so the 16 mm screw's head is flush.
    let definition = family(vec![
        block("lid", [0.0, 0.0, 0.0], [10.0, 40.0, 40.0]),
        block("base", [10.0, 0.0, 0.0], [20.0, 40.0, 40.0]),
        joint(
            &["lid", "base"],
            &[[5.0, 20.0, 20.0]],
            [1.0, 0.0, 0.0],
            5.0,
            JointFastening::Tapped,
            JointHead::Counterbored,
            true,
            None,
        ),
        holes("lid_drilled", "lid"),
        holes("base_drilled", "base"),
        standard(
            "screw",
            StandardPartKind::SocketHeadCapScrewIso4762,
            5.0,
            Some(16.0),
        ),
    ]);
    let session = Session::new().unwrap();
    let part = instance(&definition);
    let generated = part.regenerate(&session).unwrap();
    let shape = |id: &str| generated.shape(id).unwrap();
    let volume = |id: &str| session.volume(shape(id)).unwrap();
    close(volume("joint"), volume("screw"));
    let bounds = session.exact_bounds(shape("joint")).unwrap();
    assert!(bounds.min.x.abs() < 1e-9, "head is flush: {}", bounds.min.x);
    close(bounds.max.x, 5.0 + 16.0);
    // Counterbore 9.75 × 5, then a 5.5 mm clearance hole.
    let lid_cut = PI * (4.875f64.powi(2) * 5.0 + 2.75f64.powi(2) * 5.0);
    close(volume("lid_drilled"), 4000.0 * 4.0 - lid_cut);
    // 4.2 mm tap drill, 11 mm engaged + 2 pitches, 118° point.
    let r: f64 = 2.1;
    let tip = r / 59f64.to_radians().tan();
    let base_cut = PI * r * r * (11.0 + 1.6) + PI * r * r * tip / 3.0;
    close(volume("base_drilled"), 20.0 * 1600.0 - base_cut);
    for drilled in ["lid_drilled", "base_drilled"] {
        let overlap = session
            .overlap_volume(shape("joint"), shape(drilled))
            .unwrap();
        // The screw's shank (nominal 5 mm) fills the 4.2 mm tapped hole's
        // thread, as a modeled thread would; nothing else overlaps.
        if drilled == "lid_drilled" {
            assert!(overlap < 1e-6, "{overlap}");
        } else {
            close(overlap, PI * (2.5 * 2.5 - r * r) * 11.0);
        }
    }
    let joints = part.fastened_joints(&session, &generated, &[]).unwrap();
    let screw = &joints[0].1.plan.screws[0];
    assert!((screw.thread_engagement - 11.0).abs() < 1e-9);
}

#[test]
fn joints_that_cannot_assemble_are_refused_with_the_reason() {
    let nut = |stack: &[&str], positions: &[[f64; 3]]| {
        joint(
            stack,
            positions,
            [0.0, 0.0, -1.0],
            6.0,
            JointFastening::Nut,
            JointHead::Surface,
            true,
            None,
        )
    };
    let base = || block("base", [0.0, 0.0, 0.0], [60.0, 40.0, 8.0]);
    // A lid floating 1 mm above the base.
    let message = regeneration_error(vec![
        base(),
        block("lid", [0.0, 0.0, 9.0], [60.0, 40.0, 10.0]),
        nut(&["lid", "base"], &[[15.0, 20.0, 0.0]]),
    ]);
    assert!(
        message.contains("'lid' and 'base' are 1 mm apart"),
        "{message}"
    );
    // A screw line that misses the base.
    let message = regeneration_error(vec![
        base(),
        block("lid", [0.0, 0.0, 8.0], [90.0, 40.0, 10.0]),
        nut(&["lid", "base"], &[[15.0, 20.0, 0.0], [80.0, 20.0, 0.0]]),
    ]);
    assert!(
        message.contains("joint position 2 misses 'base'"),
        "{message}"
    );
    // Screws whose washers would collide.
    let message = regeneration_error(vec![
        base(),
        block("lid", [0.0, 0.0, 8.0], [60.0, 40.0, 10.0]),
        nut(&["lid", "base"], &[[15.0, 20.0, 0.0], [25.0, 20.0, 0.0]]),
    ]);
    assert!(
        message.contains("positions 1 and 2 are 10 mm apart"),
        "{message}"
    );
    // Holes for a part the joint does not clamp.
    let message = regeneration_error(vec![
        base(),
        block("lid", [0.0, 0.0, 8.0], [60.0, 40.0, 10.0]),
        block("other", [100.0, 0.0, 0.0], [10.0, 10.0, 10.0]),
        nut(&["lid", "base"], &[[15.0, 20.0, 0.0]]),
        holes("drilled", "other"),
    ]);
    assert!(
        message.contains("'other' is not in the stack of joint 'joint'"),
        "{message}"
    );
}

#[test]
fn joint_holes_follow_edits_to_the_stack() {
    // Thickening the lid through a parameter rebuilds the joint with a
    // longer screw and redrills both parts.
    let mut definition = family(vec![
        block("base", [0.0, 0.0, 0.0], [60.0, 40.0, 8.0]),
        feature(
            "lid",
            FeatureOperation::Box {
                origin: point(0.0, 0.0, 8.0),
                size: VectorExpr::Components {
                    x: length(60.0),
                    y: length(40.0),
                    z: ScalarExpr::Parameter("lid".into()),
                },
            },
        ),
        joint(
            &["lid", "base"],
            &[[30.0, 20.0, 0.0]],
            [0.0, 0.0, -1.0],
            6.0,
            JointFastening::Nut,
            JointHead::Surface,
            true,
            None,
        ),
        holes("lid_drilled", "lid"),
    ]);
    definition.parameters.push(ParameterDefinition {
        id: "lid".into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
        minimum: None,
        maximum: None,
    });
    let session = Session::new().unwrap();
    for (thickness, screw) in [(10.0, 30.0), (20.0, 40.0)] {
        let mut part = instance(&definition);
        part.overrides.insert(
            "lid".into(),
            ParameterValue::Scalar(Quantity::length(thickness, LengthUnit::Millimeter)),
        );
        let generated = part.regenerate(&session).unwrap();
        let joints = part.fastened_joints(&session, &generated, &[]).unwrap();
        assert_eq!(joints[0].1.plan.length, screw);
        close(
            session
                .volume(generated.shape("lid_drilled").unwrap())
                .unwrap(),
            2400.0 * thickness - PI * 3.3 * 3.3 * thickness,
        );
    }
}
