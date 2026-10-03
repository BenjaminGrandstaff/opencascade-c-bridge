//! Pattern edits on instance graphs: creation, rules, counts, drivers,
//! frames, resizing, suppression, and member placement.

use super::*;

impl<'definition> InstanceGraph<'definition> {
    pub fn add_linear_pattern(
        &mut self,
        id: impl Into<String>,
        member_prefix: &str,
        source: &str,
        count: usize,
        step: VectorQuantity,
        provenance: impl Into<String>,
    ) -> Result<Vec<String>, ModelError> {
        self.add_pattern(
            id,
            member_prefix,
            source,
            count,
            PatternRule::Linear { step },
            provenance,
        )
    }

    /// Adds a pattern with a freely chosen member count. Constraint-driven
    /// rules derive their count; use [`Self::add_fitted_pattern`] for them.
    pub fn add_pattern(
        &mut self,
        id: impl Into<String>,
        member_prefix: &str,
        source: &str,
        count: usize,
        rule: PatternRule,
        provenance: impl Into<String>,
    ) -> Result<Vec<String>, ModelError> {
        if rule.fitted_count()?.is_some() {
            return Err(ModelError::new(
                "constraint-driven rules derive their count; use add_fitted_pattern",
            ));
        }
        self.insert_pattern(
            id.into(),
            member_prefix,
            source,
            count,
            rule,
            provenance.into(),
        )
    }

    /// Adds a pattern whose member count and spacing are solved from a
    /// `LinearFit` or `CircularFit` rule.
    pub fn add_fitted_pattern(
        &mut self,
        id: impl Into<String>,
        member_prefix: &str,
        source: &str,
        rule: PatternRule,
        provenance: impl Into<String>,
    ) -> Result<Vec<String>, ModelError> {
        let count = rule
            .fitted_count()?
            .ok_or_else(|| ModelError::new("rule has no count constraint; use add_pattern"))?;
        self.insert_pattern(
            id.into(),
            member_prefix,
            source,
            count,
            rule,
            provenance.into(),
        )
    }

    pub(crate) fn insert_pattern(
        &mut self,
        id: String,
        member_prefix: &str,
        source: &str,
        count: usize,
        rule: PatternRule,
        provenance: String,
    ) -> Result<Vec<String>, ModelError> {
        if id.is_empty() || self.patterns.iter().any(|pattern| pattern.id == id) {
            return Err(ModelError::new("pattern id must be nonempty and unique"));
        }
        if count == 0 || member_prefix.is_empty() {
            return Err(ModelError::new("pattern requires members and a prefix"));
        }
        if !self.nodes.contains_key(source) {
            return Err(ModelError::new(format!("unknown clone source '{source}'")));
        }
        rule.validate()?;
        let members = (0..count)
            .map(|index| format!("{member_prefix}[{index}]"))
            .collect::<Vec<_>>();
        if members.iter().any(|member| self.nodes.contains_key(member)) {
            return Err(ModelError::new("pattern member id already exists"));
        }
        for (index, member) in members.iter().enumerate() {
            self.nodes.insert(
                member.clone(),
                InstanceNode::Clone {
                    id: member.clone(),
                    source: source.into(),
                    overrides: HashMap::new(),
                    placement: rule.member_placement(index, count),
                    frame: None,
                    provenance: provenance.clone(),
                },
            );
        }
        self.patterns.push(Pattern {
            id,
            source: source.into(),
            members: members
                .iter()
                .enumerate()
                .map(|(index, member)| PatternMember {
                    id: member.clone(),
                    index,
                    placement_override: None,
                    suppressed: false,
                })
                .collect(),
            rule,
            frame: None,
            slot_count: count,
            member_prefix: member_prefix.to_owned(),
            count_driver: None,
            span_driver: None,
        });
        Ok(members)
    }

    pub fn set_pattern_frame(
        &mut self,
        pattern_id: &str,
        frame: Option<&str>,
    ) -> Result<(), ModelError> {
        self.frame_chain(frame)?;
        let pattern = self
            .patterns
            .iter_mut()
            .find(|pattern| pattern.id == pattern_id)
            .ok_or_else(|| ModelError::new(format!("unknown pattern '{pattern_id}'")))?;
        pattern.frame = frame.map(str::to_owned);
        for member in &pattern.members {
            if let Some(node) = self.nodes.get_mut(&member.id) {
                *node.frame_mut() = frame.map(str::to_owned);
            }
        }
        Ok(())
    }

