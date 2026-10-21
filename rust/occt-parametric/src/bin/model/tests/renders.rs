//! Shaded PNG renders of build outputs: views, sections, limits and edits.

use super::*;
use crate::render::png::tests::decode;

/// Pixels of `file` in the build `name`, as (width, height, rgb).
fn image(dir: &Directory, name: &str, file: &str) -> (usize, usize, Vec<u8>) {
    decode(&fs::read(dir.0.join(name).join(file)).unwrap())
}

fn pixel(rgb: &[u8], width: usize, x: usize, y: usize) -> [u8; 3] {
    let at = (y * width + x) * 3;
    [rgb[at], rgb[at + 1], rgb[at + 2]]
}

/// Share of pixels in the hatched section-cap colors.
fn cap_share(rgb: &[u8]) -> f64 {
    let caps = rgb
        .chunks(3)
        // The axis triad's red has far less green than the cap hatching.
        .filter(|c| c[0] > 150 && c[1] > 60 && i32::from(c[0]) > i32::from(c[2]) + 60)
        .count();
    caps as f64 / (rgb.len() / 3) as f64
}

#[test]
fn renders_write_png_views_and_hatched_sections() {
    let dir = Directory::new();
    let model = fixture();
    let mut value = request(&model, json!([]));
    value["renders"] = json!({});
    let built = invoke(&dir, value.clone(), "views").unwrap();
    let renders = built["outputs"][0]["renders"].as_array().unwrap();
    let views: Vec<_> = renders
        .iter()
        .map(|r| r["view"].as_str().unwrap())
        .collect();
    assert_eq!(views, ["iso", "front", "top", "right"]);
    assert_eq!(built["render_quality"], "tessellated; visual review only");
    let background = [244, 246, 248];
    for entry in renders {
        let (w, h, rgb) = image(&dir, "views", entry["file"].as_str().unwrap());
        assert_eq!((w, h), (512, 512));
        // The part is framed in the middle with background at the corners.
        let part = rgb.chunks(3).filter(|c| c != &background).count();
        assert!(part * 10 > w * h, "{entry}");
        assert_eq!(pixel(&rgb, w, 4, 4), background);
        assert_eq!(pixel(&rgb, w, 507, 4), background);
        assert_eq!(cap_share(&rgb), 0.0);
    }
    // The same request renders the same bytes.
    invoke(&dir, value, "again").unwrap();
    assert_eq!(
        fs::read(dir.0.join("views/0001-iso.png")).unwrap(),
        fs::read(dir.0.join("again/0001-iso.png")).unwrap()
    );
    // A section across the base plate shows its cut face, hatched; one
    // past the part removes everything.
    let bounds = &built["outputs"][0]["bounds_mm"];
    let middle: Vec<f64> = (0..3)
        .map(|k| (bounds["min"][k].as_f64().unwrap() + bounds["max"][k].as_f64().unwrap()) / 2.0)
        .collect();
    let mut value = request(&model, json!([]));
    value["renders"] = json!({"views": [], "size_px": 200, "sections": [
        {"id": "across", "origin_mm": middle, "normal": [1, 0, 0]},
        {"id": "all", "origin_mm": [-1000, 0, 0], "normal": [1, 0, 0]},
    ]});
    let cut = invoke(&dir, value, "sections").unwrap();
    let renders = &cut["outputs"][0]["renders"];
    assert_eq!(renders[0]["file"], "0001-section-across.png");
    let (w, _, rgb) = image(&dir, "sections", "0001-section-across.png");
    assert_eq!(w, 200);
    assert!(cap_share(&rgb) > 0.02, "{}", cap_share(&rgb));
    assert_eq!(renders[1]["empty"], true);
    assert!(renders[1].get("file").is_none());
    assert!(!dir.0.join("sections/0001-section-all.png").exists());
}

