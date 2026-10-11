//! Pattern edits on instance graphs: creation, rules, counts, drivers,
//! frames, resizing, suppression, and member placement.

use super::*;
#[allow(unused_imports)]
pub use occt_parametric_core::graph::pattern_edits::*;
pub trait InstanceGraphPatternEdits<'definition> {
    /// Resolves every pattern driver and updates rule placements and stable
    /// member slots. All drivers are evaluated before the graph is mutated.
    fn refresh_driven_patterns(&mut self, session: &Session) -> Result<(), ModelError>;
}

impl<'definition> InstanceGraphPatternEdits<'definition> for InstanceGraph<'definition> {
    fn refresh_driven_patterns(&mut self, session: &Session) -> Result<(), ModelError> {
        let mut updates = Vec::new();
        for (index, pattern) in self.patterns.iter().enumerate() {
            if pattern.count_driver.is_none() && pattern.span_driver.is_none() {
                continue;
            }
            let mut rule = pattern.rule;
            if let Some(driver) = &pattern.span_driver {
                let span = self.resolve_span_driver(session, driver)?;
                match &mut rule {
                    PatternRule::LinearFit {
                        span: rule_span, ..
                    } => *rule_span = span,
                    _ => unreachable!("span-driver compatibility was validated"),
                }
            }
            rule.validate()?;
            let count = if let Some(driver) = &pattern.count_driver {
                self.resolve_count_driver(session, driver)?
            } else {
                rule.fitted_count()?.unwrap_or(pattern.slot_count)
            };
            let plan = self.plan_resize(pattern, count)?;
            updates.push((index, rule, count, plan));
        }
        for (index, rule, count, plan) in updates {
            self.patterns[index].rule = rule;
            self.apply_resize(index, count, plan);
        }
        Ok(())
    }
}

pub(crate) trait InstanceGraphPatternEditsInternal<'definition> {
    fn resolve_count_driver(
        &self,
        session: &Session,
        driver: &PatternCountDriver,
    ) -> Result<usize, ModelError>;

    fn resolve_span_driver(
        &self,
        session: &Session,
        driver: &PatternSpanDriver,
    ) -> Result<VectorQuantity, ModelError>;

    fn measure_output_extent(
        &self,
        session: &Session,
        instance: &str,
        output: &str,
        axis: CoordinateAxis,
    ) -> Result<f64, ModelError>;
}

impl<'definition> InstanceGraphPatternEditsInternal<'definition> for InstanceGraph<'definition> {
    fn resolve_count_driver(
        &self,
        session: &Session,
        driver: &PatternCountDriver,
    ) -> Result<usize, ModelError> {
        self.validate_count_driver(driver)?;
        match driver {
            PatternCountDriver::Parameter {
                instance,
                parameter,
            } => {
                let resolved = self.resolve(instance)?;
                let parameters = resolve_parameters(resolved.definition, &resolved.overrides)?;
                let ParameterValue::Integer(count) = parameters[parameter] else {
                    unreachable!("count parameter type was validated")
                };
                let count = usize::try_from(count).map_err(|_| {
                    ModelError::new(format!(
                        "pattern count parameter '{parameter}' on instance '{instance}' must be positive"
                    ))
                })?;
                if !(1..=MAX_PATTERN_MEMBERS).contains(&count) {
                    return Err(ModelError::new(format!(
                        "pattern count parameter '{parameter}' on instance '{instance}' must be 1..={MAX_PATTERN_MEMBERS}"
                    )));
                }
                Ok(count)
            }
            PatternCountDriver::BoundsExtent {
                instance,
                output,
                axis,
                maximum_spacing,
            } => LinearSpacing::Maximum(*maximum_spacing)
                .count(self.measure_output_extent(session, instance, output, *axis)?),
        }
    }

    fn resolve_span_driver(
        &self,
        session: &Session,
        driver: &PatternSpanDriver,
    ) -> Result<VectorQuantity, ModelError> {
        self.validate_span_driver(driver)?;
        let (length, direction) = match driver {
            PatternSpanDriver::Parameter {
                instance,
                parameter,
                direction,
            } => {
                let resolved = self.resolve(instance)?;
                let parameters = resolve_parameters(resolved.definition, &resolved.overrides)?;
                let ParameterValue::Scalar(value) = &parameters[parameter] else {
                    unreachable!("span parameter type was validated")
                };
                (value.normalized()?, *direction)
            }
            PatternSpanDriver::BoundsExtent {
                instance,
                output,
                axis,
                direction,
            } => (
                self.measure_output_extent(session, instance, output, *axis)?,
                *direction,
            ),
        };
        if !length.is_finite() || length <= 0.0 {
            return Err(ModelError::new(
                "driven pattern span must be positive and finite",
            ));
        }
        let direction = normalized_pattern_direction(direction)?;
        Ok(VectorQuantity::lengths(
            direction.x * length,
            direction.y * length,
            direction.z * length,
            LengthUnit::Millimeter,
        ))
    }

    fn measure_output_extent(
        &self,
        session: &Session,
        instance: &str,
        output: &str,
        axis: CoordinateAxis,
    ) -> Result<f64, ModelError> {
        self.validate_measurement_target(instance, output)?;
        let generated = self
            .resolve_with_placement(instance)?
            .regenerate(session)
            .map_err(|error| {
                ModelError::new(format!(
                    "measure pattern geometry from instance '{instance}': {}",
                    error.message
                ))
            })?;
        let measured: Result<f64, ModelError> = (|| {
            let shape = generated.shape(output).ok_or_else(|| {
                ModelError::new(format!(
                    "instance '{instance}' did not generate pattern measurement output '{output}'"
                ))
            })?;
            // Tolerance-padded bounds overstate lengths and would tip exact
            // multiples of a spacing into an extra member.
            let bounds = session.exact_bounds(shape).map_err(ModelError::from)?;
            Ok(match axis {
                CoordinateAxis::X => bounds.max.x - bounds.min.x,
                CoordinateAxis::Y => bounds.max.y - bounds.min.y,
                CoordinateAxis::Z => bounds.max.z - bounds.min.z,
            })
        })();
        cleanup(session, generated.shapes);
        measured
    }
}
