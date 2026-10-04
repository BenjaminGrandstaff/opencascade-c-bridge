use super::*;

fn family() -> FamilyDefinition {
    FamilyDefinition {
        references: Vec::new(),
        id: "block".into(),
        version: 1,
        parameters: vec![ParameterDefinition {
            id: "width".into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
            minimum: None,
            maximum: None,
        }],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        requirements: vec![],
        datums: vec![],
        features: vec![
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
                        y: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
                        z: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
                    },
                },
            },
            FeatureDefinition {
                id: "placed".into(),
                operation: FeatureOperation::Translate {
                    input: "body".into(),
                    offset: VectorExpr::Literal(VectorQuantity::lengths(
                        1.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                },
            },
        ],
    }
}
fn document() -> ModelDocument {
    let family = family();
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("source", HashMap::new(), "test").unwrap();
    graph
        .add_clone("clone", "source", HashMap::new(), "test")
        .unwrap();
    graph
        .add_clone(
            "pinned",
            "clone",
            HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
            )]),
            "test",
        )
        .unwrap();
    ModelDocument::from_graph(&graph)
}

#[test]
fn inherited_parameter_edits_propagate_downstream_but_respect_pinned_values() {
    let before = document();
    let mut after = before.clone();
    if let InstanceNode::Base { overrides, .. } = after
        .instances
        .iter_mut()
        .find(|node| node.id() == "source")
        .unwrap()
    {
        overrides.insert(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(20.0, LengthUnit::Millimeter)),
        );
    }
    let impact = before.change_impact(&after).unwrap();
    assert_eq!(
        impact
            .instances
            .iter()
            .map(|value| value.instance.as_str())
            .collect::<Vec<_>>(),
        ["clone", "source"]
    );
    for value in &impact.instances {
        assert_eq!(value.parameters, ["width"]);
        assert_eq!(value.features, ["body", "placed"]);
        assert!(!value.placement_changed && !value.material_changed);
    }
    assert!(impact.assembly_needs_verification);
    assert!(!impact.patterns_need_refresh);
    assert_eq!(before.change_impact(&before).unwrap().instances.len(), 0);
    let encoded = serde_json::to_string(&impact).unwrap();
    assert_eq!(
        serde_json::from_str::<ChangeImpact>(&encoded).unwrap(),
        impact
    );
    let mut reordered = before.clone();
    reordered.instances.reverse();
    reordered.family.features.reverse();
    assert!(
        before
            .change_impact(&reordered)
            .unwrap()
            .instances
            .is_empty()
    );
}

#[test]
fn placement_material_suppression_and_drawings_do_not_rebuild_local_geometry() {
    let mut before = document();
    before.assembly.materials.push(Material {
        id: "steel".into(),
        name: "Steel".into(),
        density_kg_per_cubic_meter: 7800.0,
    });
    before
        .assembly
        .material_assignments
        .insert("source".into(), "steel".into());
    before.frames.push(AssemblyFrame {
        id: "slide".into(),
        parent: None,
        placement: Placement::default(),
        provenance: "test".into(),
    });
    for node in &mut before.instances {
        if let InstanceNode::Clone { frame, .. } = node {
            *frame = Some("slide".into());
        }
    }
    before.drawings.push(DrawingDefinition {
        id: "view".into(),
        title: "Assembly".into(),
        paper_size_mm: [297.0, 210.0],
        views: vec![DrawingView {
            id: "top".into(),
            outputs: vec![InstanceOutputRef {
                instance: "clone".into(),
                output: "body".into(),
            }],
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [0.0, 0.0],
            scale: 1.0,
            show_hidden: false,
            kind: DrawingViewKind::Orthographic,
            detail: None,
        }],
        dimensions: vec![],
        notes: vec![],
        metadata: BTreeMap::new(),
    });
    let mut after = before.clone();
    after.frames[0].placement = Placement::translated(VectorQuantity::lengths(
        5.0,
        0.0,
        0.0,
        LengthUnit::Millimeter,
    ));
    after.assembly.materials[0].density_kg_per_cubic_meter = 8000.0;
    after.assembly.configurations.push(Configuration {
        id: "hidden".into(),
        overrides: BTreeMap::new(),
        suppressed: BTreeSet::from(["pinned".into()]),
    });
    after.assembly.active_configuration = Some("hidden".into());
    let report = before.change_impact(&after).unwrap();
    assert_eq!(report.instances.len(), 3);
    assert!(
        report
            .instances
            .iter()
            .all(|impact| impact.features.is_empty() && impact.material_changed)
    );
    let clone = report
        .instances
        .iter()
        .find(|impact| impact.instance == "clone")
        .unwrap();
    assert!(clone.placement_changed);
    assert!(
        report
            .instances
            .iter()
            .find(|impact| impact.instance == "pinned")
            .unwrap()
            .suppression_changed
    );
    assert!(
        !report
            .instances
            .iter()
            .find(|impact| impact.instance == "source")
            .unwrap()
            .placement_changed
    );
    assert_eq!(report.drawings, ["view"]);
    assert!(
        before
            .change_impact(&after)
            .unwrap()
            .instances
            .iter()
            .all(|impact| impact.features.is_empty())
    );
}

