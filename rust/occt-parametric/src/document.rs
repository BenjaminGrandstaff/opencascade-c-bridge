//! Versioned model documents: validation and schema migration.

use super::*;

/// A named assembly coordinate frame. Its placement maps frame-local
/// coordinates into the parent frame, or into model coordinates at the root.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AssemblyFrame {
    pub id: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub placement: Placement,
    pub provenance: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GenerationRecord {
    pub instance_id: String,
    pub attempted_revision: u64,
    pub accepted_revision: Option<u64>,
    pub state: RegenerationState,
    pub last_error: Option<String>,
}

pub const CURRENT_SCHEMA_VERSION: u32 = 73;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelDocument {
    pub schema_version: u32,
    pub family: FamilyDefinition,
    #[serde(default)]
    pub additional_families: Vec<FamilyDefinition>,
    pub instances: Vec<InstanceNode>,
    #[serde(default)]
    pub patterns: Vec<Pattern>,
    #[serde(default)]
    pub frames: Vec<AssemblyFrame>,
    #[serde(default)]
    pub generation_records: Vec<GenerationRecord>,
    /// Relationships, configurations, and materials.
    #[serde(default)]
    pub assembly: AssemblySemantics,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawings: Vec<DrawingDefinition>,
    /// Ordered, append-only review history; independent from generation audits.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<DocumentRevision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mesh_exports: Vec<MeshExportDefinition>,
}

