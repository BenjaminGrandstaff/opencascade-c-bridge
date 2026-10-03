//! Assembly requirements: mass, clearance, datum, and relationship rules,
//! validated and verified against regenerated instances.

use super::*;

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

    pub(crate) fn validate_assembly_requirement(
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
        }
        Ok(())
    }

    pub(crate) fn verify_assembly_requirements(
        &self,
        session: &Session,
        results: &HashMap<String, GeneratedResult<'_>>,
    ) -> Result<Vec<VerificationResult>, ModelError> {
        let mut verification = Vec::with_capacity(self.assembly.requirements.len());
        let mut required_failures = Vec::new();
        for requirement in &self.assembly.requirements {
            let result = self
                .verify_assembly_requirement(session, results, requirement)
                .map_err(|error| {
                    ModelError::new(format!(
                        "assembly requirement '{}': {}",
                        requirement.id, error.message
                    ))
                })?;
            if requirement.priority == RequirementPriority::Required
                && result.status == VerificationStatus::Failed
            {
                required_failures.push(requirement.id.clone());
            }
            verification.push(result);
        }
        if !required_failures.is_empty() {
            return Err(ModelError::new(format!(
                "required assembly verification failed: {}",
                required_failures.join(", ")
            )));
        }
        Ok(verification)
    }

    pub(crate) fn verify_assembly_requirement(
        &self,
        session: &Session,
        results: &HashMap<String, GeneratedResult<'_>>,
        requirement: &AssemblyRequirement,
    ) -> Result<VerificationResult, ModelError> {
        let (passed, message) = match &requirement.rule {
            AssemblyVerificationRule::MassRange {
                instance,
                output,
                minimum_kilograms,
                maximum_kilograms,
            } => {
                let result = results.get(instance).ok_or_else(|| {
                    ModelError::new(format!("instance '{instance}' was not generated"))
                })?;
                let shape = result.shape(output).ok_or_else(|| {
                    ModelError::new(format!(
                        "instance '{instance}' has no generated output '{output}'"
                    ))
                })?;
                let density = self
                    .material_of(instance)?
                    .ok_or_else(|| {
                        ModelError::new(format!("instance '{instance}' has no material"))
                    })?
                    .density_kg_per_cubic_meter;
                let mass = session.volume(shape)? * CUBIC_MILLIMETERS_TO_CUBIC_METERS * density;
                (
                    mass >= *minimum_kilograms && mass <= *maximum_kilograms,
                    format!(
                        "mass {mass} kg; expected {minimum_kilograms}..={maximum_kilograms} kg"
                    ),
                )
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
                let (clearance, angular) = separation(
                    self.datum(&first.instance, &first.datum)?,
                    self.datum(&second.instance, &second.datum)?,
                )?;
                let parallel =
                    angular.is_none_or(|angle| angle <= self.assembly.tolerances.angular_radians);
                let passed = parallel
                    && clearance >= minimum
                    && maximum.is_none_or(|maximum| clearance <= maximum);
                let expected = maximum
                    .map(|maximum| format!("{minimum}..={maximum}"))
                    .unwrap_or_else(|| format!(">={minimum}"));
                let angular = angular
                    .map(|angle| format!("; angular deviation {angle} rad"))
                    .unwrap_or_default();
                (
                    passed,
                    format!("datum clearance {clearance} mm; expected {expected} mm{angular}"),
                )
            }
            AssemblyVerificationRule::RelationshipSatisfied { relationship } => {
                let relationship = self
                    .assembly
                    .relationships
                    .iter()
                    .find(|candidate| candidate.id == *relationship)
                    .ok_or_else(|| {
                        ModelError::new(format!("unknown relationship '{relationship}'"))
                    })?;
                let check = self.check_relationship(relationship)?;
                (
                    check.satisfied,
                    format!(
                        "relationship '{}' {}; linear residual {:?} mm; angular residual {:?} rad",
                        relationship.id,
                        if check.satisfied {
                            "is satisfied"
                        } else {
                            "is violated"
                        },
                        check.linear_residual,
                        check.angular_residual
                    ),
                )
            }
        };
        Ok(VerificationResult {
            requirement_id: requirement.id.clone(),
            status: if passed {
                VerificationStatus::Passed
            } else {
                VerificationStatus::Failed
            },
            message,
        })
    }
}

pub(crate) fn validate_range(
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

pub(crate) fn clearance_value(value: Quantity, bound: &str) -> Result<f64, ModelError> {
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
