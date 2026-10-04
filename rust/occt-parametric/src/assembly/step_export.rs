//! Structured STEP export of generated instances.

use super::*;
use occt_bridge::StepComponent;
use std::path::Path;

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
    /// from the instance material's appearance. Assembly frames are applied
    /// to each component's placement rather than written as sub-assemblies.
    /// Returns the number of distinct parts. O(outputs) plus the STEP write.
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
        Ok(session.save_step_assembly(path, assembly_name, &components)?)
    }
}
