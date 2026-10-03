//! Named configurations: override layers, suppression, and activation.

use super::*;

impl<'definition> InstanceGraph<'definition> {
    // ---- configurations

    pub fn add_configuration(&mut self, id: impl Into<String>) -> Result<(), ModelError> {
        let id = id.into();
        if id.is_empty() || self.assembly.configuration(&id).is_some() {
            return Err(ModelError::new(
                "configuration ids must be nonempty and unique",
            ));
        }
        self.assembly.configurations.push(Configuration {
            id,
            ..Configuration::default()
        });
        Ok(())
    }

    /// Sets an override in a configuration. Every instance must still resolve
    /// under that configuration, or the change is rejected.
    pub fn set_configuration_override(
        &mut self,
        configuration: &str,
        instance: &str,
        parameter: impl Into<String>,
        value: ParameterValue,
    ) -> Result<(), ModelError> {
        self.require_instance(instance)?;
        let parameter = parameter.into();
        let mut candidate = self.clone();
        candidate
            .configuration_mut(configuration)?
            .overrides
            .entry(instance.to_owned())
            .or_default()
            .insert(parameter, value);
        candidate.validate_configuration(configuration)?;
        self.assembly = candidate.assembly;
        Ok(())
    }

    pub fn remove_configuration_override(
        &mut self,
        configuration: &str,
        instance: &str,
        parameter: &str,
    ) -> Result<Option<ParameterValue>, ModelError> {
        let entry = self.configuration_mut(configuration)?;
        let removed = entry
            .overrides
            .get_mut(instance)
            .and_then(|overrides| overrides.remove(parameter));
        entry.overrides.retain(|_, overrides| !overrides.is_empty());
        Ok(removed)
    }

    pub fn set_configuration_suppressed(
        &mut self,
        configuration: &str,
        instance: &str,
        suppressed: bool,
    ) -> Result<(), ModelError> {
        self.require_instance(instance)?;
        let entry = self.configuration_mut(configuration)?;
        if suppressed {
            entry.suppressed.insert(instance.to_owned());
        } else {
            entry.suppressed.remove(instance);
        }
        Ok(())
    }

    /// Evaluates the graph under a configuration, or the base graph with
    /// `None`. Resolution, datums, relationship checks, pattern drivers, and
    /// graph regeneration all follow the active configuration.
    pub fn set_active_configuration(
        &mut self,
        configuration: Option<&str>,
    ) -> Result<(), ModelError> {
        if let Some(id) = configuration
            && self.assembly.configuration(id).is_none()
        {
            return Err(ModelError::new(format!("unknown configuration '{id}'")));
        }
        self.assembly.active_configuration = configuration.map(str::to_owned);
        Ok(())
    }

    pub fn active_configuration(&self) -> Option<&str> {
        self.assembly.active_configuration.as_deref()
    }

    pub(crate) fn configuration_mut(&mut self, id: &str) -> Result<&mut Configuration, ModelError> {
        self.assembly
            .configurations
            .iter_mut()
            .find(|configuration| configuration.id == id)
            .ok_or_else(|| ModelError::new(format!("unknown configuration '{id}'")))
    }

    pub(crate) fn require_instance(&self, instance: &str) -> Result<(), ModelError> {
        if self.nodes.contains_key(instance) {
            Ok(())
        } else {
            Err(ModelError::new(format!("unknown instance '{instance}'")))
        }
    }

    /// Every instance must resolve valid parameters under the configuration.
    pub(crate) fn validate_configuration(&self, id: &str) -> Result<(), ModelError> {
        let configuration = self
            .assembly
            .configuration(id)
            .ok_or_else(|| ModelError::new(format!("unknown configuration '{id}'")))?;
        if let Some(instance) = configuration
            .overrides
            .keys()
            .chain(&configuration.suppressed)
            .find(|instance| !self.nodes.contains_key(instance.as_str()))
        {
            return Err(ModelError::new(format!(
                "configuration '{id}' references unknown instance '{instance}'"
            )));
        }
        let mut configured = self.clone();
        configured.assembly.active_configuration = Some(id.to_owned());
        let mut resolutions = HashMap::new();
        for instance in self.nodes.keys() {
            let resolved = configured.resolve_cached(instance, &mut resolutions)?;
            resolve_parameters(resolved.definition, &resolved.overrides).map_err(|error| {
                ModelError::new(format!(
                    "configuration '{id}' instance '{instance}': {}",
                    error.message
                ))
            })?;
        }
        Ok(())
    }
}
