use super::*;
use occt_parametric::*;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

fn fixture() -> ModelDocument {
    let family = FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "assembly".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        requirements: vec![],
        datums: vec![],
        features: vec![FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                size: VectorExpr::Literal(VectorQuantity::lengths(
                    10.0,
                    10.0,
                    10.0,
                    LengthUnit::Millimeter,
                )),
            },
        }],
    };
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("prototype", HashMap::new(), "test").unwrap();
    graph
        .add_clone("posed", "prototype", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "posed",
            Placement::translated(VectorQuantity::lengths(
                10.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    ModelDocument::from_graph(&graph)
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "occb-balance-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn inputs(directory: &Directory) -> (Vec<OsString>, Value) {
    let model = directory.0.join("model.json");
    let setup = directory.0.join("setup.json");
    let reference = ChordReference {
        leading_edge: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        direction: VectorQuantity::scalars(1.0, 0.0, 0.0),
        length: Quantity::length(100.0, LengthUnit::Millimeter),
    };
    let settings = json!({
        "schema":"occb-balance-report-v1",
        "materials":[{"id":"foam","name":"Assumed test material","density_kg_per_cubic_meter":1000}],
        "components":[
            {"id":"first","source":"prototype","output":"body","material":"foam"},
            {"id":"second","source":"posed","output":"body","material":"foam"}
        ],
        "reference":{"kind":"chord","chord":reference}
    });
    fs::write(&model, fixture().to_json_pretty().unwrap()).unwrap();
    fs::write(&setup, settings.to_string()).unwrap();
    (
        vec![
            model.into_os_string(),
            setup.into_os_string(),
            directory.0.join("report.json").into_os_string(),
        ],
        settings,
    )
}

#[test]
fn command_reports_selected_mass_material_totals_and_cg_without_changing_inputs() {
    let directory = Directory::new();
    let (args, _) = inputs(&directory);
    let before = fs::read(&args[0]).unwrap();
    run(&args).unwrap();
    let report: Value = serde_json::from_str(&fs::read_to_string(&args[2]).unwrap()).unwrap();
    assert_eq!(report["generated_variants"], 1);
    assert_eq!(report["components"].as_array().unwrap().len(), 2);
    assert_eq!(report["materials"][0]["components"], 2);
    assert!((report["total"]["mass_kg"].as_f64().unwrap() - 0.002).abs() < 1e-12);
    assert!((report["balance"]["percent_chord"].as_f64().unwrap() - 10.0).abs() < 1e-9);
    assert_eq!(report["mean_aerodynamic_chord"], Value::Null);
    assert_eq!(fs::read(&args[0]).unwrap(), before);
    assert!(
        run(&args)
            .unwrap_err()
            .to_string()
            .contains("already exists")
    );
    assert!(run(&[]).unwrap_err().to_string().contains("usage"));
}

#[test]
fn wing_project_uses_relative_path_and_matches_exact_rectangular_mac() {
    let directory = Directory::new();
    let (args, mut settings) = inputs(&directory);
    settings["reference"] = json!({"kind":"wing_project","project":"wing.json"});
    fs::write(&args[1], settings.to_string()).unwrap();
    fs::write(
        directory.0.join("wing.json"),
        json!({
            "schema":"occb-wing-layout-v1","spanMm":1000,
            "stations":[
                {"fraction":0,"chordMm":100,"leadingEdgeMm":0,"heightMm":0},
                {"fraction":1,"chordMm":100,"leadingEdgeMm":0,"heightMm":0}
            ]
        })
        .to_string(),
    )
    .unwrap();
    run(&args).unwrap();
    let report: Value = serde_json::from_str(&fs::read_to_string(&args[2]).unwrap()).unwrap();
    assert_eq!(report["mean_aerodynamic_chord"]["length_mm"], 100.0);
    assert_eq!(
        report["mean_aerodynamic_chord"]["planform_area_mm2"],
        100000.0
    );
    assert_eq!(report["balance"]["percent_chord"], 10.0);
}

#[test]
fn missing_materials_invalid_selections_and_conflicting_densities_publish_no_report() {
    let directory = Directory::new();
    let (args, original) = inputs(&directory);
    let mut cases = vec![];
    let mut add = |edit: fn(&mut Value)| {
        let mut value = original.clone();
        edit(&mut value);
        cases.push(value);
    };
    add(|s| s["schema"] = json!("future"));
    add(|s| s["components"] = json!([]));
    add(|s| s["components"][0]["source"] = json!("missing"));
    add(|s| s["components"][0]["output"] = json!("missing"));
    add(|s| s["components"][0]["material"] = Value::Null);
    add(|s| s["components"][1]["id"] = json!("first"));
    add(|s| s["components"][0]["material"] = json!("missing"));
    add(|s| s["materials"][0]["density_kg_per_cubic_meter"] = json!(-1));
    for setup in cases {
        fs::write(&args[1], setup.to_string()).unwrap();
        assert!(run(&args).is_err(), "{setup}");
        assert!(!Path::new(&args[2]).exists());
    }
    let document = fixture();
    let mut graph = document.instance_graph().unwrap();
    graph
        .add_material(Material {
            id: "foam".into(),
            name: "Original".into(),
            density_kg_per_cubic_meter: 20.0,
        })
        .unwrap();
    let document = ModelDocument::from_graph(&graph);
    let setup: config::Setup = serde_json::from_value(original).unwrap();
    assert!(
        config::prepare(&document, &setup)
            .err()
            .unwrap()
            .to_string()
            .contains("conflicts")
    );
}

#[test]
fn inherited_materials_and_matching_definitions_are_retained() {
    let document = fixture();
    let mut graph = document.instance_graph().unwrap();
    let material = Material {
        id: "foam".into(),
        name: "Foam".into(),
        density_kg_per_cubic_meter: 1000.0,
    };
    graph.add_material(material.clone()).unwrap();
    graph.assign_material("prototype", Some("foam")).unwrap();
    let document = ModelDocument::from_graph(&graph);
    let directory = Directory::new();
    let (_, mut settings) = inputs(&directory);
    settings["materials"] = json!([material]);
    settings["components"][0]["material"] = Value::Null;
    settings["components"][1]["material"] = Value::Null;
    let setup: config::Setup = serde_json::from_value(settings).unwrap();
    let (graph, outputs) = config::prepare(&document, &setup).unwrap();
    let session = Session::new().unwrap();
    let report = graph.mass_properties(&session, &outputs).unwrap();
    assert!((report.total.mass_kg - 0.002).abs() < 1e-12);
    assert_eq!(session.shape_count().unwrap(), 0);
}
