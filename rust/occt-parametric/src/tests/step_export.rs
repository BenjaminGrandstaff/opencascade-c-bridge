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
