//! Constraint-solved two-dimensional sketches with exact circular geometry.

use super::*;
use crate::assembly::{add, cross, dot, scale, unit};
use crate::sparse::SparseJacobian;
use occt_bridge::CurveSegment;
use std::collections::{HashMap, HashSet};

const MAX_ITERATIONS: usize = 100;
const RESIDUAL_TOLERANCE: f64 = 1e-9;
const DIFFERENCE_STEP: f64 = 1e-6;
const PIVOT_TOLERANCE: f64 = 1e-10;

mod diagnosis;
mod diagnostics;
mod geometry;
mod patterns;
mod profile;
mod projections;
mod shapes;
mod solver;
mod validation;

pub use diagnosis::{
    SketchConflictGroup, SketchCoordinateAxis, SketchDiagnosis, SketchEquationSource,
    SketchFreeCoordinate, SketchFreeMotion, SketchPointMotion,
};
pub use diagnostics::{
    SketchConstraintCheck, SketchEllipseCircleContact, SketchSplineCircleContact,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SketchPoint {
    pub id: String,
    pub x: ScalarExpr,
    pub y: ScalarExpr,
    #[serde(default)]
    pub fixed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SketchLine {
    pub id: String,
    pub start: String,
    pub end: String,
}

/// A full circle. The center-to-rim distance defines its radius.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SketchCircle {
    pub id: String,
    pub center: String,
    pub rim: String,
}

/// Full ellipse defined by center and positive major/minor axis endpoints.
/// The solver keeps the axes perpendicular; major radius must be >= minor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SketchEllipse {
    pub id: String,
    pub center: String,
    pub major: String,
    pub minor: String,
}

/// Saved edits to the solved profile. Constraints refer to the source entities;
/// these operations derive the profile used by features, in recorded order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SketchProfileOperation {
    /// Fractions of the entity's native parameter interval, in [0,1].
    Trim {
        entity: String,
        first: ScalarExpr,
        last: ScalarExpr,
    },
    /// Nonnegative extension distances; analytic curves continue naturally,
    /// splines use native tangent-continuous extension to the tangent targets.
    Extend {
        entity: String,
        start: ScalarExpr,
        end: ScalarExpr,
    },
    /// Native fillet at a named remaining internal corner; neighbors may be
    /// lines, arcs, splines or trimmed ellipses.
    Fillet { point: String, radius: ScalarExpr },
    /// Native chamfer distances along incoming/outgoing profile traversal.
    Chamfer {
        point: String,
        incoming: ScalarExpr,
        outgoing: ScalarExpr,
    },
    /// Signed planar profile offset: positive expands a closed profile;
    /// for an open profile positive is to the right of traversal.
    Offset {
        distance: ScalarExpr,
        #[serde(default)]
        join: SketchOffsetJoin,
    },
}
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SketchOffsetJoin {
    #[default]
    Arc,
    Intersection,
}

/// A circular arc; the solver enforces equal start/end radii automatically.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SketchArc {
    pub id: String,
    pub center: String,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub clockwise: bool,
}

/// A curve defined by named points. Without `basis`, it interpolates them;
/// repeating the first at the end creates a smooth periodic loop, and an
/// endpoint `Tangent` sets the interpolator's end direction. With `basis`,
/// these points are control poles of a clamped or periodic B-spline;
/// endpoint tangency is measured through adjacent poles. Repeating the first
/// pole closes the curve without promising a smooth periodic seam.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SketchSpline {
    pub id: String,
    pub points: Vec<String>,
    /// When present, `points` are control poles, not interpolation points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<SketchBSplineBasis>,
    /// Named geometric endpoints for a non-clamped basis; solver ties them
    /// to the native active-domain endpoints, independently of control poles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<SketchSplineEndpoints>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchSplineEndpoints {
    pub start: String,
    pub end: String,
}

/// Explicit clamped, non-clamped, or periodic B-spline basis. Empty weights mean all ones.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchBSplineBasis {
    #[serde(default)]
    pub non_clamped: bool,
    #[serde(default)]
    pub periodic: bool,
    #[schemars(range(min = 1, max = 25))]
    pub degree: i32,
    #[schemars(length(min = 2, max = 10002))]
    pub knots: Vec<f64>,
    pub multiplicities: Vec<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weights: Vec<ScalarExpr>,
}

