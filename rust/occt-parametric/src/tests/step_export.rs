//! Structured STEP export of generated graphs.

use super::*;

#[test]
fn graphs_export_shared_variants_as_shared_step_parts() {
    let mut definition = family(RequirementPriority::Advisory, 1e12);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "row",
            "member",
            "source",
            5,
            VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
            "pattern",
        )
        .unwrap();
    // A wider block is a second variant, and so a second part.
    let wide = HashMap::from([(
        "width".to_owned(),
        ParameterValue::Scalar(Quantity::length(15.0, LengthUnit::Millimeter)),
    )]);
    graph.add_base("wide", wide, "test").unwrap();
    graph
        .set_placement(
            "wide",
            Placement::translated(VectorQuantity::lengths(
                0.0,
                100.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph
        .add_material(Material {
            id: "pla".into(),
            name: "PLA".into(),
            density_kg_per_cubic_meter: 1240.0,
        })
        .unwrap();
    graph.assign_material("source", Some("pla")).unwrap();
    graph.assembly.material_appearances.insert(
        "pla".into(),
        MaterialAppearance {
            base_color: [0.2, 0.5, 0.8, 1.0],
            ..MaterialAppearance::default()
        },
    );
    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    let path = std::env::temp_dir().join(format!("occb-graph-{}.step", std::process::id()));
    let parts = graph
        .export_step(
            &session,
            &generation,
            &path,
            "row assembly",
            &OutputSet::AllWithOutput("body".into()),
        )
        .unwrap();
    assert_eq!(
        parts, 2,
        "six identical blocks share one part; the wide block is another"
    );
    let loaded = session.load_step(&path).unwrap();
    let expected = 6.0 * 10.0 * 20.0 * 30.0 + 15.0 * 20.0 * 30.0;
    assert!((session.volume(&loaded).unwrap() - expected).abs() < 1e-6 * expected);
    std::fs::remove_file(&path).unwrap();

    let error = graph
        .export_step(
            &session,
            &generation,
            &path,
            "row assembly",
            &OutputSet::AllWithOutput("bodyy".into()),
        )
        .unwrap_err();
    assert!(error.message.contains("matches no generated output"));
    assert!(!path.exists());
}

#[test]
fn linear_appearance_colors_convert_to_srgb() {
    use crate::assembly::srgb_for_tests as srgb;
    assert_eq!(srgb(0.0), 0.0);
    assert!((srgb(1.0) - 1.0).abs() < 1e-12);
    assert!((srgb(0.2) - 0.484_529).abs() < 1e-5);
    assert!((srgb(0.002) - 0.025_84).abs() < 1e-6);
}

fn lengths(x: f64, y: f64, z: f64) -> VectorQuantity {
    VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter)
}

#[test]
fn placements_become_rigid_transforms() {
    use crate::assembly::rigid_for_tests as rigid;
    // A quarter turn about z through (1, 0, 0), then up 5: (2, 0, 0) lands
    // at (1, 1, 5).
    let m = rigid(Placement {
        translation: lengths(0.0, 0.0, 5.0),
        rotation: Some(AxisAngle {
            origin: lengths(1.0, 0.0, 0.0),
            axis: VectorQuantity::scalars(0.0, 0.0, 2.0),
            angle_radians: std::f64::consts::FRAC_PI_2,
        }),
    })
    .unwrap();
    let apply = |p: [f64; 3]| {
        [0, 1, 2].map(|row| {
            m[4 * row] * p[0] + m[4 * row + 1] * p[1] + m[4 * row + 2] * p[2] + m[4 * row + 3]
        })
    };
    let moved = apply([2.0, 0.0, 0.0]);
    for (got, want) in moved.iter().zip([1.0, 1.0, 5.0]) {
        assert!((got - want).abs() < 1e-12, "{moved:?}");
    }
}

/// Frames holding outputs become STEP sub-assemblies: a turned, moved wing
/// frame, and a flap frame inside it hinged by a revolute joint. The file's
/// model-space geometry matches the generated, placed shapes.
#[test]
fn assembly_frames_export_as_step_sub_assemblies() {
    let mut definition = family(RequirementPriority::Advisory, 1e12);
    definition.requirements.clear();
    let mut graph = InstanceGraph::new(&definition);
    graph
        .add_frame(
            "wing",
            None,
            Placement {
                translation: lengths(100.0, 0.0, 0.0),
                rotation: Some(AxisAngle {
                    origin: lengths(5.0, 5.0, 0.0),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_radians: 0.4,
                }),
            },
            "test",
        )
        .unwrap();
    graph
        .add_frame(
            "flap",
            Some("wing"),
            Placement::translated(lengths(0.0, 50.0, 0.0)),
            "test",
        )
        .unwrap();
    graph
        .add_joint(AssemblyJoint {
            id: "hinge".into(),
            frame: "flap".into(),
            origin: lengths(0.0, 50.0, 0.0),
            axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            kind: JointKind::Revolute {
                angle: JointScalar {
                    value: Quantity::scalar(0.3),
                    minimum: None,
                    maximum: None,
                },
            },
        })
        .unwrap();
    graph
        .add_frame("empty", None, Placement::identity(), "test")
        .unwrap();
    for (id, frame) in [
        ("root", None),
        ("spar", Some("wing")),
        ("tab", Some("flap")),
    ] {
        graph.add_base(id, HashMap::new(), "test").unwrap();
        graph.set_instance_frame(id, frame).unwrap();
    }
    graph
        .set_placement("tab", Placement::translated(lengths(1.0, 2.0, 3.0)))
        .unwrap();
    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    let path = std::env::temp_dir().join(format!("occb-frames-{}.step", std::process::id()));
    let parts = graph
        .export_step(
            &session,
            &generation,
            &path,
            "airframe",
            &OutputSet::AllWithOutput("body".into()),
        )
        .unwrap();
    assert_eq!(parts, 1, "three placements of one block");

    let text = std::fs::read_to_string(&path).unwrap();
    for name in ["wing", "flap"] {
        assert!(text.contains(&format!("PRODUCT('{name}'")), "{name}");
    }
    assert!(
        !text.contains("PRODUCT('empty'"),
        "frames without outputs are omitted"
    );
    let occurrences = text.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count();
    assert_eq!(occurrences, 5, "wing, flap, and three components");

    let loaded = session.load_step(&path).unwrap();
    let shapes =
        ["root", "spar", "tab"].map(|id| generation.result(id).unwrap().shape("body").unwrap());
    let placed = session.create_compound(&shapes).unwrap();
    let (got, want) = (
        session.bounds(&loaded).unwrap(),
        session.bounds(&placed).unwrap(),
    );
    for (a, b) in [
        (got.min.x, want.min.x),
        (got.min.y, want.min.y),
        (got.min.z, want.min.z),
        (got.max.x, want.max.x),
        (got.max.y, want.max.y),
        (got.max.z, want.max.z),
    ] {
        assert!((a - b).abs() < 1e-6, "{got:?} vs {want:?}");
    }
    let (got, want) = (
        session.center_of_mass(&loaded).unwrap(),
        session.center_of_mass(&placed).unwrap(),
    );
    assert!(
        (got.x - want.x).abs() + (got.y - want.y).abs() + (got.z - want.z).abs() < 1e-6,
        "{got:?} vs {want:?}"
    );
    std::fs::remove_file(&path).unwrap();
}
