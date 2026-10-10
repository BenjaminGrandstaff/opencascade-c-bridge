use super::*;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

mod build;
mod edge_treatment_views;
mod helix_views;
mod inspection;
mod patching;
mod profile_views;
mod transform_views;
mod views;

fn mm(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn size(x: f64, y: f64) -> VectorExpr {
    VectorExpr::Components {
        x: mm(x),
        y: mm(y),
        z: ScalarExpr::Parameter("thickness".into()),
    }
}
fn fixture() -> ModelDocument {
    let mut family = FamilyDefinition {
        id: "bracket".into(),
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
        features: vec![],
    };
    for (id, default) in [("thickness", 4.0), ("hole_spacing", 30.0)] {
        family.parameters.push(ParameterDefinition {
            id: id.into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(default, LengthUnit::Millimeter)),
            minimum: None,
            maximum: None,
        });
    }
    family.features = vec![
        FeatureDefinition {
            id: "base".into(),
            operation: FeatureOperation::Box {
                origin: point(0.0, 0.0, 0.0),
                size: size(60.0, 30.0),
            },
        },
        FeatureDefinition {
            id: "upright".into(),
            operation: FeatureOperation::Box {
                origin: point(0.0, 0.0, 0.0),
                size: VectorExpr::Components {
                    x: mm(60.0),
                    y: ScalarExpr::Parameter("thickness".into()),
                    z: mm(25.0),
                },
            },
        },
        FeatureDefinition {
            id: "blank".into(),
            operation: FeatureOperation::Fuse {
                left: "base".into(),
                right: "upright".into(),
            },
        },
    ];
    for (id, input, x) in [
        ("first_hole", "blank", mm(10.0)),
        (
            "body",
            "first_hole",
            ScalarExpr::Add(
                Box::new(mm(10.0)),
                Box::new(ScalarExpr::Parameter("hole_spacing".into())),
            ),
        ),
    ] {
        family.features.push(FeatureDefinition {
            id: id.into(),
            operation: FeatureOperation::Hole {
                bottom: HoleBottom::Flat,
                input: input.into(),
                position: VectorExpr::Components {
                    x,
                    y: mm(18.0),
                    z: mm(0.0),
                },
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                diameter: mm(5.0),
                extent: HoleExtent::ThroughAll,
                finish: HoleFinish::Plain,
                thread: None,
            },
        });
    }
    for (id, rule) in [
        (
            "valid",
            VerificationRule::ShapeValid {
                output: "body".into(),
            },
        ),
        (
            "wall",
            VerificationRule::MinimumWall {
                output: "body".into(),
                minimum: Quantity::length(2.0, LengthUnit::Millimeter),
                mesh: Default::default(),
                maximum_samples: 1000,
            },
        ),
    ] {
        family.requirements.push(Requirement {
            id: id.into(),
            version: 1,
            kind: RequirementKind::Validation,
            priority: RequirementPriority::Required,
            statement: id.into(),
            rule,
            provenance: "example; assumed 2 mm minimum for workflow demonstration".into(),
            traces: vec![TraceTarget::Feature("body".into())],
        });
    }
    let mut graph = InstanceGraph::new(&family);
    graph
        .add_base("bracket", HashMap::new(), "example")
        .unwrap();
    ModelDocument::from_graph(&graph)
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "occb-model-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn request(model: &ModelDocument, edits: Value) -> Value {
    json!({"schema":"occb-model-request-v1","model":model,"outputs":[{"instance":"bracket","output":"body"}],"edits":edits,"step":false,"stl":false,"preview":false})
}
fn invoke(dir: &Directory, value: Value, name: &str) -> Result<Value, Failure> {
    let p = dir.0.join("request.json");
    fs::write(&p, value.to_string()).unwrap();
    run(&[p.into_os_string(), dir.0.join(name).into_os_string()])
}
fn edit(parameter: &str, v: f64) -> Value {
    json!([{"instance":"bracket","parameter":parameter,"value":ParameterValue::Scalar(Quantity::length(v,LengthUnit::Millimeter))}])
}

fn view_request(dir: &Directory, value: Value, name: &str) -> Result<Value, Failure> {
    let source = dir.0.join("view.request.json");
    fs::write(&source, value.to_string()).unwrap();
    visualize::run(&source.into_os_string(), &dir.0.join(name).into_os_string())
}

fn view_example(name: &str) -> Value {
    serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../tools/model/{name}.request.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
