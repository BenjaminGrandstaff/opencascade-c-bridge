//! Bounded analytic crossings in the sketch plane. No sampling or native handles.
use super::*;
use std::f64::consts::TAU;
type Point = [f64; 2];
const ROUND: f64 = 64. * f64::EPSILON;

#[derive(Clone, Copy)]
enum Curve {
    Line(Point, Point),
    Circle(Point, f64, Option<(Point, Point, bool)>),
    Ellipse(Point, Point, f64, f64),
}
fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn length(a: Point) -> f64 {
    a[0].hypot(a[1])
}
fn at(a: Point, d: Point, t: f64) -> Point {
    [a[0] + d[0] * t, a[1] + d[1] * t]
}
fn direction(a: Point) -> Result<(Point, f64), ModelError> {
    let n = length(a);
    if !n.is_finite() || n <= 1e-9 {
        return Err(ModelError::new(
            "intersection requires finite nondegenerate curves",
        ));
    }
    Ok(([a[0] / n, a[1] / n], n))
}
fn contains(curve: Curve, p: Point) -> bool {
    match curve {
        Curve::Line(a, b) => {
            let Ok((u, n)) = direction(sub(b, a)) else {
                return false;
            };
            let t = dot(sub(p, a), u);
            t >= -ROUND * n && t <= n * (1. + ROUND)
        }
        Curve::Circle(c, r, Some((a, b, clockwise))) => {
            if length(sub(p, a)) <= ROUND * r || length(sub(p, b)) <= ROUND * r {
                return true;
            }
            let sign = if clockwise { -1. } else { 1. };
            let angle = |q: Point| (q[1] - c[1]).atan2(q[0] - c[0]);
            let span = (sign * (angle(b) - angle(a))).rem_euclid(TAU);
            let t = (sign * (angle(p) - angle(a))).rem_euclid(TAU);
            t <= span + ROUND
        }
        _ => true,
    }
}
fn line_ellipse(
    a: Point,
    b: Point,
    c: Point,
    u: Point,
    ra: f64,
    rb: f64,
) -> Result<Vec<Point>, ModelError> {
    direction(sub(b, a))?;
    let v = [-u[1], u[0]];
    let scaled = |q| {
        let d = sub(q, c);
        [dot(d, u) / ra, dot(d, v) / rb]
    };
    let p = scaled(a);
    let d = sub(scaled(b), p);
    let (unit, n) = direction(d)?;
    // Unit-circle intersection in axis-scaled coordinates, without quadratic cancellation.
    let t = -dot(p, unit);
    let h = cross(p, unit);
    let disc = (1. - h.abs()) * (1. + h.abs());
    if disc < -ROUND {
        return Ok(Vec::new());
    }
    let offset = disc.max(0.).sqrt();
    let mut result = vec![at(a, sub(b, a), (t - offset) / n)];
    if offset > 0. {
        result.push(at(a, sub(b, a), (t + offset) / n));
    }
    Ok(result)
}
fn candidates(first: Curve, second: Curve) -> Result<Vec<Point>, ModelError> {
    let mut result = match (first, second) {
        (Curve::Line(a, b), Curve::Line(c, d)) => {
            let (u, n) = direction(sub(b, a))?;
            let (v, m) = direction(sub(d, c))?;
            let delta = sub(c, a);
            let det = cross(u, v);
            if det.abs() <= ROUND {
                if cross(delta, u).abs() <= ROUND * n.max(m).max(length(delta)) {
                    let t = dot(delta, u);
                    let end = t + dot(sub(d, c), u);
                    let lo = 0f64.max(t.min(end));
                    let hi = n.min(t.max(end));
                    if hi > lo {
                        return Err(ModelError::new(
                            "intersection curves overlap; no unique isolated crossing",
                        ));
                    }
                    if hi == lo {
                        vec![at(a, u, lo)]
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            } else {
                vec![at(a, u, cross(delta, v) / det)]
            }
        }
        (Curve::Line(a, b), Curve::Circle(c, r, _))
        | (Curve::Circle(c, r, _), Curve::Line(a, b)) => line_ellipse(a, b, c, [1., 0.], r, r)?,
        (Curve::Line(a, b), Curve::Ellipse(c, u, ra, rb))
        | (Curve::Ellipse(c, u, ra, rb), Curve::Line(a, b)) => line_ellipse(a, b, c, u, ra, rb)?,
        (Curve::Circle(a, ra, _), Curve::Circle(b, rb, _)) => {
            let delta = sub(b, a);
            let distance = length(delta);
            if distance == 0. {
                if ra == rb {
                    return Err(ModelError::new(
                        "intersection curves have coincident circular supports",
                    ));
                }
                Vec::new()
            } else {
                let u = [delta[0] / distance, delta[1] / distance];
                let scale = distance.max(ra).max(rb);
                let (d, r, s) = (distance / scale, ra / scale, rb / scale);
                if d > r + s || d < (r - s).abs() {
                    Vec::new()
                } else {
                    let along = (d * d + (r - s) * (r + s)) / (2. * d);
                    let disc = (r - along.abs()) * (r + along.abs());
                    if disc < -ROUND {
                        Vec::new()
                    } else {
                        let mid = at(a, u, along * scale);
                        let h = disc.max(0.).sqrt() * scale;
                        let v = [-u[1], u[0]];
                        let mut points = vec![at(mid, v, h)];
                        if h > 0. {
                            points.push(at(mid, v, -h));
                        }
                        points
                    }
                }
            }
        }
        _ => return Err(ModelError::new("unsupported projected intersection pair")),
    };
    if result.iter().flatten().any(|v| !v.is_finite()) {
        return Err(ModelError::new("intersection exceeds finite coordinates"));
    }
    result.retain(|&p| contains(first, p) && contains(second, p));
    Ok(result)
}
fn curve(
    sketch: &SketchDefinition,
    id: &str,
    points: &HashMap<&str, Point>,
) -> Result<Curve, ModelError> {
    let p = |id: &str| points[id];
    if let Some(l) = sketch.lines.iter().find(|l| l.id == id) {
        direction(sub(p(&l.end), p(&l.start)))?;
        return Ok(Curve::Line(p(&l.start), p(&l.end)));
    }
    if let Some(c) = sketch.circles.iter().find(|c| c.id == id) {
        let (_, r) = direction(sub(p(&c.rim), p(&c.center)))?;
        return Ok(Curve::Circle(p(&c.center), r, None));
    }
    if let Some(c) = sketch.arcs.iter().find(|c| c.id == id) {
        let (_, r) = direction(sub(p(&c.start), p(&c.center)))?;
        return Ok(Curve::Circle(
            p(&c.center),
            r,
            Some((p(&c.start), p(&c.end), c.clockwise)),
        ));
    }
    if let Some(e) = sketch.ellipses.iter().find(|e| e.id == id) {
        let (u, ra) = direction(sub(p(&e.major), p(&e.center)))?;
        let (_, rb) = direction(sub(p(&e.minor), p(&e.center)))?;
        return Ok(Curve::Ellipse(p(&e.center), u, ra, rb));
    }
    Err(ModelError::new(
        "intersection curve is not a materialized analytic projection",
    ))
}
pub(super) fn materialize(
    sketch: &mut SketchDefinition,
    projections: &[SketchProjection],
    parameters: &HashMap<String, ParameterValue>,
) -> Result<(), ModelError> {
    if projections.iter().all(|p| p.intersections.is_empty()) {
        return Ok(());
    }
    let points = sketch
        .points
        .iter()
        .map(|p| {
            Ok((
                p.id.as_str(),
                [
                    scalar(&p.x, parameters, Dimension::Length)?,
                    scalar(&p.y, parameters, Dimension::Length)?,
                ],
            ))
        })
        .collect::<Result<HashMap<_, _>, ModelError>>()?;
    let mut curves = HashMap::new();
    let mut pairs = HashMap::new();
    let mut updates = HashMap::new();
    for projection in projections {
        for reference in &projection.intersections {
            let mut run = || -> Result<Point, ModelError> {
                for id in [projection.id.as_str(), reference.other.as_str()] {
                    if !curves.contains_key(id) {
                        curves.insert(id, curve(sketch, id, &points)?);
                    }
                }
                let mut pair = [projection.id.as_str(), reference.other.as_str()];
                pair.sort_unstable();
                if let std::collections::hash_map::Entry::Vacant(entry) = pairs.entry(pair) {
                    entry.insert(candidates(curves[pair[0]], curves[pair[1]])?);
                }
                let near = vector(&reference.near, parameters, Dimension::Length)?;
                let radius = scalar(&reference.maximum_distance, parameters, Dimension::Length)?;
                if near.z != 0. {
                    return Err(ModelError::new(
                        "intersection hint must have sketch-local z equal to zero",
                    ));
                }
                if radius <= 0. {
                    return Err(ModelError::new(
                        "intersection maximum distance must be positive",
                    ));
                }
                let mut matched = pairs[&pair]
                    .iter()
                    .copied()
                    .filter(|p| length(sub(*p, [near.x, near.y])) <= radius);
                let found = matched.next().ok_or_else(|| {
                    ModelError::new(
                        "intersection reference found no crossing within maximum distance",
                    )
                })?;
                if matched.next().is_some() {
                    return Err(ModelError::new(
                        "intersection reference matches multiple crossings",
                    ));
                }
                Ok(found)
            };
            let p = run().map_err(|e| {
                e.context(&format!(
                    "sketch projection '{}' intersection '{}'",
                    projection.id, reference.id
                ))
            })?;
            updates.insert(
                format!("{}:intersection:{}", projection.id, reference.id),
                p,
            );
        }
    }
    for point in &mut sketch.points {
        if let Some(p) = updates.get(&point.id) {
            point.x = length_literal(p[0]);
            point.y = length_literal(p[1]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_line_crossings_overlap_and_parallel_cases() {
        let a = Curve::Line([0., 0.], [2., 0.]);
        assert_eq!(
            candidates(a, Curve::Line([1., -1.], [1., 1.])).unwrap(),
            vec![[1., 0.]]
        );
        assert!(
            candidates(a, Curve::Line([3., -1.], [3., 1.]))
                .unwrap()
                .is_empty()
        );
        assert!(
            candidates(a, Curve::Line([0., 1.], [2., 1.]))
                .unwrap()
                .is_empty()
        );
        assert!(candidates(a, Curve::Line([1., 0.], [3., 0.])).is_err());
        assert_eq!(
            candidates(a, Curve::Line([2., 0.], [3., 0.])).unwrap(),
            vec![[2., 0.]]
        );
    }
    #[test]
    fn circle_crossings_tangencies_and_directed_arcs() {
        let circle = Curve::Circle([0., 0.], 1., None);
        let line = Curve::Line([-2., 0.], [2., 0.]);
        assert_eq!(candidates(line, circle).unwrap(), vec![[-1., 0.], [1., 0.]]);
        assert_eq!(
            candidates(Curve::Line([-2., 1.], [2., 1.]), circle).unwrap(),
            vec![[0., 1.]]
        );
        assert_eq!(
            candidates(circle, Curve::Circle([2., 0.], 1., None)).unwrap(),
            vec![[1., 0.]]
        );
        assert_eq!(
            candidates(circle, Curve::Circle([1., 0.], 2., None)).unwrap(),
            vec![[-1., 0.]]
        );
        assert!(candidates(circle, circle).is_err());
        let arc = Curve::Circle([0., 0.], 1., Some(([1., 0.], [0., 1.], false)));
        assert_eq!(candidates(line, arc).unwrap(), vec![[1., 0.]]);
        let vertical = Curve::Line([0.5, -2.], [0.5, 2.]);
        let upper = candidates(vertical, arc).unwrap();
        assert_eq!(upper.len(), 1);
        assert!(upper[0][1] > 0.);
        let reverse = Curve::Circle([0., 0.], 1., Some(([0., 1.], [1., 0.], true)));
        assert_eq!(candidates(vertical, reverse).unwrap(), upper);
        assert!(
            candidates(circle, Curve::Circle([4., 0.], 1., None))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn rotated_ellipse_crossings_are_not_sampled() {
        let u = [0.6, 0.8];
        let e = Curve::Ellipse([10., 20.], u, 4., 2.);
        let l = Curve::Line(at([10., 20.], u, -10.), at([10., 20.], u, 10.));
        let p = candidates(e, l).unwrap();
        assert_eq!(p.len(), 2);
        for (actual, t) in p.iter().zip([-4., 4.]) {
            assert!(length(sub(*actual, at([10., 20.], u, t))) < 1e-12);
        }
    }
}
