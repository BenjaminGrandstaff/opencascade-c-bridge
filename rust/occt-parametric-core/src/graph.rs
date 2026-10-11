//! Instance graphs: bases, clones, patterns, overrides, frames, and
//! managed generation.

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstanceNode {
    Base {
        id: String,
        /// `None` selects the graph's primary family. Named families are
        /// registered on the graph and serialized in the document.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        family: Option<String>,
        overrides: HashMap<String, ParameterValue>,
        #[serde(default)]
        placement: Placement,
        #[serde(default)]
        frame: Option<String>,
        provenance: String,
    },
    Clone {
        id: String,
        source: String,
        overrides: HashMap<String, ParameterValue>,
        #[serde(default)]
        placement: Placement,
        #[serde(default)]
        frame: Option<String>,
        provenance: String,
    },
}

impl InstanceNode {
    pub fn id(&self) -> &str {
        match self {
            Self::Base { id, .. } | Self::Clone { id, .. } => id,
        }
    }

    pub fn overrides(&self) -> &HashMap<String, ParameterValue> {
        match self {
            Self::Base { overrides, .. } | Self::Clone { overrides, .. } => overrides,
        }
    }

    pub fn placement(&self) -> Placement {
        match self {
            Self::Base { placement, .. } | Self::Clone { placement, .. } => *placement,
        }
    }

    pub fn frame(&self) -> Option<&str> {
        match self {
            Self::Base { frame, .. } | Self::Clone { frame, .. } => frame.as_deref(),
        }
    }

    pub fn frame_mut(&mut self) -> &mut Option<String> {
        match self {
            Self::Base { frame, .. } | Self::Clone { frame, .. } => frame,
        }
    }

    pub fn overrides_mut(&mut self) -> &mut HashMap<String, ParameterValue> {
        match self {
            Self::Base { overrides, .. } | Self::Clone { overrides, .. } => overrides,
        }
    }
}

pub struct ResolvedInstance<'definition> {
    pub instance: PartInstance<'definition>,
    /// Placement within the instance's assembly frame.
    pub placement: Placement,
    /// Enclosing frame placements, innermost first, applied after `placement`.
    pub frames: Vec<Placement>,
}

/// Member changes a checked pattern resize will make.
pub struct ResizePlan {
    pub removed: Vec<String>,
    pub added: Vec<(String, usize)>,
}

#[derive(Clone)]
pub struct InstanceGraph<'definition> {
    pub definition: &'definition FamilyDefinition,
    pub additional_definitions: HashMap<String, &'definition FamilyDefinition>,
    pub nodes: HashMap<String, InstanceNode>,
    pub patterns: Vec<Pattern>,
    pub frames: HashMap<String, AssemblyFrame>,
    pub assembly: AssemblySemantics,
}

pub type ResolutionCache<'definition> = HashMap<String, PartInstance<'definition>>;
impl<'definition> InstanceGraph<'definition> {
    pub fn new(definition: &'definition FamilyDefinition) -> Self {
        Self {
            definition,
            additional_definitions: HashMap::new(),
            nodes: HashMap::new(),
            patterns: Vec::new(),
            frames: HashMap::new(),
            assembly: AssemblySemantics::default(),
        }
    }

    pub fn add_base(
        &mut self,
        id: impl Into<String>,
        overrides: HashMap<String, ParameterValue>,
        provenance: impl Into<String>,
    ) -> Result<(), ModelError> {
        let id = id.into();
        self.insert(InstanceNode::Base {
            id,
            family: None,
            overrides,
            placement: Placement::identity(),
            frame: None,
            provenance: provenance.into(),
        })
    }

    /// Registers another family definition for use by base instances.
    pub fn add_family(
        &mut self,
        definition: &'definition FamilyDefinition,
    ) -> Result<(), ModelError> {
        validate_definition(definition)?;
        resolve_parameters(definition, &HashMap::new())?;
        if definition.id.is_empty()
            || definition.id == self.definition.id
            || self.additional_definitions.contains_key(&definition.id)
        {
            return Err(ModelError::new(
                "family ids in an instance graph must be nonempty and unique",
            ));
        }
        self.additional_definitions
            .insert(definition.id.clone(), definition);
        Ok(())
    }