impl SketchBSplineBasis {
    pub(crate) fn weights(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Vec<f64>, ModelError> {
        self.weights
            .iter()
            .map(|w| {
                let value = scalar(w, parameters, Dimension::Scalar)?;
                if value <= 0. {
                    return Err(ModelError::new("B-spline weights must be positive"));
                }
                Ok(value)
            })
            .collect()
    }
}

impl SketchSpline {
    pub(crate) fn explicit_segment(
        &self,
        poles: Vec<Vec3>,
        weights: Vec<f64>,
    ) -> Result<CurveSegment, ModelError> {
        let b = self.basis.as_ref().expect("explicit B-spline basis");
        Ok(CurveSegment::BSpline {
            non_clamped: b.non_clamped,
            periodic: b.periodic,
            poles,
            degree: b.degree,
            knots: b.knots.clone(),
            multiplicities: b.multiplicities.clone(),
            weights,
        })
    }
    fn closed(&self) -> bool {
        self.basis.as_ref().is_some_and(|b| b.periodic)
            || self.endpoints.as_ref().map_or_else(
                || self.points.len() > 1 && self.points.first() == self.points.last(),
                |e| e.start == e.end,
            )
    }
}

/// Side of the supporting line, oriented from start to end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SketchLineSide {
    Left,
    Right,
}

/// Internal tangency means the first supporting circle contains the second.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SketchCircleTangency {
    External,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SketchConstraint {
    /// Signed angle from the first directed line to the second, in radians.
    Angle {
        first: String,
        second: String,
        value: ScalarExpr,
    },
    Radius {
        curve: String,
        value: ScalarExpr,
    },
    Diameter {
        curve: String,
        value: ScalarExpr,
    },
    /// Two points reflected across the infinite line `axis`.
    Symmetric {
        first: String,
        second: String,
        axis: String,
    },
    /// Point on the supporting line, circle, arc span, or ellipse.
    PointOnCurve {
        point: String,
        curve: String,
    },
    /// Tangency at a shared named endpoint (or a circle's rim point).
    Tangent {
        first: String,
        second: String,
        point: String,
    },
    /// Circle/arc tangency to the infinite supporting line, without a shared point.
    LineCircleTangent {
        line: String,
        circle: String,
        side: SketchLineSide,
    },
    /// Independent line tangency at a seeded point on the bounded native spline.
    LineSplineTangent {
        line: String,
        spline: String,
        point: String,
    },
    /// Tangency at a seeded contact on two distinct bounded native splines.
    SplineSplineTangent {
        first: String,
        second: String,
        point: String,
    },
    /// Tangency at a seeded contact on two distinct full native ellipses.
    EllipseEllipseTangent {
        first: String,
        second: String,
        point: String,
    },
    /// Tangency at a seeded contact on a bounded native spline and full ellipse.
    SplineEllipseTangent {
        spline: String,
        ellipse: String,
        point: String,
    },
    /// Independent spline/circle tangency, with contact inside any directed arc span.
    SplineCircleTangent {
        spline: String,
        circle: String,
        point: String,
    },
    /// Seeded full-ellipse/circle tangency, restricted to any directed arc span.
    EllipseCircleTangent {
        ellipse: String,
        circle: String,
        point: String,
    },
    /// Full ellipse tangency to the infinite supporting line, without a shared point.
    LineEllipseTangent {
        line: String,
        ellipse: String,
        side: SketchLineSide,
    },
    /// Circle/arc tangency, requiring contacts inside any directed arc spans.
    CircleCircleTangent {
        first: String,
        second: String,
        mode: SketchCircleTangency,
    },
    Coincident {
        first: String,
        second: String,
    },
    Horizontal {
        line: String,
    },
    Vertical {
        line: String,
    },
    Parallel {
        first: String,
        second: String,
    },
    Perpendicular {
        first: String,
        second: String,
    },
    /// Equal supporting-circle radii of two distinct circles or arcs.
    EqualRadius {
        first: String,
        second: String,
    },
    /// Shared centre of two distinct circles, arcs, or ellipses.
    Concentric {
        first: String,
        second: String,
    },
    /// Point at the arithmetic midpoint of a named line segment.
    Midpoint {
        point: String,
        line: String,
    },
    /// Signed perpendicular distance to the infinite supporting line.
    /// Positive is left of its start-to-end direction; zero lies on the line.
    PointLineDistance {
        point: String,
        line: String,
        value: ScalarExpr,
    },
    EqualLength {
        first: String,
        second: String,
    },
    Distance {
        first: String,
        second: String,
        value: ScalarExpr,
    },
    /// Signed sketch-x offset `x(second) − x(first)` (schema 122).
    HorizontalDistance {
        first: String,
        second: String,
        value: ScalarExpr,
    },
    /// Signed sketch-y offset `y(second) − y(first)` (schema 122).
    VerticalDistance {
        first: String,
        second: String,
        value: ScalarExpr,
    },
    /// Signed angle of a directed line from the sketch +x axis, in radians
    /// within [−π, π] (schema 122).
    LineAngle {
        line: String,
        value: ScalarExpr,
    },
    /// Sweep of an arc along its direction, in radians within (0, 2π)
    /// (schema 122).
    ArcAngle {
        arc: String,
        value: ScalarExpr,
    },
    /// Length of a line, arc, circle, full ellipse or spline; ellipse and
    /// spline lengths are measured on the native curve (schema 122).
    CurveLength {
        curve: String,
        value: ScalarExpr,
    },
    /// Two such curves with equal lengths (schema 122).
    EqualCurveLength {
        first: String,
        second: String,
    },
    /// The second line lies on the infinite supporting line of the first
    /// (schema 122).
    Collinear {
        first: String,
        second: String,
    },
    /// Two circles or arcs on the same circle: shared centre and radius
    /// (schema 122).
    Coradial {
        first: String,
        second: String,
    },
}

