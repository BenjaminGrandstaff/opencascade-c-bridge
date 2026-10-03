//! Bounded sampled motion with shared local geometry and exact collision checks.

use super::*;

pub const MAX_MOTION_SAMPLES: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JointPosition {
    pub frame: String,
    pub coordinate: JointDof,
    pub value: Quantity,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionSample {
    pub positions: Vec<JointPosition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionStudy {
    pub samples: Vec<MotionSample>,
    /// One final solid output per participating instance.
    pub outputs: Vec<InstanceOutputRef>,
    pub collision_options: CollisionOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MotionSampleResult {
    pub index: usize,
    pub positions: Vec<JointPosition>,
    pub collisions: Vec<PairCheck>,
    pub relationships: Vec<RelationshipCheck>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MotionResult {
    pub samples: Vec<MotionSampleResult>,
    pub generated_variants: usize,
}

impl MotionStudy {
    /// Includes both endpoints. Translation units normalize to mm; angles are
    /// dimensionless radians. O(samples) time/storage; at most 10,000 samples.
    pub fn linear(
        frame: impl Into<String>,
        coordinate: JointDof,
        start: Quantity,
        end: Quantity,
        sample_count: usize,
        outputs: Vec<InstanceOutputRef>,
        collision_options: CollisionOptions,
    ) -> Result<Self, ModelError> {
        if !(2..=MAX_MOTION_SAMPLES).contains(&sample_count) || start.dimension != end.dimension {
            return Err(ModelError::new(
                "linear motion needs 2–10000 samples and compatible endpoint units",
            ));
        }
        let dimension = start.dimension;
        let start = start.normalized()?;
        let end = end.normalized()?;
        if !start.is_finite() || !end.is_finite() {
            return Err(ModelError::new("motion endpoints must be finite"));
        }
        let frame = frame.into();
        let mut samples = Vec::with_capacity(sample_count);
        for index in 0..sample_count {
            let fraction = index as f64 / (sample_count - 1) as f64;
            let value = (1.0 - fraction) * start + fraction * end;
            let value = match dimension {
                Dimension::Scalar => Quantity::scalar(value),
                Dimension::Length => Quantity::length(value, LengthUnit::Millimeter),
            };
            samples.push(MotionSample {
                positions: vec![JointPosition {
                    frame: frame.clone(),
                    coordinate,
                    value,
                }],
            });
        }
        Ok(Self {
            samples,
            outputs,
            collision_options,
        })
    }
}

fn sample_graph<'definition>(
    graph: &InstanceGraph<'definition>,
    sample: &MotionSample,
    index: usize,
) -> Result<InstanceGraph<'definition>, ModelError> {
    let mut candidate = graph.clone();
    let mut seen = HashSet::new();
    for position in &sample.positions {
        if !seen.insert((&position.frame, position.coordinate)) {
            return Err(ModelError::new(format!(
                "motion sample {index} repeats a joint coordinate"
            )));
        }
        candidate
            .set_joint_coordinate(&position.frame, position.coordinate, position.value)
            .map_err(|mut error| {
                error.message = format!("motion sample {index}: {}", error.message);
                error
            })?;
    }
    Ok(candidate)
}

struct Binding {
    instance: String,
    group: usize,
    placement: Placement,
    frame: Option<String>,
}

impl InstanceGraph<'_> {
    /// Generates each parameter variant once, then places shared copies for each
    /// sample. The source graph and accepted generations stay unchanged. All
    /// samples/limits validate before kernel work. O(variants' generation +
    /// samples * (graph/joint copy + participating frame paths + BVH/pair work));
    /// handle storage is O(local outputs + one sample's outputs), independent
    /// of sample count. Reports sampled contacts, not continuous collision proof.
    pub fn run_motion_study(
        &self,
        session: &Session,
        study: &MotionStudy,
    ) -> Result<MotionResult, ModelError> {
        if study.samples.is_empty()
            || study.samples.len() > MAX_MOTION_SAMPLES
            || study.outputs.is_empty()
        {
            return Err(ModelError::new(
                "motion needs 1–10000 samples and participating solid outputs",
            ));
        }
        self.validate_joints()?;
        // Validate collision options even when a one-body study has no pairs.
        CollisionOptions::validate(study.collision_options)?;
        for (index, sample) in study.samples.iter().enumerate() {
            sample_graph(self, sample, index)?;
        }
        let ids = study
            .outputs
            .iter()
            .map(|output| output.instance.as_str())
            .collect::<Vec<_>>();
        let groups = self.group_by_parameters(&ids)?;
        let mut locals = Vec::with_capacity(groups.len());
        let mut bindings = Vec::with_capacity(ids.len());
        for (group, members) in groups.iter().enumerate() {
            let local = members[0].1.instance.regenerate(session)?;
            for (id, resolved) in members {
                bindings.push(Binding {
                    instance: (*id).to_owned(),
                    group,
                    placement: resolved.placement,
                    frame: self
                        .nodes
                        .get(*id)
                        .and_then(|node| node.frame())
                        .map(str::to_owned),
                });
            }
            locals.push(local);
        }
        let mut samples = Vec::with_capacity(study.samples.len());
        for (index, sample) in study.samples.iter().enumerate() {
            let candidate = sample_graph(self, sample, index)?;
            let generation = motion_generation(session, &candidate, &bindings, &locals)?;
            let collisions =
                generation.check_collisions(session, &study.outputs, study.collision_options)?;
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
