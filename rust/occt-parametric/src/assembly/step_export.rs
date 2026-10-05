//! Structured STEP export of generated instances.

use super::*;
use occt_bridge::{StepComponent, StepFaceColor, StepNode};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Row-major rigid transform `[R | t]`.
pub(crate) type Rigid = [f64; 12];

/// A placement as a rigid transform: rotate about the axis through the
/// origin, then translate. Rodrigues' formula; O(1).
pub(crate) fn rigid(placement: Placement) -> Result<Rigid, ModelError> {
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
pub(crate) fn compose(outer: &Rigid, inner: &Rigid) -> Rigid {
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

/// The inverse rigid transform `[Rᵀ | -Rᵀt]`.
pub(crate) fn invert(m: &Rigid) -> Rigid {
    let mut inverse = [0.0; 12];
    for row in 0..3 {
        for column in 0..3 {
            inverse[4 * row + column] = m[4 * column + row];
        }
        inverse[4 * row + 3] = -(0..3).map(|k| m[4 * k + row] * m[4 * k + 3]).sum::<f64>();
    }
    inverse
}

#[cfg(test)]
pub(crate) fn rigid_for_tests(placement: Placement) -> Result<[f64; 12], ModelError> {
    rigid(placement)
}

/// Linear RGB channel to sRGB (IEC 61966-2-1), as STEP viewers expect.
fn srgb(linear: f64) -> f64 {
    if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
pub(crate) fn srgb_for_tests(linear: f64) -> f64 {
    srgb(linear)
}

impl InstanceGraph<'_> {
    /// Writes the generated `outputs` as one named STEP assembly: a component
    /// per instance, named by instance id and placed where it was generated,
    /// referring to parts shared by instances with the same local geometry.
    /// Parts are named `family/output [representative instance]` and colored
    /// from the instance material's appearance. Assembly frames holding any
    /// output become named sub-assemblies nested as the frame tree is, each
    /// placed by its frame placement and current joint motion; components
    /// are located within their frame, so model-space geometry matches the
    /// generation. Returns the number of distinct parts. O(outputs + frames
    /// on their paths + colored faces) plus the STEP write. Faces colored
    /// through the family's feature colors are written as STEP face colors.
    pub fn export_step(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        path: impl AsRef<Path>,
        assembly_name: &str,
        outputs: &OutputSet,
    ) -> Result<usize, ModelError> {
        let outputs = self.generated_outputs(generation, outputs)?;
        let mut names = Vec::with_capacity(outputs.len());
        let mut colors = Vec::with_capacity(outputs.len());
        for output in &outputs {
            let family = &self.resolve(&output.instance)?.definition.id;
            let representative = generation
                .shared_from(&output.instance)
                .unwrap_or(&output.instance);
            names.push(format!("{family}/{} [{representative}]", output.output));
            let appearance = self
                .material_of(&output.instance)?
                .and_then(|material| self.assembly.material_appearances.get(&material.id));
            colors.push(
                appearance.map(|appearance| {
                    [0, 1, 2].map(|channel| srgb(appearance.base_color[channel]))
                }),
            );
        }
        let components = outputs
            .iter()
            .zip(names.iter().zip(&colors))
            .map(|(output, (part_name, color))| {
                let shape = generation
                    .result(&output.instance)
                    .and_then(|result| result.shape(&output.output))
                    .ok_or_else(|| {
                        ModelError::new(format!(
                            "missing generated output '{}:{}'",
                            output.instance, output.output
                        ))
                    })?;
                Ok(StepComponent {
                    shape,
                    name: &output.instance,
                    part_name,
                    color: *color,
                })
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        let mut face_colors = Vec::new();
        for (component, output) in outputs.iter().enumerate() {
            let colored = generation
                .result(&output.instance)
                .and_then(|result| result.face_colors.get(&output.output));
            for (face, color) in colored.into_iter().flatten() {
                face_colors.push(StepFaceColor {
                    component,
                    face: *face,
                    color: color.map(srgb),
                });
            }
        }
        let (frames, memberships) = self.step_frames(&outputs)?;
        let nodes = frames
            .iter()
            .map(|(id, parent, transform)| StepNode {
                name: id,
                parent: *parent,
                transform: *transform,
            })
            .collect::<Vec<_>>();
        Ok(session.save_step_assembly_tree(
            path,
            assembly_name,
            &nodes,
            &components,
            &memberships,
            &face_colors,
        )?)
    }

    /// The frames enclosing any output, parents first (by depth, then id),
    /// each with its parent's index and local transform, and each output's
    /// frame index.
    #[allow(clippy::type_complexity)]
    fn step_frames(
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
