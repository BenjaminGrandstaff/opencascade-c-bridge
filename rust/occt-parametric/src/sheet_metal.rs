//! Constant-width flange strips with circular bends and explicit neutral factors.
use super::*;
use crate::assembly::{add, scale};
use occt_bridge::WireSegment;
#[allow(unused_imports)]
pub use occt_parametric_core::sheet_metal::*;
pub(crate) trait SheetMetalDefinitionUnfoldingInternal {
    fn generate<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Shape<'a>, ModelError>;

    fn generate_flat<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        neutral_factor: f64,
    ) -> Result<Shape<'a>, ModelError>;
}

impl SheetMetalDefinitionUnfoldingInternal for SheetMetalDefinition {
    fn generate<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Shape<'a>, ModelError> {
        let sheet = self.evaluate(parameters)?;
        let left = sheet.boundary(sheet.thickness * 0.5);
        let right = sheet.boundary(-sheet.thickness * 0.5);
        let mut segments = left;
        let end_left = end(*segments.last().expect("nonempty flanges"));
        let end_right = end(*right.last().expect("nonempty flanges"));
        segments.push(WireSegment::Line {
            start: end_left,
            end: end_right,
        });
        segments.extend(right.iter().rev().copied().map(reverse));
        segments.push(WireSegment::Line {
            start: start(*right.first().unwrap()),
            end: start(*segments.first().unwrap()),
        });
        sheet.solid(session, &segments)
    }

    fn generate_flat<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        neutral_factor: f64,
    ) -> Result<Shape<'a>, ModelError> {
        let sheet = self.evaluate(parameters)?;
        let metrics = sheet.metrics(neutral_factor)?;
        let half = sheet.thickness * 0.5;
        let points = [
            (0.0, half),
            (metrics.length_mm, half),
            (metrics.length_mm, -half),
            (0.0, -half),
        ];
        let segments: Vec<_> = (0..4)
            .map(|index| WireSegment::Line {
                start: sheet.point(points[index]),
                end: sheet.point(points[(index + 1) % 4]),
            })
            .collect();
        sheet.solid(session, &segments)
    }
}
pub(crate) trait EvaluatedSheetUnfoldingInternal {
    fn point(&self, point: (f64, f64)) -> Vec3;

    fn boundary(&self, offset: f64) -> Vec<WireSegment>;

    fn solid<'a>(
        &self,
        session: &'a Session,
        segments: &[WireSegment],
    ) -> Result<Shape<'a>, ModelError>;
}

impl EvaluatedSheetUnfoldingInternal for EvaluatedSheet {
    fn point(&self, point: (f64, f64)) -> Vec3 {
        add(
            self.origin,
            add(scale(self.tangent, point.0), scale(self.normal, point.1)),
        )
    }

    fn boundary(&self, offset: f64) -> Vec<WireSegment> {
        let mut segments = Vec::with_capacity(self.flanges.len() + self.bends.len());
        let mut point = (0.0, 0.0);
        let mut angle: f64 = 0.0;
        for (index, flange) in self.flanges.iter().enumerate() {
            let next = (
                point.0 + flange * angle.cos(),
                point.1 + flange * angle.sin(),
            );
            let left = (-angle.sin(), angle.cos());
            segments.push(WireSegment::Line {
                start: self.point((point.0 + offset * left.0, point.1 + offset * left.1)),
                end: self.point((next.0 + offset * left.0, next.1 + offset * left.1)),
            });
            point = next;
            if let Some((turn, radius)) = self.bends.get(index) {
                let sign = turn.signum();
                let center_radius = radius + self.thickness * 0.5;
                let center = (
                    point.0 + sign * center_radius * left.0,
                    point.1 + sign * center_radius * left.1,
                );
                let boundary_radius = center_radius - sign * offset;
                let arc_point = |theta: f64| {
                    self.point((
                        center.0 + sign * boundary_radius * theta.sin(),
                        center.1 - sign * boundary_radius * theta.cos(),
                    ))
                };
                segments.push(WireSegment::Arc {
                    start: arc_point(angle),
                    middle: arc_point(angle + turn * 0.5),
                    end: arc_point(angle + turn),
                });
                angle += turn;
                point = (
                    center.0 + sign * center_radius * angle.sin(),
                    center.1 - sign * center_radius * angle.cos(),
                );
            }
        }
        segments
    }

    fn solid<'a>(
        &self,
        session: &'a Session,
        segments: &[WireSegment],
    ) -> Result<Shape<'a>, ModelError> {
        let wire = session.create_segment_wire(segments, true)?;
        let face = session.create_face_from_wire(&wire)?;
        if !session.is_valid(&face)? || !session.face_is_planar(&face)? {
            return Err(ModelError::new(
                "sheet outline is self-intersecting or invalid",
            ));
        }
        let solid = session.create_prism_from_face(&face, scale(self.width_axis, self.width))?;
        let volume = session.volume(&solid)?;
        if session.shape_type(&solid)? != ShapeType::Solid
            || !session.is_valid(&solid)?
            || !volume.is_finite()
            || volume <= 0.0
        {
            return Err(ModelError::new(
                "sheet outline did not produce one valid solid",
            ));
        }
        Ok(solid)
    }
}