    /// Replaces a pattern rule and re-places every member without a
    /// placement override. A constraint-driven rule also resizes the pattern
    /// to the count its constraints require.
    pub fn set_pattern_rule(
        &mut self,
        pattern_id: &str,
        rule: PatternRule,
    ) -> Result<(), ModelError> {
        rule.validate()?;
        let index = self.pattern_index(pattern_id)?;
        validate_pattern_driver_rule(
            &rule,
            self.patterns[index].count_driver.as_ref(),
            self.patterns[index].span_driver.as_ref(),
        )?;
        let count = rule
            .fitted_count()?
            .unwrap_or(self.patterns[index].slot_count);
        let plan = self.plan_resize(&self.patterns[index], count)?;
        self.patterns[index].rule = rule;
        self.apply_resize(index, count, plan);
        Ok(())
    }

    /// Grows or shrinks a freely counted pattern to `count` rule slots.
    /// Growing adds linked members named `prefix[slot]`; shrinking deletes
    /// the members in removed slots. Slots vacated by detaching stay empty.
    pub fn set_pattern_count(&mut self, pattern_id: &str, count: usize) -> Result<(), ModelError> {
        let index = self.pattern_index(pattern_id)?;
        if self.patterns[index].count_driver.is_some() {
            return Err(ModelError::new(format!(
                "pattern '{pattern_id}' count is driven"
            )));
        }
        if self.patterns[index].rule.fitted_count()?.is_some() {
            return Err(ModelError::new(format!(
                "pattern '{pattern_id}' count is driven by its constraints"
            )));
        }
        let plan = self.plan_resize(&self.patterns[index], count)?;
        self.apply_resize(index, count, plan);
        Ok(())
    }

    /// Binds a freely counted pattern's slot count to an integer parameter or
    /// a generated output's measured bounds extent.
    /// The driver is resolved on the next graph regeneration or explicit
    /// [`Self::refresh_driven_patterns`] call.
    pub fn set_pattern_count_driver(
        &mut self,
        pattern_id: &str,
        driver: Option<PatternCountDriver>,
    ) -> Result<(), ModelError> {
        let index = self.pattern_index(pattern_id)?;
        validate_pattern_driver_rule(
            &self.patterns[index].rule,
            driver.as_ref(),
            self.patterns[index].span_driver.as_ref(),
        )?;
        if let Some(driver) = &driver {
            self.validate_count_driver(driver)?;
        }
        self.patterns[index].count_driver = driver;
        Ok(())
    }

    /// Binds a `LinearFit` span to a length parameter or measured bounds.
    /// The resolved span retains the rule's spacing constraint.
    pub fn set_pattern_span_driver(
        &mut self,
        pattern_id: &str,
        driver: Option<PatternSpanDriver>,
    ) -> Result<(), ModelError> {
        let index = self.pattern_index(pattern_id)?;
        validate_pattern_driver_rule(
            &self.patterns[index].rule,
            self.patterns[index].count_driver.as_ref(),
            driver.as_ref(),
        )?;
        if let Some(driver) = &driver {
            self.validate_span_driver(driver)?;
        }
        self.patterns[index].span_driver = driver;
        Ok(())
    }

