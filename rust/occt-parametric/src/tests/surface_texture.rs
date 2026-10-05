use super::inspection::{attachment, nominal_record, page, plate};
use super::*;

fn texture(id: &str, anchor: &str) -> DrawingSurfaceTexture {
    DrawingSurfaceTexture {
        id: id.into(),
        attachment: attachment(anchor),
        unit: RoughnessUnit::Micrometer,
        roughness: RoughnessLimits {
            parameter: RoughnessParameter::Ra,
            maximum: 1.6,
            minimum: None,
        },
        cutoff_mm: None,
        waviness: None,
        lay: None,
        material_removal: MaterialRemoval::Any,
        method: None,
        all_around: false,
    }
}

/// A basic symbol, a fully extended machined one and a no-removal one.
fn textured_page() -> DrawingDefinition {
    let mut page = page();
    page.datum_features.clear();
    page.datum_reference_frames.clear();
    page.feature_control_frames.clear();
    let mut machined = texture("machined", "top");
    machined.unit = RoughnessUnit::Microinch;
    machined.roughness = RoughnessLimits {
        parameter: RoughnessParameter::Rz,
        maximum: 63.0,
        minimum: Some(32.0),
    };
    machined.cutoff_mm = Some(0.8);
    machined.waviness = Some(Waviness {
        height_mm: 0.05,
        spacing_mm: 25.0,
    });
    machined.lay = Some(SurfaceLay::Perpendicular);
    machined.material_removal = MaterialRemoval::Required;
    machined.method = Some("GRIND".into());
    machined.all_around = true;
    let mut cast = texture("cast", "right");
    cast.material_removal = MaterialRemoval::Prohibited;
    cast.lay = Some(SurfaceLay::Crossed);
    cast.attachment.offset_mm = [60.0, 45.0];
    let mut basic = texture("basic", "bottom");
    basic.attachment.offset_mm = [70.0, -20.0];
    page.surface_textures = vec![basic, machined, cast];
    page
}

#[test]
fn texture_symbols_render_values_notes_and_lay_in_svg_and_dxf() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let generated = textured_page()
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let labels: Vec<&str> = generated
        .gdt_labels
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    for expected in [
        "Ra 1.6 µm",
        "Rz 32-63 µin",
        "GRIND",
        "Lc 0.8  W 0.05-25",
        "X",
    ] {
        assert!(labels.contains(&expected), "{expected} not in {labels:?}");
    }
    // Basic 8 vertices; machined 8 + bar 2 + extension 2 + all-around 33 +
    // drawn ⊥ 4; cast 8 + circle 33 + extension 2.
    let vertices: usize = generated.gdt_lines.iter().map(|l| l.points_mm.len()).sum();
    assert_eq!(vertices, 8 + 49 + 43);
    let svg = generated.to_svg();
    if let Some(directory) = std::env::var_os("OCCB_TEXTURE_QA_DIR") {
        std::fs::write(std::path::Path::new(&directory).join("textures.svg"), &svg).unwrap();
    }
    assert!(svg.contains("Rz 32-63 µin") && svg.contains("GRIND"));
    let dxf = generated.to_dxf();
    assert!(dxf.contains("Ra 1.6 µm") && dxf.contains("Lc 0.8"));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn texture_values_cutoffs_methods_and_attachments_are_validated() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    assert!(textured_page().validate(&graph).is_ok());
    for case in 0..8 {
        let mut page = textured_page();
        let t = &mut page.surface_textures[1];
        match case {
            0 => t.id = "basic".into(),
            1 => t.cutoff_mm = Some(0.5),
            2 => t.roughness.minimum = Some(63.0),
            3 => t.roughness.maximum = -1.0,
            4 => t.method = Some("GRÌND".into()),
            5 => t.waviness.as_mut().unwrap().spacing_mm = f64::NAN,
            6 => t.attachment.view = "missing".into(),
            _ => t.attachment.offset_mm = [1.0, 1.0],
        }
        assert!(page.validate(&graph).is_err(), "case {case}");
    }
}

#[test]
fn textures_persist_in_schema_71_and_legacy_drawings_load_without_them() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(textured_page());
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"surface_textures\"") && json.contains("\"perpendicular\""));
    let restored = ModelDocument::from_json(&json).unwrap();
    assert_eq!(restored, document);
    assert_eq!(restored.schema_version, CURRENT_SCHEMA_VERSION);
    let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
    legacy["schema_version"] = serde_json::json!(67);
    legacy["drawings"][0]
        .as_object_mut()
        .unwrap()
        .remove("surface_textures");
    let migrated = ModelDocument::from_json(&legacy.to_string()).unwrap();
    assert!(migrated.drawings[0].surface_textures.is_empty());
}

#[test]
fn roughness_readings_conform_by_the_maximum_rule() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = textured_page();
    page.feature_control_frames = super::inspection::page().feature_control_frames;
    page.datum_features = super::inspection::page().datum_features;
    page.datum_reference_frames = super::inspection::page().datum_reference_frames;
    let reading = |id: &str, values: Vec<f64>| MeasuredTexture {
        id: id.into(),
        values,
    };
    let mut record = nominal_record();
    record.surface_textures = vec![
        reading("basic", vec![0.9, 1.6, 1.2]),
        reading("machined", vec![40.0, 31.0]),
    ];
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    let result = |id: &str| {
        report
            .surface_textures
            .iter()
            .find(|t| t.texture == id)
            .unwrap()
            .result
            .clone()
    };
    // The maximum is inclusive; a reading below the minimum fails.
    let TextureResult::Evaluated(basic) = result("basic") else {
        panic!()
    };
    assert_eq!(
        (basic.readings, basic.highest, basic.conforms),
        (3, 1.6, true)
    );
    let TextureResult::Evaluated(machined) = result("machined") else {
        panic!()
    };
    assert_eq!((machined.lowest, machined.conforms), (31.0, false));
    assert_eq!(result("cast"), TextureResult::NotMeasured);
    assert!(!report.conforms());
    for bad in [
        vec![reading("missing", vec![1.0])],
        vec![reading("basic", vec![])],
        vec![reading("basic", vec![-0.1])],
        vec![reading("basic", vec![1.0]), reading("basic", vec![1.0])],
    ] {
        record.surface_textures = bad;
        assert!(page.evaluate_inspection(&graph, &record).is_err());
    }
}
