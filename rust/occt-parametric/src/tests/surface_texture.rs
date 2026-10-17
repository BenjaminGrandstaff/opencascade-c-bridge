use super::inspection::{attachment, nominal_record, page, plate};
use super::*;

fn texture(id: &str, anchor: &str) -> DrawingSurfaceTexture {
    DrawingSurfaceTexture {
        short_cutoff_mm: None,
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
        acceptance: TextureAcceptance::MaxRule,
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
    // With a short-wave cutoff the note shows the transmission band λs-λc.
    let mut banded = textured_page();
    banded.surface_textures[1].short_cutoff_mm = Some(0.0025);
    let generated = banded
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(
        generated
            .gdt_labels
            .iter()
            .any(|l| l.text == "0.0025-0.8  W 0.05-25")
    );
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
    let mut page = textured_page();
    page.surface_textures[1].acceptance = TextureAcceptance::SixteenPercent;
    document.drawings.push(page);
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"surface_textures\"") && json.contains("\"perpendicular\""));
    // The maximum rule is the default and is not written (schema 155).
    assert_eq!(
        json.matches("\"acceptance\": \"sixteen_percent\"").count(),
        1
    );
    assert_eq!(json.matches("\"acceptance\"").count(), 1);
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
        profiles: vec![],
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

/// Six 0.8 mm sampling lengths of a 1 µm, 80 µm-wavelength sine at 0.5 µm
/// spacing: Ra 2/π µm once the mean line is filtered out.
fn stylus_trace() -> MeasuredProfile {
    MeasuredProfile {
        raw: false,
        spacing_um: 0.5,
        heights_um: (0..9_600)
            .map(|i| {
                let x = f64::from(i) * 0.5;
                0.001 * x + (std::f64::consts::TAU * x / 80.0).sin()
            })
            .collect(),
    }
}

#[test]
fn the_sixteen_percent_rule_and_filtered_profiles_decide_textures() {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut page = textured_page();
    page.feature_control_frames = super::inspection::page().feature_control_frames;
    page.datum_features = super::inspection::page().datum_features;
    page.datum_reference_frames = super::inspection::page().datum_reference_frames;
    for texture in &mut page.surface_textures {
        texture.acceptance = TextureAcceptance::SixteenPercent;
    }
    let evaluate = |page: &DrawingDefinition, textures: Vec<MeasuredTexture>| {
        let mut record = nominal_record();
        record.surface_textures = textures;
        let report = page.evaluate_inspection(&graph, &record).unwrap();
        report
            .surface_textures
            .iter()
            .filter_map(|t| match &t.result {
                TextureResult::Evaluated(m) => Some((t.texture.clone(), m.clone())),
                TextureResult::NotMeasured => None,
            })
            .collect::<HashMap<_, _>>()
    };
    let reading = |id: &str, values: Vec<f64>, profiles| MeasuredTexture {
        id: id.into(),
        values,
        profiles,
    };
    // Ra 1.6 µm: a first reading over 70% of the limit needs two more, and
    // one of three over it needs twelve.
    let results = evaluate(
        &page,
        vec![
            reading("basic", vec![1.5], vec![]),
            reading("cast", vec![1.5, 1.7, 1.4], vec![]),
        ],
    );
    assert_eq!(
        (
            results["basic"].readings_required,
            results["basic"].conforms
        ),
        (Some(3), false)
    );
    assert_eq!(results["cast"].readings_required, Some(12));
    assert_eq!(results["cast"].above_maximum, 1);
    // A filtered stylus trace is a reading; Ra 0.64 µm accepts at once.
    let results = evaluate(&page, vec![reading("basic", vec![], vec![stylus_trace()])]);
    let basic = &results["basic"];
    assert!((basic.profile_values[0] - 2.0 / std::f64::consts::PI).abs() < 1e-3);
    assert_eq!((basic.cutoff_mm, basic.conforms), (Some(0.8), true));
    // Rz 32-63 µin: one of twelve readings below the minimum is accepted by
    // the 16% rule, but not by the maximum rule; three of twelve are not.
    let mut machined = vec![40.0, 31.0, 45.0];
    machined.extend([50.0; 9]);
    let results = evaluate(&page, vec![reading("machined", machined.clone(), vec![])]);
    assert_eq!(
        (
            results["machined"].below_minimum,
            results["machined"].conforms
        ),
        (1, true)
    );
    let mut strict = page.clone();
    strict.surface_textures[1].acceptance = TextureAcceptance::MaxRule;
    let results = evaluate(&strict, vec![reading("machined", machined.clone(), vec![])]);
    assert!(!results["machined"].conforms);
    machined[5] = 20.0;
    machined[6] = 30.0;
    let results = evaluate(&page, vec![reading("machined", machined, vec![])]);
    assert!(!results["machined"].conforms);
    assert_eq!(results["machined"].readings_required, None);
    // Profiles that cannot be filtered fail the record.
    let mut record = nominal_record();
    let mut short = stylus_trace();
    short.heights_um.truncate(100);
    record.surface_textures = vec![reading("basic", vec![], vec![short])];
    assert!(page.evaluate_inspection(&graph, &record).is_err());
}

