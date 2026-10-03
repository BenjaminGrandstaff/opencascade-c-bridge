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
    pub(crate) fn exact(requirement_id: &str, passed: bool, message: String) -> Self {
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

    pub(crate) fn measured(mut self, measurement: Measurement) -> Self {
        self.measured = Some(measurement);
        self
    }

    pub(crate) fn witnessed(mut self, witness: Witness) -> Self {
        self.witness = Some(witness);
        self
    }
}

/// Exact solid and loose-topology counts of one output.
pub(crate) struct Connectivity {
    pub(crate) solids: usize,
    /// Largest number of shells in any one solid (more than one means voids).
    pub(crate) maximum_shells_per_solid: usize,
    /// Faces, edges, and vertices that belong to no solid.
    pub(crate) loose_faces: usize,
    pub(crate) loose_edges: usize,
    pub(crate) loose_vertices: usize,
}

/// O(topology) time and transient handles: each solid's subshapes are mapped
/// to the output's unique topology indices once, then released.
pub(crate) fn connectivity(
    session: &Session,
    shape: &Shape<'_>,
) -> Result<Connectivity, ModelError> {
    // Descendant traversal excludes the shape itself, so a bare solid has no
    // solid subshapes and, being one solid, no loose topology.
    let shape_type = session.shape_type(shape)?;
    if shape_type == ShapeType::Solid {
        return Ok(Connectivity {
            solids: 1,
            maximum_shells_per_solid: session.subshape_count(shape, ShapeType::Shell)?,
            loose_faces: 0,
            loose_edges: 0,
            loose_vertices: 0,
        });
    }
    let solids = session.subshapes(shape, ShapeType::Solid)?;
    let mut maximum_shells_per_solid = 0;
    for solid in &solids {
        maximum_shells_per_solid =
            maximum_shells_per_solid.max(session.subshape_count(solid, ShapeType::Shell)?);
    }
    let mut loose = [0; 3];
    for (slot, kind) in [ShapeType::Face, ShapeType::Edge, ShapeType::Vertex]
        .into_iter()
        .enumerate()
    {
        // A bare face, edge, or vertex output is itself loose topology.
        let total = session.subshape_count(shape, kind)? + usize::from(shape_type == kind);
        let mut owned = vec![false; total];
        for solid in &solids {
            let members = session.subshapes(solid, kind)?;
            let references = members.iter().collect::<Vec<_>>();
            for index in session.subshape_indices(shape, kind, &references)? {
                owned[index] = true;
            }
        }
        loose[slot] = owned.iter().filter(|owned| !**owned).count();
    }
    Ok(Connectivity {
        solids: solids.len(),
        maximum_shells_per_solid,
        loose_faces: loose[0],
        loose_edges: loose[1],
        loose_vertices: loose[2],
    })
}
