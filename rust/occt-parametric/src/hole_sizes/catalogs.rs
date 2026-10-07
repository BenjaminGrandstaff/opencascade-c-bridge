//! Frozen Carr Lane Rev. 9/2021 numeric recommendations, verified 2026-10-03.
//! https://www.carrlane.com/Portals/0/PDFs/CLM-Trig%20Booklet-ENG-v4-PM.pdf
//! Printed pages 3–5 and 9. Fractions use exact rational inches; decimals retain
//! printed precision. Tap drills are the manufacturer's 75% theoretical cut
//! thread recommendations. These are nominal recommendations, not tolerances.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HoleCatalogSystem {
    Metric,
    Inch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SocketHeadDimension {
    CounterboreDiameter,
    CounterboreDepth,
    NormalClearance,
    CloseClearance,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SocketHeadRecess {
    pub counterbore_diameter: Quantity,
    pub counterbore_depth: Quantity,
    pub normal_clearance: Quantity,
    pub close_clearance: Quantity,
}

const METRIC_TAPS: [[f64; 3]; 28] = [
    [1.6, 0.35, 1.25],
    [2.0, 0.4, 1.6],
    [2.5, 0.45, 2.05],
    [3.0, 0.5, 2.5],
    [3.5, 0.6, 2.9],
    [4.0, 0.7, 3.3],
    [5.0, 0.8, 4.2],
    [6.0, 1.0, 5.0],
    [8.0, 1.25, 6.8],
    [8.0, 1.0, 7.0],
    [10.0, 1.5, 8.5],
    [10.0, 1.25, 8.8],
    [12.0, 1.75, 10.2],
    [12.0, 1.25, 10.8],
    [14.0, 2.0, 12.0],
    [14.0, 1.5, 12.5],
    [16.0, 2.0, 14.0],
    [16.0, 1.5, 14.5],
    [18.0, 2.5, 15.5],
    [18.0, 1.5, 16.5],
    [20.0, 2.5, 17.5],
    [20.0, 1.5, 18.5],
    [22.0, 2.5, 19.5],
    [22.0, 1.5, 20.5],
    [24.0, 3.0, 21.0],
    [24.0, 2.0, 22.0],
    [27.0, 3.0, 24.0],
    [27.0, 2.0, 25.0],
];

const INCH_TAPS: [[f64; 3]; 36] = [
    [0.06, 80.0, 0.046875],
    [0.086, 56.0, 0.07],
    [0.086, 64.0, 0.07],
    [0.112, 40.0, 0.089],
    [0.112, 48.0, 0.0935],
    [0.125, 40.0, 0.1015],
    [0.125, 44.0, 0.104],
    [0.138, 32.0, 0.1065],
    [0.138, 40.0, 0.113],
    [0.164, 32.0, 0.136],
    [0.164, 36.0, 0.136],
    [0.19, 24.0, 0.1495],
    [0.19, 32.0, 0.159],
    [0.25, 20.0, 0.201],
    [0.25, 28.0, 0.213],
    [0.3125, 18.0, 0.257],
    [0.3125, 24.0, 0.272],
    [0.375, 16.0, 0.3125],
    [0.375, 24.0, 0.332],
    [0.4375, 14.0, 0.368],
    [0.4375, 20.0, 0.390625],
    [0.5, 13.0, 0.421875],
    [0.5, 20.0, 0.453125],
    [0.5625, 12.0, 0.484375],
    [0.5625, 18.0, 0.515625],
    [0.625, 11.0, 0.53125],
    [0.625, 18.0, 0.578125],
    [0.75, 10.0, 0.65625],
    [0.75, 16.0, 0.6875],
    [0.875, 9.0, 0.765625],
    [0.875, 14.0, 0.8125],
    [1.0, 8.0, 0.875],
    [1.0, 12.0, 0.921875],
    [1.0, 14.0, 0.9375],
    [1.125, 7.0, 0.984375],
    [1.125, 12.0, 1.046875],
];

const METRIC_RECESSES: [[f64; 5]; 18] = [
    [1.6, 3.5, 1.6, 1.95, 1.8],
    [2.0, 4.4, 2.0, 2.4, 2.2],
    [2.5, 5.4, 2.5, 3.0, 2.7],
    [3.0, 6.5, 3.0, 3.7, 3.4],
    [4.0, 8.25, 4.0, 4.8, 4.4],
    [5.0, 9.75, 5.0, 5.8, 5.4],
    [6.0, 11.2, 6.0, 6.8, 6.4],
    [8.0, 14.5, 8.0, 8.8, 8.4],
    [10.0, 17.5, 10.0, 10.8, 10.5],
    [12.0, 19.5, 12.0, 13.0, 12.5],
    [14.0, 22.5, 14.0, 15.0, 14.5],
    [16.0, 25.5, 16.0, 17.0, 16.5],
    [20.0, 31.5, 20.0, 21.0, 20.5],
    [24.0, 37.5, 24.0, 25.0, 24.5],
    [30.0, 47.5, 30.0, 31.5, 31.0],
    [36.0, 56.5, 36.0, 37.5, 37.0],
    [42.0, 66.0, 42.0, 44.0, 43.0],
    [48.0, 75.0, 48.0, 50.0, 49.0],
];

const INCH_RECESSES: [[f64; 5]; 20] = [
    [0.06, 0.125, 0.06, 0.073, 0.067],
    [0.086, 0.1875, 0.086, 0.1065, 0.09375],
    [0.112, 0.21875, 0.112, 0.136, 0.125],
    [0.125, 0.25, 0.125, 0.154, 0.140625],
    [0.138, 0.28125, 0.138, 0.1695, 0.154],
    [0.164, 0.3125, 0.164, 0.1935, 0.18],
    [0.19, 0.375, 0.19, 0.221, 0.2055],
    [0.25, 0.4375, 0.25, 0.28125, 0.265625],
    [0.3125, 0.53125, 0.312, 0.34375, 0.328125],
    [0.375, 0.625, 0.375, 0.40625, 0.390625],
    [0.4375, 0.71875, 0.438, 0.46875, 0.453125],
    [0.5, 0.8125, 0.5, 0.53125, 0.515625],
    [0.625, 1.0, 0.625, 0.65625, 0.640625],
    [0.75, 1.1875, 0.75, 0.78125, 0.765625],
    [0.875, 1.375, 0.875, 0.90625, 0.890625],
    [1.0, 1.625, 1.0, 1.03125, 1.015625],
    [1.25, 2.0, 1.25, 1.3125, 1.28125],
    [1.5, 2.375, 1.5, 1.5625, 1.53125],
    [1.75, 2.75, 1.75, 1.8125, 1.78125],
    [2.0, 3.125, 2.0, 2.0625, 2.03125],
];
fn millimeters(value: Quantity) -> Result<f64, ModelError> {
    if value.dimension != Dimension::Length {
        return Err(ModelError::new("hole catalog inputs must be lengths"));
    }
    let value = value.normalized()?;
    if !value.is_finite() || value <= 0.0 {
        return Err(ModelError::new(
            "hole catalog inputs must be finite positive lengths",
        ));
    }
    Ok(value)
}
fn factor(system: HoleCatalogSystem) -> f64 {
    match system {
        HoleCatalogSystem::Metric => 1.0,
        HoleCatalogSystem::Inch => 25.4,
    }
}
/// Frozen cut-tap recommendation. Pitch is a length in either system: for
/// 20 TPI use Quantity::length(1.0/20.0, LengthUnit::Inch). No interpolation.
pub fn carr_lane_tap_drill_v1(
    system: HoleCatalogSystem,
    nominal: Quantity,
    pitch: Quantity,
) -> Result<Quantity, ModelError> {
    let nominal = millimeters(nominal)?;
    let pitch = millimeters(pitch)?;
    let rows = match system {
        HoleCatalogSystem::Metric => METRIC_TAPS.as_slice(),
        HoleCatalogSystem::Inch => INCH_TAPS.as_slice(),
    };
    let scale = factor(system);
    let row = rows
        .iter()
        .find(|row| {
            let candidate_pitch = match system {
                HoleCatalogSystem::Metric => row[1],
                HoleCatalogSystem::Inch => 1.0 / row[1],
            };
            (row[0] * scale - nominal).abs() <= 1e-9
                && (candidate_pitch * scale - pitch).abs() <= 1e-9
        })
        .ok_or_else(|| ModelError::new("unsupported Carr Lane V1 nominal diameter/pitch pair"))?;
    Ok(Quantity::length(row[2] * scale, LengthUnit::Millimeter))
}
/// Nominal socket-head recess recommendations; the optional 60-degree
/// under-head relief in the source table is separate and is not generated.
pub fn carr_lane_socket_head_v1(
    system: HoleCatalogSystem,
    nominal: Quantity,
) -> Result<SocketHeadRecess, ModelError> {
    let nominal = millimeters(nominal)?;
    let rows = match system {
        HoleCatalogSystem::Metric => METRIC_RECESSES.as_slice(),
        HoleCatalogSystem::Inch => INCH_RECESSES.as_slice(),
    };
    let scale = factor(system);
    let row = rows
        .iter()
        .find(|row| (row[0] * scale - nominal).abs() <= 1e-9)
        .ok_or_else(|| ModelError::new("unsupported Carr Lane V1 socket-head nominal diameter"))?;
    let dimension = |column: usize| Quantity::length(row[column] * scale, LengthUnit::Millimeter);
    Ok(SocketHeadRecess {
        counterbore_diameter: dimension(1),
        counterbore_depth: dimension(2),
        normal_clearance: dimension(3),
        close_clearance: dimension(4),
    })
}
pub fn carr_lane_socket_dimension_v1(
    system: HoleCatalogSystem,
    nominal: Quantity,
    dimension: SocketHeadDimension,
) -> Result<Quantity, ModelError> {
    let recess = carr_lane_socket_head_v1(system, nominal)?;
    Ok(match dimension {
        SocketHeadDimension::CounterboreDiameter => recess.counterbore_diameter,
        SocketHeadDimension::CounterboreDepth => recess.counterbore_depth,
        SocketHeadDimension::NormalClearance => recess.normal_clearance,
        SocketHeadDimension::CloseClearance => recess.close_clearance,
    })
}
pub(crate) fn tap_scalar(
    system: HoleCatalogSystem,
    nominal: EvaluatedScalar,
    pitch: EvaluatedScalar,
) -> Result<EvaluatedScalar, ModelError> {
    if nominal.dimension != Dimension::Length || pitch.dimension != Dimension::Length {
        return Err(ModelError::new(
            "tap catalog diameter and pitch must be lengths",
        ));
    }
    EvaluatedScalar::from_quantity(carr_lane_tap_drill_v1(
        system,
        Quantity::length(nominal.value, LengthUnit::Millimeter),
        Quantity::length(pitch.value, LengthUnit::Millimeter),
    )?)
}
pub(crate) fn socket_scalar(
    system: HoleCatalogSystem,
    nominal: EvaluatedScalar,
    dimension: SocketHeadDimension,
) -> Result<EvaluatedScalar, ModelError> {
    if nominal.dimension != Dimension::Length {
        return Err(ModelError::new("socket catalog diameter must be a length"));
    }
    EvaluatedScalar::from_quantity(carr_lane_socket_dimension_v1(
        system,
        Quantity::length(nominal.value, LengthUnit::Millimeter),
        dimension,
    )?)
}
