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
        annotation_views: Vec::new(),
        dataset: None,
        approval_workflow: None,
        datum_reference_frames: Vec::new(),
        datum_features: Vec::new(),
        feature_control_frames: Vec::new(),
        surface_textures: Vec::new(),
        parts_list: Some(DrawingPartsList {
            position_mm: [250.0, 280.0],
            part_numbers: BTreeMap::new(),
            nested: false,
            merge_identical: false,
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
        releases: Vec::new(),
        revision_table: None,
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
            thread_representation: false,
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
    // A table that would run off the paper is refused when drawing.
    let mut low = page.clone();
    low.parts_list.as_mut().unwrap().position_mm = [250.0, 20.0];
    let error = low
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap_err();
    assert!(error.message.contains("fit on the paper"), "{error}");
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

#[test]
fn nested_lists_follow_sub_assembly_frames_with_hierarchical_numbers() {
    let family = block_family();
    let mut graph = assembly(&family);
    graph
        .add_frame("left", None, Placement::identity(), "test")
        .unwrap();
    graph
        .add_frame("inner", Some("left"), Placement::identity(), "test")
        .unwrap();
    graph.set_instance_frame("plate", Some("left")).unwrap();
    graph.set_instance_frame("wide", Some("inner")).unwrap();
    let members = member_ids(&graph);
    let mut page = page(&members);
    let list = page.parts_list.as_mut().unwrap();
    list.nested = true;
    list.part_numbers.insert("left".into(), "ASM-10".into());
    let items = page.parts_list_items(&graph).unwrap();
    let rows: Vec<_> = items
        .iter()
        .map(|i| {
            (
                i.label.as_str(),
                i.depth,
                i.quantity,
                i.part.as_str(),
                i.frame.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("1", 0, 1, "ASM-10", Some("left")),
            ("1.1", 1, 1, "BlockFamily / variant 1", None),
            ("1.2", 1, 1, "inner", Some("inner")),
            ("1.2.1", 2, 1, "BlockFamily / variant 2", None),
            // The pattern copies sit at the top level, apart from the plate.
            ("2", 0, 2, "BlockFamily / variant 1", None),
            ("3", 0, 1, "BlockFamily / variant 1", None),
        ]
    );
    assert_eq!(
        items.iter().map(|i| i.item).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    let session = Session::new().unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    // Balloons show hierarchical numbers; parts are indented under their frames.
    let numbers: Vec<&str> = generated
        .gdt_labels
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(numbers, ["1.1", "3"]);
    let cells: Vec<&str> = generated
        .sheet_labels
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    assert!(cells.contains(&"1.2.1"), "{cells:?}");
    if let Some(directory) = std::env::var_os("OCCB_PARTS_QA_DIR") {
        std::fs::write(
            std::path::Path::new(&directory).join("nested.svg"),
            generated.to_svg(),
        )
        .unwrap();
    }
    // The depth-2 part is indented 6 mm in the PART column (x 250 + 24 + 1.5).
    let deep = generated
        .sheet_labels
        .iter()
        .filter(|l| l.text == "BlockFamily / variant 2")
        .map(|l| l.position_mm[0])
        .next()
        .unwrap();
    assert!((deep - (250.0 + 24.0 + 1.5 + 6.0)).abs() < 1e-9, "{deep}");
    let table: usize = generated
        .sheet_lines
        .iter()
        .map(|l| l.points_mm.len())
        .sum();
    assert_eq!(table, 5 + 2 * 6 + 2 * 3);
    // Flat lists ignore frames and keep plain numbers.
    page.parts_list.as_mut().unwrap().nested = false;
    let flat = page.parts_list_items(&graph).unwrap();
    assert_eq!(
        flat.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
        ["1", "2", "3"]
    );
    let mut document = ModelDocument::from_graph(&graph);
    let mut nested_page = page.clone();
    nested_page.parts_list.as_mut().unwrap().nested = true;
    document.drawings.push(nested_page);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"nested\": true"));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    assert_eq!(session.shape_count().unwrap(), 0);
}

/// Wheels 1 and 2 each hold a plate and a hub frame with a wide block;
/// wheel 3 holds only a plate.
fn wheels(family: &FamilyDefinition) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(family);
    let wide = HashMap::from([(
        "width".into(),
        ParameterValue::Scalar(Quantity::length(25.0, LengthUnit::Millimeter)),
    )]);
    for n in 1..=3 {
        graph
            .add_frame(format!("wheel-{n}"), None, Placement::identity(), "test")
            .unwrap();
        graph
            .add_base(format!("plate-{n}"), HashMap::new(), "test")
            .unwrap();
        graph
            .set_instance_frame(&format!("plate-{n}"), Some(&format!("wheel-{n}")))
            .unwrap();
        if n < 3 {
            graph
                .add_frame(
                    format!("hub-{n}"),
                    Some(&format!("wheel-{n}")),
                    Placement::identity(),
                    "test",
                )
                .unwrap();
            graph
                .add_base(format!("block-{n}"), wide.clone(), "test")
                .unwrap();
            graph
                .set_instance_frame(&format!("block-{n}"), Some(&format!("hub-{n}")))
                .unwrap();
        }
    }
    graph
}

#[test]
fn identical_sub_assemblies_merge_into_one_row_with_their_count() {
    let family = block_family();
    let graph = wheels(&family);
    let mut page = page(&[]);
    // Copy 2 lists its hub before its plate; contents compare in any order.
    page.views[0].outputs = ["plate-1", "block-1", "block-2", "plate-2", "plate-3"]
        .map(output)
        .to_vec();
    page.balloons = vec![DrawingBalloon {
        id: "copy".into(),
        view: "top".into(),
        instance: "block-2".into(),
        anchor: None,
        offset_mm: [10.0, 25.0],
    }];
    let list = page.parts_list.as_mut().unwrap();
    list.nested = true;
    list.merge_identical = true;
    let rows = |page: &DrawingDefinition| {
        page.parts_list_items(&graph)
            .unwrap()
            .into_iter()
            .map(|i| (i.label, i.quantity, i.part, i.merged_frames, i.instances))
            .collect::<Vec<_>>()
    };
    let strings = |values: &[&str]| values.iter().map(|v| (*v).to_owned()).collect::<Vec<_>>();
    let row = |label: &str, quantity, part: &str, merged: &[&str], instances: &[&str]| {
        (
            label.to_owned(),
            quantity,
            part.to_owned(),
            strings(merged),
            strings(instances),
        )
    };
    assert_eq!(
        rows(&page),
        [
            row("1", 2, "wheel-1", &["wheel-2"], &[]),
            // Parts under a merged sub-assembly count one copy but collect
            // every copy's instances.
            row(
                "1.1",
                1,
                "BlockFamily / variant 1",
                &[],
                &["plate-1", "plate-2"]
            ),
            row("1.2", 1, "hub-1", &[], &[]),
            row(
                "1.2.1",
                1,
                "BlockFamily / variant 2",
                &[],
                &["block-1", "block-2"]
            ),
            row("2", 1, "wheel-3", &[], &[]),
            row("2.1", 1, "BlockFamily / variant 1", &[], &["plate-3"]),
        ]
    );
    // A balloon on the second copy shows the merged row's number.
    let session = Session::new().unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let labels: Vec<&str> = generated
        .gdt_labels
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(labels, ["1.2.1"]);
    // Equal part numbers merge; different ones are different parts.
    let numbers = &mut page.parts_list.as_mut().unwrap().part_numbers;
    numbers.insert("wheel-1".into(), "WHL-1".into());
    numbers.insert("wheel-2".into(), "WHL-1".into());
    assert_eq!(rows(&page)[0].0, "1");
    assert_eq!((rows(&page)[0].1, rows(&page)[0].2.as_str()), (2, "WHL-1"));
    let mut distinct = page.clone();
    distinct
        .parts_list
        .as_mut()
        .unwrap()
        .part_numbers
        .insert("wheel-2".into(), "WHL-2".into());
    assert_eq!(rows(&distinct).len(), 10);
    // One part number on sub-assemblies with different contents fails.
    let mut clash = page.clone();
    clash
        .parts_list
        .as_mut()
        .unwrap()
        .part_numbers
        .insert("wheel-3".into(), "WHL-1".into());
    assert!(clash.parts_list_items(&graph).is_err());
    assert!(clash.validate(&graph).is_err());
    let mut flat = page.clone();
    flat.parts_list.as_mut().unwrap().nested = false;
    assert!(flat.validate(&graph).is_err());
    // Without merging, each sub-assembly keeps its own row.
    let mut separate = page.clone();
    separate.parts_list.as_mut().unwrap().merge_identical = false;
    assert_eq!(rows(&separate).len(), 10);
    // The option persists (schema 157).
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"merge_identical\": true"));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
    drop(graph);
    assert_eq!(session.shape_count().unwrap(), 0);
}
