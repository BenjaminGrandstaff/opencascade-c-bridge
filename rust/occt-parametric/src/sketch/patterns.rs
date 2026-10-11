//! Linked sketch patterns. Expansion is O(copied points + copied entities)
//! after an O(sketch size) index per pattern; each copied point adds two
//! solver unknowns and two fixed-width link equations, so solve cost grows
//! with copies exactly as if they had been drawn and constrained by hand.
use super::*;
use std::borrow::Cow;

/// Bounds on generated geometry across all patterns of one sketch.
const MAX_COPIED_POINTS: usize = 20_000;
const MAX_COPIED_ENTITIES: usize = 10_000;

fn copy_id(pattern: &str, instance: u32, source: &str) -> String {
    format!("{pattern}:{instance}:{source}")
}

impl SketchPattern {
    /// Generated ID of `source`'s copy in instance `instance` (1-based).
    pub fn copy_id(&self, instance: u32, source: &str) -> String {
        copy_id(self.id(), instance, source)
    }
    pub fn id(&self) -> &str {
        match self {
            Self::Mirror { id, .. } | Self::Linear { id, .. } | Self::Circular { id, .. } => id,
        }
    }
    pub fn entities(&self) -> &[String] {
        match self {
            Self::Mirror { entities, .. }
            | Self::Linear { entities, .. }
            | Self::Circular { entities, .. } => entities,
        }
    }
    /// Generated instances; the source is instance 0 and is not copied.
    pub fn instances(&self) -> u32 {
        match self {
            Self::Mirror { .. } => 1,
            Self::Linear { count, .. } | Self::Circular { count, .. } => count.saturating_sub(1),
        }
    }
    fn shared(&self) -> &[String] {
        match self {
            Self::Mirror { on_axis, .. } => on_axis,
            _ => &[],
        }
    }
    /// Angular step between circular instances, in radians.
    pub fn circular_step(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<f64, ModelError> {
        match self {
            Self::Circular {
                angle: Some(angle), ..
            } => scalar(angle, parameters, Dimension::Scalar),
            Self::Circular { count, .. } => Ok(std::f64::consts::TAU / f64::from(*count)),
            _ => Err(ModelError::new(
                "only circular patterns have an angular step",
            )),
        }
    }
}

/// One copied point, or a shared mirror point held on the axis (`copy: None`).
#[derive(Clone, Copy)]
pub(super) struct PatternLink<'a> {
    pub(super) pattern: &'a SketchPattern,
    pub(super) instance: u32,
    pub(super) source: &'a str,
    pub(super) copy: Option<&'a str>,
}

fn defining_points(entity: Entity<'_>) -> Vec<&str> {
    match entity {
        Entity::Line(l) => vec![&l.start, &l.end],
        Entity::Circle(c) => vec![&c.center, &c.rim],
        Entity::Arc(a) => vec![&a.center, &a.start, &a.end],
        Entity::Ellipse(e) => {
            let mut points = vec![e.center.as_str(), &e.major, &e.minor];
            if let Some((start, end)) = e.arc_ends() {
                points.extend([start, end]);
            }
            points
        }
        Entity::Spline(s) => {
            let mut points = s.points.iter().map(String::as_str).collect::<Vec<_>>();
            if let Some(e) = &s.endpoints {
                points.extend([e.start.as_str(), e.end.as_str()]);
            }
            points
        }
    }
}

/// Image of `p` under instance `k` of a pattern, given seed geometry.
fn transform(
    pattern: &SketchPattern,
    k: u32,
    p: SketchPoint2,
    line: Option<(SketchPoint2, SketchPoint2)>,
    center: Option<SketchPoint2>,
    spacing: f64,
    step: f64,
) -> SketchPoint2 {
    match pattern {
        SketchPattern::Mirror { .. } => {
            let (u, v) = line.expect("mirror axis");
            let (dx, dy) = (v.x - u.x, v.y - u.y);
            let length2 = dx * dx + dy * dy;
            if length2 <= f64::EPSILON {
                return p;
            }
            let t = ((p.x - u.x) * dx + (p.y - u.y) * dy) / length2;
            let (fx, fy) = (u.x + t * dx, u.y + t * dy);
            SketchPoint2 {
                x: 2.0 * fx - p.x,
                y: 2.0 * fy - p.y,
            }
        }
        SketchPattern::Linear { .. } => {
            let (u, v) = line.expect("linear direction");
            let length = (v.x - u.x).hypot(v.y - u.y);
            if length <= f64::EPSILON {
                return p;
            }
            let d = spacing * f64::from(k) / length;
            SketchPoint2 {
                x: p.x + d * (v.x - u.x),
                y: p.y + d * (v.y - u.y),
            }
        }
        SketchPattern::Circular { .. } => {
            let c = center.expect("circular center");
            let (sin, cos) = (step * f64::from(k)).sin_cos();
            let (x, y) = (p.x - c.x, p.y - c.y);
            SketchPoint2 {
                x: c.x + cos * x - sin * y,
                y: c.y + sin * x + cos * y,
            }
        }
    }
}

