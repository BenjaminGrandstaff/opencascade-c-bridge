//! Assembly requirements: mass, clearance, datum, and relationship rules,
//! validated and verified against regenerated instances.

use super::*;

pub fn validate_range(
    minimum: f64,
    maximum: f64,
    kind: &str,
    unit: &str,
) -> Result<(), ModelError> {
    if !(minimum.is_finite() && maximum.is_finite() && minimum >= 0.0 && minimum <= maximum) {
        return Err(ModelError::new(format!(
            "{kind} range must be finite, nonnegative, and ordered in {unit}"
        )));
    }
    Ok(())
}

pub fn minimum_clearance(value: Quantity) -> Result<f64, ModelError> {
    if value.dimension != Dimension::Length {
        return Err(ModelError::new("minimum clearance must be a length"));
    }
    let value = value.normalized()?;
    if !(value.is_finite() && value >= 0.0) {
        return Err(ModelError::new(
            "minimum clearance must be finite and nonnegative",
        ));
    }
    Ok(value)
}

pub fn clearance_value(value: Quantity, bound: &str) -> Result<f64, ModelError> {
    if value.dimension != Dimension::Length {
        return Err(ModelError::new(format!(
            "datum clearance {bound} must be a length"
        )));
    }
    let value = value.normalized()?;
    if !(value.is_finite() && value >= 0.0) {
        return Err(ModelError::new(format!(
            "datum clearance {bound} must be finite and nonnegative"
        )));
    }
    Ok(value)
}

impl<'definition> InstanceGraph<'definition> {
    // ---- assembly requirements

    /// Adds a graph-level requirement after validating all of its references
    /// and units against the current model.
    pub fn add_assembly_requirement(
        &mut self,
        requirement: AssemblyRequirement,
    ) -> Result<(), ModelError> {
        self.add_assembly_requirements([requirement])
    }

    /// Atomically validates and adds a batch in O(existing + new requirements
    /// times referenced clone depth). No requirement is added on failure.
    pub fn add_assembly_requirements(
        &mut self,
        requirements: impl IntoIterator<Item = AssemblyRequirement>,
    ) -> Result<(), ModelError> {
        let requirements = requirements.into_iter().collect::<Vec<_>>();
        let mut ids = self
            .assembly
            .requirements
            .iter()
            .map(|requirement| requirement.id.as_str())
            .collect::<HashSet<_>>();
        for requirement in &requirements {
            if requirement.id.is_empty() || !ids.insert(&requirement.id) {
                return Err(ModelError::new(
                    "assembly requirement ids must be nonempty, versioned, and unique",
                ));
            }
            self.validate_assembly_requirement(requirement)?;
        }
        self.assembly.requirements.extend(requirements);
        Ok(())
    }

    pub fn remove_assembly_requirement(
        &mut self,
        id: &str,
    ) -> Result<AssemblyRequirement, ModelError> {
        let index = self
            .assembly
            .requirements
            .iter()
            .position(|requirement| requirement.id == id)
            .ok_or_else(|| ModelError::new(format!("unknown assembly requirement '{id}'")))?;
        Ok(self.assembly.requirements.remove(index))
    }

    pub fn validate_assembly_requirement(
        &self,
        requirement: &AssemblyRequirement,
    ) -> Result<(), ModelError> {
        if requirement.id.is_empty() || requirement.version == 0 {
            return Err(ModelError::new(
                "assembly requirement ids must be nonempty, versioned, and unique",
            ));
        }
        match &requirement.rule {
            AssemblyVerificationRule::MassRange {
                instance,
                output,
                minimum_kilograms,
                maximum_kilograms,
            } => {
                validate_range(*minimum_kilograms, *maximum_kilograms, "mass", "kg")?;
                let resolved = self.resolve(instance)?;
                if !resolved
                    .definition
                    .features
                    .iter()
                    .any(|feature| feature.id == *output)
                {
                    return Err(ModelError::new(format!(
                        "assembly requirement '{}' references unknown output '{output}' on instance '{instance}'",
                        requirement.id
                    )));
                }
                if self.material_of(instance)?.is_none() {
                    return Err(ModelError::new(format!(
                        "assembly requirement '{}' needs a material on instance '{instance}'",
                        requirement.id
                    )));
                }
            }
            AssemblyVerificationRule::DatumClearance {
                first,
                second,
                minimum,
                maximum,
            } => {
                let minimum = clearance_value(*minimum, "minimum")?;
                let maximum = maximum
                    .map(|value| clearance_value(value, "maximum"))
                    .transpose()?;
                if maximum.is_some_and(|maximum| minimum > maximum) {
                    return Err(ModelError::new("datum clearance minimum exceeds maximum"));
                }
                separation(
                    self.datum(&first.instance, &first.datum)?,
                    self.datum(&second.instance, &second.datum)?,
                )?;
            }
            AssemblyVerificationRule::RelationshipSatisfied { relationship } => {
                if !self
                    .assembly
                    .relationships
                    .iter()
                    .any(|candidate| candidate.id == *relationship)
                {
                    return Err(ModelError::new(format!(
                        "assembly requirement '{}' references unknown relationship '{relationship}'",
                        requirement.id
                    )));
                }
            }
            AssemblyVerificationRule::NoInterference { outputs } => {
                self.validate_output_set(outputs)?;
            }
            AssemblyVerificationRule::MinimumClearance {
                first,
                second,
                minimum,
            } => {
                minimum_clearance(*minimum)?;
                self.validate_output_set(first)?;
                if let Some(second) = second {
                    self.validate_output_set(second)?;
                }
            }
        }
        Ok(())
    }

    /// Explicit sets: nonempty, one output per instance, each a declared
    /// feature of its instance's family. O(set size times clone depth).
    fn validate_output_set(&self, set: &OutputSet) -> Result<(), ModelError> {
        match set {
            OutputSet::Explicit(outputs) => {
                if outputs.is_empty() {
                    return Err(ModelError::new("an explicit output set must not be empty"));
                }
                let mut instances = HashSet::with_capacity(outputs.len());
                for output in outputs {
                    if !instances.insert(output.instance.as_str()) {
                        return Err(ModelError::new(format!(
                            "output set names instance '{}' more than once",
                            output.instance
                        )));
                    }
                    let resolved = self.resolve(&output.instance)?;
                    if !resolved
                        .definition
                        .features
                        .iter()
                        .any(|feature| feature.id == output.output)
                    {
                        return Err(ModelError::new(format!(
                            "unknown output '{}' on instance '{}'",
                            output.output, output.instance
                        )));
                    }
                }
            }
            OutputSet::AllWithOutput(output) => {
                if output.is_empty() {
                    return Err(ModelError::new("output set needs a nonempty output name"));
                }
            }
        }
        Ok(())
    }

    /// Contact within the model's linear tolerance counts as touching, not
    /// interference or a clearance shortfall.
    pub fn collision_options(&self, minimum_clearance: Quantity) -> CollisionOptions {
        CollisionOptions {
            minimum_clearance,
            contact_tolerance: Quantity::length(
                self.assembly.tolerances.linear_millimeters,
                LengthUnit::Millimeter,
            ),
            overlap_volume_tolerance_mm3: 0.0,
        }
    }
}