#[test]
fn additions_removals_dependency_edits_and_invalid_documents_are_reported() {
    let before = document();
    let mut after = before.clone();
    after.instances.retain(|node| node.id() != "pinned");
    after.instances.push(InstanceNode::Base {
        id: "new".into(),
        family: None,
        overrides: HashMap::new(),
        placement: Placement::default(),
        frame: None,
        provenance: "test".into(),
    });
    if let FeatureOperation::Translate { offset, .. } = &mut after.family.features[1].operation {
        *offset = VectorExpr::Literal(VectorQuantity::lengths(
            2.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        ));
    }
    let impact = before.change_impact(&after).unwrap();
    assert_eq!(
        impact
            .instances
            .iter()
            .find(|value| value.instance == "new")
            .unwrap()
            .kind,
        InstanceChangeKind::Added
    );
    assert_eq!(
        impact
            .instances
            .iter()
            .find(|value| value.instance == "pinned")
            .unwrap()
            .kind,
        InstanceChangeKind::Removed
    );
    assert_eq!(
        impact
            .instances
            .iter()
            .find(|value| value.instance == "clone")
            .unwrap()
            .features,
        ["placed"]
    );
    after.family.features[1].operation = FeatureOperation::Translate {
        input: "missing".into(),
        offset: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
    };
    assert!(before.change_impact(&after).is_err());
}

#[test]
fn active_configuration_and_joint_coordinates_affect_resolved_inputs() {
    let before = document();
    let mut after = before.clone();
    after.assembly.configurations.push(Configuration {
        id: "wide".into(),
        overrides: BTreeMap::from([(
            "source".into(),
            BTreeMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(15.0, LengthUnit::Millimeter)),
            )]),
        )]),
        suppressed: BTreeSet::new(),
    });
    after.assembly.active_configuration = Some("wide".into());
    assert_eq!(before.change_impact(&after).unwrap().instances.len(), 2);
    let mut graph = before.instance_graph().unwrap();
    graph
        .add_frame("joint", None, Placement::default(), "test")
        .unwrap();
    graph.set_instance_frame("clone", Some("joint")).unwrap();
    graph
        .add_joint(AssemblyJoint {
            id: "slide".into(),
            frame: "joint".into(),
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            kind: JointKind::Prismatic {
                distance: JointScalar {
                    value: Quantity::length(0.0, LengthUnit::Millimeter),
                    minimum: None,
                    maximum: None,
                },
            },
        })
        .unwrap();
    let joint_before = ModelDocument::from_graph(&graph);
    graph
        .set_joint_coordinate(
            "joint",
            JointDof::Axial,
            Quantity::length(4.0, LengthUnit::Millimeter),
        )
        .unwrap();
    let report = joint_before
        .change_impact(&ModelDocument::from_graph(&graph))
        .unwrap();
    assert_eq!(report.instances.len(), 1);
    assert_eq!(report.instances[0].instance, "clone");
    assert!(report.instances[0].placement_changed);
    assert!(report.instances[0].features.is_empty());
}
