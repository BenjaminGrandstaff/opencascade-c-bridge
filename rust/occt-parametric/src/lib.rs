//! Unit-aware part families, instances, feature graphs, and verification.

mod assembly;
mod solve;

pub use assembly::{
    AssemblyRelationship, AssemblySemantics, Configuration, DatumDefinition, DatumKind, DatumRef,
    Material, RELATIONSHIP_ANGULAR_TOLERANCE, RELATIONSHIP_LINEAR_TOLERANCE, RelationKind,
    RelationshipCheck, ResolvedDatum,
};
pub use solve::PlacementSolution;

use occt_bridge::{
    BridgeError, CurvatureExtrema, HistoryRelation, Session, Shape, ShapeType, Vec3,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    fmt,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Scalar,
    Length,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    Millimeter,
    Centimeter,
    Meter,
    Inch,
}

impl LengthUnit {
    const fn millimeter_factor(self) -> f64 {
        match self {
            Self::Millimeter => 1.0,
            Self::Centimeter => 10.0,
            Self::Meter => 1_000.0,
            Self::Inch => 25.4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quantity {
    pub value: f64,
    pub dimension: Dimension,
    pub unit: Option<LengthUnit>,
}

impl Quantity {
    pub const fn scalar(value: f64) -> Self {
        Self {
            value,
            dimension: Dimension::Scalar,
            unit: None,
        }
    }

    pub const fn length(value: f64, unit: LengthUnit) -> Self {
        Self {
            value,
            dimension: Dimension::Length,
            unit: Some(unit),
        }
    }

    fn normalized(self) -> Result<f64, ModelError> {
        if !self.value.is_finite() {
            return Err(ModelError::new("quantity is not finite"));
        }
        match (self.dimension, self.unit) {
            (Dimension::Scalar, None) => Ok(self.value),
            (Dimension::Length, Some(unit)) => Ok(self.value * unit.millimeter_factor()),
            _ => Err(ModelError::new(
                "quantity dimension and unit are inconsistent",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorQuantity {
    pub x: Quantity,
    pub y: Quantity,
    pub z: Quantity,
}

impl VectorQuantity {
    pub const fn lengths(x: f64, y: f64, z: f64, unit: LengthUnit) -> Self {
        Self {
            x: Quantity::length(x, unit),
            y: Quantity::length(y, unit),
            z: Quantity::length(z, unit),
        }
    }

    pub const fn scalars(x: f64, y: f64, z: f64) -> Self {
        Self {
            x: Quantity::scalar(x),
            y: Quantity::scalar(y),
            z: Quantity::scalar(z),
        }
    }

    fn scaled(self, factor: f64) -> Self {
        Self {
            x: Quantity {
                value: self.x.value * factor,
                ..self.x
            },
            y: Quantity {
                value: self.y.value * factor,
                ..self.y
            },
            z: Quantity {
                value: self.z.value * factor,
                ..self.z
            },
        }
    }

    fn normalized(self, dimension: Dimension) -> Result<Vec3, ModelError> {
        for component in [self.x, self.y, self.z] {
            if component.dimension != dimension {
                return Err(ModelError::new("vector component has the wrong dimension"));
            }
        }
        Ok(Vec3::new(
            self.x.normalized()?,
            self.y.normalized()?,
            self.z.normalized()?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisAngle {
    pub origin: VectorQuantity,
    pub axis: VectorQuantity,
    pub angle_radians: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub translation: VectorQuantity,
    pub rotation: Option<AxisAngle>,
}

impl Placement {
    pub const fn identity() -> Self {
        Self {
            translation: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            rotation: None,
        }
    }

    pub const fn translated(translation: VectorQuantity) -> Self {
        Self {
            translation,
            rotation: None,
        }
    }

    fn normalized(self) -> Result<NormalizedPlacement, ModelError> {
        let translation = self.translation.normalized(Dimension::Length)?;
        let rotation = self
            .rotation
            .map(|rotation| {
                if !rotation.angle_radians.is_finite() {
                    return Err(ModelError::new("placement angle is not finite"));
                }
                let axis = rotation.axis.normalized(Dimension::Scalar)?;
                if axis.x.hypot(axis.y.hypot(axis.z)) <= f64::EPSILON {
                    return Err(ModelError::new("placement rotation axis is zero"));
                }
                Ok((
                    rotation.origin.normalized(Dimension::Length)?,
                    axis,
                    rotation.angle_radians,
                ))
            })
            .transpose()?;
        Ok(NormalizedPlacement {
            translation,
            rotation,
        })
    }
}

impl Default for Placement {
    fn default() -> Self {
        Self::identity()
    }
}

struct NormalizedPlacement {
    translation: Vec3,
    rotation: Option<(Vec3, Vec3, f64)>,
}

impl NormalizedPlacement {
    fn translation_is_zero(&self) -> bool {
        self.translation.x == 0.0 && self.translation.y == 0.0 && self.translation.z == 0.0
    }
}

fn placements_equivalent(left: Placement, right: Placement) -> Result<bool, ModelError> {
    let left = left.normalized()?;
    let right = right.normalized()?;
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
    let vector_close = |a: Vec3, b: Vec3| close(a.x, b.x) && close(a.y, b.y) && close(a.z, b.z);
    if !vector_close(left.translation, right.translation) {
        return Ok(false);
    }
    Ok(match (left.rotation, right.rotation) {
        (None, None) => true,
        (
            Some((left_origin, left_axis, left_angle)),
            Some((right_origin, right_axis, right_angle)),
        ) => {
            vector_close(left_origin, right_origin)
                && vector_close(left_axis, right_axis)
                && close(left_angle, right_angle)
        }
        _ => false,
    })
}

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
    fn component(self, point: Vec3) -> f64 {
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
    fn dependencies<'a>(&'a self, dependencies: &mut Vec<&'a str>) {
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
    fn dependencies<'a>(&'a self, dependencies: &mut Vec<&'a str>) {
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
}

impl FeatureOperation {
    fn dependencies(&self) -> Vec<&str> {
        match self {
            Self::Translate { input, .. } | Self::Rotate { input, .. } => vec![input],
            Self::Fillet { input, edges, .. } | Self::Chamfer { input, edges, .. } => {
                let mut dependencies = vec![input.as_str()];
                for selector in edges {
                    selector.dependencies(&mut dependencies);
                }
                dependencies
            }
            Self::Hollow { input, faces, .. } => {
                let mut dependencies = vec![input.as_str()];
                for selector in faces {
                    selector.dependencies(&mut dependencies);
                }
                dependencies
            }
            Self::Fuse { left, right } | Self::Common { left, right } => vec![left, right],
            Self::Cut { object, tool } => vec![object, tool],
            Self::Sew { inputs, .. } => inputs.iter().map(String::as_str).collect(),
            Self::MakeSolid { shells } => shells.iter().map(String::as_str).collect(),
            Self::Box { .. } | Self::Cylinder { .. } => Vec::new(),
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
    fn cubic_millimeters(self) -> Result<f64, ModelError> {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceNode {
    Base {
        id: String,
        /// `None` selects the graph's primary family. Named families are
        /// registered on the graph and serialized in the document.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        family: Option<String>,
        overrides: HashMap<String, ParameterValue>,
        #[serde(default)]
        placement: Placement,
        #[serde(default)]
        frame: Option<String>,
        provenance: String,
    },
    Clone {
        id: String,
        source: String,
        overrides: HashMap<String, ParameterValue>,
        #[serde(default)]
        placement: Placement,
        #[serde(default)]
        frame: Option<String>,
        provenance: String,
    },
}

impl InstanceNode {
    pub fn id(&self) -> &str {
        match self {
            Self::Base { id, .. } | Self::Clone { id, .. } => id,
        }
    }

    pub fn overrides(&self) -> &HashMap<String, ParameterValue> {
        match self {
            Self::Base { overrides, .. } | Self::Clone { overrides, .. } => overrides,
        }
    }

    pub fn placement(&self) -> Placement {
        match self {
            Self::Base { placement, .. } | Self::Clone { placement, .. } => *placement,
        }
    }

    pub fn frame(&self) -> Option<&str> {
        match self {
            Self::Base { frame, .. } | Self::Clone { frame, .. } => frame.as_deref(),
        }
    }

    fn frame_mut(&mut self) -> &mut Option<String> {
        match self {
            Self::Base { frame, .. } | Self::Clone { frame, .. } => frame,
        }
    }

    fn overrides_mut(&mut self) -> &mut HashMap<String, ParameterValue> {
        match self {
            Self::Base { overrides, .. } | Self::Clone { overrides, .. } => overrides,
        }
    }
}

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
    fn validate(&self) -> Result<(), ModelError> {
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
    fn member_placement(&self, index: usize, count: usize) -> Placement {
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

const CLOSED_SWEEP_TOLERANCE: f64 = 1e-9;

fn is_closed_sweep(sweep_radians: f64) -> bool {
    (sweep_radians - std::f64::consts::TAU).abs() <= CLOSED_SWEEP_TOLERANCE
}

fn slot_fraction(index: usize, gaps: usize) -> f64 {
    if gaps == 0 {
        0.0
    } else {
        index as f64 / gaps as f64
    }
}

fn rotation_about(origin: VectorQuantity, axis: VectorQuantity, angle_radians: f64) -> Placement {
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
fn snapped_ratio(value: f64, divisor: f64) -> f64 {
    let ratio = value / divisor;
    let nearest = ratio.round();
    if (ratio - nearest).abs() <= 1e-9 * nearest.abs().max(1.0) {
        nearest
    } else {
        ratio
    }
}

fn checked_member_count(count: f64) -> Result<usize, ModelError> {
    if count.is_finite() && (1.0..=MAX_PATTERN_MEMBERS as f64).contains(&count) {
        Ok(count as usize)
    } else {
        Err(ModelError::new(format!(
            "pattern constraints must yield 1..={MAX_PATTERN_MEMBERS} members"
        )))
    }
}

impl LinearSpacing {
    fn count(self, length: f64) -> Result<usize, ModelError> {
        let gaps = match self {
            Self::Count(count) => return checked_member_count(count as f64),
            Self::Minimum(spacing) => snapped_ratio(length, positive_spacing(spacing)?).floor(),
            Self::Maximum(spacing) => snapped_ratio(length, positive_spacing(spacing)?).ceil(),
        };
        checked_member_count(gaps + 1.0)
    }
}

fn positive_spacing(spacing: Quantity) -> Result<f64, ModelError> {
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
    fn count(self, sweep: f64, closed: bool) -> Result<usize, ModelError> {
        let gaps = match self {
            Self::Count(count) => return checked_member_count(count as f64),
            Self::MinimumRadians(angle) => snapped_ratio(sweep, positive_angle(angle)?).floor(),
            Self::MaximumRadians(angle) => snapped_ratio(sweep, positive_angle(angle)?).ceil(),
        };
        checked_member_count(if closed { gaps } else { gaps + 1.0 })
    }
}

fn positive_angle(angle: f64) -> Result<f64, ModelError> {
    if angle.is_finite() && angle > 0.0 {
        Ok(angle)
    } else {
        Err(ModelError::new(
            "angular pattern spacing must be positive and finite",
        ))
    }
}

fn validate_pattern_driver_rule(
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

fn normalized_pattern_direction(direction: VectorQuantity) -> Result<Vec3, ModelError> {
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

/// A named assembly coordinate frame. Its placement maps frame-local
/// coordinates into the parent frame, or into model coordinates at the root.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssemblyFrame {
    pub id: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub placement: Placement,
    pub provenance: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationRecord {
    pub instance_id: String,
    pub attempted_revision: u64,
    pub accepted_revision: Option<u64>,
    pub state: RegenerationState,
    pub last_error: Option<String>,
}

pub const CURRENT_SCHEMA_VERSION: u32 = 22;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelDocument {
    pub schema_version: u32,
    pub family: FamilyDefinition,
    #[serde(default)]
    pub additional_families: Vec<FamilyDefinition>,
    pub instances: Vec<InstanceNode>,
    #[serde(default)]
    pub patterns: Vec<Pattern>,
    #[serde(default)]
    pub frames: Vec<AssemblyFrame>,
    #[serde(default)]
    pub generation_records: Vec<GenerationRecord>,
    /// Relationships, configurations, and materials.
    #[serde(default)]
    pub assembly: AssemblySemantics,
}

impl ModelDocument {
    pub fn from_graph(graph: &InstanceGraph<'_>) -> Self {
        let mut instances = graph.nodes.values().cloned().collect::<Vec<_>>();
        instances.sort_by(|left, right| left.id().cmp(right.id()));
        let mut frames = graph.frames.values().cloned().collect::<Vec<_>>();
        frames.sort_by(|left, right| left.id.cmp(&right.id));
        let mut additional_families = graph
            .additional_definitions
            .values()
            .map(|definition| (*definition).clone())
            .collect::<Vec<_>>();
        additional_families.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            family: graph.definition.clone(),
            additional_families,
            instances,
            patterns: graph.patterns.clone(),
            frames,
            generation_records: Vec::new(),
            assembly: AssemblySemantics {
                active_configuration: None,
                ..graph.assembly.clone()
            },
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, ModelError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| ModelError::new(format!("serialize model document: {error}")))
    }

    pub fn from_json(json: &str) -> Result<Self, ModelError> {
        let mut value: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| ModelError::new(format!("parse model document: {error}")))?;
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| ModelError::new("model document schema_version is required"))?;
        if version == 0 || version > u64::from(CURRENT_SCHEMA_VERSION) {
            return Err(ModelError::new(format!(
                "unsupported model document schema version {version}"
            )));
        }
        if version < 14 {
            migrate_linear_pattern_steps(&mut value)?;
        }
        if version < 17 {
            migrate_pattern_member_slots(&mut value)?;
        }
        let mut document: Self = serde_json::from_value(value)
            .map_err(|error| ModelError::new(format!("decode model document: {error}")))?;
        if version < 18 {
            document.adopt_pattern_slots();
        }
        if version < 17 {
            document.adopt_member_placements();
        }
        document.schema_version = CURRENT_SCHEMA_VERSION;
        document.validate()?;
        Ok(document)
    }

    pub fn instance_graph(&self) -> Result<InstanceGraph<'_>, ModelError> {
        self.validate()?;
        let nodes = self
            .instances
            .iter()
            .cloned()
            .map(|node| (node.id().to_owned(), node))
            .collect();
        Ok(InstanceGraph {
            definition: &self.family,
            additional_definitions: self
                .additional_families
                .iter()
                .map(|definition| (definition.id.clone(), definition))
                .collect(),
            nodes,
            patterns: self.patterns.clone(),
            frames: self.frame_map(),
            assembly: self.assembly.clone(),
        })
    }

    /// Schema 17 derives member placements from the rule; a stored placement
    /// that differs from its rule slot becomes an explicit override.
    fn adopt_member_placements(&mut self) {
        let placements: HashMap<&str, Placement> = self
            .instances
            .iter()
            .map(|node| (node.id(), node.placement()))
            .collect();
        for pattern in &mut self.patterns {
            let rule = pattern.rule;
            for member in &mut pattern.members {
                let stored = placements.get(member.id.as_str()).copied();
                let expected = rule.member_placement(member.index, pattern.slot_count);
                if stored.is_some_and(|stored| stored != expected) {
                    member.placement_override = stored;
                }
            }
        }
    }

    /// Schema 18 records the slot count and the prefix for grown members.
    fn adopt_pattern_slots(&mut self) {
        for pattern in &mut self.patterns {
            if pattern.slot_count == 0 {
                pattern.slot_count = pattern
                    .members
                    .iter()
                    .map(|member| member.index + 1)
                    .max()
                    .unwrap_or(0);
            }
            if pattern.member_prefix.is_empty() {
                pattern.member_prefix = derived_member_prefix(pattern);
            }
        }
    }

    fn frame_map(&self) -> HashMap<String, AssemblyFrame> {
        self.frames
            .iter()
            .cloned()
            .map(|frame| (frame.id.clone(), frame))
            .collect()
    }

    fn validate(&self) -> Result<(), ModelError> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(ModelError::new(format!(
                "model document must be migrated to schema version {CURRENT_SCHEMA_VERSION}"
            )));
        }
        validate_definition(&self.family)?;
        resolve_parameters(&self.family, &HashMap::new())?;
        let mut family_ids = HashSet::from([self.family.id.as_str()]);
        for family in &self.additional_families {
            if family.id.is_empty() || !family_ids.insert(family.id.as_str()) {
                return Err(ModelError::new(
                    "document family ids must be nonempty and unique",
                ));
            }
            validate_definition(family)?;
            resolve_parameters(family, &HashMap::new())?;
        }
        let node_ids = self.validate_instance_ids()?;
        let graph = InstanceGraph {
            definition: &self.family,
            additional_definitions: self
                .additional_families
                .iter()
                .map(|definition| (definition.id.clone(), definition))
                .collect(),
            nodes: self
                .instances
                .iter()
                .cloned()
                .map(|node| (node.id().to_owned(), node))
                .collect(),
            patterns: self.patterns.clone(),
            frames: self.frame_map(),
            assembly: self.assembly.clone(),
        };
        self.validate_frames(&graph)?;
        for node in &self.instances {
            let resolved = graph.resolve(node.id())?;
            resolve_parameters(resolved.definition, &resolved.overrides)?;
        }
        let mut pattern_ids = HashSet::new();
        let mut patterned_members = HashSet::new();
        for pattern in &self.patterns {
            if pattern.id.is_empty() || !pattern_ids.insert(pattern.id.as_str()) {
                return Err(ModelError::new(
                    "document pattern ids must be nonempty and unique",
                ));
            }
            validate_pattern(pattern, &graph, &node_ids, &mut patterned_members)?;
        }
        let mut recorded_instances = HashSet::new();
        for record in &self.generation_records {
            if !recorded_instances.insert(record.instance_id.as_str()) {
                return Err(ModelError::new(format!(
                    "duplicate generation record for instance '{}'",
                    record.instance_id
                )));
            }
            validate_generation_record(record, &node_ids)?;
        }
        graph.validate_assembly()
    }

    fn validate_instance_ids(&self) -> Result<HashSet<&str>, ModelError> {
        let mut node_ids = HashSet::new();
        for node in &self.instances {
            if node.id().is_empty() || !node_ids.insert(node.id()) {
                return Err(ModelError::new(
                    "document instance ids must be nonempty and unique",
                ));
            }
            node.placement().normalized()?;
        }
        Ok(node_ids)
    }

    fn validate_frames(&self, graph: &InstanceGraph<'_>) -> Result<(), ModelError> {
        let mut frame_ids = HashSet::new();
        if self
            .frames
            .iter()
            .any(|frame| frame.id.is_empty() || !frame_ids.insert(frame.id.as_str()))
        {
            return Err(ModelError::new(
                "document frame ids must be nonempty and unique",
            ));
        }
        for frame in &self.frames {
            graph.frame_chain(Some(&frame.id))?;
        }
        for node in &self.instances {
            graph.frame_chain(node.frame()).map_err(|error| {
                ModelError::new(format!("instance '{}': {}", node.id(), error.message))
            })?;
        }
        Ok(())
    }
}

fn validate_pattern<'document>(
    pattern: &'document Pattern,
    graph: &InstanceGraph<'_>,
    node_ids: &HashSet<&str>,
    patterned_members: &mut HashSet<&'document str>,
) -> Result<(), ModelError> {
    let context =
        |error: ModelError| ModelError::new(format!("pattern '{}': {}", pattern.id, error.message));
    if !node_ids.contains(pattern.source.as_str()) {
        return Err(ModelError::new(format!(
            "pattern '{}' references unknown source '{}'",
            pattern.id, pattern.source
        )));
    }
    if pattern.members.is_empty() {
        return Err(ModelError::new(format!(
            "pattern '{}' has no members",
            pattern.id
        )));
    }
    pattern.rule.validate().map_err(context)?;
    validate_pattern_driver_rule(
        &pattern.rule,
        pattern.count_driver.as_ref(),
        pattern.span_driver.as_ref(),
    )
    .map_err(context)?;
    if let Some(driver) = &pattern.count_driver {
        graph.validate_count_driver(driver).map_err(context)?;
    }
    if let Some(driver) = &pattern.span_driver {
        graph.validate_span_driver(driver).map_err(context)?;
    }
    validate_pattern_slots(pattern).map_err(context)?;
    graph
        .frame_chain(pattern.frame.as_deref())
        .map_err(context)?;
    let mut slots = HashSet::new();
    for member in &pattern.members {
        if !slots.insert(member.index) {
            return Err(ModelError::new(format!(
                "pattern '{}' uses rule slot {} more than once",
                pattern.id, member.index
            )));
        }
        if !patterned_members.insert(member.id.as_str()) {
            return Err(ModelError::new(format!(
                "instance '{}' belongs to more than one pattern",
                member.id
            )));
        }
        validate_pattern_member(pattern, member, graph, node_ids)?;
    }
    Ok(())
}

/// Slot count, prefix, and member slots must agree with each other and with
/// any count the rule's constraints require.
fn validate_pattern_slots(pattern: &Pattern) -> Result<(), ModelError> {
    if pattern.slot_count == 0 || pattern.member_prefix.is_empty() {
        return Err(ModelError::new(
            "pattern requires slots and a member prefix",
        ));
    }
    if let Some(member) = pattern
        .members
        .iter()
        .find(|member| member.index >= pattern.slot_count)
    {
        return Err(ModelError::new(format!(
            "member '{}' slot {} is outside the {} pattern slots",
            member.id, member.index, pattern.slot_count
        )));
    }
    match pattern.rule.fitted_count()? {
        Some(required) if required != pattern.slot_count => Err(ModelError::new(format!(
            "constraints require {required} slots but the pattern has {}",
            pattern.slot_count
        ))),
        _ => Ok(()),
    }
}

/// A member must be a clone of the pattern source, in the pattern frame, at
/// its override or rule placement.
fn validate_pattern_member(
    pattern: &Pattern,
    member: &PatternMember,
    graph: &InstanceGraph<'_>,
    node_ids: &HashSet<&str>,
) -> Result<(), ModelError> {
    let id = member.id.as_str();
    if !node_ids.contains(id) {
        return Err(ModelError::new(format!(
            "pattern '{}' references unknown member '{id}'",
            pattern.id
        )));
    }
    if let Some(placement) = member.placement_override {
        placement.normalized()?;
    }
    let linked = matches!(
        graph.node(id),
        Some(node @ InstanceNode::Clone { source, .. })
            if source == &pattern.source && node.frame() == pattern.frame.as_deref()
    );
    if !linked {
        return Err(ModelError::new(format!(
            "pattern '{}' member '{id}' is not linked to source '{}' in the pattern frame",
            pattern.id, pattern.source
        )));
    }
    let stored = graph
        .node(id)
        .expect("pattern member existence was checked")
        .placement();
    if !placements_equivalent(stored, pattern.member_placement(member))? {
        return Err(ModelError::new(format!(
            "pattern '{}' member '{id}' placement does not match its rule slot or override",
            pattern.id
        )));
    }
    Ok(())
}

fn validate_generation_record(
    record: &GenerationRecord,
    node_ids: &HashSet<&str>,
) -> Result<(), ModelError> {
    if !node_ids.contains(record.instance_id.as_str()) {
        return Err(ModelError::new(format!(
            "generation record references unknown instance '{}'",
            record.instance_id
        )));
    }
    if record
        .accepted_revision
        .is_some_and(|revision| revision > record.attempted_revision)
    {
        return Err(ModelError::new(format!(
            "generation record for '{}' has an invalid accepted revision",
            record.instance_id
        )));
    }
    let has_accepted = record.accepted_revision.is_some();
    let state_is_accepted = matches!(
        record.state,
        RegenerationState::Current | RegenerationState::Frozen | RegenerationState::Stale
    );
    if has_accepted != state_is_accepted {
        return Err(ModelError::new(format!(
            "generation record for '{}' has an inconsistent state",
            record.instance_id
        )));
    }
    Ok(())
}

/// `pew` from a first member named `pew[0]`, otherwise the pattern id.
fn derived_member_prefix(pattern: &Pattern) -> String {
    pattern
        .members
        .first()
        .and_then(|member| {
            let (prefix, slot) = member.id.strip_suffix(']')?.rsplit_once('[')?;
            let numeric = !slot.is_empty() && slot.bytes().all(|byte| byte.is_ascii_digit());
            (numeric && !prefix.is_empty()).then(|| prefix.to_owned())
        })
        .unwrap_or_else(|| pattern.id.clone())
}

/// Schema 17 replaced member id strings with slot records numbered by position.
fn migrate_pattern_member_slots(document: &mut serde_json::Value) -> Result<(), ModelError> {
    let Some(patterns) = document
        .get_mut("patterns")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(());
    };
    for pattern in patterns {
        let Some(members) = pattern
            .get_mut("members")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return Err(ModelError::new("model document pattern requires members"));
        };
        for (index, member) in members.iter_mut().enumerate() {
            if let Some(id) = member.as_str() {
                *member = serde_json::json!({ "id": id, "index": index });
            }
        }
    }
    Ok(())
}

/// Schema 14 replaced the flat linear `step` with a tagged pattern `rule`.
fn migrate_linear_pattern_steps(document: &mut serde_json::Value) -> Result<(), ModelError> {
    let Some(patterns) = document
        .get_mut("patterns")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(());
    };
    for pattern in patterns {
        let object = pattern
            .as_object_mut()
            .ok_or_else(|| ModelError::new("model document pattern must be an object"))?;
        let step = object
            .remove("step")
            .ok_or_else(|| ModelError::new("legacy linear pattern requires a step"))?;
        object.insert(
            "rule".into(),
            serde_json::json!({ "linear": { "step": step } }),
        );
    }
    Ok(())
}

pub struct ResolvedInstance<'definition> {
    pub instance: PartInstance<'definition>,
    /// Placement within the instance's assembly frame.
    pub placement: Placement,
    /// Enclosing frame placements, innermost first, applied after `placement`.
    pub frames: Vec<Placement>,
}

impl ResolvedInstance<'_> {
    pub fn regenerate<'session>(
        &self,
        session: &'session Session,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        let result = self.instance.regenerate(session)?;
        self.place(session, result)
    }

    /// Moves a locally generated result through the instance placement and
    /// every enclosing assembly frame. The input is consumed on success and failure.
    fn place<'session>(
        &self,
        session: &'session Session,
        mut result: GeneratedResult<'session>,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        result = apply_placement(session, result, self.placement)?;
        for frame in &self.frames {
            result = apply_placement(session, result, *frame)?;
        }
        Ok(result)
    }
}

/// Placed results for several graph instances. Instances whose resolved
/// parameters are identical share one local generation and receive
/// independent placed handles.
pub struct GraphRegeneration<'session> {
    results: HashMap<String, GeneratedResult<'session>>,
    shared_from: HashMap<String, String>,
    generated_variants: usize,
}

impl<'session> GraphRegeneration<'session> {
    pub fn result(&self, instance_id: &str) -> Option<&GeneratedResult<'session>> {
        self.results.get(instance_id)
    }

    /// The instance whose local generation produced this instance's geometry.
    /// A representative instance reports its own id.
    pub fn shared_from(&self, instance_id: &str) -> Option<&str> {
        self.shared_from.get(instance_id).map(String::as_str)
    }

    /// Number of feature-graph regenerations performed.
    pub fn generated_variants(&self) -> usize {
        self.generated_variants
    }

    pub fn into_results(self) -> HashMap<String, GeneratedResult<'session>> {
        self.results
    }
}

/// Member changes a checked pattern resize will make.
struct ResizePlan {
    removed: Vec<String>,
    added: Vec<(String, usize)>,
}

#[derive(Clone)]
pub struct InstanceGraph<'definition> {
    definition: &'definition FamilyDefinition,
    additional_definitions: HashMap<String, &'definition FamilyDefinition>,
    nodes: HashMap<String, InstanceNode>,
    patterns: Vec<Pattern>,
    frames: HashMap<String, AssemblyFrame>,
    assembly: AssemblySemantics,
}

impl<'definition> InstanceGraph<'definition> {
    pub fn new(definition: &'definition FamilyDefinition) -> Self {
        Self {
            definition,
            additional_definitions: HashMap::new(),
            nodes: HashMap::new(),
            patterns: Vec::new(),
            frames: HashMap::new(),
            assembly: AssemblySemantics::default(),
        }
    }

    pub fn add_base(
        &mut self,
        id: impl Into<String>,
        overrides: HashMap<String, ParameterValue>,
        provenance: impl Into<String>,
    ) -> Result<(), ModelError> {
        let id = id.into();
        self.insert(InstanceNode::Base {
            id,
            family: None,
            overrides,
            placement: Placement::identity(),
            frame: None,
            provenance: provenance.into(),
        })
    }

    /// Registers another family definition for use by base instances.
    pub fn add_family(
        &mut self,
        definition: &'definition FamilyDefinition,
    ) -> Result<(), ModelError> {
        validate_definition(definition)?;
        resolve_parameters(definition, &HashMap::new())?;
        if definition.id.is_empty()
            || definition.id == self.definition.id
            || self.additional_definitions.contains_key(&definition.id)
        {
            return Err(ModelError::new(
                "family ids in an instance graph must be nonempty and unique",
            ));
        }
        self.additional_definitions
            .insert(definition.id.clone(), definition);
        Ok(())
    }

    /// Adds a base instance belonging to a registered family.
    pub fn add_base_from_family(
        &mut self,
        id: impl Into<String>,
        family: &str,
        overrides: HashMap<String, ParameterValue>,
        provenance: impl Into<String>,
    ) -> Result<(), ModelError> {
        self.definition_by_id(family)?;
        self.insert(InstanceNode::Base {
            id: id.into(),
            family: (family != self.definition.id).then(|| family.to_owned()),
            overrides,
            placement: Placement::identity(),
            frame: None,
            provenance: provenance.into(),
        })
    }

    pub fn add_clone(
        &mut self,
        id: impl Into<String>,
        source: impl Into<String>,
        overrides: HashMap<String, ParameterValue>,
        provenance: impl Into<String>,
    ) -> Result<(), ModelError> {
        let id = id.into();
        self.insert(InstanceNode::Clone {
            id,
            source: source.into(),
            overrides,
            placement: Placement::identity(),
            frame: None,
            provenance: provenance.into(),
        })
    }

    pub fn node(&self, id: &str) -> Option<&InstanceNode> {
        self.nodes.get(id)
    }

    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }

    /// Sets an instance placement. On a pattern member this records a
    /// placement override that later rule edits leave in place.
    pub fn set_placement(
        &mut self,
        instance_id: &str,
        placement: Placement,
    ) -> Result<(), ModelError> {
        placement.normalized()?;
        if let Some(member) = self.pattern_member_mut(instance_id) {
            member.placement_override = Some(placement);
        }
        let node = self
            .nodes
            .get_mut(instance_id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{instance_id}'")))?;
        match node {
            InstanceNode::Base {
                placement: value, ..
            }
            | InstanceNode::Clone {
                placement: value, ..
            } => *value = placement,
        }
        Ok(())
    }

    pub fn add_linear_pattern(
        &mut self,
        id: impl Into<String>,
        member_prefix: &str,
        source: &str,
        count: usize,
        step: VectorQuantity,
        provenance: impl Into<String>,
    ) -> Result<Vec<String>, ModelError> {
        self.add_pattern(
            id,
            member_prefix,
            source,
            count,
            PatternRule::Linear { step },
            provenance,
        )
    }

    /// Adds a pattern with a freely chosen member count. Constraint-driven
    /// rules derive their count; use [`Self::add_fitted_pattern`] for them.
    pub fn add_pattern(
        &mut self,
        id: impl Into<String>,
        member_prefix: &str,
        source: &str,
        count: usize,
        rule: PatternRule,
        provenance: impl Into<String>,
    ) -> Result<Vec<String>, ModelError> {
        if rule.fitted_count()?.is_some() {
            return Err(ModelError::new(
                "constraint-driven rules derive their count; use add_fitted_pattern",
            ));
        }
        self.insert_pattern(
            id.into(),
            member_prefix,
            source,
            count,
            rule,
            provenance.into(),
        )
    }

    /// Adds a pattern whose member count and spacing are solved from a
    /// `LinearFit` or `CircularFit` rule.
    pub fn add_fitted_pattern(
        &mut self,
        id: impl Into<String>,
        member_prefix: &str,
        source: &str,
        rule: PatternRule,
        provenance: impl Into<String>,
    ) -> Result<Vec<String>, ModelError> {
        let count = rule
            .fitted_count()?
            .ok_or_else(|| ModelError::new("rule has no count constraint; use add_pattern"))?;
        self.insert_pattern(
            id.into(),
            member_prefix,
            source,
            count,
            rule,
            provenance.into(),
        )
    }

    fn insert_pattern(
        &mut self,
        id: String,
        member_prefix: &str,
        source: &str,
        count: usize,
        rule: PatternRule,
        provenance: String,
    ) -> Result<Vec<String>, ModelError> {
        if id.is_empty() || self.patterns.iter().any(|pattern| pattern.id == id) {
            return Err(ModelError::new("pattern id must be nonempty and unique"));
        }
        if count == 0 || member_prefix.is_empty() {
            return Err(ModelError::new("pattern requires members and a prefix"));
        }
        if !self.nodes.contains_key(source) {
            return Err(ModelError::new(format!("unknown clone source '{source}'")));
        }
        rule.validate()?;
        let members = (0..count)
            .map(|index| format!("{member_prefix}[{index}]"))
            .collect::<Vec<_>>();
        if members.iter().any(|member| self.nodes.contains_key(member)) {
            return Err(ModelError::new("pattern member id already exists"));
        }
        for (index, member) in members.iter().enumerate() {
            self.nodes.insert(
                member.clone(),
                InstanceNode::Clone {
                    id: member.clone(),
                    source: source.into(),
                    overrides: HashMap::new(),
                    placement: rule.member_placement(index, count),
                    frame: None,
                    provenance: provenance.clone(),
                },
            );
        }
        self.patterns.push(Pattern {
            id,
            source: source.into(),
            members: members
                .iter()
                .enumerate()
                .map(|(index, member)| PatternMember {
                    id: member.clone(),
                    index,
                    placement_override: None,
                    suppressed: false,
                })
                .collect(),
            rule,
            frame: None,
            slot_count: count,
            member_prefix: member_prefix.to_owned(),
            count_driver: None,
            span_driver: None,
        });
        Ok(members)
    }

    pub fn frame(&self, id: &str) -> Option<&AssemblyFrame> {
        self.frames.get(id)
    }

    pub fn add_frame(
        &mut self,
        id: impl Into<String>,
        parent: Option<&str>,
        placement: Placement,
        provenance: impl Into<String>,
    ) -> Result<(), ModelError> {
        let id = id.into();
        if id.is_empty() || self.frames.contains_key(&id) {
            return Err(ModelError::new("frame id must be nonempty and unique"));
        }
        placement.normalized()?;
        self.frame_chain(parent)?;
        self.frames.insert(
            id.clone(),
            AssemblyFrame {
                id,
                parent: parent.map(str::to_owned),
                placement,
                provenance: provenance.into(),
            },
        );
        Ok(())
    }

    pub fn set_frame_placement(
        &mut self,
        frame_id: &str,
        placement: Placement,
    ) -> Result<(), ModelError> {
        placement.normalized()?;
        self.frames
            .get_mut(frame_id)
            .ok_or_else(|| ModelError::new(format!("unknown assembly frame '{frame_id}'")))?
            .placement = placement;
        Ok(())
    }

    /// Places an instance inside an assembly frame, or at model level with `None`.
    /// Pattern members move with their pattern through [`Self::set_pattern_frame`].
    pub fn set_instance_frame(
        &mut self,
        instance_id: &str,
        frame: Option<&str>,
    ) -> Result<(), ModelError> {
        self.frame_chain(frame)?;
        if let Some(pattern) = self
            .patterns
            .iter()
            .find(|pattern| pattern.member(instance_id).is_some())
        {
            return Err(ModelError::new(format!(
                "instance '{instance_id}' belongs to pattern '{}'; set the pattern frame instead",
                pattern.id
            )));
        }
        let node = self
            .nodes
            .get_mut(instance_id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{instance_id}'")))?;
        *node.frame_mut() = frame.map(str::to_owned);
        Ok(())
    }

    pub fn set_pattern_frame(
        &mut self,
        pattern_id: &str,
        frame: Option<&str>,
    ) -> Result<(), ModelError> {
        self.frame_chain(frame)?;
        let pattern = self
            .patterns
            .iter_mut()
            .find(|pattern| pattern.id == pattern_id)
            .ok_or_else(|| ModelError::new(format!("unknown pattern '{pattern_id}'")))?;
        pattern.frame = frame.map(str::to_owned);
        for member in &pattern.members {
            if let Some(node) = self.nodes.get_mut(&member.id) {
                *node.frame_mut() = frame.map(str::to_owned);
            }
        }
        Ok(())
    }

    /// Replaces a pattern rule and re-places every member without a
    /// placement override. A constraint-driven rule also resizes the pattern
    /// to the count its constraints require.
    pub fn set_pattern_rule(
        &mut self,
        pattern_id: &str,
        rule: PatternRule,
    ) -> Result<(), ModelError> {
        rule.validate()?;
        let index = self.pattern_index(pattern_id)?;
        validate_pattern_driver_rule(
            &rule,
            self.patterns[index].count_driver.as_ref(),
            self.patterns[index].span_driver.as_ref(),
        )?;
        let count = rule
            .fitted_count()?
            .unwrap_or(self.patterns[index].slot_count);
        let plan = self.plan_resize(&self.patterns[index], count)?;
        self.patterns[index].rule = rule;
        self.apply_resize(index, count, plan);
        Ok(())
    }

    /// Grows or shrinks a freely counted pattern to `count` rule slots.
    /// Growing adds linked members named `prefix[slot]`; shrinking deletes
    /// the members in removed slots. Slots vacated by detaching stay empty.
    pub fn set_pattern_count(&mut self, pattern_id: &str, count: usize) -> Result<(), ModelError> {
        let index = self.pattern_index(pattern_id)?;
        if self.patterns[index].count_driver.is_some() {
            return Err(ModelError::new(format!(
                "pattern '{pattern_id}' count is driven"
            )));
        }
        if self.patterns[index].rule.fitted_count()?.is_some() {
            return Err(ModelError::new(format!(
                "pattern '{pattern_id}' count is driven by its constraints"
            )));
        }
        let plan = self.plan_resize(&self.patterns[index], count)?;
        self.apply_resize(index, count, plan);
        Ok(())
    }

    /// Binds a freely counted pattern's slot count to an integer parameter or
    /// a generated output's measured bounds extent.
    /// The driver is resolved on the next graph regeneration or explicit
    /// [`Self::refresh_driven_patterns`] call.
    pub fn set_pattern_count_driver(
        &mut self,
        pattern_id: &str,
        driver: Option<PatternCountDriver>,
    ) -> Result<(), ModelError> {
        let index = self.pattern_index(pattern_id)?;
        validate_pattern_driver_rule(
            &self.patterns[index].rule,
            driver.as_ref(),
            self.patterns[index].span_driver.as_ref(),
        )?;
        if let Some(driver) = &driver {
            self.validate_count_driver(driver)?;
        }
        self.patterns[index].count_driver = driver;
        Ok(())
    }

    /// Binds a `LinearFit` span to a length parameter or measured bounds.
    /// The resolved span retains the rule's spacing constraint.
    pub fn set_pattern_span_driver(
        &mut self,
        pattern_id: &str,
        driver: Option<PatternSpanDriver>,
    ) -> Result<(), ModelError> {
        let index = self.pattern_index(pattern_id)?;
        validate_pattern_driver_rule(
            &self.patterns[index].rule,
            self.patterns[index].count_driver.as_ref(),
            driver.as_ref(),
        )?;
        if let Some(driver) = &driver {
            self.validate_span_driver(driver)?;
        }
        self.patterns[index].span_driver = driver;
        Ok(())
    }

    /// Resolves every pattern driver and updates rule placements and stable
    /// member slots. All drivers are evaluated before the graph is mutated.
    pub fn refresh_driven_patterns(&mut self, session: &Session) -> Result<(), ModelError> {
        let mut updates = Vec::new();
        for (index, pattern) in self.patterns.iter().enumerate() {
            if pattern.count_driver.is_none() && pattern.span_driver.is_none() {
                continue;
            }
            let mut rule = pattern.rule;
            if let Some(driver) = &pattern.span_driver {
                let span = self.resolve_span_driver(session, driver)?;
                match &mut rule {
                    PatternRule::LinearFit {
                        span: rule_span, ..
                    } => *rule_span = span,
                    _ => unreachable!("span-driver compatibility was validated"),
                }
            }
            rule.validate()?;
            let count = if let Some(driver) = &pattern.count_driver {
                self.resolve_count_driver(session, driver)?
            } else {
                rule.fitted_count()?.unwrap_or(pattern.slot_count)
            };
            let plan = self.plan_resize(pattern, count)?;
            updates.push((index, rule, count, plan));
        }
        for (index, rule, count, plan) in updates {
            self.patterns[index].rule = rule;
            self.apply_resize(index, count, plan);
        }
        Ok(())
    }

    fn validate_count_driver(&self, driver: &PatternCountDriver) -> Result<(), ModelError> {
        match driver {
            PatternCountDriver::Parameter {
                instance,
                parameter,
            } => {
                let resolved = self.resolve(instance)?;
                let parameters = resolve_parameters(resolved.definition, &resolved.overrides)?;
                match parameters.get(parameter) {
                    Some(ParameterValue::Integer(_)) => Ok(()),
                    Some(_) => Err(ModelError::new(format!(
                        "pattern count parameter '{parameter}' on instance '{instance}' must be an integer"
                    ))),
                    None => Err(ModelError::new(format!(
                        "unknown pattern count parameter '{parameter}' on instance '{instance}'"
                    ))),
                }
            }
            PatternCountDriver::BoundsExtent {
                instance,
                output,
                maximum_spacing,
                ..
            } => {
                self.validate_measurement_target(instance, output)?;
                positive_spacing(*maximum_spacing).map(|_| ())
            }
        }
    }

    fn resolve_count_driver(
        &self,
        session: &Session,
        driver: &PatternCountDriver,
    ) -> Result<usize, ModelError> {
        self.validate_count_driver(driver)?;
        match driver {
            PatternCountDriver::Parameter {
                instance,
                parameter,
            } => {
                let resolved = self.resolve(instance)?;
                let parameters = resolve_parameters(resolved.definition, &resolved.overrides)?;
                let ParameterValue::Integer(count) = parameters[parameter] else {
                    unreachable!("count parameter type was validated")
                };
                let count = usize::try_from(count).map_err(|_| {
                    ModelError::new(format!(
                        "pattern count parameter '{parameter}' on instance '{instance}' must be positive"
                    ))
                })?;
                if !(1..=MAX_PATTERN_MEMBERS).contains(&count) {
                    return Err(ModelError::new(format!(
                        "pattern count parameter '{parameter}' on instance '{instance}' must be 1..={MAX_PATTERN_MEMBERS}"
                    )));
                }
                Ok(count)
            }
            PatternCountDriver::BoundsExtent {
                instance,
                output,
                axis,
                maximum_spacing,
            } => LinearSpacing::Maximum(*maximum_spacing)
                .count(self.measure_output_extent(session, instance, output, *axis)?),
        }
    }

    /// The output must be a feature of the measured instance's own family,
    /// which may differ from the graph's primary family.
    fn validate_measurement_target(&self, instance: &str, output: &str) -> Result<(), ModelError> {
        let resolved = self.resolve(instance)?;
        if !resolved
            .definition
            .features
            .iter()
            .any(|feature| feature.id == output)
        {
            return Err(ModelError::new(format!(
                "unknown pattern measurement output '{output}' on instance '{instance}'"
            )));
        }
        Ok(())
    }

    fn validate_span_driver(&self, driver: &PatternSpanDriver) -> Result<(), ModelError> {
        let (instance, direction) = match driver {
            PatternSpanDriver::Parameter {
                instance,
                parameter,
                direction,
            } => {
                let resolved = self.resolve(instance)?;
                let parameters = resolve_parameters(resolved.definition, &resolved.overrides)?;
                match parameters.get(parameter) {
                    Some(ParameterValue::Scalar(value)) if value.dimension == Dimension::Length => {
                        value.normalized()?;
                    }
                    Some(_) => {
                        return Err(ModelError::new(format!(
                            "pattern span parameter '{parameter}' on instance '{instance}' must be a length"
                        )));
                    }
                    None => {
                        return Err(ModelError::new(format!(
                            "unknown pattern span parameter '{parameter}' on instance '{instance}'"
                        )));
                    }
                }
                (instance, direction)
            }
            PatternSpanDriver::BoundsExtent {
                instance,
                output,
                direction,
                ..
            } => {
                self.validate_measurement_target(instance, output)?;
                (instance, direction)
            }
        };
        normalized_pattern_direction(*direction).map_err(|error| {
            ModelError::new(format!(
                "pattern span driver for instance '{instance}': {}",
                error.message
            ))
        })?;
        Ok(())
    }

    fn resolve_span_driver(
        &self,
        session: &Session,
        driver: &PatternSpanDriver,
    ) -> Result<VectorQuantity, ModelError> {
        self.validate_span_driver(driver)?;
        let (length, direction) = match driver {
            PatternSpanDriver::Parameter {
                instance,
                parameter,
                direction,
            } => {
                let resolved = self.resolve(instance)?;
                let parameters = resolve_parameters(resolved.definition, &resolved.overrides)?;
                let ParameterValue::Scalar(value) = &parameters[parameter] else {
                    unreachable!("span parameter type was validated")
                };
                (value.normalized()?, *direction)
            }
            PatternSpanDriver::BoundsExtent {
                instance,
                output,
                axis,
                direction,
            } => (
                self.measure_output_extent(session, instance, output, *axis)?,
                *direction,
            ),
        };
        if !length.is_finite() || length <= 0.0 {
            return Err(ModelError::new(
                "driven pattern span must be positive and finite",
            ));
        }
        let direction = normalized_pattern_direction(direction)?;
        Ok(VectorQuantity::lengths(
            direction.x * length,
            direction.y * length,
            direction.z * length,
            LengthUnit::Millimeter,
        ))
    }

    fn measure_output_extent(
        &self,
        session: &Session,
        instance: &str,
        output: &str,
        axis: CoordinateAxis,
    ) -> Result<f64, ModelError> {
        self.validate_measurement_target(instance, output)?;
        let generated = self
            .resolve_with_placement(instance)?
            .regenerate(session)
            .map_err(|error| {
                ModelError::new(format!(
                    "measure pattern geometry from instance '{instance}': {}",
                    error.message
                ))
            })?;
        let measured: Result<f64, ModelError> = (|| {
            let shape = generated.shape(output).ok_or_else(|| {
                ModelError::new(format!(
                    "instance '{instance}' did not generate pattern measurement output '{output}'"
                ))
            })?;
            // Tolerance-padded bounds overstate lengths and would tip exact
            // multiples of a spacing into an extra member.
            let bounds = session.exact_bounds(shape).map_err(ModelError::from)?;
            Ok(match axis {
                CoordinateAxis::X => bounds.max.x - bounds.min.x,
                CoordinateAxis::Y => bounds.max.y - bounds.min.y,
                CoordinateAxis::Z => bounds.max.z - bounds.min.z,
            })
        })();
        cleanup(session, generated.shapes);
        measured
    }

    fn pattern_index(&self, pattern_id: &str) -> Result<usize, ModelError> {
        self.patterns
            .iter()
            .position(|pattern| pattern.id == pattern_id)
            .ok_or_else(|| ModelError::new(format!("unknown pattern '{pattern_id}'")))
    }

    /// Checks a resize without changing the graph.
    fn plan_resize(&self, pattern: &Pattern, count: usize) -> Result<ResizePlan, ModelError> {
        if !(1..=MAX_PATTERN_MEMBERS).contains(&count) {
            return Err(ModelError::new(format!(
                "pattern count must be 1..={MAX_PATTERN_MEMBERS}"
            )));
        }
        let removed = pattern
            .members
            .iter()
            .filter(|member| member.index >= count)
            .map(|member| member.id.clone())
            .collect::<Vec<_>>();
        if let Some((dependent, source)) = self.nodes.values().find_map(|node| match node {
            InstanceNode::Clone { id, source, .. }
                if removed.contains(source) && !removed.contains(id) =>
            {
                Some((id, source))
            }
            _ => None,
        }) {
            return Err(ModelError::new(format!(
                "cannot remove pattern member '{source}': '{dependent}' is cloned from it"
            )));
        }
        if let Some((member, reference)) = removed
            .iter()
            .find_map(|member| Some((member, self.assembly.reference_to(member)?)))
        {
            return Err(ModelError::new(format!(
                "cannot remove pattern member '{member}': it is named by {reference}"
            )));
        }
        let added = (pattern.slot_count..count)
            .map(|slot| (format!("{}[{slot}]", pattern.member_prefix), slot))
            .collect::<Vec<_>>();
        if let Some((existing, _)) = added.iter().find(|(id, _)| self.nodes.contains_key(id)) {
            return Err(ModelError::new(format!(
                "cannot grow pattern '{}': instance '{existing}' already exists",
                pattern.id
            )));
        }
        let remaining = pattern.members.len() - removed.len();
        if remaining == 0 && added.is_empty() {
            return Err(ModelError::new(format!(
                "pattern '{}' would have no members",
                pattern.id
            )));
        }
        Ok(ResizePlan { removed, added })
    }

    fn apply_resize(&mut self, index: usize, count: usize, plan: ResizePlan) {
        for id in &plan.removed {
            self.nodes.remove(id);
        }
        let pattern = &mut self.patterns[index];
        pattern.members.retain(|member| member.index < count);
        pattern.slot_count = count;
        for (id, slot) in plan.added {
            self.nodes.insert(
                id.clone(),
                InstanceNode::Clone {
                    id: id.clone(),
                    source: pattern.source.clone(),
                    overrides: HashMap::new(),
                    placement: Placement::identity(),
                    frame: pattern.frame.clone(),
                    provenance: format!("grown by pattern '{}'", pattern.id),
                },
            );
            pattern.members.push(PatternMember {
                id,
                index: slot,
                placement_override: None,
                suppressed: false,
            });
        }
        let pattern_id = pattern.id.clone();
        self.sync_pattern_placements(&pattern_id);
    }

    /// Returns a pattern member to its rule placement. Returns the override
    /// that was removed, if any.
    pub fn clear_placement_override(
        &mut self,
        instance_id: &str,
    ) -> Result<Option<Placement>, ModelError> {
        let member = self.pattern_member_mut(instance_id).ok_or_else(|| {
            ModelError::new(format!("instance '{instance_id}' is not a pattern member"))
        })?;
        let removed = member.placement_override.take();
        let pattern_id = self
            .pattern_of(instance_id)
            .map(|pattern| pattern.id.clone())
            .expect("member belongs to a pattern");
        self.sync_pattern_placements(&pattern_id);
        Ok(removed)
    }

    /// Suppresses or restores one pattern member. A suppressed member keeps
    /// its identity, links, and overrides but is skipped by graph regeneration.
    pub fn set_member_suppressed(
        &mut self,
        instance_id: &str,
        suppressed: bool,
    ) -> Result<(), ModelError> {
        self.pattern_member_mut(instance_id)
            .ok_or_else(|| {
                ModelError::new(format!("instance '{instance_id}' is not a pattern member"))
            })?
            .suppressed = suppressed;
        Ok(())
    }

    /// True for suppressed pattern members and for instances the active
    /// configuration suppresses.
    pub fn is_suppressed(&self, instance_id: &str) -> bool {
        self.assembly.configuration_suppresses(instance_id)
            || self
                .pattern_of(instance_id)
                .and_then(|pattern| pattern.member(instance_id))
                .is_some_and(|member| member.suppressed)
    }

    fn pattern_of(&self, instance_id: &str) -> Option<&Pattern> {
        self.patterns
            .iter()
            .find(|pattern| pattern.member(instance_id).is_some())
    }

    fn pattern_member_mut(&mut self, instance_id: &str) -> Option<&mut PatternMember> {
        self.patterns
            .iter_mut()
            .flat_map(|pattern| pattern.members.iter_mut())
            .find(|member| member.id == instance_id)
    }

    /// Writes each member's override or rule placement onto its node.
    fn sync_pattern_placements(&mut self, pattern_id: &str) {
        let Some(pattern) = self
            .patterns
            .iter()
            .find(|pattern| pattern.id == pattern_id)
        else {
            return;
        };
        for member in &pattern.members {
            let placement = pattern.member_placement(member);
            if let Some(node) = self.nodes.get_mut(&member.id) {
                match node {
                    InstanceNode::Base {
                        placement: value, ..
                    }
                    | InstanceNode::Clone {
                        placement: value, ..
                    } => *value = placement,
                }
            }
        }
    }

    /// Returns frame placements from `frame` outward to the model root.
    fn frame_chain(&self, frame: Option<&str>) -> Result<Vec<Placement>, ModelError> {
        let mut visiting: Vec<&str> = Vec::new();
        let mut placements = Vec::new();
        let mut current = frame;
        while let Some(id) = current {
            if let Some(position) = visiting.iter().position(|visited| *visited == id) {
                let mut cycle = visiting[position..].to_vec();
                cycle.push(id);
                return Err(ModelError::new(format!(
                    "assembly frame cycle: {}",
                    cycle.join(" -> ")
                )));
            }
            let frame = self
                .frames
                .get(id)
                .ok_or_else(|| ModelError::new(format!("unknown assembly frame '{id}'")))?;
            frame.placement.normalized()?;
            visiting.push(id);
            placements.push(frame.placement);
            current = frame.parent.as_deref();
        }
        Ok(placements)
    }

    pub fn set_override(
        &mut self,
        instance_id: &str,
        parameter: impl Into<String>,
        value: ParameterValue,
    ) -> Result<(), ModelError> {
        let node = self
            .nodes
            .get_mut(instance_id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{instance_id}'")))?;
        node.overrides_mut().insert(parameter.into(), value);
        Ok(())
    }

    pub fn remove_override(
        &mut self,
        instance_id: &str,
        parameter: &str,
    ) -> Result<Option<ParameterValue>, ModelError> {
        let node = self
            .nodes
            .get_mut(instance_id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{instance_id}'")))?;
        Ok(node.overrides_mut().remove(parameter))
    }

    fn definition_by_id(&self, family: &str) -> Result<&'definition FamilyDefinition, ModelError> {
        if family == self.definition.id {
            return Ok(self.definition);
        }
        self.additional_definitions
            .get(family)
            .copied()
            .ok_or_else(|| ModelError::new(format!("unknown family definition '{family}'")))
    }

    fn resolve_definition(
        &self,
        id: &str,
        visiting: &mut Vec<String>,
    ) -> Result<&'definition FamilyDefinition, ModelError> {
        if let Some(position) = visiting.iter().position(|visited| visited == id) {
            let mut cycle = visiting[position..].to_vec();
            cycle.push(id.into());
            return Err(ModelError::new(format!(
                "clone inheritance cycle: {}",
                cycle.join(" -> ")
            )));
        }
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| ModelError::new(format!("unknown clone source '{id}'")))?;
        match node {
            InstanceNode::Base { family, .. } => family
                .as_deref()
                .map_or(Ok(self.definition), |family| self.definition_by_id(family)),
            InstanceNode::Clone { source, .. } => {
                visiting.push(id.into());
                let definition = self.resolve_definition(source, visiting);
                visiting.pop();
                definition
            }
        }
    }

    pub fn resolve(&self, id: &str) -> Result<PartInstance<'definition>, ModelError> {
        let mut visiting = Vec::new();
        let overrides = self.resolve_overrides(id, &mut visiting)?;
        let definition = self.resolve_definition(id, &mut Vec::new())?;
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{id}'")))?;
        let provenance = match node {
            InstanceNode::Base { provenance, .. } | InstanceNode::Clone { provenance, .. } => {
                provenance.clone()
            }
        };
        Ok(PartInstance {
            id: id.into(),
            definition,
            overrides,
            provenance,
        })
    }

    pub fn resolve_with_placement(
        &self,
        id: &str,
    ) -> Result<ResolvedInstance<'definition>, ModelError> {
        let instance = self.resolve(id)?;
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{id}'")))?;
        Ok(ResolvedInstance {
            instance,
            placement: node.placement(),
            frames: self.frame_chain(node.frame())?,
        })
    }

    /// Regenerates every instance in the graph except suppressed pattern
    /// members; see [`Self::regenerate_instances`].
    pub fn regenerate_all<'session>(
        &mut self,
        session: &'session Session,
    ) -> Result<GraphRegeneration<'session>, ModelError> {
        self.refresh_driven_patterns(session)?;
        let mut ids = self
            .nodes
            .keys()
            .map(String::as_str)
            .filter(|id| !self.is_suppressed(id))
            .collect::<Vec<_>>();
        ids.sort_unstable();
        self.regenerate_instances_current(session, &ids)
    }

    /// Regenerates the requested instances, running the feature graph once per
    /// distinct set of resolved parameters. Clones that differ only in
    /// placement or assembly frame duplicate the shared local result and are
    /// then placed independently. On failure every handle created by the call
    /// is released.
    pub fn regenerate_instances<'session>(
        &mut self,
        session: &'session Session,
        ids: &[&str],
    ) -> Result<GraphRegeneration<'session>, ModelError> {
        self.refresh_driven_patterns(session)?;
        self.regenerate_instances_current(session, ids)
    }

    fn regenerate_instances_current<'session>(
        &self,
        session: &'session Session,
        ids: &[&str],
    ) -> Result<GraphRegeneration<'session>, ModelError> {
        let groups = self.group_by_parameters(ids)?;
        let mut output = GraphRegeneration {
            results: HashMap::new(),
            shared_from: HashMap::new(),
            generated_variants: groups.len(),
        };
        for members in groups {
            let representative = members[0].0.to_owned();
            match self.regenerate_group(session, &members) {
                Ok(results) => {
                    for (id, result) in results {
                        output
                            .shared_from
                            .insert(id.clone(), representative.clone());
                        output.results.insert(id, result);
                    }
                }
                Err(error) => {
                    release_results(session, output.results.into_values());
                    return Err(error);
                }
            }
        }
        Ok(output)
    }

    /// Groups requested instances by their complete resolved parameter set,
    /// preserving request order within and across groups.
    fn group_by_parameters<'ids>(
        &self,
        ids: &[&'ids str],
    ) -> Result<Vec<Vec<(&'ids str, ResolvedInstance<'definition>)>>, ModelError> {
        let mut seen = HashSet::new();
        let mut keys: Vec<String> = Vec::new();
        let mut groups: Vec<Vec<(&str, ResolvedInstance<'definition>)>> = Vec::new();
        for &id in ids {
            if !seen.insert(id) {
                return Err(ModelError::new(format!(
                    "instance '{id}' requested more than once"
                )));
            }
            if self.is_suppressed(id) {
                return Err(ModelError::new(format!(
                    "instance '{id}' is suppressed in its pattern"
                )));
            }
            let resolved = self
                .resolve_with_placement(id)
                .map_err(|error| instance_error(id, error))?;
            let parameters =
                resolve_parameters(resolved.instance.definition, &resolved.instance.overrides)
                    .map_err(|error| instance_error(id, error))?;
            let key = serde_json::to_string(&(
                &resolved.instance.definition.id,
                resolved.instance.definition.version,
                parameters.into_iter().collect::<BTreeMap<_, _>>(),
            ))
            .map_err(|error| ModelError::new(format!("create parameter key: {error}")))?;
            match keys.iter().position(|existing| *existing == key) {
                Some(index) => groups[index].push((id, resolved)),
                None => {
                    keys.push(key);
                    groups.push(vec![(id, resolved)]);
                }
            }
        }
        Ok(groups)
    }

    /// Generates one group's local result once and places a copy for each
    /// member. On failure every handle created for the group is released.
    fn regenerate_group<'session>(
        &self,
        session: &'session Session,
        members: &[(&str, ResolvedInstance<'definition>)],
    ) -> Result<Vec<(String, GeneratedResult<'session>)>, ModelError> {
        let (representative, first) = &members[0];
        let local = first
            .instance
            .regenerate(session)
            .map_err(|error| instance_error(representative, error))?;
        let mut unplaced = Vec::with_capacity(members.len());
        for (id, _) in &members[1..] {
            match duplicate_result(session, &local) {
                Ok(copy) => unplaced.push(copy),
                Err(error) => {
                    release_results(session, unplaced.into_iter().chain([local]));
                    return Err(instance_error(id, error));
                }
            }
        }
        unplaced.insert(0, local);

        let mut pending = unplaced.into_iter();
        let mut placed = Vec::with_capacity(members.len());
        for (index, (id, resolved)) in members.iter().enumerate() {
            let shared = pending.next().expect("one local result per member");
            match resolved.place(session, shared) {
                Ok(mut result) => {
                    if index > 0 {
                        result.regeneration = Self::all_features_reused(first.instance.definition);
                    }
                    placed.push(((*id).to_owned(), result));
                }
                Err(error) => {
                    let placed = placed.into_iter().map(|(_, result)| result);
                    release_results(session, placed.chain(pending));
                    return Err(instance_error(id, error));
                }
            }
        }
        Ok(placed)
    }

    fn all_features_reused(definition: &FamilyDefinition) -> RegenerationReport {
        RegenerationReport {
            rebuilt: Vec::new(),
            reused: definition
                .features
                .iter()
                .map(|feature| feature.id.clone())
                .collect(),
        }
    }

    /// Replaces a linked clone with an independent base carrying its resolved
    /// parameters, placement, and frame. It also leaves any pattern.
    pub fn detach(&mut self, id: &str) -> Result<(), ModelError> {
        let resolved = self.resolve(id)?;
        let inherited_material = self.material_of(id)?.map(|material| material.id.clone());
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| ModelError::new(format!("unknown instance '{id}'")))?;
        let placement = node.placement();
        let frame = node.frame().map(str::to_owned);
        self.nodes.insert(
            id.into(),
            InstanceNode::Base {
                id: id.into(),
                family: (resolved.definition.id != self.definition.id)
                    .then(|| resolved.definition.id.clone()),
                overrides: resolved.overrides,
                placement,
                frame,
                provenance: format!("detached from linked source; {}", resolved.provenance),
            },
        );
        self.pin_material(id, inherited_material);
        // A detached instance is no longer a linked pattern member; a pattern
        // left without members is removed.
        for pattern in &mut self.patterns {
            pattern.members.retain(|member| member.id != id);
        }
        self.patterns.retain(|pattern| !pattern.members.is_empty());
        Ok(())
    }

    fn insert(&mut self, node: InstanceNode) -> Result<(), ModelError> {
        if node.id().is_empty() {
            return Err(ModelError::new("instance id must not be empty"));
        }
        if self.nodes.contains_key(node.id()) {
            return Err(ModelError::new(format!(
                "duplicate instance id '{}'",
                node.id()
            )));
        }
        self.nodes.insert(node.id().into(), node);
        Ok(())
    }

    fn resolve_overrides(
        &self,
        id: &str,
        visiting: &mut Vec<String>,
    ) -> Result<HashMap<String, ParameterValue>, ModelError> {
        if let Some(position) = visiting.iter().position(|visited| visited == id) {
            let mut cycle = visiting[position..].to_vec();
            cycle.push(id.into());
            return Err(ModelError::new(format!(
                "clone inheritance cycle: {}",
                cycle.join(" -> ")
            )));
        }
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| ModelError::new(format!("unknown clone source '{id}'")))?;
        visiting.push(id.into());
        let mut resolved = match node {
            InstanceNode::Base { overrides, .. } => overrides.clone(),
            InstanceNode::Clone {
                source, overrides, ..
            } => {
                let mut inherited = self.resolve_overrides(source, visiting)?;
                inherited.extend(overrides.clone());
                inherited
            }
        };
        visiting.pop();
        if let Some(configured) = self.assembly.configured_overrides(id) {
            resolved.extend(configured.clone());
        }
        resolved.shrink_to_fit();
        Ok(resolved)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationStatus {
    Passed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationResult {
    pub requirement_id: String,
    pub status: VerificationStatus,
    pub message: String,
}

pub struct GeneratedResult<'session> {
    shapes: HashMap<String, Shape<'session>>,
    feature_signatures: HashMap<String, Vec<u8>>,
    pub verification: Vec<VerificationResult>,
    pub regeneration: RegenerationReport,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegenerationReport {
    pub rebuilt: Vec<String>,
    pub reused: Vec<String>,
}

impl<'session> GeneratedResult<'session> {
    pub fn shape(&self, name: &str) -> Option<&Shape<'session>> {
        self.shapes.get(name)
    }

    pub fn named_outputs(&self) -> impl Iterator<Item = &str> {
        self.shapes.keys().map(String::as_str)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegenerationState {
    NeverGenerated,
    Current,
    Frozen,
    Stale,
    Failed,
}

pub struct ManagedPartInstance<'definition, 'session> {
    session: &'session Session,
    instance: PartInstance<'definition>,
    attempted_revision: u64,
    accepted_revision: Option<u64>,
    state: RegenerationState,
    last_error: Option<ModelError>,
    accepted: Option<GeneratedResult<'session>>,
}

impl<'definition, 'session> ManagedPartInstance<'definition, 'session> {
    pub fn new(session: &'session Session, instance: PartInstance<'definition>) -> Self {
        Self {
            session,
            instance,
            attempted_revision: 0,
            accepted_revision: None,
            state: RegenerationState::NeverGenerated,
            last_error: None,
            accepted: None,
        }
    }

    pub fn instance(&self) -> &PartInstance<'definition> {
        &self.instance
    }

    pub fn instance_mut(&mut self) -> &mut PartInstance<'definition> {
        &mut self.instance
    }

    pub fn attempted_revision(&self) -> u64 {
        self.attempted_revision
    }

    pub fn accepted_revision(&self) -> Option<u64> {
        self.accepted_revision
    }

    pub fn state(&self) -> RegenerationState {
        self.state
    }

    pub fn last_error(&self) -> Option<&ModelError> {
        self.last_error.as_ref()
    }

    pub fn accepted(&self) -> Option<&GeneratedResult<'session>> {
        self.accepted.as_ref()
    }

    pub fn generation_record(&self) -> GenerationRecord {
        GenerationRecord {
            instance_id: self.instance.id.clone(),
            attempted_revision: self.attempted_revision,
            accepted_revision: self.accepted_revision,
            state: self.state,
            last_error: self.last_error.as_ref().map(ToString::to_string),
        }
    }

    pub fn freeze(&mut self) -> Result<u64, ModelError> {
        let revision = self
            .accepted_revision
            .ok_or_else(|| ModelError::new("cannot freeze before a generation is accepted"))?;
        self.state = RegenerationState::Frozen;
        Ok(revision)
    }

    pub fn unfreeze(&mut self) {
        if self.state == RegenerationState::Frozen {
            self.state = RegenerationState::Current;
        }
    }

    pub fn regenerate(&mut self) -> Result<(), ModelError> {
        if self.state == RegenerationState::Frozen {
            return Err(ModelError::new("accepted generation is frozen"));
        }
        self.attempted_revision = self.attempted_revision.saturating_add(1);
        let regeneration = match self.accepted.as_ref() {
            Some(previous) => self.instance.regenerate_incremental(self.session, previous),
            None => self.instance.regenerate(self.session),
        };
        match regeneration {
            Ok(result) => {
                if let Some(previous) = self.accepted.take() {
                    cleanup(self.session, previous.shapes);
                }
                self.accepted = Some(result);
                self.accepted_revision = Some(self.attempted_revision);
                self.state = RegenerationState::Current;
                self.last_error = None;
                Ok(())
            }
            Err(error) => {
                self.state = if self.accepted.is_some() {
                    RegenerationState::Stale
                } else {
                    RegenerationState::Failed
                };
                self.last_error = Some(error.clone());
                Err(error)
            }
        }
    }
}

