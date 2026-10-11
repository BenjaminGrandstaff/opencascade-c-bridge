//! Checks explicit dimensional limits against supplied measurements.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionMeasurementDisposition {
    WithinLimits,
    BelowLowerLimit,
    AboveUpperLimit,
    NoSpecifiedTolerance,
    BasicDimension,
    ReferenceDimension,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DimensionMeasurementLimits {
    pub lower: Quantity,
    pub upper: Quantity,
    /// Distance to the nearer limit; negative outside, zero on a boundary.
    pub margin: Quantity,
}

/// Length quantities use mm; scalar quantities represent radians.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DimensionMeasurementEvaluation {
    pub nominal: Quantity,
    pub measured: Quantity,
    pub deviation: Quantity,
    pub limits: Option<DimensionMeasurementLimits>,
    pub disposition: DimensionMeasurementDisposition,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingDimensionMeasurement {
    pub dimension: String,
    pub value: Quantity,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawingDimensionMeasurementResult {
    pub dimension: String,
    pub evaluation: DimensionMeasurementEvaluation,
}

fn normalize(value: Quantity, dimension: Dimension) -> Result<f64, ModelError> {
    if value.dimension != dimension {
        return Err(ModelError::new(
            "measurement and tolerance dimensions must agree",
        ));
    }
    let result = value.normalized()?;
    if !result.is_finite() {
        return Err(ModelError::new(
            "dimension measurement exceeds finite limits",
        ));
    }
    Ok(result)
}

fn quantity(value: f64, dimension: Dimension) -> Quantity {
    match dimension {
        Dimension::Length => Quantity::length(value, LengthUnit::Millimeter),
        Dimension::Scalar => Quantity::scalar(value),
    }
}

impl DimensionTolerance {
    /// Explicit limits only: does not infer title-block tolerances or uncertainty.
    /// Scalar inputs are angles in radians, without wrapping or taking abs().
    pub fn evaluate_measurement(
        &self,
        nominal: Quantity,
        measured: Quantity,
    ) -> Result<DimensionMeasurementEvaluation, ModelError> {
        let dimension = nominal.dimension;
        let n = normalize(nominal, dimension)?;
        let m = normalize(measured, dimension)?;
        if n < 0.0 || m < 0.0 {
            return Err(ModelError::new(
                "nominal and measured dimensions must be nonnegative",
            ));
        }
        let bounds = self.measurement_bounds(n, dimension)?;
        let (limits, disposition) = match bounds {
            Some((lower, upper)) => {
                let disposition = if m < lower {
                    DimensionMeasurementDisposition::BelowLowerLimit
                } else if m > upper {
                    DimensionMeasurementDisposition::AboveUpperLimit
                } else {
                    DimensionMeasurementDisposition::WithinLimits
                };
                (
                    Some(DimensionMeasurementLimits {
                        lower: quantity(lower, dimension),
                        upper: quantity(upper, dimension),
                        margin: quantity((m - lower).min(upper - m), dimension),
                    }),
                    disposition,
                )
            }
            None => (None, self.no_limit_disposition()),
        };
        Ok(DimensionMeasurementEvaluation {
            nominal: quantity(n, dimension),
            measured: quantity(m, dimension),
            deviation: quantity(m - n, dimension),
            limits,
            disposition,
        })
    }

    fn no_limit_disposition(&self) -> DimensionMeasurementDisposition {
        match self {
            Self::Basic => DimensionMeasurementDisposition::BasicDimension,
            Self::Reference => DimensionMeasurementDisposition::ReferenceDimension,
            _ => DimensionMeasurementDisposition::NoSpecifiedTolerance,
        }
    }

    fn measurement_bounds(
        &self,
        nominal: f64,
        dimension: Dimension,
    ) -> Result<Option<(f64, f64)>, ModelError> {
        let (lower, upper) = match self {
            Self::None | Self::Basic | Self::Reference => return Ok(None),
            Self::Symmetric { deviation } => {
                let deviation = normalize(*deviation, dimension)?;
                if deviation < 0.0 {
                    return Err(ModelError::new("symmetric deviation must be nonnegative"));
                }
                (nominal - deviation, nominal + deviation)
            }
            Self::Deviations { lower, upper } => {
                let lower = normalize(*lower, dimension)?;
                let upper = normalize(*upper, dimension)?;
                if lower > 0.0 || upper < 0.0 {
                    return Err(ModelError::new("signed deviations must bracket zero"));
                }
                (nominal + lower, nominal + upper)
            }
            Self::Limits { lower, upper } => {
                (normalize(*lower, dimension)?, normalize(*upper, dimension)?)
            }
        };
        if !lower.is_finite()
            || !upper.is_finite()
            || lower < 0.0
            || lower > nominal
            || upper < nominal
        {
            return Err(ModelError::new(
                "finite nonnegative limits must contain nominal",
            ));
        }
        Ok(Some((lower, upper)))
    }
}

fn nominal_value(
    dimension: &DrawingDimension,
    view: &DrawingView,
    graph: &InstanceGraph<'_>,
    context: &DimensionContext<'_>,
) -> Result<Quantity, ModelError> {
    if let Some(hole) = &dimension.presentation.hole {
        let (operation, params) = hole_source(hole, context)?;
        let FeatureOperation::Hole { diameter, .. } = operation else {
            return Err(ModelError::new(
                "measurement callout requires a Hole feature",
            ));
        };
        let value = evaluate_resolved_expression(diameter, params)?;
        if value.dimension != Dimension::Length || !value.value.is_finite() || value.value <= 0.0 {
            return Err(ModelError::new(
                "hole diameter must be a positive finite length",
            ));
        }
        return Ok(Quantity::length(value.value, LengthUnit::Millimeter));
    }
    let (_, _, _, value) = dimension_geometry(dimension, view, graph)?;
    Ok(quantity(
        value.abs(),
        if matches!(dimension.direction, DimensionDirection::Angular { .. }) {
            Dimension::Scalar
        } else {
            Dimension::Length
        },
    ))
}

impl DrawingDefinition {
    /// Validates once, resolves current nominal dimensions and evaluates in input
    /// order. Repeated dimension IDs are allowed for repeated measurements.
    pub fn evaluate_dimension_measurements(
        &self,
        graph: &InstanceGraph<'_>,
        measurements: &[DrawingDimensionMeasurement],
    ) -> Result<Vec<DrawingDimensionMeasurementResult>, ModelError> {
        self.validate(graph)?;
        let views: HashMap<_, _> = self.views.iter().map(|v| (v.id.as_str(), v)).collect();
        let dimensions: HashMap<_, _> =
            self.dimensions.iter().map(|d| (d.id.as_str(), d)).collect();
        let context = DimensionContext::new(&self.dimensions, graph)?;
        let mut nominals = HashMap::new();
        measurements
            .iter()
            .map(|measurement| {
                let d = dimensions
                    .get(measurement.dimension.as_str())
                    .ok_or_else(|| ModelError::new("unknown drawing dimension measurement"))?;
                let nominal = match nominals.get(d.id.as_str()) {
                    Some(value) => *value,
                    None => {
                        let value = nominal_value(d, views[d.view.as_str()], graph, &context)?;
                        nominals.insert(d.id.as_str(), value);
                        value
                    }
                };
                Ok(DrawingDimensionMeasurementResult {
                    dimension: measurement.dimension.clone(),
                    evaluation: d
                        .presentation
                        .tolerance
                        .evaluate_measurement(nominal, measurement.value)?,
                })
            })
            .collect()
    }
}
