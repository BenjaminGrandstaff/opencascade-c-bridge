//! Family definitions: parameters, expressions, selectors, feature
//! operations, requirements, and constraints.

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterValue {
    Scalar(Quantity),
    Integer(i64),
    Boolean(bool),
    Choice(String),
    Vector(VectorQuantity),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterType {
    Scalar(Dimension),
    Integer,
    Boolean,
    Choice(Vec<String>),
    Vector(Dimension),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParameterDefinition {
    pub id: String,
    pub parameter_type: ParameterType,
    pub default: ParameterValue,
    pub minimum: Option<Quantity>,
    pub maximum: Option<Quantity>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalarExpr {
    Literal(Quantity),
    Parameter(String),
    /// Frozen metric clearance-hole catalog; returns a length.
    Iso273ClearanceV1 {
        nominal_diameter: Box<ScalarExpr>,
        series: ClearanceSeries,
    },
    Negate(Box<ScalarExpr>),
    Absolute(Box<ScalarExpr>),
    Add(Box<ScalarExpr>, Box<ScalarExpr>),
    Subtract(Box<ScalarExpr>, Box<ScalarExpr>),
    Multiply(Box<ScalarExpr>, Box<ScalarExpr>),
    Divide(Box<ScalarExpr>, Box<ScalarExpr>),
    Minimum(Box<ScalarExpr>, Box<ScalarExpr>),
    Maximum(Box<ScalarExpr>, Box<ScalarExpr>),
    Clamp {
        value: Box<ScalarExpr>,
        minimum: Box<ScalarExpr>,
        maximum: Box<ScalarExpr>,
    },
    Conditional {
        left: Box<ScalarExpr>,
        relation: ConstraintRelation,
        right: Box<ScalarExpr>,
        when_true: Box<ScalarExpr>,
        when_false: Box<ScalarExpr>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorExpr {
    Literal(VectorQuantity),
    Parameter(String),
    Components {
        x: ScalarExpr,
        y: ScalarExpr,
        z: ScalarExpr,
    },
    Add(Box<VectorExpr>, Box<VectorExpr>),
    Subtract(Box<VectorExpr>, Box<VectorExpr>),
    Scale {
        vector: Box<VectorExpr>,
        factor: ScalarExpr,
    },
    Normalize(Box<VectorExpr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticHistoryRelation {
    Generated,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateAxis {
    X,
    Y,
    Z,
}

impl CoordinateAxis {
    pub(crate) fn component(self, point: Vec3) -> f64 {
        match self {
            Self::X => point.x,
            Self::Y => point.y,
            Self::Z => point.z,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Extremum {
    Minimum,
    Maximum,
}

impl From<SemanticHistoryRelation> for HistoryRelation {
    fn from(value: SemanticHistoryRelation) -> Self {
        match value {
            SemanticHistoryRelation::Generated => Self::Generated,
            SemanticHistoryRelation::Modified => Self::Modified,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSelector {
    NearestCenter {
        target: VectorExpr,
        maximum_distance: ScalarExpr,
    },
    AtExtreme {
        axis: CoordinateAxis,
        extremum: Extremum,
        tolerance: ScalarExpr,
    },
    Longest {
        allow_ties: bool,
        relative_tolerance: ScalarExpr,
    },
    CircularRadius {
        minimum: ScalarExpr,
        maximum: ScalarExpr,
    },
    CurvatureRadius {
        minimum: ScalarExpr,
        maximum: ScalarExpr,
    },
    CurvatureRadiusRange {
        minimum: ScalarExpr,
        maximum: ScalarExpr,
        sample_count: usize,
        require_entire_edge: bool,
    },
    /// Curvature-radius range decided from exact or error-bounded extrema.
    /// Edges whose bounds straddle the range fail instead of guessing.
    CurvatureRadiusBounds {
        minimum: ScalarExpr,
        maximum: ScalarExpr,
        relative_tolerance: ScalarExpr,
        require_entire_edge: bool,
    },
    Union(Vec<EdgeSelector>),
    Intersection(Vec<EdgeSelector>),
    Difference {
        base: Box<EdgeSelector>,
        subtract: Box<EdgeSelector>,
    },
    History {
        source_feature: String,
        source: Box<EdgeSelector>,
        relation: SemanticHistoryRelation,
    },
}

impl EdgeSelector {
    pub(crate) fn dependencies<'a>(&'a self, dependencies: &mut Vec<&'a str>) {
        match self {
            Self::History {
                source_feature,
                source,
                ..
            } => {
                dependencies.push(source_feature);
                source.dependencies(dependencies);
            }
            Self::Union(selectors) | Self::Intersection(selectors) => {
                for selector in selectors {
                    selector.dependencies(dependencies);
                }
            }
            Self::Difference { base, subtract } => {
                base.dependencies(dependencies);
                subtract.dependencies(dependencies);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaceSelector {
    NearestCenter {
        target: VectorExpr,
        maximum_distance: ScalarExpr,
    },
    AtExtreme {
        axis: CoordinateAxis,
        extremum: Extremum,
        tolerance: ScalarExpr,
    },
    NormalAligned {
        direction: VectorExpr,
        minimum_dot: ScalarExpr,
    },
    LargestArea {
        planar_only: bool,
        allow_ties: bool,
        relative_tolerance: ScalarExpr,
    },
    AdjacentToEdges {
        edges: Box<EdgeSelector>,
        minimum_count: usize,
    },
    /// Faces generated from selected edges of an earlier feature, including
    /// rib profile edges traced through extrusion, placement, and fusion.
    GeneratedFromEdges {
        source_feature: String,
        source: Box<EdgeSelector>,
    },
    TangentTo {
        faces: Box<FaceSelector>,
        minimum_count: usize,
    },
    Union(Vec<FaceSelector>),
    Intersection(Vec<FaceSelector>),
    Difference {
        base: Box<FaceSelector>,
        subtract: Box<FaceSelector>,
    },
    History {
        source_feature: String,
        source: Box<FaceSelector>,
        relation: SemanticHistoryRelation,
    },
}

impl FaceSelector {
    pub(crate) fn dependencies<'a>(&'a self, dependencies: &mut Vec<&'a str>) {
        match self {
            Self::History {
                source_feature,
                source,
                ..
            } => {
                dependencies.push(source_feature);
                source.dependencies(dependencies);
            }
            Self::AdjacentToEdges { edges, .. } => edges.dependencies(dependencies),
            Self::GeneratedFromEdges {
                source_feature,
                source,
            } => {
                dependencies.push(source_feature);
                source.dependencies(dependencies);
            }
            Self::TangentTo { faces, .. } => faces.dependencies(dependencies),
            Self::Union(selectors) | Self::Intersection(selectors) => {
                for selector in selectors {
                    selector.dependencies(dependencies);
                }
            }
            Self::Difference { base, subtract } => {
                base.dependencies(dependencies);
                subtract.dependencies(dependencies);
            }
            Self::NearestCenter { .. }
            | Self::AtExtreme { .. }
            | Self::NormalAligned { .. }
            | Self::LargestArea { .. } => {}
        }
    }
}

/// A flat-bottom blind bore or a bore through the complete input along its
/// axis line. Through-all covers both directions from the supplied position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoleExtent {
    Blind { depth: ScalarExpr },
    ThroughAll,
}

/// Entry recess, starting at the hole position along its axis.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoleFinish {
    #[default]
    Plain,
    Counterbore {
        diameter: ScalarExpr,
        depth: ScalarExpr,
    },
    /// Included cone angle in scalar radians, strictly between zero and pi.
    Countersink {
        diameter: ScalarExpr,
        angle_radians: ScalarExpr,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadHandedness {
    Right,
    Left,
}

/// Caller-supplied internal-thread intent, not a standards lookup or modeled
/// helix. The hole diameter remains the explicit cylindrical bore diameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThreadSpecification {
    pub designation: String,
    pub nominal_diameter: ScalarExpr,
    pub pitch: ScalarExpr,
    pub handedness: ThreadHandedness,
}

/// How a rib's total thickness is placed relative to its profile plane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RibThicknessMode {
    /// Extrude from the profile plane along the supplied direction.
    #[default]
    OneSided,
    /// Place half the total thickness on each side of the profile plane.
    Centered,
}

/// The bounded planar region used before applying rib thickness.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RibProfileMode {
    #[default]
    Closed,
    /// Close an open chain with its translated reversed copy and straight end
    /// bridges. The length-valued offset must produce a simple planar boundary.
    OpenStrip { offset: VectorExpr },
    /// Advance a straight open chain perpendicularly to its first body contact.
    /// Direction is dimensionless; maximum_length is a positive bounded reach.
    /// The whole translated chain must meet the first contact.
    OpenToNext {
        direction: VectorExpr,
        maximum_length: ScalarExpr,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureOperation {
    Box {
        origin: VectorExpr,
        size: VectorExpr,
    },
    Cylinder {
        origin: VectorExpr,
        axis: VectorExpr,
        radius: ScalarExpr,
        height: ScalarExpr,
    },
    SketchFace {
        sketch: Box<SketchDefinition>,
    },
    SketchWire {
        sketch: Box<SketchDefinition>,
    },
    SketchOpenWire {
        sketch: Box<SketchDefinition>,
    },
    /// Sweeps a planar face or closed planar wire by a length-valued vector.
    Extrude {
        input: String,
        direction: VectorExpr,
    },
    /// Extrudes a planar rib region normally by positive thickness into one solid.
    /// OpenStrip explicitly closes an open profile; direction is dimensionless.
    Rib {
        input: String,
        profile: String,
        thickness: ScalarExpr,
        direction: VectorExpr,
        #[serde(default)]
        thickness_mode: RibThicknessMode,
        #[serde(default)]
        profile_mode: RibProfileMode,
    },
    /// Revolves a planar face or closed planar wire about a local axis.
    Revolve {
        input: String,
        origin: VectorExpr,
        axis: VectorExpr,
        angle_radians: ScalarExpr,
    },
    Translate {
        input: String,
        offset: VectorExpr,
    },
    Rotate {
        input: String,
        origin: VectorExpr,
        axis: VectorExpr,
        angle_radians: ScalarExpr,
    },
    Fuse {
        left: String,
        right: String,
    },
    Cut {
        object: String,
        tool: String,
    },
    /// Removes a cylindrical bore from one solid. Position is length-valued,
    /// axis is dimensionless; diameter and blind depth are positive lengths.
    Hole {
        input: String,
        position: VectorExpr,
        axis: VectorExpr,
        diameter: ScalarExpr,
        extent: HoleExtent,
        #[serde(default)]
        finish: HoleFinish,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread: Option<Box<ThreadSpecification>>,
    },
    Common {
        left: String,
        right: String,
    },
    Sew {
        inputs: Vec<String>,
        tolerance: ScalarExpr,
    },
    MakeSolid {
        shells: Vec<String>,
    },
    Fillet {
        input: String,
        edges: Vec<EdgeSelector>,
        radius: ScalarExpr,
    },
    /// Linear radius evolution along each selected open tangent contour, in
    /// OCCT spine order. Endpoint radii are finite positive lengths.
    VariableFillet {
        input: String,
        edges: Vec<EdgeSelector>,
        start_radius: ScalarExpr,
        end_radius: ScalarExpr,
    },
    Chamfer {
        input: String,
        edges: Vec<EdgeSelector>,
        distance: ScalarExpr,
    },
    Hollow {
        input: String,
        faces: Vec<FaceSelector>,
        thickness: ScalarExpr,
        tolerance: ScalarExpr,
    },
    Draft {
        input: String,
        faces: Vec<FaceSelector>,
        neutral_origin: VectorExpr,
        neutral_normal: VectorExpr,
        pull_direction: VectorExpr,
        angle_radians: ScalarExpr,
    },
}

impl FeatureOperation {
    pub(crate) fn dependencies(&self) -> Vec<&str> {
        match self {
            Self::Translate { input, .. }
            | Self::Rotate { input, .. }
            | Self::Extrude { input, .. }
            | Self::Revolve { input, .. }
            | Self::Hole { input, .. } => vec![input],
            Self::Fillet { input, edges, .. }
            | Self::VariableFillet { input, edges, .. }
            | Self::Chamfer { input, edges, .. } => {
                let mut dependencies = vec![input.as_str()];
                for selector in edges {
                    selector.dependencies(&mut dependencies);
                }
                dependencies
            }
            Self::Hollow { input, faces, .. } | Self::Draft { input, faces, .. } => {
                let mut dependencies = vec![input.as_str()];
                for selector in faces {
                    selector.dependencies(&mut dependencies);
                }
                dependencies
            }
            Self::Fuse { left, right } | Self::Common { left, right } => vec![left, right],
            Self::Rib { input, profile, .. } => vec![input, profile],
            Self::Cut { object, tool } => vec![object, tool],
            Self::Sew { inputs, .. } => inputs.iter().map(String::as_str).collect(),
            Self::MakeSolid { shells } => shells.iter().map(String::as_str).collect(),
            Self::Box { .. }
            | Self::Cylinder { .. }
            | Self::SketchFace { .. }
            | Self::SketchWire { .. }
            | Self::SketchOpenWire { .. } => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureDefinition {
    pub id: String,
    pub operation: FeatureOperation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementPriority {
    Required,
    Preferred,
    Advisory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Requirement {
    pub id: String,
    pub version: u32,
    pub kind: RequirementKind,
    pub priority: RequirementPriority,
    pub statement: String,
    pub rule: VerificationRule,
    pub provenance: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DerivedParameterDefinition {
    pub id: String,
    pub dimension: Dimension,
    pub expression: ScalarExpr,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DerivedVectorParameterDefinition {
    pub id: String,
    pub dimension: Dimension,
    pub expression: VectorExpr,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintRelation {
    LessOrEqual,
    GreaterOrEqual,
    Equal { tolerance: Quantity },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParameterConstraint {
    pub id: String,
    pub statement: String,
    pub left: ScalarExpr,
    pub relation: ConstraintRelation,
    pub right: ScalarExpr,
    pub provenance: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyDefinition {
    pub id: String,
    pub version: u32,
    pub parameters: Vec<ParameterDefinition>,
    #[serde(default)]
    pub derived_parameters: Vec<DerivedParameterDefinition>,
    #[serde(default)]
    pub derived_vector_parameters: Vec<DerivedVectorParameterDefinition>,
    #[serde(default)]
    pub constraints: Vec<ParameterConstraint>,
    pub features: Vec<FeatureDefinition>,
    pub requirements: Vec<Requirement>,
    /// Named points, axes, and planes in family coordinates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datums: Vec<DatumDefinition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartInstance<'definition> {
    pub id: String,
    pub definition: &'definition FamilyDefinition,
    pub overrides: HashMap<String, ParameterValue>,
    pub provenance: String,
}