#[test]
fn render_options_are_validated_before_building() {
    let dir = Directory::new();
    let model = fixture();
    for (renders, expected) in [
        (json!({"views": [], "sections": []}), "at least one view"),
        (json!({"views": ["top", "top"]}), "repeated"),
        (json!({"size_px": 32}), "64 to 1024"),
        (
            json!({"views": ["iso", "front", "back", "top", "bottom", "left", "right", "iso", "top"]}),
            "at most 8 views",
        ),
        (
            json!({"sections": [{"id": "a b", "origin_mm": [0, 0, 0], "normal": [0, 0, 1]}]}),
            "letters, digits",
        ),
        (
            json!({"sections": [{"id": "z", "origin_mm": [0, 0, 0], "normal": [0, 0, 0]}]}),
            "nonzero normal",
        ),
        (
            json!({"sections": [
            {"id": "z", "origin_mm": [0, 0, 0], "normal": [0, 0, 1]},
            {"id": "z", "origin_mm": [0, 0, 1], "normal": [0, 0, 1]}]}),
            "repeated",
        ),
    ] {
        let mut value = request(&model, json!([]));
        value["renders"] = renders;
        let error = invoke(&dir, value, "rejected").unwrap_err();
        assert_eq!(error.stage, "request");
        assert!(error.message.contains(expected), "{}", error.message);
        assert!(!dir.0.join("rejected").exists());
    }
    // Unknown views fail decoding.
    let mut value = request(&model, json!([]));
    value["renders"] = json!({"views": ["side"]});
    assert_eq!(
        invoke(&dir, value, "rejected").unwrap_err().stage,
        "request"
    );
    // The image total is bounded across outputs.
    let options: render::RenderOptions = serde_json::from_value(json!({
        "views": ["iso", "front", "back", "top", "bottom", "left", "right"],
        "sections": [{"id": "z", "origin_mm": [0, 0, 0], "normal": [0, 0, 1]}]
    }))
    .unwrap();
    assert!(options.validate(32).is_ok());
    assert!(
        options
            .validate(33)
            .unwrap_err()
            .contains("256 images per build")
    );
    // Without `renders`, reports and directories are unchanged.
    let plain = invoke(&dir, request(&model, json!([])), "plain").unwrap();
    assert!(plain["outputs"][0].get("renders").is_none());
    assert!(plain.get("render_quality").is_none());
}

#[test]
fn flat_pattern_renders_show_dashed_bend_lines() {
    let dir = Directory::new();
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../../../../tools/model/hemmed-tray.request.json"
    ))
    .unwrap();
    value["step"] = json!(false);
    value["preview"] = json!(false);
    value["renders"] = json!({"views": ["top", "iso"], "size_px": 256});
    let built = invoke(&dir, value, "tray").unwrap();
    let outputs = built["outputs"].as_array().unwrap();
    // The folded part has no bend-line overlay; the flat blank lists its
    // lines (two bends, three edge-tab bends, the closed hem; both faces).
    assert!(outputs[0]["renders"][0].get("bend_lines").is_none());
    let flat = &outputs[1]["renders"];
    assert_eq!(flat[0]["bend_lines"], json!(10));
    // Bend-line blue, outside the lower-left corner's axis triad.
    let blue = |file: &str| {
        let (w, h, rgb) = image(&dir, "tray", file);
        rgb.chunks(3)
            .enumerate()
            .filter(|(i, _)| !(i % w < w / 4 && i / w > h * 3 / 4))
            .filter(|(_, c)| c[2] > 150 && c[0] < 110 && i32::from(c[2]) > i32::from(c[1]) + 60)
            .count()
    };
    let (top, iso) = (blue("0002-top.png"), blue("0002-iso.png"));
    assert!(top > 150, "{top} bend pixels in the top view");
    assert!(iso > 50, "{iso} bend pixels in the iso view");
    // The folded part's renders carry none.
    assert_eq!(blue("0001-top.png"), 0);
}