impl ModelDocument {
    pub fn from_graph(graph: &InstanceGraph<'_>) -> Self {
        let mut instances = graph.nodes.values().cloned().collect::<Vec<_>>();
        instances.sort_by(|left, right| left.id().cmp(right.id()));
        let mut frames = graph.frames.values().cloned().collect::<Vec<_>>();
        frames.sort_by(|left, right| left.id.cmp(&right.id));
        let mut additional_families = graph
            .additional_definitions
            .values()
            .map(|definition| (*definition).clone())
            .collect::<Vec<_>>();
        additional_families.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            family: graph.definition.clone(),
            additional_families,
            instances,
            patterns: graph.patterns.clone(),
            frames,
            generation_records: Vec::new(),
            drawings: Vec::new(),
            revisions: Vec::new(),
            mesh_exports: Vec::new(),
            assembly: AssemblySemantics {
                active_configuration: None,
                ..graph.assembly.clone()
            },
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, ModelError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| ModelError::new(format!("serialize model document: {error}")))
    }

    pub fn from_json(json: &str) -> Result<Self, ModelError> {
        let mut value: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| ModelError::new(format!("parse model document: {error}")))?;
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| ModelError::new("model document schema_version is required"))?;
        if version == 0 || version > u64::from(CURRENT_SCHEMA_VERSION) {
            return Err(ModelError::new(format!(
                "unsupported model document schema version {version}"
            )));
        }
        if version < 14 {
            migrate_linear_pattern_steps(&mut value)?;
        }
        if version < 17 {
            migrate_pattern_member_slots(&mut value)?;
        }
        let mut document: Self = serde_json::from_value(value)
            .map_err(|error| ModelError::new(format!("decode model document: {error}")))?;
        if version < 18 {
            document.adopt_pattern_slots();
        }
        if version < 17 {
            document.adopt_member_placements();
        }
        document.schema_version = CURRENT_SCHEMA_VERSION;
        document.validate()?;
        Ok(document)
    }

    pub fn instance_graph(&self) -> Result<InstanceGraph<'_>, ModelError> {
        self.validate()?;
        let nodes = self
            .instances
            .iter()
            .cloned()
            .map(|node| (node.id().to_owned(), node))
            .collect();
        Ok(InstanceGraph {
            definition: &self.family,
            additional_definitions: self
                .additional_families
                .iter()
                .map(|definition| (definition.id.clone(), definition))
                .collect(),
            nodes,
            patterns: self.patterns.clone(),
            frames: self.frame_map(),
            assembly: self.assembly.clone(),
        })
    }

    /// Schema 17 derives member placements from the rule; a stored placement
    /// that differs from its rule slot becomes an explicit override.
    pub(crate) fn adopt_member_placements(&mut self) {
        let placements: HashMap<&str, Placement> = self
            .instances
            .iter()
            .map(|node| (node.id(), node.placement()))
            .collect();
        for pattern in &mut self.patterns {
            let rule = pattern.rule;
            for member in &mut pattern.members {
                let stored = placements.get(member.id.as_str()).copied();
                let expected = rule.member_placement(member.index, pattern.slot_count);
                if stored.is_some_and(|stored| stored != expected) {
                    member.placement_override = stored;
                }
            }
        }
    }

    /// Schema 18 records the slot count and the prefix for grown members.
    pub(crate) fn adopt_pattern_slots(&mut self) {
        for pattern in &mut self.patterns {
            if pattern.slot_count == 0 {
                pattern.slot_count = pattern
                    .members
                    .iter()
                    .map(|member| member.index + 1)
                    .max()
                    .unwrap_or(0);
            }
            if pattern.member_prefix.is_empty() {
                pattern.member_prefix = derived_member_prefix(pattern);
            }
        }
    }

    pub(crate) fn frame_map(&self) -> HashMap<String, AssemblyFrame> {
        self.frames
            .iter()
            .cloned()
            .map(|frame| (frame.id.clone(), frame))
            .collect()
    }

    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(ModelError::new(format!(
                "model document must be migrated to schema version {CURRENT_SCHEMA_VERSION}"
            )));
        }
        self.validate_revisions()?;
        validate_definition(&self.family)?;
        resolve_parameters(&self.family, &HashMap::new())?;
        let mut family_ids = HashSet::from([self.family.id.as_str()]);
        for family in &self.additional_families {
            if family.id.is_empty() || !family_ids.insert(family.id.as_str()) {
                return Err(ModelError::new(
                    "document family ids must be nonempty and unique",
                ));
            }
            validate_definition(family)?;
            resolve_parameters(family, &HashMap::new())?;
        }
        let node_ids = self.validate_instance_ids()?;
        let graph = InstanceGraph {
            definition: &self.family,
            additional_definitions: self
                .additional_families
                .iter()
                .map(|definition| (definition.id.clone(), definition))
                .collect(),
            nodes: self
                .instances
                .iter()
                .cloned()
                .map(|node| (node.id().to_owned(), node))
                .collect(),
            patterns: self.patterns.clone(),
            frames: self.frame_map(),
            assembly: self.assembly.clone(),
        };
        self.validate_frames(&graph)?;
        let mut resolutions = HashMap::new();
        for node in &self.instances {
            let resolved = graph.resolve_cached(node.id(), &mut resolutions)?;
            resolve_parameters(resolved.definition, &resolved.overrides)?;
        }
        let mut pattern_ids = HashSet::new();
        let mut patterned_members = HashSet::new();
        for pattern in &self.patterns {
            if pattern.id.is_empty() || !pattern_ids.insert(pattern.id.as_str()) {
                return Err(ModelError::new(
                    "document pattern ids must be nonempty and unique",
                ));
            }
            validate_pattern(pattern, &graph, &node_ids, &mut patterned_members)?;
        }
        let mut recorded_instances = HashSet::new();
        for record in &self.generation_records {
            if !recorded_instances.insert(record.instance_id.as_str()) {
                return Err(ModelError::new(format!(
                    "duplicate generation record for instance '{}'",
                    record.instance_id
                )));
            }
            validate_generation_record(record, &node_ids)?;
        }
        graph.validate_assembly()?;
        self.validate_mesh_exports(&graph)?;
        self.validate_drawings(&graph)
    }

    fn validate_mesh_exports(&self, graph: &InstanceGraph<'_>) -> Result<(), ModelError> {
        let mut mesh_ids = HashSet::new();
        let mut mesh_context = mesh::ExportContext::new(graph);
        for definition in &self.mesh_exports {
            if !mesh_ids.insert(&definition.id) {
                return Err(ModelError::new("mesh export ids must be unique"));
            }
            definition.validate_cached(graph, &mut mesh_context)?;
        }
        Ok(())
    }

    fn validate_drawings(&self, graph: &InstanceGraph<'_>) -> Result<(), ModelError> {
        let mut drawings = HashSet::new();
        let materials = graph
            .assembly
            .materials
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        let mut resolutions = HashMap::new();
        let mut features = HashMap::new();
        for drawing in &self.drawings {
            if !drawings.insert(&drawing.id) {
                return Err(ModelError::new("document drawing IDs must be unique"));
            }
            drawing.validate_cached(graph, &mut resolutions, &mut features, &materials)?;
        }
        Ok(())
    }

    pub(crate) fn validate_instance_ids(&self) -> Result<HashSet<&str>, ModelError> {
        let mut node_ids = HashSet::new();
        for node in &self.instances {
            if node.id().is_empty() || !node_ids.insert(node.id()) {
                return Err(ModelError::new(
                    "document instance ids must be nonempty and unique",
                ));
            }
            node.placement().normalized()?;
        }
        Ok(node_ids)
    }

    pub(crate) fn validate_frames(&self, graph: &InstanceGraph<'_>) -> Result<(), ModelError> {
        let mut frame_ids = HashSet::new();
        if self
            .frames
            .iter()
            .any(|frame| frame.id.is_empty() || !frame_ids.insert(frame.id.as_str()))
        {
            return Err(ModelError::new(
                "document frame ids must be nonempty and unique",
            ));
        }
        for frame in &self.frames {
            graph.frame_chain(Some(&frame.id))?;
        }
        for node in &self.instances {
            graph.frame_chain(node.frame()).map_err(|error| {
                ModelError::new(format!("instance '{}': {}", node.id(), error.message))
            })?;
        }
        Ok(())
    }
}

