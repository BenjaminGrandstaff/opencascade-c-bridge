//! Frozen clearance-hole diameter snapshot. Source (verified 2026-10-02):
//! https://www.ekinsun.com/custom-fasteners/clearance-hole-chart/
//! Nominal diameters only: no hole tolerances or fit certification. Tap drills
//! and socket-head recesses are in the `catalogs` submodule.

use super::*;
mod catalogs;
pub use catalogs::{
    HoleCatalogSystem, SocketHeadDimension, SocketHeadRecess, carr_lane_socket_dimension_v1,
    carr_lane_socket_head_v1, carr_lane_tap_drill_v1,
};
pub(crate) use catalogs::{socket_scalar, tap_scalar};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClearanceSeries {
    Fine,
    Medium,
    Coarse,
}

// [fastener nominal diameter, fine, medium, coarse], millimeters.
// Freeze V1 to preserve regenerated geometry; changed values need a new version.
const ISO273_V1: [[f64; 4]; 19] = [
    [1.6, 1.7, 1.8, 2.0],
    [2.0, 2.2, 2.4, 2.6],
    [2.5, 2.7, 2.9, 3.1],
    [3.0, 3.2, 3.4, 3.6],
    [4.0, 4.3, 4.5, 4.8],
    [5.0, 5.3, 5.5, 5.8],
    [6.0, 6.4, 6.6, 7.0],
    [8.0, 8.4, 9.0, 10.0],
    [10.0, 10.5, 11.0, 12.0],
    [12.0, 13.0, 13.5, 14.5],
    [14.0, 15.0, 15.5, 16.5],
    [16.0, 17.0, 17.5, 18.5],
    [18.0, 19.0, 20.0, 21.0],
    [20.0, 21.0, 22.0, 24.0],
    [22.0, 23.0, 24.0, 26.0],
    [24.0, 25.0, 26.0, 28.0],
    [27.0, 28.0, 30.0, 32.0],
    [30.0, 31.0, 33.0, 35.0],
    [36.0, 37.0, 39.0, 42.0],
];

/// Resolve one supported metric clearance diameter, returning millimeters.
/// Only unit-conversion roundoff (1e-9 mm) is tolerated; never interpolate.
pub fn iso273_clearance_v1(
    nominal: Quantity,
    series: ClearanceSeries,
) -> Result<Quantity, ModelError> {
    if nominal.dimension != Dimension::Length {
        return Err(ModelError::new(
            "clearance nominal diameter must be a length",
        ));
    }
    let nominal = nominal.normalized()?;
    let row = ISO273_V1
        .iter()
        .find(|row| (row[0] - nominal).abs() <= 1e-9)
        .ok_or_else(|| {
            ModelError::new(format!(
                "unsupported ISO 273 V1 nominal diameter {nominal} mm"
            ))
        })?;
    let column = match series {
        ClearanceSeries::Fine => 1,
        ClearanceSeries::Medium => 2,
        ClearanceSeries::Coarse => 3,
    };
    Ok(Quantity::length(row[column], LengthUnit::Millimeter))
}

pub(crate) fn clearance_scalar(
    nominal: EvaluatedScalar,
    series: ClearanceSeries,
) -> Result<EvaluatedScalar, ModelError> {
    if nominal.dimension != Dimension::Length {
        return Err(ModelError::new(
            "clearance nominal diameter must be a length",
        ));
    }
    EvaluatedScalar::from_quantity(iso273_clearance_v1(
        Quantity::length(nominal.value, LengthUnit::Millimeter),
        series,
    )?)
}