/// A common closed outline expanded into ordinary named points, lines, arcs
/// and constraints before solving (schema 123). Generated IDs are
/// `{id}:{part}`; listing the shape's `id` in `profile` or `holes` stands for
/// its boundary entities in traversal order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SketchShape {
    /// Rectangle with corner `first` and the opposite corner `opposite`.
    /// Generates corners `{id}:corner1` (along side0 from `first`) and
    /// `{id}:corner3`, and sides `{id}:side0`..`{id}:side3` from `first`.
    /// Sides follow the sketch axes, or side0 has the signed `angle` from the
    /// sketch +x axis (radians).
    Rectangle {
        id: String,
        first: String,
        opposite: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        angle: Option<ScalarExpr>,
    },
    /// Straight slot (obround) around the segment between the end-cap centres
    /// `start` and `end`, with cap `radius`. Generates side points
    /// `{id}:start_right`, `{id}:end_right`, `{id}:end_left`,
    /// `{id}:start_left`, sides `{id}:right` and `{id}:left`, and caps
    /// `{id}:end_cap` and `{id}:start_cap`, tangent at every join.
    Slot {
        id: String,
        start: String,
        end: String,
        radius: ScalarExpr,
    },
    /// Regular polygon of `sides` vertices inscribed in the circle about
    /// `center` through `vertex`. Generates vertices `{id}:vertex1`.. after
    /// `vertex`, sides `{id}:side0`.. from `vertex`, and the construction
    /// circle `{id}:circle`.
    Polygon {
        id: String,
        center: String,
        vertex: String,
        #[schemars(range(min = 3, max = 1000))]
        sides: u32,
    },
}

/// A driven (reference) dimension, measured on the solved sketch. It never
/// constrains geometry and is reported beside the constraints (schema 122).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchMeasurement {
    pub id: String,
    pub measure: SketchMeasure,
}

/// What a reference dimension measures; the same quantities, signs and units
/// as the matching driving constraints.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SketchMeasure {
    Distance { first: String, second: String },
    HorizontalDistance { first: String, second: String },
    VerticalDistance { first: String, second: String },
    PointLineDistance { point: String, line: String },
    Angle { first: String, second: String },
    LineAngle { line: String },
    ArcAngle { arc: String },
    Radius { curve: String },
    Diameter { curve: String },
    CurveLength { curve: String },
}

