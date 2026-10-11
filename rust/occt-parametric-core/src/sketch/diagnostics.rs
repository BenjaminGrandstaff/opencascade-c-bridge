//! Per-constraint residual checks and curve previews for solved or diagnostic sketches.

use super::*;

/// Per-constraint diagnostics from the same residual equations as the solver.
#[derive(Clone, Debug, PartialEq)]
pub struct SketchConstraintCheck {
    pub index: usize,
    pub max_residual: Option<f64>,
    pub satisfied: bool,
    /// Spline end tangency is imposed during curve construction, not solved
    /// by a residual equation. It must not be reported as a measured zero.
    pub by_construction: bool,
}
/// Measured seed, native spline contact and circular contact, with any arc
/// domain violation kept separate from tangent alignment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SketchSplineCircleContact {
    pub points: [SketchPoint2; 3],
    pub tangent_sine: f64,
    pub arc_span_residual_mm: Option<f64>,
}

/// Measured ellipse/circular contact positions, tangent alignment and arc-domain residual.
/// Shares the contact representation with spline/circular tangency.
pub type SketchEllipseCircleContact = SketchSplineCircleContact;