pub(crate) fn validate_pattern<'document>(
    pattern: &'document Pattern,
    graph: &InstanceGraph<'_>,
    node_ids: &HashSet<&str>,
    patterned_members: &mut HashSet<&'document str>,
) -> Result<(), ModelError> {
    let context =
        |error: ModelError| ModelError::new(format!("pattern '{}': {}", pattern.id, error.message));
    if !node_ids.contains(pattern.source.as_str()) {
        return Err(ModelError::new(format!(
            "pattern '{}' references unknown source '{}'",
            pattern.id, pattern.source
        )));
    }
    if pattern.members.is_empty() {
        return Err(ModelError::new(format!(
            "pattern '{}' has no members",
            pattern.id
        )));
    }
    pattern.rule.validate().map_err(context)?;
    validate_pattern_driver_rule(
        &pattern.rule,
        pattern.count_driver.as_ref(),
        pattern.span_driver.as_ref(),
    )
    .map_err(context)?;
    if let Some(driver) = &pattern.count_driver {
        graph.validate_count_driver(driver).map_err(context)?;
    }
    if let Some(driver) = &pattern.span_driver {
        graph.validate_span_driver(driver).map_err(context)?;
    }
    validate_pattern_slots(pattern).map_err(context)?;
    graph
        .frame_chain(pattern.frame.as_deref())
        .map_err(context)?;
    let mut slots = HashSet::new();
    for member in &pattern.members {
        if !slots.insert(member.index) {
            return Err(ModelError::new(format!(
                "pattern '{}' uses rule slot {} more than once",
                pattern.id, member.index
            )));
        }
        if !patterned_members.insert(member.id.as_str()) {
            return Err(ModelError::new(format!(
                "instance '{}' belongs to more than one pattern",
                member.id
            )));
        }
        validate_pattern_member(pattern, member, graph, node_ids)?;
    }
    Ok(())
}

/// Slot count, prefix, and member slots must agree with each other and with
/// any count the rule's constraints require.
pub(crate) fn validate_pattern_slots(pattern: &Pattern) -> Result<(), ModelError> {
    if pattern.slot_count == 0 || pattern.member_prefix.is_empty() {
        return Err(ModelError::new(
            "pattern requires slots and a member prefix",
        ));
    }
    if let Some(member) = pattern
        .members
        .iter()
        .find(|member| member.index >= pattern.slot_count)
    {
        return Err(ModelError::new(format!(
            "member '{}' slot {} is outside the {} pattern slots",
            member.id, member.index, pattern.slot_count
        )));
    }
    match pattern.rule.fitted_count()? {
        Some(required) if required != pattern.slot_count => Err(ModelError::new(format!(
            "constraints require {required} slots but the pattern has {}",
            pattern.slot_count
        ))),
        _ => Ok(()),
    }
}

