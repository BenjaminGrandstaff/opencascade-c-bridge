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

mod diagnostics;
mod profile;
mod projections;
mod solver;
mod validation;

pub use diagnostics::SketchConstraintCheck;

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

/// A smooth curve interpolated through `points` in order. Its ends are the
/// first and last points. Repeating the first point at the end makes a smooth
/// closed loop with no corner, usable alone as a profile. A `Tangent`
/// constraint at an end with a line or arc sets the spline's end direction so
/// it continues smoothly from that entity; spline-to-spline tangency is not
/// supported. Interior points move with the solver like any other point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SketchSpline {
    pub id: String,
    pub points: Vec<String>,
}

impl SketchSpline {
    fn closed(&self) -> bool {
        self.points.len() > 1 && self.points.first() == self.points.last()
    }
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
/// conic; Arc requires a circular projection. General splines are not imported.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SketchProjectionKind {
    Line,
    Circle,
    Arc,
    Ellipse,
}

/// A linked orthogonal projection of exactly one semantic source edge.
/// Generated point IDs are `id:start/end` for lines, `id:center/rim` for circles,
/// `id:center/start/end` for arcs and `id:center/major/minor` for ellipses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchProjection {
    pub id: String,
    pub input: String,
    pub edge: EdgeSelector,
    pub kind: SketchProjectionKind,
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

/// A sketch in a typed 3D plane. `profile` names an ordered, closed boundary;
/// omitted entities are construction geometry. An empty profile uses all lines
/// in their original order, or a sole circle when there are no lines/arcs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SketchDefinition {
    pub id: String,
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
    pub solved: bool,
    pub iterations: usize,
    pub max_residual: f64,
    pub free_degrees: usize,
    pub redundant_equations: usize,
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
            Self::Spline(spline) => (&spline.points[0], &spline.points[spline.points.len() - 1]),
        }
    }

    fn tangent_points(self, contact: &'a str) -> (&'a str, &'a str) {
        match self {
            Self::Line(line) => (&line.start, &line.end),
            Self::Circle(circle) => (&circle.center, contact),
            Self::Ellipse(ellipse) => (&ellipse.center, contact),
            Self::Arc(arc) => (&arc.center, contact),
            // Spline tangency shapes the spline; it is never a solver equation.
            Self::Spline(_) => unreachable!("spline tangency is applied geometrically"),
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

    /// True when a tangency involves a spline and so shapes it instead of
    /// adding a solver equation.
    fn spline_tangency(&self, constraint: &SketchConstraint) -> bool {
        matches!(constraint, SketchConstraint::Tangent { first, second, .. }
            if self.splines.iter().any(|spline| spline.id == *first || spline.id == *second))
    }

    fn spline_tangent_index(&self) -> Result<HashMap<(&str, &str), &str>, ModelError> {
        let splines = self
            .splines
            .iter()
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
                SketchProfileOperation::Offset { distance, .. } => {
                    collect_scalar_parameters(distance, names)
                }
            }
        }
        for constraint in &self.constraints {
            if let SketchConstraint::Distance { value, .. }
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
