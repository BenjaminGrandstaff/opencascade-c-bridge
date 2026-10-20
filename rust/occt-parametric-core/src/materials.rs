//! Frozen material catalog `occb_materials_v1` (schema 179): typical values
//! from public manufacturer data sheets, read 2026-10-20, kept in the units
//! each sheet prints and converted here. Where a sheet gives a range, the
//! conservative end is kept (the lower strength, stiffness and temperature);
//! a property a sheet does not print is absent and asking for it is an error.
//! Typical values are not design allowables: printed parts in particular vary
//! with print settings, and the supplier's certificate governs real stock.
//!
//! Sources:
//! - Proto Labs CNC material data sheets (2026), linked from
//!   <https://www.protolabs.com/materials/cnc-machining/>, file
//!   `2026-026-cnc-material-data-sheet_<material>.pdf`.
//! - Prusa Polymers technical data sheets: Prusament PLA v1.1 (27-07-2022)
//!   and Prusament PETG v1.1 (16-02-2022); values for horizontally printed
//!   ISO 527-1 specimens.
use crate::error::ModelError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MaterialGrade {
    /// Proto Labs `aluminum-6061-t651_t6`.
    #[serde(rename = "aluminum_6061_t6")]
    Aluminum6061T6,
    /// Proto Labs `stainless-steel-304`.
    #[serde(rename = "stainless_steel_304")]
    StainlessSteel304,
    /// Proto Labs `steel-alloy-4140`; no strength is printed.
    #[serde(rename = "steel_alloy_4140")]
    SteelAlloy4140,
    /// Proto Labs `titanium`; the sheet names no grade (its values match
    /// Ti-6Al-4V) and prints no modulus.
    TitaniumCnc,
    /// Proto Labs `brass-c360`; yield at 0.5% extension.
    BrassC360,
    /// Proto Labs `abs`; yield printed as ">5,600 psi".
    AbsCnc,
    /// Proto Labs `polycarbonate-pc-black`; prints flexural, not tensile,
    /// modulus.
    PolycarbonateCnc,
    /// Proto Labs `pom` (acetal copolymer).
    PomAcetalCopolymer,
    /// Proto Labs `nylon-6-6`.
    #[serde(rename = "nylon_6_6")]
    Nylon66,
    /// Proto Labs `peek`.
    Peek,
    /// Proto Labs `pei-ultem-1000`.
    #[serde(rename = "pei_ultem_1000")]
    PeiUltem1000,
    /// Prusament PLA, horizontally printed specimens.
    PlaPrinted,
    /// Prusament PETG, horizontally printed specimens.
    PetgPrinted,
}

/// A catalog property; each is a plain number in the unit its name gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MaterialProperty {
    DensityKgPerCubicMeter,
    YoungsModulusMpa,
    YieldStrengthMpa,
    UltimateTensileStrengthMpa,
    /// ASTM D648 at 66 psi / ISO 75 at 0.45 MPa.
    #[serde(rename = "heat_deflection_0_45_mpa_c")]
    HeatDeflection045MpaC,
    /// ASTM D648 at 264 psi / ISO 75 at 1.80 MPa.
    #[serde(rename = "heat_deflection_1_80_mpa_c")]
    HeatDeflection180MpaC,
    ContinuousServiceC,
    /// Bond strength between printed layers (Prusa Polymers method).
    InterlayerStrengthMpa,
}

/// One material's frozen values in SI-style units (kg/m³, MPa, °C).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialData {
    pub density_kg_per_cubic_meter: f64,
    pub youngs_modulus_mpa: Option<f64>,
    pub yield_strength_mpa: Option<f64>,
    pub ultimate_tensile_strength_mpa: Option<f64>,
    pub heat_deflection_0_45_mpa_c: Option<f64>,
    pub heat_deflection_1_8_mpa_c: Option<f64>,
    pub continuous_service_c: Option<f64>,
    pub interlayer_strength_mpa: Option<f64>,
    /// The data sheet the values come from.
    pub source: &'static str,
}

const PSI_MPA: f64 = 0.006_894_757_293_168_361;
const LB_PER_CUBIC_INCH_KG_PER_CUBIC_METER: f64 = 27_679.904_710_203_12;

fn psi(value: f64) -> Option<f64> {
    Some(value * PSI_MPA)
}
fn ksi(value: f64) -> Option<f64> {
    psi(value * 1_000.0)
}
fn fahrenheit(value: f64) -> Option<f64> {
    Some((value - 32.0) * 5.0 / 9.0)
}
fn lb_per_cubic_inch(value: f64) -> f64 {
    value * LB_PER_CUBIC_INCH_KG_PER_CUBIC_METER
}

const NONE: MaterialData = MaterialData {
    density_kg_per_cubic_meter: 0.0,
    youngs_modulus_mpa: None,
    yield_strength_mpa: None,
    ultimate_tensile_strength_mpa: None,
    heat_deflection_0_45_mpa_c: None,
    heat_deflection_1_8_mpa_c: None,
    continuous_service_c: None,
    interlayer_strength_mpa: None,
    source: "",
};