/// Two 20 mm cubes, the second placed 15 mm along x: they share a
/// 5 × 20 × 20 mm block. A third cube 100 mm away touches nothing.
fn clashing_cubes() -> ModelDocument {
    let family = FamilyDefinition {
        history: Default::default(),
        id: "cube".into(),
        version: 1,
        references: vec![],
        feature_colors: Default::default(),
        assumptions: vec![],
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        requirements: vec![],
        datums: vec![],
        features: vec![FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: point(0.0, 0.0, 0.0),
                size: VectorExpr::Literal(VectorQuantity::lengths(
                    20.0,
                    20.0,
                    20.0,
                    LengthUnit::Millimeter,
                )),
            },
        }],
    };
    let family: &'static FamilyDefinition = Box::leak(Box::new(family));
    let mut graph = InstanceGraph::new(family);
    for (id, x) in [("a", 0.0), ("b", 15.0), ("far", 100.0)] {
        graph.add_base(id, HashMap::new(), "test").unwrap();
        graph
            .set_placement(
                id,
                Placement {
                    translation: VectorQuantity::lengths(x, 0.0, 0.0, LengthUnit::Millimeter),
                    rotation: None,
                },
            )
            .unwrap();
    }
    ModelDocument::from_graph(&graph)
}

#[test]
fn scenes_render_outputs_together_and_show_interference() {
    let dir = Directory::new();
    let model = clashing_cubes();
    let mut value = request(&model, json!([]));
    value["outputs"] = json!([
        {"instance": "a", "output": "body"},
        {"instance": "b", "output": "body"},
        {"instance": "far", "output": "body"},
    ]);
    value["renders"] = json!({"views": ["top"], "size_px": 200, "scene": true,
        "sections": [{"id": "mid", "origin_mm": [0, 10, 0], "normal": [0, 1, 0]}]});
    let built = invoke(&dir, value, "scene").unwrap();
    let scene = &built["scene"];
    assert_eq!(
        scene["parts"],
        json!([
            {"output": "a/body", "color": "#96accc"},
            {"output": "b/body", "color": "#94c496"},
            {"output": "far/body", "color": "#d6be78"},
        ])
    );
    let clashes = scene["interferences"].as_array().unwrap();
    assert_eq!(clashes.len(), 1, "{clashes:?}");
    assert_eq!(
        (&clashes[0]["first"], &clashes[0]["second"]),
        (&json!("a/body"), &json!("b/body"))
    );
    assert!((clashes[0]["volume_mm3"].as_f64().unwrap() - 2000.0).abs() < 1e-6);
    assert_eq!(clashes[0]["bounds_mm"]["min"][0], 15.0);
    let files: Vec<_> = scene["renders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap())
        .collect();
    assert_eq!(files, ["scene-top.png", "scene-section-mid.png"]);
    // The overlap shows red in the top view, between the two parts' colors.
    let red = |file: &str| {
        let (w, h, rgb) = image(&dir, "scene", file);
        rgb.chunks(3)
            .enumerate()
            .filter(|(i, _)| !(i % w < w / 4 && i / w > h * 3 / 4))
            .filter(|(_, c)| c[0] > 160 && c[1] < 120 && c[2] < 120)
            .count()
    };
    assert!(red("scene-top.png") > 50);
    assert!(red("scene-section-mid.png") > 50);
    // Each output's own renders are unchanged and show no clash.
    assert_eq!(built["outputs"][0]["renders"][0]["file"], "0001-top.png");
    assert_eq!(red("0001-top.png"), 0);
    // Scenes count toward the image limit.
    let options: render::RenderOptions =
        serde_json::from_value(json!({"views": ["iso", "front", "back", "top", "bottom", "left", "right"], "sections": [{"id": "z", "origin_mm": [0, 0, 0], "normal": [0, 0, 1]}], "scene": true})).unwrap();
    assert!(options.validate(31).is_ok());
    assert!(options.validate(32).is_err());
    assert!(
        serde_json::from_value::<render::RenderOptions>(json!({"scene": true}))
            .unwrap()
            .validate(65)
            .unwrap_err()
            .contains("at most 64")
    );
}
