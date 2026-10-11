//! Materials: definitions, assignment with inheritance, and mass.

use super::*;
#[allow(unused_imports)]
pub use occt_parametric_core::assembly::materials::*;
pub trait InstanceGraphMaterials<'definition> {
    /// Mass in kilograms of one generated output: its volume times the
    /// instance's material density.
    fn mass(&self, session: &Session, instance: &str, output: &str) -> Result<f64, ModelError>;
}

impl<'definition> InstanceGraphMaterials<'definition> for InstanceGraph<'definition> {
    fn mass(&self, session: &Session, instance: &str, output: &str) -> Result<f64, ModelError> {
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
}