impl Drop for ManagedPartInstance<'_, '_> {
    fn drop(&mut self) {
        if let Some(result) = self.accepted.take() {
            cleanup(self.session, result.shapes);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError {
    pub message: String,
}

impl ModelError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ModelError {}

impl From<BridgeError> for ModelError {
    fn from(error: BridgeError) -> Self {
        Self::new(error.to_string())
    }
}

impl PartInstance<'_> {
    pub fn regenerate<'session>(
        &self,
        session: &'session Session,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        self.regenerate_internal(session, None)
    }

    pub fn regenerate_incremental<'session>(
        &self,
        session: &'session Session,
        previous: &GeneratedResult<'session>,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        self.regenerate_internal(session, Some(previous))
    }

    fn regenerate_internal<'session>(
        &self,
        session: &'session Session,
        previous: Option<&GeneratedResult<'session>>,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        validate_definition(self.definition)?;
        let parameters = resolve_parameters(self.definition, &self.overrides)?;
        let mut build = FeatureBuild::default();
        if let Err(error) = build.run(session, self.definition, &parameters, previous) {
            cleanup(session, build.shapes);
            return Err(error);
        }
        match verify_requirements(session, self.definition, &build.shapes) {
            Ok(verification) => Ok(GeneratedResult {
                shapes: build.shapes,
                feature_signatures: build.feature_signatures,
                verification,
                regeneration: build.regeneration,
            }),
            Err(error) => {
                cleanup(session, build.shapes);
                Err(error)
            }
        }
    }
}

