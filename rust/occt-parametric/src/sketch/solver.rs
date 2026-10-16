//! Newton least-squares solve of sketch constraints with a sparse Jacobian.

use super::*;
#[allow(unused_imports)]
pub use occt_parametric_core::sketch::solver::*;

struct EllipseLocal {
    angle: f64,
    membership: f64,
    tangent: SketchPoint2,
}

pub(super) struct SketchProblem<'a> {
    sketch: &'a SketchDefinition,
    parameters: &'a HashMap<String, ParameterValue>,
    variables: HashMap<&'a str, usize>,
    fixed: HashMap<&'a str, SketchPoint2>,
    lines: HashMap<&'a str, &'a SketchLine>,
    entities: HashMap<&'a str, Entity<'a>>,
    native: Session,
    derivative_fallbacks: std::cell::Cell<usize>,
    spline_tangents: HashMap<(&'a str, &'a str), &'a str>,
    weighted_points: HashMap<&'a str, Vec<(&'a str, f64)>>,
    pattern_links: Vec<patterns::PatternLink<'a>>,
    spline_point_cache: std::cell::RefCell<HashMap<String, SplinePointCache>>,
    crossing_cache: std::cell::RefCell<HashMap<(String, String), CrossingCache>>,
}

struct SplinePointCache {
    geometry: Vec<[f64; 2]>,
    points: HashMap<u64, SketchPoint2>,
}

struct CrossingCache {
    geometry: Vec<[f64; 2]>,
    points: Vec<Vec3>,
}

