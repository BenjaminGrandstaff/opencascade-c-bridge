//! Physical mass properties of explicit component outputs at their current poses.
use super::*;
#[allow(unused_imports)]
pub use occt_parametric_core::assembly::mass_properties::*;
pub trait InstanceGraphMass {
    /// Physical per-component and total properties for one explicit solid output
    /// per distinct unsuppressed instance. Uses current instance/frame/joint poses
    /// and inherited material densities. Overlapping components contribute their
    /// full masses; this sums parts rather than measuring a boolean union.
    /// Shared parameter variants regenerate once. Time is variant generation +
    /// selected frame paths + kernel integrations + O(components) aggregation;
    /// storage is generated outputs plus O(components). No retained new handles.
    fn mass_properties(
        &self,
        session: &Session,
        outputs: &[InstanceOutputRef],
    ) -> Result<AssemblyMassProperties, ModelError>;
}

impl InstanceGraphMass for InstanceGraph<'_> {
    fn mass_properties(
        &self,
        session: &Session,
        outputs: &[InstanceOutputRef],
    ) -> Result<AssemblyMassProperties, ModelError> {
        if outputs.is_empty() {
            return Err(ModelError::new(
                "assembly mass properties need at least one component",
            ));
        }
        let mut seen = HashSet::new();
        let materials = outputs
            .iter()
            .map(|output| {
                if !seen.insert(&output.instance) {
                    return Err(ModelError::new(
                        "assembly mass properties need one output per distinct instance",
                    ));
                }
                self.material_of(&output.instance)?.cloned().ok_or_else(|| {
                    ModelError::new(format!("instance '{}' has no material", output.instance))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ids = outputs
            .iter()
            .map(|output| output.instance.as_str())
            .collect::<Vec<_>>();
        let generation = self.regenerate_instances_current(session, &ids)?;
        let mut components = Vec::with_capacity(outputs.len());
        let mut total: Option<PhysicalMassProperties> = None;
        for (output, material) in outputs.iter().zip(materials) {
            let shape = generation
                .result(&output.instance)
                .and_then(|result| result.shape(&output.output))
                .ok_or_else(|| {
                    ModelError::new(format!(
                        "missing generated output '{}:{}'",
                        output.instance, output.output
                    ))
                })?;
            let properties = PhysicalMassProperties::measured(
                session.mass_properties(shape)?,
                material.density_kg_per_cubic_meter,
            )?;
            if let Some(total) = &mut total {
                total.combine(&properties)?;
            } else {
                total = Some(properties.clone());
            }
            components.push(ComponentMassProperties {
                output: output.clone(),
                material: material.id,
                properties,
            });
        }
        Ok(AssemblyMassProperties {
            components,
            total: total.expect("nonempty component list"),
            generated_variants: generation.generated_variants(),
        })
    }
}