/// Feature outputs accumulated during one regeneration. The caller releases
/// `shapes` if any step fails.
#[derive(Default)]
struct FeatureBuild<'session> {
    shapes: HashMap<String, Shape<'session>>,
    feature_signatures: HashMap<String, Vec<u8>>,
    dirty_features: HashSet<String>,
    regeneration: RegenerationReport,
}

impl<'session> FeatureBuild<'session> {
    /// Executes features in dependency order. Each pass runs every ready
    /// feature in declaration order; a pass without progress is a cycle or a
    /// missing input.
    fn run(
        &mut self,
        session: &'session Session,
        definition: &FamilyDefinition,
        parameters: &HashMap<String, ParameterValue>,
        previous: Option<&GeneratedResult<'session>>,
    ) -> Result<(), ModelError> {
        let mut pending: Vec<&FeatureDefinition> = definition.features.iter().collect();
        while !pending.is_empty() {
            let before = pending.len();
            let mut index = 0;
            while index < pending.len() {
                if self.is_ready(pending[index]) {
                    let feature = pending.remove(index);
                    self.add_feature(session, feature, parameters, previous)?;
                } else {
                    index += 1;
                }
            }
            if pending.len() == before {
                let blocked = pending
                    .iter()
                    .map(|feature| feature.id.as_str())
                    .collect::<Vec<_>>();
                return Err(ModelError::new(format!(
                    "feature dependency cycle or unresolved input: {}",
                    blocked.join(", ")
                )));
            }
        }
        Ok(())
    }

    fn is_ready(&self, feature: &FeatureDefinition) -> bool {
        feature
            .operation
            .dependencies()
            .iter()
            .all(|dependency| self.shapes.contains_key(*dependency))
    }

    /// Reuses the previous output through a duplicate handle when the feature
    /// signature is unchanged and no dependency was rebuilt; otherwise executes it.
    fn add_feature(
        &mut self,
        session: &'session Session,
        feature: &FeatureDefinition,
        parameters: &HashMap<String, ParameterValue>,
        previous: Option<&GeneratedResult<'session>>,
    ) -> Result<(), ModelError> {
        let signature = feature_signature(feature, parameters)?;
        let dependency_is_dirty = feature
            .operation
            .dependencies()
            .iter()
            .any(|dependency| self.dirty_features.contains(*dependency));
        let reusable = previous
            .filter(|previous| {
                !dependency_is_dirty
                    && previous.feature_signatures.get(&feature.id) == Some(&signature)
            })
            .and_then(|previous| previous.shapes.get(&feature.id));
        let generated = match reusable {
            Some(shape) => session.duplicate(shape).map_err(Into::into),
            None => {
                self.dirty_features.insert(feature.id.clone());
                execute_feature(session, feature, parameters, &self.shapes)
            }
        };
        let shape = generated
            .map_err(|error| ModelError::new(format!("feature '{}': {error}", feature.id)))?;
        self.shapes.insert(feature.id.clone(), shape);
        self.feature_signatures
            .insert(feature.id.clone(), signature);
        let report = if reusable.is_some() {
            &mut self.regeneration.reused
        } else {
            &mut self.regeneration.rebuilt
        };
        report.push(feature.id.clone());
        Ok(())
    }
}

/// Evaluates every requirement; required failures reject the generation.
fn verify_requirements(
    session: &Session,
    definition: &FamilyDefinition,
    shapes: &HashMap<String, Shape<'_>>,
) -> Result<Vec<VerificationResult>, ModelError> {
    let mut verification = Vec::new();
    let mut required_failures = Vec::new();
    for requirement in &definition.requirements {
        let result = verify_requirement(session, requirement, shapes).map_err(|error| {
            ModelError::new(format!("requirement '{}': {error}", requirement.id))
        })?;
        if requirement.priority == RequirementPriority::Required
            && result.status == VerificationStatus::Failed
        {
            required_failures.push(requirement.id.clone());
        }
        verification.push(result);
    }
    if !required_failures.is_empty() {
        return Err(ModelError::new(format!(
            "required verification failed: {}",
            required_failures.join(", ")
        )));
    }
    Ok(verification)
}

#[derive(Serialize)]
struct FeatureSignature<'a> {
    feature: &'a FeatureDefinition,
    parameters: Vec<(&'a str, &'a ParameterValue)>,
}

fn feature_signature(
    feature: &FeatureDefinition,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<Vec<u8>, ModelError> {
    let mut names = HashSet::new();
    collect_operation_parameters(&feature.operation, &mut names);
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort_unstable();
    let values = names
        .into_iter()
        .map(|name| {
            parameters
                .get(name)
                .map(|value| (name, value))
                .ok_or_else(|| ModelError::new(format!("unknown parameter '{name}'")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_vec(&FeatureSignature {
        feature,
        parameters: values,
    })
    .map_err(|error| ModelError::new(format!("create feature signature: {error}")))
}

fn collect_operation_parameters<'a>(operation: &'a FeatureOperation, names: &mut HashSet<&'a str>) {
    match operation {
        FeatureOperation::Box { origin, size } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(size, names);
        }
        FeatureOperation::Cylinder {
            origin,
            axis,
            radius,
            height,
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(radius, names);
            collect_scalar_parameters(height, names);
        }
        FeatureOperation::Translate { offset, .. } => collect_vector_parameters(offset, names),
        FeatureOperation::Rotate {
            origin,
            axis,
            angle_radians,
            ..
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(angle_radians, names);
        }
        FeatureOperation::Fillet { edges, radius, .. } => {
            for selector in edges {
                collect_edge_selector_parameters(selector, names);
            }
            collect_scalar_parameters(radius, names);
        }
        FeatureOperation::Chamfer {
            edges, distance, ..
        } => {
            for selector in edges {
                collect_edge_selector_parameters(selector, names);
            }
            collect_scalar_parameters(distance, names);
        }
        FeatureOperation::Hollow {
            faces,
            thickness,
            tolerance,
            ..
        } => {
            for selector in faces {
                collect_face_selector_parameters(selector, names);
            }
            collect_scalar_parameters(thickness, names);
            collect_scalar_parameters(tolerance, names);
        }
        FeatureOperation::Sew { tolerance, .. } => {
            collect_scalar_parameters(tolerance, names);
        }
        FeatureOperation::MakeSolid { .. } => {}
        FeatureOperation::Fuse { .. }
        | FeatureOperation::Cut { .. }
        | FeatureOperation::Common { .. } => {}
    }
}

fn collect_scalar_parameters<'a>(expression: &'a ScalarExpr, names: &mut HashSet<&'a str>) {
    match expression {
        ScalarExpr::Parameter(name) => {
            names.insert(name);
        }
        ScalarExpr::Negate(value) | ScalarExpr::Absolute(value) => {
            collect_scalar_parameters(value, names);
        }
        ScalarExpr::Add(left, right)
        | ScalarExpr::Subtract(left, right)
        | ScalarExpr::Multiply(left, right)
        | ScalarExpr::Divide(left, right)
        | ScalarExpr::Minimum(left, right)
        | ScalarExpr::Maximum(left, right) => {
            collect_scalar_parameters(left, names);
            collect_scalar_parameters(right, names);
        }
        ScalarExpr::Clamp {
            value,
            minimum,
            maximum,
        } => {
            collect_scalar_parameters(value, names);
            collect_scalar_parameters(minimum, names);
            collect_scalar_parameters(maximum, names);
        }
        ScalarExpr::Conditional {
            left,
            right,
            when_true,
            when_false,
            ..
        } => {
            collect_scalar_parameters(left, names);
            collect_scalar_parameters(right, names);
            collect_scalar_parameters(when_true, names);
            collect_scalar_parameters(when_false, names);
        }
        ScalarExpr::Literal(_) => {}
    }
}

fn collect_vector_parameters<'a>(expression: &'a VectorExpr, names: &mut HashSet<&'a str>) {
    match expression {
        VectorExpr::Parameter(name) => {
            names.insert(name);
        }
        VectorExpr::Components { x, y, z } => {
            collect_scalar_parameters(x, names);
            collect_scalar_parameters(y, names);
            collect_scalar_parameters(z, names);
        }
        VectorExpr::Add(left, right) | VectorExpr::Subtract(left, right) => {
            collect_vector_parameters(left, names);
            collect_vector_parameters(right, names);
        }
        VectorExpr::Scale { vector, factor } => {
            collect_vector_parameters(vector, names);
            collect_scalar_parameters(factor, names);
        }
        VectorExpr::Normalize(vector) => collect_vector_parameters(vector, names),
        VectorExpr::Literal(_) => {}
    }
}

fn collect_edge_selector_parameters<'a>(selector: &'a EdgeSelector, names: &mut HashSet<&'a str>) {
    match selector {
        EdgeSelector::NearestCenter {
            target,
            maximum_distance,
        } => {
            collect_vector_parameters(target, names);
            collect_scalar_parameters(maximum_distance, names);
        }
        EdgeSelector::AtExtreme { tolerance, .. } => {
            collect_scalar_parameters(tolerance, names);
        }
        EdgeSelector::Longest {
            relative_tolerance, ..
        } => collect_scalar_parameters(relative_tolerance, names),
        EdgeSelector::CircularRadius { minimum, maximum }
        | EdgeSelector::CurvatureRadius { minimum, maximum }
        | EdgeSelector::CurvatureRadiusRange {
            minimum, maximum, ..
        } => {
            collect_scalar_parameters(minimum, names);
            collect_scalar_parameters(maximum, names);
        }
        EdgeSelector::CurvatureRadiusBounds {
            minimum,
            maximum,
            relative_tolerance,
            ..
        } => {
            collect_scalar_parameters(minimum, names);
            collect_scalar_parameters(maximum, names);
            collect_scalar_parameters(relative_tolerance, names);
        }
        EdgeSelector::Union(selectors) | EdgeSelector::Intersection(selectors) => {
            for selector in selectors {
                collect_edge_selector_parameters(selector, names);
            }
        }
        EdgeSelector::Difference { base, subtract } => {
            collect_edge_selector_parameters(base, names);
            collect_edge_selector_parameters(subtract, names);
        }
        EdgeSelector::History { source, .. } => collect_edge_selector_parameters(source, names),
    }
}

fn collect_face_selector_parameters<'a>(selector: &'a FaceSelector, names: &mut HashSet<&'a str>) {
    match selector {
        FaceSelector::NearestCenter {
            target,
            maximum_distance,
        } => {
            collect_vector_parameters(target, names);
            collect_scalar_parameters(maximum_distance, names);
        }
        FaceSelector::AtExtreme { tolerance, .. } => {
            collect_scalar_parameters(tolerance, names);
        }
        FaceSelector::NormalAligned {
            direction,
            minimum_dot,
        } => {
            collect_vector_parameters(direction, names);
            collect_scalar_parameters(minimum_dot, names);
        }
        FaceSelector::LargestArea {
            relative_tolerance, ..
        } => collect_scalar_parameters(relative_tolerance, names),
        FaceSelector::AdjacentToEdges { edges, .. } => {
            collect_edge_selector_parameters(edges, names);
        }
        FaceSelector::TangentTo { faces, .. } => {
            collect_face_selector_parameters(faces, names);
        }
        FaceSelector::Union(selectors) | FaceSelector::Intersection(selectors) => {
            for selector in selectors {
                collect_face_selector_parameters(selector, names);
            }
        }
        FaceSelector::Difference { base, subtract } => {
            collect_face_selector_parameters(base, names);
            collect_face_selector_parameters(subtract, names);
        }
        FaceSelector::History { source, .. } => collect_face_selector_parameters(source, names),
    }
}

fn validate_definition(definition: &FamilyDefinition) -> Result<(), ModelError> {
    if definition.id.is_empty() || definition.version == 0 {
        return Err(ModelError::new("family id and version are required"));
    }
    let mut feature_ids = HashSet::new();
    insert_unique_ids(
        &mut feature_ids,
        definition
            .features
            .iter()
            .map(|feature| feature.id.as_str()),
        "feature ids must be nonempty and unique",
    )?;
    for feature in &definition.features {
        match &feature.operation {
            FeatureOperation::Sew { inputs, .. } if inputs.is_empty() => {
                return Err(ModelError::new(format!(
                    "feature '{}' requires at least one sewing input",
                    feature.id
                )));
            }
            FeatureOperation::MakeSolid { shells } if shells.is_empty() => {
                return Err(ModelError::new(format!(
                    "feature '{}' requires at least one shell input",
                    feature.id
                )));
            }
            _ => {}
        }
        if let Some(dependency) = feature
            .operation
            .dependencies()
            .into_iter()
            .find(|dependency| !feature_ids.contains(dependency))
        {
            return Err(ModelError::new(format!(
                "feature '{}' references unknown output '{dependency}'",
                feature.id
            )));
        }
    }
    let mut parameter_ids = HashSet::new();
    insert_unique_ids(
        &mut parameter_ids,
        definition
            .parameters
            .iter()
            .map(|parameter| parameter.id.as_str()),
        "parameter ids must be nonempty and unique",
    )?;
    insert_unique_ids(
        &mut parameter_ids,
        definition
            .derived_parameters
            .iter()
            .map(|parameter| parameter.id.as_str())
            .chain(
                definition
                    .derived_vector_parameters
                    .iter()
                    .map(|parameter| parameter.id.as_str()),
            ),
        "input and derived parameter ids must be nonempty and unique",
    )?;
    insert_unique_ids(
        &mut HashSet::new(),
        definition
            .constraints
            .iter()
            .map(|constraint| constraint.id.as_str()),
        "constraint ids must be nonempty and unique",
    )?;
    if definition
        .requirements
        .iter()
        .any(|requirement| requirement.version == 0)
    {
        return Err(ModelError::new(
            "requirement ids must be nonempty, versioned, and unique",
        ));
    }
    insert_unique_ids(
        &mut HashSet::new(),
        definition
            .requirements
            .iter()
            .map(|requirement| requirement.id.as_str()),
        "requirement ids must be nonempty, versioned, and unique",
    )?;
    assembly::validate_datums(definition)
}

/// Adds every id to `seen`, failing on the first empty or repeated id.
fn insert_unique_ids<'a>(
    seen: &mut HashSet<&'a str>,
    ids: impl IntoIterator<Item = &'a str>,
    message: &str,
) -> Result<(), ModelError> {
    for id in ids {
        if id.is_empty() || !seen.insert(id) {
            return Err(ModelError::new(message));
        }
    }
    Ok(())
}

fn resolve_parameters(
    definition: &FamilyDefinition,
    overrides: &HashMap<String, ParameterValue>,
) -> Result<HashMap<String, ParameterValue>, ModelError> {
    let mut resolved = HashMap::new();
    for parameter in &definition.parameters {
        let value = overrides.get(&parameter.id).unwrap_or(&parameter.default);
        validate_parameter(parameter, value)?;
        resolved.insert(parameter.id.clone(), value.clone());
    }
    for name in overrides.keys() {
        if !resolved.contains_key(name) {
            return Err(ModelError::new(format!(
                "unknown parameter override '{name}'"
            )));
        }
    }
    let derived = definition
        .derived_parameters
        .iter()
        .map(|parameter| (parameter.id.as_str(), parameter))
        .collect::<HashMap<_, _>>();
    for parameter in &definition.derived_parameters {
        resolve_derived_parameter(&parameter.id, &derived, &mut resolved, &mut Vec::new())?;
    }
    let derived_vectors = definition
        .derived_vector_parameters
        .iter()
        .map(|parameter| (parameter.id.as_str(), parameter))
        .collect::<HashMap<_, _>>();
    for parameter in &definition.derived_vector_parameters {
        resolve_derived_vector_parameter(
            &parameter.id,
            &derived_vectors,
            &mut resolved,
            &mut Vec::new(),
        )?;
    }
    for constraint in &definition.constraints {
        validate_constraint(constraint, &resolved)?;
    }
    Ok(resolved)
}

#[derive(Clone, Copy)]
struct EvaluatedScalar {
    value: f64,
    dimension: Dimension,
}

impl EvaluatedScalar {
    fn from_quantity(quantity: Quantity) -> Result<Self, ModelError> {
        Ok(Self {
            value: quantity.normalized()?,
            dimension: quantity.dimension,
        })
    }

    fn into_parameter_value(self) -> ParameterValue {
        let quantity = match self.dimension {
            Dimension::Scalar => Quantity::scalar(self.value),
            Dimension::Length => Quantity::length(self.value, LengthUnit::Millimeter),
        };
        ParameterValue::Scalar(quantity)
    }
}

fn resolve_derived_parameter(
    name: &str,
    definitions: &HashMap<&str, &DerivedParameterDefinition>,
    resolved: &mut HashMap<String, ParameterValue>,
    visiting: &mut Vec<String>,
) -> Result<EvaluatedScalar, ModelError> {
    if let Some(value) = resolved.get(name) {
        return evaluated_parameter(name, value);
    }
    if let Some(position) = visiting.iter().position(|item| item == name) {
        let mut cycle = visiting[position..].to_vec();
        cycle.push(name.into());
        return Err(ModelError::new(format!(
            "derived parameter cycle: {}",
            cycle.join(" -> ")
        )));
    }
    let definition = definitions
        .get(name)
        .ok_or_else(|| ModelError::new(format!("unknown parameter '{name}'")))?;
    visiting.push(name.into());
    let value =
        evaluate_derived_expression(&definition.expression, definitions, resolved, visiting)?;
    visiting.pop();
    if value.dimension != definition.dimension {
        return Err(ModelError::new(format!(
            "derived parameter '{}' has the wrong dimension",
            definition.id
        )));
    }
    resolved.insert(definition.id.clone(), value.into_parameter_value());
    Ok(value)
}

fn evaluate_derived_expression(
    expression: &ScalarExpr,
    definitions: &HashMap<&str, &DerivedParameterDefinition>,
    resolved: &mut HashMap<String, ParameterValue>,
    visiting: &mut Vec<String>,
) -> Result<EvaluatedScalar, ModelError> {
    match expression {
        ScalarExpr::Literal(value) => EvaluatedScalar::from_quantity(*value),
        ScalarExpr::Parameter(name) => {
            resolve_derived_parameter(name, definitions, resolved, visiting)
        }
        ScalarExpr::Negate(value) => negate_scalar(evaluate_derived_expression(
            value,
            definitions,
            resolved,
            visiting,
        )?),
        ScalarExpr::Absolute(value) => absolute_scalar(evaluate_derived_expression(
            value,
            definitions,
            resolved,
            visiting,
        )?),
        ScalarExpr::Add(left, right) => combine_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            ScalarOperator::Add,
        ),
        ScalarExpr::Subtract(left, right) => combine_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            ScalarOperator::Subtract,
        ),
        ScalarExpr::Multiply(left, right) => combine_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            ScalarOperator::Multiply,
        ),
        ScalarExpr::Divide(left, right) => combine_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            ScalarOperator::Divide,
        ),
        ScalarExpr::Minimum(left, right) => min_or_max_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            false,
        ),
        ScalarExpr::Maximum(left, right) => min_or_max_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            true,
        ),
        ScalarExpr::Clamp {
            value,
            minimum,
            maximum,
        } => clamp_scalar(
            evaluate_derived_expression(value, definitions, resolved, visiting)?,
            evaluate_derived_expression(minimum, definitions, resolved, visiting)?,
            evaluate_derived_expression(maximum, definitions, resolved, visiting)?,
        ),
        ScalarExpr::Conditional {
            left,
            relation,
            right,
            when_true,
            when_false,
        } => conditional_scalar(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            relation,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            evaluate_derived_expression(when_true, definitions, resolved, visiting)?,
            evaluate_derived_expression(when_false, definitions, resolved, visiting)?,
        ),
    }
}

#[derive(Clone, Copy)]
enum ScalarOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

fn combine_scalars(
    left: EvaluatedScalar,
    right: EvaluatedScalar,
    operator: ScalarOperator,
) -> Result<EvaluatedScalar, ModelError> {
    let (value, dimension) = match operator {
        ScalarOperator::Add | ScalarOperator::Subtract => {
            if left.dimension != right.dimension {
                return Err(ModelError::new(
                    "addition and subtraction require matching dimensions",
                ));
            }
            let value = if matches!(operator, ScalarOperator::Add) {
                left.value + right.value
            } else {
                left.value - right.value
            };
            (value, left.dimension)
        }
        ScalarOperator::Multiply => match (left.dimension, right.dimension) {
            (Dimension::Scalar, dimension) => (left.value * right.value, dimension),
            (dimension, Dimension::Scalar) => (left.value * right.value, dimension),
            _ => {
                return Err(ModelError::new(
                    "multiplication requires at least one scalar operand",
                ));
            }
        },
        ScalarOperator::Divide => {
            if right.value == 0.0 {
                return Err(ModelError::new("division by zero in scalar expression"));
            }
            match (left.dimension, right.dimension) {
                (dimension, Dimension::Scalar) => (left.value / right.value, dimension),
                (left_dimension, right_dimension) if left_dimension == right_dimension => {
                    (left.value / right.value, Dimension::Scalar)
                }
                _ => {
                    return Err(ModelError::new(
                        "division requires a scalar divisor or matching dimensions",
                    ));
                }
            }
        }
    };
    if !value.is_finite() {
        return Err(ModelError::new("scalar expression result is not finite"));
    }
    Ok(EvaluatedScalar { value, dimension })
}

fn negate_scalar(value: EvaluatedScalar) -> Result<EvaluatedScalar, ModelError> {
    let negated = -value.value;
    if !negated.is_finite() {
        return Err(ModelError::new("scalar expression result is not finite"));
    }
    Ok(EvaluatedScalar {
        value: negated,
        dimension: value.dimension,
    })
}

fn absolute_scalar(value: EvaluatedScalar) -> Result<EvaluatedScalar, ModelError> {
    let absolute = value.value.abs();
    if !absolute.is_finite() {
        return Err(ModelError::new("scalar expression result is not finite"));
    }
    Ok(EvaluatedScalar {
        value: absolute,
        dimension: value.dimension,
    })
}

fn min_or_max_scalars(
    left: EvaluatedScalar,
    right: EvaluatedScalar,
    maximum: bool,
) -> Result<EvaluatedScalar, ModelError> {
    if left.dimension != right.dimension {
        return Err(ModelError::new(
            "minimum and maximum require matching dimensions",
        ));
    }
    Ok(EvaluatedScalar {
        value: if maximum {
            left.value.max(right.value)
        } else {
            left.value.min(right.value)
        },
        dimension: left.dimension,
    })
}

fn clamp_scalar(
    value: EvaluatedScalar,
    minimum: EvaluatedScalar,
    maximum: EvaluatedScalar,
) -> Result<EvaluatedScalar, ModelError> {
    if value.dimension != minimum.dimension || value.dimension != maximum.dimension {
        return Err(ModelError::new("clamp requires matching dimensions"));
    }
    if minimum.value > maximum.value {
        return Err(ModelError::new("clamp minimum must not exceed its maximum"));
    }
    Ok(EvaluatedScalar {
        value: value.value.clamp(minimum.value, maximum.value),
        dimension: value.dimension,
    })
}

fn compare_scalars(
    left: EvaluatedScalar,
    relation: &ConstraintRelation,
    right: EvaluatedScalar,
    subject: &str,
) -> Result<bool, ModelError> {
    if left.dimension != right.dimension {
        return Err(ModelError::new(format!(
            "{subject} compares different dimensions"
        )));
    }
    match relation {
        ConstraintRelation::LessOrEqual => Ok(left.value <= right.value),
        ConstraintRelation::GreaterOrEqual => Ok(left.value >= right.value),
        ConstraintRelation::Equal { tolerance } => {
            if tolerance.dimension != left.dimension {
                return Err(ModelError::new(format!(
                    "{subject} tolerance has the wrong dimension"
                )));
            }
            let tolerance = tolerance.normalized()?;
            if tolerance < 0.0 {
                return Err(ModelError::new(format!("{subject} tolerance is negative")));
            }
            Ok((left.value - right.value).abs() <= tolerance)
        }
    }
}

fn conditional_scalar(
    left: EvaluatedScalar,
    relation: &ConstraintRelation,
    right: EvaluatedScalar,
    when_true: EvaluatedScalar,
    when_false: EvaluatedScalar,
) -> Result<EvaluatedScalar, ModelError> {
    if when_true.dimension != when_false.dimension {
        return Err(ModelError::new(
            "conditional expression branches require matching dimensions",
        ));
    }
    if compare_scalars(left, relation, right, "conditional expression")? {
        Ok(when_true)
    } else {
        Ok(when_false)
    }
}

fn evaluated_parameter(name: &str, value: &ParameterValue) -> Result<EvaluatedScalar, ModelError> {
    match value {
        ParameterValue::Scalar(quantity) => EvaluatedScalar::from_quantity(*quantity),
        _ => Err(ModelError::new(format!("parameter '{name}' is not scalar"))),
    }
}