#[derive(Clone, Copy)]
enum Equation<'a> {
    Constraint(usize, &'a SketchConstraint),
    Pattern(patterns::PatternLink<'a>),
    Arc(&'a SketchArc),
    Ellipse(&'a SketchEllipse),
    SplineEndpoints(&'a SketchSpline),
}

impl<'a> SketchProblem<'a> {
    pub(super) fn new(
        sketch: &'a SketchDefinition,
        parameters: &'a HashMap<String, ParameterValue>,
        variables: HashMap<&'a str, usize>,
        fixed: HashMap<&'a str, SketchPoint2>,
    ) -> Result<Self, ModelError> {
        let mut weighted_points: HashMap<&str, Vec<(&str, f64)>> = HashMap::new();
        for index in sketch.weighted_point_order()? {
            let constraint = &sketch.constraints[index];
            if !sketch.affine_point_link(constraint) {
                continue;
            }
            let point = constraint.dependent_point().unwrap();
            let terms = sketch.point_link_terms(constraint, parameters)?;
            let weights = terms.iter().map(|(_, w)| *w).collect::<Vec<_>>();
            let scale = weights.iter().copied().fold(0_f64, f64::max);
            let sum = weights.iter().map(|w| w / scale).sum::<f64>();
            let mut flattened = std::collections::BTreeMap::new();
            for ((source, _), weight) in terms.iter().zip(weights) {
                let coefficient = (weight / scale) / sum;
                if let Some(sources) = weighted_points.get(source) {
                    for &(source, value) in sources {
                        *flattened.entry(source).or_insert(0.) += coefficient * value;
                    }
                } else {
                    *flattened.entry(*source).or_insert(0.) += coefficient;
                }
            }
            if flattened.len() > 4096 {
                return Err(ModelError::new(
                    "weighted point exceeds 4096 independent source references",
                ));
            }
            weighted_points.insert(
                point,
                flattened
                    .into_iter()
                    .filter(|&(_, v)| v != 0.)
                    .collect::<Vec<_>>(),
            );
        }
        Ok(Self {
            spline_point_cache: std::cell::RefCell::new(HashMap::new()),
            crossing_cache: std::cell::RefCell::new(HashMap::new()),
            weighted_points,
            sketch,
            parameters,
            variables,
            fixed,
            lines: sketch
                .lines
                .iter()
                .map(|line| (line.id.as_str(), line))
                .collect(),
            entities: sketch.entities(),
            derivative_fallbacks: std::cell::Cell::new(0),
            native: Session::new()?,
            spline_tangents: sketch.spline_tangent_index()?,
            pattern_links: sketch.pattern_links()?,
        })
    }
}

impl SketchProblem<'_> {
    fn weighted_point(
        &self,
        terms: &[SketchWeightedPointTerm],
        values: &[f64],
    ) -> Result<SketchPoint2, ModelError> {
        let weights = terms
            .iter()
            .map(|t| scalar(&t.weight, self.parameters, Dimension::Scalar))
            .collect::<Result<Vec<_>, _>>()?;
        let scale = weights.iter().copied().fold(0_f64, f64::max);
        let sum = weights.iter().map(|w| w / scale).sum::<f64>();
        let mut target = SketchPoint2 { x: 0., y: 0. };
        for (term, weight) in terms.iter().zip(weights) {
            let p = self.point(&term.point, values);
            let coefficient = (weight / scale) / sum;
            target.x += coefficient * p.x;
            target.y += coefficient * p.y;
        }
        if !target.x.is_finite() || !target.y.is_finite() {
            return Err(ModelError::new("weighted point target must be finite"));
        }
        Ok(target)
    }
    fn equations(&self) -> impl Iterator<Item = Equation<'_>> {
        self.sketch
            .constraints
            .iter()
            .enumerate()
            .filter(|(_, constraint)| {
                !self.sketch.spline_tangency(constraint)
                    && !self.sketch.affine_point_link(constraint)
            })
            .map(|(index, c)| Equation::Constraint(index, c))
            .chain(self.sketch.arcs.iter().map(Equation::Arc))
            .chain(self.sketch.ellipses.iter().map(Equation::Ellipse))
            .chain(
                self.sketch
                    .splines
                    .iter()
                    .filter(|s| s.endpoints.is_some())
                    .map(Equation::SplineEndpoints),
            )
            .chain(self.pattern_links.iter().copied().map(Equation::Pattern))
    }

    fn equation_residuals(
        &self,
        equation: Equation<'_>,
        values: &[f64],
        residuals: &mut Vec<f64>,
    ) -> Result<(), ModelError> {
        match equation {
            Equation::Constraint(_, constraint) => {
                self.constraint_residuals(constraint, values, residuals)
            }
            Equation::Pattern(link) => self.pattern_residuals(link, values, residuals),
            Equation::SplineEndpoints(s) => {
                let wire = self.spline_curve(s, values)?;
                let (a, b, _, _) = self.native.curve_end_data(&wire)?;
                let e = s.endpoints.as_ref().unwrap();
                let start = self.point(&e.start, values);
                let end = self.point(&e.end, values);
                residuals.extend([start.x - a.x, start.y - a.y, end.x - b.x, end.y - b.y]);
                Ok(())
            }
            Equation::Ellipse(e) => {
                let c = self.point(&e.center, values);
                residuals.push(normalized_dot(
                    (c, self.point(&e.major, values)),
                    (c, self.point(&e.minor, values)),
                )?);
                // An end at an axis vertex is on the ellipse by definition;
                // its membership equation would be identically zero.
                if let Some((start, end)) = e.arc_ends() {
                    for id in [start, end] {
                        if id != e.major && id != e.minor {
                            let local = self.ellipse_local(e, self.point(id, values), values)?;
                            residuals.push(local.membership);
                        }
                    }
                }
                Ok(())
            }
            Equation::Arc(arc) => {
                let center = self.point(&arc.center, values);
                let residual = line_length((center, self.point(&arc.start, values)))
                    - line_length((center, self.point(&arc.end, values)));
                if !residual.is_finite() {
                    return Err(ModelError::new("sketch arc residual is not finite"));
                }
                residuals.push(residual);
                Ok(())
            }
        }
    }

    /// Mirror: midpoint on the axis and chord along its normal (a shared
    /// point only lies on it). Linear: copy = source + k·spacing·unit
    /// direction. Circular: copy = source rotated k steps about the center.
    fn pattern_residuals(
        &self,
        link: patterns::PatternLink<'_>,
        values: &[f64],
        residuals: &mut Vec<f64>,
    ) -> Result<(), ModelError> {
        let s = self.point(link.source, values);
        let start = residuals.len();
        match link.pattern {
            SketchPattern::Mirror { axis: line, .. }
            | SketchPattern::Linear {
                direction: line, ..
            } => {
                let (u, v) = self.line(line, values);
                let (dx, dy) = (v.x - u.x, v.y - u.y);
                let length = dx.hypot(dy);
                if length <= f64::EPSILON {
                    return Err(ModelError::new(
                        "sketch pattern reference line has zero length",
                    ));
                }
                let (tx, ty) = (dx / length, dy / length);
                match (link.pattern, link.copy) {
                    (SketchPattern::Mirror { .. }, None) => {
                        residuals.push((s.x - u.x) * ty - (s.y - u.y) * tx);
                    }
                    (SketchPattern::Mirror { .. }, Some(copy)) => {
                        let c = self.point(copy, values);
                        residuals
                            .push(((s.x + c.x) * 0.5 - u.x) * ty - ((s.y + c.y) * 0.5 - u.y) * tx);
                        residuals.push((c.x - s.x) * tx + (c.y - s.y) * ty);
                    }
                    (SketchPattern::Linear { spacing, .. }, Some(copy)) => {
                        let c = self.point(copy, values);
                        let d = scalar(spacing, self.parameters, Dimension::Length)?
                            * f64::from(link.instance);
                        residuals.extend([c.x - s.x - d * tx, c.y - s.y - d * ty]);
                    }
                    _ => unreachable!("linear patterns copy every point"),
                }
            }
            SketchPattern::Circular { center, .. } => {
                let o = self.point(center, values);
                let c = self.point(link.copy.expect("circular copies"), values);
                let angle = link.pattern.circular_step(self.parameters)? * f64::from(link.instance);
                let (sin, cos) = angle.sin_cos();
                let (x, y) = (s.x - o.x, s.y - o.y);
                residuals.extend([
                    c.x - (o.x + cos * x - sin * y),
                    c.y - (o.y + sin * x + cos * y),
                ]);
            }
        }
        if residuals[start..].iter().any(|value| !value.is_finite()) {
            return Err(ModelError::new("sketch pattern residual is not finite"));
        }
        Ok(())
    }

    fn tangent(
        &self,
        id: &str,
        contact: &str,
        values: &[f64],
    ) -> Result<(SketchPoint2, SketchPoint2), ModelError> {
        let entity = self.entities[id];
        if let Entity::Spline(s) = entity
            && let Some(e) = &s.endpoints
        {
            let wire = self.spline_curve(s, values)?;
            let (_, _, u, v) = self.native.curve_end_data(&wire)?;
            let d = if e.start == contact { u } else { v };
            return Ok((
                SketchPoint2 { x: 0., y: 0. },
                SketchPoint2 { x: d.x, y: d.y },
            ));
        }
        if let Entity::Ellipse(e) = entity {
            let local = self.ellipse_local(e, self.point(contact, values), values)?;
            return Ok((SketchPoint2 { x: 0.0, y: 0.0 }, local.tangent));
        }
        let (first, second) = entity.tangent_points(contact);
        let (a, b) = (self.point(first, values), self.point(second, values));
        let direction = if matches!(entity, Entity::Line(_) | Entity::Spline(_)) {
            SketchPoint2 {
                x: b.x - a.x,
                y: b.y - a.y,
            }
        } else {
            SketchPoint2 {
                x: a.y - b.y,
                y: b.x - a.x,
            }
        };
        Ok((SketchPoint2 { x: 0.0, y: 0.0 }, direction))
    }

    fn spline_neighbor_refs<'a>(&'a self, spline: &'a SketchSpline) -> Vec<&'a str> {
        let mut refs = spline.points.iter().map(String::as_str).collect::<Vec<_>>();
        for at in [
            spline.points.first().unwrap(),
            spline.points.last().unwrap(),
        ] {
            if let Some(neighbor) = self.spline_tangents.get(&(spline.id.as_str(), at.as_str())) {
                match self.entities[neighbor] {
                    Entity::Line(l) => refs.extend([l.start.as_str(), l.end.as_str()]),
                    Entity::Arc(a) => {
                        refs.extend([a.center.as_str(), a.start.as_str(), a.end.as_str()])
                    }
                    _ => {}
                }
            }
        }
        refs
    }
    fn interpolated_spline_point(
        &self,
        id: &str,
        fraction: &ScalarExpr,
        values: &[f64],
    ) -> Result<SketchPoint2, ModelError> {
        let spline = match self.entities[id] {
            Entity::Spline(s) => s,
            _ => unreachable!(),
        };
        let fraction = scalar(fraction, self.parameters, Dimension::Scalar)?;
        let fraction = if spline.closed() && fraction == 1. {
            0.
        } else {
            fraction
        };
        let key = fraction.to_bits();
        let geometry = self
            .spline_neighbor_refs(spline)
            .iter()
            .map(|id| {
                let p = self.point(id, values);
                [p.x, p.y]
            })
            .collect::<Vec<_>>();
        if geometry.len() > 65536 {
            return Err(ModelError::new(
                "spline point cache exceeds 65536 source references",
            ));
        }
        let cached = {
            let cache = self.spline_point_cache.borrow();
            cache
                .get(id)
                .filter(|c| c.geometry == geometry)
                .and_then(|c| c.points.get(&key))
                .copied()
        };
        if let Some(point) = cached {
            return Ok(point);
        }
        {
            let cache = self.spline_point_cache.borrow();
            if !cache.contains_key(id) && cache.len() >= 256 {
                return Err(ModelError::new(
                    "spline point cache exceeds 256 source curves",
                ));
            }
            let samples = cache
                .iter()
                .filter(|(name, c)| name.as_str() != id || c.geometry == geometry)
                .map(|(_, c)| c.points.len())
                .sum::<usize>();
            if samples >= 10000 {
                return Err(ModelError::new(
                    "spline point cache exceeds 10000 parameter samples",
                ));
            }
            let other_refs = cache
                .iter()
                .filter(|(name, _)| name.as_str() != id)
                .map(|(_, c)| c.geometry.len())
                .sum::<usize>();
            if other_refs + geometry.len() > 65536 {
                return Err(ModelError::new(
                    "spline point cache exceeds 65536 source references",
                ));
            }
        }
        let curve = self.spline_curve(spline, values)?;
        let p = self.native.curve_point_at_fraction(&curve, fraction)?;
        let point = SketchPoint2 { x: p.x, y: p.y };
        let mut cache = self.spline_point_cache.borrow_mut();
        let entry = cache
            .entry(id.to_owned())
            .or_insert_with(|| SplinePointCache {
                geometry: geometry.clone(),
                points: HashMap::new(),
            });
        if entry.geometry != geometry {
            entry.geometry = geometry;
            entry.points.clear();
        }
        entry.points.insert(key, point);
        Ok(point)
    }

    fn spline_curve<'a>(
        &'a self,
        spline: &SketchSpline,
        values: &[f64],
    ) -> Result<Shape<'a>, ModelError> {
        if let Some(basis) = &spline.basis {
            let poles = spline
                .points
                .iter()
                .map(|id| {
                    let p = self.point(id, values);
                    Vec3::new(p.x, p.y, 0.)
                })
                .collect();
            return Ok(self.native.create_curve_wire(
                &[spline.explicit_segment(poles, basis.weights(self.parameters)?)?],
                basis.periodic,
            )?);
        }
        let away = |at: &str| -> Option<Vec3> {
            let neighbor = self.spline_tangents.get(&(spline.id.as_str(), at))?;
            let p = self.point(at, values);
            let d = match self.entities[neighbor] {
                Entity::Line(l) => {
                    let other = self.point(if l.start == at { &l.end } else { &l.start }, values);
                    SketchPoint2 {
                        x: other.x - p.x,
                        y: other.y - p.y,
                    }
                }
                Entity::Arc(a) => {
                    let c = self.point(&a.center, values);
                    let k = (if a.clockwise { -1.0 } else { 1.0 })
                        * (if a.start == at { 1.0 } else { -1.0 });
                    SketchPoint2 {
                        x: -(p.y - c.y) * k,
                        y: (p.x - c.x) * k,
                    }
                }
                _ => unreachable!("validated spline tangency"),
            };
            Some(Vec3::new(d.x, d.y, 0.0))
        };
        let periodic = spline.closed();
        let through = if periodic {
            &spline.points[..spline.points.len() - 1]
        } else {
            &spline.points[..]
        };
        let segment = CurveSegment::Spline {
            points: through
                .iter()
                .map(|id| {
                    let p = self.point(id, values);
                    Vec3::new(p.x, p.y, 0.0)
                })
                .collect(),
            start_tangent: away(&spline.points[0]).map(|d| Vec3::new(-d.x, -d.y, -d.z)),
            end_tangent: away(spline.points.last().unwrap()),
            periodic,
        };
        Ok(self.native.create_curve_wire(&[segment], periodic)?)
    }

    fn spline_contact_data(
        &self,
        spline: &str,
        point: &str,
        values: &[f64],
    ) -> Result<(SketchPoint2, SketchPoint2, SketchPoint2), ModelError> {
        let Entity::Spline(s) = self.entities[spline] else {
            unreachable!("validated spline contact")
        };
        let p = self.point(point, values);
        let wire = self.spline_curve(s, values)?;
        let (native, tangent) = self
            .native
            .curve_closest_data(&wire, Vec3::new(p.x, p.y, 0.))?;
        let q = SketchPoint2 {
            x: native.x,
            y: native.y,
        };
        // Direction is independent of knot parameter scaling.
        let tangent_scale = tangent.x.abs().max(tangent.y.abs());
        if tangent_scale == 0. {
            return Err(ModelError::new("spline contact has a zero planar tangent"));
        }
        Ok((
            p,
            q,
            SketchPoint2 {
                x: tangent.x / tangent_scale,
                y: tangent.y / tangent_scale,
            },
        ))
    }

    pub(super) fn line_spline_contact(
        &self,
        line: &str,
        spline: &str,
        point: &str,
        values: &[f64],
    ) -> Result<([SketchPoint2; 3], f64), ModelError> {
        let (p, q, tangent) = self.spline_contact_data(spline, point, values)?;
        let (a, b) = self.line(line, values);
        let length = line_length((a, b));
        if length <= f64::EPSILON {
            return Err(ModelError::new("line-spline tangency has a zero line"));
        }
        let dx = (b.x - a.x) / length;
        let dy = (b.y - a.y) / length;
        let distance = (p.y - a.y) * dx - (p.x - a.x) * dy;
        let foot = SketchPoint2 {
            x: p.x + distance * dy,
            y: p.y - distance * dx,
        };
        let sine = normalized_cross(
            (a, b),
            (
                SketchPoint2 { x: 0., y: 0. },
                SketchPoint2 {
                    x: tangent.x,
                    y: tangent.y,
                },
            ),
        )?;
        Ok(([p, q, foot], sine))
    }

    pub(super) fn spline_spline_contact(
        &self,
        first: &str,
        second: &str,
        point: &str,
        values: &[f64],
    ) -> Result<([SketchPoint2; 3], f64), ModelError> {
        let (p, a, u) = self.spline_contact_data(first, point, values)?;
        let (_, b, v) = self.spline_contact_data(second, point, values)?;
        let zero = SketchPoint2 { x: 0., y: 0. };
        Ok(([p, a, b], normalized_cross((zero, u), (zero, v))?))
    }

    pub(super) fn ellipse_ellipse_contact(
        &self,
        first: &str,
        second: &str,
        point: &str,
        values: &[f64],
    ) -> Result<([SketchPoint2; 3], f64), ModelError> {
        let (p, a, u) = self.ellipse_contact_data(first, point, values)?;
        let (_, b, v) = self.ellipse_contact_data(second, point, values)?;
        let zero = SketchPoint2 { x: 0., y: 0. };
        Ok(([p, a, b], normalized_cross((zero, u), (zero, v))?))
    }

    fn ellipse_contact_data(
        &self,
        ellipse: &str,
        point: &str,
        values: &[f64],
    ) -> Result<(SketchPoint2, SketchPoint2, SketchPoint2), ModelError> {
        let p = self.point(point, values);
        let Entity::Ellipse(e) = self.entities[ellipse] else {
            unreachable!("validated ellipse contact")
        };
        let c = self.point(&e.center, values);
        let major = self.point(&e.major, values);
        let minor = self.point(&e.minor, values);
        let ra = line_length((c, major));
        let rb = line_length((c, minor));
        if ra <= f64::EPSILON || rb <= f64::EPSILON {
            return Err(ModelError::new("ellipse tangency needs positive axes"));
        }
        let axis = Vec3::new((major.x - c.x) / ra, (major.y - c.y) / ra, 0.);
        // Trial coordinates can temporarily swap axis lengths. Preserve the locus
        // while satisfying OCCT's major >= minor constructor requirement.
        let (axis, ra, rb) = if ra >= rb {
            (axis, ra, rb)
        } else {
            (Vec3::new(-axis.y, axis.x, 0.), rb, ra)
        };
        let wire = self.native.create_ellipse_wire_axes(
            Vec3::new(c.x, c.y, 0.),
            Vec3::new(0., 0., 1.),
            axis,
            ra,
            rb,
        )?;
        let (b, v) = self
            .native
            .curve_closest_data(&wire, Vec3::new(p.x, p.y, 0.))?;
        let scale = v.x.abs().max(v.y.abs());
        if scale == 0. {
            return Err(ModelError::new("ellipse contact has a zero planar tangent"));
        }
        Ok((
            p,
            SketchPoint2 { x: b.x, y: b.y },
            SketchPoint2 {
                x: v.x / scale,
                y: v.y / scale,
            },
        ))
    }

    pub(super) fn spline_ellipse_contact(
        &self,
        spline: &str,
        ellipse: &str,
        point: &str,
        values: &[f64],
    ) -> Result<([SketchPoint2; 3], f64), ModelError> {
        let (p, a, u) = self.spline_contact_data(spline, point, values)?;
        let (_, b, v) = self.ellipse_contact_data(ellipse, point, values)?;
        let zero = SketchPoint2 { x: 0., y: 0. };
        Ok(([p, a, b], normalized_cross((zero, u), (zero, v))?))
    }

    pub(super) fn spline_circle_contact(
        &self,
        spline: &str,
        circle: &str,
        point: &str,
        values: &[f64],
    ) -> Result<SketchSplineCircleContact, ModelError> {
        let (p, q, tangent) = self.spline_contact_data(spline, point, values)?;
        self.curve_circle_contact(p, q, tangent, circle, values)
    }

    pub(super) fn ellipse_circle_contact(
        &self,
        ellipse: &str,
        circle: &str,
        point: &str,
        values: &[f64],
    ) -> Result<SketchEllipseCircleContact, ModelError> {
        let (p, q, tangent) = self.ellipse_contact_data(ellipse, point, values)?;
        self.curve_circle_contact(p, q, tangent, circle, values)
    }

    fn curve_circle_contact(
        &self,
        p: SketchPoint2,
        q: SketchPoint2,
        tangent: SketchPoint2,
        circle: &str,
        values: &[f64],
    ) -> Result<SketchSplineCircleContact, ModelError> {
        let (center, rim) = self.radius_points(circle);
        let c = self.point(center, values);
        let radius = line_length((c, self.point(rim, values)));
        let (rx, ry) = (p.x - c.x, p.y - c.y);
        let distance = rx.hypot(ry);
        if radius <= f64::EPSILON || distance <= f64::EPSILON {
            return Err(ModelError::new(
                "circular tangency needs a positive radius and a contact away from the centre",
            ));
        }
        let on_circle = SketchPoint2 {
            x: c.x + radius * rx / distance,
            y: c.y + radius * ry / distance,
        };
        let zero = SketchPoint2 { x: 0., y: 0. };
        let sine = normalized_cross(
            (zero, tangent),
            (
                zero,
                SketchPoint2 {
                    x: -ry / distance,
                    y: rx / distance,
                },
            ),
        )?;
        Ok(SketchSplineCircleContact {
            points: [p, q, on_circle],
            tangent_sine: sine,
            arc_span_residual_mm: self.arc_contact_residual(circle, (rx, ry), values),
        })
    }

    fn center_point(&self, id: &str) -> &str {
        match self.entities[id] {
            Entity::Circle(c) => &c.center,
            Entity::Arc(a) => &a.center,
            Entity::Ellipse(e) => &e.center,
            _ => unreachable!("validated concentric curve"),
        }
    }

    fn circular_points(&self, id: &str) -> Vec<&str> {
        match self.entities[id] {
            Entity::Circle(c) => vec![&c.center, &c.rim],
            Entity::Arc(a) => vec![&a.center, &a.start, &a.end],
            _ => unreachable!("validated circular tangency curve"),
        }
    }

    /// Distance along the supporting circle from an excluded contact to the
    /// nearest directed arc endpoint. Zero in the span, including endpoints.
    fn arc_contact_residual(&self, id: &str, direction: (f64, f64), values: &[f64]) -> Option<f64> {
        let Entity::Arc(arc) = self.entities[id] else {
            return None;
        };
        let c = self.point(&arc.center, values);
        let a = self.point(&arc.start, values);
        let b = self.point(&arc.end, values);
        let sign = if arc.clockwise { -1. } else { 1. };
        let start = (a.y - c.y).atan2(a.x - c.x);
        let end = (b.y - c.y).atan2(b.x - c.x);
        let at = direction.1.atan2(direction.0);
        let tau = std::f64::consts::TAU;
        let span = (sign * (end - start)).rem_euclid(tau);
        let along = (sign * (at - start)).rem_euclid(tau);
        let violation = if along <= span {
            0.
        } else {
            (along - span).min(tau - along)
        };
        Some(violation * line_length((c, a)))
    }

    fn radius_points<'a>(&'a self, id: &str) -> (&'a str, &'a str) {
        match self.entities[id] {
            Entity::Circle(c) => (&c.center, &c.rim),
            Entity::Arc(a) => (&a.center, &a.start),
            _ => unreachable!("validated radius curve"),
        }
    }

    /// Points whose coordinates move a curve's length or shape.
    fn curve_refs<'a>(&'a self, id: &'a str) -> Vec<&'a str> {
        match self.entities[id] {
            Entity::Line(l) => vec![l.start.as_str(), l.end.as_str()],
            Entity::Circle(c) => vec![c.center.as_str(), c.rim.as_str()],
            Entity::Arc(a) => vec![a.center.as_str(), a.start.as_str(), a.end.as_str()],
            Entity::Ellipse(e) => {
                let mut refs = vec![e.center.as_str(), e.major.as_str(), e.minor.as_str()];
                if let Some((start, end)) = e.arc_ends() {
                    refs.extend([start, end]);
                }
                refs
            }
            Entity::Spline(spline) => self.spline_neighbor_refs(spline),
        }
    }

    /// A point in an ellipse's frame: its sketch-plane eccentric angle
    /// (counterclockwise from the major vertex), the scaled implicit
    /// membership residual, and the curve tangent there.
    fn ellipse_local(
        &self,
        e: &SketchEllipse,
        p: SketchPoint2,
        values: &[f64],
    ) -> Result<EllipseLocal, ModelError> {
        let c = self.point(&e.center, values);
        let major = self.point(&e.major, values);
        let a = line_length((c, major));
        let b = line_length((c, self.point(&e.minor, values)));
        if a <= f64::EPSILON || b <= f64::EPSILON {
            return Err(ModelError::new("ellipse needs positive axes"));
        }
        let u = ((major.x - c.x) / a, (major.y - c.y) / a);
        let (dx, dy) = (p.x - c.x, p.y - c.y);
        let (x, y) = (dx * u.0 + dy * u.1, -dx * u.1 + dy * u.0);
        let (tx, ty) = (-y / (b * b), x / (a * a));
        Ok(EllipseLocal {
            angle: (y / b).atan2(x / a),
            membership: ((x / a).hypot(y / b) - 1.0) * b,
            tangent: SketchPoint2 {
                x: tx * u.0 - ty * u.1,
                y: tx * u.1 + ty * u.0,
            },
        })
    }

    /// Tangent direction (any length) and curvature vector of a curve at
    /// one of its points: zero for lines, toward the centre for circles,
    /// closed form for ellipses, and native for splines.
    fn curve_frame(
        &self,
        id: &str,
        at: &str,
        values: &[f64],
    ) -> Result<(SketchPoint2, SketchPoint2), ModelError> {
        let p = self.point(at, values);
        let zero = SketchPoint2 { x: 0.0, y: 0.0 };
        Ok(match self.entities[id] {
            Entity::Line(l) => {
                let (a, b) = (self.point(&l.start, values), self.point(&l.end, values));
                (
                    SketchPoint2 {
                        x: b.x - a.x,
                        y: b.y - a.y,
                    },
                    zero,
                )
            }
            Entity::Circle(_) | Entity::Arc(_) => {
                let c = self.point(self.center_point(id), values);
                let r2 = (p.x - c.x).powi(2) + (p.y - c.y).powi(2);
                if r2 <= f64::EPSILON {
                    return Err(ModelError::new("curvature needs a positive radius"));
                }
                (
                    SketchPoint2 {
                        x: c.y - p.y,
                        y: p.x - c.x,
                    },
                    SketchPoint2 {
                        x: (c.x - p.x) / r2,
                        y: (c.y - p.y) / r2,
                    },
                )
            }
            Entity::Ellipse(e) => {
                let c = self.point(&e.center, values);
                let major = self.point(&e.major, values);
                let a = line_length((c, major));
                let b = line_length((c, self.point(&e.minor, values)));
                let theta = self.ellipse_local(e, p, values)?.angle;
                let u = ((major.x - c.x) / a, (major.y - c.y) / a);
                let world = |x: f64, y: f64| SketchPoint2 {
                    x: x * u.0 - y * u.1,
                    y: x * u.1 + y * u.0,
                };
                let (d1, d2) = (
                    (-a * theta.sin(), b * theta.cos()),
                    (-a * theta.cos(), -b * theta.sin()),
                );
                let speed2 = d1.0 * d1.0 + d1.1 * d1.1;
                let along = (d2.0 * d1.0 + d2.1 * d1.1) / speed2;
                (
                    world(d1.0, d1.1),
                    world(
                        (d2.0 - along * d1.0) / speed2,
                        (d2.1 - along * d1.1) / speed2,
                    ),
                )
            }
            Entity::Spline(spline) => {
                // The joint is one of the spline's ends (validated), so use
                // the native end derivative there rather than a projection.
                let wire = self.spline_curve(spline, values)?;
                let (start, end, start_tangent, end_tangent) = self.native.curve_end_data(&wire)?;
                let (point, tangent) = if self.entities[id].endpoints().0 == at {
                    (start, start_tangent)
                } else {
                    (end, end_tangent)
                };
                let k = self.native.curve_curvature_at(&wire, point)?;
                (
                    SketchPoint2 {
                        x: tangent.x,
                        y: tangent.y,
                    },
                    SketchPoint2 { x: k.x, y: k.y },
                )
            }
        })
    }

    /// Directed sweep of an arc from start to end, in [0, 2π).
    fn arc_sweep(&self, arc: &SketchArc, values: &[f64]) -> f64 {
        let c = self.point(&arc.center, values);
        let (a, b) = (self.point(&arc.start, values), self.point(&arc.end, values));
        let sign = if arc.clockwise { -1.0 } else { 1.0 };
        let start = (a.y - c.y).atan2(a.x - c.x);
        let end = (b.y - c.y).atan2(b.x - c.x);
        (sign * (end - start)).rem_euclid(std::f64::consts::TAU)
    }

    /// Length of a line, arc or circle in closed form, or of a full ellipse
    /// or spline measured on its native curve.
    fn crossing_curve(&self, id: &str, values: &[f64]) -> Result<Shape<'_>, ModelError> {
        let at = |id: &str| {
            let p = self.point(id, values);
            Vec3::new(p.x, p.y, 0.)
        };
        let normal = Vec3::new(0., 0., 1.);
        Ok(match self.entities[id] {
            Entity::Spline(s) => self.spline_curve(s, values)?,
            Entity::Line(l) => self.native.create_curve_wire(
                &[CurveSegment::Line {
                    start: at(&l.start),
                    end: at(&l.end),
                }],
                false,
            )?,
            Entity::Circle(c) => self.native.create_circle_wire(
                at(&c.center),
                normal,
                line_length((self.point(&c.center, values), self.point(&c.rim, values))),
            )?,
            Entity::Arc(a) => {
                let c = self.point(&a.center, values);
                let start = self.point(&a.start, values);
                let radius = line_length((c, start));
                let angle = (start.y - c.y).atan2(start.x - c.x)
                    + if a.clockwise { -1. } else { 1. } * self.arc_sweep(a, values) * 0.5;
                self.native.create_curve_wire(
                    &[CurveSegment::Arc {
                        start: at(&a.start),
                        middle: Vec3::new(
                            c.x + radius * angle.cos(),
                            c.y + radius * angle.sin(),
                            0.,
                        ),
                        end: at(&a.end),
                    }],
                    false,
                )?
            }
            Entity::Ellipse(e) => {
                if let Some((start, end)) = e.arc_ends() {
                    self.native.create_curve_wire(
                        &[e.native_arc(
                            [
                                at(&e.center),
                                at(&e.major),
                                at(&e.minor),
                                at(start),
                                at(end),
                            ],
                            normal,
                        )],
                        false,
                    )?
                } else {
                    let c = self.point(&e.center, values);
                    let major = self.point(&e.major, values);
                    let minor = self.point(&e.minor, values);
                    let (ra, rb) = (line_length((c, major)), line_length((c, minor)));
                    if ra <= f64::EPSILON || rb <= f64::EPSILON {
                        return Err(ModelError::new(
                            "selected crossing ellipse needs positive axes",
                        ));
                    }
                    let axis = Vec3::new((major.x - c.x) / ra, (major.y - c.y) / ra, 0.);
                    let (axis, ra, rb) = if ra >= rb {
                        (axis, ra, rb)
                    } else {
                        (Vec3::new(-axis.y, axis.x, 0.), rb, ra)
                    };
                    self.native
                        .create_ellipse_wire_axes(at(&e.center), normal, axis, ra, rb)?
                }
            }
        })
    }
    fn crossing_candidates(
        &self,
        first: &str,
        second: &str,
        values: &[f64],
    ) -> Result<Vec<Vec3>, ModelError> {
        let (first, second) = if first < second {
            (first, second)
        } else {
            (second, first)
        };
        let key = (first.to_owned(), second.to_owned());
        let geometry = self
            .curve_refs(first)
            .into_iter()
            .chain(self.curve_refs(second))
            .map(|id| {
                let p = self.point(id, values);
                [p.x, p.y]
            })
            .collect::<Vec<_>>();
        let cached = {
            let cache = self.crossing_cache.borrow();
            cache
                .get(&key)
                .filter(|c| c.geometry == geometry)
                .map(|c| c.points.clone())
        };
        let points = if let Some(points) = cached {
            points
        } else {
            let cache = self.crossing_cache.borrow();
            if !cache.contains_key(&key) && cache.len() >= 256 {
                return Err(ModelError::new(
                    "selected crossings exceed 256 distinct curve pairs",
                ));
            }
            let retained = cache
                .iter()
                .filter(|(k, _)| **k != key)
                .map(|(_, v)| v.geometry.len())
                .sum::<usize>();
            let candidates = cache
                .iter()
                .filter(|(k, _)| **k != key)
                .map(|(_, v)| v.points.len())
                .sum::<usize>();
            if retained + geometry.len() > 65536 {
                return Err(ModelError::new(
                    "selected crossing cache exceeds its reference or candidate budget",
                ));
            }
            drop(cache);
            let a = self.crossing_curve(first, values)?;
            let b = self.crossing_curve(second, values)?;
            let points = self.native.planar_curve_intersections(&a, &b, 1000)?;
            if candidates + points.len() > 10000 {
                return Err(ModelError::new(
                    "selected crossing cache exceeds its candidate budget",
                ));
            }

            self.crossing_cache.borrow_mut().insert(
                key,
                CrossingCache {
                    geometry,
                    points: points.clone(),
                },
            );
            points
        };
        Ok(points)
    }
    fn selected_crossing(
        &self,
        first: &str,
        second: &str,
        near: &VectorExpr,
        maximum_distance: &ScalarExpr,
        values: &[f64],
    ) -> Result<SketchPoint2, ModelError> {
        let points = self.crossing_candidates(first, second, values)?;
        let hint = vector(near, self.parameters, Dimension::Length)?;
        let radius = scalar(maximum_distance, self.parameters, Dimension::Length)?;
        let mut candidates = points
            .iter()
            .filter(|p| (p.x - hint.x).hypot(p.y - hint.y) <= radius);
        let found = candidates.next().ok_or_else(|| {
            ModelError::new("selected crossing found no crossing within maximum distance")
        })?;
        if candidates.next().is_some() {
            return Err(ModelError::new(
                "selected crossing matches multiple crossings within maximum distance",
            ));
        }
        Ok(SketchPoint2 {
            x: found.x,
            y: found.y,
        })
    }

    fn curve_length(&self, id: &str, values: &[f64]) -> Result<f64, ModelError> {
        let native = |wire: Shape<'_>| -> Result<f64, ModelError> {
            let mut total = 0.0;
            for edge in self.native.subshapes(&wire, ShapeType::Edge)? {
                total += self.native.edge_length(&edge)?;
            }
            Ok(total)
        };
        Ok(match self.entities[id] {
            Entity::Line(l) => {
                line_length((self.point(&l.start, values), self.point(&l.end, values)))
            }
            Entity::Circle(c) => {
                std::f64::consts::TAU
                    * line_length((self.point(&c.center, values), self.point(&c.rim, values)))
            }
            Entity::Arc(a) => {
                line_length((self.point(&a.center, values), self.point(&a.start, values)))
                    * self.arc_sweep(a, values)
            }
            Entity::Ellipse(e) if e.arc_ends().is_some() => {
                let (start, end) = e.arc_ends().unwrap();
                let at = |id: &str| {
                    let p = self.point(id, values);
                    Vec3::new(p.x, p.y, 0.)
                };
                native(self.native.create_curve_wire(
                    &[e.native_arc(
                        [
                            at(&e.center),
                            at(&e.major),
                            at(&e.minor),
                            at(start),
                            at(end),
                        ],
                        Vec3::new(0., 0., 1.),
                    )],
                    false,
                )?)?
            }
            Entity::Ellipse(e) => {
                let c = self.point(&e.center, values);
                let major = self.point(&e.major, values);
                let minor = self.point(&e.minor, values);
                let (ra, rb) = (line_length((c, major)), line_length((c, minor)));
                if ra <= f64::EPSILON || rb <= f64::EPSILON {
                    return Err(ModelError::new("ellipse length needs positive axes"));
                }
                let axis = Vec3::new((major.x - c.x) / ra, (major.y - c.y) / ra, 0.);
                let (axis, ra, rb) = if ra >= rb {
                    (axis, ra, rb)
                } else {
                    (Vec3::new(-axis.y, axis.x, 0.), rb, ra)
                };
                native(self.native.create_ellipse_wire_axes(
                    Vec3::new(c.x, c.y, 0.),
                    Vec3::new(0., 0., 1.),
                    axis,
                    ra,
                    rb,
                )?)?
            }
            Entity::Spline(spline) => native(self.spline_curve(spline, values)?)?,
        })
    }

    fn point(&self, id: &str, values: &[f64]) -> SketchPoint2 {
        if let Some(terms) = self.weighted_points.get(id) {
            let mut p = SketchPoint2 { x: 0., y: 0. };
            for &(source, weight) in terms {
                let q = self.point(source, values);
                p.x += weight * q.x;
                p.y += weight * q.y;
            }
            return p;
        }
        self.variables.get(id).map_or_else(
            || self.fixed[id],
            |index| SketchPoint2 {
                x: values[*index],
                y: values[*index + 1],
            },
        )
    }

    fn line(&self, id: &str, values: &[f64]) -> (SketchPoint2, SketchPoint2) {
        let line = self.lines[id];
        (
            self.point(&line.start, values),
            self.point(&line.end, values),
        )
    }

    pub(super) fn line_search(
        &self,
        values: &[f64],
        step: &[f64],
        residual: &[f64],
        report: &mut SketchSolverReport,
    ) -> Option<(Vec<f64>, Vec<f64>)> {
        let cost_scale = max_abs(residual).max(1.);
        let current_cost = scaled_cost(residual, cost_scale);
        let mut fraction = 1.;
        while fraction >= 1e-6 {
            let trial: Vec<_> = values
                .iter()
                .zip(step)
                .map(|(v, d)| v + fraction * d)
                .collect();
            let result = if trial.iter().all(|v| v.is_finite()) {
                self.residuals(&trial)
            } else {
                Err(ModelError::new("trial coordinates are not finite"))
            };
            match result {
                Ok(r) if scaled_cost(&r, cost_scale) < current_cost => return Some((trial, r)),
                Err(e) => report.last_rejected_trial = Some(e.message),
                _ => {}
            }
            report.rejected_trials += 1;
            fraction *= 0.5;
        }
        None
    }

    fn residuals(&self, values: &[f64]) -> Result<Vec<f64>, ModelError> {
        let mut residuals = Vec::new();
        for equation in self.equations() {
            self.equation_residuals(equation, values, &mut residuals)?;
        }
        Ok(residuals)
    }

    pub(super) fn constraint_residuals(
        &self,
        constraint: &SketchConstraint,
        values: &[f64],
        residuals: &mut Vec<f64>,
    ) -> Result<(), ModelError> {
        let start = residuals.len();
        match constraint {
            SketchConstraint::Angle {
                first,
                second,
                value,
            } => {
                let angle =
                    normalized_cross(self.line(first, values), self.line(second, values))?.atan2(
                        normalized_dot(self.line(first, values), self.line(second, values))?,
                    );
                let target = scalar(value, self.parameters, Dimension::Scalar)?;
                let delta = angle - target;
                residuals.push(delta.sin().atan2(delta.cos()));
            }
            SketchConstraint::Radius { curve, value }
            | SketchConstraint::Diameter { curve, value } => {
                let (center, rim) = self.radius_points(curve);
                let radius = line_length((self.point(center, values), self.point(rim, values)));
                let factor = if matches!(constraint, SketchConstraint::Diameter { .. }) {
                    2.0
                } else {
                    1.0
                };
                residuals
                    .push(factor * radius - scalar(value, self.parameters, Dimension::Length)?);
            }
            SketchConstraint::Symmetric {
                first,
                second,
                axis,
            } => {
                let (a, b) = (self.point(first, values), self.point(second, values));
                let (u, v) = self.line(axis, values);
                let (dx, dy) = (v.x - u.x, v.y - u.y);
                let length = dx.hypot(dy);
                if length <= f64::EPSILON {
                    return Err(ModelError::new("symmetry axis has zero length"));
                }
                residuals.push(
                    ((a.x + b.x) * 0.5 - u.x) * dy / length
                        - ((a.y + b.y) * 0.5 - u.y) * dx / length,
                );
                residuals.push((b.x - a.x) * dx / length + (b.y - a.y) * dy / length);
            }
            SketchConstraint::PointOnCurve { point, curve } => {
                let p = self.point(point, values);
                match self.entities[curve.as_str()] {
                    Entity::Line(line) => {
                        let (a, b) = (
                            self.point(&line.start, values),
                            self.point(&line.end, values),
                        );
                        let length = line_length((a, b));
                        if length <= f64::EPSILON {
                            return Err(ModelError::new("point-on-line has a zero length line"));
                        }
                        residuals
                            .push(((p.x - a.x) * (b.y - a.y) - (p.y - a.y) * (b.x - a.x)) / length);
                    }
                    Entity::Circle(_) | Entity::Arc(_) => {
                        let (center, rim) = self.radius_points(curve);
                        let c = self.point(center, values);
                        residuals
                            .push(line_length((c, p)) - line_length((c, self.point(rim, values))));
                        if let Entity::Arc(arc) = self.entities[curve.as_str()] {
                            let a = self.point(&arc.start, values);
                            let b = self.point(&arc.end, values);
                            let sign = if arc.clockwise { -1.0 } else { 1.0 };
                            let start = (a.y - c.y).atan2(a.x - c.x);
                            let end = (b.y - c.y).atan2(b.x - c.x);
                            let at = (p.y - c.y).atan2(p.x - c.x);
                            let span = (sign * (end - start)).rem_euclid(std::f64::consts::TAU);
                            let along = (sign * (at - start)).rem_euclid(std::f64::consts::TAU);
                            let violation = if along <= span {
                                0.0
                            } else {
                                (along - span).min(std::f64::consts::TAU - along)
                            };
                            residuals.push(violation * line_length((c, a)));
                        }
                    }
                    Entity::Ellipse(e) => {
                        let c = self.point(&e.center, values);
                        let major = self.point(&e.major, values);
                        let minor = self.point(&e.minor, values);
                        let a = line_length((c, major));
                        let b = line_length((c, minor));
                        if a <= f64::EPSILON || b <= f64::EPSILON {
                            return Err(ModelError::new("point-on-ellipse needs positive axes"));
                        }
                        let x = ((p.x - c.x) * (major.x - c.x) + (p.y - c.y) * (major.y - c.y)) / a;
                        let y =
                            (-(p.x - c.x) * (major.y - c.y) + (p.y - c.y) * (major.x - c.x)) / a;
                        residuals.push(((x / a).hypot(y / b) - 1.0) * b);
                        if let Some((start, end)) = e.arc_ends() {
                            let at = |id: &str| -> Result<f64, ModelError> {
                                Ok(self.ellipse_local(e, self.point(id, values), values)?.angle)
                            };
                            let sign = if e.clockwise { -1.0 } else { 1.0 };
                            let (s, t) = (at(start)?, at(end)?);
                            let here = self.ellipse_local(e, p, values)?.angle;
                            let span = (sign * (t - s)).rem_euclid(std::f64::consts::TAU);
                            let along = (sign * (here - s)).rem_euclid(std::f64::consts::TAU);
                            let violation = if along <= span {
                                0.0
                            } else {
                                (along - span).min(std::f64::consts::TAU - along)
                            };
                            residuals.push(violation * b);
                        }
                    }
                    Entity::Spline(spline) => {
                        let curve = self.spline_curve(spline, values)?;
                        let nearest = self
                            .native
                            .curve_closest_point(&curve, Vec3::new(p.x, p.y, 0.0))?;
                        residuals.extend([p.x - nearest.x, p.y - nearest.y]);
                    }
                }
            }
            SketchConstraint::Tangent {
                first,
                second,
                point,
            } => {
                residuals.push(normalized_cross(
                    self.tangent(first, point, values)?,
                    self.tangent(second, point, values)?,
                )?);
            }
            SketchConstraint::Coincident { first, second } => {
                let (a, b) = (self.point(first, values), self.point(second, values));
                residuals.extend([a.x - b.x, a.y - b.y]);
            }
            SketchConstraint::Horizontal { line } => {
                let (a, b) = self.line(line, values);
                residuals.push(b.y - a.y);
            }
            SketchConstraint::Vertical { line } => {
                let (a, b) = self.line(line, values);
                residuals.push(b.x - a.x);
            }
            SketchConstraint::Parallel { first, second } => {
                residuals.push(normalized_cross(
                    self.line(first, values),
                    self.line(second, values),
                )?);
            }
            SketchConstraint::Perpendicular { first, second } => {
                residuals.push(normalized_dot(
                    self.line(first, values),
                    self.line(second, values),
                )?);
            }
            SketchConstraint::EqualRadius { first, second } => {
                let (a, b) = self.radius_points(first);
                let (c, d) = self.radius_points(second);
                residuals.push(
                    line_length((self.point(a, values), self.point(b, values)))
                        - line_length((self.point(c, values), self.point(d, values))),
                );
            }
            SketchConstraint::SplineSplineTangent {
                first,
                second,
                point,
            } => {
                let ([p, a, b], sine) = self.spline_spline_contact(first, second, point, values)?;
                residuals.extend([p.x - a.x, p.y - a.y, p.x - b.x, p.y - b.y, sine]);
            }
            SketchConstraint::EllipseEllipseTangent {
                first,
                second,
                point,
            } => {
                let ([p, a, b], sine) =
                    self.ellipse_ellipse_contact(first, second, point, values)?;
                residuals.extend([p.x - a.x, p.y - a.y, p.x - b.x, p.y - b.y, sine]);
            }
            SketchConstraint::SplineEllipseTangent {
                spline,
                ellipse,
                point,
            } => {
                let ([p, a, b], sine) =
                    self.spline_ellipse_contact(spline, ellipse, point, values)?;
                residuals.extend([p.x - a.x, p.y - a.y, p.x - b.x, p.y - b.y, sine]);
            }
            SketchConstraint::SplineCircleTangent {
                spline,
                circle,
                point,
            } => {
                let contact = self.spline_circle_contact(spline, circle, point, values)?;
                let [p, q, _] = contact.points;
                let (center, rim) = self.radius_points(circle);
                let c = self.point(center, values);
                residuals.extend([
                    p.x - q.x,
                    p.y - q.y,
                    line_length((c, p)) - line_length((c, self.point(rim, values))),
                    contact.tangent_sine,
                ]);
                if let Some(span) = contact.arc_span_residual_mm {
                    residuals.push(span);
                }
            }
            SketchConstraint::EllipseCircleTangent {
                ellipse,
                circle,
                point,
            } => {
                let contact = self.ellipse_circle_contact(ellipse, circle, point, values)?;
                let [p, q, _] = contact.points;
                let (center, rim) = self.radius_points(circle);
                let c = self.point(center, values);
                residuals.extend([
                    p.x - q.x,
                    p.y - q.y,
                    line_length((c, p)) - line_length((c, self.point(rim, values))),
                    contact.tangent_sine,
                ]);
                if let Some(span) = contact.arc_span_residual_mm {
                    residuals.push(span);
                }
            }
            SketchConstraint::LineSplineTangent {
                line,
                spline,
                point,
            } => {
                let ([p, q, foot], sine) = self.line_spline_contact(line, spline, point, values)?;
                let (a, b) = self.line(line, values);
                let length = line_length((a, b));
                residuals.extend([
                    p.x - q.x,
                    p.y - q.y,
                    ((p.y - foot.y) * (b.x - a.x) - (p.x - foot.x) * (b.y - a.y)) / length,
                    sine,
                ]);
            }
            SketchConstraint::LineEllipseTangent {
                line,
                ellipse,
                side,
            } => {
                let Entity::Ellipse(e) = self.entities[ellipse.as_str()] else {
                    unreachable!("validated ellipse tangency")
                };
                let (residual, _) = super::geometry::line_ellipse_contact(
                    self.line(line, values),
                    self.point(&e.center, values),
                    self.point(&e.major, values),
                    self.point(&e.minor, values),
                    *side,
                )?;
                residuals.push(residual);
            }
            SketchConstraint::LineCircleTangent { line, circle, side } => {
                let (c, r) = self.radius_points(circle);
                let center = self.point(c, values);
                let radius = line_length((center, self.point(r, values)));
                let (a, b) = self.line(line, values);
                let dx = b.x - a.x;
                let dy = b.y - a.y;
                let length = dx.hypot(dy);
                if length <= f64::EPSILON || radius <= f64::EPSILON {
                    return Err(ModelError::new(
                        "line-circle tangency requires a nonzero line and radius",
                    ));
                }
                let sign = match side {
                    SketchLineSide::Left => 1.,
                    SketchLineSide::Right => -1.,
                };
                residuals.push(
                    (center.y - a.y) * (dx / length)
                        - (center.x - a.x) * (dy / length)
                        - sign * radius,
                );
                if let Some(span) = self.arc_contact_residual(
                    circle,
                    (sign * dy / length, -sign * dx / length),
                    values,
                ) {
                    residuals.push(span);
                }
            }
            SketchConstraint::CircleCircleTangent {
                first,
                second,
                mode,
            } => {
                let (c1, r1) = self.radius_points(first);
                let (c2, r2) = self.radius_points(second);
                let a = self.point(c1, values);
                let b = self.point(c2, values);
                let ra = line_length((a, self.point(r1, values)));
                let rb = line_length((b, self.point(r2, values)));
                let distance = line_length((a, b));
                if ra <= f64::EPSILON || rb <= f64::EPSILON || distance <= f64::EPSILON {
                    return Err(ModelError::new(
                        "circle-circle tangency requires positive radii and distinct centres",
                    ));
                }
                let target = match mode {
                    SketchCircleTangency::External => ra + rb,
                    SketchCircleTangency::Internal => ra - rb,
                };
                if matches!(mode, SketchCircleTangency::Internal) && target <= f64::EPSILON {
                    return Err(ModelError::new(
                        "internal tangency requires the first circle to have a larger radius",
                    ));
                }
                residuals.push(distance - target);
                let direction = ((b.x - a.x) / distance, (b.y - a.y) / distance);
                if let Some(span) = self.arc_contact_residual(first, direction, values) {
                    residuals.push(span);
                }
                let sign = match mode {
                    SketchCircleTangency::External => -1.,
                    SketchCircleTangency::Internal => 1.,
                };
                if let Some(span) = self.arc_contact_residual(
                    second,
                    (sign * direction.0, sign * direction.1),
                    values,
                ) {
                    residuals.push(span);
                }
            }
            SketchConstraint::Concentric { first, second } => {
                let a = self.point(self.center_point(first), values);
                let b = self.point(self.center_point(second), values);
                residuals.extend([a.x - b.x, a.y - b.y]);
            }
            SketchConstraint::CrossingPoint {
                point,
                first,
                second,
                near,
                maximum_distance,
            } => {
                let target = self
                    .selected_crossing(first, second, near, maximum_distance, values)
                    .map_err(|e| e.context(&format!("selected crossing point '{point}'")))?;
                let p = self.point(point, values);
                residuals.extend([p.x - target.x, p.y - target.y]);
            }
            SketchConstraint::SplinePoint {
                point,
                spline,
                fraction,
            } if !self.sketch.affine_point_link(constraint) => {
                let target = self
                    .interpolated_spline_point(spline, fraction, values)
                    .map_err(|e| e.context(&format!("interpolated spline point '{point}'")))?;
                let p = self.point(point, values);
                residuals.extend([p.x - target.x, p.y - target.y]);
            }
            SketchConstraint::SplinePoint { point, .. } => {
                let terms = self.sketch.point_link_terms(constraint, self.parameters)?;
                let scale = terms.iter().map(|(_, w)| *w).fold(0_f64, f64::max);
                let sum = terms.iter().map(|(_, w)| w / scale).sum::<f64>();
                let mut target = SketchPoint2 { x: 0., y: 0. };
                for (id, w) in terms {
                    let p = self.point(id, values);
                    let c = (w / scale) / sum;
                    target.x += c * p.x;
                    target.y += c * p.y;
                }
                let p = self.point(point, values);
                residuals.extend([p.x - target.x, p.y - target.y]);
            }
            SketchConstraint::WeightedPoint { point, terms } => {
                let target = self.weighted_point(terms, values)?;
                let p = self.point(point, values);
                residuals.extend([p.x - target.x, p.y - target.y]);
            }
            SketchConstraint::Midpoint { point, line } => {
                let p = self.point(point, values);
                let (a, b) = self.line(line, values);
                residuals.extend([p.x - (a.x * 0.5 + b.x * 0.5), p.y - (a.y * 0.5 + b.y * 0.5)]);
            }
            SketchConstraint::PointLineDistance { point, line, value } => {
                let p = self.point(point, values);
                let (a, b) = self.line(line, values);
                let dx = b.x - a.x;
                let dy = b.y - a.y;
                let length = dx.hypot(dy);
                if length <= f64::EPSILON {
                    return Err(ModelError::new(
                        "point-line distance has a zero length line",
                    ));
                }
                residuals.push(
                    (p.y - a.y) * (dx / length)
                        - (p.x - a.x) * (dy / length)
                        - scalar(value, self.parameters, Dimension::Length)?,
                );
            }
            SketchConstraint::EqualLength { first, second } => {
                residuals.push(
                    line_length(self.line(first, values)) - line_length(self.line(second, values)),
                );
            }
            SketchConstraint::Distance {
                first,
                second,
                value,
            } => {
                let (a, b) = (self.point(first, values), self.point(second, values));
                let target = scalar(value, self.parameters, Dimension::Length)?;
                residuals.push((b.x - a.x).hypot(b.y - a.y) - target);
            }
            SketchConstraint::HorizontalDistance {
                first,
                second,
                value,
            } => {
                let (a, b) = (self.point(first, values), self.point(second, values));
                residuals.push(b.x - a.x - scalar(value, self.parameters, Dimension::Length)?);
            }
            SketchConstraint::VerticalDistance {
                first,
                second,
                value,
            } => {
                let (a, b) = (self.point(first, values), self.point(second, values));
                residuals.push(b.y - a.y - scalar(value, self.parameters, Dimension::Length)?);
            }
            SketchConstraint::LineAngle { line, value } => {
                let (a, b) = self.line(line, values);
                if line_length((a, b)) <= f64::EPSILON {
                    return Err(ModelError::new("sketch line has zero length"));
                }
                let delta = (b.y - a.y).atan2(b.x - a.x)
                    - scalar(value, self.parameters, Dimension::Scalar)?;
                residuals.push(delta.sin().atan2(delta.cos()));
            }
            SketchConstraint::ArcAngle { arc, value } => {
                let Entity::Arc(arc) = self.entities[arc.as_str()] else {
                    unreachable!("validated arc angle")
                };
                residuals.push(
                    self.arc_sweep(arc, values)
                        - scalar(value, self.parameters, Dimension::Scalar)?,
                );
            }
            SketchConstraint::CurveLength { curve, value } => {
                residuals.push(
                    self.curve_length(curve, values)?
                        - scalar(value, self.parameters, Dimension::Length)?,
                );
            }
            SketchConstraint::EqualCurveLength { first, second } => {
                residuals
                    .push(self.curve_length(first, values)? - self.curve_length(second, values)?);
            }
            SketchConstraint::Collinear { first, second } => {
                let (a, b) = self.line(first, values);
                let length = line_length((a, b));
                if length <= f64::EPSILON {
                    return Err(ModelError::new(
                        "collinear constraint has a zero length line",
                    ));
                }
                let (c, d) = self.line(second, values);
                for p in [c, d] {
                    residuals
                        .push(((p.x - a.x) * (b.y - a.y) - (p.y - a.y) * (b.x - a.x)) / length);
                }
            }
            SketchConstraint::CurvatureContinuous {
                first,
                second,
                point,
            } => {
                let (ta, ka) = self.curve_frame(first, point, values)?;
                let (tb, kb) = self.curve_frame(second, point, values)?;
                let (la, lb) = (ta.x.hypot(ta.y), tb.x.hypot(tb.y));
                if la <= f64::EPSILON || lb <= f64::EPSILON {
                    return Err(ModelError::new(
                        "curvature continuity needs defined tangents",
                    ));
                }
                residuals.push((ta.x * tb.y - ta.y * tb.x) / (la * lb));
                // Curvature vectors do not depend on direction; compare
                // them along the shared normal.
                residuals.push(((ka.x - kb.x) * -ta.y + (ka.y - kb.y) * ta.x) / la);
            }
            SketchConstraint::Coradial { first, second } => {
                let (a, ar) = self.radius_points(first);
                let (b, br) = self.radius_points(second);
                let (a, ar, b, br) = (
                    self.point(a, values),
                    self.point(ar, values),
                    self.point(b, values),
                    self.point(br, values),
                );
                residuals.extend([
                    a.x - b.x,
                    a.y - b.y,
                    line_length((a, ar)) - line_length((b, br)),
                ]);
            }
        }
        if residuals[start..].iter().any(|value| !value.is_finite()) {
            return Err(ModelError::new("sketch residual is not finite"));
        }
        Ok(())
    }

    fn constraint_points<'b>(&'b self, constraint: &'b SketchConstraint) -> Vec<&'b str> {
        match constraint {
            SketchConstraint::SplineSplineTangent {
                first,
                second,
                point,
            } => {
                let mut refs = vec![point.as_str()];
                for id in [first, second] {
                    let Entity::Spline(s) = self.entities[id.as_str()] else {
                        unreachable!("validated spline pair")
                    };
                    refs.extend(self.spline_neighbor_refs(s));
                }
                refs
            }
            SketchConstraint::EllipseEllipseTangent {
                first,
                second,
                point,
            } => {
                let mut refs = vec![point.as_str()];
                for id in [first, second] {
                    let Entity::Ellipse(e) = self.entities[id.as_str()] else {
                        unreachable!("validated ellipse pair")
                    };
                    refs.extend([e.center.as_str(), e.major.as_str(), e.minor.as_str()]);
                }
                refs
            }
            SketchConstraint::SplineEllipseTangent {
                spline,
                ellipse,
                point,
            } => {
                let Entity::Spline(s) = self.entities[spline.as_str()] else {
                    unreachable!("validated spline")
                };
                let Entity::Ellipse(e) = self.entities[ellipse.as_str()] else {
                    unreachable!("validated ellipse")
                };
                let mut refs = vec![
                    point.as_str(),
                    e.center.as_str(),
                    e.major.as_str(),
                    e.minor.as_str(),
                ];
                refs.extend(self.spline_neighbor_refs(s));
                refs
            }
            SketchConstraint::EllipseCircleTangent {
                ellipse,
                circle,
                point,
            } => {
                let Entity::Ellipse(e) = self.entities[ellipse.as_str()] else {
                    unreachable!("validated ellipse")
                };
                let mut refs = vec![
                    point.as_str(),
                    e.center.as_str(),
                    e.major.as_str(),
                    e.minor.as_str(),
                ];
                refs.extend(self.circular_points(circle));
                refs
            }
            SketchConstraint::SplineCircleTangent {
                spline,
                circle,
                point,
            } => {
                let Entity::Spline(s) = self.entities[spline.as_str()] else {
                    unreachable!("validated spline contact")
                };
                let mut refs = vec![point.as_str()];
                refs.extend(self.circular_points(circle));
                refs.extend(self.spline_neighbor_refs(s));
                refs
            }
            SketchConstraint::LineSplineTangent {
                line,
                spline,
                point,
            } => {
                let l = self.lines[line.as_str()];
                let Entity::Spline(s) = self.entities[spline.as_str()] else {
                    unreachable!("validated spline contact")
                };
                let mut refs = vec![l.start.as_str(), l.end.as_str(), point.as_str()];
                refs.extend(self.spline_neighbor_refs(s));
                refs
            }
            SketchConstraint::LineEllipseTangent { line, ellipse, .. } => {
                let l = self.lines[line.as_str()];
                let Entity::Ellipse(e) = self.entities[ellipse.as_str()] else {
                    unreachable!("validated ellipse tangency")
                };
                vec![
                    l.start.as_str(),
                    l.end.as_str(),
                    &e.center,
                    &e.major,
                    &e.minor,
                ]
            }
            SketchConstraint::LineCircleTangent { line, circle, .. } => {
                let l = self.lines[line.as_str()];
                let mut points = vec![l.start.as_str(), l.end.as_str()];
                points.extend(self.circular_points(circle));
                points
            }
            SketchConstraint::CircleCircleTangent { first, second, .. } => {
                let mut points = self.circular_points(first);
                points.extend(self.circular_points(second));
                points
            }
            SketchConstraint::Concentric { first, second } => {
                vec![self.center_point(first), self.center_point(second)]
            }
            SketchConstraint::CrossingPoint {
                point,
                first,
                second,
                ..
            } => std::iter::once(point.as_str())
                .chain(self.curve_refs(first))
                .chain(self.curve_refs(second))
                .collect(),
            SketchConstraint::SplinePoint { point, .. } => std::iter::once(point.as_str())
                .chain(self.sketch.point_link_sources(constraint).unwrap())
                .collect(),
            SketchConstraint::WeightedPoint { point, terms } => std::iter::once(point.as_str())
                .chain(terms.iter().map(|t| t.point.as_str()))
                .collect(),
            SketchConstraint::Midpoint { point, line }
            | SketchConstraint::PointLineDistance { point, line, .. } => {
                let l = self.lines[line.as_str()];
                vec![point.as_str(), &l.start, &l.end]
            }
            SketchConstraint::EqualRadius { first, second } => {
                let (a, b) = self.radius_points(first);
                let (c, d) = self.radius_points(second);
                vec![a, b, c, d]
            }
            SketchConstraint::Radius { curve, .. } | SketchConstraint::Diameter { curve, .. } => {
                let (a, b) = self.radius_points(curve);
                vec![a, b]
            }
            SketchConstraint::Symmetric {
                first,
                second,
                axis,
            } => {
                let line = self.lines[axis.as_str()];
                vec![first.as_str(), second.as_str(), &line.start, &line.end]
            }
            SketchConstraint::PointOnCurve { point, curve } => {
                let mut refs = vec![point.as_str()];
                match self.entities[curve.as_str()] {
                    Entity::Line(l) => refs.extend([l.start.as_str(), l.end.as_str()]),
                    Entity::Circle(c) => refs.extend([c.center.as_str(), c.rim.as_str()]),
                    Entity::Arc(a) => {
                        refs.extend([a.center.as_str(), a.start.as_str(), a.end.as_str()])
                    }
                    Entity::Ellipse(e) => {
                        refs.extend([e.center.as_str(), e.major.as_str(), e.minor.as_str()])
                    }
                    Entity::Spline(spline) => refs.extend(self.spline_neighbor_refs(spline)),
                }
                refs
            }
            SketchConstraint::Tangent {
                first,
                second,
                point,
            } => {
                let mut refs = Vec::new();
                for id in [first, second] {
                    match self.entities[id.as_str()] {
                        Entity::Spline(s) if s.endpoints.is_some() => {
                            refs.extend(s.points.iter().map(String::as_str))
                        }
                        entity => {
                            let (a, b) = entity.tangent_points(point);
                            refs.extend([a, b]);
                        }
                    }
                }
                refs
            }
            SketchConstraint::Coincident { first, second }
            | SketchConstraint::Distance { first, second, .. }
            | SketchConstraint::HorizontalDistance { first, second, .. }
            | SketchConstraint::VerticalDistance { first, second, .. } => {
                vec![first.as_str(), second.as_str()]
            }
            SketchConstraint::LineAngle { line, .. } => {
                let line = self.lines[line.as_str()];
                vec![line.start.as_str(), line.end.as_str()]
            }
            SketchConstraint::ArcAngle { arc: curve, .. }
            | SketchConstraint::CurveLength { curve, .. } => self.curve_refs(curve),
            SketchConstraint::EqualCurveLength { first, second }
            | SketchConstraint::Coradial { first, second } => {
                let mut refs = self.curve_refs(first);
                refs.extend(self.curve_refs(second));
                refs
            }
            SketchConstraint::CurvatureContinuous {
                first,
                second,
                point,
            } => {
                let mut refs = self.curve_refs(first);
                refs.extend(self.curve_refs(second));
                refs.push(point);
                refs
            }
            SketchConstraint::Collinear { first, second } => {
                let (a, b) = (self.lines[first.as_str()], self.lines[second.as_str()]);
                vec![
                    a.start.as_str(),
                    a.end.as_str(),
                    b.start.as_str(),
                    b.end.as_str(),
                ]
            }
            SketchConstraint::Horizontal { line } | SketchConstraint::Vertical { line } => {
                let line = self.lines[line.as_str()];
                vec![line.start.as_str(), line.end.as_str()]
            }
            SketchConstraint::Angle { first, second, .. }
            | SketchConstraint::Parallel { first, second }
            | SketchConstraint::Perpendicular { first, second }
            | SketchConstraint::EqualLength { first, second } => {
                let (a, b) = (self.lines[first.as_str()], self.lines[second.as_str()]);
                vec![
                    a.start.as_str(),
                    a.end.as_str(),
                    b.start.as_str(),
                    b.end.as_str(),
                ]
            }
        }
    }

    fn point_columns(&self, points: Vec<&str>) -> Vec<usize> {
        let mut source_points = Vec::new();
        let mut pending = points;
        let mut seen = HashSet::new();
        while let Some(point) = pending.pop() {
            if !seen.insert(point) {
                continue;
            }
            if let Some(terms) = self.weighted_points.get(point) {
                pending.extend(terms.iter().map(|&(p, _)| p));
            } else {
                source_points.push(point);
            }
        }
        let mut columns = source_points
            .into_iter()
            .filter_map(|point| self.variables.get(point))
            .flat_map(|&index| [index, index + 1])
            .collect::<Vec<_>>();
        columns.sort_unstable();
        columns.dedup();
        columns
    }

    /// Differentiate only the points named by each constraint, with one
    /// shared scratch vector. Duplicate endpoints contribute only once.
    fn jacobian(&self, values: &[f64]) -> Result<SparseJacobian, ModelError> {
        self.jacobian_labeled(values, None)
    }
    fn jacobian_labeled(
        &self,
        values: &[f64],
        mut labels: Option<(&mut Vec<SketchEquationSource>, &mut Vec<bool>)>,
    ) -> Result<SparseJacobian, ModelError> {
        let mut rows = Vec::new();
        let mut shifted = values.to_vec();
        for equation in self.equations() {
            let mut base = Vec::new();
            self.equation_residuals(equation, values, &mut base)?;
            if let Some((labels, _)) = labels.as_mut() {
                let source = match equation {
                    Equation::Constraint(index, _) => SketchEquationSource::Constraint { index },
                    Equation::Arc(e) => SketchEquationSource::ArcRadii {
                        entity: e.id.clone(),
                    },
                    Equation::Ellipse(e) => SketchEquationSource::EllipseAxes {
                        entity: e.id.clone(),
                    },
                    Equation::SplineEndpoints(e) => SketchEquationSource::SplineEndpoints {
                        entity: e.id.clone(),
                    },
                    Equation::Pattern(link) => SketchEquationSource::PatternLink {
                        pattern: link.pattern.id().into(),
                        point: link.copy.unwrap_or(link.source).into(),
                    },
                };
                labels.extend(std::iter::repeat_n(source, base.len()));
            }
            let mut term_rows = vec![Vec::new(); base.len()];
            let refs = match equation {
                Equation::Constraint(_, constraint) => self.constraint_points(constraint),
                Equation::Arc(arc) => {
                    vec![arc.center.as_str(), arc.start.as_str(), arc.end.as_str()]
                }
                Equation::SplineEndpoints(s) => {
                    let e = s.endpoints.as_ref().unwrap();
                    let mut refs = s.points.iter().map(String::as_str).collect::<Vec<_>>();
                    refs.extend([e.start.as_str(), e.end.as_str()]);
                    refs
                }
                Equation::Ellipse(e) => {
                    let mut refs = vec![e.center.as_str(), &e.major, &e.minor];
                    if let Some((start, end)) = e.arc_ends() {
                        refs.extend([start, end]);
                    }
                    refs
                }
                Equation::Pattern(link) => {
                    let mut refs = vec![link.source];
                    refs.extend(link.copy);
                    match link.pattern {
                        SketchPattern::Mirror { axis: line, .. }
                        | SketchPattern::Linear {
                            direction: line, ..
                        } => {
                            let l = self.lines[line.as_str()];
                            refs.extend([l.start.as_str(), l.end.as_str()]);
                        }
                        SketchPattern::Circular { center, .. } => refs.push(center),
                    }
                    refs
                }
            };
            let local_points: Vec<_> = refs.iter().map(|id| self.point(id, values)).collect();
            let extent = local_points
                .first()
                .map_or(1., |first| {
                    local_points.iter().fold(0_f64, |size, p| {
                        size.max((p.x - first.x).abs()).max((p.y - first.y).abs())
                    })
                })
                .max(1e-3);
            let columns = self.point_columns(refs);
            if let Some((_, structural)) = labels.as_mut() {
                structural.extend(std::iter::repeat_n(!columns.is_empty(), base.len()));
            }
            for column in columns {
                // Local geometry sets truncation scale; the coordinate ULP
                // sets representability. Use actual displaced denominators.
                let mut step = (DIFFERENCE_STEP * extent)
                    .max(8. * f64::EPSILON * values[column].abs())
                    .max(1e-12);
                let mut derivative = None;
                for _ in 0..8 {
                    shifted[column] = values[column] + step;
                    let plus = shifted[column];
                    let mut forward = Vec::new();
                    let forward_ok = self
                        .equation_residuals(equation, &shifted, &mut forward)
                        .is_ok();
                    shifted[column] = values[column] - step;
                    let minus = shifted[column];
                    let mut backward = Vec::new();
                    let backward_ok = self
                        .equation_residuals(equation, &shifted, &mut backward)
                        .is_ok();
                    shifted[column] = values[column];
                    if !forward_ok || !backward_ok {
                        self.derivative_fallbacks
                            .set(self.derivative_fallbacks.get() + 1);
                    }
                    let pair = match (forward_ok, backward_ok) {
                        (true, true) if plus != minus => Some((forward, backward, plus - minus)),
                        (true, false) if plus != values[column] => {
                            Some((forward, base.clone(), plus - values[column]))
                        }
                        (false, true) if minus != values[column] => {
                            Some((base.clone(), backward, values[column] - minus))
                        }
                        _ => None,
                    };
                    if let Some((a, b, denominator)) = pair {
                        let periodic = matches!(
                            equation,
                            Equation::Constraint(
                                _,
                                SketchConstraint::Angle { .. } | SketchConstraint::LineAngle { .. }
                            )
                        );
                        derivative = Some(
                            a.iter()
                                .zip(b)
                                .map(|(a, b)| {
                                    let delta = a - b;
                                    (if periodic {
                                        delta.sin().atan2(delta.cos())
                                    } else {
                                        delta
                                    }) / denominator
                                })
                                .collect::<Vec<_>>(),
                        );
                        break;
                    }
                    step *= 0.5;
                }
                let derivative = derivative.ok_or_else(|| {
                    ModelError::new("sketch cannot differentiate a degenerate neighborhood")
                })?;
                for (row, value) in term_rows.iter_mut().zip(derivative) {
                    push_derivative(row, column, value)?;
                }
            }
            rows.extend(term_rows);
        }
        Ok(SparseJacobian {
            rows,
            columns: values.len(),
        })
    }
}

