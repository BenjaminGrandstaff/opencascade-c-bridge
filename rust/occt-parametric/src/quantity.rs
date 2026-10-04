//! Units, dimensioned quantities, and rigid placements.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Scalar,
    Length,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    Millimeter,
    Centimeter,
    Meter,
    Inch,
}

impl LengthUnit {
    pub(crate) const fn millimeter_factor(self) -> f64 {
        match self {
            Self::Millimeter => 1.0,
            Self::Centimeter => 10.0,
            Self::Meter => 1_000.0,
            Self::Inch => 25.4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quantity {
    pub value: f64,
    pub dimension: Dimension,
    pub unit: Option<LengthUnit>,
}

impl Quantity {
    pub const fn scalar(value: f64) -> Self {
        Self {
            value,
            dimension: Dimension::Scalar,
            unit: None,
        }
    }

    pub const fn length(value: f64, unit: LengthUnit) -> Self {
        Self {
            value,
            dimension: Dimension::Length,
            unit: Some(unit),
        }
    }

    pub(crate) fn normalized(self) -> Result<f64, ModelError> {
        if !self.value.is_finite() {
            return Err(ModelError::new("quantity is not finite"));
        }
        match (self.dimension, self.unit) {
            (Dimension::Scalar, None) => Ok(self.value),
            (Dimension::Length, Some(unit)) => Ok(self.value * unit.millimeter_factor()),
            _ => Err(ModelError::new(
                "quantity dimension and unit are inconsistent",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorQuantity {
    pub x: Quantity,
    pub y: Quantity,
    pub z: Quantity,
}

impl VectorQuantity {
    pub const fn lengths(x: f64, y: f64, z: f64, unit: LengthUnit) -> Self {
        Self {
            x: Quantity::length(x, unit),
            y: Quantity::length(y, unit),
            z: Quantity::length(z, unit),
        }
    }

    pub const fn scalars(x: f64, y: f64, z: f64) -> Self {
        Self {
            x: Quantity::scalar(x),
            y: Quantity::scalar(y),
            z: Quantity::scalar(z),
        }
    }

    pub(crate) fn scaled(self, factor: f64) -> Self {
        Self {
            x: Quantity {
                value: self.x.value * factor,
                ..self.x
            },
            y: Quantity {
                value: self.y.value * factor,
                ..self.y
            },
            z: Quantity {
                value: self.z.value * factor,
                ..self.z
            },
        }
    }

    pub(crate) fn normalized(self, dimension: Dimension) -> Result<Vec3, ModelError> {
        for component in [self.x, self.y, self.z] {
            if component.dimension != dimension {
                return Err(ModelError::new("vector component has the wrong dimension"));
            }
        }
        Ok(Vec3::new(
            self.x.normalized()?,
            self.y.normalized()?,
            self.z.normalized()?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisAngle {
    pub origin: VectorQuantity,
    pub axis: VectorQuantity,
    pub angle_radians: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub translation: VectorQuantity,
    pub rotation: Option<AxisAngle>,
}

impl Placement {
    pub const fn identity() -> Self {
        Self {
            translation: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            rotation: None,
        }
    }

    pub const fn translated(translation: VectorQuantity) -> Self {
        Self {
            translation,
            rotation: None,
        }
    }

    pub(crate) fn normalized(self) -> Result<NormalizedPlacement, ModelError> {
        let translation = self.translation.normalized(Dimension::Length)?;
        let rotation = self
            .rotation
            .map(|rotation| {
                if !rotation.angle_radians.is_finite() {
                    return Err(ModelError::new("placement angle is not finite"));
                }
                let axis = rotation.axis.normalized(Dimension::Scalar)?;
                if axis.x.hypot(axis.y.hypot(axis.z)) <= f64::EPSILON {
                    return Err(ModelError::new("placement rotation axis is zero"));
                }
                Ok((
                    rotation.origin.normalized(Dimension::Length)?,
                    axis,
                    rotation.angle_radians,
                ))
            })
            .transpose()?;
        Ok(NormalizedPlacement {
            translation,
            rotation,
        })
    }
}

impl Default for Placement {
    fn default() -> Self {
        Self::identity()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct NormalizedPlacement {
    pub(crate) translation: Vec3,
    pub(crate) rotation: Option<(Vec3, Vec3, f64)>,
}

impl NormalizedPlacement {
    pub(crate) fn translation_is_zero(&self) -> bool {
        self.translation.x == 0.0 && self.translation.y == 0.0 && self.translation.z == 0.0
    }
}

pub(crate) fn placements_equivalent(left: Placement, right: Placement) -> Result<bool, ModelError> {
    let left = left.normalized()?;
    let right = right.normalized()?;
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
    let vector_close = |a: Vec3, b: Vec3| close(a.x, b.x) && close(a.y, b.y) && close(a.z, b.z);
    if !vector_close(left.translation, right.translation) {
        return Ok(false);
    }
    Ok(match (left.rotation, right.rotation) {
        (None, None) => true,
        (
            Some((left_origin, left_axis, left_angle)),
            Some((right_origin, right_axis, right_angle)),
        ) => {
            vector_close(left_origin, right_origin)
                && vector_close(left_axis, right_axis)
                && close(left_angle, right_angle)
        }
        _ => false,
    })
}
