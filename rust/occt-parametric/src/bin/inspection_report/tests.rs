use super::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn point(x: f64, y: f64, z: f64) -> VectorQuantity {
    VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter)
}
fn fixture() -> ModelDocument {
    let family = FamilyDefinition {
        references: Vec::new(),
        id: "part".into(),
        version: 1,
        parameters: Vec::new(),
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        requirements: Vec::new(),
        datums: [
            ("origin", point(0.0, 0.0, 0.0)),
            ("corner", point(10.0, 20.0, 0.0)),
        ]
        .into_iter()
        .map(|(id, origin)| DatumDefinition {
            id: id.into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(origin),
            },
        })
        .collect(),
        features: vec![FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: VectorExpr::Literal(point(0.0, 0.0, 0.0)),
                size: VectorExpr::Literal(point(10.0, 20.0, 30.0)),
            },
        }],
    };
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let mut document = ModelDocument::from_graph(&graph);
    let output = InstanceOutputRef {
        instance: "part".into(),
        output: "body".into(),
    };
    let mut page = DrawingDefinition {
        id: "page".into(),
        title: "Inspection fixture".into(),
        paper_size_mm: [297.0, 210.0],
        views: vec![DrawingView {
            id: "top".into(),
            outputs: vec![output.clone()],
            origin: point(0.0, 0.0, 0.0),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [30.0, 50.0],
            scale: 1.0,
            show_hidden: false,
            kind: DrawingViewKind::Orthographic,
            detail: None,
            hatching: None,
        }],
        dimensions: Vec::new(),
        notes: Vec::new(),
        guides: Vec::new(),
        sheet: None,
        metadata: BTreeMap::new(),
        datum_features: Vec::new(),
        datum_reference_frames: Vec::new(),
        feature_control_frames: Vec::new(),
    };
    for (id, tolerance) in [
        (
            "width",
            DimensionTolerance::Symmetric {
                deviation: mm(0.125),
            },
        ),
        ("basic", DimensionTolerance::Basic),
        ("reference", DimensionTolerance::Reference),
        ("none", DimensionTolerance::None),
    ] {
        page.dimensions.push(DrawingDimension {
            id: id.into(),
            view: "top".into(),
            first: DatumRef::new("part", "origin"),
            second: DatumRef::new("part", "corner"),
            direction: DimensionDirection::Horizontal,
            offset_mm: 10.0,
            precision: 3,
            presentation: DimensionPresentation {
                tolerance,
                ..Default::default()
            },
        });
    }
    let attachment = DrawingGdtAttachment {
        view: "top".into(),
        output,
        anchor: DatumRef::new("part", "corner"),
        offset_mm: [30.0, 30.0],
    };
    for id in ["A", "B", "C"] {
        page.datum_features.push(DrawingDatumFeature {
            id: id.into(),
            label: id.into(),
            feature_of_size: false,
            attachment: attachment.clone(),
        });
    }
    page.datum_reference_frames
        .push(DrawingDatumReferenceFrame {
            id: "ABC".into(),
            datums: ["A", "B", "C"]
                .into_iter()
                .map(|id| DrawingDatumReference {
                    datum_feature: id.into(),
                    boundary: DatumMaterialBoundary::Regardless,
                })
                .collect(),
        });
    page.feature_control_frames
        .push(DrawingFeatureControlFrame {
            id: "position".into(),
            attachment,
            characteristic: GeometricCharacteristic::Position,
            tolerance: mm(0.1),
            display_unit: LengthUnit::Millimeter,
            precision: 2,
            zone: GeometricToleranceZone::Diameter,
            material: ToleranceMaterialCondition::Maximum,
            feature_of_size: true,
            datums: Vec::new(),
            refinement: None,
            datum_reference_frame: Some("ABC".into()),
            size_limits: Some(DrawingSizeLimits {
                kind: FeatureOfSizeKind::Internal,
                lower: mm(10.0),
                upper: mm(12.0),
            }),
        });
    document.drawings.push(page);
    document
}
fn position(size: f64, radial: f64) -> Value {
    json!({"control":"position","size":mm(size),"nominal_axis":{"origin":point(0.0,0.0,0.0),"direction":VectorQuantity::scalars(0.0,0.0,1.0)},"samples":[point(radial,0.0,5.0)]})
}
fn setup() -> Value {
    json!({"schema":"occb-inspection-setup-v1","drawings":[{"drawing":"page", "dimensions":[
        {"dimension":"width","value":mm(10.0)}, {"dimension":"width","value":mm(10.25)}, {"dimension":"width","value":mm(9.75)},
        {"dimension":"basic","value":mm(20.0)}, {"dimension":"reference","value":mm(20.0)}, {"dimension":"none","value":mm(20.0)}
    ],"positions":[position(10.0,0.03125),position(10.0,0.0625),position(10.5,0.25)]}]})
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "occb-inspection-test-{}-{}",
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
fn inputs(directory: &Directory, document: &ModelDocument, setup: &Value) -> Vec<OsString> {
    let model = directory.0.join("model.json");
    let source = directory.0.join("measurements.json");
    fs::write(&model, document.to_json_pretty().unwrap()).unwrap();
    fs::write(&source, setup.to_string()).unwrap();
    vec![
        model.into_os_string(),
        source.into_os_string(),
        directory.0.join("report.json").into_os_string(),
    ]
}

#[test]
fn report_preserves_order_units_and_uncontrolled_states_with_violations() {
    let document = fixture();
    let directory = Directory::new();
    let args = inputs(&directory, &document, &setup());

    if let Some(path) = std::env::var_os("OCCB_INSPECTION_QA_DIR") {
        let path = Path::new(&path);
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("model.json"), document.to_json_pretty().unwrap()).unwrap();
        fs::write(path.join("violations.json"), setup().to_string()).unwrap();
        fs::write(path.join("within.json"), json!({"schema":"occb-inspection-setup-v1","drawings":[{"drawing":"page","dimensions":[{"dimension":"width","value":mm(10.0)}]}]}).to_string()).unwrap();
    }
    let before = fs::read(&args[0]).unwrap();
    let source = fs::read(&args[1]).unwrap();
    assert!(run(&args).unwrap());
    let result: Value = serde_json::from_str(&fs::read_to_string(&args[2]).unwrap()).unwrap();
    assert_eq!(result["schema"], "occb-inspection-report-v1");
    assert_eq!(result["model_schema_version"], CURRENT_SCHEMA_VERSION);
    assert_eq!(
        result["summary"],
        json!({"dimensions_within_limits":1,"dimensions_outside_limits":2,"dimensions_without_limits":3,"positions_within_zone":2,"positions_outside_zone":1})
    );
    let dimensions = &result["drawings"][0]["dimensions"];
    assert_eq!(dimensions[1]["evaluation"]["deviation"], json!(mm(0.25)));
    assert_eq!(
        dimensions[1]["evaluation"]["limits"]["margin"],
        json!(mm(-0.125))
    );
    assert_eq!(
        dimensions[2]["evaluation"]["disposition"],
        "below_lower_limit"
    );
    assert_eq!(
        dimensions[3]["evaluation"]["disposition"],
        "basic_dimension"
    );
    assert_eq!(
        dimensions[4]["evaluation"]["disposition"],
        "reference_dimension"
    );
    assert_eq!(
        dimensions[5]["evaluation"]["disposition"],
        "no_specified_tolerance"
    );
    assert!(dimensions[3]["evaluation"]["limits"].is_null());
    assert_eq!(
        result["drawings"][0]["positions"][2]["evaluation"]["allowance"]["bonus_mm"],
        0.5
    );
    assert_eq!(fs::read(&args[0]).unwrap(), before);
    assert_eq!(fs::read(&args[1]).unwrap(), source);
    assert!(run(&args).is_err());
    assert!(run(&[]).is_err());
}
#[test]
fn successful_or_informational_measurements_produce_reports_without_failure() {
    let document = fixture();
    for group in [
        json!({"drawing":"page","dimensions":[{"dimension":"width","value":mm(10.0)}]}),
        json!({"drawing":"page","positions":[position(10.5,0.25)]}),
        json!({"drawing":"page","dimensions":[{"dimension":"basic","value":mm(100.0)}]}),
    ] {
        let directory = Directory::new();
        let args = inputs(
            &directory,
            &document,
            &json!({"schema":"occb-inspection-setup-v1","drawings":[group]}),
        );
        assert!(!run(&args).unwrap());
        assert!(Path::new(&args[2]).exists());
    }
}
#[test]
fn invalid_setup_and_measurements_publish_no_report() {
    let document = fixture();
    for case in 0..12 {
        let mut data = setup();
        match case {
            0 => data["schema"] = json!("future"),
            1 => data["drawings"] = json!([]),
            2 => data["drawings"][0]["drawing"] = json!("missing"),
            3 => {
                let copy = data["drawings"][0].clone();
                data["drawings"].as_array_mut().unwrap().push(copy);
            }
            4 => data["drawings"][0] = json!({"drawing":"page"}),
            5 => data["drawings"][0]["dimensions"][0]["dimension"] = json!("missing"),
            6 => data["drawings"][0]["dimensions"][0]["value"] = json!(Quantity::scalar(10.0)),
            7 => data["drawings"][0]["positions"][0]["control"] = json!("missing"),
            8 => data["drawings"][0]["positions"][0]["samples"] = json!([]),
            9 => data["drawings"][0]["positions"][0]["size"] = json!(mm(9.0)),
            10 => data["unexpected"] = json!(true),
            _ => data["drawings"][0]["positions"][0]["nominal_axis"]["unexpected"] = json!(true),
        }
        let directory = Directory::new();
        let args = inputs(&directory, &document, &data);
        assert!(run(&args).is_err(), "case {case}");
        assert!(!Path::new(&args[2]).exists());
    }
}
#[test]
fn positional_batches_resolve_named_frames_once_and_serialize_results() {
    let document = fixture();
    let graph = document.instance_graph().unwrap();
    let measurements: Vec<DrawingPositionMeasurement> =
        serde_json::from_value(json!([position(10.0, 0.0625), position(10.5, 0.25)])).unwrap();
    let results = document.drawings[0]
        .evaluate_position_measurements(&graph, &measurements)
        .unwrap();
    assert!(!results[0].evaluation.samples_within_zone);
    assert!(results[1].evaluation.samples_within_zone);
    assert_eq!(
        serde_json::from_str::<Vec<DrawingPositionMeasurementResult>>(
            &serde_json::to_string(&results).unwrap()
        )
        .unwrap(),
        results
    );
    assert!(
        document.drawings[0]
            .evaluate_position_measurements(&graph, &[])
            .unwrap()
            .is_empty()
    );
    let mut page = document.drawings[0].clone();
    page.datum_reference_frames[0].datums[1].boundary = DatumMaterialBoundary::Maximum;
    assert!(
        page.evaluate_position_measurements(&graph, &measurements)
            .is_err()
    );
}