fn normalized_cross(
    a: (SketchPoint2, SketchPoint2),
    b: (SketchPoint2, SketchPoint2),
) -> Result<f64, ModelError> {
    let (u, v) = (
        (a.1.x - a.0.x, a.1.y - a.0.y),
        (b.1.x - b.0.x, b.1.y - b.0.y),
    );
    let scale = u.0.hypot(u.1) * v.0.hypot(v.1);
    if scale <= f64::EPSILON {
        return Err(ModelError::new("sketch line has zero length"));
    }
    Ok((u.0 * v.1 - u.1 * v.0) / scale)
}

fn normalized_dot(
    a: (SketchPoint2, SketchPoint2),
    b: (SketchPoint2, SketchPoint2),
) -> Result<f64, ModelError> {
    let (u, v) = (
        (a.1.x - a.0.x, a.1.y - a.0.y),
        (b.1.x - b.0.x, b.1.y - b.0.y),
    );
    let scale = u.0.hypot(u.1) * v.0.hypot(v.1);
    if scale <= f64::EPSILON {
        return Err(ModelError::new("sketch line has zero length"));
    }
    Ok((u.0 * v.0 + u.1 * v.1) / scale)
}

fn scaled_cost(values: &[f64], scale: f64) -> f64 {
    values.iter().map(|v| (v / scale).powi(2)).sum()
}

