//! Requirements, verification rules, traceability targets, and assumptions.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequirementPriority {
    Required,
    Preferred,
    Advisory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequirementKind {
    Dimensional,
    Geometric,
    Topological,
    Functional,
    Interface,
    Manufacturing,
    Assembly,
    Validation,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Volume {
    pub value: f64,
    pub unit: LengthUnit,
}

impl Volume {
    pub(crate) fn cubic_millimeters(self) -> Result<f64, ModelError> {
        if !self.value.is_finite() {
            return Err(ModelError::new("volume is not finite"));
        }
        Ok(self.value * self.unit.millimeter_factor().powi(3))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationRule {
    ShapeValid {
        output: String,
    },
    VolumeRange {
        output: String,
        minimum: Volume,
        maximum: Volume,
    },
    /// Exactly `solids` solids and no faces, edges, or vertices outside them.
    /// A solid with more than one shell (an internal void) fails unless
    /// `allow_voids` is set.
    Connectivity {
        output: String,
        solids: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        allow_voids: bool,
    },
    /// Every face radius on `side` is at least `minimum` (a positive length).
    /// Exact on planes, cylinders, cones, spheres, and tori; other faces are
    /// sampled on a `samples_per_direction` squared grid, in [2, 1024].
    MinimumRadius {
        output: String,
        minimum: Quantity,
        side: RadiusSide,
        sharp_edges: SharpEdges,
        #[serde(
            default = "default_radius_samples",
            skip_serializing_if = "is_default_radius_samples"
        )]
        samples_per_direction: u32,
    },
    /// Sampled: inward normal rays from up to `maximum_samples` triangle
    /// centroids of the output's tessellation must travel at least `minimum`
    /// (a positive length) before leaving the material.
    MinimumWall {
        output: String,
        minimum: Quantity,
        #[serde(default, skip_serializing_if = "MeshSettings::is_default")]
        mesh: MeshSettings,
        #[serde(
            default = "default_wall_samples",
            skip_serializing_if = "is_default_wall_samples"
        )]
        maximum_samples: usize,
    },
    /// Every face not perpendicular to the dimensionless `pull_direction`
    /// has draft of at least `minimum_radians`, in [0, pi/2), leaning either
    /// way. Exact on analytic faces (planes, cylinders, cones, and spheres or
    /// tori whose extremes lie on the face); other faces use facet draft.
    DraftAngle {
        output: String,
        pull_direction: VectorQuantity,
        minimum_radians: f64,
        #[serde(default, skip_serializing_if = "MeshSettings::is_default")]
        mesh: MeshSettings,
    },
    /// With the mold parted by the plane through the length-valued
    /// `parting_origin` normal to the dimensionless `pull_direction`, no
    /// face above the plane turns against the pull, and none below turns
    /// along it, by more than `tolerance_radians`, in [0, pi/2). Exact on
    /// analytic faces, facet-sampled on others. For a planar parting and a
    /// straight pull this is complete: material that traps a face along the
    /// pull is entered through a face turned against it on the same side.
    Undercut {
        output: String,
        pull_direction: VectorQuantity,
        parting_origin: VectorQuantity,
        #[serde(default)]
        tolerance_radians: f64,
        #[serde(default, skip_serializing_if = "MeshSettings::is_default")]
        mesh: MeshSettings,
    },
    /// Exact: the output's bounding box, in family axes, fits inside the
    /// length-valued `envelope` (such as a print bed) in some axis-aligned
    /// orientation. Rotations that are not quarter turns are not searched.
    FitsWithin {
        output: String,
        envelope: VectorQuantity,
    },
    /// Sampled: no downward-facing facet above the lowest build plane leans
    /// more than `maximum_radians`, in [0, pi/2], from vertical.
    Overhang {
        output: String,
        build_direction: VectorQuantity,
        maximum_radians: f64,
        #[serde(default, skip_serializing_if = "MeshSettings::is_default")]
        mesh: MeshSettings,
    },
}

pub const DEFAULT_WALL_SAMPLES: usize = 1_000;

pub(crate) fn sorted_extents(value: Vec3) -> [f64; 3] {
    let mut extents = [value.x, value.y, value.z];
    extents.sort_by(f64::total_cmp);
    extents
}

fn default_wall_samples() -> usize {
    DEFAULT_WALL_SAMPLES
}

fn is_default_wall_samples(value: &usize) -> bool {
    *value == DEFAULT_WALL_SAMPLES
}

/// Which way a surface curves relative to the part's outward normal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RadiusSide {
    /// Outside surfaces: cylinders, spheres, fillets on outside corners.
    Convex,
    /// Inside surfaces: bores and fillets in inside corners.
    Concave,
    Both,
}

/// Whether sharp (non-tangent) edges on the measured side count as radius zero.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SharpEdges {
    /// Measure curved faces only, such as checking fillet sizes.
    Ignore,
    /// A sharp edge on the measured side has radius zero, such as an inside
    /// corner a round cutter cannot reach. Faces meeting within
    /// `tangency_radians`, in (0, pi/2), are smooth.
    ZeroRadius { tangency_radians: f64 },
}

pub const DEFAULT_RADIUS_SAMPLES: u32 = 17;

fn default_radius_samples() -> u32 {
    DEFAULT_RADIUS_SAMPLES
}

fn is_default_radius_samples(value: &u32) -> bool {
    *value == DEFAULT_RADIUS_SAMPLES
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Requirement {
    pub id: String,
    pub version: u32,
    pub kind: RequirementKind,
    pub priority: RequirementPriority,
    pub statement: String,
    pub rule: VerificationRule,
    pub provenance: String,
    /// The design items this requirement constrains or rests on, beyond its
    /// rule's own output. Change impact reports the requirement when any of
    /// them changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traces: Vec<TraceTarget>,
}

/// A design item a requirement traces to.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TraceTarget {
    Feature(String),
    /// An input, derived scalar, or derived vector parameter.
    Parameter(String),
    Assumption(String),
}

/// A stated engineering assumption, such as a load case or a material
/// property, that requirements can trace to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Assumption {
    pub id: String,
    pub statement: String,
    pub provenance: String,
}

impl VerificationRule {
    /// The named output the rule checks.
    pub fn output(&self) -> &str {
        match self {
            Self::ShapeValid { output }
            | Self::VolumeRange { output, .. }
            | Self::Connectivity { output, .. }
            | Self::MinimumRadius { output, .. }
            | Self::MinimumWall { output, .. }
            | Self::DraftAngle { output, .. }
            | Self::Undercut { output, .. }
            | Self::FitsWithin { output, .. }
            | Self::Overhang { output, .. } => output,
        }
    }
}
