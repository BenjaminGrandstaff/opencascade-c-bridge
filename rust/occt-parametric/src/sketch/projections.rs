//! Structural expansion of declared projections; seeds validate IDs/types only.
//! O(sketch definition size + projections) cloning/storage; never used as solved data.
use super::*;
impl SketchDefinition {
    pub(crate) fn projection_seed(&self) -> Result<Self, ModelError> {
        if self.projections.len() > 1000 {
            return Err(ModelError::new("sketch projections exceed 1000"));
        }
        let mut resolved = self.clone();
        resolved.projections.clear();
        for projection in &self.projections {
            if projection.id.is_empty() || projection.input.is_empty() {
                return Err(ModelError::new(
                    "sketch projections require nonempty IDs and source features",
                ));
            }
            let id = &projection.id;
            let mut point = |suffix: &str, x: f64, y: f64| {
                let name = format!("{id}:{suffix}");
                resolved.points.push(SketchPoint {
                    id: name.clone(),
                    x: ScalarExpr::Literal(Quantity::length(x, LengthUnit::Millimeter)),
                    y: ScalarExpr::Literal(Quantity::length(y, LengthUnit::Millimeter)),
                    fixed: true,
                });
                name
            };
            match projection.kind {
                SketchProjectionKind::Line => resolved.lines.push(SketchLine {
                    id: id.clone(),
                    start: point("start", 0., 0.),
                    end: point("end", 1., 0.),
                }),
                SketchProjectionKind::Circle => resolved.circles.push(SketchCircle {
                    id: id.clone(),
                    center: point("center", 0., 0.),
                    rim: point("rim", 1., 0.),
                }),
                SketchProjectionKind::Arc => resolved.arcs.push(SketchArc {
                    id: id.clone(),
                    center: point("center", 0., 0.),
                    start: point("start", 1., 0.),
                    end: point("end", 0., 1.),
                    clockwise: false,
                }),
                SketchProjectionKind::Ellipse => resolved.ellipses.push(SketchEllipse {
                    id: id.clone(),
                    center: point("center", 0., 0.),
                    major: point("major", 2., 0.),
                    minor: point("minor", 0., 1.),
                }),
            }
        }
        Ok(resolved)
    }
}
