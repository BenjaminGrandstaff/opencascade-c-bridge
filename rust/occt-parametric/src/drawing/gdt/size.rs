//! Size-departure arithmetic; inputs are supplied sizes, not fitted measurements.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureOfSizeKind {
    Internal,
    External,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingSizeLimits {
    pub kind: FeatureOfSizeKind,
    pub lower: Quantity,
    pub upper: Quantity,
}

/// Arithmetic allowance only: does not establish measured conformity or datum shift.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometricToleranceAllowance {
    pub supplied_size_mm: f64,
    pub maximum_material_size_mm: f64,
    pub least_material_size_mm: f64,
    pub bonus_mm: f64,
    pub total_tolerance_mm: f64,
    pub refinement_total_tolerance_mm: Option<f64>,
}

fn length(value: &Quantity) -> Result<f64, ModelError> {
    if value.dimension != Dimension::Length {
        return Err(ModelError::new("feature size must be a length"));
    }
    let mm = value.normalized()?;
    if !mm.is_finite() || mm <= 0.0 {
        return Err(ModelError::new("feature size must be positive and finite"));
    }
    Ok(mm)
}

impl DrawingSizeLimits {
    fn bounds(&self) -> Result<(f64, f64), ModelError> {
        let lower = length(&self.lower)?;
        let upper = length(&self.upper)?;
        if lower > upper {
            return Err(ModelError::new("feature size limits are reversed"));
        }
        Ok((lower, upper))
    }
}

pub(super) fn validate(frame: &DrawingFeatureControlFrame) -> Result<(), ModelError> {
    if let Some(limits) = &frame.size_limits {
        if !frame.feature_of_size {
            return Err(ModelError::new(
                "size limits require a declared feature of size",
            ));
        }
        limits.bounds()?;
    }
    Ok(())
}

fn total(base: &Quantity, bonus: f64) -> Result<f64, ModelError> {
    let result = length(base)? + bonus;
    if !result.is_finite() {
        return Err(ModelError::new("geometric tolerance allowance overflow"));
    }
    Ok(result)
}

impl DrawingFeatureControlFrame {
    /// Calculates size-departure allowance for supported feature-of-size controls.
    /// Caller supplies a size appropriate to the selected material condition;
    /// this method does not derive an actual mating envelope from geometry.
    /// Out-of-limit sizes are errors, including at RFS. Validate the drawing
    /// separately to check attachments and datum-reference semantics.
    pub fn tolerance_allowance(
        &self,
        supplied_size: Quantity,
    ) -> Result<GeometricToleranceAllowance, ModelError> {
        validate(self)?;
        number(self)?;
        if !self.feature_of_size || !self.characteristic.material() {
            return Err(ModelError::new(
                "allowance requires a supported feature-of-size control",
            ));
        }
        if self.zone == GeometricToleranceZone::Diameter && !self.characteristic.diameter()
            || self.characteristic == GeometricCharacteristic::Position
                && self.zone != GeometricToleranceZone::Diameter
        {
            return Err(ModelError::new(
                "unsupported feature-of-size tolerance zone",
            ));
        }
        let limits = self
            .size_limits
            .as_ref()
            .ok_or_else(|| ModelError::new("tolerance allowance requires explicit size limits"))?;
        let (lower, upper) = limits.bounds()?;
        let supplied = length(&supplied_size)?;
        if supplied < lower || supplied > upper {
            return Err(ModelError::new(
                "supplied feature size is outside its limits",
            ));
        }
        let (maximum, least) = match limits.kind {
            FeatureOfSizeKind::Internal => (lower, upper),
            FeatureOfSizeKind::External => (upper, lower),
        };
        let bonus = match self.material {
            ToleranceMaterialCondition::Regardless => 0.0,
            ToleranceMaterialCondition::Maximum => (supplied - maximum).abs(),
            ToleranceMaterialCondition::Least => (supplied - least).abs(),
        };
        let refinement = refinement_total(self, bonus)?;
        Ok(GeometricToleranceAllowance {
            supplied_size_mm: supplied,
            maximum_material_size_mm: maximum,
            least_material_size_mm: least,
            bonus_mm: bonus,
            total_tolerance_mm: total(&self.tolerance, bonus)?,
            refinement_total_tolerance_mm: refinement,
        })
    }
}

fn refinement_total(
    frame: &DrawingFeatureControlFrame,
    bonus: f64,
) -> Result<Option<f64>, ModelError> {
    let Some(refinement) = &frame.refinement else {
        return Ok(None);
    };
    if frame.characteristic != GeometricCharacteristic::Position {
        return Err(ModelError::new(
            "size allowance supports only position composites",
        ));
    }
    let lower = composite::refined(frame);
    number(&lower)?;
    if length(&lower.tolerance)? >= length(&frame.tolerance)? || number(&lower)? == number(frame)? {
        return Err(ModelError::new("composite refinement must be tighter"));
    }
    Ok(Some(total(&refinement.tolerance, bonus)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allowance_sum_rejects_overflow() {
        assert!(
            total(
                &Quantity::length(f64::MAX, LengthUnit::Millimeter),
                f64::MAX
            )
            .is_err()
        );
    }
}