impl SketchDefinition {
    /// Ordered distinct points copied by `pattern`, excluding shared mirror points.
    fn copied_points<'a>(
        &'a self,
        entities: &HashMap<&str, Entity<'a>>,
        pattern: &SketchPattern,
    ) -> Result<Vec<&'a str>, ModelError> {
        let mut seen = HashSet::new();
        let mut points = Vec::new();
        for id in pattern.entities() {
            let entity = entities.get(id.as_str()).copied().ok_or_else(|| {
                ModelError::new(format!(
                    "sketch pattern '{}' names unknown entity '{id}'",
                    pattern.id()
                ))
            })?;
            for point in defining_points(entity) {
                if seen.insert(point) {
                    points.push(point);
                }
            }
        }
        for shared in pattern.shared() {
            if !seen.contains(shared.as_str()) {
                return Err(ModelError::new(format!(
                    "mirror '{}' on-axis point '{shared}' is not a point of its entities",
                    pattern.id()
                )));
            }
        }
        points.retain(|p| !pattern.shared().iter().any(|s| s == p));
        Ok(points)
    }

    /// `None` when there is nothing to expand; `Some(true)` when every
    /// generated entity already exists (an expanded snapshot).
    fn patterns_expanded(&self) -> Result<Option<bool>, ModelError> {
        if self.patterns.is_empty() {
            return Ok(None);
        }
        let existing = self.entities();
        let (mut present, mut missing) = (0usize, 0usize);
        for pattern in &self.patterns {
            for k in 1..=pattern.instances() {
                for entity in pattern.entities() {
                    if existing.contains_key(copy_id(pattern.id(), k, entity).as_str()) {
                        present += 1;
                    } else {
                        missing += 1;
                    }
                }
            }
        }
        match (present, missing) {
            (_, 0) => Ok(Some(true)),
            (0, _) => Ok(Some(false)),
            _ => Err(ModelError::new(
                "sketch pattern copy IDs collide with existing geometry",
            )),
        }
    }

    fn validate_pattern_specs(&self) -> Result<(), ModelError> {
        let mut ids = HashSet::new();
        for pattern in &self.patterns {
            let id = pattern.id();
            if id.is_empty() || id.contains(':') || !ids.insert(id) {
                return Err(ModelError::new(
                    "sketch pattern IDs must be nonempty, unique and free of ':'",
                ));
            }
            let entities = pattern.entities();
            if entities.is_empty()
                || entities.len() > 1000
                || entities.iter().collect::<HashSet<_>>().len() != entities.len()
            {
                return Err(ModelError::new(format!(
                    "sketch pattern '{id}' needs 1-1000 distinct entities"
                )));
            }
            match pattern {
                SketchPattern::Linear { count, .. } | SketchPattern::Circular { count, .. }
                    if !(2..=1000).contains(count) =>
                {
                    return Err(ModelError::new(format!(
                        "sketch pattern '{id}' count must be 2-1000"
                    )));
                }
                SketchPattern::Mirror { axis, .. }
                | SketchPattern::Linear {
                    direction: axis, ..
                } if entities.contains(axis) => {
                    return Err(ModelError::new(format!(
                        "sketch pattern '{id}' cannot copy its own reference line"
                    )));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The sketch with linked pattern copies appended, as features and the
    /// viewer see it; borrowed when there are no patterns.
    pub fn with_pattern_copies(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Cow<'_, Self>, ModelError> {
        self.with_patterns(Some(parameters))
    }

    /// The sketch with pattern copies appended, borrowed when there are no
    /// patterns or they are already expanded. Copy points are seeded at the
    /// images of their sources' initial positions when parameters are given.
    pub(crate) fn with_patterns(
        &self,
        parameters: Option<&HashMap<String, ParameterValue>>,
    ) -> Result<Cow<'_, Self>, ModelError> {
        if !self.shapes.is_empty() {
            let expanded = self.with_shapes_expanded(parameters)?;
            return Ok(Cow::Owned(expanded.with_patterns(parameters)?.into_owned()));
        }
        if self.patterns.is_empty() {
            return Ok(Cow::Borrowed(self));
        }
        // Spec checks run first: a pattern with no instances generates
        // nothing, which would otherwise look already expanded.
        self.validate_pattern_specs()?;
        if self.patterns_expanded()?.is_none_or(|expanded| expanded) {
            return Ok(Cow::Borrowed(self));
        }
        let mut result = self.clone();
        let mut seeds = HashMap::new();
        if let Some(parameters) = parameters {
            for point in &self.points {
                seeds.insert(
                    point.id.clone(),
                    SketchPoint2 {
                        x: scalar(&point.x, parameters, Dimension::Length)?,
                        y: scalar(&point.y, parameters, Dimension::Length)?,
                    },
                );
            }
        }
        let seed = |seeds: &HashMap<String, SketchPoint2>, id: &str| {
            seeds
                .get(id)
                .copied()
                .unwrap_or(SketchPoint2 { x: 0.0, y: 0.0 })
        };
        let (mut copied_points, mut copied_entities) = (0usize, 0usize);
        for pattern in &self.patterns {
            let snapshot = result.clone();
            let entities = snapshot.entities();
            let sources = snapshot.copied_points(&entities, pattern)?;
            let points = snapshot
                .points
                .iter()
                .map(|p| p.id.as_str())
                .collect::<HashSet<_>>();
            let lines = snapshot
                .lines
                .iter()
                .map(|l| (l.id.as_str(), l))
                .collect::<HashMap<_, _>>();
            let id = pattern.id();
            let line = match pattern {
                SketchPattern::Mirror { axis: line, .. }
                | SketchPattern::Linear {
                    direction: line, ..
                } => {
                    let l = lines.get(line.as_str()).ok_or_else(|| {
                        ModelError::new(format!(
                            "sketch pattern '{id}' reference line '{line}' is unknown"
                        ))
                    })?;
                    if let SketchPattern::Mirror { .. } = pattern
                        && [&l.start, &l.end]
                            .iter()
                            .any(|p| sources.contains(&p.as_str()))
                    {
                        return Err(ModelError::new(format!(
                            "mirror '{id}' axis endpoints in its entities must be listed on_axis"
                        )));
                    }
                    Some((seed(&seeds, &l.start), seed(&seeds, &l.end)))
                }
                SketchPattern::Circular { .. } => None,
            };
            let center = match pattern {
                SketchPattern::Circular { center, .. } => {
                    if !points.contains(center.as_str()) {
                        return Err(ModelError::new(format!(
                            "sketch pattern '{id}' center '{center}' is unknown"
                        )));
                    }
                    if sources.contains(&center.as_str()) {
                        return Err(ModelError::new(format!(
                            "circular pattern '{id}' cannot copy its own center"
                        )));
                    }
                    Some(seed(&seeds, center))
                }
                _ => None,
            };
            let (spacing, step) = match (pattern, parameters) {
                (SketchPattern::Linear { spacing, .. }, Some(p)) => {
                    (scalar(spacing, p, Dimension::Length)?, 0.0)
                }
                (SketchPattern::Circular { .. }, Some(p)) => (0.0, pattern.circular_step(p)?),
                _ => (0.0, 0.0),
            };
            let mirror = matches!(pattern, SketchPattern::Mirror { .. });
            copied_points += sources.len() * pattern.instances() as usize;
            copied_entities += pattern.entities().len() * pattern.instances() as usize;
            if copied_points > MAX_COPIED_POINTS || copied_entities > MAX_COPIED_ENTITIES {
                return Err(ModelError::new(format!(
                    "sketch patterns exceed {MAX_COPIED_POINTS} copied points or {MAX_COPIED_ENTITIES} copied entities"
                )));
            }
            for k in 1..=pattern.instances() {
                let mut map = HashMap::new();
                for &source in &sources {
                    let name = copy_id(id, k, source);
                    let at = transform(
                        pattern,
                        k,
                        seed(&seeds, source),
                        line,
                        center,
                        spacing,
                        step,
                    );
                    seeds.insert(name.clone(), at);
                    result.points.push(SketchPoint {
                        id: name.clone(),
                        x: ScalarExpr::Literal(Quantity::length(at.x, LengthUnit::Millimeter)),
                        y: ScalarExpr::Literal(Quantity::length(at.y, LengthUnit::Millimeter)),
                        fixed: false,
                    });
                    map.insert(source, name);
                }
                let to = |p: &String| map.get(p.as_str()).cloned().unwrap_or_else(|| p.clone());
                for entity_id in pattern.entities() {
                    let name = copy_id(id, k, entity_id);
                    match entities[entity_id.as_str()] {
                        Entity::Line(l) => {
                            let (start, end) = if mirror {
                                (to(&l.end), to(&l.start))
                            } else {
                                (to(&l.start), to(&l.end))
                            };
                            result.lines.push(SketchLine {
                                id: name,
                                start,
                                end,
                            });
                        }
                        Entity::Circle(c) => result.circles.push(SketchCircle {
                            id: name,
                            center: to(&c.center),
                            rim: to(&c.rim),
                        }),
                        // Mirrored arcs swap their ends, as circular arcs do.
                        Entity::Ellipse(e) => {
                            let ends = e.start.as_ref().zip(e.end.as_ref()).map(|(start, end)| {
                                if mirror {
                                    (to(end), to(start))
                                } else {
                                    (to(start), to(end))
                                }
                            });
                            result.ellipses.push(SketchEllipse {
                                id: name,
                                center: to(&e.center),
                                major: to(&e.major),
                                minor: to(&e.minor),
                                start: ends.as_ref().map(|(start, _)| start.clone()),
                                end: ends.map(|(_, end)| end),
                                clockwise: e.clockwise,
                            });
                        }
                        // A reflection reverses orientation; swapping the ends
                        // restores the arc's turning sense along the reversed copy.
                        Entity::Arc(a) => {
                            let (start, end) = if mirror {
                                (to(&a.end), to(&a.start))
                            } else {
                                (to(&a.start), to(&a.end))
                            };
                            result.arcs.push(SketchArc {
                                id: name,
                                center: to(&a.center),
                                start,
                                end,
                                clockwise: a.clockwise,
                            });
                        }
                        Entity::Spline(s) => {
                            let mut copy = SketchSpline {
                                id: name,
                                points: s.points.iter().map(to).collect(),
                                basis: s.basis.clone(),
                                endpoints: s.endpoints.as_ref().map(|e| SketchSplineEndpoints {
                                    start: to(&e.start),
                                    end: to(&e.end),
                                }),
                            };
                            if mirror {
                                reverse_spline(&mut copy);
                            }
                            result.splines.push(copy);
                        }
                    }
                }
            }
        }
        Ok(Cow::Owned(result))
    }

    /// Link equations for an expanded sketch, in pattern order.
    pub(super) fn pattern_links(&self) -> Result<Vec<PatternLink<'_>>, ModelError> {
        let mut links = Vec::new();
        if self.patterns.is_empty() {
            return Ok(links);
        }
        let points = self
            .points
            .iter()
            .map(|p| (p.id.as_str(), p.id.as_str()))
            .collect::<HashMap<_, _>>();
        let entities = self.entities();
        for pattern in &self.patterns {
            for source in self.copied_points(&entities, pattern)? {
                for k in 1..=pattern.instances() {
                    let copy = points
                        .get(copy_id(pattern.id(), k, source).as_str())
                        .copied()
                        .ok_or_else(|| ModelError::new("sketch pattern is not expanded"))?;
                    links.push(PatternLink {
                        pattern,
                        instance: k,
                        source,
                        copy: Some(copy),
                    });
                }
            }
            for shared in pattern.shared() {
                links.push(PatternLink {
                    pattern,
                    instance: 1,
                    source: shared,
                    copy: None,
                });
            }
        }
        Ok(links)
    }

    /// Parameter checks that need values: nonzero spacing and circular
    /// steps that never bring an instance back onto an earlier one.
    pub(super) fn validate_pattern_values(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<(), ModelError> {
        for pattern in &self.patterns {
            match pattern {
                SketchPattern::Linear { id, spacing, .. } => {
                    let value = scalar(spacing, parameters, Dimension::Length)?;
                    if !value.is_finite() || value.abs() <= RESIDUAL_TOLERANCE {
                        return Err(ModelError::new(format!(
                            "linear pattern '{id}' spacing must be a nonzero length"
                        )));
                    }
                }
                SketchPattern::Circular { id, count, .. } => {
                    let step = pattern.circular_step(parameters)?;
                    if !step.is_finite()
                        || (1..*count).any(|k| (step * f64::from(k) * 0.5).sin().abs() <= 1e-9)
                    {
                        return Err(ModelError::new(format!(
                            "circular pattern '{id}' step makes instances coincide"
                        )));
                    }
                }
                SketchPattern::Mirror { .. } => {}
            }
        }
        Ok(())
    }
}

/// Reverses traversal of a spline copy: poles/points, weights and
/// multiplicities in reverse, knots reflected within their span.
fn reverse_spline(spline: &mut SketchSpline) {
    spline.points.reverse();
    if let Some(e) = &mut spline.endpoints {
        std::mem::swap(&mut e.start, &mut e.end);
    }
    if let Some(b) = &mut spline.basis {
        let (first, last) = (b.knots[0], b.knots[b.knots.len() - 1]);
        b.knots = b.knots.iter().rev().map(|k| first + last - k).collect();
        b.multiplicities.reverse();
        b.weights.reverse();
    }
}
