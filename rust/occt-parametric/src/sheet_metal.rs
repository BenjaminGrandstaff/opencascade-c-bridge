//! Constant-width flange strips with circular bends and explicit neutral factors.
use super::*;
use crate::assembly::{add, cross, dot, length, scale, subtract};
use occt_bridge::{CurveSegment, WireSegment};
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
        let solid = if sheet.edge_flanges.is_empty() && sheet.reliefs.is_empty() {
            sheet.profile_solid(session)?
        } else {
            // Slabs across the width, those inside a relief without its
            // bend, and each edge flange as a strip of its own starting on
            // the side face it shares: pieces that only touch, joined by one
            // glued fuse with no cutting.
            let mut pieces = sheet.slabs(session)?;
            for edge in &sheet.edge_flanges {
                let strip = sheet.edge_strip(edge);
                pieces.push(strip.piece(session)?);
            }
            let solid = session.fuse_glued(&pieces.iter().collect::<Vec<_>>())?;
            let volume = session.volume(&solid)?;
            if session.shape_type(&solid)? != ShapeType::Solid
                || !session.is_valid(&solid)?
                || !volume.is_finite()
                || volume <= 0.0
            {
                return Err(ModelError::new(
                    "sheet edge flanges and reliefs did not make one valid solid",
                ));
            }
            solid
        };
        // Each cutout is cut square to its flange, at its midplane frame,
        // in one boolean.
        let frames = sheet.flange_frames();
        let developed = sheet.developed_starts();
        let mut cutters = Vec::with_capacity(sheet.cutouts.len());
        for cutout in &sheet.cutouts {
            let ((x, y), angle) = frames[cutout.flange];
            let center = developed[cutout.flange] + cutout.along;
            let (half, _) = cutout.outline.half_extents();
            let flange = cutout.flange;
            if center - half >= developed[flange]
                && center + half <= developed[flange] + sheet.flanges[flange]
            {
                cutters.push(sheet.cutter(
                    session,
                    (x, y),
                    angle,
                    cutout.outline.segments(cutout.along, cutout.across),
                )?);
                continue;
            }
            // A cutout crossing bends (schema 178): its part on each flange
            // cut square to that flange, its part in each bend pushed
            // radially through the bend.
            let outline = cutout.outline.segments(center, cutout.across);
            let shift = |piece: Vec<OutlineSegment>, by: f64| -> Vec<OutlineSegment> {
                let at = |p: [f64; 2]| [p[0] - by, p[1]];
                piece
                    .into_iter()
                    .map(|segment| match segment {
                        OutlineSegment::Line(a, b) => OutlineSegment::Line(at(a), at(b)),
                        OutlineSegment::Arc(a, m, b) => OutlineSegment::Arc(at(a), at(m), at(b)),
                    })
                    .collect()
            };
            for k in 0..sheet.flanges.len() {
                let low = developed[k];
                let high = low + sheet.flanges[k];
                if sheet.flanges[k] > 0.0 && high > center - half && low < center + half {
                    let piece = clip_outline(&outline, low, high);
                    if piece.len() > 1 {
                        let ((x, y), angle) = frames[k];
                        cutters.push(sheet.cutter(session, (x, y), angle, shift(piece, low))?);
                    }
                }
                if let Some(&(turn, radius)) = sheet.bends.get(k) {
                    let arc = turn.abs() * (radius + sheet.thickness * 0.5);
                    if high + arc > center - half && high < center + half {
                        let piece = clip_outline(&outline, high, high + arc);
                        if piece.len() > 1 {
                            cutters.push(sheet.bend_cutter(
                                session,
                                k,
                                frames[k],
                                shift(piece, high),
                            )?);
                        }
                    }
                }
            }
        }
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
        // Tabs and notches make the blank a polygon: always a face with
        // its cutouts as holes.
        if !metrics.edge_tabs.is_empty() || !metrics.reliefs.is_empty() {
            return sheet.pierced_blank(session, &metrics);
        }
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
        let reach = sheet.thickness * 0.55;
        let cutters = metrics
            .cutouts
            .iter()
            .map(|cutout| {
                let wire = sheet.blank_hole(session, cutout, reach)?;
                let face = session.create_face_from_wire(&wire)?;
                Ok(session.create_prism_from_face(&face, scale(sheet.normal, -2.0 * reach))?)
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        sheet.cut(session, solid, &cutters)
    }
}
pub(crate) trait EvaluatedSheetUnfoldingInternal {
    fn point(&self, point: (f64, f64)) -> Vec3;

