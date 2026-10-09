//! Family definitions: parameters, expressions, selectors, feature
//! operations, requirements, and constraints.

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParameterValue {
    Scalar(Quantity),
    Integer(i64),
    Boolean(bool),
    Choice(String),
    Vector(VectorQuantity),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParameterType {
    Scalar(Dimension),
    Integer,
    Boolean,
    Choice(Vec<String>),
    Vector(Dimension),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ParameterDefinition {
    pub id: String,
    pub parameter_type: ParameterType,
    pub default: ParameterValue,
    pub minimum: Option<Quantity>,
    pub maximum: Option<Quantity>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScalarExpr {
    Literal(Quantity),
    Parameter(String),
    /// Frozen metric clearance-hole catalog; returns a length.
    Iso273ClearanceV1 {
        nominal_diameter: Box<ScalarExpr>,
        series: ClearanceSeries,
    },
    /// Frozen manufacturer cut-tap recommendations; pitch is a length.
    CarrLaneTapDrillV1 {
        nominal_diameter: Box<ScalarExpr>,
        pitch: Box<ScalarExpr>,
        system: HoleCatalogSystem,
    },
    /// Frozen nominal socket-head clearance and counterbore dimensions.
    CarrLaneSocketHeadV1 {
        nominal_diameter: Box<ScalarExpr>,
        system: HoleCatalogSystem,
        dimension: SocketHeadDimension,
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
    /// Square root of a nonnegative dimensionless value.
    SquareRoot(Box<ScalarExpr>),
    /// `base` raised to `exponent`, both dimensionless.
    Power {
        base: Box<ScalarExpr>,
        exponent: Box<ScalarExpr>,
    },
    /// sqrt(a^2 + b^2) of two values with the same dimension.
    Hypotenuse(Box<ScalarExpr>, Box<ScalarExpr>),
    /// Sine of dimensionless radians.
    Sine(Box<ScalarExpr>),
    /// Cosine of dimensionless radians.
    Cosine(Box<ScalarExpr>),
    /// Tangent of dimensionless radians.
    Tangent(Box<ScalarExpr>),
    /// Radians in [-pi/2, pi/2] of a dimensionless value in [-1, 1].
    ArcSine(Box<ScalarExpr>),
    /// Radians in [0, pi] of a dimensionless value in [-1, 1].
    ArcCosine(Box<ScalarExpr>),
    /// Radians in (-pi, pi] of the direction (x, y); same dimensions.
    ArcTangent2 {
        y: Box<ScalarExpr>,
        x: Box<ScalarExpr>,
    },
    /// `from + (to - from) * fraction`: `from` and `to` share a dimension
    /// and `fraction` is dimensionless (not limited to [0, 1]).
    Interpolate {
        from: Box<ScalarExpr>,
        to: Box<ScalarExpr>,
        fraction: Box<ScalarExpr>,
    },
    /// `value` rounded to a multiple of the positive `step`, which shares its
    /// dimension, such as a standard stock size.
    RoundToStep {
        value: Box<ScalarExpr>,
        step: Box<ScalarExpr>,
        mode: RoundingMode,
    },
    /// Length of a vector, in the vector's dimension.
    VectorLength(Box<VectorExpr>),
    /// Dot product of two vectors, at most one of them a length.
    DotProduct(Box<VectorExpr>, Box<VectorExpr>),
}

/// How `ScalarExpr::RoundToStep` picks a multiple.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    Nearest,
    Down,
    Up,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SemanticHistoryRelation {
    Generated,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// The family's named edge reference (see [`FamilyDefinition::references`]).
    Named(String),
    /// Edges chosen by `select` on `feature`'s own output, followed through
    /// every later feature like [`FaceSelector::Persistent`].
    Persistent {
        feature: String,
        select: Box<EdgeSelector>,
    },
}

impl EdgeSelector {
    /// Named references used anywhere in this selector.
    pub(crate) fn names<'a>(&'a self, names: &mut Vec<(&'a str, ReferenceUse)>) {
        match self {
            Self::Named(name) => names.push((name, ReferenceUse::Edges)),
            Self::Union(selectors) | Self::Intersection(selectors) => {
                for selector in selectors {
                    selector.names(names);
                }
            }
            Self::Difference { base, subtract } => {
                base.names(names);
                subtract.names(names);
            }
            Self::History { source, .. } => source.names(names),
            Self::Persistent { select, .. } => select.names(names),
            _ => {}
        }
    }

    pub(crate) fn dependencies<'a>(&'a self, dependencies: &mut Vec<&'a str>) {
        match self {
            Self::Persistent { feature, select } => {
                dependencies.push(feature);
                select.dependencies(dependencies);
            }
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// Faces meeting at least `minimum_count` of `faces` with G1 or better
    /// continuity. Continuity recorded on the shared edge is used as is; with
    /// an `angular_tolerance` (radians), an unrecorded edge is measured too,
    /// which finds tangent junctions that booleans create.
    TangentTo {
        faces: Box<FaceSelector>,
        minimum_count: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        angular_tolerance: Option<ScalarExpr>,
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
    /// The family's named face reference (see [`FamilyDefinition::references`]).
    Named(String),
    /// Faces chosen by `select` on `feature`'s own output, where the rule is
    /// unambiguous, then followed through every later feature to the one
    /// being built: unchanged faces carry over, modified faces map to their
    /// replacements (a split face yields every piece), and a face that a later
    /// feature removes fails, naming that feature. A stable reference that
    /// survives parameter edits, booleans, and placements downstream.
    Persistent {
        feature: String,
        select: Box<FaceSelector>,
    },
}

impl FaceSelector {
    /// Named references used anywhere in this selector.
    pub(crate) fn names<'a>(&'a self, names: &mut Vec<(&'a str, ReferenceUse)>) {
        match self {
            Self::Named(name) => names.push((name, ReferenceUse::Faces)),
            Self::AdjacentToEdges { edges, .. } => edges.names(names),
            Self::GeneratedFromEdges { source, .. } => source.names(names),
            Self::TangentTo { faces, .. } => faces.names(names),
            Self::Union(selectors) | Self::Intersection(selectors) => {
                for selector in selectors {
                    selector.names(names);
                }
            }
            Self::Difference { base, subtract } => {
                base.names(names);
                subtract.names(names);
            }
            Self::History { source, .. } => source.names(names),
            Self::Persistent { select, .. } => select.names(names),
            Self::NearestCenter { .. }
            | Self::AtExtreme { .. }
            | Self::NormalAligned { .. }
            | Self::LargestArea { .. } => {}
        }
    }

    pub(crate) fn dependencies<'a>(&'a self, dependencies: &mut Vec<&'a str>) {
        match self {
            // Added by dependencies_with_references, which sees the family.
            Self::Named(_) => {}
            Self::Persistent { feature, select } => {
                dependencies.push(feature);
                select.dependencies(dependencies);
            }
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

/// A full-diameter blind bore depth or a bore through the complete input along its
/// axis line. Through-all covers both directions from the supplied position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HoleExtent {
    Blind {
        depth: ScalarExpr,
    },
    ThroughAll,
    /// One selected bounded face, resolved against the hole input.
    UpToFace {
        face: Box<FaceSelector>,
    },
    /// Nearest whole-profile forward cutoff among the hole input's faces.
    UpToNext,
}

/// Shape below a blind hole's full-diameter bore depth.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HoleBottom {
    #[default]
    Flat,
    /// Included point angle in scalar radians (0 < angle < pi).
    /// The conical tip adds diameter / (2*tan(angle/2)) to the bore depth.
    /// The complete tip must remain within the input material.
    DrillPoint { angle_radians: ScalarExpr },
}

/// Entry recess, starting at the hole position along its axis.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThreadHandedness {
    Right,
    Left,
}

/// Caller-supplied internal-thread intent, not a standards lookup or modeled
/// helix. The hole diameter remains the explicit cylindrical bore diameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ThreadSpecification {
    pub designation: String,
    pub nominal_diameter: ScalarExpr,
    pub pitch: ScalarExpr,
    pub handedness: ThreadHandedness,
}

/// How a rib's total thickness is placed relative to its profile plane.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RibThicknessMode {
    /// Extrude from the profile plane along the supplied direction.
    #[default]
    OneSided,
    /// Place half the total thickness on each side of the profile plane.
    Centered,
}

/// The bounded planar region used before applying rib thickness.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

/// An interior sample of a smooth radius law. Position is dimensionless and
/// strictly between 0 and 1; radius is a positive length.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FilletRadiusStation {
    pub position: ScalarExpr,
    pub radius: ScalarExpr,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FilletSpineDirection {
    #[default]
    Kernel,
    Reversed,
    /// Start at the contour endpoint nearest this length-valued point.
    FromPoint {
        point: VectorExpr,
    },
}

/// End condition for a planar profile extrusion. Geometric limits must
/// terminate the entire profile strictly forward, using their finite boundaries.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtrudeExtent {
    /// The direction vector is the full length and direction of travel.
    #[default]
    Distance,
    /// Total length is split equally about the sketch plane.
    Symmetric,
    /// The vector supplies orientation; its magnitude is ignored.
    UpToFace {
        target: String,
        face: Box<FaceSelector>,
    },
    /// Nearest forward face terminating the complete profile. Crossing
    /// competing limits are ambiguous and require an explicit face selector.
    /// The vector supplies orientation; its magnitude is ignored.
    UpToNext { target: String },
}

/// Angular extent of a profile revolution. Angles are signed radians.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RevolveExtent {
    /// Begin at the source sketch plane and sweep through the signed angle.
    #[default]
    Angle,
    /// Sweep from minus half the angle to plus half about the sketch plane.
    Symmetric,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeatureOperation {
    /// Constant-width sheet with tangent-length flanges and circular bends.
    SheetMetal {
        definition: Box<SheetMetalDefinition>,
    },
    /// Flat blank linked to a directly named SheetMetal feature.
    SheetMetalFlat {
        input: String,
        neutral_factor: ScalarExpr,
    },
    Box {
        origin: VectorExpr,
        size: VectorExpr,
    },
    /// Sweeps the `profile` output (a wire, or a planar face including holes) along
    /// the `path` output (an edge or wire). Place the profile at the path's
    /// start, usually across it. A face or closed wire makes a solid. Holed
    /// faces sweep each boundary and subtract the contained inner volumes.
    Sweep {
        profile: String,
        path: String,
        #[serde(default)]
        orientation: SweepOrientation,
    },
    /// A solid through two or more sections with equal point counts. `smooth`
    /// interpolates each section as one B-spline with a corner only at its
    /// first point (an airfoil trailing edge); otherwise sections are
    /// polygons. `ruled` keeps straight lines between sections.
    Loft {
        sections: Vec<LoftSection>,
        smooth: bool,
        ruled: bool,
    },
    /// Solid loft through ordered saved planar sketch faces/wires, retaining
    /// native boundary curves. Faces must have exactly one boundary wire.
    ProfileLoft {
        #[schemars(length(min = 2, max = 1000))]
        profiles: Vec<String>,
        /// Each inner track supplies one simple profile for every outer station.
        #[serde(default)]
        #[schemars(length(max = 100))]
        holes: Vec<Vec<String>>,
        #[serde(default)]
        ruled: bool,
    },
    Cylinder {
        origin: VectorExpr,
        axis: VectorExpr,
        radius: ScalarExpr,
        height: ScalarExpr,
    },
    /// Full cone/frustum. Radii are nonnegative (not both zero), height positive.
    /// Equal radii produce a cylinder; origin is the base center.
    Cone {
        origin: VectorExpr,
        axis: VectorExpr,
        base_radius: ScalarExpr,
        top_radius: ScalarExpr,
        height: ScalarExpr,
    },
    /// Full sphere with length-valued center and positive radius.
    Sphere {
        center: VectorExpr,
        radius: ScalarExpr,
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
    /// One planar face with strictly contained, disjoint inner boundaries.
    /// Inputs are closed planar wires or single-boundary planar faces.
    PlanarRegion {
        outer: String,
        #[schemars(length(min = 1, max = 100))]
        holes: Vec<String>,
    },
    /// Sweeps a planar face or closed planar wire by a length-valued vector.
    Extrude {
        input: String,
        direction: VectorExpr,
        #[serde(default)]
        extent: ExtrudeExtent,
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
        #[serde(default)]
        extent: RevolveExtent,
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
        bottom: HoleBottom,
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
    /// Radius evolution over open tangent contours. With no interior stations,
    /// the law is linear. Otherwise OCCT smoothly interpolates the samples.
    /// Positive length-valued radii and dimensionless station positions.
    VariableFillet {
        input: String,
        edges: Vec<EdgeSelector>,
        start_radius: ScalarExpr,
        end_radius: ScalarExpr,
        #[serde(default)]
        stations: Vec<FilletRadiusStation>,
        #[serde(default)]
        spine_direction: FilletSpineDirection,
    },
    Chamfer {
        input: String,
        edges: Vec<EdgeSelector>,
        distance: ScalarExpr,
    },
    /// Merges adjacent faces on the same surface, and edges on the same
    /// curve, that booleans left split, so later shells, drafts, and
    /// selectors see one face per surface. Surfaces match within
    /// `linear_tolerance` (a length) and `angular_tolerance` (radians, in
    /// (0, pi/2)). Merged faces are recorded as modified, so persistent
    /// references follow them.
    Unify {
        input: String,
        linear_tolerance: ScalarExpr,
        angular_tolerance: ScalarExpr,
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
    /// Named references used by this operation's selectors.
    pub(crate) fn reference_names(&self) -> Vec<(&str, ReferenceUse)> {
        let mut names = Vec::new();
        match self {
            Self::Extrude {
                extent: ExtrudeExtent::UpToFace { face, .. },
                ..
            }
            | Self::Hole {
                extent: HoleExtent::UpToFace { face },
                ..
            } => face.names(&mut names),
            Self::Fillet { edges, .. }
            | Self::VariableFillet { edges, .. }
            | Self::Chamfer { edges, .. } => {
                for selector in edges {
                    selector.names(&mut names);
                }
            }
            Self::Hollow { faces, .. } | Self::Draft { faces, .. } => {
                for selector in faces {
                    selector.names(&mut names);
                }
            }
            _ => {}
        }
        names
    }

    /// Direct output dependencies, including embedded selectors.
    /// Use `FamilyDefinition::feature_inputs` to expand named references.
    pub fn dependencies(&self) -> Vec<&str> {
        match self {
            Self::Extrude { input, extent, .. } => {
                let mut dependencies = vec![input.as_str()];
                match extent {
                    ExtrudeExtent::UpToFace { target, face } => {
                        dependencies.push(target);
                        face.dependencies(&mut dependencies);
                    }
                    ExtrudeExtent::UpToNext { target } => dependencies.push(target),
                    _ => {}
                }
                dependencies
            }
            Self::Hole { input, extent, .. } => {
                let mut dependencies = vec![input.as_str()];
                if let HoleExtent::UpToFace { face } = extent {
                    face.dependencies(&mut dependencies);
                }
                dependencies
            }
            Self::ProfileLoft {
                profiles, holes, ..
            } => profiles
                .iter()
                .chain(holes.iter().flatten())
                .map(String::as_str)
                .collect(),
            Self::PlanarRegion { outer, holes } => std::iter::once(outer.as_str())
                .chain(holes.iter().map(String::as_str))
                .collect(),
            Self::Sweep { profile, path, .. } => vec![profile, path],
            Self::SheetMetalFlat { input, .. }
            | Self::Translate { input, .. }
            | Self::Rotate { input, .. }
            | Self::Revolve { input, .. }
            | Self::Unify { input, .. } => vec![input],
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
            Self::SheetMetal { .. }
            | Self::Loft { .. }
            | Self::Box { .. }
            | Self::Cylinder { .. }
            | Self::Cone { .. }
            | Self::Sphere { .. }
            | Self::SketchFace { .. }
            | Self::SketchWire { .. }
            | Self::SketchOpenWire { .. } => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeatureDefinition {
    pub id: String,
    pub operation: FeatureOperation,
}

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

/// How a swept profile turns as it follows its path.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SweepOrientation {
    /// Least twist; the usual choice.
    #[default]
    CorrectedFrenet,
    /// Follows the path's curvature frame.
    Frenet,
    /// Keeps the profile's normal-side axis along a dimensionless direction.
    Binormal { direction: VectorExpr },
    /// Never rotates: every section stays parallel to the first.
    Fixed,
}

/// A closed planar outline for a loft, in dimensionless profile units (for an
/// airfoil, fractions of chord). Each profile point (u, v) is rotated by
/// `rotation_radians` about `pivot`, counterclockwise from `x_axis` toward
/// `y_axis`, scaled by the length `scale`, and placed at
/// `origin + scale * (u * x_axis + v * y_axis)`. The axes are dimensionless,
/// nonzero, and perpendicular. The first point is not repeated at the end.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoftSection {
    pub profile: Vec<[f64; 2]>,
    pub origin: VectorExpr,
    pub x_axis: VectorExpr,
    pub y_axis: VectorExpr,
    pub scale: ScalarExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_radians: Option<ScalarExpr>,
    #[serde(default, skip_serializing_if = "is_origin")]
    pub pivot: [f64; 2],
}

fn is_origin(point: &[f64; 2]) -> bool {
    *point == [0.0, 0.0]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DerivedParameterDefinition {
    pub id: String,
    pub dimension: Dimension,
    pub expression: ScalarExpr,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DerivedVectorParameterDefinition {
    pub id: String,
    pub dimension: Dimension,
    pub expression: VectorExpr,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintRelation {
    LessOrEqual,
    GreaterOrEqual,
    Equal { tolerance: Quantity },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ParameterConstraint {
    pub id: String,
    pub statement: String,
    pub left: ScalarExpr,
    pub relation: ConstraintRelation,
    pub right: ScalarExpr,
    pub provenance: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// Face and edge rules declared once and used by name through
    /// `FaceSelector::Named` and `EdgeSelector::Named`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<NamedReference>,
    /// Linear RGB colors, channels in [0, 1], by feature id: a feature
    /// colors the faces it creates, and later features carry those colors
    /// to the faces they keep or modify. Exported as STEP face colors.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub feature_colors: BTreeMap<String, [f64; 3]>,
    /// Engineering assumptions that requirements trace to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<Assumption>,
}

/// A face or edge rule declared once in a family and used by name, so the
/// rule (often a persistent reference) is written and edited in one place.
/// A reference's rule cannot itself use named references.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NamedReference {
    pub name: String,
    pub target: ReferenceTarget,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceTarget {
    Faces(FaceSelector),
    Edges(EdgeSelector),
}

/// Whether a named reference is used where faces or edges are expected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceUse {
    Faces,
    Edges,
}

impl ReferenceTarget {
    pub(crate) fn kind(&self) -> ReferenceUse {
        match self {
            Self::Faces(_) => ReferenceUse::Faces,
            Self::Edges(_) => ReferenceUse::Edges,
        }
    }

    pub(crate) fn dependencies<'a>(&'a self, dependencies: &mut Vec<&'a str>) {
        match self {
            Self::Faces(selector) => selector.dependencies(dependencies),
            Self::Edges(selector) => selector.dependencies(dependencies),
        }
    }

    pub(crate) fn names<'a>(&'a self, names: &mut Vec<(&'a str, ReferenceUse)>) {
        match self {
            Self::Faces(selector) => selector.names(names),
            Self::Edges(selector) => selector.names(names),
        }
    }
}

/// A family's named references by name.
pub(crate) type References<'a> = HashMap<&'a str, &'a NamedReference>;

/// Indexes a family's named references by name. O(references).
pub(crate) fn reference_map(family: &FamilyDefinition) -> References<'_> {
    family
        .references
        .iter()
        .map(|reference| (reference.name.as_str(), reference))
        .collect()
}

impl FamilyDefinition {
    /// Validated, sorted input-output identities for every feature, including
    /// dependencies introduced by named references. Does not generate geometry.
    pub fn feature_inputs(
        &self,
    ) -> Result<std::collections::BTreeMap<&str, Vec<&str>>, ModelError> {
        validate_definition(self)?;
        let references = reference_map(self);
        Ok(self
            .features
            .iter()
            .map(|feature| {
                let mut inputs = dependencies_with_references(feature, &references);
                inputs.sort_unstable();
                inputs.dedup();
                (feature.id.as_str(), inputs)
            })
            .collect())
    }
}

/// Feature dependencies, including those of the named references it uses.
/// O(selectors + used references).
pub(crate) fn dependencies_with_references<'a>(
    feature: &'a FeatureDefinition,
    references: &References<'a>,
) -> Vec<&'a str> {
    let mut dependencies = feature.operation.dependencies();
    for (name, _) in feature.operation.reference_names() {
        if let Some(reference) = references.get(name) {
            reference.target.dependencies(&mut dependencies);
        }
    }
    dependencies
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartInstance<'definition> {
    pub id: String,
    pub definition: &'definition FamilyDefinition,
    pub overrides: HashMap<String, ParameterValue>,
    pub provenance: String,
}