impl SketchMeasure {
    /// The driving constraint whose residual at a zero target is this measure.
    pub(crate) fn constraint(&self) -> SketchConstraint {
        let zero = |dimension: Dimension| {
            ScalarExpr::Literal(match dimension {
                Dimension::Length => Quantity::length(0.0, LengthUnit::Millimeter),
                _ => Quantity::scalar(0.0),
            })
        };
        let (length, angle) = (zero(Dimension::Length), zero(Dimension::Scalar));
        match self.clone() {
            Self::Distance { first, second } => SketchConstraint::Distance {
                first,
                second,
                value: length,
            },
            Self::HorizontalDistance { first, second } => SketchConstraint::HorizontalDistance {
                first,
                second,
                value: length,
            },
            Self::VerticalDistance { first, second } => SketchConstraint::VerticalDistance {
                first,
                second,
                value: length,
            },
            Self::PointLineDistance { point, line } => SketchConstraint::PointLineDistance {
                point,
                line,
                value: length,
            },
            Self::Angle { first, second } => SketchConstraint::Angle {
                first,
                second,
                value: angle,
            },
            Self::LineAngle { line } => SketchConstraint::LineAngle { line, value: angle },
            Self::ArcAngle { arc } => SketchConstraint::ArcAngle { arc, value: angle },
            Self::Radius { curve } => SketchConstraint::Radius {
                curve,
                value: length,
            },
            Self::Diameter { curve } => SketchConstraint::Diameter {
                curve,
                value: length,
            },
            Self::CurveLength { curve } => SketchConstraint::CurveLength {
                curve,
                value: length,
            },
        }
    }

    /// Whether the measured value is an angle in radians rather than a length.
    pub fn is_angle(&self) -> bool {
        matches!(
            self,
            Self::Angle { .. } | Self::LineAngle { .. } | Self::ArcAngle { .. }
        )
    }
}

/// Linked copies of sketch entities, generated before solving. Copy IDs are
/// `{pattern}:{k}:{source}` for instance k (mirror uses k = 1); later patterns
/// and the profile may name them. Copy points are solver unknowns tied to
/// their sources by transform equations, so source edits move every copy.
/// Source constraints are not duplicated; copies are rigid images.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SketchPattern {
    /// Reflection across the infinite line `axis`. Copies are reversed so a
    /// half profile followed by its mirror, listed in reverse, forms one loop.
    /// `on_axis` points are shared rather than copied and held on the axis.
    Mirror {
        id: String,
        #[schemars(length(min = 1, max = 1000))]
        entities: Vec<String>,
        axis: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        on_axis: Vec<String>,
    },
    /// `count` instances (source included) spaced along the start-to-end
    /// direction of `direction`. Negative spacing reverses the direction.
    Linear {
        id: String,
        #[schemars(length(min = 1, max = 1000))]
        entities: Vec<String>,
        direction: String,
        spacing: ScalarExpr,
        #[schemars(range(min = 2, max = 1000))]
        count: u32,
    },
    /// `count` instances (source included) rotated about `center` by `angle`
    /// radians per step, counterclockwise; omitted means a full equal turn.
    Circular {
        id: String,
        #[schemars(length(min = 1, max = 1000))]
        entities: Vec<String>,
        center: String,
        #[schemars(range(min = 2, max = 1000))]
        count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        angle: Option<ScalarExpr>,
    },
}

/// Attached sketch planes or resolution errors, keyed by sketch feature ID.
pub type SketchSupportPlanes = HashMap<String, Result<ResolvedDatum, ModelError>>;

/// A semantic planar-face attachment. Local sketch zero is the selected face's
/// area centre plus `offset` along its oriented normal. The sketch's explicit
/// X axis must lie in this plane; its origin/Y axis are replaced by the support.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchFaceSupport {
    pub input: String,
    pub face: FaceSelector,
    #[serde(default = "zero_support_offset")]
    pub offset: ScalarExpr,
}
fn zero_support_offset() -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter))
}

/// Expected analytic type of a projected source edge. Ellipse requires a full
/// conic; Arc requires a circular projection. Spline imports exact rational
/// B-spline geometry from a finite standard edge, including trimmed conics.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SketchProjectionKind {
    Line,
    Circle,
    Arc,
    Ellipse,
    Spline,
    /// Connected selection of up to 1000 edges, imported as one rational spline.
    Chain,
}