    /// The closed outline of the strip's cross-section: both faces and the
    /// two ends, a closed or teardrop hem lying on its flange at either end
    /// (schema 177); and the teardrops' enclosed voids as inner loops.
    fn outline(&self) -> (Vec<WireSegment>, Vec<Vec<WireSegment>>);

    /// The cap of a hem lying on its flange at a strip end `end` (2D), facing
    /// outward along `d`, folded by `turn` (signed toward the hem's side,
    /// seen looking out) at `radius`, with its flange `length`: the path from
    /// the hem side's face, cut back by the returned distance, round to the
    /// other face's end; and a teardrop's void.
    #[allow(clippy::type_complexity)]
    fn cap(
        &self,
        end: (f64, f64),
        d: [f64; 2],
        turn: f64,
        radius: f64,
        length: f64,
        kind: HemEnd,
    ) -> (Vec<OutlineSegment>, f64, Option<Vec<OutlineSegment>>);

    /// The strip's cross-section, with any voids as holes, pushed across
    /// the width and checked.
    fn profile_solid<'a>(&self, session: &'a Session) -> Result<Shape<'a>, ModelError>;

    /// An edge flange as a strip of its own: from the base flange's width
    /// edge, a zero-length flange, its bend and its leg, extruded along the
    /// base flange over the edge flange's length.
    fn edge_strip(&self, edge: &EvaluatedEdgeFlange) -> EvaluatedSheet;

    /// The strip as prisms across the width, split where reliefs start:
    /// a slab inside a relief leaves out that bend, falling apart into the
    /// chains of flanges between relieved bends. O(slabs × flanges).
    fn slabs<'a>(&self, session: &'a Session) -> Result<Vec<Shape<'a>>, ModelError>;

    /// The strip's prism without its own checks, as an internal piece of a
    /// solid checked once whole.
    fn piece<'a>(&self, session: &'a Session) -> Result<Shape<'a>, ModelError>;

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

    /// A radial cutter through bend `bend`, starting after flange `bend`
    /// at `frame`, over an outline in (developed length into the bend,
    /// across) coordinates.
    fn bend_cutter<'a>(
        &self,
        session: &'a Session,
        bend: usize,
        frame: ((f64, f64), f64),
        outline: Vec<OutlineSegment>,
    ) -> Result<Shape<'a>, ModelError>;

    /// The wire of a blank cutout's outline in the blank's plane at height
    /// `height` above the midplane: exact stretched pieces for one crossing
    /// bends.
    fn blank_hole<'a>(
        &self,
        session: &'a Session,
        cutout: &FlatCutout,
        height: f64,
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

    fn outline(&self) -> (Vec<WireSegment>, Vec<Vec<WireSegment>>) {
        // The walk without hems lying on their flanges, which end the strip
        // with caps instead of square ends.
        let (lying_start, lying_end) = (
            self.start_end != HemEnd::Plain,
            self.end_end != HemEnd::Plain,
        );
        let core = EvaluatedSheet {
            start: if lying_start {
                self.flange_frames()[1]
            } else {
                self.start
            },
            flanges: self.flanges
                [usize::from(lying_start)..self.flanges.len() - usize::from(lying_end)]
                .to_vec(),
            bends: self.bends[usize::from(lying_start)..self.bends.len() - usize::from(lying_end)]
                .to_vec(),
            cutouts: Vec::new(),
            edge_flanges: Vec::new(),
            reliefs: Vec::new(),
            start_end: HemEnd::Plain,
            end_end: HemEnd::Plain,
            ..*self
        };
        let half = self.thickness * 0.5;
        let mut left = core.boundary(half);
        let mut right = core.boundary(-half);
        let to3 = |segment: OutlineSegment| match segment {
            OutlineSegment::Line(a, b) => WireSegment::Line {
                start: self.point((a[0], a[1])),
                end: self.point((b[0], b[1])),
            },
            OutlineSegment::Arc(a, m, b) => WireSegment::Arc {
                start: self.point((a[0], a[1])),
                middle: self.point((m[0], m[1])),
                end: self.point((b[0], b[1])),
            },
        };
        let mut inners = Vec::new();
        // The end cap: from the hem side's truncated face to the other
        // face's end, with the hem side and any void.
        let mut end_path = None;
        if lying_end {
            let frames = core.flange_frames();
            let at = advance(
                frames[frames.len() - 1],
                core.flanges[core.flanges.len() - 1],
                None,
                self.thickness,
            );
            let (turn, radius) = self.bends[self.bends.len() - 1];
            let d = [at.1.cos(), at.1.sin()];
            let (cap, back, inner) = self.cap(
                at.0,
                d,
                turn,
                radius,
                self.flanges[self.flanges.len() - 1],
                self.end_end,
            );
            let truncated = self.point((
                at.0.0 - back * d[0] + turn.signum() * half * -d[1],
                at.0.1 - back * d[1] + turn.signum() * half * d[0],
            ));
            let side = if turn > 0.0 { &mut left } else { &mut right };
            if let Some(WireSegment::Line { end, .. }) = side.last_mut() {
                *end = truncated;
            }
            let path: Vec<WireSegment> = cap.into_iter().map(to3).collect();
            end_path = Some(if turn > 0.0 {
                path
            } else {
                path.iter().rev().copied().map(reverse).collect()
            });
            inners.extend(inner.map(|loop_| loop_.into_iter().map(to3).collect::<Vec<_>>()));
        }
        let mut start_path = None;
        if lying_start {
            let ((x, y), angle) = core.start;
            let (turn, radius) = self.bends[0];
            let d = [-angle.cos(), -angle.sin()];
            // Seen from the start, looking out of the strip, the hem's side
            // is on the walk's opposite hand.
            let (cap, back, inner) =
                self.cap((x, y), d, -turn, radius, self.flanges[0], self.start_end);
            let left_hand = [-angle.sin(), angle.cos()];
            let side_sign = turn.signum();
            let truncated = self.point((
                x - back * d[0] + side_sign * half * left_hand[0],
                y - back * d[1] + side_sign * half * left_hand[1],
            ));
            let side = if turn > 0.0 { &mut left } else { &mut right };
            if let Some(WireSegment::Line { start, .. }) = side.first_mut() {
                *start = truncated;
            }
            let path: Vec<WireSegment> = cap.into_iter().map(to3).collect();
            // The outline closes from the right face's start to the left's.
            start_path = Some(if turn > 0.0 {
                path.iter().rev().copied().map(reverse).collect()
            } else {
                path
            });
            inners.extend(inner.map(|loop_| loop_.into_iter().map(to3).collect::<Vec<_>>()));
        }
        let mut segments = left;
        let end_left = end(*segments.last().expect("nonempty flanges"));
        let end_right = end(*right.last().expect("nonempty flanges"));
        match end_path {
            Some(path) => segments.extend(path),
            None => segments.push(WireSegment::Line {
                start: end_left,
                end: end_right,
            }),
        }
        let start_left = start(*segments.first().unwrap());
        let start_right = start(*right.first().unwrap());
        segments.extend(right.iter().rev().copied().map(reverse));
        match start_path {
            Some(path) => segments.extend(path),
            None => segments.push(WireSegment::Line {
                start: start_right,
                end: start_left,
            }),
        }
        (segments, inners)
    }

    fn cap(
        &self,
        end: (f64, f64),
        d: [f64; 2],
        turn: f64,
        radius: f64,
        length: f64,
        kind: HemEnd,
    ) -> (Vec<OutlineSegment>, f64, Option<Vec<OutlineSegment>>) {
        let n = [-d[1] * turn.signum(), d[0] * turn.signum()];
        let at = |[x, y]: [f64; 2]| [end.0 + x * d[0] + y * n[0], end.1 + x * d[1] + y * n[1]];
        let t = self.thickness;
        let half = t * 0.5;
        if kind == HemEnd::Closed {
            // The leaf on the flange's face, and the fold's half circle of
            // radius t about the crease at the face's end.
            return (
                vec![
                    OutlineSegment::Line(at([-length, half]), at([-length, 3.0 * half])),
                    OutlineSegment::Line(at([-length, 3.0 * half]), at([0.0, 3.0 * half])),
                    OutlineSegment::Arc(at([0.0, 3.0 * half]), at([t, half]), at([0.0, -half])),
                ],
                length,
                None,
            );
        }
        let g = TeardropGeometry::new(radius, turn.abs(), t)
            .expect("a teardrop validated when the sheet was evaluated");
        let on_roll =
            |r: f64, theta: f64| [g.center[0] + r * theta.sin(), g.center[1] - r * theta.cos()];
        let middle = turn.abs() * 0.5;
        (
            vec![
                OutlineSegment::Line(at(g.outer_landing), at(g.outer_start)),
                OutlineSegment::Arc(
                    at(g.outer_start),
                    at(on_roll(radius + t, middle)),
                    at([0.0, -half]),
                ),
            ],
            g.covered(),
            Some(vec![
                OutlineSegment::Arc(
                    at([0.0, half]),
                    at(on_roll(radius, middle)),
                    at(g.inner_start),
                ),
                OutlineSegment::Line(at(g.inner_start), at(g.inner_landing)),
                OutlineSegment::Line(at(g.inner_landing), at([0.0, half])),
            ]),
        )
    }

    fn profile_solid<'a>(&self, session: &'a Session) -> Result<Shape<'a>, ModelError> {
        let (outer, inners) = self.outline();
        if inners.is_empty() {
            return self.solid(session, &outer);
        }
        let wire = session.create_segment_wire(&outer, true)?;
        let holes = inners
            .iter()
            .map(|inner| session.create_segment_wire(inner, true))
            .collect::<Result<Vec<_>, _>>()?;
        let face = session.create_face_with_holes(&wire, &holes.iter().collect::<Vec<_>>())?;
        if !session.is_valid(&face)? {
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

    fn edge_strip(&self, edge: &EvaluatedEdgeFlange) -> EvaluatedSheet {
        let ((x, y), angle) = self.flange_frames()[edge.flange];
        let (cos, sin) = (angle.cos(), angle.sin());
        let along = add(scale(self.tangent, cos), scale(self.normal, sin));
        let normal = add(scale(self.tangent, -sin), scale(self.normal, cos));
        let base = self.point((x + edge.start * cos, y + edge.start * sin));
        let (origin, outward) = match edge.edge {
            SheetEdge::Far => (
                add(base, scale(self.width_axis, self.width)),
                self.width_axis,
            ),
            SheetEdge::Origin => (base, scale(self.width_axis, -1.0)),
        };
        EvaluatedSheet {
            start: ((0.0, 0.0), 0.0),
            origin,
            width_axis: along,
            tangent: outward,
            normal,
            width: edge.length,
            thickness: self.thickness,
            flanges: vec![0.0, edge.height],
            bends: vec![(edge.angle, edge.radius)],
            cutouts: Vec::new(),
            edge_flanges: Vec::new(),
            reliefs: Vec::new(),
            start_end: HemEnd::Plain,
            end_end: HemEnd::Plain,
        }
    }

    fn slabs<'a>(&self, session: &'a Session) -> Result<Vec<Shape<'a>>, ModelError> {
        let width = self.width;
        let mut cuts = vec![0.0, width];
        for relief in &self.reliefs {
            cuts.push(match relief.edge {
                SheetEdge::Origin => relief.depth,
                SheetEdge::Far => width - relief.depth,
            });
        }
        cuts.sort_by(f64::total_cmp);
        let tolerance = 1e-9 * (1.0 + width);
        cuts.dedup_by(|a, b| (*a - *b).abs() <= tolerance);
        let frames = self.flange_frames();
        let mut pieces = Vec::new();
        for pair in cuts.windows(2) {
            let (low, high) = (pair[0], pair[1]);
            let removed = |bend: usize| {
                self.reliefs.iter().any(|relief| {
                    relief.bend == bend
                        && match relief.edge {
                            SheetEdge::Origin => high <= relief.depth + tolerance,
                            SheetEdge::Far => low >= width - relief.depth - tolerance,
                        }
                })
            };
            let mut first = 0;
            for last in 0..self.flanges.len() {
                if last + 1 < self.flanges.len() && !removed(last) {
                    continue;
                }
                let bends = self.bends[first..last].to_vec();
                let flanges = self.flanges[first..=last].to_vec();
                // A rolled hem's empty flange cut off from its roll is nothing.
                if !bends.is_empty() || flanges.iter().any(|f| *f > 0.0) {
                    let chain = EvaluatedSheet {
                        start: frames[first],
                        origin: add(self.origin, scale(self.width_axis, low)),
                        width_axis: self.width_axis,
                        tangent: self.tangent,
                        normal: self.normal,
                        width: high - low,
                        thickness: self.thickness,
                        flanges,
                        bends,
                        cutouts: Vec::new(),
                        edge_flanges: Vec::new(),
                        reliefs: Vec::new(),
                        // Hems lying on their flange are never relieved,
                        // so they stay in the chains holding their flange.
                        start_end: if first == 0 {
                            self.start_end
                        } else {
                            HemEnd::Plain
                        },
                        end_end: if last + 1 == self.flanges.len() {
                            self.end_end
                        } else {
                            HemEnd::Plain
                        },
                    };
                    pieces.push(chain.piece(session)?);
                }
                first = last + 1;
            }
        }
        Ok(pieces)
    }

    fn piece<'a>(&self, session: &'a Session) -> Result<Shape<'a>, ModelError> {
        let (outer, inners) = self.outline();
        let wire = session.create_segment_wire(&outer, true)?;
        let face = if inners.is_empty() {
            session.create_face_from_wire(&wire)?
        } else {
            let holes = inners
                .iter()
                .map(|inner| session.create_segment_wire(inner, true))
                .collect::<Result<Vec<_>, _>>()?;
            session.create_face_with_holes(&wire, &holes.iter().collect::<Vec<_>>())?
        };
        Ok(session.create_prism_from_face(&face, scale(self.width_axis, self.width))?)
    }

    fn flange_frames(&self) -> Vec<((f64, f64), f64)> {
        // The midplane walk of `boundary` at zero offset.
        let mut frames = Vec::with_capacity(self.flanges.len());
        let mut at = self.start;
        for (index, flange) in self.flanges.iter().enumerate() {
            frames.push(at);
            at = advance(at, *flange, self.bends.get(index).copied(), self.thickness);
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
        let outline = metrics.outline();
        let outer = wire(
            (0..outline.len())
                .map(|i| OutlineSegment::Line(outline[i], outline[(i + 1) % outline.len()]))
                .collect(),
        )?;
        let holes = metrics
            .cutouts
            .iter()
            .map(|cutout| self.blank_hole(session, cutout, self.thickness * 0.5))
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

    fn bend_cutter<'a>(
        &self,
        session: &'a Session,
        bend: usize,
        frame: ((f64, f64), f64),
        outline: Vec<OutlineSegment>,
    ) -> Result<Shape<'a>, ModelError> {
        let (turn, radius) = self.bends[bend];
        let ((point, heading), _) = (advance(frame, self.flanges[bend], None, self.thickness), ());
        let (start, angle) = (point, heading);
        let middle = radius + self.thickness * 0.5;
        let left = (-angle.sin(), angle.cos());
        let sign = turn.signum();
        let center = (
            start.0 + sign * middle * left.0,
            start.1 + sign * middle * left.1,
        );
        let origin = self.point(center);
        let outward = subtract(self.point(start), origin);
        let outward = scale(outward, 1.0 / length(outward));
        let along = add(
            scale(self.tangent, angle.cos()),
            scale(self.normal, angle.sin()),
        );
        let axis = cross(outward, along);
        // Developed coordinates run along the walk; across runs along the
        // width, which the cutter's axis follows or opposes.
        let across = dot(axis, self.width_axis).signum();
        let segments: Vec<WireSegment> = outline
            .into_iter()
            .map(|segment| match segment {
                OutlineSegment::Line(a, b) => WireSegment::Line {
                    start: Vec3::new(a[0], across * a[1], 0.0),
                    end: Vec3::new(b[0], across * b[1], 0.0),
                },
                OutlineSegment::Arc(a, m, b) => WireSegment::Arc {
                    start: Vec3::new(a[0], across * a[1], 0.0),
                    middle: Vec3::new(m[0], across * m[1], 0.0),
                    end: Vec3::new(b[0], across * b[1], 0.0),
                },
            })
            .collect();
        // A little beyond the bend's faces: cutters on exactly its radii
        // cut several times faster but left material behind in testing.
        let clearance = 0.05 * self.thickness;
        Ok(session.create_radial_cutter(
            origin,
            axis,
            outward,
            middle,
            (radius - clearance).max(radius * 0.5),
            radius + self.thickness + clearance,
            &segments,
        )?)
    }

    fn blank_hole<'a>(
        &self,
        session: &'a Session,
        cutout: &FlatCutout,
        height: f64,
    ) -> Result<Shape<'a>, ModelError> {
        let at = |[u, v]: [f64; 2]| add(self.point((u, height)), scale(self.width_axis, v));
        if cutout.mapped.is_empty() {
            let [u, v] = cutout.center_mm;
            let segments: Vec<WireSegment> = cutout
                .outline
                .segments(u, v)
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
            return Ok(session.create_segment_wire(&segments, true)?);
        }
        let segments: Vec<CurveSegment> = cutout
            .mapped
            .iter()
            .map(|segment| match *segment {
                FlatSegment::Line { start, end } => CurveSegment::Line {
                    start: at(start),
                    end: at(end),
                },
                FlatSegment::Arc { start, middle, end } => CurveSegment::Arc {
                    start: at(start),
                    middle: at(middle),
                    end: at(end),
                },
                FlatSegment::EllipseArc {
                    center,
                    radii,
                    start,
                    end,
                    clockwise,
                } => CurveSegment::EllipseArc {
                    center: at(center),
                    major: at([center[0] + radii[0], center[1]]),
                    minor: at([center[0], center[1] + radii[1]]),
                    start: at(start),
                    end: at(end),
                    clockwise,
                },
            })
            .collect();
        Ok(session.create_curve_wire(&segments, true)?)
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
        let (mut point, mut angle) = self.start;
        for (index, flange) in self.flanges.iter().enumerate() {
            let next = (
                point.0 + flange * angle.cos(),
                point.1 + flange * angle.sin(),
            );
            let left = (-angle.sin(), angle.cos());
            // A rolled hem's flange has no length and no edge.
            if *flange > 0.0 {
                segments.push(WireSegment::Line {
                    start: self.point((point.0 + offset * left.0, point.1 + offset * left.1)),
                    end: self.point((next.0 + offset * left.0, next.1 + offset * left.1)),
                });
            }
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