#[derive(Clone, Copy)]
struct EvaluatedVector {
    value: Vec3,
    dimension: Dimension,
}

impl EvaluatedVector {
    fn from_quantity(vector: VectorQuantity) -> Result<Self, ModelError> {
        let dimension = vector.x.dimension;
        Ok(Self {
            value: vector.normalized(dimension)?,
            dimension,
        })
    }

    fn into_parameter_value(self) -> ParameterValue {
        let vector = match self.dimension {
            Dimension::Scalar => VectorQuantity::scalars(self.value.x, self.value.y, self.value.z),
            Dimension::Length => VectorQuantity::lengths(
                self.value.x,
                self.value.y,
                self.value.z,
                LengthUnit::Millimeter,
            ),
        };
        ParameterValue::Vector(vector)
    }
}

fn evaluated_vector_parameter(
    name: &str,
    value: &ParameterValue,
) -> Result<EvaluatedVector, ModelError> {
    match value {
        ParameterValue::Vector(vector) => EvaluatedVector::from_quantity(*vector),
        _ => Err(ModelError::new(format!(
            "parameter '{name}' is not a vector"
        ))),
    }
}

fn resolve_derived_vector_parameter(
    name: &str,
    definitions: &HashMap<&str, &DerivedVectorParameterDefinition>,
    resolved: &mut HashMap<String, ParameterValue>,
    visiting: &mut Vec<String>,
) -> Result<EvaluatedVector, ModelError> {
    if let Some(value) = resolved.get(name) {
        return evaluated_vector_parameter(name, value);
    }
    if let Some(position) = visiting.iter().position(|item| item == name) {
        let mut cycle = visiting[position..].to_vec();
        cycle.push(name.into());
        return Err(ModelError::new(format!(
            "derived vector parameter cycle: {}",
            cycle.join(" -> ")
        )));
    }
    let definition = definitions
        .get(name)
        .ok_or_else(|| ModelError::new(format!("unknown vector parameter '{name}'")))?;
    visiting.push(name.into());
    let value = evaluate_derived_vector_expression(
        &definition.expression,
        definitions,
        resolved,
        visiting,
    )?;
    visiting.pop();
    if value.dimension != definition.dimension {
        return Err(ModelError::new(format!(
            "derived vector parameter '{}' has the wrong dimension",
            definition.id
        )));
    }
    resolved.insert(definition.id.clone(), value.into_parameter_value());
    Ok(value)
}

fn evaluate_derived_vector_expression(
    expression: &VectorExpr,
    definitions: &HashMap<&str, &DerivedVectorParameterDefinition>,
    resolved: &mut HashMap<String, ParameterValue>,
    visiting: &mut Vec<String>,
) -> Result<EvaluatedVector, ModelError> {
    match expression {
        VectorExpr::Literal(value) => EvaluatedVector::from_quantity(*value),
        VectorExpr::Parameter(name) => match resolved.get(name) {
            Some(value) => evaluated_vector_parameter(name, value),
            None => resolve_derived_vector_parameter(name, definitions, resolved, visiting),
        },
        VectorExpr::Components { x, y, z } => vector_from_components(
            evaluate_resolved_expression(x, resolved)?,
            evaluate_resolved_expression(y, resolved)?,
            evaluate_resolved_expression(z, resolved)?,
        ),
        VectorExpr::Add(left, right) => combine_vectors(
            evaluate_derived_vector_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_vector_expression(right, definitions, resolved, visiting)?,
            false,
        ),
        VectorExpr::Subtract(left, right) => combine_vectors(
            evaluate_derived_vector_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_vector_expression(right, definitions, resolved, visiting)?,
            true,
        ),
        VectorExpr::Scale { vector, factor } => scale_vector(
            evaluate_derived_vector_expression(vector, definitions, resolved, visiting)?,
            evaluate_resolved_expression(factor, resolved)?,
        ),
        VectorExpr::Normalize(vector) => normalize_vector(evaluate_derived_vector_expression(
            vector,
            definitions,
            resolved,
            visiting,
        )?),
    }
}

fn vector_from_components(
    x: EvaluatedScalar,
    y: EvaluatedScalar,
    z: EvaluatedScalar,
) -> Result<EvaluatedVector, ModelError> {
    if x.dimension != y.dimension || x.dimension != z.dimension {
        return Err(ModelError::new(
            "vector components require matching dimensions",
        ));
    }
    Ok(EvaluatedVector {
        value: Vec3::new(x.value, y.value, z.value),
        dimension: x.dimension,
    })
}

fn combine_vectors(
    left: EvaluatedVector,
    right: EvaluatedVector,
    subtract: bool,
) -> Result<EvaluatedVector, ModelError> {
    if left.dimension != right.dimension {
        return Err(ModelError::new(
            "vector addition and subtraction require matching dimensions",
        ));
    }
    let sign = if subtract { -1.0 } else { 1.0 };
    Ok(EvaluatedVector {
        value: Vec3::new(
            left.value.x + sign * right.value.x,
            left.value.y + sign * right.value.y,
            left.value.z + sign * right.value.z,
        ),
        dimension: left.dimension,
    })
}

fn scale_vector(
    vector: EvaluatedVector,
    factor: EvaluatedScalar,
) -> Result<EvaluatedVector, ModelError> {
    if factor.dimension != Dimension::Scalar {
        return Err(ModelError::new("vector scale factor must be scalar"));
    }
    let value = Vec3::new(
        vector.value.x * factor.value,
        vector.value.y * factor.value,
        vector.value.z * factor.value,
    );
    if !value.x.is_finite() || !value.y.is_finite() || !value.z.is_finite() {
        return Err(ModelError::new("vector expression result is not finite"));
    }
    Ok(EvaluatedVector {
        value,
        dimension: vector.dimension,
    })
}

fn normalize_vector(vector: EvaluatedVector) -> Result<EvaluatedVector, ModelError> {
    let magnitude = vector.value.x.hypot(vector.value.y.hypot(vector.value.z));
    if magnitude <= f64::EPSILON {
        return Err(ModelError::new("cannot normalize a zero vector"));
    }
    Ok(EvaluatedVector {
        value: Vec3::new(
            vector.value.x / magnitude,
            vector.value.y / magnitude,
            vector.value.z / magnitude,
        ),
        dimension: Dimension::Scalar,
    })
}

fn validate_constraint(
    constraint: &ParameterConstraint,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<(), ModelError> {
    let left = evaluate_resolved_expression(&constraint.left, parameters)?;
    let right = evaluate_resolved_expression(&constraint.right, parameters)?;
    let passed = compare_scalars(
        left,
        &constraint.relation,
        right,
        &format!("constraint '{}'", constraint.id),
    )?;
    if passed {
        Ok(())
    } else {
        Err(ModelError::new(format!(
            "constraint '{}' failed: {}",
            constraint.id, constraint.statement
        )))
    }
}

fn validate_parameter(
    definition: &ParameterDefinition,
    value: &ParameterValue,
) -> Result<(), ModelError> {
    match (&definition.parameter_type, value) {
        (ParameterType::Scalar(dimension), ParameterValue::Scalar(quantity)) => {
            if quantity.dimension != *dimension {
                return Err(ModelError::new(format!(
                    "parameter '{}' has the wrong dimension",
                    definition.id
                )));
            }
            let normalized = quantity.normalized()?;
            check_parameter_bound(
                definition,
                *dimension,
                definition.minimum,
                "minimum",
                |bound| (normalized < bound).then_some("below"),
            )?;
            check_parameter_bound(
                definition,
                *dimension,
                definition.maximum,
                "maximum",
                |bound| (normalized > bound).then_some("above"),
            )
        }
        (ParameterType::Vector(dimension), ParameterValue::Vector(vector)) => {
            vector.normalized(*dimension).map(|_| ())
        }
        (ParameterType::Integer, ParameterValue::Integer(_))
        | (ParameterType::Boolean, ParameterValue::Boolean(_)) => Ok(()),
        (ParameterType::Choice(options), ParameterValue::Choice(choice))
            if options.contains(choice) =>
        {
            Ok(())
        }
        _ => Err(ModelError::new(format!(
            "parameter '{}' has the wrong value type",
            definition.id
        ))),
    }
}

/// Checks one optional bound; `violated` returns "below" or "above" when the
/// normalized value lies outside the normalized bound.
fn check_parameter_bound(
    definition: &ParameterDefinition,
    dimension: Dimension,
    bound: Option<Quantity>,
    name: &str,
    violated: impl Fn(f64) -> Option<&'static str>,
) -> Result<(), ModelError> {
    let Some(bound) = bound else {
        return Ok(());
    };
    if bound.dimension != dimension {
        return Err(ModelError::new(format!(
            "parameter '{}' {name} has the wrong dimension",
            definition.id
        )));
    }
    match violated(bound.normalized()?) {
        Some(side) => Err(ModelError::new(format!(
            "parameter '{}' is {side} its {name}",
            definition.id
        ))),
        None => Ok(()),
    }
}

fn scalar(
    expression: &ScalarExpr,
    parameters: &HashMap<String, ParameterValue>,
    dimension: Dimension,
) -> Result<f64, ModelError> {
    let value = evaluate_resolved_expression(expression, parameters)?;
    if value.dimension != dimension {
        return Err(ModelError::new("expression has the wrong dimension"));
    }
    Ok(value.value)
}

fn evaluate_resolved_expression(
    expression: &ScalarExpr,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<EvaluatedScalar, ModelError> {
    match expression {
        ScalarExpr::Literal(value) => EvaluatedScalar::from_quantity(*value),
        ScalarExpr::Parameter(name) => parameters
            .get(name)
            .ok_or_else(|| ModelError::new(format!("unknown parameter '{name}'")))
            .and_then(|value| evaluated_parameter(name, value)),
        ScalarExpr::Negate(value) => {
            negate_scalar(evaluate_resolved_expression(value, parameters)?)
        }
        ScalarExpr::Absolute(value) => {
            absolute_scalar(evaluate_resolved_expression(value, parameters)?)
        }
        ScalarExpr::Add(left, right) => combine_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            ScalarOperator::Add,
        ),
        ScalarExpr::Subtract(left, right) => combine_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            ScalarOperator::Subtract,
        ),
        ScalarExpr::Multiply(left, right) => combine_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            ScalarOperator::Multiply,
        ),
        ScalarExpr::Divide(left, right) => combine_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            ScalarOperator::Divide,
        ),
        ScalarExpr::Minimum(left, right) => min_or_max_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            false,
        ),
        ScalarExpr::Maximum(left, right) => min_or_max_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            true,
        ),
        ScalarExpr::Clamp {
            value,
            minimum,
            maximum,
        } => clamp_scalar(
            evaluate_resolved_expression(value, parameters)?,
            evaluate_resolved_expression(minimum, parameters)?,
            evaluate_resolved_expression(maximum, parameters)?,
        ),
        ScalarExpr::Conditional {
            left,
            relation,
            right,
            when_true,
            when_false,
        } => conditional_scalar(
            evaluate_resolved_expression(left, parameters)?,
            relation,
            evaluate_resolved_expression(right, parameters)?,
            evaluate_resolved_expression(when_true, parameters)?,
            evaluate_resolved_expression(when_false, parameters)?,
        ),
    }
}

fn vector(
    expression: &VectorExpr,
    parameters: &HashMap<String, ParameterValue>,
    dimension: Dimension,
) -> Result<Vec3, ModelError> {
    let value = evaluate_resolved_vector_expression(expression, parameters)?;
    if value.dimension != dimension {
        return Err(ModelError::new("vector expression has the wrong dimension"));
    }
    Ok(value.value)
}

fn evaluate_resolved_vector_expression(
    expression: &VectorExpr,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<EvaluatedVector, ModelError> {
    match expression {
        VectorExpr::Literal(value) => EvaluatedVector::from_quantity(*value),
        VectorExpr::Parameter(name) => parameters
            .get(name)
            .ok_or_else(|| ModelError::new(format!("unknown parameter '{name}'")))
            .and_then(|value| evaluated_vector_parameter(name, value)),
        VectorExpr::Components { x, y, z } => vector_from_components(
            evaluate_resolved_expression(x, parameters)?,
            evaluate_resolved_expression(y, parameters)?,
            evaluate_resolved_expression(z, parameters)?,
        ),
        VectorExpr::Add(left, right) => combine_vectors(
            evaluate_resolved_vector_expression(left, parameters)?,
            evaluate_resolved_vector_expression(right, parameters)?,
            false,
        ),
        VectorExpr::Subtract(left, right) => combine_vectors(
            evaluate_resolved_vector_expression(left, parameters)?,
            evaluate_resolved_vector_expression(right, parameters)?,
            true,
        ),
        VectorExpr::Scale { vector, factor } => scale_vector(
            evaluate_resolved_vector_expression(vector, parameters)?,
            evaluate_resolved_expression(factor, parameters)?,
        ),
        VectorExpr::Normalize(vector) => {
            normalize_vector(evaluate_resolved_vector_expression(vector, parameters)?)
        }
    }
}

fn execute_feature<'session>(
    session: &'session Session,
    feature: &FeatureDefinition,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    let result = match &feature.operation {
        FeatureOperation::Box { origin, size } => session.create_box(
            vector(origin, parameters, Dimension::Length)?,
            vector(size, parameters, Dimension::Length)?,
        ),
        FeatureOperation::Cylinder {
            origin,
            axis,
            radius,
            height,
        } => session.create_cylinder(
            vector(origin, parameters, Dimension::Length)?,
            vector(axis, parameters, Dimension::Scalar)?,
            scalar(radius, parameters, Dimension::Length)?,
            scalar(height, parameters, Dimension::Length)?,
        ),
        FeatureOperation::Translate { input, offset } => session.translate(
            shape(shapes, input)?,
            vector(offset, parameters, Dimension::Length)?,
        ),
        FeatureOperation::Rotate {
            input,
            origin,
            axis,
            angle_radians,
        } => session.rotate(
            shape(shapes, input)?,
            vector(origin, parameters, Dimension::Length)?,
            vector(axis, parameters, Dimension::Scalar)?,
            scalar(angle_radians, parameters, Dimension::Scalar)?,
        ),
        FeatureOperation::Fuse { left, right } => {
            session.fuse(shape(shapes, left)?, shape(shapes, right)?)
        }
        FeatureOperation::Cut { object, tool } => {
            session.cut(shape(shapes, object)?, shape(shapes, tool)?)
        }
        FeatureOperation::Common { left, right } => {
            session.common(shape(shapes, left)?, shape(shapes, right)?)
        }
        FeatureOperation::Sew { inputs, tolerance } => {
            let inputs = inputs
                .iter()
                .map(|input| shape(shapes, input))
                .collect::<Result<Vec<_>, _>>()?;
            session.sew(&inputs, scalar(tolerance, parameters, Dimension::Length)?)
        }
        FeatureOperation::MakeSolid { shells } => {
            let shells = shells
                .iter()
                .map(|shell| shape(shapes, shell))
                .collect::<Result<Vec<_>, _>>()?;
            session.make_solid_from_shells(&shells)
        }
        FeatureOperation::Fillet {
            input,
            edges,
            radius,
        } => {
            return execute_fillet(
                session,
                shape(shapes, input)?,
                edges,
                scalar(radius, parameters, Dimension::Length)?,
                parameters,
                shapes,
            );
        }
        FeatureOperation::Chamfer {
            input,
            edges,
            distance,
        } => {
            return execute_chamfer(
                session,
                shape(shapes, input)?,
                edges,
                scalar(distance, parameters, Dimension::Length)?,
                parameters,
                shapes,
            );
        }
        FeatureOperation::Hollow {
            input,
            faces,
            thickness,
            tolerance,
        } => {
            return execute_hollow(
                session,
                shape(shapes, input)?,
                faces,
                scalar(thickness, parameters, Dimension::Length)?,
                scalar(tolerance, parameters, Dimension::Length)?,
                parameters,
                shapes,
            );
        }
    };
    result.map_err(Into::into)
}

fn execute_fillet<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[EdgeSelector],
    radius: f64,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    let selected = resolve_edge_selectors(session, input, selectors, parameters, shapes, "fillet")?;
    let references = selected.iter().collect::<Vec<_>>();
    let result = session
        .fillet(input, &references, radius)
        .map_err(Into::into);
    cleanup_shapes(session, selected);
    result
}

fn execute_chamfer<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[EdgeSelector],
    distance: f64,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    let selected =
        resolve_edge_selectors(session, input, selectors, parameters, shapes, "chamfer")?;
    let references = selected.iter().collect::<Vec<_>>();
    let result = session
        .chamfer(input, &references, distance)
        .map_err(Into::into);
    cleanup_shapes(session, selected);
    result
}

fn execute_hollow<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[FaceSelector],
    thickness: f64,
    tolerance: f64,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    if selectors.is_empty() {
        return Err(ModelError::new(
            "hollow requires at least one face selector",
        ));
    }
    let mut selected = Vec::new();
    for selector in selectors {
        match resolve_face_selector(session, input, selector, parameters, shapes) {
            Ok(faces) => selected.extend(faces),
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error);
            }
        }
    }
    let references = selected.iter().collect::<Vec<_>>();
    let result = session
        .hollow(input, &references, thickness, tolerance)
        .map_err(Into::into);
    cleanup_shapes(session, selected);
    result
}

fn resolve_edge_selectors<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[EdgeSelector],
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    operation: &str,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if selectors.is_empty() {
        return Err(ModelError::new(format!(
            "{operation} requires at least one edge selector"
        )));
    }
    let mut selected = Vec::new();
    for selector in selectors {
        match resolve_edge_selector(session, input, selector, parameters, shapes) {
            Ok(edges) => selected.extend(edges),
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error);
            }
        }
    }
    Ok(selected)
}

fn resolve_edge_selector<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    selector: &EdgeSelector,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Vec<Shape<'session>>, ModelError> {
    match selector {
        EdgeSelector::NearestCenter {
            target,
            maximum_distance,
        } => select_nearest_center(
            session,
            result,
            ShapeType::Edge,
            "edge",
            vector(target, parameters, Dimension::Length)?,
            scalar(maximum_distance, parameters, Dimension::Length)?,
        )
        .map(|shape| vec![shape]),
        EdgeSelector::AtExtreme {
            axis,
            extremum,
            tolerance,
        } => select_at_extreme(
            session,
            result,
            ShapeType::Edge,
            "edge",
            *axis,
            *extremum,
            scalar(tolerance, parameters, Dimension::Length)?,
        ),
        EdgeSelector::Longest {
            allow_ties,
            relative_tolerance,
        } => select_longest_edges(
            session,
            result,
            *allow_ties,
            scalar(relative_tolerance, parameters, Dimension::Scalar)?,
        ),
        EdgeSelector::CircularRadius { minimum, maximum } => select_circular_edges_by_radius(
            session,
            result,
            scalar(minimum, parameters, Dimension::Length)?,
            scalar(maximum, parameters, Dimension::Length)?,
        ),
        EdgeSelector::CurvatureRadius { minimum, maximum } => select_edges_by_curvature_radius(
            session,
            result,
            scalar(minimum, parameters, Dimension::Length)?,
            scalar(maximum, parameters, Dimension::Length)?,
        ),
        EdgeSelector::CurvatureRadiusRange {
            minimum,
            maximum,
            sample_count,
            require_entire_edge,
        } => select_edges_by_curvature_radius_range(
            session,
            result,
            scalar(minimum, parameters, Dimension::Length)?,
            scalar(maximum, parameters, Dimension::Length)?,
            *sample_count,
            *require_entire_edge,
        ),
        EdgeSelector::CurvatureRadiusBounds {
            minimum,
            maximum,
            relative_tolerance,
            require_entire_edge,
        } => select_edges_by_bounded_curvature_radius(
            session,
            result,
            scalar(minimum, parameters, Dimension::Length)?,
            scalar(maximum, parameters, Dimension::Length)?,
            scalar(relative_tolerance, parameters, Dimension::Scalar)?,
            *require_entire_edge,
        ),
        EdgeSelector::Union(selectors) => {
            let sets = resolve_edge_selector_sets(session, result, selectors, parameters, shapes)?;
            compose_shape_sets(session, sets, ShapeSetOperation::Union)
        }
        EdgeSelector::Intersection(selectors) => {
            let sets = resolve_edge_selector_sets(session, result, selectors, parameters, shapes)?;
            compose_shape_sets(session, sets, ShapeSetOperation::Intersection)
        }
        EdgeSelector::Difference { base, subtract } => {
            let base = resolve_edge_selector(session, result, base, parameters, shapes)?;
            let subtract =
                match resolve_edge_selector(session, result, subtract, parameters, shapes) {
                    Ok(subtract) => subtract,
                    Err(error) => {
                        cleanup_shapes(session, base);
                        return Err(error);
                    }
                };
            compose_shape_sets(session, vec![base, subtract], ShapeSetOperation::Difference)
        }
        EdgeSelector::History {
            source_feature,
            source,
            relation,
        } => {
            let source_result = shape(shapes, source_feature)?;
            let source_edges =
                resolve_edge_selector(session, source_result, source, parameters, shapes)?;
            resolve_history(
                session,
                result,
                source_edges,
                (*relation).into(),
                source_feature,
                "edges",
            )
        }
    }
}

fn resolve_face_selector<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    selector: &FaceSelector,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Vec<Shape<'session>>, ModelError> {
    match selector {
        FaceSelector::NearestCenter {
            target,
            maximum_distance,
        } => select_nearest_center(
            session,
            result,
            ShapeType::Face,
            "face",
            vector(target, parameters, Dimension::Length)?,
            scalar(maximum_distance, parameters, Dimension::Length)?,
        )
        .map(|shape| vec![shape]),
        FaceSelector::AtExtreme {
            axis,
            extremum,
            tolerance,
        } => select_at_extreme(
            session,
            result,
            ShapeType::Face,
            "face",
            *axis,
            *extremum,
            scalar(tolerance, parameters, Dimension::Length)?,
        ),
        FaceSelector::NormalAligned {
            direction,
            minimum_dot,
        } => select_faces_by_normal(
            session,
            result,
            vector(direction, parameters, Dimension::Scalar)?,
            scalar(minimum_dot, parameters, Dimension::Scalar)?,
        ),
        FaceSelector::LargestArea {
            planar_only,
            allow_ties,
            relative_tolerance,
        } => select_largest_faces(
            session,
            result,
            *planar_only,
            *allow_ties,
            scalar(relative_tolerance, parameters, Dimension::Scalar)?,
        ),
        FaceSelector::AdjacentToEdges {
            edges,
            minimum_count,
        } => {
            let edges = resolve_edge_selector(session, result, edges, parameters, shapes)?;
            select_faces_adjacent_to_edges(session, result, edges, *minimum_count)
        }
        FaceSelector::TangentTo {
            faces,
            minimum_count,
        } => {
            let faces = resolve_face_selector(session, result, faces, parameters, shapes)?;
            select_faces_tangent_to_faces(session, result, faces, *minimum_count)
        }
        FaceSelector::Union(selectors) => {
            let sets = resolve_face_selector_sets(session, result, selectors, parameters, shapes)?;
            compose_shape_sets(session, sets, ShapeSetOperation::Union)
        }
        FaceSelector::Intersection(selectors) => {
            let sets = resolve_face_selector_sets(session, result, selectors, parameters, shapes)?;
            compose_shape_sets(session, sets, ShapeSetOperation::Intersection)
        }
        FaceSelector::Difference { base, subtract } => {
            let base = resolve_face_selector(session, result, base, parameters, shapes)?;
            let subtract =
                match resolve_face_selector(session, result, subtract, parameters, shapes) {
                    Ok(subtract) => subtract,
                    Err(error) => {
                        cleanup_shapes(session, base);
                        return Err(error);
                    }
                };
            compose_shape_sets(session, vec![base, subtract], ShapeSetOperation::Difference)
        }
        FaceSelector::History {
            source_feature,
            source,
            relation,
        } => {
            let source_result = shape(shapes, source_feature)?;
            let source_faces =
                resolve_face_selector(session, source_result, source, parameters, shapes)?;
            resolve_history(
                session,
                result,
                source_faces,
                (*relation).into(),
                source_feature,
                "faces",
            )
        }
    }
}

#[derive(Clone, Copy)]
enum ShapeSetOperation {
    Union,
    Intersection,
    Difference,
}

fn resolve_edge_selector_sets<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    selectors: &[EdgeSelector],
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Vec<Vec<Shape<'session>>>, ModelError> {
    let mut sets = Vec::new();
    for selector in selectors {
        match resolve_edge_selector(session, result, selector, parameters, shapes) {
            Ok(set) => sets.push(set),
            Err(error) => {
                cleanup_shapes(session, sets.into_iter().flatten());
                return Err(error);
            }
        }
    }
    Ok(sets)
}

fn resolve_face_selector_sets<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    selectors: &[FaceSelector],
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Vec<Vec<Shape<'session>>>, ModelError> {
    let mut sets = Vec::new();
    for selector in selectors {
        match resolve_face_selector(session, result, selector, parameters, shapes) {
            Ok(set) => sets.push(set),
            Err(error) => {
                cleanup_shapes(session, sets.into_iter().flatten());
                return Err(error);
            }
        }
    }
    Ok(sets)
}

fn shape_set_contains(
    session: &Session,
    set: &[Shape<'_>],
    candidate: &Shape<'_>,
) -> Result<bool, ModelError> {
    for item in set {
        if session.is_same(item, candidate)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn cleanup_shape_sets<'session>(session: &'session Session, sets: Vec<Vec<Shape<'session>>>) {
    cleanup_shapes(session, sets.into_iter().flatten());
}

fn compose_shape_sets<'session>(
    session: &'session Session,
    sets: Vec<Vec<Shape<'session>>>,
    operation: ShapeSetOperation,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if sets.is_empty() || matches!(operation, ShapeSetOperation::Difference) && sets.len() != 2 {
        cleanup_shape_sets(session, sets);
        return Err(ModelError::new(
            "selector composition requires at least one selector",
        ));
    }
    let result = match operation {
        ShapeSetOperation::Union => union_shapes(session, sets)?,
        ShapeSetOperation::Intersection => intersect_shapes(session, sets)?,
        ShapeSetOperation::Difference => {
            let mut sets = sets.into_iter();
            let base = sets.next().unwrap_or_default();
            let subtract = sets.next().unwrap_or_default();
            let difference = filter_by_membership(session, base, &subtract, false);
            cleanup_shapes(session, subtract);
            difference?
        }
    };
    if result.is_empty() {
        return Err(ModelError::new("selector composition found no matches"));
    }
    Ok(result)
}

/// Keeps the first occurrence of each topologically distinct shape, in order.
fn union_shapes<'session>(
    session: &'session Session,
    sets: Vec<Vec<Shape<'session>>>,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let mut union = Vec::new();
    let mut pending = sets.into_iter().flatten();
    while let Some(candidate) = pending.next() {
        match shape_set_contains(session, &union, &candidate) {
            Ok(true) => {
                let _ = session.remove(candidate);
            }
            Ok(false) => union.push(candidate),
            Err(error) => {
                let _ = session.remove(candidate);
                cleanup_shapes(session, pending);
                cleanup_shapes(session, union);
                return Err(error);
            }
        }
    }
    Ok(union)
}

fn intersect_shapes<'session>(
    session: &'session Session,
    sets: Vec<Vec<Shape<'session>>>,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let mut sets = sets.into_iter();
    let mut intersection = sets.next().unwrap_or_default();
    for set in sets.by_ref() {
        let kept = filter_by_membership(session, intersection, &set, true);
        cleanup_shapes(session, set);
        match kept {
            Ok(kept) => intersection = kept,
            Err(error) => {
                cleanup_shapes(session, sets.flatten());
                return Err(error);
            }
        }
    }
    Ok(intersection)
}

/// Keeps candidates whose membership in `reference` equals `keep_members`,
/// in order, releasing the rest. On error every candidate handle is released.
fn filter_by_membership<'session>(
    session: &'session Session,
    candidates: Vec<Shape<'session>>,
    reference: &[Shape<'session>],
    keep_members: bool,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let mut kept = Vec::new();
    let mut pending = candidates.into_iter();
    while let Some(candidate) = pending.next() {
        match shape_set_contains(session, reference, &candidate) {
            Ok(member) if member == keep_members => kept.push(candidate),
            Ok(_) => {
                let _ = session.remove(candidate);
            }
            Err(error) => {
                let _ = session.remove(candidate);
                cleanup_shapes(session, pending);
                cleanup_shapes(session, kept);
                return Err(error);
            }
        }
    }
    Ok(kept)
}

fn validate_relative_tolerance(value: f64, kind: &str) -> Result<(), ModelError> {
    if !(0.0..=1.0).contains(&value) {
        return Err(ModelError::new(format!(
            "{kind} relative tolerance must be between 0 and 1"
        )));
    }
    Ok(())
}

fn select_longest_edges<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    allow_ties: bool,
    relative_tolerance: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    validate_relative_tolerance(relative_tolerance, "longest-edge selector")?;
    let count = session.subshape_count(shape, ShapeType::Edge)?;
    let mut candidates = Vec::with_capacity(count);
    for index in 0..count {
        let edge = match session.subshape(shape, ShapeType::Edge, index) {
            Ok(edge) => edge,
            Err(error) => {
                cleanup_shapes(
                    session,
                    candidates
                        .into_iter()
                        .map(|item: (Shape<'session>, f64)| item.0),
                );
                return Err(error.into());
            }
        };
        match session.edge_length(&edge) {
            Ok(length) => candidates.push((edge, length)),
            Err(error) => {
                let _ = session.remove(edge);
                cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
                return Err(error.into());
            }
        }
    }
    if candidates.is_empty() {
        return Err(ModelError::new("longest-edge selector found no candidates"));
    }
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));
    let threshold = candidates[0].1 * (1.0 - relative_tolerance);
    let split = candidates.partition_point(|candidate| candidate.1 >= threshold);
    let rejected = candidates.split_off(split);
    cleanup_shapes(session, rejected.into_iter().map(|item| item.0));
    if !allow_ties && candidates.len() > 1 {
        let count = candidates.len();
        cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
        return Err(ModelError::new(format!(
            "longest-edge selector is ambiguous across {count} edges"
        )));
    }
    Ok(candidates.into_iter().map(|item| item.0).collect())
}

fn select_circular_edges_by_radius<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    minimum: f64,
    maximum: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum < 0.0 || maximum < minimum {
        return Err(ModelError::new(
            "circular-edge radius range must be nonnegative and ordered",
        ));
    }
    let count = session.subshape_count(shape, ShapeType::Edge)?;
    let mut selected = Vec::new();
    for index in 0..count {
        let edge = match session.subshape(shape, ShapeType::Edge, index) {
            Ok(edge) => edge,
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        match session.edge_circle_radius(&edge) {
            Ok(Some(radius)) if (minimum..=maximum).contains(&radius) => selected.push(edge),
            Ok(_) => {
                let _ = session.remove(edge);
            }
            Err(error) => {
                let _ = session.remove(edge);
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        }
    }
    if selected.is_empty() {
        return Err(ModelError::new(
            "circular-edge radius selector found no matches",
        ));
    }
    Ok(selected)
}

fn select_edges_by_curvature_radius<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    minimum: f64,
    maximum: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum <= 0.0 || maximum < minimum {
        return Err(ModelError::new(
            "curvature-radius range must be positive and ordered",
        ));
    }
    filter_edges(
        session,
        shape,
        "curvature-radius selector found no matches",
        |_, edge| {
            Ok(match session.edge_curvature(edge)? {
                Some(curvature) if curvature > f64::EPSILON => {
                    (minimum..=maximum).contains(&(1.0 / curvature))
                }
                _ => false,
            })
        },
    )
}

