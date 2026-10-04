//! Geometry-based pair checks and indexed broad-phase assembly checks.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct InstanceOutputRef {
    pub instance: String,
    pub output: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CollisionOptions {
    pub minimum_clearance: Quantity,
    pub contact_tolerance: Quantity,
    /// Absolute overlap threshold in mm³, combined with operand-relative roundoff.
    pub overlap_volume_tolerance_mm3: f64,
}

impl Default for CollisionOptions {
    fn default() -> Self {
        Self {
            minimum_clearance: Quantity::length(0.0, LengthUnit::Millimeter),
            contact_tolerance: Quantity::length(1e-7, LengthUnit::Millimeter),
            overlap_volume_tolerance_mm3: 0.0,
        }
    }
}

impl CollisionOptions {
    pub(crate) fn validate(self) -> Result<(), ModelError> {
        self.checked().map(|_| ())
    }

    pub(super) fn checked(self) -> Result<(f64, f64), ModelError> {
        let length = |quantity: Quantity| {
            if quantity.dimension != Dimension::Length {
                return Err(ModelError::new(
                    "collision clearance and tolerance must be lengths",
                ));
            }
            let value = quantity.normalized()?;
            if !value.is_finite() || value < 0.0 {
                return Err(ModelError::new(
                    "collision clearance and tolerance must be finite/nonnegative",
                ));
            }
            Ok(value)
        };
        if !self.overlap_volume_tolerance_mm3.is_finite() || self.overlap_volume_tolerance_mm3 < 0.0
        {
            return Err(ModelError::new(
                "overlap volume tolerance must be finite/nonnegative",
            ));
        }
        Ok((
            length(self.minimum_clearance)?,
            length(self.contact_tolerance)?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairStatus {
    Clear,
    Touching,
    Interference,
    InsufficientClearance,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PairCheck {
    pub first: InstanceOutputRef,
    pub second: InstanceOutputRef,
    pub separation_mm: f64,
    pub overlap_volume_mm3: f64,
    pub first_witness_mm: Vec3,
    pub second_witness_mm: Vec3,
    pub status: PairStatus,
}

pub(super) struct Body<'a, 'session> {
    pub(super) reference: &'a InstanceOutputRef,
    pub(super) shape: &'a Shape<'session>,
    pub(super) bounds: occt_bridge::Bounds,
    pub(super) volume: f64,
}

impl<'session> GraphRegeneration<'session> {
    pub(super) fn bodies<'a>(
        &'a self,
        session: &Session,
        outputs: &'a [InstanceOutputRef],
    ) -> Result<Vec<Body<'a, 'session>>, ModelError> {
        let mut seen = HashSet::new();
        let mut volumes = HashMap::new();
        let mut bodies = Vec::with_capacity(outputs.len());
        for reference in outputs {
            if !seen.insert(&reference.instance) {
                return Err(ModelError::new(
                    "collision checks require one output per distinct instance",
                ));
            }
            let shape = self
                .result(&reference.instance)
                .and_then(|result| result.shape(&reference.output))
                .ok_or_else(|| {
                    ModelError::new(format!(
                        "missing generated output '{}:{}'",
                        reference.instance, reference.output
                    ))
                })?;
            let key = (
                self.shared_from(&reference.instance)
                    .unwrap_or(&reference.instance),
                reference.output.as_str(),
            );
            let volume = if let Some(volume) = volumes.get(&key) {
                *volume
            } else {
                if session.subshape_count(shape, ShapeType::Solid)? == 0
                    || !session.is_valid(shape)?
                {
                    return Err(ModelError::new(
                        "collision checks require valid solid outputs",
                    ));
                }
                let volume = session.volume(shape)?;
                volumes.insert(key, volume);
                volume
            };
            bodies.push(Body {
                reference,
                shape,
                bounds: session.exact_bounds(shape)?,
                volume,
            });
        }
        Ok(bodies)
    }

    /// Exact BREP separation and overlap of two generated, placed solids.
    /// No regeneration, persistent handles, or mutations of input geometry.
    pub fn check_pair(
        &self,
        session: &Session,
        first: &InstanceOutputRef,
        second: &InstanceOutputRef,
        options: CollisionOptions,
    ) -> Result<PairCheck, ModelError> {
        let checked = options.checked()?;
        let outputs = [first.clone(), second.clone()];
        let bodies = self.bodies(session, &outputs)?;
        inspect_pair(session, &bodies[0], &bodies[1], options, checked)
    }

    /// Reports interference, contact, and clearance violations among explicitly
    /// chosen final outputs. Median BVH construction is expected O(n log n),
    /// O(n) storage. Query cost is geometry-dependent; dense/degenerate bounds
    /// can require O(n²) candidates, inherent when all pairs may intersect.
    /// Expands bounds by the clearance threshold; exact BREP rejects false hits.
    /// Validity/volume are measured once per shared variant/output, not per pair.
    pub fn check_collisions(
        &self,
        session: &Session,
        outputs: &[InstanceOutputRef],
        options: CollisionOptions,
    ) -> Result<Vec<PairCheck>, ModelError> {
        let checked = options.checked()?;
        let bodies = self.bodies(session, outputs)?;
        if bodies.len() < 2 {
            return Ok(Vec::new());
        }
        let mut indices = (0..bodies.len()).collect::<Vec<_>>();
        let tree = Node::build(&bodies, &mut indices);
        let mut report = Vec::new();
        for (index, body) in bodies.iter().enumerate() {
            let mut candidates = Vec::new();
            tree.query(body.bounds, checked.0 + checked.1, index, &mut candidates);
            candidates.sort_unstable();
            for second in candidates {
                let check = inspect_pair(session, body, &bodies[second], options, checked)?;
                if check.status != PairStatus::Clear {
                    report.push(check);
                }
            }
        }
        Ok(report)
    }
}

pub(super) fn inspect_pair(
    session: &Session,
    first: &Body<'_, '_>,
    second: &Body<'_, '_>,
    options: CollisionOptions,
    (minimum, tolerance): (f64, f64),
) -> Result<PairCheck, ModelError> {
    let distance = session.distance(first.shape, second.shape)?;
    let overlap = if distance.distance <= tolerance {
        session.overlap_volume(first.shape, second.shape)?
    } else {
        0.0
    };
    let volume_margin = options
        .overlap_volume_tolerance_mm3
        .max(64.0 * f64::EPSILON * first.volume.max(second.volume));
    let status = if overlap > volume_margin {
        PairStatus::Interference
    } else if distance.distance + tolerance < minimum {
        PairStatus::InsufficientClearance
    } else if distance.distance <= tolerance {
        PairStatus::Touching
    } else {
        PairStatus::Clear
    };
    Ok(PairCheck {
        first: first.reference.clone(),
        second: second.reference.clone(),
        separation_mm: distance.distance,
        overlap_volume_mm3: overlap,
        first_witness_mm: distance.first,
        second_witness_mm: distance.second,
        status,
    })
}

pub(super) struct Node {
    bounds: occt_bridge::Bounds,
    kind: NodeKind,
}
enum NodeKind {
    Leaf(usize),
    Branch(Box<Node>, Box<Node>),
}

fn component(point: Vec3, axis: usize) -> f64 {
    match axis {
        0 => point.x,
        1 => point.y,
        _ => point.z,
    }
}

fn joined(first: occt_bridge::Bounds, second: occt_bridge::Bounds) -> occt_bridge::Bounds {
    occt_bridge::Bounds {
        min: Vec3::new(
            first.min.x.min(second.min.x),
            first.min.y.min(second.min.y),
            first.min.z.min(second.min.z),
        ),
        max: Vec3::new(
            first.max.x.max(second.max.x),
            first.max.y.max(second.max.y),
            first.max.z.max(second.max.z),
        ),
    }
}

fn intersects(first: occt_bridge::Bounds, second: occt_bridge::Bounds, margin: f64) -> bool {
    (0..3).all(|axis| {
        component(first.min, axis) <= component(second.max, axis) + margin
            && component(second.min, axis) <= component(first.max, axis) + margin
    })
}

impl Node {
    pub(super) fn build(bodies: &[Body<'_, '_>], indices: &mut [usize]) -> Self {
        let bounds = indices
            .iter()
            .map(|index| bodies[*index].bounds)
            .reduce(joined)
            .expect("nonempty BVH partition");
        if indices.len() == 1 {
            return Self {
                bounds,
                kind: NodeKind::Leaf(indices[0]),
            };
        }
        let axis = (0..3)
            .max_by(|a, b| {
                (component(bounds.max, *a) - component(bounds.min, *a))
                    .total_cmp(&(component(bounds.max, *b) - component(bounds.min, *b)))
            })
            .unwrap();
        let middle = indices.len() / 2;
        indices.select_nth_unstable_by(middle, |a, b| {
            let center = |index: usize| {
                let bounds = bodies[index].bounds;
                component(bounds.min, axis)
                    + 0.5 * (component(bounds.max, axis) - component(bounds.min, axis))
            };
            center(*a).total_cmp(&center(*b)).then(a.cmp(b))
        });
        let (left, right) = indices.split_at_mut(middle);
        Self {
            bounds,
            kind: NodeKind::Branch(
                Box::new(Self::build(bodies, left)),
                Box::new(Self::build(bodies, right)),
            ),
        }
    }

    pub(super) fn query(
        &self,
        bounds: occt_bridge::Bounds,
        margin: f64,
        first: usize,
        output: &mut Vec<usize>,
    ) {
        if !intersects(self.bounds, bounds, margin) {
            return;
        }
        match &self.kind {
            NodeKind::Leaf(index) => {
                if *index > first {
                    output.push(*index);
                }
            }
            NodeKind::Branch(left, right) => {
                left.query(bounds, margin, first, output);
                right.query(bounds, margin, first, output);
            }
        }
    }
}