/// A member must be a clone of the pattern source, in the pattern frame, at
/// its override or rule placement.
pub(crate) fn validate_pattern_member(
    pattern: &Pattern,
    member: &PatternMember,
    graph: &InstanceGraph<'_>,
    node_ids: &HashSet<&str>,
) -> Result<(), ModelError> {
    let id = member.id.as_str();
    if !node_ids.contains(id) {
        return Err(ModelError::new(format!(
            "pattern '{}' references unknown member '{id}'",
            pattern.id
        )));
    }
    if let Some(placement) = member.placement_override {
        placement.normalized()?;
    }
    let linked = matches!(
        graph.node(id),
        Some(node @ InstanceNode::Clone { source, .. })
            if source == &pattern.source && node.frame() == pattern.frame.as_deref()
    );
    if !linked {
        return Err(ModelError::new(format!(
            "pattern '{}' member '{id}' is not linked to source '{}' in the pattern frame",
            pattern.id, pattern.source
        )));
    }
    let stored = graph
        .node(id)
        .expect("pattern member existence was checked")
        .placement();
    if !placements_equivalent(stored, pattern.member_placement(member))? {
        return Err(ModelError::new(format!(
            "pattern '{}' member '{id}' placement does not match its rule slot or override",
            pattern.id
        )));
    }
    Ok(())
}

pub(crate) fn validate_generation_record(
    record: &GenerationRecord,
    node_ids: &HashSet<&str>,
) -> Result<(), ModelError> {
    if !node_ids.contains(record.instance_id.as_str()) {
        return Err(ModelError::new(format!(
            "generation record references unknown instance '{}'",
            record.instance_id
        )));
    }
    if record
        .accepted_revision
        .is_some_and(|revision| revision > record.attempted_revision)
    {
        return Err(ModelError::new(format!(
            "generation record for '{}' has an invalid accepted revision",
            record.instance_id
        )));
    }
    let has_accepted = record.accepted_revision.is_some();
    let state_is_accepted = matches!(
        record.state,
        RegenerationState::Current | RegenerationState::Frozen | RegenerationState::Stale
    );
    if has_accepted != state_is_accepted {
        return Err(ModelError::new(format!(
            "generation record for '{}' has an inconsistent state",
            record.instance_id
        )));
    }
    Ok(())
}

/// `pew` from a first member named `pew[0]`, otherwise the pattern id.
pub(crate) fn derived_member_prefix(pattern: &Pattern) -> String {
    pattern
        .members
        .first()
        .and_then(|member| {
            let (prefix, slot) = member.id.strip_suffix(']')?.rsplit_once('[')?;
            let numeric = !slot.is_empty() && slot.bytes().all(|byte| byte.is_ascii_digit());
            (numeric && !prefix.is_empty()).then(|| prefix.to_owned())
        })
        .unwrap_or_else(|| pattern.id.clone())
}

/// Schema 17 replaced member id strings with slot records numbered by position.
pub(crate) fn migrate_pattern_member_slots(
    document: &mut serde_json::Value,
) -> Result<(), ModelError> {
    let Some(patterns) = document
        .get_mut("patterns")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(());
    };
    for pattern in patterns {
        let Some(members) = pattern
            .get_mut("members")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return Err(ModelError::new("model document pattern requires members"));
        };
        for (index, member) in members.iter_mut().enumerate() {
            if let Some(id) = member.as_str() {
                *member = serde_json::json!({ "id": id, "index": index });
            }
        }
    }
    Ok(())
}

/// Schema 14 replaced the flat linear `step` with a tagged pattern `rule`.
pub(crate) fn migrate_linear_pattern_steps(
    document: &mut serde_json::Value,
) -> Result<(), ModelError> {
    let Some(patterns) = document
        .get_mut("patterns")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(());
    };
    for pattern in patterns {
        let object = pattern
            .as_object_mut()
            .ok_or_else(|| ModelError::new("model document pattern must be an object"))?;
        let step = object
            .remove("step")
            .ok_or_else(|| ModelError::new("legacy linear pattern requires a step"))?;
        object.insert(
            "rule".into(),
            serde_json::json!({ "linear": { "step": step } }),
        );
    }
    Ok(())
}
