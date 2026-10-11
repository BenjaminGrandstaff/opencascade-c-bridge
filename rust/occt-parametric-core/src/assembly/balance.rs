//! Material totals and geometric center-of-gravity positions, without a target.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialMassProperties {
    pub material: String,
    pub components: usize,
    pub properties: PhysicalMassProperties,
}

/// A reference chord in world coordinates. It need not be along world X.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChordReference {
    pub leading_edge: VectorQuantity,
    pub direction: VectorQuantity,
    pub length: Quantity,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BalancePosition {
    pub center_mm: Vec3,
    /// Signed projection aft of the reference leading edge along the chord.
    pub distance_from_leading_edge_mm: f64,
    /// May lie outside [0, 1]; this is geometry, not a recommended CG range.
    pub chord_fraction: f64,
}

impl ChordReference {
    pub fn validate(self) -> Result<(), ModelError> {
        self.locate(self.leading_edge.normalized(Dimension::Length)?)
            .map(|_| ())
    }
    /// Projects a world-space point onto a unit-aware chord. O(1), kernel-free.
    pub fn locate(self, center_mm: Vec3) -> Result<BalancePosition, ModelError> {
        let leading = self.leading_edge.normalized(Dimension::Length)?;
        let direction = self.direction.normalized(Dimension::Scalar)?;
        let maximum = direction
            .x
            .abs()
            .max(direction.y.abs())
            .max(direction.z.abs());
        if self.length.dimension != Dimension::Length {
            return Err(ModelError::new("reference chord must be a length"));
        }
        let length = self.length.normalized()?;
        if !(maximum.is_finite() && maximum > 0.0 && length.is_finite() && length > 0.0) {
            return Err(ModelError::new(
                "reference chord needs a positive finite length and nonzero finite direction",
            ));
        }
        let scaled = Vec3::new(
            direction.x / maximum,
            direction.y / maximum,
            direction.z / maximum,
        );
        let norm = scaled.x.hypot(scaled.y).hypot(scaled.z);
        let delta = subtract(center_mm, leading);
        let distance = dot(delta, scale(scaled, 1.0 / norm));
        let fraction = distance / length;
        if ![center_mm.x, center_mm.y, center_mm.z, distance, fraction]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(ModelError::new(
                "balance position exceeds finite measurement limits",
            ));
        }
        Ok(BalancePosition {
            center_mm,
            distance_from_leading_edge_mm: distance,
            chord_fraction: fraction,
        })
    }
}

impl AssemblyMassProperties {
    /// Ordered per-material totals, preserving world central inertia and volume
    /// error estimates. O(components * log(materials)), with O(materials) storage.
    /// No second generation or integration is required.
    pub fn material_totals(&self) -> Result<Vec<MaterialMassProperties>, ModelError> {
        let mut groups: BTreeMap<&str, MaterialMassProperties> = BTreeMap::new();
        for component in &self.components {
            component.properties.validate()?;
            match groups.entry(&component.material) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(MaterialMassProperties {
                        material: component.material.clone(),
                        components: 1,
                        properties: component.properties.clone(),
                    });
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    let group = entry.get_mut();
                    group.properties.combine(&component.properties)?;
                    group.components += 1;
                }
            }
        }
        if groups.is_empty() {
            return Err(ModelError::new(
                "material totals need at least one component",
            ));
        }
        Ok(groups.into_values().collect())
    }

    pub fn balance(&self, reference: ChordReference) -> Result<BalancePosition, ModelError> {
        self.total.validate()?;
        reference.locate(self.total.center_mm)
    }
}
