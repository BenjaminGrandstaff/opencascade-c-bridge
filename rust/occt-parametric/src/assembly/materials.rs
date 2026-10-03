//! Materials: definitions, assignment with inheritance, and mass.

use super::*;

impl<'definition> InstanceGraph<'definition> {
    // ---- materials

    pub fn add_material(&mut self, material: Material) -> Result<(), ModelError> {
        validate_material(&material)?;
        if self
            .assembly
            .materials
            .iter()
            .any(|existing| existing.id == material.id)
        {
            return Err(ModelError::new("material ids must be unique"));
        }
        self.assembly.materials.push(material);
        Ok(())
    }

    /// Assigns a material to an instance, or clears its own assignment so it
    /// inherits from its clone source again.
    pub fn assign_material(
        &mut self,
        instance: &str,
        material: Option<&str>,
    ) -> Result<(), ModelError> {
        self.require_instance(instance)?;
        let previous = self.assembly.material_assignments.get(instance).cloned();
        match material {
            Some(id) => {
                self.material(id)?;
                self.assembly
                    .material_assignments
                    .insert(instance.to_owned(), id.to_owned());
            }
            None => {
                self.assembly.material_assignments.remove(instance);
            }
        }
        if let Err(error) = self
            .assembly
            .requirements
            .iter()
            .try_for_each(|requirement| self.validate_assembly_requirement(requirement))
        {
            match previous {
                Some(material) => {
                    self.assembly
                        .material_assignments
                        .insert(instance.to_owned(), material);
                }
                None => {
                    self.assembly.material_assignments.remove(instance);
                }
            }
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn material(&self, id: &str) -> Result<&Material, ModelError> {
        self.assembly
            .materials
            .iter()
            .find(|material| material.id == id)
            .ok_or_else(|| ModelError::new(format!("unknown material '{id}'")))
    }

    /// The instance's own material, or the nearest one along its clone sources.
    pub fn material_of(&self, instance: &str) -> Result<Option<&Material>, ModelError> {
        let mut current = instance;
        for _ in 0..=self.nodes.len() {
            if let Some(id) = self.assembly.material_assignments.get(current) {
                return self.material(id).map(Some);
            }
            match self.nodes.get(current) {
                Some(InstanceNode::Clone { source, .. }) => current = source,
                Some(InstanceNode::Base { .. }) => return Ok(None),
                None => return Err(ModelError::new(format!("unknown instance '{current}'"))),
            }
        }
        Err(ModelError::new(format!(
            "clone cycle while resolving material of '{instance}'"
        )))
    }

    /// Mass in kilograms of one generated output: its volume times the
    /// instance's material density.
    pub fn mass(&self, session: &Session, instance: &str, output: &str) -> Result<f64, ModelError> {
        let density = self
            .material_of(instance)?
            .ok_or_else(|| ModelError::new(format!("instance '{instance}' has no material")))?
            .density_kg_per_cubic_meter;
        let generated = self.resolve(instance)?.regenerate(session)?;
        let volume = generated
            .shape(output)
            .ok_or_else(|| {
                ModelError::new(format!("instance '{instance}' has no output '{output}'"))
            })
            .and_then(|shape| session.volume(shape).map_err(ModelError::from));
        cleanup(session, generated.shapes);
        Ok(volume? * CUBIC_MILLIMETERS_TO_CUBIC_METERS * density)
    }

    /// Keeps an inherited material when an instance stops inheriting.
    pub(crate) fn pin_material(&mut self, instance: &str, inherited: Option<String>) {
        if let Some(material) = inherited {
            self.assembly
                .material_assignments
                .entry(instance.to_owned())
                .or_insert(material);
        }
    }
}

pub(crate) const CUBIC_MILLIMETERS_TO_CUBIC_METERS: f64 = 1e-9;

pub(crate) fn validate_material(material: &Material) -> Result<(), ModelError> {
    if material.id.is_empty() || material.name.is_empty() {
        return Err(ModelError::new("material id and name are required"));
    }
    if !(material.density_kg_per_cubic_meter.is_finite()
        && material.density_kg_per_cubic_meter > 0.0)
    {
        return Err(ModelError::new(format!(
            "material '{}' density must be finite and positive",
            material.id
        )));
    }
    Ok(())
}