/// A linked orthogonal projection of a semantic source edge, or a connected
/// multi-edge selection for `Chain`.
/// Generated point IDs are `id:start/end` for lines, `id:center/rim` for circles,
/// `id:center/start/end` for arcs and `id:center/major/minor` for ellipses.
/// Spline/chain imports expose stable `id:start/end`; interior `id:pole:N` points are runtime
/// data whose count can change with the source edge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchProjection {
    pub id: String,
    pub input: String,
    pub edge: EdgeSelector,
    pub kind: SketchProjectionKind,
    /// Named fixed references to junctions between distinct selected edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 1000))]
    pub junctions: Vec<SketchProjectionJunction>,
    /// Named fixed crossings with another projection, in sketch coordinates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 1000))]
    pub intersections: Vec<SketchProjectionIntersection>,
}

/// A uniquely matched source-space junction, exposed as `projection:junction:id`.
/// The hint and radius are lengths in located family coordinates, before
/// projection into the sketch plane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchProjectionJunction {
    pub id: String,
    pub near: VectorExpr,
    pub maximum_distance: ScalarExpr,
}

/// A unique crossing of two finite projected curves. The hint is a length vector
/// in sketch-local coordinates (z must be zero); the radius is a positive length.
/// Exposed as `projection:intersection:id`. All projected curve kinds are supported
/// for isolated crossings. Profile edits do not change these references.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchProjectionIntersection {
    pub id: String,
    pub other: String,
    pub near: VectorExpr,
    pub maximum_distance: ScalarExpr,
}

/// Runtime sketch snapshot, with fixed projected geometry and its native plane.
/// The saved source definition retains the links; this is inspection/build data.
#[derive(Clone, Debug)]
pub struct ResolvedSketch {
    pub sketch: SketchDefinition,
    pub plane: Option<ResolvedDatum>,
}
/// Resolved sketch snapshots or per-feature resolution errors.
pub type ResolvedSketches = HashMap<String, Result<ResolvedSketch, ModelError>>;

/// Acceptance of a converged sketch when constructing native geometry.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SketchAcceptance {
    /// Require satisfied equations; remaining free motion is allowed.
    #[default]
    Solved,
    /// Also require zero local free degrees, including construction points.
    FullyConstrained,
}
impl SketchAcceptance {
    fn is_default(&self) -> bool {
        *self == Self::Solved
    }
}

/// A sketch in a typed 3D plane. `profile` names an ordered, closed boundary;
/// omitted entities are construction geometry. An empty profile uses all lines
/// in their original order, or a sole circle when there are no lines/arcs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SketchDefinition {
    pub id: String,
    #[serde(default, skip_serializing_if = "SketchAcceptance::is_default")]
    pub acceptance: SketchAcceptance,
    /// Optional plane datum in the owning family. Its origin/normal replace
    /// `origin`/`y_axis`; `x_axis` must be perpendicular to the datum normal.
    #[serde(default)]
    pub datum_plane: Option<String>,
    /// Optional face attachment, mutually exclusive with `datum_plane`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_support: Option<SketchFaceSupport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 1000))]
    pub projections: Vec<SketchProjection>,
    pub origin: VectorExpr,
    pub x_axis: VectorExpr,
    pub y_axis: VectorExpr,
    pub points: Vec<SketchPoint>,
    pub lines: Vec<SketchLine>,
    #[serde(default)]
    pub circles: Vec<SketchCircle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ellipses: Vec<SketchEllipse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profile_operations: Vec<SketchProfileOperation>,
    #[serde(default)]
    pub arcs: Vec<SketchArc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub splines: Vec<SketchSpline>,
    #[serde(default)]
    pub profile: Vec<String>,
    /// Inner loops cut from the face output, each an ordered closed boundary
    /// like `profile`. Face outputs only; wire outputs reject holes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 1000))]
    pub holes: Vec<Vec<String>>,
    /// Rectangles, slots and regular polygons, expanded before patterns
    /// (schema 123).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 1000))]
    pub shapes: Vec<SketchShape>,
    /// Driven (reference) dimensions measured after solving (schema 122).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 10000))]
    pub measurements: Vec<SketchMeasurement>,
    /// Linked entity copies, expanded in order before solving.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 100))]
    pub patterns: Vec<SketchPattern>,
    #[serde(default)]
    pub constraints: Vec<SketchConstraint>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SketchPoint2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SketchSolution {
    pub points: HashMap<String, SketchPoint2>,
    /// Evaluated dimensionless weights for explicit spline previews and profiles.
    pub spline_weights: HashMap<String, Vec<f64>>,
    pub solved: bool,
    pub iterations: usize,
    pub max_residual: f64,
    pub free_degrees: usize,
    pub redundant_equations: usize,
    pub diagnosis: SketchDiagnosis,
}

