//! Parameter values, scalar and vector expressions, derived parameters, and constraints.

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
