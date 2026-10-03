use super::*;

fn merged(base: &ModelDocument, left: &ModelDocument, right: &ModelDocument) -> ModelDocument {
    match base.three_way_merge(left, right).unwrap() {
        DocumentMerge::Merged(document) => *document,
        DocumentMerge::Conflicts(conflicts) => panic!("unexpected conflicts: {conflicts:?}"),
    }
}

fn conflicts(
    base: &ModelDocument,
    left: &ModelDocument,
    right: &ModelDocument,
) -> Vec<DocumentConflict> {
    match base.three_way_merge(left, right).unwrap() {
        DocumentMerge::Conflicts(conflicts) => conflicts,
        DocumentMerge::Merged(_) => panic!("expected conflicts"),
    }
}

fn provenance(document: &mut ModelDocument, index: usize, text: &str) {
    match &mut document.instances[index] {
        InstanceNode::Base { provenance, .. } | InstanceNode::Clone { provenance, .. } => {
            *provenance = text.into()
        }
    }
}

#[test]
fn independent_fields_merge_without_mutation_and_round_trip() {
    let base = document();
    let mut left = base.clone();
    let mut right = base.clone();
    left.family.parameters[0].default = ParameterValue::Boolean(true);
    provenance(&mut right, 0, "edited");
    right.instances.reverse();
    let originals = (base.clone(), left.clone(), right.clone());
    let result = merged(&base, &left, &right);
    assert_eq!(
        result
            .family
            .parameters
            .iter()
            .find(|parameter| parameter.id == "width/with~punctuation")
            .unwrap()
            .default,
        ParameterValue::Boolean(true)
    );
    assert_eq!(result.instances[0], right.instances[1]);
    assert_eq!(
        ModelDocument::from_json(&result.to_json_pretty().unwrap()).unwrap(),
        result
    );
    assert_eq!((base.clone(), left.clone(), right.clone()), originals);
    assert!(
        result
            .semantic_diff(&merged(&base, &right, &left))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn identical_edits_and_reordering_coalesce() {
    let base = document();
    let mut left = base.clone();
    provenance(&mut left, 0, "edited");
    let mut right = left.clone();
    right.instances.reverse();
    right.family.parameters.reverse();
    assert!(
        left.semantic_diff(&merged(&base, &left, &right))
            .unwrap()
            .is_empty()
    );
    assert!(
        left.semantic_diff(&merged(&base, &left, &base))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn incompatible_field_edits_return_typed_conflicts() {
    let base = document();
    let mut left = base.clone();
    let mut right = base.clone();
    provenance(&mut left, 0, "left");
    provenance(&mut right, 0, "right");
    let conflict = conflicts(&base, &left, &right);
    assert_eq!(
        conflict,
        vec![DocumentConflict {
            path: vec![
                field("instances"),
                DocumentPathSegment::Entity("one".into()),
                field("base"),
                field("provenance")
            ],
            base: Some(json!("test")),
            left: Some(json!("left")),
            right: Some(json!("right")),
        }]
    );
    let swapped = conflicts(&base, &right, &left);
    assert_eq!(swapped[0].left, conflict[0].right);
    assert_eq!(swapped[0].right, conflict[0].left);
}

#[test]
fn deletion_against_edit_conflicts_at_entity_and_serializes_absence() {
    let base = document();
    let mut left = base.clone();
    let mut right = base.clone();
    left.instances.remove(0);
    provenance(&mut right, 0, "edited");
    let conflict = conflicts(&base, &left, &right);
    assert_eq!(conflict.len(), 1);
    assert_eq!(
        conflict[0].path,
        vec![
            field("instances"),
            DocumentPathSegment::Entity("one".into())
        ]
    );
    assert!(conflict[0].left.is_none());
    let json = serde_json::to_string(&conflict).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<DocumentConflict>>(&json).unwrap(),
        conflict
    );
    assert_eq!(merged(&base, &left, &left).instances.len(), 1);
}

#[test]
fn distinct_additions_combine_and_same_id_additions_conflict() {
    let base = document();
    let mut left = base.clone();
    let mut right = base.clone();
    let mut new = base.instances[0].clone();
    if let InstanceNode::Base { id, .. } = &mut new {
        *id = "three".into();
    }
    left.instances.push(new.clone());
    if let InstanceNode::Base { id, .. } = &mut new {
        *id = "four".into();
    }
    right.instances.push(new);
    assert_eq!(merged(&base, &left, &right).instances.len(), 4);
    right = left.clone();
    assert_eq!(merged(&base, &left, &right).instances.len(), 3);
    provenance(&mut right, 2, "different");
    let conflict = conflicts(&base, &left, &right);
    assert_eq!(conflict.len(), 1);
    assert!(conflict[0].base.is_none());
    assert_eq!(
        conflict[0].path.last(),
        Some(&DocumentPathSegment::Entity("three".into()))
    );
}

#[test]
fn map_entries_merge_independently_in_existing_and_omitted_maps() {
    let mut base = document();
    base.assembly.materials.push(Material {
        id: "steel".into(),
        name: "Steel".into(),
        density_kg_per_cubic_meter: 7850.0,
    });
    let mut left = base.clone();
    let mut right = base.clone();
    if let InstanceNode::Base { overrides, .. } = &mut left.instances[0] {
        overrides.insert("enabled".into(), ParameterValue::Boolean(true));
    }
    if let InstanceNode::Base { overrides, .. } = &mut right.instances[0] {
        overrides.insert(
            "width/with~punctuation".into(),
            ParameterValue::Boolean(true),
        );
    }
    left.assembly
        .material_assignments
        .insert("one".into(), "steel".into());
    right
        .assembly
        .material_assignments
        .insert("two".into(), "steel".into());
    let result = merged(&base, &left, &right);
    assert_eq!(result.instances[0].overrides().len(), 2);
    assert_eq!(result.assembly.material_assignments.len(), 2);
}

#[test]
fn variant_replacement_against_variant_edit_conflicts_atomically() {
    let base = document();
    let mut left = base.clone();
    let mut right = base.clone();
    left.instances[0] = InstanceNode::Clone {
        id: "one".into(),
        source: "two".into(),
        overrides: HashMap::new(),
        placement: Placement::default(),
        frame: None,
        provenance: "test".into(),
    };
    provenance(&mut right, 0, "edited");
    assert_eq!(
        conflicts(&base, &left, &right)[0].path,
        vec![
            field("instances"),
            DocumentPathSegment::Entity("one".into())
        ]
    );
    left = base.clone();
    left.family.parameters[0].parameter_type = ParameterType::Choice(vec!["a".into(), "b".into()]);
    left.family.parameters[0].default = ParameterValue::Choice("a".into());
    right = base.clone();
    right.family.parameters[0].default = ParameterValue::Boolean(true);
    let conflict = conflicts(&base, &left, &right);
    assert_eq!(conflict.len(), 1);
    assert_eq!(conflict[0].path.last(), Some(&field("default")));
}

#[test]
fn ordered_arrays_conflict_as_whole_values() {
    let base = document();
    let mut left = base.clone();
    let mut right = base.clone();
    let record = |revision| GenerationRecord {
        instance_id: "one".into(),
        attempted_revision: revision,
        accepted_revision: Some(1),
        state: RegenerationState::Stale,
        last_error: None,
    };
    left.generation_records.push(record(2));
    right.generation_records.push(record(3));
    let conflict = conflicts(&base, &left, &right);
    assert_eq!(conflict.len(), 1);
    assert_eq!(conflict[0].path, vec![field("generation_records")]);
}

#[test]
fn individually_valid_edits_with_combined_dangling_reference_fail_validation() {
    let base = document();
    let mut left = base.clone();
    left.instances.remove(0);
    let mut right = base.clone();
    right.instances.push(InstanceNode::Clone {
        id: "dependent".into(),
        source: "one".into(),
        overrides: HashMap::new(),
        placement: Placement::default(),
        frame: None,
        provenance: "test".into(),
    });
    let error = base.three_way_merge(&left, &right).unwrap_err();
    assert!(error.message.starts_with("merged document:"));
    assert!(error.message.contains("one"));
}

#[test]
fn combined_parameter_bounds_are_validated() {
    let mut base = document();
    base.family.parameters[0] = ParameterDefinition {
        id: "number".into(),
        parameter_type: ParameterType::Scalar(Dimension::Scalar),
        default: ParameterValue::Scalar(Quantity::scalar(0.0)),
        minimum: Some(Quantity::scalar(-10.0)),
        maximum: Some(Quantity::scalar(10.0)),
    };
    let mut left = base.clone();
    let mut right = base.clone();
    left.family.parameters[0].default = ParameterValue::Scalar(Quantity::scalar(6.0));
    right.family.parameters[0].maximum = Some(Quantity::scalar(5.0));
    assert!(
        base.three_way_merge(&left, &right)
            .unwrap_err()
            .message
            .starts_with("merged document:")
    );
}

#[test]
fn invalid_inputs_are_identified_before_merge() {
    let base = document();
    let mut left = base.clone();
    left.instances.push(left.instances[0].clone());
    assert!(
        base.three_way_merge(&left, &base)
            .unwrap_err()
            .message
            .starts_with("merge left:")
    );
    let mut old = base.clone();
    old.schema_version = 1;
    assert!(
        base.three_way_merge(&base, &old)
            .unwrap_err()
            .message
            .starts_with("merge right:")
    );
}

#[test]
fn conflict_serialization_preserves_null_and_absence() {
    let conflict = DocumentConflict {
        path: vec![field("nullable")],
        base: Some(Value::Null),
        left: None,
        right: Some(json!(3)),
    };
    let encoded = serde_json::to_string(&conflict).unwrap();
    assert_eq!(
        serde_json::from_str::<DocumentConflict>(&encoded).unwrap(),
        conflict
    );
}

#[test]
fn deleting_last_map_entry_combines_with_another_entry_added() {
    let mut base = document();
    base.assembly.materials.push(Material {
        id: "steel".into(),
        name: "Steel".into(),
        density_kg_per_cubic_meter: 7850.0,
    });
    base.assembly
        .material_assignments
        .insert("one".into(), "steel".into());
    let mut left = base.clone();
    left.assembly.material_assignments.clear();
    let mut right = base.clone();
    right
        .assembly
        .material_assignments
        .insert("two".into(), "steel".into());
    let result = merged(&base, &left, &right);
    assert_eq!(
        result.assembly.material_assignments,
        std::collections::BTreeMap::from([("two".into(), "steel".into())])
    );
}

#[test]
fn additional_family_collections_are_restored_and_sorted() {
    let mut base = document();
    let mut additional = base.family.clone();
    additional.id = "extra".into();
    base.additional_families.push(additional);
    let mut left = base.clone();
    let mut right = base.clone();
    left.additional_families[0].parameters[0].default = ParameterValue::Boolean(true);
    right.additional_families[0].parameters[1].default = ParameterValue::Boolean(true);
    let result = merged(&base, &left, &right);
    assert!(
        result.additional_families[0]
            .parameters
            .iter()
            .all(|parameter| parameter.default == ParameterValue::Boolean(true))
    );
    assert!(ModelDocument::from_json(&result.to_json_pretty().unwrap()).is_ok());
}

#[test]
fn merged_geometry_regenerates_with_both_dimension_edits_and_bounded_handles() {
    let mut base = document();
    base.family.parameters = ["width", "height"]
        .into_iter()
        .map(|id| ParameterDefinition {
            id: id.into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
            minimum: None,
            maximum: None,
        })
        .collect();
    let zero = || ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter));
    base.family.features.push(FeatureDefinition {
        id: "body".into(),
        operation: FeatureOperation::Box {
            origin: VectorExpr::Components {
                x: zero(),
                y: zero(),
                z: zero(),
            },
            size: VectorExpr::Components {
                x: ScalarExpr::Parameter("width".into()),
                y: ScalarExpr::Parameter("height".into()),
                z: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
            },
        },
    });
    let mut left = base.clone();
    let mut right = base.clone();
    left.family.parameters[0].default =
        ParameterValue::Scalar(Quantity::length(20.0, LengthUnit::Millimeter));
    right.family.parameters[1].default =
        ParameterValue::Scalar(Quantity::length(30.0, LengthUnit::Millimeter));
    let result = merged(&base, &left, &right);
    let session = occt_bridge::Session::new().unwrap();
    let starting = session.shape_count().unwrap();
    for _ in 0..3 {
        let graph = result.instance_graph().unwrap();
        let generated = graph.resolve("one").unwrap().regenerate(&session).unwrap();
        let body = generated.shape("body").unwrap();
        assert!((session.volume(body).unwrap() - 3000.0).abs() < 1e-8);
        assert!(session.is_valid(body).unwrap());
        drop(generated);
        assert_eq!(session.shape_count().unwrap(), starting);
    }
}