    /// Resolves every pattern driver and updates rule placements and stable
    /// member slots. All drivers are evaluated before the graph is mutated.
    pub fn refresh_driven_patterns(&mut self, session: &Session) -> Result<(), ModelError> {
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

    pub(crate) fn validate_count_driver(
        &self,
        driver: &PatternCountDriver,
    ) -> Result<(), ModelError> {
        match driver {
            PatternCountDriver::Parameter {
                instance,
                parameter,
            } => {
                let resolved = self.resolve(instance)?;
                let parameters = resolve_parameters(resolved.definition, &resolved.overrides)?;
                match parameters.get(parameter) {
                    Some(ParameterValue::Integer(_)) => Ok(()),
                    Some(_) => Err(ModelError::new(format!(
                        "pattern count parameter '{parameter}' on instance '{instance}' must be an integer"
                    ))),
                    None => Err(ModelError::new(format!(
                        "unknown pattern count parameter '{parameter}' on instance '{instance}'"
                    ))),
                }
            }
            PatternCountDriver::BoundsExtent {
                instance,
                output,
                maximum_spacing,
                ..
            } => {
                self.validate_measurement_target(instance, output)?;
                positive_spacing(*maximum_spacing).map(|_| ())
            }
        }
    }

    pub(crate) fn resolve_count_driver(
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

    /// The output must be a feature of the measured instance's own family,
    /// which may differ from the graph's primary family.
    pub(crate) fn validate_measurement_target(
        &self,
        instance: &str,
        output: &str,
    ) -> Result<(), ModelError> {
        let resolved = self.resolve(instance)?;
        if !resolved
            .definition
            .features
            .iter()
            .any(|feature| feature.id == output)
        {
            return Err(ModelError::new(format!(
                "unknown pattern measurement output '{output}' on instance '{instance}'"
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_span_driver(
        &self,
        driver: &PatternSpanDriver,
    ) -> Result<(), ModelError> {
        let (instance, direction) = match driver {
            PatternSpanDriver::Parameter {
                instance,
                parameter,
                direction,
            } => {
                let resolved = self.resolve(instance)?;
                let parameters = resolve_parameters(resolved.definition, &resolved.overrides)?;
                match parameters.get(parameter) {
                    Some(ParameterValue::Scalar(value)) if value.dimension == Dimension::Length => {
                        value.normalized()?;
                    }
                    Some(_) => {
                        return Err(ModelError::new(format!(
                            "pattern span parameter '{parameter}' on instance '{instance}' must be a length"
                        )));
                    }
                    None => {
                        return Err(ModelError::new(format!(
                            "unknown pattern span parameter '{parameter}' on instance '{instance}'"
                        )));
                    }
                }
                (instance, direction)
            }
            PatternSpanDriver::BoundsExtent {
                instance,
                output,
                direction,
                ..
            } => {
                self.validate_measurement_target(instance, output)?;
                (instance, direction)
            }
        };
        normalized_pattern_direction(*direction).map_err(|error| {
            ModelError::new(format!(
                "pattern span driver for instance '{instance}': {}",
                error.message
            ))
        })?;
        Ok(())
    }

    pub(crate) fn resolve_span_driver(
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

    pub(crate) fn measure_output_extent(
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

    pub(crate) fn pattern_index(&self, pattern_id: &str) -> Result<usize, ModelError> {
        self.patterns
            .iter()
            .position(|pattern| pattern.id == pattern_id)
            .ok_or_else(|| ModelError::new(format!("unknown pattern '{pattern_id}'")))
    }

    /// Checks a resize without changing the graph.
    pub(crate) fn plan_resize(
        &self,
        pattern: &Pattern,
        count: usize,
    ) -> Result<ResizePlan, ModelError> {
        if !(1..=MAX_PATTERN_MEMBERS).contains(&count) {
            return Err(ModelError::new(format!(
                "pattern count must be 1..={MAX_PATTERN_MEMBERS}"
            )));
        }
        let removed = pattern
            .members
            .iter()
            .filter(|member| member.index >= count)
            .map(|member| member.id.clone())
            .collect::<Vec<_>>();
        if let Some((dependent, source)) = self.nodes.values().find_map(|node| match node {
            InstanceNode::Clone { id, source, .. }
                if removed.contains(source) && !removed.contains(id) =>
            {
                Some((id, source))
            }
            _ => None,
        }) {
            return Err(ModelError::new(format!(
                "cannot remove pattern member '{source}': '{dependent}' is cloned from it"
            )));
        }
        if let Some((member, reference)) = removed
            .iter()
            .find_map(|member| Some((member, self.assembly.reference_to(member)?)))
        {
            return Err(ModelError::new(format!(
                "cannot remove pattern member '{member}': it is named by {reference}"
            )));
        }
        let added = (pattern.slot_count..count)
            .map(|slot| (format!("{}[{slot}]", pattern.member_prefix), slot))
            .collect::<Vec<_>>();
        if let Some((existing, _)) = added.iter().find(|(id, _)| self.nodes.contains_key(id)) {
            return Err(ModelError::new(format!(
                "cannot grow pattern '{}': instance '{existing}' already exists",
                pattern.id
            )));
        }
        let remaining = pattern.members.len() - removed.len();
        if remaining == 0 && added.is_empty() {
            return Err(ModelError::new(format!(
                "pattern '{}' would have no members",
                pattern.id
            )));
        }
        Ok(ResizePlan { removed, added })
    }

    pub(crate) fn apply_resize(&mut self, index: usize, count: usize, plan: ResizePlan) {
        for id in &plan.removed {
            self.nodes.remove(id);
        }
        let pattern = &mut self.patterns[index];
        pattern.members.retain(|member| member.index < count);
        pattern.slot_count = count;
        for (id, slot) in plan.added {
            self.nodes.insert(
                id.clone(),
                InstanceNode::Clone {
                    id: id.clone(),
                    source: pattern.source.clone(),
                    overrides: HashMap::new(),
                    placement: Placement::identity(),
                    frame: pattern.frame.clone(),
                    provenance: format!("grown by pattern '{}'", pattern.id),
                },
            );
            pattern.members.push(PatternMember {
                id,
                index: slot,
                placement_override: None,
                suppressed: false,
            });
        }
        let pattern_id = pattern.id.clone();
        self.sync_pattern_placements(&pattern_id);
    }

    /// Returns a pattern member to its rule placement. Returns the override
    /// that was removed, if any.
    pub fn clear_placement_override(
        &mut self,
        instance_id: &str,
    ) -> Result<Option<Placement>, ModelError> {
        let member = self.pattern_member_mut(instance_id).ok_or_else(|| {
            ModelError::new(format!("instance '{instance_id}' is not a pattern member"))
        })?;
        let removed = member.placement_override.take();
        let pattern_id = self
            .pattern_of(instance_id)
            .map(|pattern| pattern.id.clone())
            .expect("member belongs to a pattern");
        self.sync_pattern_placements(&pattern_id);
        Ok(removed)
    }

    /// Suppresses or restores one pattern member. A suppressed member keeps
    /// its identity, links, and overrides but is skipped by graph regeneration.
    pub fn set_member_suppressed(
        &mut self,
        instance_id: &str,
        suppressed: bool,
    ) -> Result<(), ModelError> {
        self.pattern_member_mut(instance_id)
            .ok_or_else(|| {
                ModelError::new(format!("instance '{instance_id}' is not a pattern member"))
            })?
            .suppressed = suppressed;
        Ok(())
    }

    /// True for suppressed pattern members and for instances the active
    /// configuration suppresses.
    pub fn is_suppressed(&self, instance_id: &str) -> bool {
        self.assembly.configuration_suppresses(instance_id)
            || self
                .pattern_of(instance_id)
                .and_then(|pattern| pattern.member(instance_id))
                .is_some_and(|member| member.suppressed)
    }

    pub(crate) fn pattern_of(&self, instance_id: &str) -> Option<&Pattern> {
        self.patterns
            .iter()
            .find(|pattern| pattern.member(instance_id).is_some())
    }

    pub(crate) fn pattern_member_mut(&mut self, instance_id: &str) -> Option<&mut PatternMember> {
        self.patterns
            .iter_mut()
            .flat_map(|pattern| pattern.members.iter_mut())
            .find(|member| member.id == instance_id)
    }

    /// Writes each member's override or rule placement onto its node.
    pub(crate) fn sync_pattern_placements(&mut self, pattern_id: &str) {
        let Some(pattern) = self
            .patterns
            .iter()
            .find(|pattern| pattern.id == pattern_id)
        else {
            return;
        };
        for member in &pattern.members {
            let placement = pattern.member_placement(member);
            if let Some(node) = self.nodes.get_mut(&member.id) {
                match node {
                    InstanceNode::Base {
                        placement: value, ..
                    }
                    | InstanceNode::Clone {
                        placement: value, ..
                    } => *value = placement,
                }
            }
        }
    }
}
