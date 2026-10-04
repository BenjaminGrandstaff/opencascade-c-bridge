use super::*;

impl JointSeedAxis {
    /// Inclusive finite linear grid; supports compatible mixed length units.
    pub fn linear(
        variable: JointVariable,
        start: Quantity,
        end: Quantity,
        count: usize,
    ) -> Result<Self, ModelError> {
        let a = start.normalized()?;
        let b = end.normalized()?;
        let dimension = if variable.coordinate == JointDof::Angle {
            Dimension::Scalar
        } else {
            Dimension::Length
        };
        if start.dimension != dimension
            || start.dimension != end.dimension
            || !a.is_finite()
            || !b.is_finite()
            || a == b
            || !(2..=10_000).contains(&count)
        {
            return Err(ModelError::new("invalid joint seed interval or count"));
        }
        let factor = start.unit.map_or(1.0, LengthUnit::millimeter_factor);
        let seeds = (0..count)
            .map(|index| {
                if index == 0 {
                    start
                } else if index + 1 == count {
                    end
                } else {
                    let fraction = index as f64 / (count - 1) as f64;
                    Quantity {
                        value: ((1.0 - fraction) * a + fraction * b) / factor,
                        ..start
                    }
                }
            })
            .collect();
        Ok(Self { variable, seeds })
    }
}

pub(super) fn checked_axes(
    graph: &InstanceGraph<'_>,
    axes: &[JointSeedAxis],
    length: f64,
    include_current: bool,
) -> Result<(Vec<JointVariable>, usize), ModelError> {
    if axes.is_empty()
        || axes.len() > 10_000
        || graph.assembly.relationships.is_empty()
        || graph.assembly.relationships.len() > 10_000
    {
        return Err(ModelError::new(
            "joint branch search needs 1..10000 axes and relationships",
        ));
    }
    graph.validate_joints()?;
    graph.assembly.tolerances.validate()?;
    graph.check_relationships()?;
    let mut variables = Vec::with_capacity(axes.len());
    let mut seen = HashSet::new();
    let mut product = 1usize;
    let mut candidate = graph.clone();
    for axis in axes {
        if !seen.insert(&axis.variable) || axis.seeds.is_empty() {
            return Err(ModelError::new(
                "joint seed axes must be unique and nonempty",
            ));
        }
        Variable::new(graph, &axis.variable, length)?;
        product = product
            .checked_mul(axis.seeds.len())
            .filter(|count| *count <= 10_000)
            .ok_or_else(|| ModelError::new("joint branch search seed grid exceeds 10000 starts"))?;
        let mut normalized = Vec::with_capacity(axis.seeds.len());
        for &seed in &axis.seeds {
            candidate.set_joint_coordinate(&axis.variable.frame, axis.variable.coordinate, seed)?;
            normalized.push(seed.normalized()?);
        }
        normalized.sort_by(f64::total_cmp);
        if normalized.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ModelError::new(
                "joint seed axis contains duplicate coordinates",
            ));
        }
        variables.push(axis.variable.clone());
    }
    Influence::new(graph, &variables, length)?;
    if (product + usize::from(include_current))
        .checked_mul(axes.len())
        .is_none_or(|count| count > 1_000_000)
    {
        return Err(ModelError::new(
            "joint branch search coordinate work budget exceeded",
        ));
    }
    Ok((variables, product))
}

pub(super) fn seed_graph<'definition>(
    graph: &InstanceGraph<'definition>,
    axes: &[JointSeedAxis],
    index: usize,
    include_current: bool,
) -> Result<InstanceGraph<'definition>, ModelError> {
    let mut candidate = graph.clone();
    if include_current && index == 0 {
        return Ok(candidate);
    }
    let mut index = index - usize::from(include_current);
    // First axis changes fastest. This order is deterministic and independent
    // of hash-map insertion order or the native geometry kernel.
    for axis in axes {
        let seed = axis.seeds[index % axis.seeds.len()];
        index /= axis.seeds.len();
        candidate.set_joint_coordinate(&axis.variable.frame, axis.variable.coordinate, seed)?;
    }
    Ok(candidate)
}
