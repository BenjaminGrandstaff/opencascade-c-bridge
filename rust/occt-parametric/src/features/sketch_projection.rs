//! Linked analytic edge projections. Expansion/storage are O(sketch definition
//! size + projections); source-query cost follows the existing semantic selector.
//! Each analytic projection is O(1), with no sampling or retained native handles.
use super::*;
use crate::assembly::{dot, subtract};
use occt_bridge::AnalyticCurve;

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
    for projection in &sketch.projections {
        let edges = resolve_edge_selector(
            session,
            shape(shapes, &projection.input)?,
            &projection.edge,
            parameters,
            shapes,
            definitions,
        )?;
        if edges.len() != 1 {
            return Err(ModelError::new(format!(
                "sketch projection '{}' requires exactly one edge (selected {})",
                projection.id,
                edges.len()
            )));
        }
        let curve=session.edge_analytic_curve(&edges[0])?.ok_or_else(||ModelError::new("sketch projection requires an analytic line or conic; spline projection is not supported"))?;
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
    Ok(result)
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
