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
    /// Explicit unordered pairs omitted from collision checks at every pose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_pairs: Vec<CollisionPairRef>,
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
            excluded_pairs: Vec::new(),
        })
    }
}

pub fn sample_graph<'definition>(
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

pub fn validate_study(
    graph: &InstanceGraph<'_>,
    study: &MotionStudy,
) -> Result<super::collisions::PairExclusions, ModelError> {
    if study.samples.is_empty()
        || study.samples.len() > MAX_MOTION_SAMPLES
        || study.outputs.is_empty()
    {
        return Err(ModelError::new(
            "motion needs 1–10000 samples and participating solid outputs",
        ));
    }
    let exclusions = super::collisions::PairExclusions::new(&study.outputs, &study.excluded_pairs)?;
    graph.validate_joints()?;
    // Validate collision options even when a one-body study has no pairs.
    CollisionOptions::validate(study.collision_options)?;
    for (index, sample) in study.samples.iter().enumerate() {
        sample_graph(graph, sample, index)?;
    }
    Ok(exclusions)
}

pub struct Binding {
    pub instance: String,
    pub group: usize,
    pub placement: Placement,
    pub frame: Option<String>,
}

pub mod closed;
pub use closed::*;

pub mod continuous;
pub use continuous::*;
