//! Geometry-based pair checks and indexed broad-phase assembly checks.

use super::*;
pub mod exclusions;
pub use exclusions::CollisionPairRef;
pub use exclusions::PairExclusions;

#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct InstanceOutputRef {
    pub instance: String,
    pub output: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CollisionOptions {
    pub minimum_clearance: Quantity,
    pub contact_tolerance: Quantity,
    /// Absolute overlap threshold in mm³, combined with operand-relative roundoff.
    pub overlap_volume_tolerance_mm3: f64,
}

impl Default for CollisionOptions {
    fn default() -> Self {
        Self {
            minimum_clearance: Quantity::length(0.0, LengthUnit::Millimeter),
            contact_tolerance: Quantity::length(1e-7, LengthUnit::Millimeter),
            overlap_volume_tolerance_mm3: 0.0,
        }
    }
}

impl CollisionOptions {
    pub fn validate(self) -> Result<(), ModelError> {
        self.checked().map(|_| ())
    }

    pub fn checked(self) -> Result<(f64, f64), ModelError> {
        let length = |quantity: Quantity| {
            if quantity.dimension != Dimension::Length {
                return Err(ModelError::new(
                    "collision clearance and tolerance must be lengths",
                ));
            }
            let value = quantity.normalized()?;
            if !value.is_finite() || value < 0.0 {
                return Err(ModelError::new(
                    "collision clearance and tolerance must be finite/nonnegative",
                ));
            }
            Ok(value)
        };
        if !self.overlap_volume_tolerance_mm3.is_finite() || self.overlap_volume_tolerance_mm3 < 0.0
        {
            return Err(ModelError::new(
                "overlap volume tolerance must be finite/nonnegative",
            ));
        }
        Ok((
            length(self.minimum_clearance)?,
            length(self.contact_tolerance)?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairStatus {
    Clear,
    Touching,
    Interference,
    InsufficientClearance,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PairCheck {
    pub first: InstanceOutputRef,
    pub second: InstanceOutputRef,
    pub separation_mm: f64,
    pub overlap_volume_mm3: f64,
    pub first_witness_mm: Vec3,
    pub second_witness_mm: Vec3,
    pub status: PairStatus,
}

pub fn component(point: Vec3, axis: usize) -> f64 {
    match axis {
        0 => point.x,
        1 => point.y,
        _ => point.z,
    }
}

pub fn joined(first: occt_types::Bounds, second: occt_types::Bounds) -> occt_types::Bounds {
    occt_types::Bounds {
        min: Vec3::new(
            first.min.x.min(second.min.x),
            first.min.y.min(second.min.y),
            first.min.z.min(second.min.z),
        ),
        max: Vec3::new(
            first.max.x.max(second.max.x),
            first.max.y.max(second.max.y),
            first.max.z.max(second.max.z),
        ),
    }
}

pub fn intersects(first: occt_types::Bounds, second: occt_types::Bounds, margin: f64) -> bool {
    (0..3).all(|axis| {
        component(first.min, axis) <= component(second.max, axis) + margin
            && component(second.min, axis) <= component(first.max, axis) + margin
    })
}
