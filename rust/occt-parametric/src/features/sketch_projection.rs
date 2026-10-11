//! Linked analytic and exact rational edge projections. Expansion/storage are O(sketch definition
//! size + projections); source-query cost follows the existing semantic selector.
//! Analytic projection is O(1); rational import is O(returned poles) plus
//! native conversion. Neither path samples curves or retains native handles.
use super::*;
use crate::assembly::{dot, subtract};
use occt_bridge::{AnalyticCurve, BezierSpan};

pub(super) fn materialize(
    session: &Session,
    sketch: &SketchDefinition,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'_>>,
    definitions: &Features<'_>,
    plane: Option<ResolvedDatum>,
) -> Result<SketchDefinition, ModelError> {
    let (origin, x, y) = sketch.resolved_frame(parameters, plane)?;
    let mut result = sketch.projection_seed()?;
    result.validate_structure()?;
    let indexes: HashMap<_, _> = result
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.clone(), i))
        .collect();
    let point = |p| {
        let v = subtract(p, origin);
        [dot(v, x), dot(v, y)]
    };
    let vector = |v| [dot(v, x), dot(v, y)];
    let mut imported_poles = 0usize;
    for projection in &sketch.projections {
        let edges = resolve_edge_selector(
            session,
            shape(shapes, &projection.input)?,
            &projection.edge,
            parameters,
            shapes,
            definitions,
        )?;
        if projection.kind == SketchProjectionKind::Chain
            && (edges.is_empty() || edges.len() > 1000)
        {
            return Err(ModelError::new(format!(
                "sketch chain projection '{}' requires 1–1000 edges (selected {})",
                projection.id,
                edges.len()
            )));
        }
        if projection.kind != SketchProjectionKind::Chain && edges.len() != 1 {
            return Err(ModelError::new(format!(
                "sketch projection '{}' requires exactly one edge (selected {})",
                projection.id,
                edges.len()
            )));
        }
        if matches!(
            projection.kind,
            SketchProjectionKind::Spline | SketchProjectionKind::Chain
        ) {
            let mut junction_points = Vec::new();
            let spans = if projection.kind == SketchProjectionKind::Chain {
                let mut curves = Vec::with_capacity(edges.len());
                let mut count = 0usize;
                for edge in &edges {
                    if count > 9998 {
                        return Err(ModelError::new(
                            "chain projection exceeds 10000 native poles",
                        ));
                    }
                    let spans = session.edge_bezier_spans(edge, 10000 - count)?;
                    count += spans.iter().map(|s| s.poles.len()).sum::<usize>();
                    curves.push(spans);
                }
                let chain = chain_geometry(curves)
                    .map_err(|e| e.context(&format!("sketch projection '{}'", projection.id)))?;
                junction_points = chain.junctions;
                chain.spans
            } else {
                session.edge_bezier_spans(&edges[0], 10000)?
            };
            for junction in &projection.junctions {
                let matched =
                    match_junction(junction, &junction_points, parameters).map_err(|e| {
                        e.context(&format!(
                            "sketch projection '{}' junction '{}'",
                            projection.id, junction.id
                        ))
                    })?;
                let p = point(matched);
                if !p.iter().all(|v| v.is_finite()) {
                    return Err(ModelError::new(
                        "junction projection exceeds finite coordinates",
                    ));
                }
                let id = format!("{}:junction:{}", projection.id, junction.id);
                let index = indexes[&id];
                result.points[index] = SketchPoint {
                    id,
                    x: length_literal(p[0]),
                    y: length_literal(p[1]),
                    fixed: true,
                };
            }
            let (poles, basis) = project_spans(&spans, &point)
                .map_err(|e| e.context(&format!("sketch projection '{}'", projection.id)))?;
            imported_poles += poles.len();
            if imported_poles > 100000 {
                return Err(ModelError::new(
                    "sketch spline projections exceed 100000 total poles",
                ));
            }
            let closed = (poles[0][0] - poles[poles.len() - 1][0])
                .hypot(poles[0][1] - poles[poles.len() - 1][1])
                <= if projection.kind == SketchProjectionKind::Chain {
                    CHAIN_TOLERANCE
                } else {
                    1e-9
                };
            let mut ids = Vec::with_capacity(poles.len());
            for (i, p) in poles.iter().enumerate() {
                let suffix = if i == 0 {
                    "start".into()
                } else if i == poles.len() - 1 {
                    "end".into()
                } else {
                    format!("pole:{i}")
                };
                let id = format!("{}:{suffix}", projection.id);
                let value = SketchPoint {
                    id: id.clone(),
                    x: length_literal(p[0]),
                    y: length_literal(p[1]),
                    fixed: true,
                };
                if i == 0 || i == poles.len() - 1 {
                    result.points[indexes[&id]] = value;
                } else {
                    result.points.push(value);
                }
                ids.push(if closed && i == poles.len() - 1 {
                    format!("{}:start", projection.id)
                } else {
                    id
                });
            }
            let spline = result
                .splines
                .iter_mut()
                .find(|s| s.id == projection.id)
                .expect("declared spline");
            spline.points = ids;
            spline.basis = Some(basis);
            continue;
        }
        let curve=session.edge_analytic_curve(&edges[0])?.ok_or_else(||ModelError::new("sketch projection requires an analytic line or conic; use kind spline for exact rational import"))?;
        let (points, clockwise) = project(curve, projection.kind, &point, &vector)
            .map_err(|e| e.context(&format!("sketch projection '{}'", projection.id)))?;
        for (suffix, p) in points {
            if !p.iter().all(|v| v.is_finite()) {
                return Err(ModelError::new(
                    "projection exceeds finite sketch coordinates",
                ));
            }
            let point = &mut result.points[indexes[&format!("{}:{suffix}", projection.id)]];
            point.x = ScalarExpr::Literal(Quantity::length(p[0], LengthUnit::Millimeter));
            point.y = ScalarExpr::Literal(Quantity::length(p[1], LengthUnit::Millimeter));
        }
        if let Some(clockwise) = clockwise {
            result
                .arcs
                .iter_mut()
                .find(|a| a.id == projection.id)
                .expect("declared arc")
                .clockwise = clockwise;
        }
    }
    result.validate_structure()?;
    Ok(result)
}
const CHAIN_TOLERANCE: f64 = 1e-7;
fn close(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).hypot(a.y - b.y).hypot(a.z - b.z) <= CHAIN_TOLERANCE
}
fn point_order(a: Vec3, b: Vec3) -> std::cmp::Ordering {
    a.x.total_cmp(&b.x)
        .then(a.y.total_cmp(&b.y))
        .then(a.z.total_cmp(&b.z))
}
/// Bounded O(E²) endpoint clustering for E<=1000. Match source-space endpoints
/// before projection, so overlapping projections cannot join unrelated edges.
struct ChainGeometry {
    spans: Vec<BezierSpan>,
    junctions: Vec<Vec3>,
}
#[cfg(test)]
fn chain_spans(curves: Vec<Vec<BezierSpan>>) -> Result<Vec<BezierSpan>, ModelError> {
    Ok(chain_geometry(curves)?.spans)
}
fn chain_geometry(mut curves: Vec<Vec<BezierSpan>>) -> Result<ChainGeometry, ModelError> {
    if curves.is_empty() || curves.len() > 1000 {
        return Err(ModelError::new("chain projection requires 1–1000 edges"));
    }
    let mut nodes: Vec<Vec3> = Vec::new();
    let mut incidence: Vec<Vec<usize>> = Vec::new();
    let mut members: Vec<Vec<Vec3>> = Vec::new();
    let mut ends = Vec::new();
    let degree = curves
        .iter()
        .flatten()
        .map(|s| s.poles.len().saturating_sub(1))
        .max()
        .unwrap_or(0);
    if !(1..=25).contains(&degree)
        || curves.iter().any(|c| c.is_empty())
        || curves
            .iter()
            .flatten()
            .any(|s| s.poles.len() < 2 || s.poles.len() != s.weights.len())
    {
        return Err(ModelError::new(
            "chain projection requires supported finite Bezier edges",
        ));
    }
    if curves.iter().map(Vec::len).sum::<usize>() * (degree + 1) > 10000 {
        return Err(ModelError::new(
            "chain projection degree elevation exceeds 10000 poles",
        ));
    }
    for (index, curve) in curves.iter().enumerate() {
        let mut endpoints = [0; 2];
        for (side, &p) in [
            curve[0].poles.first().unwrap(),
            curve.last().unwrap().poles.last().unwrap(),
        ]
        .into_iter()
        .enumerate()
        {
            let mut matches = Vec::new();
            for (node, cluster) in members.iter().enumerate() {
                let count = cluster.iter().filter(|&&q| close(p, q)).count();
                if count > 0 {
                    if count != cluster.len() {
                        return Err(ModelError::new(
                            "chain projection has ambiguous endpoint matches",
                        ));
                    }
                    matches.push(node);
                }
            }
            let node = match matches.as_slice() {
                [] => {
                    nodes.push(p);
                    members.push(vec![p]);
                    incidence.push(Vec::new());
                    nodes.len() - 1
                }
                [i] => {
                    members[*i].push(p);
                    if point_order(p, nodes[*i]).is_lt() {
                        nodes[*i] = p;
                    }
                    *i
                }
                _ => {
                    return Err(ModelError::new(
                        "chain projection has ambiguous endpoint matches",
                    ));
                }
            };
            endpoints[side] = node;
            incidence[node].push(index);
        }
        ends.push(endpoints);
    }
    if incidence.iter().any(|edges| edges.len() > 2) {
        return Err(ModelError::new("chain projection is branched"));
    }
    let terminals = incidence
        .iter()
        .enumerate()
        .filter_map(|(i, e)| (e.len() == 1).then_some(i))
        .collect::<Vec<_>>();
    if !terminals.is_empty() && terminals.len() != 2 {
        return Err(ModelError::new("chain projection is disconnected"));
    }
    let start = if terminals.is_empty() {
        (0..nodes.len())
            .min_by(|&a, &b| point_order(nodes[a], nodes[b]))
            .unwrap()
    } else {
        *terminals
            .iter()
            .min_by(|&&a, &&b| point_order(nodes[a], nodes[b]))
            .unwrap()
    };
    let mut used = vec![false; curves.len()];
    let mut ordered = Vec::new();
    let mut current = start;
    let mut retained = vec![None; nodes.len()];
    loop {
        let edge = incidence[current]
            .iter()
            .copied()
            .filter(|&e| !used[e])
            .min_by(|&a, &b| {
                let other = |e: usize| {
                    if ends[e][0] == current {
                        ends[e][1]
                    } else {
                        ends[e][0]
                    }
                };
                point_order(nodes[other(a)], nodes[other(b)])
                    .then_with(|| {
                        // Parallel graph edges (for example an arch and its closing
                        // line) need a geometric tie-breaker, not selector order.
                        let key = |e: usize| {
                            let max = curves[e]
                                .iter()
                                .flat_map(|s| &s.weights)
                                .copied()
                                .fold(0., f64::max);
                            let mut key = curves[e]
                                .iter()
                                .flat_map(|s| {
                                    s.poles
                                        .iter()
                                        .copied()
                                        .zip(s.weights.iter().map(|w| w / max))
                                })
                                .collect::<Vec<_>>();
                            if ends[e][1] == current && ends[e][0] != current {
                                key.reverse();
                            }
                            key
                        };
                        let (a, b) = (key(a), key(b));
                        a.iter()
                            .zip(&b)
                            .map(|(&(p, w), &(q, v))| point_order(p, q).then(w.total_cmp(&v)))
                            .find(|o| !o.is_eq())
                            .unwrap_or_else(|| a.len().cmp(&b.len()))
                    })
                    .then(a.cmp(&b))
            });
        let Some(edge) = edge else { break };
        used[edge] = true;
        let reverse = ends[edge][1] == current && ends[edge][0] != current;
        let mut curve = std::mem::take(&mut curves[edge]);
        if reverse {
            curve.reverse();
            for span in &mut curve {
                span.poles.reverse();
                span.weights.reverse();
            }
        }
        if retained[current].is_none() {
            retained[current] = Some(curve[0].poles[0]);
        }
        let endpoint = *curve.last().unwrap().poles.last().unwrap();
        for span in curve {
            ordered.push(elevate_span(span, degree)?);
        }
        current = if reverse {
            ends[edge][0]
        } else {
            ends[edge][1]
        };
        if retained[current].is_none() {
            retained[current] = Some(endpoint);
        }
    }
    if used.iter().any(|used| !*used) {
        return Err(ModelError::new("chain projection is disconnected"));
    }
    let junctions = incidence
        .iter()
        .enumerate()
        .filter(|(_, edges)| edges.len() == 2 && edges[0] != edges[1])
        .map(|(i, _)| retained[i].unwrap())
        .collect();
    Ok(ChainGeometry {
        spans: ordered,
        junctions,
    })
}
fn match_junction(
    reference: &SketchProjectionJunction,
    points: &[Vec3],
    parameters: &HashMap<String, ParameterValue>,
) -> Result<Vec3, ModelError> {
    let near = vector(&reference.near, parameters, Dimension::Length)?;
    let radius = scalar(&reference.maximum_distance, parameters, Dimension::Length)?;
    if radius <= 0. {
        return Err(ModelError::new(
            "junction maximum distance must be positive",
        ));
    }
    let mut matches = points
        .iter()
        .copied()
        .filter(|p| (p.x - near.x).hypot(p.y - near.y).hypot(p.z - near.z) <= radius);
    let first = matches.next().ok_or_else(|| {
        ModelError::new("junction reference found no source junction within maximum distance")
    })?;
    if matches.next().is_some() {
        return Err(ModelError::new(
            "junction reference matches multiple source junctions",
        ));
    }
    Ok(first)
}
/// Exact rational degree elevation using normalized weights and convex pole
/// combinations, avoiding coordinate*large-weight overflow.
fn elevate_span(mut span: BezierSpan, degree: usize) -> Result<BezierSpan, ModelError> {
    let max = span.weights.iter().copied().fold(0., f64::max);
    for w in &mut span.weights {
        *w /= max;
    }
    while span.poles.len() <= degree {
        let n = span.poles.len() - 1;
        let mut poles = Vec::with_capacity(n + 2);
        let mut weights = Vec::with_capacity(n + 2);
        poles.push(span.poles[0]);
        weights.push(span.weights[0]);
        for i in 1..=n {
            let alpha = i as f64 / (n + 1) as f64;
            let a = alpha * span.weights[i - 1];
            let b = (1. - alpha) * span.weights[i];
            let w = a + b;
            let p = Vec3::new(
                (a / w) * span.poles[i - 1].x + (b / w) * span.poles[i].x,
                (a / w) * span.poles[i - 1].y + (b / w) * span.poles[i].y,
                (a / w) * span.poles[i - 1].z + (b / w) * span.poles[i].z,
            );
            poles.push(p);
            weights.push(w);
        }
        poles.push(*span.poles.last().unwrap());
        weights.push(*span.weights.last().unwrap());
        span = BezierSpan { poles, weights };
    }
    if span.weights.iter().any(|w| !w.is_finite() || *w <= 0.)
        || span
            .poles
            .iter()
            .any(|p| ![p.x, p.y, p.z].iter().all(|v| v.is_finite()))
    {
        return Err(ModelError::new(
            "chain degree elevation needs finite poles and positive weights",
        ));
    }
    Ok(span)
}

