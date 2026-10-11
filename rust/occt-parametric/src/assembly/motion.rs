//! Bounded sampled motion with shared local geometry and exact collision checks.

use super::*;
#[allow(unused_imports)]
pub use occt_parametric_core::assembly::motion::*;

mod closed;
pub use closed::InstanceGraphClosedMotion;

mod continuous;
pub use continuous::InstanceGraphContinuousMotion;
pub trait InstanceGraphMotion {
    /// Generates each parameter variant once, then places shared copies for each
    /// sample. The source graph and accepted generations stay unchanged. All
    /// samples/limits validate before kernel work. O(variants' generation +
    /// samples * (graph/joint copy + participating frame paths + BVH/pair work));
    /// handle storage is O(local outputs + one sample's outputs), independent
    /// of sample count. Reports sampled contacts, not continuous collision proof.
    fn run_motion_study(
        &self,
        session: &Session,
        study: &MotionStudy,
    ) -> Result<MotionResult, ModelError>;
}

impl InstanceGraphMotion for InstanceGraph<'_> {
    fn run_motion_study(
        &self,
        session: &Session,
        study: &MotionStudy,
    ) -> Result<MotionResult, ModelError> {
        let exclusions = validate_study(self, study)?;
        let (locals, bindings) = prepare_motion(session, self, &study.outputs)?;
        let mut samples = Vec::with_capacity(study.samples.len());
        for (index, sample) in study.samples.iter().enumerate() {
            let candidate = sample_graph(self, sample, index)?;
            let generation = motion_generation(session, &candidate, &bindings, &locals)?;
            let collisions = generation.check_selected_collisions(
                session,
                &study.outputs,
                study.collision_options,
                &exclusions,
            )?;
            let relationships = candidate.check_relationships()?;
            samples.push(MotionSampleResult {
                index,
                positions: sample.positions.clone(),
                collisions,
                relationships,
            });
        }
        Ok(MotionResult {
            samples,
            generated_variants: locals.len(),
        })
    }
}

fn motion_generation<'session>(
    session: &'session Session,
    graph: &InstanceGraph<'_>,
    bindings: &[Binding],
    locals: &[GeneratedResult<'session>],
) -> Result<GraphRegeneration<'session>, ModelError> {
    let mut generation = GraphRegeneration {
        results: HashMap::new(),
        shared_from: HashMap::new(),
        generated_variants: locals.len(),
        verification: Vec::new(),
    };
    let mut representatives = HashMap::new();
    for binding in bindings {
        let mut result = duplicate_result(session, &locals[binding.group])?;
        result = apply_placement(session, result, binding.placement)?;
        for placement in graph.frame_chain(binding.frame.as_deref())? {
            result = apply_placement(session, result, placement)?;
        }
        let representative = representatives
            .entry(binding.group)
            .or_insert_with(|| binding.instance.clone());
        generation
            .shared_from
            .insert(binding.instance.clone(), representative.clone());
        generation.results.insert(binding.instance.clone(), result);
    }
    Ok(generation)
}

fn prepare_motion<'session>(
    session: &'session Session,
    graph: &InstanceGraph<'_>,
    outputs: &[InstanceOutputRef],
) -> Result<(Vec<GeneratedResult<'session>>, Vec<Binding>), ModelError> {
    let ids = outputs
        .iter()
        .map(|output| output.instance.as_str())
        .collect::<Vec<_>>();
    let groups = graph.group_by_parameters(&ids)?;
    let mut locals = Vec::with_capacity(groups.len());
    let mut bindings = Vec::with_capacity(ids.len());
    for (group, members) in groups.iter().enumerate() {
        let local = members[0].1.instance.regenerate(session)?;
        for (id, resolved) in members {
            bindings.push(Binding {
                instance: (*id).to_owned(),
                group,
                placement: resolved.placement,
                frame: graph
                    .nodes
                    .get(*id)
                    .and_then(|node| node.frame())
                    .map(str::to_owned),
            });
        }
        locals.push(local);
    }
    Ok((locals, bindings))
}
