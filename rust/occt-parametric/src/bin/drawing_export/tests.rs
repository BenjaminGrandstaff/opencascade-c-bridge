use super::*;
use occt_parametric::*;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

fn fixture() -> (ModelDocument, DrawingDefinition) {
    let family = FamilyDefinition {
        references: Vec::new(),
        id: "part".into(),
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
                    20.0,
                    30.0,
                    LengthUnit::Millimeter,
                )),
            },
        }],
    };
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let drawing = DrawingDefinition {
        datum_reference_frames: Vec::new(),
        datum_features: Vec::new(),
        feature_control_frames: Vec::new(),
        sheet: None,
        id: "../../unsafe-filename".into(),
        title: "Section template".into(),
        paper_size_mm: [100.0, 100.0],
        views: vec![DrawingView {
            id: "profile".into(),
            outputs: vec![InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            }],
            origin: VectorQuantity::lengths(0.0, 0.0, 15.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [20.0, 40.0],
            scale: 1.0,
            show_hidden: false,
            kind: DrawingViewKind::Slice,
            detail: None,
            hatching: None,
        }],
        guides: Vec::new(),
        dimensions: vec![],
        notes: vec![],
        metadata: BTreeMap::new(),
    };
    (ModelDocument::from_graph(&graph), drawing)
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "occb-drawing-export-test-{}-{}",
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
fn inputs(
    directory: &Directory,
    document: &ModelDocument,
    drawings: &[DrawingDefinition],
) -> Vec<OsString> {
    let model = directory.0.join("source.json");
    let setup = directory.0.join("setup.json");
    fs::write(&model, document.to_json_pretty().unwrap()).unwrap();
    fs::write(&setup,json!({"schema":"occb-drawing-export-v1","drawings":drawings,"options":{"curve_samples":8,"maximum_vertices":1000}}).to_string()).unwrap();
    vec![
        model.into_os_string(),
        setup.into_os_string(),
        directory.0.join("output").into_os_string(),
    ]
}

#[test]
fn command_exports_numbered_files_and_reloadable_drawings_without_overwriting_input() {
    let (document, drawing) = fixture();
    let directory = Directory::new();
    let args = inputs(&directory, &document, &[drawing]);
    let before = fs::read(&args[0]).unwrap();
    run(&args).unwrap();
    let output = Path::new(&args[2]);
    assert!(
        fs::read_to_string(output.join("0001.dxf"))
            .unwrap()
            .contains("$INSUNITS")
    );
    assert!(
        fs::read_to_string(output.join("0001.svg"))
            .unwrap()
            .contains("width=\"100mm\"")
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["drawings"][0]["polylines"], 4);
    assert_eq!(manifest["drawings"][0]["empty"], false);
    let loaded =
        ModelDocument::from_json(&fs::read_to_string(output.join("drawings.model.json")).unwrap())
            .unwrap();
    assert_eq!(loaded.drawings.len(), 1);
    assert_eq!(loaded.family, document.family);
    assert_eq!(fs::read(&args[0]).unwrap(), before);
    assert!(run(&args).is_err());
    assert!(run(&[]).is_err());
}

#[test]
fn stored_drawings_are_used_and_conflicting_definitions_or_budget_failures_publish_nothing() {
    let (mut document, drawing) = fixture();
    document.drawings.push(drawing.clone());
    let directory = Directory::new();
    let args = inputs(&directory, &document, &[]);
    run(&args).unwrap();
    let mut changed = drawing.clone();
    changed.title = "Different".into();
    let directory = Directory::new();
    let args = inputs(&directory, &document, &[changed]);
    assert!(run(&args).unwrap_err().to_string().contains("conflicts"));
    assert!(!Path::new(&args[2]).exists());
    let directory = Directory::new();
    let args = inputs(&directory, &document, &[drawing]);
    fs::write(
        &args[1],
        json!({"schema":"occb-drawing-export-v1","options":{"maximum_vertices":10}}).to_string(),
    )
    .unwrap();
    assert!(run(&args).is_err());
    assert!(!Path::new(&args[2]).exists());
    fs::write(&args[1], json!({"schema":"future"}).to_string()).unwrap();
    assert!(run(&args).is_err());
    let (document, _) = fixture();
    let directory = Directory::new();
    let args = inputs(&directory, &document, &[]);
    assert!(run(&args).is_err());
}