fn push_derivative(
    row: &mut Vec<(usize, f64)>,
    column: usize,
    derivative: f64,
) -> Result<(), ModelError> {
    if !derivative.is_finite() {
        return Err(ModelError::new("sketch derivative is not finite"));
    }
    if derivative != 0.0 {
        row.push((column, derivative));
    }
    Ok(())
}
pub trait SketchDefinitionSolving {
    /// Solves using the assembly solver's sparse normal-matrix algebra.
    /// Analytic constraints touch only defining coordinates. Point-on-spline
    /// also touches its interpolation and tangent-neighbor points: native
    /// projection/interpolation and local finite differences scale with that
    /// curve, not the full sketch. Normal assembly is O(sum(row widths²));
    /// elimination depends on fill-in, small for independent components.
    /// Source constraints are solved before saved profile edits are applied.
    fn solve(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<SketchSolution, ModelError>;

    /// Read-only exact finite crossings of two current solved sketch curves, in
    /// sketch-local millimeters. Projections must already be materialized.
    /// Rejects overlaps and exhausted native budgets; at most 1000 candidates.
    /// This does not add constraints or verify downstream model requirements.
    fn finite_curve_intersections(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        first: &str,
        second: &str,
    ) -> Result<Vec<SketchPoint2>, ModelError>;

    /// Read-only unique closest parameter on a solved source spline, including
    /// generated copies. Hint/result are sketch-local millimeters. Projections
    /// must be materialized; equal nearest parameters reject the query.
    fn closest_spline_parameter(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        spline: &str,
        hint: SketchPoint2,
    ) -> Result<(f64, SketchPoint2), ModelError>;

    /// Warm start free points by stable point ID. New points use authored seeds,
    /// removed IDs are ignored, and fixed points always use the definition.
    fn solve_with_seed(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        seed: &HashMap<String, SketchPoint2>,
    ) -> Result<SketchSolution, ModelError>;
}

impl SketchDefinitionSolving for SketchDefinition {
    fn solve(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<SketchSolution, ModelError> {
        self.solve_internal(parameters, None)
    }

    fn finite_curve_intersections(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        first: &str,
        second: &str,
    ) -> Result<Vec<SketchPoint2>, ModelError> {
        if let std::borrow::Cow::Owned(expanded) = self.with_pattern_copies(parameters)? {
            return expanded.finite_curve_intersections(parameters, first, second);
        }
        let entities = self.entities();
        if first == second || !entities.contains_key(first) || !entities.contains_key(second) {
            return Err(ModelError::new(
                "crossing query requires two distinct known curves",
            ));
        }
        let solution = self.solve(parameters)?;
        self.check_acceptance(&solution)?;
        let fixed = self
            .points
            .iter()
            .map(|p| (p.id.as_str(), solution.points[&p.id]))
            .collect();
        let problem = SketchProblem::new(self, parameters, HashMap::new(), fixed)?;
        Ok(problem
            .crossing_candidates(first, second, &[])?
            .into_iter()
            .map(|p| SketchPoint2 { x: p.x, y: p.y })
            .collect())
    }

    fn closest_spline_parameter(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        spline: &str,
        hint: SketchPoint2,
    ) -> Result<(f64, SketchPoint2), ModelError> {
        if !hint.x.is_finite() || !hint.y.is_finite() {
            return Err(ModelError::new("sample hint must be finite"));
        }
        if let std::borrow::Cow::Owned(expanded) = self.with_pattern_copies(parameters)? {
            return expanded.closest_spline_parameter(parameters, spline, hint);
        }
        let source = self
            .splines
            .iter()
            .find(|s| s.id == spline)
            .ok_or_else(|| ModelError::new("sample query requires a source spline"))?;
        let solution = self.solve(parameters)?;
        self.check_acceptance(&solution)?;
        let fixed = self
            .points
            .iter()
            .map(|p| (p.id.as_str(), solution.points[&p.id]))
            .collect();
        let problem = SketchProblem::new(self, parameters, HashMap::new(), fixed)?;
        let curve = problem.spline_curve(source, &[])?;
        let (fraction, point) = problem
            .native
            .curve_closest_fraction(&curve, Vec3::new(hint.x, hint.y, 0.))?;
        Ok((
            fraction,
            SketchPoint2 {
                x: point.x,
                y: point.y,
            },
        ))
    }

    fn solve_with_seed(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        seed: &HashMap<String, SketchPoint2>,
    ) -> Result<SketchSolution, ModelError> {
        self.solve_internal(parameters, Some(seed))
    }
}

pub(crate) trait SketchDefinitionSolvingInternal {
    fn solve_internal(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        seed: Option<&HashMap<String, SketchPoint2>>,
    ) -> Result<SketchSolution, ModelError>;
}

impl SketchDefinitionSolvingInternal for SketchDefinition {
    fn solve_internal(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        seed: Option<&HashMap<String, SketchPoint2>>,
    ) -> Result<SketchSolution, ModelError> {
        if !self.projections.is_empty() {
            return Err(ModelError::new(
                "projected sketch requires source-geometry resolution before solving",
            ));
        }
        if let std::borrow::Cow::Owned(expanded) = self.with_patterns(Some(parameters))? {
            return expanded.solve_internal(parameters, seed);
        }
        self.validate(parameters)?;
        let mut values = Vec::new();
        let mut variables = HashMap::new();
        let mut fixed = HashMap::new();
        let dependent = self
            .constraints
            .iter()
            .filter(|c| self.affine_point_link(c))
            .filter_map(SketchConstraint::dependent_point)
            .collect::<HashSet<_>>();
        for point in &self.points {
            if dependent.contains(point.id.as_str()) {
                continue;
            }
            let mut value = SketchPoint2 {
                x: scalar(&point.x, parameters, Dimension::Length)?,
                y: scalar(&point.y, parameters, Dimension::Length)?,
            };
            if point.fixed {
                fixed.insert(point.id.as_str(), value);
            } else {
                if let Some(supplied) = seed.and_then(|s| s.get(&point.id)) {
                    if !supplied.x.is_finite() || !supplied.y.is_finite() {
                        return Err(ModelError::new(format!(
                            "sketch seed for '{}' is not finite",
                            point.id
                        )));
                    }
                    value = *supplied;
                }
                variables.insert(point.id.as_str(), values.len());
                values.extend([value.x, value.y]);
            }
        }
        let problem = SketchProblem::new(self, parameters, variables, fixed)?;
        let mut residual = problem.residuals(&values)?;
        let mut iterations = 0;
        let nonlinear = self.constraints.iter().any(|c| {
            matches!(
                c,
                SketchConstraint::Angle { .. }
                    | SketchConstraint::Radius { .. }
                    | SketchConstraint::Diameter { .. }
                    | SketchConstraint::EqualRadius { .. }
                    | SketchConstraint::Midpoint { .. }
                    | SketchConstraint::PointLineDistance { .. }
                    | SketchConstraint::Tangent { .. }
                    | SketchConstraint::LineCircleTangent { .. }
                    | SketchConstraint::LineEllipseTangent { .. }
                    | SketchConstraint::LineSplineTangent { .. }
                    | SketchConstraint::SplineCircleTangent { .. }
                    | SketchConstraint::SplineSplineTangent { .. }
                    | SketchConstraint::SplineEllipseTangent { .. }
                    | SketchConstraint::EllipseEllipseTangent { .. }
                    | SketchConstraint::EllipseCircleTangent { .. }
                    | SketchConstraint::CircleCircleTangent { .. }
                    | SketchConstraint::Concentric { .. }
                    | SketchConstraint::Symmetric { .. }
                    | SketchConstraint::PointOnCurve { .. }
                    | SketchConstraint::LineAngle { .. }
                    | SketchConstraint::ArcAngle { .. }
                    | SketchConstraint::CurveLength { .. }
                    | SketchConstraint::EqualCurveLength { .. }
                    | SketchConstraint::Collinear { .. }
                    | SketchConstraint::Coradial { .. }
                    | SketchConstraint::CurvatureContinuous { .. }
            )
        }) || self.splines.iter().any(|s| s.endpoints.is_some())
            || self.ellipses.iter().any(|e| e.arc_ends().is_some())
            || !self.patterns.is_empty();
        let mut damping = if nonlinear { 1e-8 } else { 0. };
        let mut report = SketchSolverReport {
            termination: SketchTermination::IterationLimit,
            accepted_steps: 0,
            rejected_trials: 0,
            derivative_fallbacks: 0,
            recovery_retries: 0,
            final_damping: damping,
            warm_started: self
                .points
                .iter()
                .any(|p| !p.fixed && seed.is_some_and(|s| s.contains_key(&p.id))),
            rank_reliable: false,
            last_rejected_trial: None,
        };
        while iterations < MAX_ITERATIONS && max_abs(&residual) > RESIDUAL_TOLERANCE {
            iterations += 1;
            let jacobian = problem.jacobian(&values)?;
            let mut accepted = None;
            let mut stationary = false;
            for retry in 0..8 {
                if retry > 0 {
                    report.recovery_retries += 1;
                }
                if let Some(step) = least_squares_step(&jacobian, &residual, damping, retry > 0) {
                    if step.iter().all(|v| *v == 0.) {
                        if retry > 0 {
                            stationary = true;
                            break;
                        }
                        continue;
                    }
                    accepted = problem.line_search(&values, &step, &residual, &mut report);
                }
                if accepted.is_some() {
                    break;
                }
                damping = (damping * 100.).max(1e-6);
            }
            let Some((trial, trial_residual)) = accepted else {
                report.termination = if stationary {
                    SketchTermination::Stationary
                } else {
                    SketchTermination::NoDescent
                };
                break;
            };
            report.accepted_steps += 1;
            values = trial;
            residual = trial_residual;
            damping = (damping * 0.1).max(if nonlinear { 1e-8 } else { 0. });
        }
        if max_abs(&residual) <= RESIDUAL_TOLERANCE {
            report.termination = SketchTermination::Converged;
        }
        report.final_damping = damping;
        let mut sources = Vec::new();
        let mut structural = Vec::new();
        let jacobian = problem.jacobian_labeled(&values, Some((&mut sources, &mut structural)))?;
        let (conditioned, _) = equilibrated(&jacobian, true);
        let rank = conditioned.normal_matrix().rank(PIVOT_TOLERANCE);
        let points = self
            .points
            .iter()
            .map(|p| (p.id.clone(), problem.point(&p.id, &values)))
            .collect::<HashMap<_, _>>();
        self.validate_curve_geometry(&points)?;
        let coordinates = self
            .points
            .iter()
            .filter(|p| problem.variables.contains_key(p.id.as_str()))
            .flat_map(|p| {
                [
                    SketchFreeCoordinate {
                        point: p.id.clone(),
                        axis: SketchCoordinateAxis::X,
                    },
                    SketchFreeCoordinate {
                        point: p.id.clone(),
                        axis: SketchCoordinateAxis::Y,
                    },
                ]
            })
            .collect::<Vec<_>>();
        let diagnosis = super::diagnosis::analyze(
            &jacobian,
            &residual,
            &sources,
            &structural,
            &coordinates,
            rank,
        );
        report.rank_reliable = diagnosis.local_rank == Some(rank);
        report.derivative_fallbacks = problem.derivative_fallbacks.get();
        Ok(SketchSolution {
            points,
            spline_weights: self
                .splines
                .iter()
                .filter_map(|s| s.basis.as_ref().map(|b| (s, b)))
                .map(|(s, b)| Ok((s.id.clone(), b.weights(parameters)?)))
                .collect::<Result<HashMap<_, _>, ModelError>>()?,
            solved: max_abs(&residual) <= RESIDUAL_TOLERANCE,
            iterations,
            max_residual: max_abs(&residual),
            free_degrees: values.len().saturating_sub(rank),
            redundant_equations: residual.len().saturating_sub(rank),
            diagnosis,
            solver_report: report,
        })
    }
}
