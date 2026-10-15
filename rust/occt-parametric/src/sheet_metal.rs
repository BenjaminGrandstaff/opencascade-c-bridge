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
        table: Option<&BendTable>,
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
        let solid = sheet.solid(session, &segments)?;
        // Each cutout is cut square to its flange, at its midplane frame.
        let frames = sheet.flange_frames();
        let cutters = sheet
            .cutouts
            .iter()
            .map(|cutout| {
                let ((x, y), angle) = frames[cutout.flange];
                sheet.cutter(
                    session,
                    (x, y),
                    angle,
                    cutout.outline.segments(cutout.along, cutout.across),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        sheet.cut(session, solid, &cutters)
    }

    fn generate_flat<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        neutral_factor: f64,
        table: Option<&BendTable>,
    ) -> Result<Shape<'a>, ModelError> {
        let sheet = self.evaluate(parameters)?;
        let metrics = sheet.metrics_with(neutral_factor, table)?;
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
        if metrics.cutouts.is_empty() {
            return sheet.solid(session, &segments);
        }
        // With cutouts, the blank's top face is built with them as holes and
        // pushed through the thickness: no boolean. Overlapping cutouts,
        // which a face cannot hold as separate holes, fall back to cutting.
        if let Ok(solid) = sheet.pierced_blank(session, &metrics) {
            return Ok(solid);
        }
        let solid = sheet.solid(session, &segments)?;
        let cutters = metrics
            .cutouts
            .iter()
            .map(|cutout| {
                let [u, v] = cutout.center_mm;
                sheet.cutter(session, (0.0, 0.0), 0.0, cutout.outline.segments(u, v))
            })
            .collect::<Result<Vec<_>, _>>()?;
        sheet.cut(session, solid, &cutters)
    }
}
pub(crate) trait EvaluatedSheetUnfoldingInternal {
    fn point(&self, point: (f64, f64)) -> Vec3;

    /// Each flange's midplane start point and direction angle in the sheet's
    /// (tangent, normal) plane.
    fn flange_frames(&self) -> Vec<((f64, f64), f64)>;

