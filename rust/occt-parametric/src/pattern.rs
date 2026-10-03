//! Linear, circular, and driven pattern rules and their members.

use super::*;

/// Upper bound on members a constraint-driven pattern may produce.
pub const MAX_PATTERN_MEMBERS: usize = 10_000;

/// How a linear fit chooses its member count along the span.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinearSpacing {
    /// Exactly this many members, ends included.
    Count(usize),
    /// As many members as fit with gaps at least this long.
    Minimum(Quantity),
    /// As few members as keep gaps at most this long.
    Maximum(Quantity),
}

/// How a circular fit chooses its member count over the sweep.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AngularSpacing {
    Count(usize),
    MinimumRadians(f64),
    MaximumRadians(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternRule {
    Linear {
        step: VectorQuantity,
    },
    Circular {
        origin: VectorQuantity,
        axis: VectorQuantity,
        angle_step_radians: f64,
    },
    /// Members spread evenly from the source placement to `span`; the count
    /// is derived from the spacing constraint.
    LinearFit {
        span: VectorQuantity,
        spacing: LinearSpacing,
    },
    /// Members spread evenly over `sweep_radians` in (0, 2π]. A full turn is
    /// closed: members divide it into equal gaps without doubling up at 2π.
    CircularFit {
        origin: VectorQuantity,
        axis: VectorQuantity,
        sweep_radians: f64,
        spacing: AngularSpacing,
    },
}

/// Resolves a freely counted pattern's slot count from an instance parameter
/// or from the measured extent of generated assembly geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternCountDriver {
    Parameter {
        instance: String,
        parameter: String,
    },
    /// Fits the fewest members whose gaps do not exceed `maximum_spacing`
    /// across the measured output extent.
    BoundsExtent {
        instance: String,
        output: String,
        axis: CoordinateAxis,
        maximum_spacing: Quantity,
    },
}

/// Resolves the span of a `LinearFit` rule. A scalar parameter supplies a
/// length along `direction`; a bounds extent measures a named generated
/// output along `axis` and applies that length along `direction`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternSpanDriver {
    Parameter {
        instance: String,
        parameter: String,
        direction: VectorQuantity,
    },
    BoundsExtent {
        instance: String,
        output: String,
        axis: CoordinateAxis,
        direction: VectorQuantity,
    },
}

impl PatternRule {
    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        let count = self.fitted_count()?.unwrap_or(2).max(2);
        self.member_placement(1, count).normalized().map(|_| ())
    }

    /// The member count a constraint-driven rule requires; `None` for rules
    /// whose count is chosen freely.
    pub fn fitted_count(&self) -> Result<Option<usize>, ModelError> {
        match *self {
            Self::Linear { .. } | Self::Circular { .. } => Ok(None),
            Self::LinearFit { span, spacing } => {
                let span = span.normalized(Dimension::Length)?;
                let length = span.x.hypot(span.y.hypot(span.z));
                if length <= 0.0 {
                    return Err(ModelError::new("linear fit span must be nonzero"));
                }
                spacing.count(length).map(Some)
            }
            Self::CircularFit {
                sweep_radians,
                spacing,
                ..
            } => {
                if !(sweep_radians > 0.0
                    && sweep_radians <= std::f64::consts::TAU + CLOSED_SWEEP_TOLERANCE)
                {
                    return Err(ModelError::new("circular fit sweep must be in (0, 2π]"));
                }
                spacing
                    .count(sweep_radians, is_closed_sweep(sweep_radians))
                    .map(Some)
            }
        }
    }

    /// Placement of rule slot `index` in a pattern with `count` slots.
    pub(crate) fn member_placement(&self, index: usize, count: usize) -> Placement {
        match *self {
            Self::Linear { step } => Placement::translated(step.scaled(index as f64)),
            Self::Circular {
                origin,
                axis,
                angle_step_radians,
            } => rotation_about(origin, axis, angle_step_radians * index as f64),
            Self::LinearFit { span, .. } => {
                Placement::translated(span.scaled(slot_fraction(index, count.saturating_sub(1))))
            }
            Self::CircularFit {
                origin,
                axis,
                sweep_radians,
                ..
            } => {
                let gaps = if is_closed_sweep(sweep_radians) {
                    count
                } else {
                    count.saturating_sub(1)
                };
                rotation_about(origin, axis, sweep_radians * slot_fraction(index, gaps))
            }
        }
    }
}

pub(crate) const CLOSED_SWEEP_TOLERANCE: f64 = 1e-9;

pub(crate) fn is_closed_sweep(sweep_radians: f64) -> bool {
    (sweep_radians - std::f64::consts::TAU).abs() <= CLOSED_SWEEP_TOLERANCE
}

pub(crate) fn slot_fraction(index: usize, gaps: usize) -> f64 {
    if gaps == 0 {
        0.0
    } else {
        index as f64 / gaps as f64
    }
}

pub(crate) fn rotation_about(
    origin: VectorQuantity,
    axis: VectorQuantity,
    angle_radians: f64,
) -> Placement {
    Placement {
        translation: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        rotation: Some(AxisAngle {
            origin,
            axis,
            angle_radians,
        }),
    }
}

