//! Structured STEP export of generated instances.

use super::*;
use occt_bridge::{StepComponent, StepFaceColor, StepNode, StepPmi, StepPmiVerification};
#[allow(unused_imports)]
pub use occt_parametric_core::assembly::step_export::*;
use std::path::Path;

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
pub trait InstanceGraphStepExport {
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
    fn export_step(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        path: impl AsRef<Path>,
        assembly_name: &str,
        outputs: &OutputSet,
    ) -> Result<usize, ModelError>;
}

impl InstanceGraphStepExport for InstanceGraph<'_> {
    fn export_step(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        path: impl AsRef<Path>,
        assembly_name: &str,
        outputs: &OutputSet,
    ) -> Result<usize, ModelError> {
        let outputs = self.generated_outputs(generation, outputs)?;
        Ok(self
            .write_step(
                session,
                generation,
                path.as_ref(),
                assembly_name,
                &outputs,
                None,
            )?
            .0)
    }
}

pub(crate) trait InstanceGraphStepWrite {
    /// [`InstanceGraphStepExport::export_step`] of resolved outputs, one
    /// component each in order, with semantic PMI on their parts when given.
    fn write_step(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        path: &Path,
        assembly_name: &str,
        outputs: &[InstanceOutputRef],
        pmi: Option<&StepPmi<'_>>,
    ) -> Result<(usize, StepPmiVerification), ModelError>;
}

impl InstanceGraphStepWrite for InstanceGraph<'_> {
    fn write_step(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        path: &Path,
        assembly_name: &str,
        outputs: &[InstanceOutputRef],
        pmi: Option<&StepPmi<'_>>,
    ) -> Result<(usize, StepPmiVerification), ModelError> {
        let mut names = Vec::with_capacity(outputs.len());
        let mut colors = Vec::with_capacity(outputs.len());
        for output in outputs {
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
        let (frames, memberships) = self.step_frames(outputs)?;
        let nodes = frames
            .iter()
            .map(|(id, parent, transform)| StepNode {
                name: id,
                parent: *parent,
                transform: *transform,
            })
            .collect::<Vec<_>>();
        match pmi {
            None => Ok((
                session.save_step_assembly_tree(
                    path,
                    assembly_name,
                    &nodes,
                    &components,
                    &memberships,
                    &face_colors,
                )?,
                StepPmiVerification::default(),
            )),
            Some(pmi) => Ok(session.save_step_assembly_pmi(
                path,
                assembly_name,
                &nodes,
                &components,
                &memberships,
                &face_colors,
                pmi,
            )?),
        }
    }
}
