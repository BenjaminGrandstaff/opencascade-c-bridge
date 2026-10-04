//! Kernel-free impact of edits on resolved instances and feature dependencies.
use crate::*;
use std::{
    collections::{BTreeSet, VecDeque},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceChangeKind {
    Added,
    Removed,
    Updated,
}

/// Features include direct changes and their downstream dependents. Placement,
/// material, and suppression changes do not require rebuilding local features.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceImpact {
    pub instance: String,
    pub kind: InstanceChangeKind,
    pub parameters: Vec<String>,
    pub features: Vec<String>,
    pub placement_changed: bool,
    pub material_changed: bool,
    pub suppression_changed: bool,
    pub intent_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeImpact {
    pub instances: Vec<InstanceImpact>,
    pub drawings: Vec<String>,
    #[serde(default)]
    pub mesh_exports: Vec<String>,
    /// Conservative flags: regeneration may refresh driven patterns or solve
    /// relationships differently. This report does not predict those results.
    pub patterns_need_refresh: bool,
    pub assembly_needs_verification: bool,
}

struct Variant {
    family: String,
    parameters: BTreeMap<String, ParameterValue>,
    signatures: BTreeMap<String, Vec<u8>>,
    downstream: HashMap<String, Vec<String>>,
    definition: Vec<u8>,
}
struct Snapshot {
    variant: Rc<Variant>,
    placement: Placement,
    frames: Rc<Vec<Placement>>,
    material: Option<Vec<u8>>,
    suppressed: bool,
    intent: Vec<u8>,
}

impl ModelDocument {
    /// Compare validated intent at current stored poses without generating
    /// geometry. Clone/configuration overrides and derived values are resolved;
    /// feature signatures use the same inputs as incremental regeneration.
    /// Added/removed features and the union of old/new dependency edges are
    /// propagated once per distinct pair of parameter variants.
    ///
    /// Shared variants, clone materials, and enclosing frame chains are cached.
    /// Cost is document validation, serialization/resolution, one feature walk
    /// per distinct variant pair, and emitted report size. Shared instances do
    /// not repeatedly traverse the whole feature graph or clone ancestry.
    /// Exact serialized placements can flag equivalent representations as
    /// changed. Pending pattern refresh/relationship solves are conservative
    /// flags, not predictions of future geometry or continuous motion.
    pub fn change_impact(&self, after: &Self) -> Result<ChangeImpact, ModelError> {
        let before_graph = self.instance_graph()?;
        let after_graph = after.instance_graph()?;
        let before = snapshots(&before_graph)?;
        let next = snapshots(&after_graph)?;
        let ids: BTreeSet<_> = before.keys().chain(next.keys()).collect();
        let mut instances = Vec::new();
        let mut pairs = HashMap::new();
        for id in ids {
            let impact = compare_instance(id, before.get(id), next.get(id), &mut pairs);
            if let Some(impact) = impact {
                instances.push(impact);
            }
        }
        let affected: HashSet<_> = instances
            .iter()
            .map(|impact| impact.instance.as_str())
            .collect();
        let drawings = drawing_impact(self, after, &affected);
        let mesh_exports = mesh_impact(self, after, &affected);
        let relevant = instances.iter().any(|impact| {
            impact.kind != InstanceChangeKind::Updated
                || !impact.parameters.is_empty()
                || !impact.features.is_empty()
                || impact.placement_changed
                || impact.material_changed
                || impact.suppression_changed
        });
        Ok(ChangeImpact {
            drawings,
            mesh_exports,
            instances,
            patterns_need_refresh: self.patterns != after.patterns
                || (relevant && (!self.patterns.is_empty() || !after.patterns.is_empty())),
            assembly_needs_verification: relevant
                || family_requirements_changed(self, after)
                || self.assembly.requirements != after.assembly.requirements
                || self.assembly.relationships != after.assembly.relationships,
        })
    }
}

fn family_requirements_changed(before: &ModelDocument, after: &ModelDocument) -> bool {
    // Family rules may change even when no local feature needs rebuilding.
    before.family.requirements != after.family.requirements
        || before
            .additional_families
            .iter()
            .map(|family| (&family.id, &family.requirements))
            .collect::<BTreeMap<_, _>>()
            != after
                .additional_families
                .iter()
                .map(|family| (&family.id, &family.requirements))
                .collect::<BTreeMap<_, _>>()
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>, ModelError> {
    let value = serde_json::to_value(value)
        .map_err(|error| ModelError::new(format!("impact signature: {error}")))?;
    serde_json::to_vec(&value)
        .map_err(|error| ModelError::new(format!("impact signature: {error}")))
}

fn snapshots(graph: &InstanceGraph<'_>) -> Result<BTreeMap<String, Snapshot>, ModelError> {
    let mut resolutions = HashMap::new();
    let mut variants = HashMap::new();
    let mut frames = HashMap::new();
    let mut inherited_materials = HashMap::new();
    let materials: HashMap<_, _> = graph
        .assembly
        .materials
        .iter()
        .map(|material| {
            Ok((
                material.id.as_str(),
                encode(&(
                    material,
                    graph.assembly.material_appearances.get(&material.id),
                ))?,
            ))
        })
        .collect::<Result<_, ModelError>>()?;
    let mut suppressed: HashSet<_> = graph
        .patterns
        .iter()
        .flat_map(|pattern| &pattern.members)
        .filter(|member| member.suppressed)
        .map(|member| member.id.as_str())
        .collect();
    if let Some(configuration) = graph.assembly.active_configuration.as_ref().and_then(|id| {
        graph
            .assembly
            .configurations
            .iter()
            .find(|configuration| &configuration.id == id)
    }) {
        suppressed.extend(configuration.suppressed.iter().map(String::as_str));
    }
    let mut result = BTreeMap::new();
    for (id, node) in &graph.nodes {
        let instance = graph.resolve_cached(id, &mut resolutions)?;
        let overrides: BTreeMap<_, _> = instance.overrides.iter().collect();
        let key = encode(&(instance.definition.id.as_str(), overrides))?;
        let variant = match variants.get(&key) {
            Some(variant) => Rc::clone(variant),
            None => {
                let variant = Rc::new(variant(&instance)?);
                variants.insert(key, Rc::clone(&variant));
                variant
            }
        };
        let frame = node.frame();
        let chain = match frames.get(&frame) {
            Some(chain) => Rc::clone(chain),
            None => {
                let chain = Rc::new(graph.frame_chain(frame)?);
                frames.insert(frame, Rc::clone(&chain));
                chain
            }
        };
        let material_id = inherited_material(id, graph, &mut inherited_materials);
        let material = material_id
            .as_deref()
            .and_then(|id| materials.get(id))
            .cloned();
        result.insert(
            id.clone(),
            Snapshot {
                variant,
                placement: node.placement(),
                frames: chain,
                material,
                suppressed: suppressed.contains(id.as_str()),
                intent: encode(node)?,
            },
        );
    }
    Ok(result)
}

pub(crate) fn inherited_material(
    id: &str,
    graph: &InstanceGraph<'_>,
    cache: &mut HashMap<String, Option<String>>,
) -> Option<String> {
    let mut current = id;
    let mut path = Vec::new();
    let material = loop {
        if let Some(value) = cache.get(current) {
            break value.clone();
        }
        path.push(current);
        if let Some(material) = graph.assembly.material_assignments.get(current) {
            break Some(material.clone());
        }
        match graph.nodes.get(current) {
            Some(InstanceNode::Clone { source, .. }) => current = source,
            _ => break None,
        }
    };
    for ancestor in path {
        cache.insert(ancestor.to_owned(), material.clone());
    }
    material
}

fn variant(instance: &PartInstance<'_>) -> Result<Variant, ModelError> {
    let parameters = resolve_parameters(instance.definition, &instance.overrides)?;
    let datums = instance
        .definition
        .datums
        .iter()
        .map(|datum| (datum.id.as_str(), datum))
        .collect();
    let references = reference_map(instance.definition);
    let mut signatures = BTreeMap::new();
    let mut downstream: HashMap<String, Vec<String>> = HashMap::new();
    for feature in &instance.definition.features {
        signatures.insert(
            feature.id.clone(),
            regeneration::feature_signature(&datums, feature, &references, &parameters)?,
        );
        for dependency in dependencies_with_references(feature, &references) {
            downstream
                .entry(dependency.into())
                .or_default()
                .push(feature.id.clone());
        }
    }
    Ok(Variant {
        family: instance.definition.id.clone(),
        parameters: parameters.into_iter().collect(),
        signatures,
        downstream,
        definition: definition_signature(instance.definition)?,
    })
}

fn definition_signature(definition: &FamilyDefinition) -> Result<Vec<u8>, ModelError> {
    let mut value = serde_json::to_value(definition)
        .map_err(|error| ModelError::new(format!("impact definition: {error}")))?;
    // Family declaration order is immaterial. Nested operation/expression
    // arrays retain order, matching semantic document comparison.
    if let Some(object) = value.as_object_mut() {
        for array in object
            .values_mut()
            .filter_map(serde_json::Value::as_array_mut)
        {
            array.sort_by(|left, right| {
                left.get("id")
                    .and_then(serde_json::Value::as_str)
                    .cmp(&right.get("id").and_then(serde_json::Value::as_str))
            });
        }
    }
    encode(&value)
}

fn affected_features(before: &Variant, after: &Variant) -> Vec<String> {
    let ids: BTreeSet<_> = before
        .signatures
        .keys()
        .chain(after.signatures.keys())
        .collect();
    let mut dirty: BTreeSet<String> = ids
        .into_iter()
        .filter(|id| {
            before.family != after.family || before.signatures.get(*id) != after.signatures.get(*id)
        })
        .cloned()
        .collect();
    let mut pending: VecDeque<_> = dirty.iter().cloned().collect();
    while let Some(id) = pending.pop_front() {
        for dependent in before
            .downstream
            .get(&id)
            .into_iter()
            .flatten()
            .chain(after.downstream.get(&id).into_iter().flatten())
        {
            if dirty.insert(dependent.clone()) {
                pending.push_back(dependent.clone());
            }
        }
    }
    dirty.into_iter().collect()
}

type PairCache = HashMap<(usize, usize), (Vec<String>, Vec<String>)>;
fn compare_instance(
    id: &str,
    before: Option<&Snapshot>,
    after: Option<&Snapshot>,
    pairs: &mut PairCache,
) -> Option<InstanceImpact> {
    let (Some(before), Some(after)) = (before, after) else {
        let value = before.or(after).expect("instance exists on one side");
        return Some(InstanceImpact {
            instance: id.into(),
            kind: if before.is_none() {
                InstanceChangeKind::Added
            } else {
                InstanceChangeKind::Removed
            },
            parameters: value.variant.parameters.keys().cloned().collect(),
            features: value.variant.signatures.keys().cloned().collect(),
            placement_changed: true,
            material_changed: value.material.is_some(),
            suppression_changed: value.suppressed,
            intent_changed: true,
        });
    };
    let key = (
        Rc::as_ptr(&before.variant) as usize,
        Rc::as_ptr(&after.variant) as usize,
    );
    let (parameters, features) = pairs.entry(key).or_insert_with(|| {
        let ids: BTreeSet<_> = before
            .variant
            .parameters
            .keys()
            .chain(after.variant.parameters.keys())
            .collect();
        let parameters = ids
            .into_iter()
            .filter(|id| before.variant.parameters.get(*id) != after.variant.parameters.get(*id))
            .cloned()
            .collect();
        (
            parameters,
            affected_features(&before.variant, &after.variant),
        )
    });
    let impact = InstanceImpact {
        instance: id.into(),
        kind: InstanceChangeKind::Updated,
        parameters: parameters.clone(),
        features: features.clone(),
        placement_changed: before.placement != after.placement || before.frames != after.frames,
        material_changed: before.material != after.material,
        suppression_changed: before.suppressed != after.suppressed,
        intent_changed: before.intent != after.intent
            || before.variant.definition != after.variant.definition,
    };
    (!impact.parameters.is_empty()
        || !impact.features.is_empty()
        || impact.placement_changed
        || impact.material_changed
        || impact.suppression_changed
        || impact.intent_changed)
        .then_some(impact)
}

fn drawing_impact(
    before: &ModelDocument,
    after: &ModelDocument,
    affected: &HashSet<&str>,
) -> Vec<String> {
    let old: BTreeMap<_, _> = before
        .drawings
        .iter()
        .map(|drawing| (drawing.id.as_str(), drawing))
        .collect();
    let new: BTreeMap<_, _> = after
        .drawings
        .iter()
        .map(|drawing| (drawing.id.as_str(), drawing))
        .collect();
    let ids: BTreeSet<_> = old.keys().chain(new.keys()).copied().collect();
    ids.into_iter().filter(|id| old.get(id) != new.get(id) || old.get(id).into_iter().chain(new.get(id)).any(|drawing| {
        drawing.views.iter().flat_map(|view| &view.outputs).any(|output| affected.contains(output.instance.as_str()))
            || drawing.dimensions.iter().any(|dimension| affected.contains(dimension.first.instance.as_str()) || affected.contains(dimension.second.instance.as_str()))
            || drawing.notes.iter().any(|note| matches!(&note.text, DrawingText::Parameter { instance, .. } if affected.contains(instance.as_str())))
    })).map(String::from).collect()
}

#[cfg(test)]
mod tests;

fn mesh_impact(
    before: &ModelDocument,
    after: &ModelDocument,
    affected: &HashSet<&str>,
) -> Vec<String> {
    let old: BTreeMap<_, _> = before
        .mesh_exports
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect();
    let new: BTreeMap<_, _> = after
        .mesh_exports
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect();
    let ids: BTreeSet<_> = old.keys().chain(new.keys()).copied().collect();
    ids.into_iter()
        .filter(|id| {
            old.get(id) != new.get(id)
                || old
                    .get(id)
                    .into_iter()
                    .chain(new.get(id))
                    .any(|definition| affected.contains(definition.output.instance.as_str()))
        })
        .map(String::from)
        .collect()
}