/// Visits every edge of `shape` and keeps those `matches` accepts. Rejected
/// edges are released immediately; on error, or when nothing matches, every
/// selected handle is released as well.
fn filter_edges<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    no_match_message: &str,
    mut matches: impl FnMut(usize, &Shape<'session>) -> Result<bool, ModelError>,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let count = session.subshape_count(shape, ShapeType::Edge)?;
    let mut selected = Vec::new();
    for index in 0..count {
        let edge = match session.subshape(shape, ShapeType::Edge, index) {
            Ok(edge) => edge,
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        match matches(index, &edge) {
            Ok(true) => selected.push(edge),
            Ok(false) => {
                let _ = session.remove(edge);
            }
            Err(error) => {
                let _ = session.remove(edge);
                cleanup_shapes(session, selected);
                return Err(error);
            }
        }
    }
    if selected.is_empty() {
        return Err(ModelError::new(no_match_message));
    }
    Ok(selected)
}

/// Decides a curvature interval test from proven bounds. `lowest` and
/// `highest` are curvatures (reciprocal radii). `None` means the bounds
/// straddle a range boundary.
fn classify_curvature_bounds(
    extrema: &CurvatureExtrema,
    lowest: f64,
    highest: f64,
    require_entire_edge: bool,
) -> Option<bool> {
    if require_entire_edge {
        if extrema.minimum_lower_bound >= lowest && extrema.maximum_upper_bound <= highest {
            Some(true)
        } else if extrema.minimum < lowest || extrema.maximum > highest {
            Some(false)
        } else {
            None
        }
    } else if extrema.maximum >= lowest && extrema.minimum <= highest {
        Some(true)
    } else if extrema.maximum_upper_bound < lowest || extrema.minimum_lower_bound > highest {
        Some(false)
    } else {
        None
    }
}

fn select_edges_by_bounded_curvature_radius<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    minimum: f64,
    maximum: f64,
    relative_tolerance: f64,
    require_entire_edge: bool,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum <= 0.0 || maximum < minimum || !maximum.is_finite() {
        return Err(ModelError::new(
            "curvature-radius bounds must be positive, finite, and ordered",
        ));
    }
    if !(relative_tolerance > 0.0 && relative_tolerance <= 1.0) {
        return Err(ModelError::new(
            "curvature-radius relative tolerance must be in (0, 1]",
        ));
    }
    filter_edges(
        session,
        shape,
        "bounded curvature-radius selector found no matches",
        |index, edge| {
            let extrema = session
                .edge_curvature_extrema(edge, relative_tolerance)
                .map_err(|error| {
                    ModelError::new(format!(
                        "curvature-radius bounds for edge {index}: {}",
                        error.message
                    ))
                })?;
            classify_curvature_bounds(&extrema, 1.0 / maximum, 1.0 / minimum, require_entire_edge)
                .ok_or_else(|| {
                    let radius = |curvature: f64| 1.0 / curvature;
                    ModelError::new(format!(
                        "edge {index} curvature radius bounds straddle the selector range \
                         (minimum radius in [{:.9}, {:.9}] mm, maximum radius in [{:.9}, {:.9}] mm); \
                         tighten relative_tolerance",
                        radius(extrema.maximum_upper_bound),
                        radius(extrema.maximum),
                        radius(extrema.minimum),
                        radius(extrema.minimum_lower_bound),
                    ))
                })
        },
    )
}

fn select_edges_by_curvature_radius_range<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    minimum: f64,
    maximum: f64,
    sample_count: usize,
    require_entire_edge: bool,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum <= 0.0 || maximum < minimum {
        return Err(ModelError::new(
            "curvature-radius range must be positive and ordered",
        ));
    }
    if !(2..=100_000).contains(&sample_count) {
        return Err(ModelError::new(
            "curvature-radius sample count must be 2..100000",
        ));
    }
    filter_edges(
        session,
        shape,
        "curvature-radius range selector found no matches",
        |_, edge| {
            let (minimum_curvature, maximum_curvature) =
                session.edge_curvature_range(edge, sample_count)?;
            if maximum_curvature <= f64::EPSILON {
                return Ok(false);
            }
            let minimum_radius = 1.0 / maximum_curvature;
            let maximum_radius = if minimum_curvature <= f64::EPSILON {
                f64::INFINITY
            } else {
                1.0 / minimum_curvature
            };
            Ok(if require_entire_edge {
                minimum_radius >= minimum && maximum_radius <= maximum
            } else {
                maximum_radius >= minimum && minimum_radius <= maximum
            })
        },
    )
}

fn select_largest_faces<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    planar_only: bool,
    allow_ties: bool,
    relative_tolerance: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    validate_relative_tolerance(relative_tolerance, "largest-face selector")?;
    let count = session.subshape_count(shape, ShapeType::Face)?;
    let mut candidates = Vec::with_capacity(count);
    for index in 0..count {
        let face = match session.subshape(shape, ShapeType::Face, index) {
            Ok(face) => face,
            Err(error) => {
                cleanup_shapes(
                    session,
                    candidates
                        .into_iter()
                        .map(|item: (Shape<'session>, f64)| item.0),
                );
                return Err(error.into());
            }
        };
        if planar_only {
            match session.face_is_planar(&face) {
                Ok(true) => {}
                Ok(false) => {
                    let _ = session.remove(face);
                    continue;
                }
                Err(error) => {
                    let _ = session.remove(face);
                    cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
                    return Err(error.into());
                }
            }
        }
        match session.surface_area(&face) {
            Ok(area) => candidates.push((face, area)),
            Err(error) => {
                let _ = session.remove(face);
                cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
                return Err(error.into());
            }
        }
    }
    if candidates.is_empty() {
        return Err(ModelError::new("largest-face selector found no candidates"));
    }
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));
    let threshold = candidates[0].1 * (1.0 - relative_tolerance);
    let split = candidates.partition_point(|candidate| candidate.1 >= threshold);
    let rejected = candidates.split_off(split);
    cleanup_shapes(session, rejected.into_iter().map(|item| item.0));
    if !allow_ties && candidates.len() > 1 {
        let count = candidates.len();
        cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
        return Err(ModelError::new(format!(
            "largest-face selector is ambiguous across {count} faces"
        )));
    }
    Ok(candidates.into_iter().map(|item| item.0).collect())
}

fn select_faces_by_normal<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    direction: Vec3,
    minimum_dot: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let magnitude = direction.x.hypot(direction.y.hypot(direction.z));
    if magnitude <= f64::EPSILON {
        return Err(ModelError::new("face orientation direction is zero"));
    }
    if !(-1.0..=1.0).contains(&minimum_dot) {
        return Err(ModelError::new(
            "face orientation minimum dot must be between -1 and 1",
        ));
    }
    let direction = Vec3::new(
        direction.x / magnitude,
        direction.y / magnitude,
        direction.z / magnitude,
    );
    let count = session.subshape_count(shape, ShapeType::Face)?;
    let mut selected = Vec::new();
    for index in 0..count {
        let face = match session.subshape(shape, ShapeType::Face, index) {
            Ok(face) => face,
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        let normal = match session.face_normal(&face) {
            Ok(normal) => normal,
            Err(error) => {
                let _ = session.remove(face);
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        let dot = normal.x * direction.x + normal.y * direction.y + normal.z * direction.z;
        if dot >= minimum_dot {
            selected.push(face);
        } else {
            let _ = session.remove(face);
        }
    }
    if selected.is_empty() {
        return Err(ModelError::new(
            "face orientation selector found no matches",
        ));
    }
    Ok(selected)
}

fn select_faces_adjacent_to_edges<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    edges: Vec<Shape<'session>>,
    minimum_count: usize,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum_count == 0 {
        cleanup_shapes(session, edges);
        return Err(ModelError::new(
            "face adjacency minimum count must be positive",
        ));
    }
    if edges.len() < minimum_count {
        let edge_count = edges.len();
        cleanup_shapes(session, edges);
        return Err(ModelError::new(format!(
            "face adjacency requires {minimum_count} edges but selector resolved {edge_count}"
        )));
    }
    let count = match session.subshape_count(shape, ShapeType::Face) {
        Ok(count) => count,
        Err(error) => {
            cleanup_shapes(session, edges);
            return Err(error.into());
        }
    };
    let mut selected = Vec::new();
    for index in 0..count {
        let face = match session.subshape(shape, ShapeType::Face, index) {
            Ok(face) => face,
            Err(error) => {
                cleanup_shapes(session, selected);
                cleanup_shapes(session, edges);
                return Err(error.into());
            }
        };
        let mut adjacent_count = 0;
        for edge in &edges {
            match session.is_adjacent(shape, &face, edge) {
                Ok(true) => adjacent_count += 1,
                Ok(false) => {}
                Err(error) => {
                    let _ = session.remove(face);
                    cleanup_shapes(session, selected);
                    cleanup_shapes(session, edges);
                    return Err(error.into());
                }
            }
        }
        if adjacent_count >= minimum_count {
            selected.push(face);
        } else {
            let _ = session.remove(face);
        }
    }
    cleanup_shapes(session, edges);
    if selected.is_empty() {
        return Err(ModelError::new("face adjacency selector found no matches"));
    }
    Ok(selected)
}

fn select_faces_tangent_to_faces<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    sources: Vec<Shape<'session>>,
    minimum_count: usize,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum_count == 0 {
        cleanup_shapes(session, sources);
        return Err(ModelError::new(
            "face tangency minimum count must be positive",
        ));
    }
    if sources.len() < minimum_count {
        let source_count = sources.len();
        cleanup_shapes(session, sources);
        return Err(ModelError::new(format!(
            "face tangency requires {minimum_count} source faces but selector resolved {source_count}"
        )));
    }
    let count = match session.subshape_count(shape, ShapeType::Face) {
        Ok(count) => count,
        Err(error) => {
            cleanup_shapes(session, sources);
            return Err(error.into());
        }
    };
    let mut selected = Vec::new();
    for index in 0..count {
        let face = match session.subshape(shape, ShapeType::Face, index) {
            Ok(face) => face,
            Err(error) => {
                cleanup_shapes(session, selected);
                cleanup_shapes(session, sources);
                return Err(error.into());
            }
        };
        let mut tangent_count = 0;
        for source in &sources {
            match session.faces_are_tangent(shape, &face, source) {
                Ok(true) => tangent_count += 1,
                Ok(false) => {}
                Err(error) => {
                    let _ = session.remove(face);
                    cleanup_shapes(session, selected);
                    cleanup_shapes(session, sources);
                    return Err(error.into());
                }
            }
        }
        if tangent_count >= minimum_count {
            selected.push(face);
        } else {
            let _ = session.remove(face);
        }
    }
    cleanup_shapes(session, sources);
    if selected.is_empty() {
        return Err(ModelError::new("face tangency selector found no matches"));
    }
    Ok(selected)
}

fn resolve_history<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    sources: Vec<Shape<'session>>,
    relation: HistoryRelation,
    source_feature: &str,
    kind: &str,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let mut resolved = Vec::new();
    for source in &sources {
        let count = match session.history_count(result, source, relation) {
            Ok(count) => count,
            Err(error) => {
                cleanup_shapes(session, resolved);
                cleanup_shapes(session, sources);
                return Err(error.into());
            }
        };
        if count == 0 {
            cleanup_shapes(session, resolved);
            cleanup_shapes(session, sources);
            return Err(ModelError::new(format!(
                "history selector from '{source_feature}' resolved to no {kind}"
            )));
        }
        for index in 0..count {
            match session.history(result, source, relation, index) {
                Ok(shape) => resolved.push(shape),
                Err(error) => {
                    cleanup_shapes(session, resolved);
                    cleanup_shapes(session, sources);
                    return Err(error.into());
                }
            }
        }
    }
    cleanup_shapes(session, sources);
    Ok(resolved)
}

fn select_nearest_center<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    shape_type: ShapeType,
    kind: &str,
    target: Vec3,
    maximum_distance: f64,
) -> Result<Shape<'session>, ModelError> {
    if maximum_distance < 0.0 {
        return Err(ModelError::new(
            "{kind} selector maximum distance must be nonnegative",
        ));
    }
    let count = session.subshape_count(shape, shape_type)?;
    let mut candidates: Vec<(Shape<'session>, f64)> = Vec::with_capacity(count);
    for index in 0..count {
        let candidate = match session.subshape(shape, shape_type, index) {
            Ok(candidate) => candidate,
            Err(error) => {
                cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
                return Err(error.into());
            }
        };
        let center = match session.center_of_mass(&candidate) {
            Ok(center) => center,
            Err(error) => {
                let _ = session.remove(candidate);
                cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
                return Err(error.into());
            }
        };
        let distance = ((center.x - target.x).powi(2)
            + (center.y - target.y).powi(2)
            + (center.z - target.z).powi(2))
        .sqrt();
        candidates.push((candidate, distance));
    }
    if candidates.is_empty() {
        return Err(ModelError::new(format!(
            "{kind} selector found no candidates"
        )));
    }
    candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
    let best_distance = candidates[0].1;
    if best_distance > maximum_distance {
        cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
        return Err(ModelError::new(format!(
            "nearest {kind} is {best_distance} mm away, beyond the {maximum_distance} mm limit"
        )));
    }
    let ambiguity_tolerance = 1e-9_f64.max(best_distance.abs() * 1e-12);
    if candidates
        .get(1)
        .is_some_and(|candidate| (candidate.1 - best_distance).abs() <= ambiguity_tolerance)
    {
        cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
        return Err(ModelError::new(format!(
            "nearest-{kind} selector is ambiguous at distance {best_distance} mm"
        )));
    }
    let selected = candidates.remove(0).0;
    cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
    Ok(selected)
}

fn select_at_extreme<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    shape_type: ShapeType,
    kind: &str,
    axis: CoordinateAxis,
    extremum: Extremum,
    tolerance: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if tolerance < 0.0 {
        return Err(ModelError::new(format!(
            "{kind} extremum tolerance must be nonnegative"
        )));
    }
    let bounds = session.bounds(shape)?;
    let target = axis.component(match extremum {
        Extremum::Minimum => bounds.min,
        Extremum::Maximum => bounds.max,
    });
    let count = session.subshape_count(shape, shape_type)?;
    let mut selected = Vec::new();
    for index in 0..count {
        let candidate = match session.subshape(shape, shape_type, index) {
            Ok(candidate) => candidate,
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        let center = match session.center_of_mass(&candidate) {
            Ok(center) => center,
            Err(error) => {
                let _ = session.remove(candidate);
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        if (axis.component(center) - target).abs() <= tolerance {
            selected.push(candidate);
        } else {
            let _ = session.remove(candidate);
        }
    }
    if selected.is_empty() {
        return Err(ModelError::new(format!(
            "{kind} extremum selector found no matches"
        )));
    }
    Ok(selected)
}

fn cleanup_shapes<'session>(session: &Session, shapes: impl IntoIterator<Item = Shape<'session>>) {
    for shape in shapes {
        let _ = session.remove(shape);
    }
}

fn shape<'a, 'session>(
    shapes: &'a HashMap<String, Shape<'session>>,
    name: &str,
) -> Result<&'a Shape<'session>, ModelError> {
    shapes
        .get(name)
        .ok_or_else(|| ModelError::new(format!("named output '{name}' was not generated")))
}

fn verify_requirement(
    session: &Session,
    requirement: &Requirement,
    shapes: &HashMap<String, Shape<'_>>,
) -> Result<VerificationResult, ModelError> {
    let (passed, message) = match &requirement.rule {
        VerificationRule::ShapeValid { output } => {
            let passed = session.is_valid(shape(shapes, output)?)?;
            (
                passed,
                if passed {
                    "shape is valid"
                } else {
                    "shape is invalid"
                }
                .into(),
            )
        }
        VerificationRule::VolumeRange {
            output,
            minimum,
            maximum,
        } => {
            let volume = session.volume(shape(shapes, output)?)?;
            let minimum = minimum.cubic_millimeters()?;
            let maximum = maximum.cubic_millimeters()?;
            let passed = volume >= minimum && volume <= maximum;
            (
                passed,
                format!("volume {volume} mm^3; expected {minimum}..={maximum} mm^3"),
            )
        }
    };
    Ok(VerificationResult {
        requirement_id: requirement.id.clone(),
        status: if passed {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        message,
    })
}

fn instance_error(id: &str, error: ModelError) -> ModelError {
    ModelError::new(format!("instance '{id}': {}", error.message))
}

fn release_results<'session>(
    session: &Session,
    results: impl IntoIterator<Item = GeneratedResult<'session>>,
) {
    for result in results {
        cleanup(session, result.shapes);
    }
}

/// Duplicates every named shape so another instance can own and place it.
fn duplicate_result<'session>(
    session: &'session Session,
    result: &GeneratedResult<'session>,
) -> Result<GeneratedResult<'session>, ModelError> {
    let mut shapes = HashMap::new();
    for (name, shape) in &result.shapes {
        match session.duplicate(shape) {
            Ok(copy) => {
                shapes.insert(name.clone(), copy);
            }
            Err(error) => {
                cleanup(session, shapes);
                return Err(error.into());
            }
        }
    }
    Ok(GeneratedResult {
        shapes,
        feature_signatures: result.feature_signatures.clone(),
        verification: result.verification.clone(),
        regeneration: result.regeneration.clone(),
    })
}

fn cleanup(session: &Session, shapes: HashMap<String, Shape<'_>>) {
    for shape in shapes.into_values() {
        let _ = session.remove(shape);
    }
}

fn apply_placement<'session>(
    session: &'session Session,
    result: GeneratedResult<'session>,
    placement: Placement,
) -> Result<GeneratedResult<'session>, ModelError> {
    let normalized = match placement.normalized() {
        Ok(normalized) => normalized,
        Err(error) => {
            cleanup(session, result.shapes);
            return Err(error);
        }
    };
    if normalized.rotation.is_none() && normalized.translation_is_zero() {
        return Ok(result);
    }

    let GeneratedResult {
        shapes,
        feature_signatures,
        verification,
        regeneration,
    } = result;
    let mut placed = HashMap::new();
    for (name, source) in &shapes {
        match place_shape(session, source, &normalized) {
            Ok(shape) => {
                placed.insert(name.clone(), shape);
            }
            Err(error) => {
                cleanup(session, placed);
                cleanup(session, shapes);
                return Err(error);
            }
        }
    }
    cleanup(session, shapes);
    Ok(GeneratedResult {
        shapes: placed,
        feature_signatures,
        verification,
        regeneration,
    })
}