#[test]
fn textures_on_persistent_faces_govern_them_and_land_their_leaders() {
    let definition = super::pmi::drilled_plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let on = |id: &str, anchor: &str, face: &str| {
        let mut t = texture(id, anchor);
        t.attachment.faces = Some(FaceSelector::Named(face.into()));
        t
    };
    let mut page = textured_page();
    // The bore wall nearest the right datum's origin (60, 0, 0); a top face
    // requirement repeated identically, as from a second view.
    page.surface_textures = vec![
        on("bore", "right", "first_wall"),
        on("top", "top", "top_face"),
        on("top-again", "top", "top_face"),
        texture("loose", "bottom"),
    ];
    page.surface_textures[3].attachment.offset_mm = [70.0, -20.0];
    let faces = page.surface_texture_faces(&graph, &session).unwrap();
    assert_eq!(
        faces.iter().map(|f| f.texture.as_str()).collect::<Vec<_>>(),
        ["bore", "top", "top-again"]
    );
    let bore = &faces[0];
    assert_eq!(bore.faces.len(), 1);
    assert!((bore.area_mm2 - std::f64::consts::PI * 10.0 * 10.0).abs() < 1e-3);
    let toward = [40.0, -20.0].map(|v: f64| v / 40f64.hypot(20.0));
    let expected = [20.0 + 5.0 * toward[0], 20.0 + 5.0 * toward[1], 0.0];
    for (got, want) in bore.landing_mm.iter().zip(expected) {
        assert!((got - want).abs() < 1e-6, "{:?}", bore.landing_mm);
    }
    assert_eq!(faces[1].faces, faces[2].faces);
    assert!((faces[1].area_mm2 - (60.0 * 40.0 - 2.0 * std::f64::consts::PI * 25.0)).abs() < 1e-3);
    // The leader of the bore requirement starts on the bore, not the datum.
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let view = &page.views[0];
    let [x, y, z] = bore.landing_mm;
    let landing = view
        .paper(view.project(Vec3::new(x, y, z)).unwrap())
        .unwrap();
    assert!(generated.gdt_lines.iter().any(|line| {
        line.points_mm
            .iter()
            .any(|p| (p[0] - landing[0]).hypot(p[1] - landing[1]) < 1e-9)
    }));
    // Textures are not exchanged as AP242 PMI.
    let plan = page.pmi_plan(&graph, |_| true).unwrap();
    assert_eq!(
        plan.skipped
            .iter()
            .filter(|s| s.reason.starts_with("surface texture"))
            .count(),
        4
    );
    // Different requirements may not govern one face.
    let mut conflicting = page.clone();
    conflicting.surface_textures[2].roughness.maximum = 3.2;
    let error = conflicting
        .surface_texture_faces(&graph, &session)
        .unwrap_err();
    assert!(
        error.message.contains("different requirements"),
        "{error:?}"
    );
    assert!(
        conflicting
            .generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    // Unknown face references fail validation; empty selections fail.
    let mut unknown = page.clone();
    unknown.surface_textures[0].attachment.faces = Some(FaceSelector::Named("missing".into()));
    assert!(unknown.validate(&graph).is_err());
    let mut empty = page.clone();
    empty.surface_textures[0].attachment.faces = Some(FaceSelector::NearestCenter {
        target: VectorExpr::Literal(VectorQuantity::lengths(
            500.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        maximum_distance: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
    });
    assert!(empty.surface_texture_faces(&graph, &session).is_err());
    assert_eq!(session.shape_count().unwrap(), 0);
}

fn texture_results(
    page: &DrawingDefinition,
    textures: Vec<MeasuredTexture>,
) -> HashMap<String, TextureMeasurement> {
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut record = nominal_record();
    record.surface_textures = textures;
    let mut page = page.clone();
    page.feature_control_frames = super::inspection::page().feature_control_frames;
    page.datum_features = super::inspection::page().datum_features;
    page.datum_reference_frames = super::inspection::page().datum_reference_frames;
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    report
        .surface_textures
        .iter()
        .filter_map(|t| match &t.result {
            TextureResult::Evaluated(m) => Some((t.texture.clone(), m.clone())),
            TextureResult::NotMeasured => None,
        })
        .collect()
}

fn profile_of(spacing_um: f64, n: usize, raw: bool, z: impl Fn(f64) -> f64) -> MeasuredProfile {
    MeasuredProfile {
        raw,
        spacing_um,
        heights_um: (0..n).map(|i| z(i as f64 * spacing_um)).collect(),
    }
}

#[test]
fn raw_traces_take_the_short_wave_filter_first() {
    // λc 0.25 mm and its standard λs 2.5 µm. The trace is a 1 µm, 80 µm
    // roughness sine with a 0.5 µm ripple at 1 µm wavelength, which λs
    // removes: raw, Ra is the sine's 2/π; taken as already filtered, the
    // ripple raises it.
    let mut page = textured_page();
    page.surface_textures[0].cutoff_mm = Some(0.25);
    let tau = std::f64::consts::TAU;
    let trace = |raw| {
        profile_of(0.25, 6_200, raw, |x| {
            (tau * x / 80.0).sin() + 0.5 * (tau * x / 1.0 + 0.3).sin()
        })
    };
    let reading = |raw| MeasuredTexture {
        id: "basic".into(),
        values: vec![],
        profiles: vec![trace(raw)],
    };
    let raw = &texture_results(&page, vec![reading(true)])["basic"];
    assert!(
        (raw.profile_values[0] - 2.0 / std::f64::consts::PI).abs() < 5e-3,
        "{raw:?}"
    );
    assert_eq!(raw.short_cutoff_mm, Some(0.0025));
    let primary = &texture_results(&page, vec![reading(false)])["basic"];
    assert!(
        primary.profile_values[0] > 2.0 / std::f64::consts::PI + 0.02,
        "{primary:?}"
    );
    assert_eq!(primary.short_cutoff_mm, None);
    // A raw trace sampled too coarsely for λs is refused.
    let definition = plate();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut record = nominal_record();
    record.datum_features.clear();
    record.controls.clear();
    record.surface_textures = vec![MeasuredTexture {
        id: "basic".into(),
        values: vec![],
        profiles: vec![profile_of(1.0, 1_600, true, |_| 0.0)],
    }];
    let error = page
        .evaluate_inspection(&graph, &record)
        .unwrap_err()
        .message;
    assert!(error.contains("spacing of at most 0.5 µm"), "{error}");
    // λs must be standard and at most a thirtieth of λc.
    let mut ok = textured_page();
    ok.surface_textures[1].short_cutoff_mm = Some(0.0025);
    ok.validate(&graph).unwrap();
    for (cutoff, short) in [(0.8, 0.003), (0.25, 0.025)] {
        let mut bad = textured_page();
        bad.surface_textures[1].cutoff_mm = Some(cutoff);
        bad.surface_textures[1].short_cutoff_mm = Some(short);
        assert!(bad.validate(&graph).is_err(), "{short}");
    }
}

#[test]
fn waviness_is_measured_from_the_mean_line_less_its_form() {
    // λc 0.8 mm. Roughness (1 µm at 80 µm) on a 1 µm waviness at 2 mm, on a
    // sloping form: the mean line (the Gaussian truncated at half a cutoff)
    // keeps most of the 2 µm peak-to-valley waviness and none of the
    // roughness, at a 2 mm spacing.
    let tau = std::f64::consts::TAU;
    let wavy = profile_of(0.5, 9_600, false, |x| {
        0.002 * x + (tau * x / 80.0).sin() + (tau * x / 2000.0).sin()
    });
    let mut page = textured_page();
    let basic = &mut page.surface_textures[0];
    basic.cutoff_mm = Some(0.8);
    basic.waviness = Some(Waviness {
        height_mm: 0.002,
        spacing_mm: 1.0,
    });
    let reading = MeasuredTexture {
        id: "basic".into(),
        values: vec![],
        profiles: vec![wavy],
    };
    let result = &texture_results(&page, vec![reading.clone()])["basic"];
    let waviness = result.waviness.as_ref().unwrap();
    assert!(
        (0.0017..0.0020).contains(&waviness.heights_mm[0]),
        "{waviness:?}"
    );
    let spacing = waviness.spacings_mm[0].unwrap();
    assert!((spacing - 2.0).abs() < 0.05, "{waviness:?}");
    // The 2 mm spacing exceeds the 1 mm limit, so the texture fails though
    // its roughness conforms; a 3 mm limit accepts it.
    assert!(!waviness.conforms && !result.conforms, "{result:?}");
    page.surface_textures[0]
        .waviness
        .as_mut()
        .unwrap()
        .spacing_mm = 3.0;
    let result = &texture_results(&page, vec![reading.clone()])["basic"];
    assert!(result.conforms, "{result:?}");
    // A 1.5 µm height limit rejects it.
    page.surface_textures[0]
        .waviness
        .as_mut()
        .unwrap()
        .height_mm = 0.0015;
    let result = &texture_results(&page, vec![reading])["basic"];
    assert!(!result.waviness.as_ref().unwrap().conforms);
    // Readings given as values carry no waviness.
    let values = MeasuredTexture {
        id: "basic".into(),
        values: vec![0.5],
        profiles: vec![],
    };
    assert_eq!(texture_results(&page, vec![values])["basic"].waviness, None);
}

#[test]
fn a_million_sample_raw_trace_is_filtered_and_waviness_measured_quickly() {
    // The record's largest profile: 1,000,000 raw samples at 0.5 µm (a
    // 500 mm trace), through λs, λc roughness and waviness.
    let tau = std::f64::consts::TAU;
    let trace = profile_of(0.5, 1_000_000, true, |x| {
        (tau * x / 80.0).sin() + 0.3 * (tau * x / 1.3).sin() + (tau * x / 5000.0).sin()
    });
    let mut page = textured_page();
    let basic = &mut page.surface_textures[0];
    basic.cutoff_mm = Some(0.8);
    basic.waviness = Some(Waviness {
        height_mm: 0.01,
        spacing_mm: 10.0,
    });
    let started = std::time::Instant::now();
    let results = texture_results(
        &page,
        vec![MeasuredTexture {
            id: "basic".into(),
            values: vec![],
            profiles: vec![trace],
        }],
    );
    let elapsed = started.elapsed().as_secs_f64();
    let basic = &results["basic"];
    let spacing = basic.waviness.as_ref().unwrap().spacings_mm[0].unwrap();
    assert!((spacing - 5.0).abs() < 0.05, "{basic:?}");
    assert!(elapsed < 2.0, "{elapsed} s");
}