/// `value / divisor`, snapped to the nearest integer when within
/// floating-point noise, so a 9 m span at 3 m spacing has exactly 3 gaps.
pub(crate) fn snapped_ratio(value: f64, divisor: f64) -> f64 {
    let ratio = value / divisor;
    let nearest = ratio.round();
    if (ratio - nearest).abs() <= 1e-9 * nearest.abs().max(1.0) {
        nearest
    } else {
        ratio
    }
}

pub(crate) fn checked_member_count(count: f64) -> Result<usize, ModelError> {
    if count.is_finite() && (1.0..=MAX_PATTERN_MEMBERS as f64).contains(&count) {
        Ok(count as usize)
    } else {
        Err(ModelError::new(format!(
            "pattern constraints must yield 1..={MAX_PATTERN_MEMBERS} members"
        )))
    }
}

impl LinearSpacing {
    pub(crate) fn count(self, length: f64) -> Result<usize, ModelError> {
        let gaps = match self {
            Self::Count(count) => return checked_member_count(count as f64),
            Self::Minimum(spacing) => snapped_ratio(length, positive_spacing(spacing)?).floor(),
            Self::Maximum(spacing) => snapped_ratio(length, positive_spacing(spacing)?).ceil(),
        };
        checked_member_count(gaps + 1.0)
    }
}

pub(crate) fn positive_spacing(spacing: Quantity) -> Result<f64, ModelError> {
    if spacing.dimension != Dimension::Length {
        return Err(ModelError::new("linear pattern spacing must be a length"));
    }
    let value = spacing.normalized()?;
    if value > 0.0 {
        Ok(value)
    } else {
        Err(ModelError::new("linear pattern spacing must be positive"))
    }
}

impl AngularSpacing {
    pub(crate) fn count(self, sweep: f64, closed: bool) -> Result<usize, ModelError> {
        let gaps = match self {
            Self::Count(count) => return checked_member_count(count as f64),
            Self::MinimumRadians(angle) => snapped_ratio(sweep, positive_angle(angle)?).floor(),
            Self::MaximumRadians(angle) => snapped_ratio(sweep, positive_angle(angle)?).ceil(),
        };
        checked_member_count(if closed { gaps } else { gaps + 1.0 })
    }
}

pub(crate) fn positive_angle(angle: f64) -> Result<f64, ModelError> {
    if angle.is_finite() && angle > 0.0 {
        Ok(angle)
    } else {
        Err(ModelError::new(
            "angular pattern spacing must be positive and finite",
        ))
    }
}

pub(crate) fn validate_pattern_driver_rule(
    rule: &PatternRule,
    count_driver: Option<&PatternCountDriver>,
    span_driver: Option<&PatternSpanDriver>,
) -> Result<(), ModelError> {
    if count_driver.is_some() && rule.fitted_count()?.is_some() {
        return Err(ModelError::new(
            "a parameter count driver requires a freely counted linear or circular rule",
        ));
    }
    if span_driver.is_some() && !matches!(rule, PatternRule::LinearFit { .. }) {
        return Err(ModelError::new(
            "a span driver requires a linear_fit pattern rule",
        ));
    }
    Ok(())
}

pub(crate) fn normalized_pattern_direction(direction: VectorQuantity) -> Result<Vec3, ModelError> {
    let direction = direction.normalized(Dimension::Scalar)?;
    let magnitude = direction.x.hypot(direction.y.hypot(direction.z));
    if !magnitude.is_finite() || magnitude <= f64::EPSILON {
        return Err(ModelError::new("pattern span direction is zero"));
    }
    Ok(Vec3::new(
        direction.x / magnitude,
        direction.y / magnitude,
        direction.z / magnitude,
    ))
}

/// One linked copy in a pattern. `index` is its slot in the rule and stays
/// fixed when other members leave the pattern.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PatternMember {
    pub id: String,
    pub index: usize,
    /// Replaces the rule placement for this member until cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_override: Option<Placement>,
    /// Kept in the pattern and linked, but excluded from graph regeneration.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub source: String,
    pub members: Vec<PatternMember>,
    pub rule: PatternRule,
    /// Assembly frame in which the rule and every member placement are expressed.
    #[serde(default)]
    pub frame: Option<String>,
    /// Number of rule slots; members occupy a subset of `0..slot_count`.
    #[serde(default)]
    pub slot_count: usize,
    /// Members created when the pattern grows are named `prefix[slot]`.
    #[serde(default)]
    pub member_prefix: String,
    /// Optional parameter or measured-geometry source for the slot count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_driver: Option<PatternCountDriver>,
    /// Optional source for a `LinearFit` rule's span.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_driver: Option<PatternSpanDriver>,
}

impl Pattern {
    pub fn member(&self, id: &str) -> Option<&PatternMember> {
        self.members.iter().find(|member| member.id == id)
    }

    pub fn member_ids(&self) -> impl Iterator<Item = &str> {
        self.members.iter().map(|member| member.id.as_str())
    }

    /// The member's override, or the rule placement for its slot.
    pub fn member_placement(&self, member: &PatternMember) -> Placement {
        member
            .placement_override
            .unwrap_or_else(|| self.rule.member_placement(member.index, self.slot_count))
    }
}
