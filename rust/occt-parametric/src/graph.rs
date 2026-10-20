//! Instance graphs: bases, clones, patterns, overrides, frames, and
//! managed generation.

use super::*;
#[allow(unused_imports)]
pub use occt_parametric_core::graph::*;

mod pattern_edits;
pub use pattern_edits::InstanceGraphPatternEdits;
pub trait ResolvedInstanceRegeneration {
    fn regenerate<'session>(
        &self,
        session: &'session Session,
    ) -> Result<GeneratedResult<'session>, ModelError>;
}

impl ResolvedInstanceRegeneration for ResolvedInstance<'_> {
    fn regenerate<'session>(
        &self,
        session: &'session Session,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        let result = self.instance.regenerate(session)?;
        self.place(session, result)
    }
}

pub(crate) trait ResolvedInstanceRegenerationInternal {
    /// Moves a locally generated result through the instance placement and
    /// every enclosing assembly frame. The input is consumed on success and failure.
    fn place<'session>(
        &self,
        session: &'session Session,
        result: GeneratedResult<'session>,
    ) -> Result<GeneratedResult<'session>, ModelError>;
}

impl ResolvedInstanceRegenerationInternal for ResolvedInstance<'_> {
    fn place<'session>(
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
    /// Generated instances, including dynamically expanded pattern members.
    /// Iteration order is unspecified.
    pub fn instances(&self) -> impl Iterator<Item = (&str, &GeneratedResult<'session>)> {
        self.results
            .iter()
            .map(|(id, result)| (id.as_str(), result))
    }

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
pub trait InstanceGraphRegeneration<'definition> {
    /// Regenerates every instance in the graph except suppressed pattern
    /// members; see [`Self::regenerate_instances`].
    fn regenerate_all<'session>(
        &mut self,
        session: &'session Session,
    ) -> Result<GraphRegeneration<'session>, ModelError>;

    /// Like [`Self::regenerate_all`], but failed required requirements stay
    /// in the verification results instead of failing the call, so a caller
    /// can report the whole table. Callers must not accept such a generation;
    /// geometry and requirement evaluation errors still fail.
    fn regenerate_all_reporting<'session>(
        &mut self,
        session: &'session Session,
    ) -> Result<GraphRegeneration<'session>, ModelError>;

    /// Regenerates the requested instances, running the feature graph once per
    /// distinct set of resolved parameters. Clones that differ only in
    /// placement or assembly frame duplicate the shared local result and are
    /// then placed independently. On failure every handle created by the call
    /// is released.
    fn regenerate_instances<'session>(
        &mut self,
        session: &'session Session,
        ids: &[&str],
    ) -> Result<GraphRegeneration<'session>, ModelError>;
}

impl<'definition> InstanceGraphRegeneration<'definition> for InstanceGraph<'definition> {
    fn regenerate_all<'session>(
        &mut self,
        session: &'session Session,
    ) -> Result<GraphRegeneration<'session>, ModelError> {
        regenerate_all_with(self, session, true)
    }

    fn regenerate_all_reporting<'session>(
        &mut self,
        session: &'session Session,
    ) -> Result<GraphRegeneration<'session>, ModelError> {
        regenerate_all_with(self, session, false)
    }

    fn regenerate_instances<'session>(
        &mut self,
        session: &'session Session,
        ids: &[&str],
    ) -> Result<GraphRegeneration<'session>, ModelError> {
        self.refresh_driven_patterns(session)?;
        self.regenerate_instances_current(session, ids)
    }
}

/// [`InstanceGraphRegeneration::regenerate_all`] with a requirement policy.
fn regenerate_all_with<'session>(
    graph: &mut InstanceGraph<'_>,
    session: &'session Session,
    reject: bool,
) -> Result<GraphRegeneration<'session>, ModelError> {
    graph.refresh_driven_patterns(session)?;
    let mut ids = graph
        .nodes
        .keys()
        .map(String::as_str)
        .filter(|id| !graph.is_suppressed(id))
        .collect::<Vec<_>>();
    ids.sort_unstable();
    let mut generation = graph.regenerate_instances_with(session, &ids, reject)?;
    match graph.verify_assembly_requirements(session, &generation, reject) {
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

pub(crate) trait InstanceGraphRegenerationInternal<'definition> {
    fn regenerate_instances_current<'session>(
        &self,
        session: &'session Session,
        ids: &[&str],
    ) -> Result<GraphRegeneration<'session>, ModelError>;

    /// Generates one group's local result once and places a copy for each
    /// member. On failure every handle created for the group is released.
    fn regenerate_group<'session>(
        &self,
        session: &'session Session,
        members: &[(&str, ResolvedInstance<'definition>)],
        reject: bool,
    ) -> Result<Vec<(String, GeneratedResult<'session>)>, ModelError>;

    /// [`Self::regenerate_instances_current`] with the requirement policy
    /// of [`PartInstanceRegenerationInternal::regenerate_internal`].
    fn regenerate_instances_with<'session>(
        &self,
        session: &'session Session,
        ids: &[&str],
        reject: bool,
    ) -> Result<GraphRegeneration<'session>, ModelError>;
}

impl<'definition> InstanceGraphRegenerationInternal<'definition> for InstanceGraph<'definition> {
    fn regenerate_instances_current<'session>(
        &self,
        session: &'session Session,
        ids: &[&str],
    ) -> Result<GraphRegeneration<'session>, ModelError> {
        self.regenerate_instances_with(session, ids, true)
    }

    fn regenerate_instances_with<'session>(
        &self,
        session: &'session Session,
        ids: &[&str],
        reject: bool,
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
            match self.regenerate_group(session, &members, reject) {
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

    fn regenerate_group<'session>(
        &self,
        session: &'session Session,
        members: &[(&str, ResolvedInstance<'definition>)],
        reject: bool,
    ) -> Result<Vec<(String, GeneratedResult<'session>)>, ModelError> {
        let (representative, first) = &members[0];
        let local = first
            .instance
            .regenerate_internal(session, None, reject)
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
}
