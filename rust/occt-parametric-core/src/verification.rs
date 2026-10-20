//! Requirement verification results: measured values, evidence quality, and
//! witnesses, plus exact topology measurements used by part rules.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationStatus {
    Passed,
    Failed,
}

/// How strongly a result supports its status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    /// Computed from exact BREP geometry or topology.
    Exact,
    /// Screened at finitely many samples: a failure is a real violation, but a
    /// pass does not prove that no violation exists between samples.
    Sampled { samples: usize, unresolved: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasurementUnit {
    Count,
    Millimeter,
    CubicMillimeter,
    Kilogram,
    Radian,
    DegreeCelsius,
    /// A dimensionless ratio, such as a safety factor.
    Ratio,
}

/// The value a rule measured and the limits it was compared against, in
/// normalized units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurement {
    pub value: f64,
    pub unit: MeasurementUnit,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}

/// Where a failing (or limiting) value was found: the instances or outputs
/// involved and model-space points in millimeters.
#[derive(Clone, Debug, PartialEq)]
pub struct Witness {
    pub subjects: Vec<String>,
    pub points_mm: Vec<Vec3>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VerificationResult {
    pub requirement_id: String,
    pub status: VerificationStatus,
    pub message: String,
    pub measured: Option<Measurement>,
    pub evidence: Evidence,
    pub witness: Option<Witness>,
}

impl VerificationResult {
    pub fn exact(requirement_id: &str, passed: bool, message: String) -> Self {
        Self {
            requirement_id: requirement_id.to_owned(),
            status: if passed {
                VerificationStatus::Passed
            } else {
                VerificationStatus::Failed
            },
            message,
            measured: None,
            evidence: Evidence::Exact,
            witness: None,
        }
    }

    pub fn measured(mut self, measurement: Measurement) -> Self {
        self.measured = Some(measurement);
        self
    }

    pub fn witnessed(mut self, witness: Witness) -> Self {
        self.witness = Some(witness);
        self
    }
}

/// Exact solid and loose-topology counts of one output.
pub struct Connectivity {
    pub solids: usize,
    /// Largest number of shells in any one solid (more than one means voids).
    pub maximum_shells_per_solid: usize,
    /// Faces, edges, and vertices that belong to no solid.
    pub loose_faces: usize,
    pub loose_edges: usize,
    pub loose_vertices: usize,
}

/// Relative roundoff allowed when comparing a radius with its minimum.
pub const RADIUS_ROUNDOFF: f64 = 1e-12;

pub struct Smallest {
    pub radius: f64,
    pub subject: String,
    pub point: Vec3,
}

pub fn consider(
    smallest: &mut Option<Smallest>,
    radius: f64,
    subject: impl FnOnce() -> String,
    point: Vec3,
) {
    if smallest.as_ref().is_none_or(|found| radius < found.radius) {
        *smallest = Some(Smallest {
            radius,
            subject: subject(),
            point,
        });
    }
}
