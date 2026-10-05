//! Native DRAW viewer export.

use super::*;
use std::collections::HashSet;

fn view_graph(definition: &FamilyDefinition) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "row",
            "member",
            "source",
            3,
            VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
            "pattern",
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
    graph
}

#[test]
fn draw_views_write_named_colored_parts_that_draw_opens() {
    let mut definition = family(RequirementPriority::Advisory, 1e12);
    definition.requirements.clear();
    let mut graph = view_graph(&definition);
    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    let directory = std::env::temp_dir().join(format!("occb-view-{}", std::process::id()));
    let script = graph
        .export_draw_view(
            &session,
            &generation,
            &directory,
            &OutputSet::AllWithOutput("body".into()),
        )
        .unwrap();
    let text = std::fs::read_to_string(&script).unwrap();
    // Members inherit the source's material and color (sRGB of linear 0.2/0.5/0.8).
    for name in ["source", "member_0_", "member_1_", "member_2_"] {
        assert!(
            text.contains(&format!("vdisplay -dispMode 1 {name}\n")),
            "{text}"
        );
        assert!(
            text.contains(&format!("vsetcolor {name} #7CBCE7\n")),
            "{text}"
        );
    }
    assert!(text.contains("puts {member_2_ = member[2]:body}"));
    let loaded = session.load_brep(directory.join("model.brep")).unwrap();
    let expected = 4.0 * 10.0 * 20.0 * 30.0;
    assert!((session.volume(&loaded).unwrap() - expected).abs() < 1e-6 * expected);

    // The BREP and script assertions above always run. Executing the viewer
    // additionally needs DRAW and an X display, even in DRAW virtual mode.
    let wrapper = directory.join("check.tcl");
    let reload = std::fs::read_to_string(directory.join("reload.tcl")).unwrap();
    assert!(reload.contains("vremove -all\n") && !reload.contains("vinit"));
    assert!(reload.contains("vsetcolor member_2_ #7CBCE7\n"), "{reload}");
    std::fs::write(
        &wrapper,
        "source view.tcl\nputs VIEW_OK\nsource reload.tcl\nputs RELOAD_OK\n",
    )
    .unwrap();
    if let Some(output) = draw_output(&directory) {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("VIEW_OK"),
            "{stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(stdout.contains("source = source:body"), "{stdout}");
        assert!(stdout.contains("RELOAD_OK"), "{stdout}");
    }
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn draw_names_are_valid_and_unique() {
    use crate::assembly::draw_name_for_tests as name;
    let mut used = HashSet::new();
    assert_eq!(name("member[3]", &mut used), "member_3_");
    assert_eq!(name("member(3)", &mut used), "member_3__2");
    assert_eq!(name("7th", &mut used), "part_7th");
    assert_eq!(name("", &mut used), "part_");
    assert_eq!(name("wing.left", &mut used), "wing_left");
}

fn draw_output(directory: &std::path::Path) -> Option<std::process::Output> {
    std::env::var_os("DISPLAY")?;
    std::process::Command::new("DRAWEXE")
        .args(["-v", "-f", "check.tcl"])
        .current_dir(directory)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()
}
