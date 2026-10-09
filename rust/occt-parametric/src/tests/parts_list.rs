//! Parts lists and item balloons on drawings.

use super::*;

fn block_family() -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family
}

fn output(instance: &str) -> InstanceOutputRef {
    InstanceOutputRef {
        instance: instance.into(),
        output: "body".into(),
    }
}

/// Five blocks: three identical aluminum ones (a source and its pattern of
/// two, one more suppressed), a wider one, and a steel one.
fn assembly(family: &FamilyDefinition) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(family);
    graph.add_base("plate", HashMap::new(), "test").unwrap();
    let members = graph
        .add_linear_pattern(
            "row",
            "copy",
            "plate",
            3,
            VectorQuantity::lengths(0.0, 60.0, 0.0, LengthUnit::Millimeter),
            "test",
        )
        .unwrap();
    graph.set_member_suppressed(&members[2], true).unwrap();
    graph
        .add_base(
            "wide",
            HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(25.0, LengthUnit::Millimeter)),
            )]),
            "test",
        )
        .unwrap();
    graph
        .add_base("steel_plate", HashMap::new(), "test")
        .unwrap();
    for (id, name) in [("al", "Aluminum 6061"), ("st", "Steel 1018")] {
        graph
            .add_material(Material {
                id: id.into(),
                name: name.into(),
                density_kg_per_cubic_meter: 2700.0,
            })
            .unwrap();
    }
    for instance in ["plate", "wide"] {
        graph.assign_material(instance, Some("al")).unwrap();
    }
    graph.assign_material("steel_plate", Some("st")).unwrap();
    for (id, x) in [("wide", 150.0), ("steel_plate", 300.0)] {
        graph
            .set_placement(
                id,
                Placement::translated(VectorQuantity::lengths(x, 0.0, 0.0, LengthUnit::Millimeter)),
            )
            .unwrap();
    }
    graph
}

