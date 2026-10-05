//! Fixed cylindrical position-zone checks for supplied axis samples.
use super::*;

/// Nominal axis in the same established datum coordinate frame as the samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionToleranceAxis {
    pub origin: VectorQuantity,
    pub direction: VectorQuantity,
}

/// Checks only supplied samples; does not fit surfaces, datums or a mating envelope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionSampleEvaluation {
    pub allowance: GeometricToleranceAllowance,
    pub sample_count: usize,
    pub worst_sample_index: usize,
    pub maximum_radial_error_mm: f64,
    pub required_zone_diameter_mm: f64,
    pub diameter_margin_mm: f64,
    pub samples_within_zone: bool,
}

fn point(value: VectorQuantity) -> Result<Vec3, ModelError> {
    let p = value.normalized(Dimension::Length)?;
    if ![p.x, p.y, p.z].into_iter().all(f64::is_finite) {
        return Err(ModelError::new("position point exceeds finite limits"));
    }
    Ok(p)
}

fn supported(frame: &DrawingFeatureControlFrame) -> Result<(), ModelError> {
    if frame.characteristic != GeometricCharacteristic::Position
        || frame.zone != GeometricToleranceZone::Diameter
        || frame.refinement.is_some()
        || frame.datum_reference_frame.is_some()
        || frame.datums.len() != 3
        || frame
            .datums
            .iter()
            .any(|r| r.boundary != DatumMaterialBoundary::Regardless)
    {
        return Err(ModelError::new(
            "fixed position checks require a single diameter control with three resolved RFS datum references",
        ));
    }
    let mut ids = HashSet::new();
    if frame
        .datums
        .iter()
        .any(|r| r.datum_feature.is_empty() || !ids.insert(&r.datum_feature))
    {
        return Err(ModelError::new(
            "fixed position check needs distinct nonempty datum references",
        ));
    }
    Ok(())
}

impl DrawingFeatureControlFrame {
    /// Samples and the nominal axis must already share an established datum frame.
    /// Uses controlled-feature bonus, never datum shift. Named references must
    /// first be resolved; the DrawingDefinition method handles that resolution.
    pub fn evaluate_position_samples(
        &self,
        supplied_size: Quantity,
        nominal_axis: PositionToleranceAxis,
        samples: &[VectorQuantity],
    ) -> Result<PositionSampleEvaluation, ModelError> {
        supported(self)?;
        let allowance = self.tolerance_allowance(supplied_size)?;
        let origin = point(nominal_axis.origin)?;
        let direction = axis(nominal_axis.direction)?;
        if samples.is_empty() {
            return Err(ModelError::new(
                "position check requires at least one sample",
            ));
        }
        let (maximum, worst) = maximum_error(origin, direction, samples)?;
        let required = 2.0 * maximum;
        if !required.is_finite() {
            return Err(ModelError::new("position diameter exceeds finite limits"));
        }
        Ok(PositionSampleEvaluation {
            allowance,
            sample_count: samples.len(),
            worst_sample_index: worst,
            maximum_radial_error_mm: maximum,
            required_zone_diameter_mm: required,
            diameter_margin_mm: allowance.total_tolerance_mm - required,
            samples_within_zone: required <= allowance.total_tolerance_mm,
        })
    }
}

fn maximum_error(
    origin: Vec3,
    direction: Vec3,
    samples: &[VectorQuantity],
) -> Result<(f64, usize), ModelError> {
    let mut maximum = 0.0;
    let mut worst = 0;
    for (index, sample) in samples.iter().enumerate() {
        let delta = subtract(point(*sample)?, origin);
        if ![delta.x, delta.y, delta.z].into_iter().all(f64::is_finite) {
            return Err(ModelError::new(
                "position displacement exceeds finite limits",
            ));
        }
        let perpendicular = cross(delta, direction);
        let radius = perpendicular
            .x
            .hypot(perpendicular.y)
            .hypot(perpendicular.z);
        if !radius.is_finite() {
            return Err(ModelError::new(
                "position radial error exceeds finite limits",
            ));
        }
        if radius > maximum {
            maximum = radius;
            worst = index;
        }
    }
    Ok((maximum, worst))
}

impl DrawingDefinition {
    /// Validates saved drawing intent and resolves inline/named references.
    /// Does not establish the supplied measurement coordinate frame from geometry.
    pub fn evaluate_position_samples(
        &self,
        control_id: &str,
        graph: &InstanceGraph<'_>,
        supplied_size: Quantity,
        nominal_axis: PositionToleranceAxis,
        samples: &[VectorQuantity],
    ) -> Result<PositionSampleEvaluation, ModelError> {
        self.validate(graph)?;
        let control = self
            .feature_control_frames
            .iter()
            .find(|f| f.id == control_id)
            .ok_or_else(|| ModelError::new("unknown position control"))?;
        let resolved =
            effective_frames(std::slice::from_ref(control), &self.datum_reference_frames)?;
        resolved[0].evaluate_position_samples(supplied_size, nominal_axis, samples)
    }
}