    /// Adds a base instance belonging to a registered family.
    pub fn add_base_from_family(
        &mut self,
        id: impl Into<String>,
        family: &str,
        overrides: HashMap<String, ParameterValue>,
        provenance: impl Into<String>,
    ) -> Result<(), ModelError> {
        self.definition_by_id(family)?;
        self.insert(InstanceNode::Base {
            id: id.into(),
            family: (family != self.definition.id).then(|| family.to_owned()),
            overrides,
            placement: Placement::identity(),
            frame: None,
            provenance: provenance.into(),
        })
    }

    pub fn add_clone(
        &mut self,
        id: impl Into<String>,
        source: impl Into<String>,
        overrides: HashMap<String, ParameterValue>,
        provenance: impl Into<String>,
    ) -> Result<(), ModelError> {
        let id = id.into();
        self.insert(InstanceNode::Clone {
            id,
            source: source.into(),
            overrides,
            placement: Placement::identity(),
            frame: None,
            provenance: provenance.into(),
        })
    }

    pub fn node(&self, id: &str) -> Option<&InstanceNode> {
        self.nodes.get(id)
    }

    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }

    /// Sets an instance placement. On a pattern member this records a
    /// placement override that later rule edits leave in place.
    pub fn set_placement(
        &mut self,
        instance_id: &str,
        placement: Placement,
    ) -> Result<(), ModelError> {
        placement.normalized()?;
        if let Some(member) = self.pattern_member_mut(instance_id) {
            member.placement_override = Some(placement);
        }
        let node = self
            .nodes
            .get_mut(instance_id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{instance_id}'")))?;
        match node {
            InstanceNode::Base {
                placement: value, ..
            }
            | InstanceNode::Clone {
                placement: value, ..
            } => *value = placement,
        }
        Ok(())
    }

    pub fn frame(&self, id: &str) -> Option<&AssemblyFrame> {
        self.frames.get(id)
    }

    pub fn add_frame(
        &mut self,
        id: impl Into<String>,
        parent: Option<&str>,
        placement: Placement,
        provenance: impl Into<String>,
    ) -> Result<(), ModelError> {
        let id = id.into();
        if id.is_empty() || self.frames.contains_key(&id) {
            return Err(ModelError::new("frame id must be nonempty and unique"));
        }
        placement.normalized()?;
        self.frame_chain(parent)?;
        self.frames.insert(
            id.clone(),
            AssemblyFrame {
                id,
                parent: parent.map(str::to_owned),
                placement,
                provenance: provenance.into(),
            },
        );
        Ok(())
    }

    pub fn set_frame_placement(
        &mut self,
        frame_id: &str,
        placement: Placement,
    ) -> Result<(), ModelError> {
        placement.normalized()?;
        self.frames
            .get_mut(frame_id)
            .ok_or_else(|| ModelError::new(format!("unknown assembly frame '{frame_id}'")))?
            .placement = placement;
        Ok(())
    }

    /// Places an instance inside an assembly frame, or at model level with `None`.
    /// Pattern members move with their pattern through [`Self::set_pattern_frame`].
    pub fn set_instance_frame(
        &mut self,
        instance_id: &str,
        frame: Option<&str>,
    ) -> Result<(), ModelError> {
        self.frame_chain(frame)?;
        if let Some(pattern) = self
            .patterns
            .iter()
            .find(|pattern| pattern.member(instance_id).is_some())
        {
            return Err(ModelError::new(format!(
                "instance '{instance_id}' belongs to pattern '{}'; set the pattern frame instead",
                pattern.id
            )));
        }
        let node = self
            .nodes
            .get_mut(instance_id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{instance_id}'")))?;
        *node.frame_mut() = frame.map(str::to_owned);
        Ok(())
    }

    pub fn set_override(
        &mut self,
        instance_id: &str,
        parameter: impl Into<String>,
        value: ParameterValue,
    ) -> Result<(), ModelError> {
        let node = self
            .nodes
            .get_mut(instance_id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{instance_id}'")))?;
        node.overrides_mut().insert(parameter.into(), value);
        Ok(())
    }

    pub fn remove_override(
        &mut self,
        instance_id: &str,
        parameter: &str,
    ) -> Result<Option<ParameterValue>, ModelError> {
        let node = self
            .nodes
            .get_mut(instance_id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{instance_id}'")))?;
        Ok(node.overrides_mut().remove(parameter))
    }

    pub fn definition_by_id(
        &self,
        family: &str,
    ) -> Result<&'definition FamilyDefinition, ModelError> {
        if family == self.definition.id {
            return Ok(self.definition);
        }
        self.additional_definitions
            .get(family)
            .copied()
            .ok_or_else(|| ModelError::new(format!("unknown family definition '{family}'")))
    }

    /// Resolves one clone in O(uncached depth), memoizing every parent crossed
    /// by the walk. A shared operation-local cache makes resolving a forest
    /// O(nodes + inherited override copies), while avoiding stale state after
    /// graph mutations. The visited-position map retains exact cycle paths.
    pub fn resolve_cached(
        &self,
        id: &str,
        cache: &mut ResolutionCache<'definition>,
    ) -> Result<PartInstance<'definition>, ModelError> {
        if let Some(resolved) = cache.get(id) {
            return Ok(resolved.clone());
        }
        let mut path: Vec<&InstanceNode> = Vec::new();
        let mut positions = HashMap::new();
        let mut current = id;
        let (definition, mut overrides) = loop {
            if let Some(resolved) = cache.get(current) {
                break (resolved.definition, resolved.overrides.clone());
            }
            if let Some(position) = positions.get(current).copied() {
                let mut cycle = path[position..]
                    .iter()
                    .map(|node| node.id())
                    .collect::<Vec<_>>();
                cycle.push(current);
                return Err(ModelError::new(format!(
                    "clone inheritance cycle: {}",
                    cycle.join(" -> ")
                )));
            }
            let node = self
                .nodes
                .get(current)
                .ok_or_else(|| ModelError::new(format!("unknown clone source '{current}'")))?;
            positions.insert(node.id(), path.len());
            path.push(node);
            match node {
                InstanceNode::Base { family, .. } => {
                    let definition = family
                        .as_deref()
                        .map_or(Ok(self.definition), |family| self.definition_by_id(family))?;
                    break (definition, HashMap::new());
                }
                InstanceNode::Clone { source, .. } => current = source,
            }
        };
        for node in path.iter().rev() {
            overrides.extend(node.overrides().clone());
            if let Some(configured) = self.assembly.configured_overrides(node.id()) {
                overrides.extend(configured.clone());
            }
            let provenance = match node {
                InstanceNode::Base { provenance, .. } | InstanceNode::Clone { provenance, .. } => {
                    provenance.clone()
                }
            };
            cache.insert(
                node.id().to_owned(),
                PartInstance {
                    id: node.id().to_owned(),
                    definition,
                    overrides: overrides.clone(),
                    provenance,
                },
            );
        }
        cache
            .get(id)
            .cloned()
            .ok_or_else(|| ModelError::new(format!("unknown instance '{id}'")))
    }

    pub fn resolve(&self, id: &str) -> Result<PartInstance<'definition>, ModelError> {
        self.resolve_cached(id, &mut HashMap::new())
    }

    pub fn resolve_with_placement(
        &self,
        id: &str,
    ) -> Result<ResolvedInstance<'definition>, ModelError> {
        self.resolve_with_placement_cached(id, &mut HashMap::new())
    }

    pub fn resolve_with_placement_cached(
        &self,
        id: &str,
        cache: &mut ResolutionCache<'definition>,
    ) -> Result<ResolvedInstance<'definition>, ModelError> {
        let instance = self.resolve_cached(id, cache)?;
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{id}'")))?;
        Ok(ResolvedInstance {
            instance,
            placement: node.placement(),
            frames: self.frame_chain(node.frame())?,
        })
    }

    /// Groups requested instances by their complete resolved parameter set,
    /// preserving request order within and across groups.
    pub fn group_by_parameters<'ids>(
        &self,
        ids: &[&'ids str],
    ) -> Result<Vec<Vec<(&'ids str, ResolvedInstance<'definition>)>>, ModelError> {
        let mut seen = HashSet::new();
        let mut keys: Vec<String> = Vec::new();
        let mut groups: Vec<Vec<(&str, ResolvedInstance<'definition>)>> = Vec::new();
        let mut resolutions = HashMap::new();
        for &id in ids {
            if !seen.insert(id) {
                return Err(ModelError::new(format!(
                    "instance '{id}' requested more than once"
                )));
            }
            if self.is_suppressed(id) {
                return Err(ModelError::new(format!(
                    "instance '{id}' is suppressed in its pattern"
                )));
            }
            let resolved = self
                .resolve_with_placement_cached(id, &mut resolutions)
                .map_err(|error| instance_error(id, error))?;
            let parameters =
                resolve_parameters(resolved.instance.definition, &resolved.instance.overrides)
                    .map_err(|error| instance_error(id, error))?;
            let key = serde_json::to_string(&(
                &resolved.instance.definition.id,
                resolved.instance.definition.version,
                parameters.into_iter().collect::<BTreeMap<_, _>>(),
            ))
            .map_err(|error| ModelError::new(format!("create parameter key: {error}")))?;
            match keys.iter().position(|existing| *existing == key) {
                Some(index) => groups[index].push((id, resolved)),
                None => {
                    keys.push(key);
                    groups.push(vec![(id, resolved)]);
                }
            }
        }
        Ok(groups)
    }

    pub fn all_features_reused(definition: &FamilyDefinition) -> RegenerationReport {
        RegenerationReport {
            rebuilt: Vec::new(),
            reused: definition
                .features
                .iter()
                .map(|feature| feature.id.clone())
                .collect(),
        }
    }

    /// Replaces a linked clone with an independent base carrying its resolved
    /// parameters, placement, and frame. It also leaves any pattern.
    pub fn detach(&mut self, id: &str) -> Result<(), ModelError> {
        let resolved = self.resolve(id)?;
        let inherited_material = self.material_of(id)?.map(|material| material.id.clone());
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{id}'")))?;
        let placement = node.placement();
        let frame = node.frame().map(str::to_owned);
        self.nodes.insert(
            id.into(),
            InstanceNode::Base {
                id: id.into(),
                family: (resolved.definition.id != self.definition.id)
                    .then(|| resolved.definition.id.clone()),
                overrides: resolved.overrides,
                placement,
                frame,
                provenance: format!("detached from linked source; {}", resolved.provenance),
            },
        );
        self.pin_material(id, inherited_material);
        // A detached instance is no longer a linked pattern member; a pattern
        // left without members is removed.
        for pattern in &mut self.patterns {
            pattern.members.retain(|member| member.id != id);
        }
        self.patterns.retain(|pattern| !pattern.members.is_empty());
        Ok(())
    }

    pub fn insert(&mut self, node: InstanceNode) -> Result<(), ModelError> {
        if node.id().is_empty() {
            return Err(ModelError::new("instance id must not be empty"));
        }
        if self.nodes.contains_key(node.id()) {
            return Err(ModelError::new(format!(
                "duplicate instance id '{}'",
                node.id()
            )));
        }
        self.nodes.insert(node.id().into(), node);
        Ok(())
    }

    /// Returns frame placements from `frame` outward to the model root.
    pub fn frame_chain(&self, frame: Option<&str>) -> Result<Vec<Placement>, ModelError> {
        let mut visiting: Vec<&str> = Vec::new();
        let mut placements = Vec::new();
        let mut current = frame;
        while let Some(id) = current {
            if let Some(position) = visiting.iter().position(|visited| *visited == id) {
                let mut cycle = visiting[position..].to_vec();
                cycle.push(id);
                return Err(ModelError::new(format!(
                    "assembly frame cycle: {}",
                    cycle.join(" -> ")
                )));
            }
            let frame = self
                .frames
                .get(id)
                .ok_or_else(|| ModelError::new(format!("unknown assembly frame '{id}'")))?;
            frame.placement.normalized()?;
            visiting.push(id);
            placements.push(frame.placement);
            if let Some(joint) = self.assembly.joints.get(id) {
                placements.push(joint.motion()?);
            }
            current = frame.parent.as_deref();
        }
        Ok(placements)
    }
}

pub mod pattern_edits;
