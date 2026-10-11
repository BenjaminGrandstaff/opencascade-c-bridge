//! Structured STEP export of generated instances.

use super::*;

use std::collections::{BTreeMap, HashMap};

/// Row-major rigid transform `[R | t]`.
pub type Rigid = [f64; 12];

/// A placement as a rigid transform: rotate about the axis through the
/// origin, then translate. Rodrigues' formula; O(1).
pub fn rigid(placement: Placement) -> Result<Rigid, ModelError> {
    let placement = placement.normalized()?;
    let t = placement.translation;
    let Some((o, axis, angle)) = placement.rotation else {
        return Ok([1.0, 0.0, 0.0, t.x, 0.0, 1.0, 0.0, t.y, 0.0, 0.0, 1.0, t.z]);
    };
    let n = axis.x.hypot(axis.y.hypot(axis.z));
    let (x, y, z) = (axis.x / n, axis.y / n, axis.z / n);
    let (s, c) = angle.sin_cos();
    let k = 1.0 - c;
    let r = [
        [c + x * x * k, x * y * k - z * s, x * z * k + y * s],
        [y * x * k + z * s, c + y * y * k, y * z * k - x * s],
        [z * x * k - y * s, z * y * k + x * s, c + z * z * k],
    ];
    let mut m = [0.0; 12];
    for row in 0..3 {
        let turned = r[row][0] * o.x + r[row][1] * o.y + r[row][2] * o.z;
        let origin = [o.x, o.y, o.z][row];
        let shift = [t.x, t.y, t.z][row];
        m[4 * row..4 * row + 3].copy_from_slice(&r[row]);
        m[4 * row + 3] = origin - turned + shift;
    }
    Ok(m)
}

/// `outer` after `inner`: p -> outer(inner(p)).
pub fn compose(outer: &Rigid, inner: &Rigid) -> Rigid {
    let mut m = [0.0; 12];
    for row in 0..3 {
        for column in 0..4 {
            let mut value = (0..3)
                .map(|k| outer[4 * row + k] * inner[4 * k + column])
                .sum::<f64>();
            if column == 3 {
                value += outer[4 * row + 3];
            }
            m[4 * row + column] = value;
        }
    }
    m
}

impl InstanceGraph<'_> {
    /// The frames enclosing any output, parents first (by depth, then id),
    /// each with its parent's index and local transform, and each output's
    /// frame index.
    #[allow(clippy::type_complexity)]
    pub fn step_frames(
        &self,
        outputs: &[InstanceOutputRef],
    ) -> Result<(Vec<(&str, Option<usize>, Rigid)>, Vec<Option<usize>>), ModelError> {
        let mut depths: BTreeMap<&str, usize> = BTreeMap::new();
        let mut owners = Vec::with_capacity(outputs.len());
        for output in outputs {
            let owner = self
                .nodes
                .get(output.instance.as_str())
                .ok_or_else(|| ModelError::new(format!("unknown instance '{}'", output.instance)))?
                .frame();
            owners.push(owner);
            let mut current = owner;
            while let Some(id) = current {
                if depths.contains_key(id) {
                    break;
                }
                let parent = self
                    .frames
                    .get(id)
                    .ok_or_else(|| ModelError::new(format!("unknown assembly frame '{id}'")))?
                    .parent
                    .as_deref();
                // Frames are acyclic by construction.
                let mut depth = 0;
                let mut above = parent;
                while let Some(ancestor) = above {
                    depth += 1;
                    above = self
                        .frames
                        .get(ancestor)
                        .and_then(|frame| frame.parent.as_deref());
                }
                depths.insert(id, depth);
                current = parent;
            }
        }
        let mut order = depths.keys().copied().collect::<Vec<_>>();
        order.sort_by_key(|id| (depths[id], *id));
        let index = order
            .iter()
            .enumerate()
            .map(|(position, id)| (*id, position))
            .collect::<HashMap<_, _>>();
        let mut frames = Vec::with_capacity(order.len());
        for id in &order {
            let frame = &self.frames[*id];
            let mut transform = rigid(frame.placement)?;
            if let Some(joint) = self.assembly.joints.get(*id) {
                transform = compose(&rigid(joint.motion()?)?, &transform);
            }
            let parent = frame.parent.as_deref().map(|parent| index[parent]);
            frames.push((*id, parent, transform));
        }
        let memberships = owners
            .into_iter()
            .map(|owner| owner.map(|id| index[id]))
            .collect();
        Ok((frames, memberships))
    }
}