fn length_literal(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}
/// Equal-length span domains preserve the exact trace and orientation, not the
/// source parameterization. Shared endpoint weights are normalized per span.
fn project_spans(
    spans: &[BezierSpan],
    point: &impl Fn(Vec3) -> [f64; 2],
) -> Result<(Vec<[f64; 2]>, SketchBSplineBasis), ModelError> {
    let degree = spans.first().map_or(0, |s| s.poles.len().saturating_sub(1));
    if !(1..=25).contains(&degree)
        || spans
            .iter()
            .any(|s| s.poles.len() != degree + 1 || s.weights.len() != s.poles.len())
    {
        return Err(ModelError::new(
            "spline projection needs finite supported equal-degree Bezier spans",
        ));
    }
    let mut poles: Vec<[f64; 2]> = Vec::new();
    let mut weights: Vec<f64> = Vec::new();
    for span in spans {
        let scale = weights.last().copied().unwrap_or(span.weights[0]) / span.weights[0];
        for (i, (&p, &w)) in span.poles.iter().zip(&span.weights).enumerate() {
            let p = point(p);
            let w = w * scale;
            if !p.iter().all(|v| v.is_finite()) || !w.is_finite() || w <= 0. {
                return Err(ModelError::new(
                    "spline projection needs finite poles and positive weights",
                ));
            }
            if !poles.is_empty() && i == 0 {
                let last = poles.last().unwrap();
                if (last[0] - p[0]).hypot(last[1] - p[1]) > 1e-7 {
                    return Err(ModelError::new("spline projection spans are disconnected"));
                }
                continue;
            }
            poles.push(p);
            weights.push(w);
        }
    }
    if poles.len() > 10000 {
        return Err(ModelError::new("spline projection exceeds 10000 poles"));
    }
    if poles
        .iter()
        .all(|p| (p[0] - poles[0][0]).hypot(p[1] - poles[0][1]) <= 1e-9)
    {
        return Err(ModelError::new("spline projection collapses to a point"));
    }
    let mut multiplicities = vec![degree as i32; spans.len() + 1];
    multiplicities[0] += 1;
    *multiplicities.last_mut().unwrap() += 1;
    Ok((
        poles,
        SketchBSplineBasis {
            degree: degree as i32,
            knots: (0..=spans.len()).map(|i| i as f64).collect(),
            multiplicities,
            weights: weights
                .into_iter()
                .map(|v| ScalarExpr::Literal(Quantity::scalar(v)))
                .collect(),
            periodic: false,
            non_clamped: false,
        },
    ))
}