fn page(members: &[String]) -> DrawingDefinition {
    let mut outputs = vec![output("plate"), output("wide")];
    // Views may not show suppressed members, so the third copy is left out.
    outputs.extend(members.iter().take(2).map(|m| output(m)));
    outputs.push(output("steel_plate"));
    DrawingDefinition {
        datum_reference_frames: Vec::new(),
        datum_features: Vec::new(),
        feature_control_frames: Vec::new(),
        surface_textures: Vec::new(),
        parts_list: Some(DrawingPartsList {
            position_mm: [250.0, 280.0],
            part_numbers: BTreeMap::new(),
        }),
        balloons: vec![
            DrawingBalloon {
                id: "plate".into(),
                view: "top".into(),
                instance: "plate".into(),
                anchor: None,
                offset_mm: [-15.0, 20.0],
            },
            DrawingBalloon {
                id: "steel".into(),
                view: "top".into(),
                instance: "steel_plate".into(),
                anchor: None,
                offset_mm: [10.0, 25.0],
            },
        ],
        sheet: None,
        id: "assembly".into(),
        title: "Plate assembly".into(),
        paper_size_mm: [420.0, 297.0],
        views: vec![DrawingView {
            id: "top".into(),
            outputs,
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [40.0, 60.0],
            scale: 0.5,
            show_hidden: false,
            kind: DrawingViewKind::Orthographic,
            detail: None,
            material_hatching: Default::default(),
            hatching: None,
        }],
        guides: Vec::new(),
        dimensions: Vec::new(),
        notes: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

fn member_ids(graph: &InstanceGraph<'_>) -> Vec<String> {
    let mut ids: Vec<String> = graph.patterns()[0]
        .members
        .iter()
        .map(|m| m.id.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn items_group_identical_parts_and_split_variants_and_materials() {
    let family = block_family();
    let graph = assembly(&family);
    let members = member_ids(&graph);
    let items = page(&members).parts_list_items(&graph).unwrap();
    let rows: Vec<_> = items
        .iter()
        .map(|i| (i.item, i.quantity, i.part.as_str(), i.material.as_deref()))
        .collect();
    assert_eq!(
        rows,
        [
            (1, 3, "BlockFamily / variant 1", Some("Aluminum 6061")),
            (2, 1, "BlockFamily / variant 2", Some("Aluminum 6061")),
            (3, 1, "BlockFamily / variant 1", Some("Steel 1018")),
        ]
    );
    // The suppressed member is neither shown nor counted.
    assert_eq!(
        items[0].instances,
        ["plate", members[0].as_str(), members[1].as_str()]
    );
    // Part numbers replace family IDs.
    let mut numbered = page(&members);
    numbered
        .parts_list
        .as_mut()
        .unwrap()
        .part_numbers
        .insert("BlockFamily".into(), "BLK-100".into());
    assert_eq!(
        numbered.parts_list_items(&graph).unwrap()[1].part,
        "BLK-100 / variant 2"
    );
}

#[test]
fn tables_and_balloons_render_with_exact_budgets_in_svg_and_dxf() {
    let family = block_family();
    let graph = assembly(&family);
    let session = Session::new().unwrap();
    let page = page(&member_ids(&graph));
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    // Three items: rectangle 5 + 3 row separators + 3 column separators.
    let table: usize = generated
        .sheet_lines
        .iter()
        .map(|l| l.points_mm.len())
        .sum();
    assert_eq!(table, 5 + 2 * 3 + 2 * 3);
    let cells: Vec<&str> = generated
        .sheet_labels
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    for text in [
        "ITEM",
        "QTY",
        "PART",
        "MATERIAL",
        "3",
        "Steel 1018",
        "BlockFamily / variant 2",
    ] {
        assert!(cells.contains(&text), "{text} not in {cells:?}");
    }
    let balloons: usize = generated.gdt_lines.iter().map(|l| l.points_mm.len()).sum();
    assert_eq!(balloons, 2 * 44);
    let numbers: Vec<&str> = generated
        .gdt_labels
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(numbers, ["1", "3"]);
    // The plate balloon's leader starts at the plate's placed origin.
    let origin = [40.0, 60.0];
    let dot = &generated.gdt_lines[2].points_mm;
    let center = [
        dot.iter().map(|p| p[0]).sum::<f64>() / dot.len() as f64,
        dot.iter().map(|p| p[1]).sum::<f64>() / dot.len() as f64,
    ];
    assert!(
        (center[0] - origin[0]).abs() < 0.1 && (center[1] - origin[1]).abs() < 0.1,
        "{center:?}"
    );
    let svg = generated.to_svg();
    let dxf = generated.to_dxf();
    if let Some(directory) = std::env::var_os("OCCB_PARTS_QA_DIR") {
        std::fs::write(std::path::Path::new(&directory).join("parts.svg"), &svg).unwrap();
    }
    assert!(svg.contains("Steel 1018") && dxf.contains("BlockFamily / variant 2"));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn balloons_and_part_numbers_are_validated_and_persist() {
    let family = block_family();
    let graph = assembly(&family);
    let members = member_ids(&graph);
    assert!(page(&members).validate(&graph).is_ok());
    for case in 0..6 {
        let mut page = page(&members);
        let balloon = &mut page.balloons[0];
        match case {
            0 => balloon.id = "steel".into(),
            1 => balloon.view = "missing".into(),
            2 => balloon.instance = members[2].clone(),
            3 => balloon.anchor = Some(DatumRef::new("wide", "origin")),
            4 => balloon.offset_mm = [3.0, 3.0],
            _ => {
                page.parts_list
                    .as_mut()
                    .unwrap()
                    .part_numbers
                    .insert("BlockFamily".into(), " ".into());
            }
        }
        assert!(page.validate(&graph).is_err(), "case {case}");
    }
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page(&members));
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"parts_list\"") && json.contains("\"balloons\""));
    let restored = ModelDocument::from_json(&json).unwrap();
    assert_eq!(restored, document);
    assert_eq!(restored.schema_version, CURRENT_SCHEMA_VERSION);
    let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
    legacy["schema_version"] = serde_json::json!(86);
    let drawing = legacy["drawings"][0].as_object_mut().unwrap();
    drawing.remove("parts_list");
    drawing.remove("balloons");
    let migrated = ModelDocument::from_json(&legacy.to_string()).unwrap();
    assert!(migrated.drawings[0].parts_list.is_none() && migrated.drawings[0].balloons.is_empty());
}