impl MaterialGrade {
    pub const ALL: [Self; 13] = [
        Self::Aluminum6061T6,
        Self::StainlessSteel304,
        Self::SteelAlloy4140,
        Self::TitaniumCnc,
        Self::BrassC360,
        Self::AbsCnc,
        Self::PolycarbonateCnc,
        Self::PomAcetalCopolymer,
        Self::Nylon66,
        Self::Peek,
        Self::PeiUltem1000,
        Self::PlaPrinted,
        Self::PetgPrinted,
    ];

    /// The frozen values, as printed and converted.
    pub fn data(self) -> MaterialData {
        match self {
            Self::Aluminum6061T6 => MaterialData {
                density_kg_per_cubic_meter: 2_700.0,
                youngs_modulus_mpa: Some(68_300.0),
                yield_strength_mpa: Some(276.0),
                ultimate_tensile_strength_mpa: Some(310.0),
                source: "Proto Labs aluminum-6061-t651_t6 (2026)",
                ..NONE
            },
            Self::StainlessSteel304 => MaterialData {
                density_kg_per_cubic_meter: lb_per_cubic_inch(0.285),
                youngs_modulus_mpa: psi(29.0e6),
                yield_strength_mpa: ksi(48.0),
                ultimate_tensile_strength_mpa: ksi(99.0),
                source: "Proto Labs stainless-steel-304 (2026)",
                ..NONE
            },
            Self::SteelAlloy4140 => MaterialData {
                density_kg_per_cubic_meter: lb_per_cubic_inch(0.28),
                youngs_modulus_mpa: psi(33.0e6),
                source: "Proto Labs steel-alloy-4140 (2026)",
                ..NONE
            },
            Self::TitaniumCnc => MaterialData {
                density_kg_per_cubic_meter: 4_470.0,
                yield_strength_mpa: Some(880.0),
                ultimate_tensile_strength_mpa: Some(950.0),
                source: "Proto Labs titanium (2026)",
                ..NONE
            },
            Self::BrassC360 => MaterialData {
                density_kg_per_cubic_meter: lb_per_cubic_inch(0.307),
                youngs_modulus_mpa: ksi(14_000.0),
                yield_strength_mpa: psi(45_000.0),
                ultimate_tensile_strength_mpa: psi(56_000.0),
                source: "Proto Labs brass-c360 (2026)",
                ..NONE
            },
            Self::AbsCnc => MaterialData {
                density_kg_per_cubic_meter: 1_030.0,
                youngs_modulus_mpa: psi(301_000.0),
                yield_strength_mpa: psi(5_600.0),
                source: "Proto Labs abs (2026)",
                ..NONE
            },
            Self::PolycarbonateCnc => MaterialData {
                density_kg_per_cubic_meter: 1_200.0,
                yield_strength_mpa: psi(8_990.0),
                ultimate_tensile_strength_mpa: psi(9_860.0),
                heat_deflection_0_45_mpa_c: fahrenheit(275.0),
                heat_deflection_1_8_mpa_c: fahrenheit(255.0),
                source: "Proto Labs polycarbonate-pc-black (2026)",
                ..NONE
            },
            Self::PomAcetalCopolymer => MaterialData {
                density_kg_per_cubic_meter: 1_410.0,
                youngs_modulus_mpa: psi(380_000.0),
                yield_strength_mpa: psi(8_800.0),
                heat_deflection_0_45_mpa_c: fahrenheit(316.0),
                heat_deflection_1_8_mpa_c: fahrenheit(220.0),
                continuous_service_c: fahrenheit(180.0),
                source: "Proto Labs pom (2026)",
                ..NONE
            },
            Self::Nylon66 => MaterialData {
                density_kg_per_cubic_meter: 1_140.0,
                youngs_modulus_mpa: psi(350_000.0),
                yield_strength_mpa: psi(12_000.0),
                heat_deflection_0_45_mpa_c: fahrenheit(390.0),
                heat_deflection_1_8_mpa_c: fahrenheit(194.0),
                source: "Proto Labs nylon-6-6 (2026)",
                ..NONE
            },
            Self::Peek => MaterialData {
                density_kg_per_cubic_meter: 1_310.0,
                youngs_modulus_mpa: psi(500_000.0),
                yield_strength_mpa: psi(16_000.0),
                heat_deflection_1_8_mpa_c: fahrenheit(320.0),
                continuous_service_c: fahrenheit(480.0),
                source: "Proto Labs peek (2026)",
                ..NONE
            },
            Self::PeiUltem1000 => MaterialData {
                density_kg_per_cubic_meter: 1_270.0,
                youngs_modulus_mpa: psi(430_000.0),
                yield_strength_mpa: psi(16_700.0),
                ultimate_tensile_strength_mpa: psi(17_000.0),
                heat_deflection_0_45_mpa_c: fahrenheit(405.0),
                heat_deflection_1_8_mpa_c: fahrenheit(394.0),
                continuous_service_c: fahrenheit(338.0),
                source: "Proto Labs pei-ultem-1000 (2026)",
                ..NONE
            },
            Self::PlaPrinted => MaterialData {
                density_kg_per_cubic_meter: 1_240.0,
                youngs_modulus_mpa: Some(2_300.0),
                yield_strength_mpa: Some(51.0),
                heat_deflection_0_45_mpa_c: Some(55.0),
                heat_deflection_1_8_mpa_c: Some(55.0),
                interlayer_strength_mpa: Some(17.0),
                source: "Prusament PLA technical data sheet v1.1 (2022)",
                ..NONE
            },
            Self::PetgPrinted => MaterialData {
                density_kg_per_cubic_meter: 1_270.0,
                youngs_modulus_mpa: Some(1_500.0),
                yield_strength_mpa: Some(47.0),
                heat_deflection_0_45_mpa_c: Some(68.0),
                heat_deflection_1_8_mpa_c: Some(68.0),
                interlayer_strength_mpa: Some(18.0),
                source: "Prusament PETG technical data sheet v1.1 (2022)",
                ..NONE
            },
        }
    }