type ProjectedPoints = Vec<(&'static str, [f64; 2])>;
fn project(
    curve: AnalyticCurve,
    kind: SketchProjectionKind,
    point: &impl Fn(Vec3) -> [f64; 2],
    vector: &impl Fn(Vec3) -> [f64; 2],
) -> Result<(ProjectedPoints, Option<bool>), ModelError> {
    let mismatch = || ModelError::new("projected edge does not match the declared analytic kind");
    match curve {
        AnalyticCurve::Line { start, end } => {
            if kind != SketchProjectionKind::Line {
                return Err(mismatch());
            }
            let (a, b) = (point(start), point(end));
            if (b[0] - a[0]).hypot(b[1] - a[1]) <= 1e-9 {
                return Err(ModelError::new("projection collapses to a point"));
            }
            Ok((vec![("start", a), ("end", b)], None))
        }
        AnalyticCurve::Conic {
            center,
            major,
            minor,
            first,
            last,
        } => {
            let c = point(center);
            let u = vector(major);
            let v = vector(minor);
            let full = ((last - first).abs() - std::f64::consts::TAU).abs()
                <= 64.0 * f64::EPSILON * std::f64::consts::TAU;
            let (a, b, r1, r2) = ellipse_axes(u, v)?;
            let circular = (r1 - r2).abs() <= 64.0 * f64::EPSILON * r1;
            let add = |v: [f64; 2]| [c[0] + v[0], c[1] + v[1]];
            match kind {
                SketchProjectionKind::Circle if full && circular => Ok((
                    vec![("center", c), ("rim", add([0.5 * (r1 + r2), 0.]))],
                    None,
                )),
                SketchProjectionKind::Ellipse if full => Ok((
                    vec![("center", c), ("major", add(a)), ("minor", add(b))],
                    None,
                )),
                SketchProjectionKind::Arc if !full && circular => {
                    let at = |t: f64| {
                        add([
                            u[0] * t.cos() + v[0] * t.sin(),
                            u[1] * t.cos() + v[1] * t.sin(),
                        ])
                    };
                    let scale = u
                        .iter()
                        .chain(v.iter())
                        .map(|v| v.abs())
                        .fold(0.0, f64::max);
                    let orientation =
                        (u[0] / scale) * (v[1] / scale) - (u[1] / scale) * (v[0] / scale);
                    let clockwise = orientation.signum() * (last - first).signum() < 0.;
                    Ok((
                        vec![("center", c), ("start", at(first)), ("end", at(last))],
                        Some(clockwise),
                    ))
                }
                _ => Err(mismatch()),
            }
        }
    }
}
/// Principal axes of the projected conic's 2×2 matrix. Scale normalization and
/// determinant-based minor radius avoid overflow and subtractive cancellation.
fn ellipse_axes(u: [f64; 2], v: [f64; 2]) -> Result<([f64; 2], [f64; 2], f64, f64), ModelError> {
    let s = u
        .iter()
        .chain(v.iter())
        .map(|v| v.abs())
        .fold(0.0, f64::max);
    if !s.is_finite() || s == 0. {
        return Err(ModelError::new("conic projection is degenerate"));
    }
    let (u, v) = ([u[0] / s, u[1] / s], [v[0] / s, v[1] / s]);
    let a = u[0] * u[0] + v[0] * v[0];
    let b = u[0] * u[1] + v[0] * v[1];
    let d = u[1] * u[1] + v[1] * v[1];
    let discriminant = (a - d).hypot(2. * b);
    let lambda = (a + d + discriminant) * 0.5;
    let r1 = s * lambda.sqrt();
    let r2 = s * (u[0] * v[1] - u[1] * v[0]).abs() / lambda.sqrt();
    if !r1.is_finite() || !r2.is_finite() || r2 <= 1e-9 {
        return Err(ModelError::new("conic projection is degenerate"));
    }
    let theta = if discriminant <= 64.0 * f64::EPSILON * (a + d) {
        0.
    } else {
        0.5 * (2. * b).atan2(a - d)
    };
    Ok((
        [r1 * theta.cos(), r1 * theta.sin()],
        [-r2 * theta.sin(), r2 * theta.cos()],
        r1,
        r2,
    ))
}

impl PartInstance<'_> {
    /// Runtime snapshots for sketches with linked geometry or face supports.
    /// Resolves parameters, datums and feature indexes once. Cost is linear in
    /// those definitions plus total selector/query and sketch-copy work.
    pub fn resolved_sketches(
        &self,
        session: &Session,
        generated: &GeneratedResult<'_>,
    ) -> Result<ResolvedSketches, ModelError> {
        let parameters = self.resolved_parameters()?;
        let definitions = Features::new(self.definition);
        let datums = self
            .definition
            .datums
            .iter()
            .map(|d| (d.id.as_str(), d))
            .collect();
        let mut result = HashMap::new();
        for feature in &self.definition.features {
            let sketch = match &feature.operation {
                FeatureOperation::SketchFace { sketch }
                | FeatureOperation::SketchWire { sketch }
                | FeatureOperation::SketchOpenWire { sketch }
                    if sketch.face_support.is_some() || !sketch.projections.is_empty() =>
                {
                    sketch
                }
                _ => continue,
            };
            let resolve = || {
                let plane = if sketch.face_support.is_some() {
                    super::sketch_support::resolve(
                        session,
                        sketch,
                        &parameters,
                        &generated.shapes,
                        &definitions,
                    )?
                } else {
                    sketch_datum(&datums, &feature.operation)?
                        .map(|d| d.kind.evaluate(&parameters))
                        .transpose()?
                };
                let resolved = if sketch.projections.is_empty() {
                    sketch.as_ref().clone()
                } else {
                    materialize(
                        session,
                        sketch,
                        &parameters,
                        &generated.shapes,
                        &definitions,
                        plane,
                    )?
                };
                Ok(ResolvedSketch {
                    sketch: resolved,
                    plane,
                })
            };
            result.insert(feature.id.clone(), resolve());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line_span(a: Vec3, b: Vec3) -> Vec<BezierSpan> {
        vec![BezierSpan {
            poles: vec![a, b],
            weights: vec![1., 1.],
        }]
    }
    #[test]
    fn junction_matches_are_independent_of_chain_order_and_exclude_terminals() {
        let (a, b, c) = (
            Vec3::new(0., 0., 0.),
            Vec3::new(2., 0., 0.),
            Vec3::new(3., 1., 0.),
        );
        let mut reference = SketchProjectionJunction {
            id: "middle".into(),
            near: VectorExpr::Literal(VectorQuantity::lengths(2., 0., 0., LengthUnit::Millimeter)),
            maximum_distance: length_literal(1e-6),
        };
        for curves in [
            vec![line_span(a, b), line_span(b, c)],
            vec![line_span(c, b), line_span(b, a)],
        ] {
            let chain = chain_geometry(curves).unwrap();
            assert_eq!(chain.junctions, vec![b]);
            assert_eq!(
                match_junction(&reference, &chain.junctions, &HashMap::new()).unwrap(),
                b
            );
            reference.near =
                VectorExpr::Literal(VectorQuantity::lengths(0., 0., 0., LengthUnit::Millimeter));
            assert!(match_junction(&reference, &chain.junctions, &HashMap::new()).is_err());
            reference.near =
                VectorExpr::Literal(VectorQuantity::lengths(2., 0., 0., LengthUnit::Millimeter));
        }
        let loop_edge = vec![BezierSpan {
            poles: vec![a, b, a],
            weights: vec![1., 1., 1.],
        }];
        assert!(
            chain_geometry(vec![loop_edge])
                .unwrap()
                .junctions
                .is_empty()
        );
    }
    #[test]
    fn chain_endpoint_tolerance_rejects_nontransitive_matches_in_every_order() {
        let curves = [
            line_span(Vec3::new(0., 0., 0.), Vec3::new(10., 0., 0.)),
            line_span(Vec3::new(1.5e-7, 0., 0.), Vec3::new(20., 0., 0.)),
            line_span(Vec3::new(0.75e-7, 0., 0.), Vec3::new(30., 0., 0.)),
        ];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            assert!(
                chain_spans(order.into_iter().map(|i| curves[i].clone()).collect())
                    .unwrap_err()
                    .message
                    .contains("ambiguous endpoint")
            );
        }
    }
    #[test]
    fn chain_order_is_independent_of_selection_order_and_supports_mixed_degree_loops() {
        let (a, b, c) = (
            Vec3::new(0., 0., 0.),
            Vec3::new(2., 0., 0.),
            Vec3::new(3., 1., 0.),
        );
        let chain = chain_spans(vec![line_span(c, b), line_span(a, b)]).unwrap();
        assert_eq!(chain[0].poles, vec![a, b]);
        assert_eq!(chain[1].poles, vec![b, c]);
        let arc = vec![BezierSpan {
            poles: vec![a, Vec3::new(1., 2., 0.), b],
            weights: vec![1., 0.5, 1.],
        }];
        let first = chain_spans(vec![arc.clone(), line_span(b, a)]).unwrap();
        let second = chain_spans(vec![line_span(a, b), arc.clone()]).unwrap();
        assert_eq!(first, second);
        assert!(first.iter().all(|s| s.poles.len() == 3));
        let xy = |p: Vec3| [p.x, p.y];
        assert!(project_spans(&first, &xy).is_ok());
        let elevated = elevate_span(
            BezierSpan {
                poles: vec![a, b],
                weights: vec![1., 1.],
            },
            3,
        )
        .unwrap();
        assert_eq!(elevated.poles.len(), 4);
        assert!((elevated.poles[1].x - 2. / 3.).abs() < 1e-12);
        assert!(
            chain_spans(vec![
                line_span(a, b),
                line_span(b, c),
                line_span(b, Vec3::new(2., 2., 0.))
            ])
            .unwrap_err()
            .message
            .contains("branched")
        );
        assert!(
            chain_spans(vec![
                line_span(a, b),
                line_span(Vec3::new(10., 0., 0.), Vec3::new(11., 0., 0.))
            ])
            .unwrap_err()
            .message
            .contains("disconnected")
        );
    }
    #[test]
    fn projected_bezier_spans_normalize_join_weights_and_reject_invalid_geometry() {
        let spans = vec![
            BezierSpan {
                poles: vec![
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(2., 0., 0.),
                ],
                weights: vec![1., 0.5, 2.],
            },
            BezierSpan {
                poles: vec![
                    Vec3::new(2., 0., 0.),
                    Vec3::new(3., -1., 0.),
                    Vec3::new(4., 0., 0.),
                ],
                weights: vec![4., 1., 2.],
            },
        ];
        let xy = |p: Vec3| [p.x, p.y];
        let (points, basis) = project_spans(&spans, &xy).unwrap();
        assert_eq!(points.len(), 5);
        assert_eq!(basis.multiplicities, vec![3, 2, 3]);
        assert_eq!(
            basis.weights,
            vec![1., 0.5, 2., 0.5, 1.]
                .into_iter()
                .map(|v| ScalarExpr::Literal(Quantity::scalar(v)))
                .collect::<Vec<_>>()
        );
        let mut bad = spans.clone();
        bad[1].poles[0].x += 1.;
        assert!(project_spans(&bad, &xy).is_err());
        bad = spans.clone();
        bad[0].weights[0] = 0.;
        assert!(project_spans(&bad, &xy).is_err());
        bad = spans.clone();
        bad[0].poles[1].x = f64::INFINITY;
        assert!(project_spans(&bad, &xy).is_err());
        assert!(project_spans(&[], &xy).is_err());
    }
    #[test]
    fn almost_closed_arc_is_not_imported_as_a_full_circle() {
        let curve = |last| AnalyticCurve::Conic {
            center: Vec3::new(0., 0., 0.),
            major: Vec3::new(1e6, 0., 0.),
            minor: Vec3::new(0., 1e6, 0.),
            first: 0.,
            last,
        };
        let xy = |p: Vec3| [p.x, p.y];
        let almost = curve(std::f64::consts::TAU - 1e-10);
        assert!(project(almost, SketchProjectionKind::Circle, &xy, &xy).is_err());
        let (points, clockwise) = project(almost, SketchProjectionKind::Arc, &xy, &xy).unwrap();
        assert!((points[1].1[0] - points[2].1[0]).hypot(points[1].1[1] - points[2].1[1]) > 1e-5);
        assert_eq!(clockwise, Some(false));
        assert!(
            project(
                curve(std::f64::consts::TAU),
                SketchProjectionKind::Circle,
                &xy,
                &xy
            )
            .is_ok()
        );
    }
}
