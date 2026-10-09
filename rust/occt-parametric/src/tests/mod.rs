use super::*;

mod assembly_motion;
mod balance;
mod branch_search;
mod closed_motion;
mod continuous_motion;
mod documents;
mod draft;
mod draw_view;
mod drawing;
mod expressions;
mod face_colors;
mod graph;
mod helix;
mod hole_catalogs;
mod holes;
mod hollow_lofts;
mod hollow_sweeps;
mod inspection;
mod linkage;
mod loft;
mod math_functions;
mod mesh;
mod mirror;
mod named_references;
mod patterns;
mod persistent;
mod primitives;
mod regeneration;
mod regions;
mod requirements;
mod revolutions;
mod ribs;
mod rotating_motion;
mod scaling;
mod selection;
mod sheet_metal;
mod step_export;
mod surface_texture;
mod sweep;
mod tangency;
mod threads;
mod traceability;
mod unify;
mod variable_fillet;

fn length_parameter(id: &str, default: f64) -> ParameterDefinition {
    ParameterDefinition {
        id: id.into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: ParameterValue::Scalar(Quantity::length(default, LengthUnit::Millimeter)),
        minimum: Some(Quantity::length(1.0, LengthUnit::Millimeter)),
        maximum: None,
    }
}

fn family(priority: RequirementPriority, maximum_volume: f64) -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "BlockFamily".into(),
        version: 1,
        parameters: vec![
            length_parameter("width", 10.0),
            length_parameter("depth", 20.0),
            length_parameter("height", 30.0),
        ],
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        features: vec![
            FeatureDefinition {
                id: "placed".into(),
                operation: FeatureOperation::Translate {
                    input: "body".into(),
                    offset: VectorExpr::Literal(VectorQuantity::lengths(
                        1.0,
                        2.0,
                        3.0,
                        LengthUnit::Centimeter,
                    )),
                },
            },
            FeatureDefinition {
                id: "body".into(),
                operation: FeatureOperation::Box {
                    origin: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                    size: VectorExpr::Components {
                        x: ScalarExpr::Parameter("width".into()),
                        y: ScalarExpr::Parameter("depth".into()),
                        z: ScalarExpr::Parameter("height".into()),
                    },
                },
            },
        ],
        datums: Vec::new(),
        requirements: vec![
            Requirement {
                id: "block.valid".into(),
                version: 1,
                kind: RequirementKind::Validation,
                priority: RequirementPriority::Required,
                statement: "The placed block must be a valid BREP.".into(),
                rule: VerificationRule::ShapeValid {
                    output: "placed".into(),
                },
                provenance: "test".into(),
                traces: Vec::new(),
            },
            Requirement {
                id: "block.volume".into(),
                version: 1,
                kind: RequirementKind::Dimensional,
                priority,
                statement: "The block volume must remain in range.".into(),
                rule: VerificationRule::VolumeRange {
                    output: "placed".into(),
                    minimum: Volume {
                        value: 5_000.0,
                        unit: LengthUnit::Millimeter,
                    },
                    maximum: Volume {
                        value: maximum_volume,
                        unit: LengthUnit::Millimeter,
                    },
                },
                provenance: "test".into(),
                traces: Vec::new(),
            },
        ],
    }
}

fn pew_row(definition: &FamilyDefinition) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_linear_pattern(
            "pews",
            "pew",
            "source",
            3,
            VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
            "pattern",
        )
        .unwrap();
    graph
}