    /// One property's value, or an error naming the source that omits it.
    pub fn property(self, property: MaterialProperty) -> Result<f64, ModelError> {
        let data = self.data();
        let value = match property {
            MaterialProperty::DensityKgPerCubicMeter => Some(data.density_kg_per_cubic_meter),
            MaterialProperty::YoungsModulusMpa => data.youngs_modulus_mpa,
            MaterialProperty::YieldStrengthMpa => data.yield_strength_mpa,
            MaterialProperty::UltimateTensileStrengthMpa => data.ultimate_tensile_strength_mpa,
            MaterialProperty::HeatDeflection045MpaC => data.heat_deflection_0_45_mpa_c,
            MaterialProperty::HeatDeflection180MpaC => data.heat_deflection_1_8_mpa_c,
            MaterialProperty::ContinuousServiceC => data.continuous_service_c,
            MaterialProperty::InterlayerStrengthMpa => data.interlayer_strength_mpa,
        };
        value.ok_or_else(|| {
            ModelError::new(format!(
                "{} does not print {} for material {}",
                data.source,
                property.name(),
                self.name()
            ))
        })
    }

    pub fn name(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }
}

impl MaterialProperty {
    pub fn name(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }
}

/// Which temperature a temperature-limit requirement compares against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MaterialTemperatureLimit {
    ContinuousService,
    #[serde(rename = "heat_deflection_0_45_mpa")]
    HeatDeflection045Mpa,
    #[serde(rename = "heat_deflection_1_80_mpa")]
    HeatDeflection180Mpa,
}

impl MaterialTemperatureLimit {
    pub fn property(self) -> MaterialProperty {
        match self {
            Self::ContinuousService => MaterialProperty::ContinuousServiceC,
            Self::HeatDeflection045Mpa => MaterialProperty::HeatDeflection045MpaC,
            Self::HeatDeflection180Mpa => MaterialProperty::HeatDeflection180MpaC,
        }
    }
}

/// Which strength a bending check divides by the peak stress.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MaterialStrength {
    #[default]
    Yield,
    UltimateTensile,
    /// Printed layer bond, for bending across the layers of a print.
    Interlayer,
}

impl MaterialStrength {
    pub fn property(self) -> MaterialProperty {
        match self {
            Self::Yield => MaterialProperty::YieldStrengthMpa,
            Self::UltimateTensile => MaterialProperty::UltimateTensileStrengthMpa,
            Self::Interlayer => MaterialProperty::InterlayerStrengthMpa,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_convert_from_the_printed_units() {
        let steel = MaterialGrade::StainlessSteel304.data();
        // 0.285 lb/in³, 29 × 10⁶ psi, 48 ksi.
        assert!((steel.density_kg_per_cubic_meter - 7_888.77).abs() < 0.01);
        assert!((steel.youngs_modulus_mpa.unwrap() - 199_948.0).abs() < 1.0);
        assert!((steel.yield_strength_mpa.unwrap() - 330.948).abs() < 1e-3);
        let pom = MaterialGrade::PomAcetalCopolymer.data();
        assert!((pom.continuous_service_c.unwrap() - 82.222).abs() < 1e-3);
        assert_eq!(
            MaterialGrade::Aluminum6061T6
                .property(MaterialProperty::YieldStrengthMpa)
                .unwrap(),
            276.0
        );
    }

    #[test]
    fn missing_values_are_errors_naming_the_source() {
        let error = MaterialGrade::TitaniumCnc
            .property(MaterialProperty::YoungsModulusMpa)
            .unwrap_err()
            .message;
        assert_eq!(
            error,
            "Proto Labs titanium (2026) does not print youngs_modulus_mpa for material titanium_cnc"
        );
        for grade in MaterialGrade::ALL {
            let data = grade.data();
            assert!(data.density_kg_per_cubic_meter > 500.0, "{grade:?}");
            assert!(!data.source.is_empty());
            for value in [
                data.youngs_modulus_mpa,
                data.yield_strength_mpa,
                data.ultimate_tensile_strength_mpa,
                data.interlayer_strength_mpa,
            ]
            .into_iter()
            .flatten()
            {
                assert!(value > 0.0 && value.is_finite(), "{grade:?}");
            }
        }
    }
}