#[derive(Clone, Copy)]
enum Entity<'a> {
    Line(&'a SketchLine),
    Circle(&'a SketchCircle),
    Ellipse(&'a SketchEllipse),
    Arc(&'a SketchArc),
    Spline(&'a SketchSpline),
}

impl<'a> Entity<'a> {
    fn id(self) -> &'a str {
        match self {
            Self::Line(l) => &l.id,
            Self::Circle(c) => &c.id,
            Self::Ellipse(e) => &e.id,
            Self::Arc(a) => &a.id,
            Self::Spline(s) => &s.id,
        }
    }
    fn endpoints(self) -> (&'a str, &'a str) {
        match self {
            Self::Line(line) => (&line.start, &line.end),
            Self::Circle(circle) => (&circle.rim, &circle.rim),
            Self::Ellipse(ellipse) => (&ellipse.major, &ellipse.major),
            Self::Arc(arc) => (&arc.start, &arc.end),
            // Closed periodic profiles have no named geometric seam endpoint.
            // This private identity is used only for standalone cycle checks.
            Self::Spline(s) if s.basis.as_ref().is_some_and(|b| b.periodic) => {
                (&s.points[0], &s.points[0])
            }
            Self::Spline(s) => match &s.endpoints {
                Some(e) => (&e.start, &e.end),
                None => (&s.points[0], &s.points[s.points.len() - 1]),
            },
        }
    }

    fn tangent_points(self, contact: &'a str) -> (&'a str, &'a str) {
        match self {
            Self::Line(line) => (&line.start, &line.end),
            Self::Circle(circle) => (&circle.center, contact),
            Self::Ellipse(ellipse) => (&ellipse.center, contact),
            Self::Arc(arc) => (&arc.center, contact),
            // Spline tangency shapes the spline; it is never a solver equation.
            Self::Spline(s) if s.basis.is_some() => {
                if s.points[0] == contact {
                    (&s.points[0], &s.points[1])
                } else {
                    (&s.points[s.points.len() - 2], &s.points[s.points.len() - 1])
                }
            }
            Self::Spline(_) => {
                unreachable!("interpolated spline tangency is applied geometrically")
            }
        }
    }
}

impl SketchDefinition {
    fn entities(&self) -> HashMap<&str, Entity<'_>> {
        self.lines
            .iter()
            .map(|line| (line.id.as_str(), Entity::Line(line)))
            .chain(
                self.circles
                    .iter()
                    .map(|circle| (circle.id.as_str(), Entity::Circle(circle))),
            )
            .chain(
                self.ellipses
                    .iter()
                    .map(|e| (e.id.as_str(), Entity::Ellipse(e))),
            )
            .chain(
                self.arcs
                    .iter()
                    .map(|arc| (arc.id.as_str(), Entity::Arc(arc))),
            )
            .chain(
                self.splines
                    .iter()
                    .map(|spline| (spline.id.as_str(), Entity::Spline(spline))),
            )
            .collect()
    }

    /// True when a tangency involves an interpolated spline and shapes it instead of
    /// adding a solver equation.
    fn spline_tangency(&self, constraint: &SketchConstraint) -> bool {
        matches!(constraint, SketchConstraint::Tangent { first, second, .. }
            if self.splines.iter().any(|spline| spline.basis.is_none() && (spline.id == *first || spline.id == *second)))
    }

    fn spline_tangent_index(&self) -> Result<HashMap<(&str, &str), &str>, ModelError> {
        let splines = self
            .splines
            .iter()
            .filter(|s| s.basis.is_none())
            .map(|s| s.id.as_str())
            .collect::<HashSet<_>>();
        let mut result = HashMap::new();
        for constraint in &self.constraints {
            if let SketchConstraint::Tangent {
                first,
                second,
                point,
            } = constraint
            {
                for (s, n) in [(first, second), (second, first)] {
                    if splines.contains(s.as_str())
                        && result
                            .insert((s.as_str(), point.as_str()), n.as_str())
                            .is_some()
                    {
                        return Err(ModelError::new(
                            "a spline endpoint has more than one tangent neighbor",
                        ));
                    }
                }
            }
        }
        Ok(result)
    }

    pub(crate) fn collect_parameters<'a>(&'a self, names: &mut HashSet<&'a str>) {
        for projection in &self.projections {
            collect_edge_selector_parameters(&projection.edge, names);
            for intersection in &projection.intersections {
                collect_vector_parameters(&intersection.near, names);
                collect_scalar_parameters(&intersection.maximum_distance, names);
            }
            for junction in &projection.junctions {
                collect_vector_parameters(&junction.near, names);
                collect_scalar_parameters(&junction.maximum_distance, names);
            }
        }
        collect_vector_parameters(&self.x_axis, names);
        if self.datum_plane.is_none() {
            if let Some(support) = &self.face_support {
                collect_scalar_parameters(&support.offset, names);
                collect_face_selector_parameters(&support.face, names);
            }
            collect_vector_parameters(&self.origin, names);
            collect_vector_parameters(&self.y_axis, names);
        }
        for point in &self.points {
            collect_scalar_parameters(&point.x, names);
            collect_scalar_parameters(&point.y, names);
        }
        for operation in &self.profile_operations {
            match operation {
                SketchProfileOperation::Trim { first, last, .. } => {
                    collect_scalar_parameters(first, names);
                    collect_scalar_parameters(last, names);
                }
                SketchProfileOperation::Extend { start, end, .. } => {
                    collect_scalar_parameters(start, names);
                    collect_scalar_parameters(end, names);
                }
                SketchProfileOperation::Fillet { radius, .. } => {
                    collect_scalar_parameters(radius, names)
                }
                SketchProfileOperation::Chamfer {
                    incoming, outgoing, ..
                } => {
                    collect_scalar_parameters(incoming, names);
                    collect_scalar_parameters(outgoing, names);
                }
                SketchProfileOperation::Offset { distance, .. } => {
                    collect_scalar_parameters(distance, names)
                }
            }
        }
        for pattern in &self.patterns {
            match pattern {
                SketchPattern::Linear { spacing, .. } => collect_scalar_parameters(spacing, names),
                SketchPattern::Circular {
                    angle: Some(angle), ..
                } => collect_scalar_parameters(angle, names),
                _ => {}
            }
        }
        for shape in &self.shapes {
            match shape {
                SketchShape::Rectangle {
                    angle: Some(angle), ..
                } => collect_scalar_parameters(angle, names),
                SketchShape::Slot { radius, .. } => collect_scalar_parameters(radius, names),
                _ => {}
            }
        }
        for spline in &self.splines {
            if let Some(basis) = &spline.basis {
                for weight in &basis.weights {
                    collect_scalar_parameters(weight, names);
                }
            }
        }
        for constraint in &self.constraints {
            if let SketchConstraint::Distance { value, .. }
            | SketchConstraint::HorizontalDistance { value, .. }
            | SketchConstraint::VerticalDistance { value, .. }
            | SketchConstraint::LineAngle { value, .. }
            | SketchConstraint::ArcAngle { value, .. }
            | SketchConstraint::CurveLength { value, .. }
            | SketchConstraint::PointLineDistance { value, .. }
            | SketchConstraint::Angle { value, .. }
            | SketchConstraint::Radius { value, .. }
            | SketchConstraint::Diameter { value, .. } = constraint
            {
                collect_scalar_parameters(value, names);
            }
        }
    }
}

#[cfg(test)]
mod tests;

fn line_length((a, b): (SketchPoint2, SketchPoint2)) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

fn arc_middle(arc: &SketchArc, solution: &SketchSolution) -> SketchPoint2 {
    let center = solution.points[&arc.center];
    let (start, end) = (solution.points[&arc.start], solution.points[&arc.end]);
    let angle = (start.y - center.y).atan2(start.x - center.x);
    let end_angle = (end.y - center.y).atan2(end.x - center.x);
    let sweep = if arc.clockwise {
        -((angle - end_angle).rem_euclid(std::f64::consts::TAU))
    } else {
        (end_angle - angle).rem_euclid(std::f64::consts::TAU)
    };
    let radius = line_length((center, start));
    let middle_angle = angle + sweep / 2.0;
    SketchPoint2 {
        x: center.x + radius * middle_angle.cos(),
        y: center.y + radius * middle_angle.sin(),
    }
}

fn max_abs(values: &[f64]) -> f64 {
    values
        .iter()
        .fold(0.0_f64, |largest, value| largest.max(value.abs()))
}
