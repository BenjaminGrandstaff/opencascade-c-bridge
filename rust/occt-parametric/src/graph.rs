//! Instance graphs: bases, clones, patterns, overrides, frames, and
//! managed generation.

use super::*;

mod pattern_edits;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

    pub(crate) fn frame_mut(&mut self) -> &mut Option<String> {
        match self {
            Self::Base { frame, .. } | Self::Clone { frame, .. } => frame,
        }
    }

    pub(crate) fn overrides_mut(&mut self) -> &mut HashMap<String, ParameterValue> {
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

impl ResolvedInstance<'_> {
    pub fn regenerate<'session>(
        &self,
        session: &'session Session,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        let result = self.instance.regenerate(session)?;
        self.place(session, result)
    }

    /// Moves a locally generated result through the instance placement and
    /// every enclosing assembly frame. The input is consumed on success and failure.
    pub(crate) fn place<'session>(
        &self,
        session: &'session Session,
        mut result: GeneratedResult<'session>,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        result = apply_placement(session, result, self.placement)?;
        for frame in &self.frames {
            result = apply_placement(session, result, *frame)?;
        }
        Ok(result)
    }
}

/// Placed results for several graph instances. Instances whose resolved
/// parameters are identical share one local generation and receive
/// independent placed handles.
pub struct GraphRegeneration<'session> {
    pub(crate) results: HashMap<String, GeneratedResult<'session>>,
    pub(crate) shared_from: HashMap<String, String>,
    pub(crate) generated_variants: usize,
    pub(crate) verification: Vec<VerificationResult>,
}

impl<'session> GraphRegeneration<'session> {
    pub fn result(&self, instance_id: &str) -> Option<&GeneratedResult<'session>> {
        self.results.get(instance_id)
    }

    /// The instance whose local generation produced this instance's geometry.
    /// A representative instance reports its own id.
    pub fn shared_from(&self, instance_id: &str) -> Option<&str> {
        self.shared_from.get(instance_id).map(String::as_str)
    }

    /// Number of feature-graph regenerations performed.
    pub fn generated_variants(&self) -> usize {
        self.generated_variants
    }

    /// Results of graph-level assembly requirements. Partial regeneration
    /// leaves this empty because it may not contain every referenced instance.
    pub fn verification(&self) -> &[VerificationResult] {
        &self.verification
    }

    pub fn into_results(self) -> HashMap<String, GeneratedResult<'session>> {
        self.results
    }
}

/// Member changes a checked pattern resize will make.
pub(crate) struct ResizePlan {
    pub(crate) removed: Vec<String>,
    pub(crate) added: Vec<(String, usize)>,
}

#[derive(Clone)]
pub struct InstanceGraph<'definition> {
    pub(crate) definition: &'definition FamilyDefinition,
    pub(crate) additional_definitions: HashMap<String, &'definition FamilyDefinition>,
    pub(crate) nodes: HashMap<String, InstanceNode>,
    pub(crate) patterns: Vec<Pattern>,
    pub(crate) frames: HashMap<String, AssemblyFrame>,
    pub(crate) assembly: AssemblySemantics,
}

pub(crate) type ResolutionCache<'definition> = HashMap<String, PartInstance<'definition>>;
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

    pub(crate) fn definition_by_id(
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
    pub(crate) fn resolve_cached(
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

    pub(crate) fn resolve_with_placement_cached(
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

    /// Regenerates every instance in the graph except suppressed pattern
    /// members; see [`Self::regenerate_instances`].
    pub fn regenerate_all<'session>(
        &mut self,
        session: &'session Session,
    ) -> Result<GraphRegeneration<'session>, ModelError> {
        self.refresh_driven_patterns(session)?;
        let mut ids = self
            .nodes
            .keys()
            .map(String::as_str)
            .filter(|id| !self.is_suppressed(id))
            .collect::<Vec<_>>();
        ids.sort_unstable();
        let mut generation = self.regenerate_instances_current(session, &ids)?;
        match self.verify_assembly_requirements(session, &generation.results) {
            Ok(verification) => {
                generation.verification = verification;
                Ok(generation)
            }
            Err(error) => {
                release_results(session, generation.results.into_values());
                Err(error)
            }
        }
    }

    /// Regenerates the requested instances, running the feature graph once per
    /// distinct set of resolved parameters. Clones that differ only in
    /// placement or assembly frame duplicate the shared local result and are
    /// then placed independently. On failure every handle created by the call
    /// is released.
    pub fn regenerate_instances<'session>(
        &mut self,
        session: &'session Session,
        ids: &[&str],
    ) -> Result<GraphRegeneration<'session>, ModelError> {
        self.refresh_driven_patterns(session)?;
        self.regenerate_instances_current(session, ids)
    }

    pub(crate) fn regenerate_instances_current<'session>(
        &self,
        session: &'session Session,
        ids: &[&str],
    ) -> Result<GraphRegeneration<'session>, ModelError> {
        let groups = self.group_by_parameters(ids)?;
        let mut output = GraphRegeneration {
            results: HashMap::new(),
            shared_from: HashMap::new(),
            generated_variants: groups.len(),
            verification: Vec::new(),
        };
        for members in groups {
            let representative = members[0].0.to_owned();
            match self.regenerate_group(session, &members) {
                Ok(results) => {
                    for (id, result) in results {
                        output
                            .shared_from
                            .insert(id.clone(), representative.clone());
                        output.results.insert(id, result);
                    }
                }
                Err(error) => {
                    release_results(session, output.results.into_values());
                    return Err(error);
                }
            }
        }
        Ok(output)
    }

    /// Groups requested instances by their complete resolved parameter set,
    /// preserving request order within and across groups.
    pub(crate) fn group_by_parameters<'ids>(
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

    /// Generates one group's local result once and places a copy for each
    /// member. On failure every handle created for the group is released.
    pub(crate) fn regenerate_group<'session>(
        &self,
        session: &'session Session,
        members: &[(&str, ResolvedInstance<'definition>)],
    ) -> Result<Vec<(String, GeneratedResult<'session>)>, ModelError> {
        let (representative, first) = &members[0];
        let local = first
            .instance
            .regenerate(session)
            .map_err(|error| instance_error(representative, error))?;
        let mut unplaced = Vec::with_capacity(members.len());
        for (id, _) in &members[1..] {
            match duplicate_result(session, &local) {
                Ok(copy) => unplaced.push(copy),
                Err(error) => {
                    release_results(session, unplaced.into_iter().chain([local]));
                    return Err(instance_error(id, error));
                }
            }
        }
        unplaced.insert(0, local);

        let mut pending = unplaced.into_iter();
        let mut placed = Vec::with_capacity(members.len());
        for (index, (id, resolved)) in members.iter().enumerate() {
            let shared = pending.next().expect("one local result per member");
            match resolved.place(session, shared) {
                Ok(mut result) => {
                    if index > 0 {
                        result.regeneration = Self::all_features_reused(first.instance.definition);
                    }
                    placed.push(((*id).to_owned(), result));
                }
                Err(error) => {
                    let placed = placed.into_iter().map(|(_, result)| result);
                    release_results(session, placed.chain(pending));
                    return Err(instance_error(id, error));
                }
            }
        }
        Ok(placed)
    }

    pub(crate) fn all_features_reused(definition: &FamilyDefinition) -> RegenerationReport {
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

    pub(crate) fn insert(&mut self, node: InstanceNode) -> Result<(), ModelError> {
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
    pub(crate) fn frame_chain(&self, frame: Option<&str>) -> Result<Vec<Placement>, ModelError> {
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
            current = frame.parent.as_deref();
        }
        Ok(placements)
    }
}
