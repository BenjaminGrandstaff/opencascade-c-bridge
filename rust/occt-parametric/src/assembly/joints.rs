//! Frame-based kinematic joints. Rest placements define attachment geometry;
//! joint motion acts in the parent frame after the rest placement.

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JointScalar {
    pub value: Quantity,
    pub minimum: Option<Quantity>,
    pub maximum: Option<Quantity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointDof {
    Angle,
    Axial,
    PlanarX,
    PlanarY,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JointKind {
    Fixed,
    Revolute {
        angle: JointScalar,
    },
    Prismatic {
        distance: JointScalar,
    },
    Cylindrical {
        angle: JointScalar,
        distance: JointScalar,
    },
    Planar {
        x_axis: VectorQuantity,
        x: JointScalar,
        y: JointScalar,
        angle: JointScalar,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AssemblyJoint {
    pub id: String,
    pub frame: String,
    /// Origin and axis in the frame's parent coordinates, not world coordinates.
    pub origin: VectorQuantity,
    pub axis: VectorQuantity,
    pub kind: JointKind,
}

impl JointScalar {
    fn checked(&self, dimension: Dimension) -> Result<f64, ModelError> {
        let normalize = |quantity: Quantity| {
            if quantity.dimension != dimension {
                return Err(ModelError::new(
                    "joint coordinate or limit has the wrong dimension",
                ));
            }
            let value = quantity.normalized()?;
            if !value.is_finite() {
                return Err(ModelError::new("joint coordinate or limit is not finite"));
            }
            Ok(value)
        };
        let value = normalize(self.value)?;
        let minimum = self.minimum.map(normalize).transpose()?;
        let maximum = self.maximum.map(normalize).transpose()?;
        if minimum.zip(maximum).is_some_and(|(lo, hi)| lo > hi)
            || minimum.is_some_and(|lo| value < lo)
            || maximum.is_some_and(|hi| value > hi)
        {
            return Err(ModelError::new(
                "joint coordinate is outside its limits or limits are reversed",
            ));
        }
        Ok(value)
    }
}

impl JointKind {
    pub(super) fn coordinate_mut(
        &mut self,
        coordinate: JointDof,
    ) -> Result<&mut JointScalar, ModelError> {
        match (self, coordinate) {
            (Self::Revolute { angle }, JointDof::Angle)
            | (Self::Cylindrical { angle, .. }, JointDof::Angle)
            | (Self::Planar { angle, .. }, JointDof::Angle) => Ok(angle),
            (Self::Prismatic { distance }, JointDof::Axial)
            | (Self::Cylindrical { distance, .. }, JointDof::Axial) => Ok(distance),
            (Self::Planar { x, .. }, JointDof::PlanarX) => Ok(x),
            (Self::Planar { y, .. }, JointDof::PlanarY) => Ok(y),
            _ => Err(ModelError::new(
                "joint does not have the requested degree of freedom",
            )),
        }
    }
}

fn normalized_axis(value: VectorQuantity) -> Result<Vec3, ModelError> {
    let axis = value.normalized(Dimension::Scalar)?;
    let maximum = axis.x.abs().max(axis.y.abs()).max(axis.z.abs());
    if maximum == 0.0 {
        return Err(ModelError::new("joint axis must be nonzero"));
    }
    unit(Vec3::new(
        axis.x / maximum,
        axis.y / maximum,
        axis.z / maximum,
    ))
}

impl AssemblyJoint {
    /// O(1) coordinate/limit validation and motion construction. Angles are
    /// dimensionless radians, translations are normalized to millimeters.
    pub(crate) fn motion(&self) -> Result<Placement, ModelError> {
        let origin = self.origin.normalized(Dimension::Length)?;
        if ![origin.x, origin.y, origin.z]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(ModelError::new("joint origin must be finite"));
        }
        let axis = normalized_axis(self.axis)?;
        let (angle, translation) = match &self.kind {
            JointKind::Fixed => (0.0, Vec3::new(0.0, 0.0, 0.0)),
            JointKind::Revolute { angle } => {
                (angle.checked(Dimension::Scalar)?, Vec3::new(0.0, 0.0, 0.0))
            }
            JointKind::Prismatic { distance } => {
                (0.0, scale(axis, distance.checked(Dimension::Length)?))
            }
            JointKind::Cylindrical { angle, distance } => (
                angle.checked(Dimension::Scalar)?,
                scale(axis, distance.checked(Dimension::Length)?),
            ),
            JointKind::Planar {
                x_axis,
                x,
                y,
                angle,
            } => {
                let x_axis = normalized_axis(*x_axis)?;
                if dot(axis, x_axis).abs() > 1e-9 {
                    return Err(ModelError::new(
                        "planar joint x axis must be perpendicular to its normal",
                    ));
                }
                (
                    angle.checked(Dimension::Scalar)?,
                    add(
                        scale(x_axis, x.checked(Dimension::Length)?),
                        scale(cross(axis, x_axis), y.checked(Dimension::Length)?),
                    ),
                )
            }
        };
        let rotation = if angle == 0.0 {
            None
        } else {
            Some(AxisAngle {
                origin: VectorQuantity::lengths(
                    origin.x,
                    origin.y,
                    origin.z,
                    LengthUnit::Millimeter,
                ),
                axis: VectorQuantity::scalars(axis.x, axis.y, axis.z),
                angle_radians: angle,
            })
        };
        let placement = Placement {
            rotation,
            translation: VectorQuantity::lengths(
                translation.x,
                translation.y,
                translation.z,
                LengthUnit::Millimeter,
            ),
        };
        placement.normalized()?;
        Ok(placement)
    }
}

impl InstanceGraph<'_> {
    pub fn joints(&self) -> impl Iterator<Item = &AssemblyJoint> {
        self.assembly.joints.values()
    }

    pub fn add_joint(&mut self, joint: AssemblyJoint) -> Result<(), ModelError> {
        self.add_joints([joint])
    }

    /// Atomic batch insertion, O(existing joints + additions * log(joints)).
    /// Each frame has at most one joint; the frame tree supplies parentage.
    pub fn add_joints(
        &mut self,
        joints: impl IntoIterator<Item = AssemblyJoint>,
    ) -> Result<(), ModelError> {
        let mut ids = self
            .assembly
            .joints
            .values()
            .map(|joint| joint.id.as_str().to_owned())
            .collect::<HashSet<_>>();
        let mut candidate = self.assembly.joints.clone();
        for joint in joints {
            if joint.id.is_empty()
                || !ids.insert(joint.id.clone())
                || candidate.contains_key(&joint.frame)
            {
                return Err(ModelError::new(
                    "joint ids must be unique and a frame may have only one joint",
                ));
            }
            if !self.frames.contains_key(&joint.frame) {
                return Err(ModelError::new(format!(
                    "unknown assembly frame '{}'",
                    joint.frame
                )));
            }
            joint.motion()?;
            candidate.insert(joint.frame.clone(), joint);
        }
        self.assembly.joints = candidate;
        Ok(())
    }

    /// Changes one coordinate atomically; preserves the accepted state on bad
    /// units, unknown DOFs, and limit violations. O(log(joints)).
    pub fn set_joint_coordinate(
        &mut self,
        frame: &str,
        coordinate: JointDof,
        value: Quantity,
    ) -> Result<(), ModelError> {
        let mut joint = self
            .assembly
            .joints
            .get(frame)
            .cloned()
            .ok_or_else(|| ModelError::new(format!("frame '{frame}' has no joint")))?;
        joint.kind.coordinate_mut(coordinate)?.value = value;
        joint.motion()?;
        self.assembly.joints.insert(frame.to_owned(), joint);
        Ok(())
    }

    pub fn remove_joint(&mut self, frame: &str) -> Result<AssemblyJoint, ModelError> {
        self.assembly
            .joints
            .remove(frame)
            .ok_or_else(|| ModelError::new(format!("frame '{frame}' has no joint")))
    }

    pub(crate) fn validate_joints(&self) -> Result<(), ModelError> {
        let mut ids = HashSet::new();
        for (frame, joint) in &self.assembly.joints {
            if frame != &joint.frame
                || !self.frames.contains_key(frame)
                || joint.id.is_empty()
                || !ids.insert(&joint.id)
            {
                return Err(ModelError::new("invalid joint id or frame binding"));
            }
            joint.motion()?;
        }
        Ok(())
    }
}
