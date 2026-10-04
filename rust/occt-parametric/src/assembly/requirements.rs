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

    /// Generated members of a set, in deterministic order. Suppressed
    /// instances are skipped; a set with no generated members fails, so a
    /// misspelled output cannot pass vacuously.
    pub(crate) fn generated_outputs(
        &self,
        generation: &GraphRegeneration<'_>,
        set: &OutputSet,
    ) -> Result<Vec<InstanceOutputRef>, ModelError> {
        let outputs = match set {
            OutputSet::Explicit(outputs) => outputs
                .iter()
                .filter(|output| !self.is_suppressed(&output.instance))
                .cloned()
                .collect::<Vec<_>>(),
            OutputSet::AllWithOutput(output) => {
                let mut outputs = generation
                    .results
                    .iter()
                    .filter(|(_, result)| result.shape(output).is_some())
                    .map(|(instance, _)| InstanceOutputRef {
                        instance: instance.clone(),
                        output: output.clone(),
                    })
                    .collect::<Vec<_>>();
                outputs.sort_unstable();
                outputs
            }
        };
        if outputs.is_empty() {
            return Err(ModelError::new("output set matches no generated output"));
        }
        Ok(outputs)
    }

    pub(crate) fn verify_assembly_requirements(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
    ) -> Result<Vec<VerificationResult>, ModelError> {
        let mut verification = Vec::with_capacity(self.assembly.requirements.len());
        let mut required_failures = Vec::new();
        for requirement in &self.assembly.requirements {
            let result = self
                .verify_assembly_requirement(session, generation, requirement)
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
        generation: &GraphRegeneration<'_>,
        requirement: &AssemblyRequirement,
    ) -> Result<VerificationResult, ModelError> {
        let id = requirement.id.as_str();
        Ok(match &requirement.rule {
            AssemblyVerificationRule::MassRange {
                instance,
                output,
                minimum_kilograms,
                maximum_kilograms,
            } => {
                let result = generation.result(instance).ok_or_else(|| {
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
                VerificationResult::exact(
                    id,
                    mass >= *minimum_kilograms && mass <= *maximum_kilograms,
                    format!(
                        "mass {mass} kg; expected {minimum_kilograms}..={maximum_kilograms} kg"
                    ),
                )
                .measured(Measurement {
                    value: mass,
                    unit: MeasurementUnit::Kilogram,
                    minimum: Some(*minimum_kilograms),
                    maximum: Some(*maximum_kilograms),
                })
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
                VerificationResult::exact(
                    id,
                    passed,
                    format!("datum clearance {clearance} mm; expected {expected} mm{angular}"),
                )
                .measured(Measurement {
                    value: clearance,
                    unit: MeasurementUnit::Millimeter,
                    minimum: Some(minimum),
                    maximum,
                })
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
                VerificationResult::exact(
                    id,
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
            AssemblyVerificationRule::NoInterference { outputs } => {
                self.verify_no_interference(session, generation, id, outputs)?
            }
            AssemblyVerificationRule::MinimumClearance {
                first,
                second,
                minimum,
            } => self.verify_minimum_clearance(
                session,
                generation,
                id,
                first,
                second.as_ref(),
                *minimum,
            )?,
        })
    }

    fn verify_no_interference(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        id: &str,
        outputs: &OutputSet,
    ) -> Result<VerificationResult, ModelError> {
        let outputs = self.generated_outputs(generation, outputs)?;
        let checks = generation.check_collisions(
            session,
            &outputs,
            self.collision_options(Quantity::length(0.0, LengthUnit::Millimeter)),
        )?;
        let interfering = checks
            .iter()
            .filter(|check| check.status == PairStatus::Interference)
            .collect::<Vec<_>>();
        let worst = interfering
            .iter()
            .max_by(|a, b| a.overlap_volume_mm3.total_cmp(&b.overlap_volume_mm3));
        let result = VerificationResult::exact(
            id,
            interfering.is_empty(),
            match worst {
                Some(worst) => format!(
                    "{} interfering pair(s) among {} outputs; largest overlap {} mm^3",
                    interfering.len(),
                    outputs.len(),
                    worst.overlap_volume_mm3
                ),
                None => format!("no interference among {} outputs", outputs.len()),
            },
        )
        .measured(Measurement {
            value: worst.map_or(0.0, |worst| worst.overlap_volume_mm3),
            unit: MeasurementUnit::CubicMillimeter,
            minimum: None,
            maximum: None,
        });
        Ok(match worst {
            Some(worst) => result.witnessed(pair_witness(worst)),
            None => result,
        })
    }

    fn verify_minimum_clearance(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        id: &str,
        first: &OutputSet,
        second: Option<&OutputSet>,
        minimum: Quantity,
    ) -> Result<VerificationResult, ModelError> {
        let limit = minimum_clearance(minimum)?;
        let first = self.generated_outputs(generation, first)?;
        let options = self.collision_options(minimum);
        let (checks, scope) = match second {
            Some(second) => {
                let second = self.generated_outputs(generation, second)?;
                let checks =
                    generation.check_collisions_between(session, &first, &second, options)?;
                (
                    checks,
                    format!("{} x {} outputs", first.len(), second.len()),
                )
            }
            None => (
                generation.check_collisions(session, &first, options)?,
                format!("{} outputs", first.len()),
            ),
        };
        let violations = checks
            .iter()
            .filter(|check| {
                matches!(
                    check.status,
                    PairStatus::Interference | PairStatus::InsufficientClearance
                )
            })
            .collect::<Vec<_>>();
        let worst = violations.iter().min_by(|a, b| {
            a.separation_mm
                .total_cmp(&b.separation_mm)
                .then(b.overlap_volume_mm3.total_cmp(&a.overlap_volume_mm3))
        });
        Ok(match worst {
            Some(worst) => VerificationResult::exact(
                id,
                false,
                format!(
                    "{} pair(s) closer than {limit} mm among {scope}; closest {} mm",
                    violations.len(),
                    worst.separation_mm
                ),
            )
            .measured(Measurement {
                value: worst.separation_mm,
                unit: MeasurementUnit::Millimeter,
                minimum: Some(limit),
                maximum: None,
            })
            .witnessed(pair_witness(worst)),
            None => VerificationResult::exact(
                id,
                true,
                format!("no pair closer than {limit} mm among {scope}"),
            ),
        })
    }

    /// Contact within the model's linear tolerance counts as touching, not
    /// interference or a clearance shortfall.
    fn collision_options(&self, minimum_clearance: Quantity) -> CollisionOptions {
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

fn minimum_clearance(value: Quantity) -> Result<f64, ModelError> {
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

fn pair_witness(check: &PairCheck) -> Witness {
    let name = |output: &InstanceOutputRef| format!("{}:{}", output.instance, output.output);
    Witness {
        subjects: vec![name(&check.first), name(&check.second)],
        points_mm: vec![check.first_witness_mm, check.second_witness_mm],
    }
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