/// Rotates then translates one shape for a non-identity placement, releasing
/// the intermediate rotated handle.
fn place_shape<'session>(
    session: &'session Session,
    source: &Shape<'session>,
    placement: &NormalizedPlacement,
) -> Result<Shape<'session>, ModelError> {
    let rotated = match placement.rotation {
        Some((origin, axis, angle)) => Some(session.rotate(source, origin, axis, angle)?),
        None => None,
    };
    if placement.translation_is_zero() {
        return Ok(rotated.expect("rotation exists for non-identity placement"));
    }
    let translated = session.translate(rotated.as_ref().unwrap_or(source), placement.translation);
    if let Some(intermediate) = rotated {
        let _ = session.remove(intermediate);
    }
    Ok(translated?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn length_parameter(id: &str, default: f64) -> ParameterDefinition {
        ParameterDefinition {
            id: id.into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(default, LengthUnit::Millimeter)),
            minimum: Some(Quantity::length(1.0, LengthUnit::Millimeter)),
            maximum: None,
        }
    }

    fn integer_parameter(id: &str, default: i64) -> ParameterDefinition {
        ParameterDefinition {
            id: id.into(),
            parameter_type: ParameterType::Integer,
            default: ParameterValue::Integer(default),
            minimum: None,
            maximum: None,
        }
    }

    fn family(priority: RequirementPriority, maximum_volume: f64) -> FamilyDefinition {
        FamilyDefinition {
            id: "BlockFamily".into(),
            version: 1,
            parameters: vec![
                length_parameter("width", 10.0),
                length_parameter("depth", 20.0),
                length_parameter("height", 30.0),
            ],
            derived_parameters: Vec::new(),
            derived_vector_parameters: Vec::new(),
            constraints: Vec::new(),
            features: vec![
                FeatureDefinition {
                    id: "placed".into(),
                    operation: FeatureOperation::Translate {
                        input: "body".into(),
                        offset: VectorExpr::Literal(VectorQuantity::lengths(
                            1.0,
                            2.0,
                            3.0,
                            LengthUnit::Centimeter,
                        )),
                    },
                },
                FeatureDefinition {
                    id: "body".into(),
                    operation: FeatureOperation::Box {
                        origin: VectorExpr::Literal(VectorQuantity::lengths(
                            0.0,
                            0.0,
                            0.0,
                            LengthUnit::Millimeter,
                        )),
                        size: VectorExpr::Components {
                            x: ScalarExpr::Parameter("width".into()),
                            y: ScalarExpr::Parameter("depth".into()),
                            z: ScalarExpr::Parameter("height".into()),
                        },
                    },
                },
            ],
            datums: Vec::new(),
            requirements: vec![
                Requirement {
                    id: "block.valid".into(),
                    version: 1,
                    kind: RequirementKind::Validation,
                    priority: RequirementPriority::Required,
                    statement: "The placed block must be a valid BREP.".into(),
                    rule: VerificationRule::ShapeValid {
                        output: "placed".into(),
                    },
                    provenance: "test".into(),
                },
                Requirement {
                    id: "block.volume".into(),
                    version: 1,
                    kind: RequirementKind::Dimensional,
                    priority,
                    statement: "The block volume must remain in range.".into(),
                    rule: VerificationRule::VolumeRange {
                        output: "placed".into(),
                        minimum: Volume {
                            value: 5_000.0,
                            unit: LengthUnit::Millimeter,
                        },
                        maximum: Volume {
                            value: maximum_volume,
                            unit: LengthUnit::Millimeter,
                        },
                    },
                    provenance: "test".into(),
                },
            ],
        }
    }

    #[test]
    fn regenerates_out_of_order_features_with_units_and_named_results() {
        let definition = family(RequirementPriority::Required, 7_000.0);
        let instance = PartInstance {
            id: "block-01".into(),
            definition: &definition,
            overrides: HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(1.0, LengthUnit::Centimeter)),
            )]),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();

        let placed = result.shape("placed").unwrap();
        let bounds = session.bounds(placed).unwrap();
        assert!((bounds.min.x - 10.0).abs() < 1e-6);
        assert!((bounds.min.y - 20.0).abs() < 1e-6);
        assert!((bounds.min.z - 30.0).abs() < 1e-6);
        assert!(
            result
                .verification
                .iter()
                .all(|item| item.status == VerificationStatus::Passed)
        );
        assert_eq!(session.shape_count().unwrap(), 2);
    }

    #[test]
    fn sewing_and_multi_shell_solids_are_feature_graph_operations() {
        let millimeters =
            |x, y, z| VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter));
        let definition = FamilyDefinition {
            id: "VoidBlock".into(),
            version: 1,
            parameters: Vec::new(),
            derived_parameters: Vec::new(),
            derived_vector_parameters: Vec::new(),
            constraints: Vec::new(),
            features: vec![
                FeatureDefinition {
                    id: "void-solid".into(),
                    operation: FeatureOperation::MakeSolid {
                        shells: vec!["inner".into(), "outer".into()],
                    },
                },
                FeatureDefinition {
                    id: "outer-sewn".into(),
                    operation: FeatureOperation::Sew {
                        inputs: vec!["outer".into()],
                        tolerance: ScalarExpr::Literal(Quantity::length(
                            1.0e-6,
                            LengthUnit::Millimeter,
                        )),
                    },
                },
                FeatureDefinition {
                    id: "inner".into(),
                    operation: FeatureOperation::Box {
                        origin: millimeters(2.0, 2.0, 2.0),
                        size: millimeters(2.0, 2.0, 2.0),
                    },
                },
                FeatureDefinition {
                    id: "outer".into(),
                    operation: FeatureOperation::Box {
                        origin: millimeters(0.0, 0.0, 0.0),
                        size: millimeters(10.0, 10.0, 10.0),
                    },
                },
            ],
            datums: Vec::new(),
            requirements: vec![Requirement {
                id: "void.valid".into(),
                version: 1,
                kind: RequirementKind::Validation,
                priority: RequirementPriority::Required,
                statement: "The multi-shell result must be valid.".into(),
                rule: VerificationRule::ShapeValid {
                    output: "void-solid".into(),
                },
                provenance: "test".into(),
            }],
        };
        let instance = PartInstance {
            id: "void-block-01".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();

        assert!(result.shape("outer-sewn").is_some());
        let solid = result.shape("void-solid").unwrap();
        assert_eq!(session.subshape_count(solid, ShapeType::Shell).unwrap(), 2);
        assert!((session.volume(solid).unwrap() - 992.0).abs() < 1e-9);

        let document = ModelDocument::from_graph(&InstanceGraph::new(&definition));
        let json = document.to_json_pretty().unwrap();
        assert!(json.contains("\"make_solid\""));
        assert!(json.contains("\"sew\""));
        assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    }

    #[test]
    fn required_failure_rejects_generation_and_cleans_shapes() {
        let definition = family(RequirementPriority::Required, 5_500.0);
        let instance = PartInstance {
            id: "block-02".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();

        let error = instance.regenerate(&session).err().unwrap();
        assert!(error.message.contains("block.volume"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn preferred_failure_publishes_geometry_with_diagnostic() {
        let definition = family(RequirementPriority::Preferred, 5_500.0);
        let instance = PartInstance {
            id: "block-03".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();

        assert!(result.shape("placed").is_some());
        assert_eq!(result.verification[1].status, VerificationStatus::Failed);
        assert_eq!(session.shape_count().unwrap(), 2);
    }

    #[test]
    fn invalid_override_is_rejected_before_geometry_is_created() {
        let definition = family(RequirementPriority::Required, 7_000.0);
        let instance = PartInstance {
            id: "block-04".into(),
            definition: &definition,
            overrides: HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
            )]),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();

        assert!(instance.regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn dependency_cycle_is_reported_without_leaking_shapes() {
        let mut definition = family(RequirementPriority::Required, 7_000.0);
        definition.features = vec![
            FeatureDefinition {
                id: "a".into(),
                operation: FeatureOperation::Translate {
                    input: "b".into(),
                    offset: VectorExpr::Literal(VectorQuantity::lengths(
                        1.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                },
            },
            FeatureDefinition {
                id: "b".into(),
                operation: FeatureOperation::Translate {
                    input: "a".into(),
                    offset: VectorExpr::Literal(VectorQuantity::lengths(
                        1.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                },
            },
        ];
        definition.requirements.clear();
        let instance = PartInstance {
            id: "cycle".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();

        let error = instance.regenerate(&session).err().unwrap();
        assert!(error.message.contains("cycle"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn invalid_verification_target_cleans_generated_shapes() {
        let mut definition = family(RequirementPriority::Required, 7_000.0);
        definition.requirements[0].rule = VerificationRule::ShapeValid {
            output: "missing".into(),
        };
        let instance = PartInstance {
            id: "invalid-requirement".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();

        let error = instance.regenerate(&session).err().unwrap();
        assert!(error.message.contains("block.valid"));
        assert!(error.message.contains("missing"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn clone_graph_inherits_sparse_overrides_and_detaches_explicitly() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph
            .add_base(
                "source",
                HashMap::from([(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
                )]),
                "user",
            )
            .unwrap();
        graph
            .add_clone(
                "middle",
                "source",
                HashMap::from([(
                    "depth".into(),
                    ParameterValue::Scalar(Quantity::length(40.0, LengthUnit::Millimeter)),
                )]),
                "clone",
            )
            .unwrap();
        graph
            .add_clone(
                "leaf",
                "middle",
                HashMap::from([(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(15.0, LengthUnit::Millimeter)),
                )]),
                "clone",
            )
            .unwrap();

        graph
            .set_override(
                "source",
                "width",
                ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
            )
            .unwrap();
        let leaf = graph.resolve("leaf").unwrap();
        assert_eq!(
            leaf.overrides["width"],
            ParameterValue::Scalar(Quantity::length(15.0, LengthUnit::Millimeter))
        );
        assert_eq!(
            leaf.overrides["depth"],
            ParameterValue::Scalar(Quantity::length(40.0, LengthUnit::Millimeter))
        );

        graph.remove_override("leaf", "width").unwrap();
        assert_eq!(
            graph.resolve("leaf").unwrap().overrides["width"],
            ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter))
        );
        graph.detach("leaf").unwrap();
        graph
            .set_override(
                "source",
                "width",
                ParameterValue::Scalar(Quantity::length(20.0, LengthUnit::Millimeter)),
            )
            .unwrap();
        graph
            .set_override(
                "middle",
                "depth",
                ParameterValue::Scalar(Quantity::length(50.0, LengthUnit::Millimeter)),
            )
            .unwrap();
        let detached = graph.resolve("leaf").unwrap();
        assert!(matches!(
            graph.node("leaf"),
            Some(InstanceNode::Base { .. })
        ));
        assert_eq!(
            detached.overrides["width"],
            ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter))
        );
        assert_eq!(
            detached.overrides["depth"],
            ParameterValue::Scalar(Quantity::length(40.0, LengthUnit::Millimeter))
        );

        let session = Session::new().unwrap();
        let result = detached.regenerate(&session).unwrap();
        assert!((session.volume(result.shape("body").unwrap()).unwrap() - 14_400.0).abs() < 1e-6);
    }

    #[test]
    fn clone_graph_reports_inheritance_cycles() {
        let definition = family(RequirementPriority::Required, 7_000.0);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_clone("a", "b", HashMap::new(), "test").unwrap();
        graph.add_clone("b", "a", HashMap::new(), "test").unwrap();

        let error = graph.resolve("a").err().unwrap();
        assert!(error.message.contains("a -> b -> a"));
    }

    #[test]
    fn one_graph_regenerates_and_round_trips_multiple_families() {
        let mut primary = family(RequirementPriority::Required, 100_000.0);
        primary.requirements.clear();
        let mut secondary = primary.clone();
        secondary.id = "MarkedBlockFamily".into();
        secondary.features.push(FeatureDefinition {
            id: "marker".into(),
            operation: FeatureOperation::Cylinder {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    50.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
                height: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
            },
        });

        let mut graph = InstanceGraph::new(&primary);
        graph.add_family(&secondary).unwrap();
        graph.add_base("plain", HashMap::new(), "test").unwrap();
        graph
            .add_base_from_family("marked", "MarkedBlockFamily", HashMap::new(), "test")
            .unwrap();
        graph
            .add_clone("marked-copy", "marked", HashMap::new(), "test")
            .unwrap();

        assert_eq!(graph.resolve("plain").unwrap().definition.id, "BlockFamily");
        assert_eq!(
            graph.resolve("marked-copy").unwrap().definition.id,
            "MarkedBlockFamily"
        );

        let session = Session::new().unwrap();
        let generated = graph.regenerate_all(&session).unwrap();
        // Identical parameter maps do not cause distinct families to share a
        // feature graph result.
        assert_eq!(generated.generated_variants(), 2);
        assert!(generated.result("plain").unwrap().shape("marker").is_none());
        assert!(
            generated
                .result("marked")
                .unwrap()
                .shape("marker")
                .is_some()
        );
        assert!(
            generated
                .result("marked-copy")
                .unwrap()
                .shape("marker")
                .is_some()
        );
        assert_eq!(generated.shared_from("marked-copy"), Some("marked"));
        drop(generated);

        graph.detach("marked-copy").unwrap();
        assert!(matches!(
            graph.node("marked-copy"),
            Some(InstanceNode::Base {
                family: Some(family),
                ..
            }) if family == "MarkedBlockFamily"
        ));

        let document = ModelDocument::from_graph(&graph);
        assert_eq!(document.additional_families, [secondary.clone()]);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
        let loaded_graph = loaded.instance_graph().unwrap();
        assert_eq!(
            loaded_graph.resolve("marked-copy").unwrap().definition.id,
            "MarkedBlockFamily"
        );
    }

    #[test]
    fn multi_family_graphs_reject_unknown_and_duplicate_family_ids() {
        let mut primary = family(RequirementPriority::Required, 100_000.0);
        primary.requirements.clear();
        let mut graph = InstanceGraph::new(&primary);
        assert!(graph.add_family(&primary).is_err());
        assert!(
            graph
                .add_base_from_family("unknown", "MissingFamily", HashMap::new(), "test")
                .is_err()
        );
        assert!(graph.node("unknown").is_none());

        graph.add_base("plain", HashMap::new(), "test").unwrap();
        let document = ModelDocument::from_graph(&graph);
        let mut unknown = document.clone();
        if let InstanceNode::Base { family, .. } = &mut unknown.instances[0] {
            *family = Some("MissingFamily".into());
        }
        let error = unknown.to_json_pretty().unwrap_err();
        assert!(
            error.message.contains("unknown family definition"),
            "{error}"
        );

        let mut duplicate = document;
        duplicate.additional_families.push(primary.clone());
        let error = duplicate.to_json_pretty().unwrap_err();
        assert!(error.message.contains("family ids"), "{error}");
    }

    #[test]
    fn managed_regeneration_retains_stale_result_then_replaces_it() {
        let definition = family(RequirementPriority::Required, 7_000.0);
        let instance = PartInstance {
            id: "managed".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        {
            let mut managed = ManagedPartInstance::new(&session, instance);
            managed.regenerate().unwrap();
            assert_eq!(managed.state(), RegenerationState::Current);
            assert_eq!(managed.attempted_revision(), 1);
            assert_eq!(managed.accepted_revision(), Some(1));
            assert_eq!(session.shape_count().unwrap(), 2);

            managed.instance_mut().overrides.insert(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
            );
            assert!(managed.regenerate().is_err());
            assert_eq!(managed.state(), RegenerationState::Stale);
            assert_eq!(managed.attempted_revision(), 2);
            assert_eq!(managed.accepted_revision(), Some(1));
            assert!(managed.accepted().is_some());
            assert!(managed.last_error().is_some());
            assert_eq!(session.shape_count().unwrap(), 2);

            managed.instance_mut().overrides.insert(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(11.0, LengthUnit::Millimeter)),
            );
            managed.regenerate().unwrap();
            assert_eq!(managed.state(), RegenerationState::Current);
            assert_eq!(managed.attempted_revision(), 3);
            assert_eq!(managed.accepted_revision(), Some(3));
            assert!(managed.last_error().is_none());
            assert_eq!(session.shape_count().unwrap(), 2);
            assert!(
                (session
                    .volume(managed.accepted().unwrap().shape("body").unwrap())
                    .unwrap()
                    - 6_600.0)
                    .abs()
                    < 1e-6
            );
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn managed_initial_failure_has_no_accepted_result() {
        let definition = family(RequirementPriority::Required, 7_000.0);
        let instance = PartInstance {
            id: "managed-failure".into(),
            definition: &definition,
            overrides: HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
            )]),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let mut managed = ManagedPartInstance::new(&session, instance);

        assert!(managed.regenerate().is_err());
        assert_eq!(managed.state(), RegenerationState::Failed);
        assert_eq!(managed.accepted_revision(), None);
        assert!(managed.accepted().is_none());
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn placed_instance_rotates_then_translates_every_named_output() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph
            .add_base("placed-instance", HashMap::new(), "test")
            .unwrap();
        graph
            .set_placement(
                "placed-instance",
                Placement {
                    translation: VectorQuantity::lengths(100.0, 0.0, 0.0, LengthUnit::Millimeter),
                    rotation: Some(AxisAngle {
                        origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                        axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                        angle_radians: std::f64::consts::FRAC_PI_2,
                    }),
                },
            )
            .unwrap();
        let session = Session::new().unwrap();
        let result = graph
            .resolve_with_placement("placed-instance")
            .unwrap()
            .regenerate(&session)
            .unwrap();

        let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
        assert!((bounds.min.x - 80.0).abs() < 1e-6);
        assert!((bounds.max.x - 100.0).abs() < 1e-6);
        assert!(bounds.min.y.abs() < 1e-6);
        assert!((bounds.max.y - 10.0).abs() < 1e-6);
        assert_eq!(session.shape_count().unwrap(), 2);
    }

    #[test]
    fn linear_pattern_creates_linked_clones_with_independent_placements() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        let members = graph
            .add_linear_pattern(
                "row",
                "member",
                "source",
                3,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "pattern",
            )
            .unwrap();

        assert_eq!(members, ["member[0]", "member[1]", "member[2]"]);
        assert_eq!(graph.patterns().len(), 1);
        assert_eq!(graph.patterns()[0].source, "source");
        assert!(matches!(
            graph.node("member[2]"),
            Some(InstanceNode::Clone { source, .. }) if source == "source"
        ));

        graph
            .set_override(
                "source",
                "width",
                ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
            )
            .unwrap();
        let resolved = graph.resolve_with_placement("member[2]").unwrap();
        assert_eq!(
            resolved.instance.overrides["width"],
            ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter))
        );
        let session = Session::new().unwrap();
        let result = resolved.regenerate(&session).unwrap();
        let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
        assert!((bounds.min.x - 100.0).abs() < 1e-6);
        assert!((bounds.max.x - 112.0).abs() < 1e-6);
        assert_eq!(session.shape_count().unwrap(), 2);
    }

    #[test]
    fn circular_pattern_rotates_linked_clones_about_a_typed_axis() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        let members = graph
            .add_pattern(
                "ring",
                "spoke",
                "source",
                4,
                PatternRule::Circular {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_step_radians: std::f64::consts::FRAC_PI_2,
                },
                "pattern",
            )
            .unwrap();

        assert_eq!(members.len(), 4);
        assert!(matches!(
            graph.patterns()[0].rule,
            PatternRule::Circular { angle_step_radians, .. }
                if angle_step_radians == std::f64::consts::FRAC_PI_2
        ));
        let session = Session::new().unwrap();
        let quarter = graph.resolve_with_placement("spoke[1]").unwrap();
        let result = quarter.regenerate(&session).unwrap();
        let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
        assert!((bounds.min.x + 20.0).abs() < 1e-6);
        assert!(bounds.max.x.abs() < 1e-6);
        assert!(bounds.min.y.abs() < 1e-6);
        assert!((bounds.max.y - 10.0).abs() < 1e-6);
        drop(result);

        let half = graph.resolve_with_placement("spoke[2]").unwrap();
        let result = half.regenerate(&session).unwrap();
        let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
        assert!((bounds.min.x + 10.0).abs() < 1e-6);
        assert!((bounds.min.y + 20.0).abs() < 1e-6);

        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
    }

    fn quarter_turn_about_z() -> Placement {
        Placement {
            translation: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            rotation: Some(AxisAngle {
                origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                angle_radians: std::f64::consts::FRAC_PI_2,
            }),
        }
    }

    /// Regenerates in a fresh session so the shape count proves that every
    /// intermediate placement shape was released.
    fn body_bounds(graph: &InstanceGraph<'_>, id: &str) -> occt_bridge::Bounds {
        let session = Session::new().unwrap();
        let result = graph
            .resolve_with_placement(id)
            .unwrap()
            .regenerate(&session)
            .unwrap();
        let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
        assert_eq!(session.shape_count().unwrap(), 2);
        bounds
    }

    fn assert_bounds(bounds: occt_bridge::Bounds, min: (f64, f64), max: (f64, f64)) {
        assert!((bounds.min.x - min.0).abs() < 1e-6, "{bounds:?}");
        assert!((bounds.min.y - min.1).abs() < 1e-6, "{bounds:?}");
        assert!((bounds.max.x - max.0).abs() < 1e-6, "{bounds:?}");
        assert!((bounds.max.y - max.1).abs() < 1e-6, "{bounds:?}");
    }

    #[test]
    fn nested_assembly_frames_compose_placements_and_move_their_contents() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph
            .add_frame(
                "building",
                None,
                Placement::translated(VectorQuantity::lengths(1.0, 0.0, 0.0, LengthUnit::Meter)),
                "layout",
            )
            .unwrap();
        graph
            .add_frame("row", Some("building"), quarter_turn_about_z(), "layout")
            .unwrap();
        assert!(
            graph
                .add_frame("orphan", Some("missing"), Placement::identity(), "layout")
                .unwrap_err()
                .message
                .contains("unknown assembly frame 'missing'")
        );
        graph.add_base("source", HashMap::new(), "test").unwrap();
        graph
            .add_clone("pew", "source", HashMap::new(), "test")
            .unwrap();
        graph
            .set_placement(
                "pew",
                Placement::translated(VectorQuantity::lengths(
                    50.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            )
            .unwrap();
        graph.set_instance_frame("pew", Some("row")).unwrap();

        // Local x 50..60 turns into y 50..60, then the building shifts x by 1 m.
        assert_bounds(body_bounds(&graph, "pew"), (980.0, 50.0), (1000.0, 60.0));
        assert_bounds(body_bounds(&graph, "source"), (0.0, 0.0), (10.0, 20.0));

        graph
            .set_frame_placement(
                "building",
                Placement::translated(VectorQuantity::lengths(
                    0.0,
                    500.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            )
            .unwrap();
        assert_bounds(body_bounds(&graph, "pew"), (-20.0, 550.0), (0.0, 560.0));

        graph.detach("pew").unwrap();
        assert_eq!(graph.node("pew").unwrap().frame(), Some("row"));
        graph.set_instance_frame("pew", None).unwrap();
        assert_bounds(body_bounds(&graph, "pew"), (50.0, 0.0), (60.0, 20.0));
    }

    #[test]
    fn patterns_follow_their_assembly_frame() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph
            .add_frame(
                "flange",
                None,
                Placement::translated(VectorQuantity::lengths(
                    100.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                "layout",
            )
            .unwrap();
        graph.add_base("source", HashMap::new(), "test").unwrap();
        graph
            .add_pattern(
                "bolts",
                "bolt",
                "source",
                4,
                PatternRule::Circular {
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_step_radians: std::f64::consts::FRAC_PI_2,
                },
                "pattern",
            )
            .unwrap();
        graph.set_pattern_frame("bolts", Some("flange")).unwrap();
        assert_eq!(graph.patterns()[0].frame.as_deref(), Some("flange"));
        assert_eq!(graph.node("bolt[3]").unwrap().frame(), Some("flange"));
        let error = graph.set_instance_frame("bolt[1]", None).unwrap_err();
        assert!(error.message.contains("set the pattern frame instead"));
        assert!(graph.set_pattern_frame("bolts", Some("missing")).is_err());

        assert_bounds(body_bounds(&graph, "bolt[1]"), (80.0, 0.0), (100.0, 10.0));

        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
        assert_bounds(
            body_bounds(&loaded.instance_graph().unwrap(), "bolt[1]"),
            (80.0, 0.0),
            (100.0, 10.0),
        );
    }

    #[test]
    fn documents_reject_invalid_assembly_frames() {
        let definition = family(RequirementPriority::Required, 100_000.0);
        let mut graph = InstanceGraph::new(&definition);
        graph
            .add_frame("a", None, Placement::identity(), "layout")
            .unwrap();
        graph
            .add_frame("b", Some("a"), Placement::identity(), "layout")
            .unwrap();
        graph.add_base("source", HashMap::new(), "test").unwrap();
        graph
            .add_linear_pattern(
                "row",
                "member",
                "source",
                2,
                VectorQuantity::lengths(5.0, 0.0, 0.0, LengthUnit::Millimeter),
                "pattern",
            )
            .unwrap();
        graph.set_pattern_frame("row", Some("b")).unwrap();
        let document = ModelDocument::from_graph(&graph);
        document.to_json_pretty().unwrap();

        let mut cyclic = document.clone();
        cyclic.frames[0].parent = Some("b".into());
        let error = cyclic.to_json_pretty().unwrap_err();
        assert!(error.message.contains("assembly frame cycle: a -> b -> a"));

        let mut unknown = document.clone();
        *unknown
            .instances
            .iter_mut()
            .find(|node| node.id() == "source")
            .unwrap()
            .frame_mut() = Some("missing".into());
        let error = unknown.to_json_pretty().unwrap_err();
        assert!(error.message.contains("instance 'source'"));
        assert!(error.message.contains("unknown assembly frame 'missing'"));

        let mut mismatched = document.clone();
        *mismatched
            .instances
            .iter_mut()
            .find(|node| node.id() == "member[1]")
            .unwrap()
            .frame_mut() = Some("a".into());
        let error = mismatched.to_json_pretty().unwrap_err();
        assert!(error.message.contains("in the pattern frame"));

        let mut duplicate = document;
        duplicate.frames.push(duplicate.frames[0].clone());
        assert!(duplicate.to_json_pretty().is_err());
    }

    #[test]
    fn clones_differing_only_in_placement_share_one_generation() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph
            .add_frame(
                "row",
                None,
                Placement::translated(VectorQuantity::lengths(
                    0.0,
                    100.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                "layout",
            )
            .unwrap();
        graph.add_base("source", HashMap::new(), "test").unwrap();
        graph
            .add_linear_pattern(
                "pews",
                "pew",
                "source",
                3,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "pattern",
            )
            .unwrap();
        graph.set_pattern_frame("pews", Some("row")).unwrap();
        graph
            .add_clone(
                "wide",
                "source",
                HashMap::from([(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
                )]),
                "test",
            )
            .unwrap();
        graph
            .add_clone(
                "explicit_default",
                "source",
                HashMap::from([(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
                )]),
                "test",
            )
            .unwrap();

        let session = Session::new().unwrap();
        let generation = graph.regenerate_all(&session).unwrap();
        assert_eq!(generation.generated_variants(), 2);
        assert_eq!(session.shape_count().unwrap(), 6 * 2);
        for id in ["explicit_default", "pew[0]", "pew[1]", "pew[2]", "source"] {
            assert_eq!(generation.shared_from(id), Some("explicit_default"), "{id}");
        }
        assert_eq!(generation.shared_from("wide"), Some("wide"));
        assert!(
            !generation
                .result("explicit_default")
                .unwrap()
                .regeneration
                .rebuilt
                .is_empty()
        );
        let shared = &generation.result("pew[2]").unwrap().regeneration;
        assert!(shared.rebuilt.is_empty());
        assert_eq!(shared.reused.len(), definition.features.len());

        let bounds = |id: &str| {
            session
                .bounds(generation.result(id).unwrap().shape("body").unwrap())
                .unwrap()
        };
        assert_bounds(bounds("pew[2]"), (100.0, 100.0), (110.0, 120.0));
        assert_bounds(bounds("pew[0]"), (0.0, 100.0), (10.0, 120.0));
        assert_bounds(bounds("source"), (0.0, 0.0), (10.0, 20.0));
        assert_bounds(bounds("wide"), (0.0, 0.0), (12.0, 20.0));

        let pews = graph
            .regenerate_instances(&session, &["pew[1]", "pew[0]"])
            .unwrap();
        assert_eq!(pews.generated_variants(), 1);
        assert_eq!(pews.shared_from("pew[0]"), Some("pew[1]"));
        assert!(
            graph
                .regenerate_instances(&session, &["pew[0]", "pew[0]"])
                .is_err()
        );
    }

    #[test]
    fn shared_regeneration_failure_releases_every_created_handle() {
        let definition = family(RequirementPriority::Required, 7_000.0);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        graph
            .add_clone("copy", "source", HashMap::new(), "test")
            .unwrap();
        graph
            .add_clone(
                "too_wide",
                "source",
                HashMap::from([(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
                )]),
                "test",
            )
            .unwrap();

        let session = Session::new().unwrap();
        let error = graph
            .regenerate_instances(&session, &["source", "copy", "too_wide"])
            .err()
            .unwrap();
        assert!(error.message.contains("instance 'too_wide'"), "{error}");
        assert_eq!(session.shape_count().unwrap(), 0);
        assert!(
            graph
                .regenerate_instances(&session, &["missing"])
                .err()
                .unwrap()
                .message
                .contains("instance 'missing'")
        );
    }

    #[test]
    fn curvature_bounds_classify_only_proven_decisions() {
        let bounds =
            |minimum_lower_bound, minimum, maximum, maximum_upper_bound| CurvatureExtrema {
                minimum,
                minimum_lower_bound,
                maximum,
                maximum_upper_bound,
                is_exact: false,
            };
        // Curvature range [0.4, 0.6] is radius range [1.667, 2.5].
        let inside = bounds(0.45, 0.46, 0.54, 0.55);
        assert_eq!(
            classify_curvature_bounds(&inside, 0.4, 0.6, true),
            Some(true)
        );
        assert_eq!(
            classify_curvature_bounds(&inside, 0.4, 0.6, false),
            Some(true)
        );
        let straddling_top = bounds(0.45, 0.46, 0.59, 0.61);
        assert_eq!(
            classify_curvature_bounds(&straddling_top, 0.4, 0.6, true),
            None
        );
        assert_eq!(
            classify_curvature_bounds(&straddling_top, 0.4, 0.6, false),
            Some(true)
        );
        let exceeds = bounds(0.45, 0.46, 0.61, 0.62);
        assert_eq!(
            classify_curvature_bounds(&exceeds, 0.4, 0.6, true),
            Some(false)
        );
        let near_below = bounds(0.35, 0.36, 0.39, 0.41);
        assert_eq!(
            classify_curvature_bounds(&near_below, 0.4, 0.6, false),
            None
        );
        let below = bounds(0.35, 0.36, 0.38, 0.39);
        assert_eq!(
            classify_curvature_bounds(&below, 0.4, 0.6, false),
            Some(false)
        );
        let straight = bounds(0.0, 0.0, 0.0, 0.0);
        assert_eq!(
            classify_curvature_bounds(&straight, 0.4, 0.6, false),
            Some(false)
        );
    }

    #[test]
    fn bounded_curvature_selector_matches_exact_and_spline_edges() {
        let session = Session::new().unwrap();
        let cylinder = session
            .create_cylinder(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 2.0, 5.0)
            .unwrap();
        let rims =
            select_edges_by_bounded_curvature_radius(&session, &cylinder, 1.9, 2.1, 1e-9, true)
                .unwrap();
        assert_eq!(rims.len(), 2);
        cleanup_shapes(&session, rims);

        let compound = session
            .load_brep(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/curvature_edges.brep"
            ))
            .unwrap();
        let before = session.shape_count().unwrap();
        let arcs =
            select_edges_by_bounded_curvature_radius(&session, &compound, 1.9, 2.1, 1e-9, true)
                .unwrap();
        let rational_circle = session.subshape(&compound, ShapeType::Edge, 1).unwrap();
        assert_eq!(arcs.len(), 1);
        assert!(session.is_same(&arcs[0], &rational_circle).unwrap());
        cleanup_shapes(&session, arcs);
        let _ = session.remove(rational_circle);

        // The parabola spans radius 0.5..5.59, so it overlaps but is not contained.
        let overlapping =
            select_edges_by_bounded_curvature_radius(&session, &compound, 4.0, 4.5, 1e-9, false)
                .unwrap();
        let parabola = session.subshape(&compound, ShapeType::Edge, 0).unwrap();
        assert!(
            overlapping
                .iter()
                .any(|edge| session.is_same(edge, &parabola).unwrap())
        );
        cleanup_shapes(&session, overlapping);

        let _ = session.remove(parabola);

        // Put the lower radius bound strictly inside the spline's loose
        // maximum-curvature gap: undecidable at 0.5, decided at 1e-6.
        let spline = session.subshape(&compound, ShapeType::Edge, 2).unwrap();
        let loose = session.edge_curvature_extrema(&spline, 0.5).unwrap();
        let tight = session.edge_curvature_extrema(&spline, 1e-6).unwrap();
        let _ = session.remove(spline);
        let boundary_curvature = 0.5 * (loose.maximum + loose.maximum_upper_bound);
        assert!(
            loose.maximum < boundary_curvature && boundary_curvature < loose.maximum_upper_bound
        );
        assert!(tight.maximum_upper_bound < boundary_curvature);
        let boundary = 1.0 / boundary_curvature;
        let error = select_edges_by_bounded_curvature_radius(
            &session, &compound, boundary, 100.0, 0.5, true,
        )
        .unwrap_err();
        assert!(error.message.contains("edge 2"), "{error}");
        assert!(error.message.contains("tighten relative_tolerance"));
        assert_eq!(session.shape_count().unwrap(), before);
        // Rational circle, spline, ellipse arc, and the conic parabola
        // (radius 1..11.2) and hyperbola (radius 1.33..17.1); not the Bezier
        // parabola (radius 0.5 at its vertex) or the line.
        let resolved = select_edges_by_bounded_curvature_radius(
            &session, &compound, boundary, 100.0, 1e-6, true,
        )
        .unwrap();
        assert_eq!(resolved.len(), 5);
        cleanup_shapes(&session, resolved);
        assert_eq!(session.shape_count().unwrap(), before);

        for (minimum, maximum, tolerance) in [
            (0.0, 1.0, 1e-6),
            (2.0, 1.0, 1e-6),
            (1.0, 2.0, 0.0),
            (1.0, 2.0, 2.0),
        ] {
            assert!(
                select_edges_by_bounded_curvature_radius(
                    &session, &compound, minimum, maximum, tolerance, true
                )
                .is_err()
            );
        }
    }

    #[test]
    fn detaching_a_pattern_member_removes_it_from_the_pattern() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph
            .add_frame("row", None, Placement::identity(), "layout")
            .unwrap();
        graph.add_base("source", HashMap::new(), "test").unwrap();
        graph
            .add_linear_pattern(
                "pews",
                "pew",
                "source",
                2,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "pattern",
            )
            .unwrap();
        graph.set_pattern_frame("pews", Some("row")).unwrap();

        graph.detach("pew[1]").unwrap();
        assert_eq!(
            graph.patterns()[0].member_ids().collect::<Vec<_>>(),
            ["pew[0]"]
        );
        assert!(matches!(
            graph.node("pew[1]"),
            Some(InstanceNode::Base { .. })
        ));
        assert_eq!(graph.node("pew[1]").unwrap().frame(), Some("row"));
        // Now independent, so it may leave the pattern frame on its own.
        graph.set_instance_frame("pew[1]", None).unwrap();
        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);

        graph.detach("pew[0]").unwrap();
        assert!(graph.patterns().is_empty());
        ModelDocument::from_graph(&graph).to_json_pretty().unwrap();
    }

    fn pew_row(definition: &FamilyDefinition) -> InstanceGraph<'_> {
        let mut graph = InstanceGraph::new(definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        graph
            .add_linear_pattern(
                "pews",
                "pew",
                "source",
                3,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "pattern",
            )
            .unwrap();
        graph
    }

    fn along_x(millimeters: f64) -> Placement {
        Placement::translated(VectorQuantity::lengths(
            millimeters,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        ))
    }

    fn step_x(millimeters: f64) -> PatternRule {
        PatternRule::Linear {
            step: VectorQuantity::lengths(millimeters, 0.0, 0.0, LengthUnit::Millimeter),
        }
    }

    #[test]
    fn rule_edits_move_members_except_placement_overrides() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = pew_row(&definition);
        let placement = |graph: &InstanceGraph<'_>, id: &str| graph.node(id).unwrap().placement();

        graph.set_placement("pew[1]", along_x(500.0)).unwrap();
        assert_eq!(
            graph.patterns()[0]
                .member("pew[1]")
                .unwrap()
                .placement_override,
            Some(along_x(500.0))
        );
        graph.set_pattern_rule("pews", step_x(100.0)).unwrap();
        assert_eq!(placement(&graph, "pew[0]"), along_x(0.0));
        assert_eq!(placement(&graph, "pew[1]"), along_x(500.0));
        assert_eq!(placement(&graph, "pew[2]"), along_x(200.0));

        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);

        assert_eq!(
            graph.clear_placement_override("pew[1]").unwrap(),
            Some(along_x(500.0))
        );
        assert_eq!(placement(&graph, "pew[1]"), along_x(100.0));
        assert_eq!(graph.clear_placement_override("pew[1]").unwrap(), None);
        assert!(graph.clear_placement_override("source").is_err());
        assert!(graph.set_pattern_rule("missing", step_x(1.0)).is_err());
        assert!(
            graph
                .set_pattern_rule(
                    "pews",
                    PatternRule::Circular {
                        origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                        axis: VectorQuantity::scalars(0.0, 0.0, 0.0),
                        angle_step_radians: 1.0,
                    },
                )
                .is_err()
        );
        assert_eq!(placement(&graph, "pew[2]"), along_x(200.0));

        // Moving the base instance is not a pattern override.
        graph.set_placement("source", along_x(7.0)).unwrap();
        assert!(
            graph.patterns()[0]
                .members
                .iter()
                .all(|member| member.placement_override.is_none())
        );
    }

    #[test]
    fn member_slots_survive_detaching_a_neighbor() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = pew_row(&definition);
        graph.detach("pew[1]").unwrap();
        graph.set_pattern_rule("pews", step_x(100.0)).unwrap();
        assert_eq!(graph.node("pew[2]").unwrap().placement(), along_x(200.0));
        assert_eq!(graph.patterns()[0].member("pew[2]").unwrap().index, 2);
        // The detached instance keeps its last placement.
        assert_eq!(graph.node("pew[1]").unwrap().placement(), along_x(50.0));
        ModelDocument::from_graph(&graph).to_json_pretty().unwrap();
    }

    #[test]
    fn suppressed_members_stay_linked_but_skip_regeneration() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = pew_row(&definition);
        graph.set_member_suppressed("pew[1]", true).unwrap();
        assert!(graph.is_suppressed("pew[1]"));
        assert!(!graph.is_suppressed("pew[0]"));
        assert!(graph.set_member_suppressed("source", true).is_err());

        let session = Session::new().unwrap();
        let generation = graph.regenerate_all(&session).unwrap();
        assert!(generation.result("pew[1]").is_none());
        assert!(generation.result("pew[2]").is_some());
        assert_eq!(generation.shared_from("pew[2]"), Some("pew[0]"));
        drop(generation);
        let error = graph
            .regenerate_instances(&session, &["pew[1]"])
            .err()
            .unwrap();
        assert!(error.message.contains("suppressed"), "{error}");

        // Inheritance still reaches a suppressed member.
        graph
            .set_override(
                "source",
                "width",
                ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
            )
            .unwrap();
        assert_eq!(
            graph.resolve("pew[1]").unwrap().overrides["width"],
            ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter))
        );

        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
        assert!(loaded.instance_graph().unwrap().is_suppressed("pew[1]"));

        graph.set_member_suppressed("pew[1]", false).unwrap();
        let generation = graph.regenerate_all(&session).unwrap();
        assert!(generation.result("pew[1]").is_some());
    }

    #[test]
    fn schema_sixteen_member_placements_become_overrides() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = pew_row(&definition);
        graph.set_placement("pew[2]", along_x(900.0)).unwrap();
        let current = ModelDocument::from_graph(&graph);

        let mut legacy = serde_json::to_value(&current).unwrap();
        legacy["schema_version"] = serde_json::json!(16);
        legacy["patterns"][0]["members"] = serde_json::json!(["pew[0]", "pew[1]", "pew[2]"]);
        let migrated = ModelDocument::from_json(&legacy.to_string()).unwrap();
        assert_eq!(migrated, current);
        let members = &migrated.patterns[0].members;
        assert_eq!(members[1].index, 1);
        assert_eq!(members[1].placement_override, None);
        assert_eq!(members[2].placement_override, Some(along_x(900.0)));
    }

    #[test]
    fn documents_reject_inconsistent_member_slots() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let graph = pew_row(&definition);
        let document = ModelDocument::from_graph(&graph);

        let mut moved = document.clone();
        let node = moved
            .instances
            .iter_mut()
            .find(|node| node.id() == "pew[1]")
            .unwrap();
        if let InstanceNode::Clone { placement, .. } = node {
            *placement = along_x(3.0);
        }
        let error = moved.to_json_pretty().unwrap_err();
        assert!(error.message.contains("does not match"), "{error}");

        let mut duplicated = document;
        duplicated.patterns[0].members[2].index = 0;
        let error = duplicated.to_json_pretty().unwrap_err();
        assert!(error.message.contains("slot 0"), "{error}");
    }

    fn member_ids(graph: &InstanceGraph<'_>) -> Vec<String> {
        let mut ids = graph.patterns()[0]
            .member_ids()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    #[test]
    fn pattern_count_edits_add_and_remove_rule_slots() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = pew_row(&definition);
        graph
            .add_frame("aisle", None, along_x(1000.0), "layout")
            .unwrap();
        graph.set_pattern_frame("pews", Some("aisle")).unwrap();

        graph.set_pattern_count("pews", 5).unwrap();
        assert_eq!(graph.patterns()[0].slot_count, 5);
        assert_eq!(graph.node("pew[4]").unwrap().placement(), along_x(200.0));
        assert_eq!(graph.node("pew[4]").unwrap().frame(), Some("aisle"));
        assert!(matches!(
            graph.node("pew[3]"),
            Some(InstanceNode::Clone { source, .. }) if source == "source"
        ));

        graph.set_pattern_count("pews", 2).unwrap();
        assert_eq!(member_ids(&graph), ["pew[0]", "pew[1]"]);
        assert!(graph.node("pew[2]").is_none() && graph.node("pew[4]").is_none());

        // A detached slot stays empty when the pattern grows again.
        graph.detach("pew[1]").unwrap();
        graph.set_pattern_count("pews", 3).unwrap();
        assert_eq!(member_ids(&graph), ["pew[0]", "pew[2]"]);
        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);

        graph
            .add_clone("fan", "pew[2]", HashMap::new(), "test")
            .unwrap();
        let error = graph.set_pattern_count("pews", 1).unwrap_err();
        assert!(error.message.contains("'fan' is cloned from it"), "{error}");
        graph.add_base("pew[3]", HashMap::new(), "test").unwrap();
        let error = graph.set_pattern_count("pews", 4).unwrap_err();
        assert!(error.message.contains("'pew[3]' already exists"), "{error}");
        assert!(graph.set_pattern_count("pews", 0).is_err());
        assert!(graph.set_pattern_count("missing", 2).is_err());
        assert_eq!(graph.patterns()[0].slot_count, 3);
    }

    fn linear_fit(span_millimeters: f64, spacing: LinearSpacing) -> PatternRule {
        PatternRule::LinearFit {
            span: VectorQuantity::lengths(span_millimeters, 0.0, 0.0, LengthUnit::Millimeter),
            spacing,
        }
    }

    #[test]
    fn linear_fit_solves_member_count_from_spacing_constraints() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        let minimum = LinearSpacing::Minimum(Quantity::length(900.0, LengthUnit::Millimeter));
        let members = graph
            .add_fitted_pattern(
                "pews",
                "pew",
                "source",
                linear_fit(10_000.0, minimum),
                "fit",
            )
            .unwrap();
        // floor(10000 / 900) = 11 gaps of 909.09 mm.
        assert_eq!(members.len(), 12);
        let last = graph.node("pew[11]").unwrap().placement();
        assert_eq!(last, along_x(10_000.0));
        let gap = graph
            .node("pew[1]")
            .unwrap()
            .placement()
            .translation
            .x
            .value;
        assert!((gap - 10_000.0 / 11.0).abs() < 1e-9);

        // A 9 m span at 3 m maximum spacing is exactly 3 gaps, not 4.
        let maximum = LinearSpacing::Maximum(Quantity::length(3.0, LengthUnit::Meter));
        graph
            .set_pattern_rule("pews", linear_fit(9_000.0, maximum))
            .unwrap();
        assert_eq!(member_ids(&graph).len(), 4);
        assert_eq!(graph.node("pew[3]").unwrap().placement(), along_x(9_000.0));
        assert!(graph.node("pew[4]").is_none());

        graph
            .set_pattern_rule("pews", linear_fit(9_000.0, LinearSpacing::Count(1)))
            .unwrap();
        assert_eq!(member_ids(&graph), ["pew[0]"]);
        let error = graph.set_pattern_count("pews", 3).unwrap_err();
        assert!(error.message.contains("driven by its constraints"));

        let too_many = LinearSpacing::Minimum(Quantity::length(0.01, LengthUnit::Millimeter));
        assert!(
            graph
                .set_pattern_rule("pews", linear_fit(10_000.0, too_many))
                .is_err()
        );
        let wrong_unit = LinearSpacing::Minimum(Quantity::scalar(1.0));
        assert!(
            graph
                .set_pattern_rule("pews", linear_fit(1_000.0, wrong_unit))
                .is_err()
        );
        assert!(
            graph
                .set_pattern_rule("pews", linear_fit(0.0, LinearSpacing::Count(2)))
                .is_err()
        );
        assert!(
            graph
                .add_pattern(
                    "free",
                    "free",
                    "source",
                    2,
                    linear_fit(10.0, LinearSpacing::Count(2)),
                    "x"
                )
                .is_err()
        );
        assert!(
            graph
                .add_fitted_pattern("free", "free", "source", step_x(10.0), "x")
                .is_err()
        );

        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
    }

    #[test]
    fn integer_parameters_drive_pattern_counts() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition
            .parameters
            .push(integer_parameter("bolt_count", 5));
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        graph
            .add_linear_pattern(
                "bolts",
                "bolt",
                "source",
                2,
                VectorQuantity::lengths(25.0, 0.0, 0.0, LengthUnit::Millimeter),
                "parameter-driven",
            )
            .unwrap();
        graph
            .set_pattern_count_driver(
                "bolts",
                Some(PatternCountDriver::Parameter {
                    instance: "source".into(),
                    parameter: "bolt_count".into(),
                }),
            )
            .unwrap();

        let session = Session::new().unwrap();
        let generated = graph.regenerate_all(&session).unwrap();
        assert_eq!(graph.patterns()[0].slot_count, 5);
        assert!(generated.result("bolt[4]").is_some());
        assert_eq!(graph.node("bolt[4]").unwrap().placement(), along_x(100.0));
        drop(generated);

        graph
            .set_override("source", "bolt_count", ParameterValue::Integer(3))
            .unwrap();
        let generated = graph.regenerate_all(&session).unwrap();
        assert_eq!(graph.patterns()[0].slot_count, 3);
        assert!(generated.result("bolt[2]").is_some());
        assert!(graph.node("bolt[4]").is_none());
        drop(generated);

        assert!(graph.set_pattern_count("bolts", 7).is_err());
        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);

        graph
            .set_override(
                "source",
                "width",
                ParameterValue::Scalar(Quantity::length(110.0, LengthUnit::Millimeter)),
            )
            .unwrap();
        graph
            .set_pattern_count_driver(
                "bolts",
                Some(PatternCountDriver::BoundsExtent {
                    instance: "source".into(),
                    output: "body".into(),
                    axis: CoordinateAxis::X,
                    maximum_spacing: Quantity::length(30.0, LengthUnit::Millimeter),
                }),
            )
            .unwrap();
        let measurement_session = Session::new().unwrap();
        graph.refresh_driven_patterns(&measurement_session).unwrap();
        assert_eq!(graph.patterns()[0].slot_count, 5);
        assert_eq!(measurement_session.shape_count().unwrap(), 0);
        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
    }

    #[test]
    fn measured_drivers_use_exact_extents_of_the_measured_family() {
        let mut primary = family(RequirementPriority::Required, 100_000.0);
        primary.requirements.clear();
        let mut secondary = primary.clone();
        secondary.id = "MarkedBlockFamily".into();
        secondary.features.push(FeatureDefinition {
            id: "marker".into(),
            operation: FeatureOperation::Cylinder {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    50.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
                height: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
            },
        });
        let mut graph = InstanceGraph::new(&primary);
        graph.add_family(&secondary).unwrap();
        graph
            .add_base(
                "source",
                HashMap::from([(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(120.0, LengthUnit::Millimeter)),
                )]),
                "test",
            )
            .unwrap();
        graph
            .add_base_from_family("marked", "MarkedBlockFamily", HashMap::new(), "test")
            .unwrap();
        graph
            .add_linear_pattern(
                "bolts",
                "bolt",
                "source",
                2,
                VectorQuantity::lengths(10.0, 0.0, 0.0, LengthUnit::Millimeter),
                "test",
            )
            .unwrap();
        let session = Session::new().unwrap();

        // 120 mm at 30 mm maximum spacing is exactly 4 gaps. Tolerance-padded
        // bounds measured 120.0000002 mm and produced a sixth member.
        graph
            .set_pattern_count_driver(
                "bolts",
                Some(PatternCountDriver::BoundsExtent {
                    instance: "source".into(),
                    output: "body".into(),
                    axis: CoordinateAxis::X,
                    maximum_spacing: Quantity::length(30.0, LengthUnit::Millimeter),
                }),
            )
            .unwrap();
        graph.refresh_driven_patterns(&session).unwrap();
        assert_eq!(graph.patterns()[0].slot_count, 5);

        // `marker` exists only in the secondary family of the measured
        // instance; its 5 mm curved height at 1 mm spacing is 5 gaps.
        graph
            .set_pattern_count_driver(
                "bolts",
                Some(PatternCountDriver::BoundsExtent {
                    instance: "marked".into(),
                    output: "marker".into(),
                    axis: CoordinateAxis::Z,
                    maximum_spacing: Quantity::length(1.0, LengthUnit::Millimeter),
                }),
            )
            .unwrap();
        graph.refresh_driven_patterns(&session).unwrap();
        assert_eq!(graph.patterns()[0].slot_count, 6);
        assert_eq!(session.shape_count().unwrap(), 0);

        let missing = graph
            .set_pattern_count_driver(
                "bolts",
                Some(PatternCountDriver::BoundsExtent {
                    instance: "source".into(),
                    output: "marker".into(),
                    axis: CoordinateAxis::Z,
                    maximum_spacing: Quantity::length(1.0, LengthUnit::Millimeter),
                }),
            )
            .unwrap_err();
        assert!(
            missing
                .message
                .contains("unknown pattern measurement output")
        );
    }

    #[test]
    fn invalid_pattern_drivers_are_rejected_before_mutation() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition
            .parameters
            .push(integer_parameter("bolt_count", 4));
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        graph
            .add_linear_pattern(
                "free",
                "free_member",
                "source",
                2,
                VectorQuantity::lengths(10.0, 0.0, 0.0, LengthUnit::Millimeter),
                "test",
            )
            .unwrap();

        let wrong_type = graph
            .set_pattern_count_driver(
                "free",
                Some(PatternCountDriver::Parameter {
                    instance: "source".into(),
                    parameter: "width".into(),
                }),
            )
            .unwrap_err();
        assert!(wrong_type.message.contains("must be an integer"));
        assert!(graph.patterns()[0].count_driver.is_none());

        let missing_output = graph
            .set_pattern_count_driver(
                "free",
                Some(PatternCountDriver::BoundsExtent {
                    instance: "source".into(),
                    output: "missing".into(),
                    axis: CoordinateAxis::X,
                    maximum_spacing: Quantity::length(10.0, LengthUnit::Millimeter),
                }),
            )
            .unwrap_err();
        assert!(
            missing_output
                .message
                .contains("unknown pattern measurement output")
        );
        assert!(graph.patterns()[0].count_driver.is_none());

        let incompatible_span = graph
            .set_pattern_span_driver(
                "free",
                Some(PatternSpanDriver::Parameter {
                    instance: "source".into(),
                    parameter: "width".into(),
                    direction: VectorQuantity::scalars(1.0, 0.0, 0.0),
                }),
            )
            .unwrap_err();
        assert!(incompatible_span.message.contains("requires a linear_fit"));

        graph
            .add_fitted_pattern(
                "fit",
                "fit_member",
                "source",
                linear_fit(100.0, LinearSpacing::Count(3)),
                "test",
            )
            .unwrap();
        let incompatible_count = graph
            .set_pattern_count_driver(
                "fit",
                Some(PatternCountDriver::Parameter {
                    instance: "source".into(),
                    parameter: "bolt_count".into(),
                }),
            )
            .unwrap_err();
        assert!(incompatible_count.message.contains("freely counted"));
        let zero_direction = graph
            .set_pattern_span_driver(
                "fit",
                Some(PatternSpanDriver::Parameter {
                    instance: "source".into(),
                    parameter: "width".into(),
                    direction: VectorQuantity::scalars(0.0, 0.0, 0.0),
                }),
            )
            .unwrap_err();
        assert!(zero_direction.message.contains("direction is zero"));
        assert!(graph.patterns()[1].span_driver.is_none());
    }

    #[test]
    fn parameters_and_measured_geometry_drive_linear_fit_spans() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition
            .parameters
            .push(length_parameter("aisle_length", 120.0));
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        graph
            .add_fitted_pattern(
                "pews",
                "pew",
                "source",
                linear_fit(
                    30.0,
                    LinearSpacing::Maximum(Quantity::length(30.0, LengthUnit::Millimeter)),
                ),
                "parameter-driven",
            )
            .unwrap();
        graph
            .set_pattern_span_driver(
                "pews",
                Some(PatternSpanDriver::Parameter {
                    instance: "source".into(),
                    parameter: "aisle_length".into(),
                    direction: VectorQuantity::scalars(1.0, 0.0, 0.0),
                }),
            )
            .unwrap();
        let session = Session::new().unwrap();
        graph.refresh_driven_patterns(&session).unwrap();
        assert_eq!(graph.patterns()[0].slot_count, 5);
        assert_eq!(graph.node("pew[4]").unwrap().placement(), along_x(120.0));
        assert_eq!(session.shape_count().unwrap(), 0);

        graph
            .set_override(
                "source",
                "width",
                ParameterValue::Scalar(Quantity::length(110.0, LengthUnit::Millimeter)),
            )
            .unwrap();
        graph
            .set_pattern_span_driver(
                "pews",
                Some(PatternSpanDriver::BoundsExtent {
                    instance: "source".into(),
                    output: "body".into(),
                    axis: CoordinateAxis::X,
                    direction: VectorQuantity::scalars(0.0, 1.0, 0.0),
                }),
            )
            .unwrap();
        graph.refresh_driven_patterns(&session).unwrap();
        assert_eq!(graph.patterns()[0].slot_count, 5);
        let measured_placement = graph.node("pew[4]").unwrap().placement();
        assert!(measured_placement.translation.x.value.abs() < 1e-9);
        assert!((measured_placement.translation.y.value - 110.0).abs() < 1e-9);
        assert!(measured_placement.translation.z.value.abs() < 1e-9);
        assert_eq!(session.shape_count().unwrap(), 0);
        for member in &graph.patterns()[0].members {
            assert_eq!(
                graph.node(&member.id).unwrap().placement(),
                graph.patterns()[0].member_placement(member),
                "{}",
                member.id
            );
        }

        let document = ModelDocument::from_graph(&graph);
        let json = document.to_json_pretty().unwrap();
        let loaded = ModelDocument::from_json(&json).unwrap();
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(loaded.patterns[0].slot_count, 5);
        assert_eq!(
            loaded.patterns[0].span_driver,
            document.patterns[0].span_driver
        );
    }

    fn circular_fit(sweep_radians: f64, spacing: AngularSpacing) -> PatternRule {
        PatternRule::CircularFit {
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            sweep_radians,
            spacing,
        }
    }

    #[test]
    fn circular_fit_divides_closed_and_open_sweeps() {
        use std::f64::consts::{PI, TAU};
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        let angle = |graph: &InstanceGraph<'_>, id: &str| {
            graph
                .node(id)
                .unwrap()
                .placement()
                .rotation
                .unwrap()
                .angle_radians
        };

        graph
            .add_fitted_pattern(
                "bolts",
                "bolt",
                "source",
                circular_fit(TAU, AngularSpacing::Count(6)),
                "fit",
            )
            .unwrap();
        // A closed turn does not repeat the first bolt at 360 degrees.
        assert!((angle(&graph, "bolt[5]") - 5.0 * PI / 3.0).abs() < 1e-12);

        graph
            .set_pattern_rule("bolts", circular_fit(PI, AngularSpacing::Count(5)))
            .unwrap();
        assert!((angle(&graph, "bolt[4]") - PI).abs() < 1e-12);
        assert!(graph.node("bolt[5]").is_none());

        graph
            .set_pattern_rule(
                "bolts",
                circular_fit(TAU, AngularSpacing::MaximumRadians(PI / 4.0)),
            )
            .unwrap();
        assert_eq!(member_ids(&graph).len(), 8);
        graph
            .set_pattern_rule(
                "bolts",
                circular_fit(PI, AngularSpacing::MinimumRadians(PI / 3.0)),
            )
            .unwrap();
        assert_eq!(member_ids(&graph).len(), 4);

        for invalid in [
            circular_fit(TAU, AngularSpacing::MinimumRadians(7.0)),
            circular_fit(0.0, AngularSpacing::Count(2)),
            circular_fit(7.0, AngularSpacing::Count(2)),
            circular_fit(PI, AngularSpacing::MaximumRadians(f64::NAN)),
        ] {
            assert!(
                graph.set_pattern_rule("bolts", invalid).is_err(),
                "{invalid:?}"
            );
        }
        assert_eq!(member_ids(&graph).len(), 4);
    }

    #[test]
    fn schema_seventeen_patterns_gain_slot_counts_and_prefixes() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = pew_row(&definition);
        graph.detach("pew[2]").unwrap();
        let current = ModelDocument::from_graph(&graph);

        let mut legacy = serde_json::to_value(&current).unwrap();
        legacy["schema_version"] = serde_json::json!(17);
        let pattern = legacy["patterns"][0].as_object_mut().unwrap();
        pattern.remove("slot_count");
        pattern.remove("member_prefix");
        let migrated = ModelDocument::from_json(&legacy.to_string()).unwrap();
        // Slot 2 was detached, so the highest remaining slot sets the count.
        assert_eq!(migrated.patterns[0].slot_count, 2);
        assert_eq!(migrated.patterns[0].member_prefix, "pew");

        let mut renamed = legacy.clone();
        renamed["patterns"][0]["members"][0]["id"] = serde_json::json!("alpha");
        let instances = renamed["instances"].as_array_mut().unwrap();
        for node in instances.iter_mut() {
            let variant = node.as_object_mut().unwrap().values_mut().next().unwrap();
            if variant["id"] == "pew[0]" {
                variant["id"] = serde_json::json!("alpha");
            }
        }
        let migrated = ModelDocument::from_json(&renamed.to_string()).unwrap();
        assert_eq!(migrated.patterns[0].member_prefix, "pews");
    }

    #[test]
    fn documents_reject_slot_counts_that_disagree_with_members_or_constraints() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let graph = pew_row(&definition);
        let document = ModelDocument::from_graph(&graph);

        let mut short = document.clone();
        short.patterns[0].slot_count = 2;
        let error = short.to_json_pretty().unwrap_err();
        assert!(
            error.message.contains("outside the 2 pattern slots"),
            "{error}"
        );

        let mut constrained = document;
        constrained.patterns[0].rule = linear_fit(100.0, LinearSpacing::Count(5));
        let error = constrained.to_json_pretty().unwrap_err();
        assert!(error.message.contains("require 5 slots"), "{error}");
    }

    #[test]
    fn invalid_pattern_rules_are_rejected() {
        let definition = family(RequirementPriority::Required, 100_000.0);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        let origin = VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter);
        let zero_axis = graph.add_pattern(
            "ring",
            "spoke",
            "source",
            3,
            PatternRule::Circular {
                origin,
                axis: VectorQuantity::scalars(0.0, 0.0, 0.0),
                angle_step_radians: 1.0,
            },
            "pattern",
        );
        assert!(zero_axis.unwrap_err().message.contains("axis is zero"));
        let infinite_angle = graph.add_pattern(
            "ring",
            "spoke",
            "source",
            3,
            PatternRule::Circular {
                origin,
                axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                angle_step_radians: f64::INFINITY,
            },
            "pattern",
        );
        assert!(infinite_angle.unwrap_err().message.contains("not finite"));
        assert!(graph.patterns().is_empty());
        assert!(graph.node("spoke[0]").is_none());

        graph
            .add_pattern(
                "ring",
                "spoke",
                "source",
                2,
                PatternRule::Circular {
                    origin,
                    axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    angle_step_radians: 1.0,
                },
                "pattern",
            )
            .unwrap();
        let mut value = serde_json::to_value(ModelDocument::from_graph(&graph)).unwrap();
        value["patterns"][0]["rule"]["circular"]["axis"] =
            serde_json::to_value(VectorQuantity::scalars(0.0, 0.0, 0.0)).unwrap();
        let error = ModelDocument::from_json(&value.to_string()).unwrap_err();
        assert!(error.message.contains("pattern 'ring'"));
    }

    #[test]
    fn managed_generation_can_be_frozen_and_unfrozen() {
        let definition = family(RequirementPriority::Required, 100_000.0);
        let instance = PartInstance {
            id: "frozen".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let mut managed = ManagedPartInstance::new(&session, instance);

        assert!(managed.freeze().is_err());
        managed.regenerate().unwrap();
        assert_eq!(managed.freeze().unwrap(), 1);
        assert_eq!(managed.state(), RegenerationState::Frozen);
        managed.instance_mut().overrides.insert(
            "width".into(),
            ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
        );
        let error = managed.regenerate().err().unwrap();
        assert!(error.message.contains("frozen"));
        assert_eq!(managed.attempted_revision(), 1);
        assert_eq!(managed.accepted_revision(), Some(1));
        assert_eq!(session.shape_count().unwrap(), 2);

        managed.unfreeze();
        managed.regenerate().unwrap();
        assert_eq!(managed.state(), RegenerationState::Current);
        assert_eq!(managed.attempted_revision(), 2);
        assert_eq!(managed.accepted_revision(), Some(2));
        assert!(
            (session
                .volume(managed.accepted().unwrap().shape("body").unwrap())
                .unwrap()
                - 7_200.0)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn invalid_placement_is_rejected_before_graph_mutation() {
        let definition = family(RequirementPriority::Required, 100_000.0);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        let invalid = Placement {
            translation: VectorQuantity::lengths(1.0, 0.0, 0.0, LengthUnit::Millimeter),
            rotation: Some(AxisAngle {
                origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                axis: VectorQuantity::scalars(0.0, 0.0, 0.0),
                angle_radians: 1.0,
            }),
        };

        assert!(graph.set_placement("source", invalid).is_err());
        assert_eq!(
            graph.node("source").unwrap().placement(),
            Placement::identity()
        );
    }

    #[test]
    fn derived_parameters_drive_geometry_and_constraints_precede_generation() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition.parameters.push(length_parameter("margin", 5.0));
        definition
            .derived_parameters
            .push(DerivedParameterDefinition {
                id: "overall_width".into(),
                dimension: Dimension::Length,
                expression: ScalarExpr::Add(
                    Box::new(ScalarExpr::Parameter("width".into())),
                    Box::new(ScalarExpr::Multiply(
                        Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
                        Box::new(ScalarExpr::Parameter("margin".into())),
                    )),
                ),
            });
        if let FeatureOperation::Box {
            size: VectorExpr::Components { x, .. },
            ..
        } = &mut definition.features[1].operation
        {
            *x = ScalarExpr::Parameter("overall_width".into());
        }
        definition.constraints.push(ParameterConstraint {
            id: "overall-width.limit".into(),
            statement: "overall width must not exceed 40 mm".into(),
            left: ScalarExpr::Parameter("overall_width".into()),
            relation: ConstraintRelation::LessOrEqual,
            right: ScalarExpr::Literal(Quantity::length(40.0, LengthUnit::Millimeter)),
            provenance: "test".into(),
        });

        let valid = PartInstance {
            id: "derived-valid".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let valid_session = Session::new().unwrap();
        let result = valid.regenerate(&valid_session).unwrap();
        let bounds = valid_session.bounds(result.shape("body").unwrap()).unwrap();
        assert!((bounds.max.x - 20.0).abs() < 1e-6);

        let invalid = PartInstance {
            id: "derived-invalid".into(),
            definition: &definition,
            overrides: HashMap::from([(
                "width".into(),
                ParameterValue::Scalar(Quantity::length(35.0, LengthUnit::Millimeter)),
            )]),
            provenance: "test".into(),
        };
        let invalid_session = Session::new().unwrap();
        let error = invalid.regenerate(&invalid_session).err().unwrap();
        assert!(error.message.contains("overall-width.limit"));
        assert_eq!(invalid_session.shape_count().unwrap(), 0);
    }

    #[test]
    fn richer_scalar_functions_are_dimension_safe_and_drive_geometry() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition.derived_parameters = vec![
            DerivedParameterDefinition {
                id: "negated_width".into(),
                dimension: Dimension::Length,
                expression: ScalarExpr::Negate(Box::new(ScalarExpr::Parameter("width".into()))),
            },
            DerivedParameterDefinition {
                id: "absolute_width".into(),
                dimension: Dimension::Length,
                expression: ScalarExpr::Absolute(Box::new(ScalarExpr::Parameter(
                    "negated_width".into(),
                ))),
            },
            DerivedParameterDefinition {
                id: "at_least_twelve".into(),
                dimension: Dimension::Length,
                expression: ScalarExpr::Maximum(
                    Box::new(ScalarExpr::Parameter("absolute_width".into())),
                    Box::new(ScalarExpr::Literal(Quantity::length(
                        12.0,
                        LengthUnit::Millimeter,
                    ))),
                ),
            },
            DerivedParameterDefinition {
                id: "at_most_fifteen".into(),
                dimension: Dimension::Length,
                expression: ScalarExpr::Minimum(
                    Box::new(ScalarExpr::Parameter("at_least_twelve".into())),
                    Box::new(ScalarExpr::Literal(Quantity::length(
                        15.0,
                        LengthUnit::Millimeter,
                    ))),
                ),
            },
            DerivedParameterDefinition {
                id: "bounded_depth".into(),
                dimension: Dimension::Length,
                expression: ScalarExpr::Clamp {
                    value: Box::new(ScalarExpr::Parameter("depth".into())),
                    minimum: Box::new(ScalarExpr::Parameter("at_most_fifteen".into())),
                    maximum: Box::new(ScalarExpr::Literal(Quantity::length(
                        18.0,
                        LengthUnit::Millimeter,
                    ))),
                },
            },
        ];
        let body = definition
            .features
            .iter_mut()
            .find(|feature| feature.id == "body")
            .unwrap();
        if let FeatureOperation::Box { size, .. } = &mut body.operation {
            *size = VectorExpr::Components {
                x: ScalarExpr::Parameter("at_most_fifteen".into()),
                y: ScalarExpr::Parameter("bounded_depth".into()),
                z: ScalarExpr::Parameter("height".into()),
            };
        } else {
            panic!("body feature must be a box");
        }

        let instance = PartInstance {
            id: "richer-expressions".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();
        let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
        assert!((bounds.max.x - 12.0).abs() < 1e-6);
        assert!((bounds.max.y - 18.0).abs() < 1e-6);
    }

    #[test]
    fn richer_scalar_functions_reject_invalid_dimensions_and_bounds() {
        let parameters = HashMap::new();
        let mismatched = ScalarExpr::Minimum(
            Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
            Box::new(ScalarExpr::Literal(Quantity::length(
                1.0,
                LengthUnit::Millimeter,
            ))),
        );
        assert!(
            evaluate_resolved_expression(&mismatched, &parameters)
                .err()
                .unwrap()
                .message
                .contains("matching dimensions")
        );

        let reversed = ScalarExpr::Clamp {
            value: Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
            minimum: Box::new(ScalarExpr::Literal(Quantity::scalar(3.0))),
            maximum: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
        };
        assert!(
            evaluate_resolved_expression(&reversed, &parameters)
                .err()
                .unwrap()
                .message
                .contains("minimum")
        );
    }

    #[test]
    fn conditional_scalar_expressions_choose_branches_and_drive_geometry() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition
            .derived_parameters
            .push(DerivedParameterDefinition {
                id: "selected_width".into(),
                dimension: Dimension::Length,
                expression: ScalarExpr::Conditional {
                    left: Box::new(ScalarExpr::Parameter("width".into())),
                    relation: ConstraintRelation::LessOrEqual,
                    right: Box::new(ScalarExpr::Literal(Quantity::length(
                        15.0,
                        LengthUnit::Millimeter,
                    ))),
                    when_true: Box::new(ScalarExpr::Parameter("depth".into())),
                    when_false: Box::new(ScalarExpr::Parameter("height".into())),
                },
            });
        let body = definition
            .features
            .iter_mut()
            .find(|feature| feature.id == "body")
            .unwrap();
        if let FeatureOperation::Box { size, .. } = &mut body.operation {
            *size = VectorExpr::Components {
                x: ScalarExpr::Parameter("selected_width".into()),
                y: ScalarExpr::Parameter("depth".into()),
                z: ScalarExpr::Parameter("height".into()),
            };
        }

        for (width, expected) in [(10.0, 20.0), (20.0, 30.0)] {
            let instance = PartInstance {
                id: "conditional".into(),
                definition: &definition,
                overrides: HashMap::from([(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(width, LengthUnit::Millimeter)),
                )]),
                provenance: "test".into(),
            };
            let session = Session::new().unwrap();
            let result = instance.regenerate(&session).unwrap();
            let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
            assert!((bounds.max.x - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn conditional_scalar_expressions_validate_comparisons_and_both_branches() {
        let mismatched_branches = ScalarExpr::Conditional {
            left: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
            relation: ConstraintRelation::GreaterOrEqual,
            right: Box::new(ScalarExpr::Literal(Quantity::scalar(0.0))),
            when_true: Box::new(ScalarExpr::Literal(Quantity::length(
                1.0,
                LengthUnit::Millimeter,
            ))),
            when_false: Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
        };
        let error = evaluate_resolved_expression(&mismatched_branches, &HashMap::new())
            .err()
            .unwrap();
        assert!(
            error
                .message
                .contains("branches require matching dimensions")
        );

        let invalid_comparison = ScalarExpr::Conditional {
            left: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
            relation: ConstraintRelation::Equal {
                tolerance: Quantity::length(0.1, LengthUnit::Millimeter),
            },
            right: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
            when_true: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
            when_false: Box::new(ScalarExpr::Literal(Quantity::scalar(0.0))),
        };
        let error = evaluate_resolved_expression(&invalid_comparison, &HashMap::new())
            .err()
            .unwrap();
        assert!(error.message.contains("tolerance has the wrong dimension"));
    }

    #[test]
    fn derived_vector_arithmetic_and_normalization_drive_features() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition.derived_vector_parameters = vec![
            DerivedVectorParameterDefinition {
                id: "base_offset".into(),
                dimension: Dimension::Length,
                expression: VectorExpr::Components {
                    x: ScalarExpr::Parameter("width".into()),
                    y: ScalarExpr::Parameter("depth".into()),
                    z: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
                },
            },
            DerivedVectorParameterDefinition {
                id: "shifted_offset".into(),
                dimension: Dimension::Length,
                expression: VectorExpr::Add(
                    Box::new(VectorExpr::Parameter("base_offset".into())),
                    Box::new(VectorExpr::Literal(VectorQuantity::lengths(
                        1.0,
                        2.0,
                        3.0,
                        LengthUnit::Millimeter,
                    ))),
                ),
            },
            DerivedVectorParameterDefinition {
                id: "delta".into(),
                dimension: Dimension::Length,
                expression: VectorExpr::Subtract(
                    Box::new(VectorExpr::Parameter("shifted_offset".into())),
                    Box::new(VectorExpr::Parameter("base_offset".into())),
                ),
            },
            DerivedVectorParameterDefinition {
                id: "scaled_delta".into(),
                dimension: Dimension::Length,
                expression: VectorExpr::Scale {
                    vector: Box::new(VectorExpr::Parameter("delta".into())),
                    factor: ScalarExpr::Literal(Quantity::scalar(2.0)),
                },
            },
            DerivedVectorParameterDefinition {
                id: "axis".into(),
                dimension: Dimension::Scalar,
                expression: VectorExpr::Normalize(Box::new(VectorExpr::Parameter(
                    "scaled_delta".into(),
                ))),
            },
        ];
        definition.features.push(FeatureDefinition {
            id: "vector-placed".into(),
            operation: FeatureOperation::Translate {
                input: "body".into(),
                offset: VectorExpr::Add(
                    Box::new(VectorExpr::Parameter("base_offset".into())),
                    Box::new(VectorExpr::Parameter("scaled_delta".into())),
                ),
            },
        });
        definition.features.push(FeatureDefinition {
            id: "vector-cylinder".into(),
            operation: FeatureOperation::Cylinder {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    50.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                axis: VectorExpr::Parameter("axis".into()),
                radius: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
                height: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
            },
        });
        let instance = PartInstance {
            id: "derived-vectors".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();
        let bounds = session
            .bounds(result.shape("vector-placed").unwrap())
            .unwrap();
        assert!((bounds.min.x - 12.0).abs() < 1e-6);
        assert!((bounds.min.y - 24.0).abs() < 1e-6);
        assert!((bounds.min.z - 6.0).abs() < 1e-6);
        assert!(
            session
                .is_valid(result.shape("vector-cylinder").unwrap())
                .unwrap()
        );
    }

    #[test]
    fn derived_vectors_report_cycles_and_invalid_operations() {
        let mut cycle = family(RequirementPriority::Required, 100_000.0);
        cycle.derived_vector_parameters = vec![
            DerivedVectorParameterDefinition {
                id: "a".into(),
                dimension: Dimension::Length,
                expression: VectorExpr::Parameter("b".into()),
            },
            DerivedVectorParameterDefinition {
                id: "b".into(),
                dimension: Dimension::Length,
                expression: VectorExpr::Parameter("a".into()),
            },
        ];
        let error = resolve_parameters(&cycle, &HashMap::new()).err().unwrap();
        assert!(error.message.contains("a -> b -> a"));

        let mut invalid = family(RequirementPriority::Required, 100_000.0);
        invalid
            .derived_vector_parameters
            .push(DerivedVectorParameterDefinition {
                id: "bad".into(),
                dimension: Dimension::Length,
                expression: VectorExpr::Scale {
                    vector: Box::new(VectorExpr::Literal(VectorQuantity::lengths(
                        1.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    ))),
                    factor: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
                },
            });
        let error = resolve_parameters(&invalid, &HashMap::new()).err().unwrap();
        assert!(error.message.contains("scale factor"));
    }

    #[test]
    fn derived_parameter_cycles_report_the_dependency_path() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.derived_parameters = vec![
            DerivedParameterDefinition {
                id: "a".into(),
                dimension: Dimension::Length,
                expression: ScalarExpr::Parameter("b".into()),
            },
            DerivedParameterDefinition {
                id: "b".into(),
                dimension: Dimension::Length,
                expression: ScalarExpr::Parameter("a".into()),
            },
        ];
        let instance = PartInstance {
            id: "derived-cycle".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();

        let error = instance.regenerate(&session).err().unwrap();
        assert!(error.message.contains("a -> b -> a"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn derived_expression_rejects_invalid_dimension_arithmetic() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition
            .derived_parameters
            .push(DerivedParameterDefinition {
                id: "invalid".into(),
                dimension: Dimension::Length,
                expression: ScalarExpr::Add(
                    Box::new(ScalarExpr::Parameter("width".into())),
                    Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
                ),
            });
        let instance = PartInstance {
            id: "dimension-error".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();

        let error = instance.regenerate(&session).err().unwrap();
        assert!(error.message.contains("matching dimensions"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn model_document_round_trips_intent_and_regenerates_loaded_instances() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph
            .add_base(
                "source",
                HashMap::from([(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter)),
                )]),
                "user",
            )
            .unwrap();
        graph
            .add_linear_pattern(
                "row",
                "member",
                "source",
                2,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "pattern",
            )
            .unwrap();
        graph
            .set_override(
                "member[1]",
                "depth",
                ParameterValue::Scalar(Quantity::length(25.0, LengthUnit::Millimeter)),
            )
            .unwrap();
        let mut document = ModelDocument::from_graph(&graph);
        document.generation_records.push(GenerationRecord {
            instance_id: "source".into(),
            attempted_revision: 3,
            accepted_revision: Some(2),
            state: RegenerationState::Stale,
            last_error: Some("new parameters failed verification".into()),
        });

        let json = document.to_json_pretty().unwrap();
        let loaded = ModelDocument::from_json(&json).unwrap();
        assert_eq!(loaded, document);
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(loaded.generation_records[0].accepted_revision, Some(2));

        let loaded_graph = loaded.instance_graph().unwrap();
        let resolved = loaded_graph.resolve_with_placement("member[1]").unwrap();
        assert_eq!(
            resolved.instance.overrides["width"],
            ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter))
        );
        assert_eq!(
            resolved.instance.overrides["depth"],
            ParameterValue::Scalar(Quantity::length(25.0, LengthUnit::Millimeter))
        );
        let session = Session::new().unwrap();
        let result = resolved.regenerate(&session).unwrap();
        let bounds = session.bounds(result.shape("body").unwrap()).unwrap();
        assert!((bounds.min.x - 50.0).abs() < 1e-6);
        assert!((bounds.max.x - 62.0).abs() < 1e-6);
    }

    #[test]
    fn schema_one_documents_migrate_missing_fields_to_current_defaults() {
        let definition = family(RequirementPriority::Required, 100_000.0);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("legacy", HashMap::new(), "import").unwrap();
        let current = ModelDocument::from_graph(&graph);
        let mut value = serde_json::to_value(current).unwrap();
        let object = value.as_object_mut().unwrap();
        object.insert("schema_version".into(), serde_json::json!(1));
        object.remove("patterns");
        object.remove("frames");
        object.remove("generation_records");
        let family = object.get_mut("family").unwrap().as_object_mut().unwrap();
        family.remove("derived_parameters");
        family.remove("derived_vector_parameters");
        family.remove("constraints");
        for instance in object.get_mut("instances").unwrap().as_array_mut().unwrap() {
            let variant = instance
                .as_object_mut()
                .unwrap()
                .values_mut()
                .next()
                .unwrap();
            variant.as_object_mut().unwrap().remove("placement");
            variant.as_object_mut().unwrap().remove("frame");
        }

        let migrated = ModelDocument::from_json(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(migrated.family.derived_parameters.is_empty());
        assert!(migrated.family.derived_vector_parameters.is_empty());
        assert!(migrated.family.constraints.is_empty());
        assert!(migrated.patterns.is_empty());
        assert!(migrated.frames.is_empty());
        assert_eq!(migrated.instances[0].frame(), None);
        assert!(migrated.generation_records.is_empty());
        assert_eq!(migrated.instances[0].placement(), Placement::identity());
        assert!(migrated.instance_graph().unwrap().resolve("legacy").is_ok());

        for version in 2..CURRENT_SCHEMA_VERSION {
            let mut previous = serde_json::to_value(ModelDocument::from_graph(&graph)).unwrap();
            previous["schema_version"] = serde_json::json!(version);
            let migrated =
                ModelDocument::from_json(&serde_json::to_string(&previous).unwrap()).unwrap();
            assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        }
    }

    #[test]
    fn schema_thirteen_linear_patterns_migrate_to_tagged_rules() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        let step = VectorQuantity::lengths(5.0, 0.0, 0.0, LengthUnit::Centimeter);
        graph
            .add_linear_pattern("row", "member", "source", 2, step, "pattern")
            .unwrap();
        let current = ModelDocument::from_graph(&graph);
        let mut legacy = serde_json::to_value(&current).unwrap();
        legacy["schema_version"] = serde_json::json!(13);
        let pattern = legacy["patterns"][0].as_object_mut().unwrap();
        pattern.remove("rule");
        pattern.insert("step".into(), serde_json::to_value(step).unwrap());

        let migrated = ModelDocument::from_json(&legacy.to_string()).unwrap();
        assert_eq!(migrated, current);
        assert_eq!(migrated.patterns[0].rule, PatternRule::Linear { step });

        legacy["patterns"][0]
            .as_object_mut()
            .unwrap()
            .remove("step");
        let error = ModelDocument::from_json(&legacy.to_string()).unwrap_err();
        assert!(error.message.contains("requires a step"));
    }

    #[test]
    fn schema_thirteen_round_trips_new_selectors_and_expressions() {
        let edge_selectors = vec![
            EdgeSelector::Longest {
                allow_ties: true,
                relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
            },
            EdgeSelector::CircularRadius {
                minimum: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
                maximum: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
            },
            EdgeSelector::CurvatureRadius {
                minimum: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
                maximum: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
            },
            EdgeSelector::CurvatureRadiusRange {
                minimum: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
                maximum: ScalarExpr::Literal(Quantity::length(8.0, LengthUnit::Millimeter)),
                sample_count: 17,
                require_entire_edge: true,
            },
            EdgeSelector::CurvatureRadiusBounds {
                minimum: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
                maximum: ScalarExpr::Literal(Quantity::length(8.0, LengthUnit::Millimeter)),
                relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
                require_entire_edge: false,
            },
            EdgeSelector::Union(vec![
                EdgeSelector::Intersection(vec![EdgeSelector::Longest {
                    allow_ties: true,
                    relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
                }]),
                EdgeSelector::Difference {
                    base: Box::new(EdgeSelector::Longest {
                        allow_ties: true,
                        relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
                    }),
                    subtract: Box::new(EdgeSelector::Longest {
                        allow_ties: true,
                        relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-7)),
                    }),
                },
            ]),
        ];
        let face_selectors = vec![
            FaceSelector::LargestArea {
                planar_only: true,
                allow_ties: false,
                relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
            },
            FaceSelector::TangentTo {
                faces: Box::new(FaceSelector::LargestArea {
                    planar_only: true,
                    allow_ties: true,
                    relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
                }),
                minimum_count: 1,
            },
            FaceSelector::Union(vec![FaceSelector::Intersection(vec![
                FaceSelector::Difference {
                    base: Box::new(FaceSelector::LargestArea {
                        planar_only: true,
                        allow_ties: true,
                        relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-6)),
                    }),
                    subtract: Box::new(FaceSelector::LargestArea {
                        planar_only: false,
                        allow_ties: true,
                        relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-7)),
                    }),
                },
            ])]),
        ];
        let expressions = vec![
            ScalarExpr::Negate(Box::new(ScalarExpr::Literal(Quantity::scalar(1.0)))),
            ScalarExpr::Absolute(Box::new(ScalarExpr::Literal(Quantity::scalar(-1.0)))),
            ScalarExpr::Minimum(
                Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
                Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
            ),
            ScalarExpr::Maximum(
                Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
                Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
            ),
            ScalarExpr::Clamp {
                value: Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
                minimum: Box::new(ScalarExpr::Literal(Quantity::scalar(0.0))),
                maximum: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
            },
            ScalarExpr::Conditional {
                left: Box::new(ScalarExpr::Literal(Quantity::scalar(1.0))),
                relation: ConstraintRelation::LessOrEqual,
                right: Box::new(ScalarExpr::Literal(Quantity::scalar(2.0))),
                when_true: Box::new(ScalarExpr::Literal(Quantity::scalar(3.0))),
                when_false: Box::new(ScalarExpr::Literal(Quantity::scalar(4.0))),
            },
        ];
        let vector_expressions = vec![
            VectorExpr::Add(
                Box::new(VectorExpr::Literal(VectorQuantity::scalars(1.0, 2.0, 3.0))),
                Box::new(VectorExpr::Literal(VectorQuantity::scalars(4.0, 5.0, 6.0))),
            ),
            VectorExpr::Subtract(
                Box::new(VectorExpr::Literal(VectorQuantity::scalars(4.0, 5.0, 6.0))),
                Box::new(VectorExpr::Literal(VectorQuantity::scalars(1.0, 2.0, 3.0))),
            ),
            VectorExpr::Scale {
                vector: Box::new(VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0))),
                factor: ScalarExpr::Literal(Quantity::scalar(2.0)),
            },
            VectorExpr::Normalize(Box::new(VectorExpr::Literal(VectorQuantity::lengths(
                1.0,
                2.0,
                3.0,
                LengthUnit::Millimeter,
            )))),
        ];

        let edges_json = serde_json::to_string(&edge_selectors).unwrap();
        let face_json = serde_json::to_string(&face_selectors).unwrap();
        let expressions_json = serde_json::to_string(&expressions).unwrap();
        let vector_expressions_json = serde_json::to_string(&vector_expressions).unwrap();
        assert_eq!(
            serde_json::from_str::<Vec<EdgeSelector>>(&edges_json).unwrap(),
            edge_selectors
        );
        assert_eq!(
            serde_json::from_str::<Vec<FaceSelector>>(&face_json).unwrap(),
            face_selectors
        );
        assert_eq!(
            serde_json::from_str::<Vec<ScalarExpr>>(&expressions_json).unwrap(),
            expressions
        );
        assert_eq!(
            serde_json::from_str::<Vec<VectorExpr>>(&vector_expressions_json).unwrap(),
            vector_expressions
        );
        assert!(edges_json.contains("longest"));
        assert!(edges_json.contains("circular_radius"));
        assert!(edges_json.contains("curvature_radius"));
        assert!(edges_json.contains("curvature_radius_range"));
        assert!(edges_json.contains("intersection"));
        assert!(edges_json.contains("difference"));
        assert!(face_json.contains("largest_area"));
        assert!(face_json.contains("tangent_to"));
        assert!(face_json.contains("union"));
        assert!(expressions_json.contains("absolute"));
        assert!(expressions_json.contains("clamp"));
        assert!(expressions_json.contains("conditional"));
        assert!(vector_expressions_json.contains("normalize"));
    }

    #[test]
    fn model_document_rejects_future_versions_and_broken_links() {
        let definition = family(RequirementPriority::Required, 100_000.0);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("source", HashMap::new(), "test").unwrap();
        let document = ModelDocument::from_graph(&graph);
        let mut future = serde_json::to_value(&document).unwrap();
        future["schema_version"] = serde_json::json!(CURRENT_SCHEMA_VERSION + 1);
        let error = ModelDocument::from_json(&serde_json::to_string(&future).unwrap())
            .err()
            .unwrap();
        assert!(error.message.contains("unsupported"));

        let mut broken = document;
        broken.instances.push(InstanceNode::Clone {
            id: "orphan".into(),
            source: "missing".into(),
            overrides: HashMap::new(),
            placement: Placement::identity(),
            frame: None,
            provenance: "test".into(),
        });
        let error = broken.to_json_pretty().err().unwrap();
        assert!(error.message.contains("missing"));
    }

    #[test]
    fn semantic_history_selector_survives_serialization_and_drives_fillet() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition.features.push(FeatureDefinition {
            id: "filleted".into(),
            operation: FeatureOperation::Fillet {
                input: "placed".into(),
                edges: vec![EdgeSelector::History {
                    source_feature: "body".into(),
                    source: Box::new(EdgeSelector::NearestCenter {
                        target: VectorExpr::Literal(VectorQuantity::lengths(
                            0.0,
                            0.0,
                            15.0,
                            LengthUnit::Millimeter,
                        )),
                        maximum_distance: ScalarExpr::Literal(Quantity::length(
                            0.01,
                            LengthUnit::Millimeter,
                        )),
                    }),
                    relation: SemanticHistoryRelation::Modified,
                }],
                radius: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
            },
        });
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let json = ModelDocument::from_graph(&graph).to_json_pretty().unwrap();
        let loaded = ModelDocument::from_json(&json).unwrap();
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(json.contains("source_feature"));

        let loaded_graph = loaded.instance_graph().unwrap();
        let instance = loaded_graph.resolve("part").unwrap();
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();
        let filleted = result.shape("filleted").unwrap();
        assert!(session.is_valid(filleted).unwrap());
        assert!(session.volume(filleted).unwrap() < 6_000.0);
        assert_eq!(session.shape_count().unwrap(), 3);
    }

    #[test]
    fn ambiguous_semantic_selector_fails_and_cleans_temporary_shapes() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition.features.push(FeatureDefinition {
            id: "ambiguous-fillet".into(),
            operation: FeatureOperation::Fillet {
                input: "body".into(),
                edges: vec![EdgeSelector::NearestCenter {
                    target: VectorExpr::Literal(VectorQuantity::lengths(
                        5.0,
                        10.0,
                        15.0,
                        LengthUnit::Millimeter,
                    )),
                    maximum_distance: ScalarExpr::Literal(Quantity::length(
                        20.0,
                        LengthUnit::Millimeter,
                    )),
                }],
                radius: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
            },
        });
        let instance = PartInstance {
            id: "ambiguous".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();

        let error = instance.regenerate(&session).err().unwrap();
        assert!(error.message.contains("ambiguous"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn extremum_selector_chamfers_multiple_edges_without_topology_indices() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition.features.push(FeatureDefinition {
            id: "top-chamfer".into(),
            operation: FeatureOperation::Chamfer {
                input: "body".into(),
                edges: vec![EdgeSelector::AtExtreme {
                    axis: CoordinateAxis::Z,
                    extremum: Extremum::Maximum,
                    tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
                }],
                distance: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
            },
        });
        let instance = PartInstance {
            id: "multi-edge".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();

        let chamfered = result.shape("top-chamfer").unwrap();
        assert!(session.is_valid(chamfered).unwrap());
        assert!(session.volume(chamfered).unwrap() < 6_000.0);
        assert_eq!(session.shape_count().unwrap(), 3);
    }

    #[test]
    fn face_extremum_selector_opens_the_top_of_a_hollow_part() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition.features.push(FeatureDefinition {
            id: "open-shell".into(),
            operation: FeatureOperation::Hollow {
                input: "body".into(),
                faces: vec![FaceSelector::AtExtreme {
                    axis: CoordinateAxis::Z,
                    extremum: Extremum::Maximum,
                    tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
                }],
                thickness: ScalarExpr::Literal(Quantity::length(-1.0, LengthUnit::Millimeter)),
                tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
            },
        });
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("hollow", HashMap::new(), "test").unwrap();
        let json = ModelDocument::from_graph(&graph).to_json_pretty().unwrap();
        let loaded = ModelDocument::from_json(&json).unwrap();
        let loaded_graph = loaded.instance_graph().unwrap();
        let instance = loaded_graph.resolve("hollow").unwrap();
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();

        let hollow = result.shape("open-shell").unwrap();
        assert!(session.is_valid(hollow).unwrap());
        assert!(session.volume(hollow).unwrap() < 6_000.0);
        assert_eq!(session.shape_count().unwrap(), 3);
    }

    #[test]
    fn orientation_selector_tracks_a_face_after_feature_rotation() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition.features.push(FeatureDefinition {
            id: "rotated".into(),
            operation: FeatureOperation::Rotate {
                input: "body".into(),
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    0.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
            },
        });
        definition.features.push(FeatureDefinition {
            id: "oriented-shell".into(),
            operation: FeatureOperation::Hollow {
                input: "rotated".into(),
                faces: vec![FaceSelector::NormalAligned {
                    direction: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                    minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999)),
                }],
                thickness: ScalarExpr::Literal(Quantity::length(-1.0, LengthUnit::Millimeter)),
                tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
            },
        });
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("oriented", HashMap::new(), "test").unwrap();
        let json = ModelDocument::from_graph(&graph).to_json_pretty().unwrap();
        let loaded = ModelDocument::from_json(&json).unwrap();
        let loaded_graph = loaded.instance_graph().unwrap();
        let instance = loaded_graph.resolve("oriented").unwrap();
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();

        assert!(
            session
                .is_valid(result.shape("oriented-shell").unwrap())
                .unwrap()
        );
        assert_eq!(session.shape_count().unwrap(), 4);
    }

    #[test]
    fn adjacency_selector_finds_face_shared_by_semantic_edge_set() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition.features.push(FeatureDefinition {
            id: "adjacent-shell".into(),
            operation: FeatureOperation::Hollow {
                input: "body".into(),
                faces: vec![FaceSelector::AdjacentToEdges {
                    edges: Box::new(EdgeSelector::AtExtreme {
                        axis: CoordinateAxis::Z,
                        extremum: Extremum::Maximum,
                        tolerance: ScalarExpr::Literal(Quantity::length(
                            0.001,
                            LengthUnit::Millimeter,
                        )),
                    }),
                    minimum_count: 4,
                }],
                thickness: ScalarExpr::Literal(Quantity::length(-1.0, LengthUnit::Millimeter)),
                tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
            },
        });
        let instance = PartInstance {
            id: "adjacent".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();

        assert!(
            session
                .is_valid(result.shape("adjacent-shell").unwrap())
                .unwrap()
        );
        assert_eq!(session.shape_count().unwrap(), 3);
    }

    #[test]
    fn tangency_selector_finds_fillet_neighbors_without_topology_indices() {
        let session = Session::new().unwrap();
        let box_shape = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let edge = session.subshape(&box_shape, ShapeType::Edge, 0).unwrap();
        let filleted = session.fillet(&box_shape, &[&edge], 1.0).unwrap();
        let faces = (0..session.subshape_count(&filleted, ShapeType::Face).unwrap())
            .map(|index| session.subshape(&filleted, ShapeType::Face, index).unwrap())
            .collect::<Vec<_>>();
        let mut source_center = None;
        'pairs: for first in 0..faces.len() {
            for second in first + 1..faces.len() {
                if session
                    .faces_are_tangent(&filleted, &faces[first], &faces[second])
                    .unwrap()
                {
                    source_center = Some(session.center_of_mass(&faces[first]).unwrap());
                    break 'pairs;
                }
            }
        }
        let source_center = source_center.expect("fillet must record tangent face continuity");
        cleanup_shapes(&session, faces);

        let selector = FaceSelector::TangentTo {
            faces: Box::new(FaceSelector::NearestCenter {
                target: VectorExpr::Literal(VectorQuantity::lengths(
                    source_center.x,
                    source_center.y,
                    source_center.z,
                    LengthUnit::Millimeter,
                )),
                maximum_distance: ScalarExpr::Literal(Quantity::length(
                    0.001,
                    LengthUnit::Millimeter,
                )),
            }),
            minimum_count: 1,
        };
        let selected = resolve_face_selector(
            &session,
            &filleted,
            &selector,
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert!(!selected.is_empty());
        cleanup_shapes(&session, selected);
        assert_eq!(session.shape_count().unwrap(), 3);
    }

    #[test]
    fn selector_composition_performs_topological_set_operations() {
        let session = Session::new().unwrap();
        let box_shape = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let longest = EdgeSelector::Longest {
            allow_ties: true,
            relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-9)),
        };
        let minimum_x = EdgeSelector::AtExtreme {
            axis: CoordinateAxis::X,
            extremum: Extremum::Minimum,
            tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
        };
        let intersection = EdgeSelector::Intersection(vec![longest.clone(), minimum_x.clone()]);
        let difference = EdgeSelector::Difference {
            base: Box::new(longest),
            subtract: Box::new(minimum_x),
        };
        let edges = resolve_edge_selector(
            &session,
            &box_shape,
            &EdgeSelector::Union(vec![intersection, difference]),
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(edges.len(), 4);
        cleanup_shapes(&session, edges);

        let largest = FaceSelector::LargestArea {
            planar_only: true,
            allow_ties: true,
            relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-9)),
        };
        let maximum_x = FaceSelector::AtExtreme {
            axis: CoordinateAxis::X,
            extremum: Extremum::Maximum,
            tolerance: ScalarExpr::Literal(Quantity::length(0.001, LengthUnit::Millimeter)),
        };
        let faces = resolve_face_selector(
            &session,
            &box_shape,
            &FaceSelector::Union(vec![
                FaceSelector::Intersection(vec![largest.clone(), maximum_x.clone()]),
                FaceSelector::Difference {
                    base: Box::new(largest),
                    subtract: Box::new(maximum_x),
                },
            ]),
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(faces.len(), 2);
        cleanup_shapes(&session, faces);

        let error = resolve_edge_selector(
            &session,
            &box_shape,
            &EdgeSelector::Union(Vec::new()),
            &HashMap::new(),
            &HashMap::new(),
        )
        .err()
        .unwrap();
        assert!(error.message.contains("requires at least one"));
        assert_eq!(session.shape_count().unwrap(), 1);
    }

    #[test]
    fn longest_edge_selector_drives_chamfer_and_rejects_disallowed_ties() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition.features.push(FeatureDefinition {
            id: "long-edge-chamfer".into(),
            operation: FeatureOperation::Chamfer {
                input: "body".into(),
                edges: vec![EdgeSelector::Longest {
                    allow_ties: true,
                    relative_tolerance: ScalarExpr::Literal(Quantity::scalar(1e-9)),
                }],
                distance: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
            },
        });
        let instance = PartInstance {
            id: "longest-edges".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();
        let chamfered = result.shape("long-edge-chamfer").unwrap();
        assert!(session.is_valid(chamfered).unwrap());
        assert!(session.volume(chamfered).unwrap() < 6_000.0);

        let direct_session = Session::new().unwrap();
        let box_shape = direct_session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let error = select_longest_edges(&direct_session, &box_shape, false, 1e-9)
            .err()
            .unwrap();
        assert!(error.message.contains("ambiguous across 4 edges"));
        assert_eq!(direct_session.shape_count().unwrap(), 1);
    }

    #[test]
    fn circular_and_curvature_radius_selectors_drive_cylinder_chamfers() {
        let definition = FamilyDefinition {
            id: "CylinderSelectorFamily".into(),
            version: 1,
            parameters: Vec::new(),
            derived_parameters: Vec::new(),
            derived_vector_parameters: Vec::new(),
            constraints: Vec::new(),
            features: vec![
                FeatureDefinition {
                    id: "cylinder".into(),
                    operation: FeatureOperation::Cylinder {
                        origin: VectorExpr::Literal(VectorQuantity::lengths(
                            0.0,
                            0.0,
                            0.0,
                            LengthUnit::Millimeter,
                        )),
                        axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                        radius: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
                        height: ScalarExpr::Literal(Quantity::length(5.0, LengthUnit::Millimeter)),
                    },
                },
                FeatureDefinition {
                    id: "rim-chamfer".into(),
                    operation: FeatureOperation::Chamfer {
                        input: "cylinder".into(),
                        edges: vec![EdgeSelector::CircularRadius {
                            minimum: ScalarExpr::Literal(Quantity::length(
                                1.9,
                                LengthUnit::Millimeter,
                            )),
                            maximum: ScalarExpr::Literal(Quantity::length(
                                2.1,
                                LengthUnit::Millimeter,
                            )),
                        }],
                        distance: ScalarExpr::Literal(Quantity::length(
                            0.25,
                            LengthUnit::Millimeter,
                        )),
                    },
                },
                FeatureDefinition {
                    id: "curvature-chamfer".into(),
                    operation: FeatureOperation::Chamfer {
                        input: "cylinder".into(),
                        edges: vec![EdgeSelector::CurvatureRadius {
                            minimum: ScalarExpr::Literal(Quantity::length(
                                1.9,
                                LengthUnit::Millimeter,
                            )),
                            maximum: ScalarExpr::Literal(Quantity::length(
                                2.1,
                                LengthUnit::Millimeter,
                            )),
                        }],
                        distance: ScalarExpr::Literal(Quantity::length(
                            0.25,
                            LengthUnit::Millimeter,
                        )),
                    },
                },
                FeatureDefinition {
                    id: "full-curve-chamfer".into(),
                    operation: FeatureOperation::Chamfer {
                        input: "cylinder".into(),
                        edges: vec![EdgeSelector::CurvatureRadiusRange {
                            minimum: ScalarExpr::Literal(Quantity::length(
                                1.9,
                                LengthUnit::Millimeter,
                            )),
                            maximum: ScalarExpr::Literal(Quantity::length(
                                2.1,
                                LengthUnit::Millimeter,
                            )),
                            sample_count: 9,
                            require_entire_edge: true,
                        }],
                        distance: ScalarExpr::Literal(Quantity::length(
                            0.25,
                            LengthUnit::Millimeter,
                        )),
                    },
                },
            ],
            datums: Vec::new(),
            requirements: Vec::new(),
        };
        let instance = PartInstance {
            id: "circular-radius".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let result = instance.regenerate(&session).unwrap();
        assert!(
            session
                .is_valid(result.shape("rim-chamfer").unwrap())
                .unwrap()
        );
        assert!(
            session
                .is_valid(result.shape("curvature-chamfer").unwrap())
                .unwrap()
        );
        assert!(
            session
                .is_valid(result.shape("full-curve-chamfer").unwrap())
                .unwrap()
        );

        let straight_session = Session::new().unwrap();
        let box_shape = straight_session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let error = select_edges_by_curvature_radius(&straight_session, &box_shape, 1.0, 10.0)
            .err()
            .unwrap();
        assert!(error.message.contains("no matches"));
        assert_eq!(straight_session.shape_count().unwrap(), 1);

        let ellipse_session = Session::new().unwrap();
        let ellipse = ellipse_session
            .create_ellipse_wire(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 4.0, 2.0)
            .unwrap();
        let entire =
            select_edges_by_curvature_radius_range(&ellipse_session, &ellipse, 0.9, 8.1, 5, true)
                .unwrap();
        assert_eq!(entire.len(), 1);
        cleanup_shapes(&ellipse_session, entire);
        let partial =
            select_edges_by_curvature_radius_range(&ellipse_session, &ellipse, 7.9, 8.1, 5, false)
                .unwrap();
        assert_eq!(partial.len(), 1);
        cleanup_shapes(&ellipse_session, partial);
        assert_eq!(ellipse_session.shape_count().unwrap(), 1);
    }

    #[test]
    fn largest_planar_face_selector_finds_tied_box_faces() {
        let session = Session::new().unwrap();
        let box_shape = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let faces = select_largest_faces(&session, &box_shape, true, true, 1e-9).unwrap();
        assert_eq!(faces.len(), 2);
        for face in &faces {
            assert!(session.face_is_planar(face).unwrap());
            assert!((session.surface_area(face).unwrap() - 600.0).abs() < 1e-9);
        }
        cleanup_shapes(&session, faces);
        assert_eq!(session.shape_count().unwrap(), 1);
    }

    #[test]
    fn managed_regeneration_rebuilds_only_dirty_dependency_branches() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition
            .parameters
            .push(length_parameter("pin_radius", 2.0));
        definition.features.push(FeatureDefinition {
            id: "pin".into(),
            operation: FeatureOperation::Cylinder {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    50.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                radius: ScalarExpr::Parameter("pin_radius".into()),
                height: ScalarExpr::Literal(Quantity::length(10.0, LengthUnit::Millimeter)),
            },
        });
        let instance = PartInstance {
            id: "incremental".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let mut managed = ManagedPartInstance::new(&session, instance);
        managed.regenerate().unwrap();
        assert_eq!(managed.accepted().unwrap().regeneration.rebuilt.len(), 3);
        assert!(managed.accepted().unwrap().regeneration.reused.is_empty());

        managed.instance_mut().overrides.insert(
            "pin_radius".into(),
            ParameterValue::Scalar(Quantity::length(3.0, LengthUnit::Millimeter)),
        );
        managed.regenerate().unwrap();
        let report = &managed.accepted().unwrap().regeneration;
        assert_eq!(report.rebuilt, ["pin"]);
        let mut reused = report.reused.clone();
        reused.sort();
        assert_eq!(reused, ["body", "placed"]);
        assert_eq!(session.shape_count().unwrap(), 3);
        assert!(
            (session
                .volume(managed.accepted().unwrap().shape("pin").unwrap())
                .unwrap()
                - std::f64::consts::PI * 90.0)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn incremental_reuse_preserves_history_and_rolls_back_failed_downstream_work() {
        let mut definition = family(RequirementPriority::Required, 100_000.0);
        definition.requirements.clear();
        definition
            .parameters
            .push(length_parameter("fillet_radius", 1.0));
        definition.features.push(FeatureDefinition {
            id: "filleted".into(),
            operation: FeatureOperation::Fillet {
                input: "placed".into(),
                edges: vec![EdgeSelector::History {
                    source_feature: "body".into(),
                    source: Box::new(EdgeSelector::NearestCenter {
                        target: VectorExpr::Literal(VectorQuantity::lengths(
                            0.0,
                            0.0,
                            15.0,
                            LengthUnit::Millimeter,
                        )),
                        maximum_distance: ScalarExpr::Literal(Quantity::length(
                            0.01,
                            LengthUnit::Millimeter,
                        )),
                    }),
                    relation: SemanticHistoryRelation::Modified,
                }],
                radius: ScalarExpr::Parameter("fillet_radius".into()),
            },
        });
        let instance = PartInstance {
            id: "incremental-history".into(),
            definition: &definition,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        let session = Session::new().unwrap();
        let mut managed = ManagedPartInstance::new(&session, instance);
        managed.regenerate().unwrap();
        assert_eq!(session.shape_count().unwrap(), 3);

        managed.instance_mut().overrides.insert(
            "fillet_radius".into(),
            ParameterValue::Scalar(Quantity::length(1.5, LengthUnit::Millimeter)),
        );
        managed.regenerate().unwrap();
        let report = &managed.accepted().unwrap().regeneration;
        assert_eq!(report.rebuilt, ["filleted"]);
        let mut reused = report.reused.clone();
        reused.sort();
        assert_eq!(reused, ["body", "placed"]);
        assert_eq!(session.shape_count().unwrap(), 3);
        assert_eq!(managed.accepted_revision(), Some(2));

        managed.instance_mut().overrides.insert(
            "fillet_radius".into(),
            ParameterValue::Scalar(Quantity::length(100.0, LengthUnit::Millimeter)),
        );
        assert!(managed.regenerate().is_err());
        assert_eq!(managed.state(), RegenerationState::Stale);
        assert_eq!(managed.accepted_revision(), Some(2));
        assert_eq!(session.shape_count().unwrap(), 3);
        assert!(
            session
                .is_valid(managed.accepted().unwrap().shape("filleted").unwrap())
                .unwrap()
        );
    }
}