    /// A prism through the sheet square to the flange starting at `start`
    /// at `angle`, over an outline in (along, across) flange coordinates.
    fn cutter<'a>(
        &self,
        session: &'a Session,
        start: (f64, f64),
        angle: f64,
        outline: Vec<OutlineSegment>,
    ) -> Result<Shape<'a>, ModelError>;

    /// The flat blank's top face with its cutouts as holes, pushed through
    /// the thickness. O(cutout segments).
    fn pierced_blank<'a>(
        &self,
        session: &'a Session,
        metrics: &FlatPatternMetrics,
    ) -> Result<Shape<'a>, ModelError>;

    /// Cuts all cutters from `solid` in one boolean and checks the result.
    fn cut<'a>(
        &self,
        session: &'a Session,
        solid: Shape<'a>,
        cutters: &[Shape<'a>],
    ) -> Result<Shape<'a>, ModelError>;

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

    fn flange_frames(&self) -> Vec<((f64, f64), f64)> {
        // The midplane walk of `boundary` at zero offset.
        let mut frames = Vec::with_capacity(self.flanges.len());
        let mut point = (0.0, 0.0);
        let mut angle: f64 = 0.0;
        for (index, flange) in self.flanges.iter().enumerate() {
            frames.push((point, angle));
            point = (
                point.0 + flange * angle.cos(),
                point.1 + flange * angle.sin(),
            );
            if let Some((turn, radius)) = self.bends.get(index) {
                let sign = turn.signum();
                let center_radius = radius + self.thickness * 0.5;
                let left = (-angle.sin(), angle.cos());
                let center = (
                    point.0 + sign * center_radius * left.0,
                    point.1 + sign * center_radius * left.1,
                );
                angle += turn;
                point = (
                    center.0 + sign * center_radius * angle.sin(),
                    center.1 - sign * center_radius * angle.cos(),
                );
            }
        }
        frames
    }

    fn cutter<'a>(
        &self,
        session: &'a Session,
        start: (f64, f64),
        angle: f64,
        outline: Vec<OutlineSegment>,
    ) -> Result<Shape<'a>, ModelError> {
        // Slightly thicker than the sheet so the cut leaves no skin, but far
        // from any neighbouring flange.
        let reach = self.thickness * 0.55;
        let (cos, sin) = (angle.cos(), angle.sin());
        let at = |[u, v]: [f64; 2]| {
            add(
                self.point((
                    start.0 + u * cos - reach * -sin,
                    start.1 + u * sin - reach * cos,
                )),
                scale(self.width_axis, v),
            )
        };
        let segments: Vec<WireSegment> = outline
            .into_iter()
            .map(|segment| match segment {
                OutlineSegment::Line(a, b) => WireSegment::Line {
                    start: at(a),
                    end: at(b),
                },
                OutlineSegment::Arc(a, m, b) => WireSegment::Arc {
                    start: at(a),
                    middle: at(m),
                    end: at(b),
                },
            })
            .collect();
        let wire = session.create_segment_wire(&segments, true)?;
        let face = session.create_face_from_wire(&wire)?;
        let through = add(scale(self.tangent, -sin), scale(self.normal, cos));
        Ok(session.create_prism_from_face(&face, scale(through, 2.0 * reach))?)
    }

    fn pierced_blank<'a>(
        &self,
        session: &'a Session,
        metrics: &FlatPatternMetrics,
    ) -> Result<Shape<'a>, ModelError> {
        let top = |[u, v]: [f64; 2]| {
            add(
                self.point((u, self.thickness * 0.5)),
                scale(self.width_axis, v),
            )
        };
        let wire = |outline: Vec<OutlineSegment>| {
            let segments: Vec<WireSegment> = outline
                .into_iter()
                .map(|segment| match segment {
                    OutlineSegment::Line(a, b) => WireSegment::Line {
                        start: top(a),
                        end: top(b),
                    },
                    OutlineSegment::Arc(a, m, b) => WireSegment::Arc {
                        start: top(a),
                        middle: top(m),
                        end: top(b),
                    },
                })
                .collect();
            session.create_segment_wire(&segments, true)
        };
        let (length, width) = (metrics.length_mm, metrics.width_mm);
        let outer = wire(vec![
            OutlineSegment::Line([0.0, 0.0], [length, 0.0]),
            OutlineSegment::Line([length, 0.0], [length, width]),
            OutlineSegment::Line([length, width], [0.0, width]),
            OutlineSegment::Line([0.0, width], [0.0, 0.0]),
        ])?;
        let holes = metrics
            .cutouts
            .iter()
            .map(|cutout| {
                let [u, v] = cutout.center_mm;
                wire(cutout.outline.segments(u, v))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let face = session.create_face_with_holes(&outer, &holes.iter().collect::<Vec<_>>())?;
        let solid = session.create_prism_from_face(&face, scale(self.normal, -self.thickness))?;
        let volume = session.volume(&solid)?;
        if session.shape_type(&solid)? != ShapeType::Solid
            || !session.is_valid(&solid)?
            || !volume.is_finite()
            || volume <= 0.0
        {
            return Err(ModelError::new("pierced blank is not one valid solid"));
        }
        Ok(solid)
    }

    fn cut<'a>(
        &self,
        session: &'a Session,
        solid: Shape<'a>,
        cutters: &[Shape<'a>],
    ) -> Result<Shape<'a>, ModelError> {
        if cutters.is_empty() {
            return Ok(solid);
        }
        let tools = session.create_compound(&cutters.iter().collect::<Vec<_>>())?;
        let mut result = session.cut(&solid, &tools)?;
        // A compound tool gives a compound result; it must hold one solid.
        if session.shape_type(&result)? == ShapeType::Compound
            && session.subshape_count(&result, ShapeType::Solid)? == 1
        {
            result = session.subshape(&result, ShapeType::Solid, 0)?;
        }
        let volume = session.volume(&result)?;
        if session.shape_type(&result)? != ShapeType::Solid
            || !session.is_valid(&result)?
            || !volume.is_finite()
            || volume <= 0.0
        {
            return Err(ModelError::new(
                "sheet cutouts did not leave one valid solid",
            ));
        }
        Ok(result)
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
