//! Physical mass properties of explicit component outputs at their current poses.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalMassProperties {
    pub mass_kg: f64,
    pub volume_mm3: f64,
    pub center_mm: Vec3,
    /// Central tensor, row-major in world XYZ axes, in kg mm².
    pub inertia_kg_mm2: [[f64; 3]; 3],
    pub relative_volume_error: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentMassProperties {
    pub output: InstanceOutputRef,
    pub material: String,
    pub properties: PhysicalMassProperties,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssemblyMassProperties {
    pub components: Vec<ComponentMassProperties>,
    pub total: PhysicalMassProperties,
    pub generated_variants: usize,
}

impl PhysicalMassProperties {
    pub fn measured(value: occt_types::MassProperties, density: f64) -> Result<Self, ModelError> {
        let density = density * CUBIC_MILLIMETERS_TO_CUBIC_METERS;
        let properties = Self {
            mass_kg: value.volume * density,
            volume_mm3: value.volume,
            center_mm: value.center,
            inertia_kg_mm2: value.inertia.map(|row| row.map(|entry| entry * density)),
            relative_volume_error: value.relative_volume_error,
        };
        properties.validate()?;
        Ok(properties)
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        if self.mass_kg <= 0.0
            || self.volume_mm3 <= 0.0
            || self.relative_volume_error < 0.0
            || ![
                self.mass_kg,
                self.volume_mm3,
                self.center_mm.x,
                self.center_mm.y,
                self.center_mm.z,
                self.relative_volume_error,
            ]
            .iter()
            .chain(self.inertia_kg_mm2.iter().flatten())
            .all(|value| value.is_finite())
        {
            return Err(ModelError::new(
                "physical mass properties exceed finite positive measurement limits",
            ));
        }
        Ok(())
    }

    /// Weighted central combination avoids subtracting world-origin inertia.
    pub fn combine(&mut self, other: &Self) -> Result<(), ModelError> {
        let mass = self.mass_kg + other.mass_kg;
        let fraction = other.mass_kg / mass;
        let delta = subtract(other.center_mm, self.center_mm);
        let offsets = [delta.x, delta.y, delta.z];
        let norm_squared = dot(delta, delta);
        let reduced_mass = (self.mass_kg / mass) * other.mass_kg;
        for (row, entries) in self.inertia_kg_mm2.iter_mut().enumerate() {
            for (column, entry) in entries.iter_mut().enumerate() {
                let diagonal = if row == column { norm_squared } else { 0.0 };
                *entry += other.inertia_kg_mm2[row][column]
                    + reduced_mass * (diagonal - offsets[row] * offsets[column]);
            }
        }
        self.relative_volume_error = (self.relative_volume_error * self.volume_mm3
            + other.relative_volume_error * other.volume_mm3)
            / (self.volume_mm3 + other.volume_mm3);
        self.volume_mm3 += other.volume_mm3;
        self.center_mm = add(self.center_mm, scale(delta, fraction));
        self.mass_kg = mass;
        self.validate()
    }
}
