//! Exact drawing geometry and kernel-based detail trimming.
use super::*;
use occt_types::AnalyticCurve;
use std::f64::consts::TAU;

#[derive(Clone, Debug, PartialEq)]
pub enum DrawingCurveGeometry {
    Line {
        start_mm: [f64; 2],
        end_mm: [f64; 2],
    },
    /// Exact rational Bezier span (degree = poles.len()-1). Empty SVG points
    /// mean a polynomial line/quadratic/cubic can be emitted exactly.
    Bezier {
        poles_mm: Vec<[f64; 2]>,
        weights: Vec<f64>,
        svg_points_mm: Vec<[f64; 2]>,
    },
    /// Counterclockwise paper-space arc, measured from the major axis.
    /// A full ellipse has end_parameter - start_parameter == 2*pi.
    Ellipse {
        center_mm: [f64; 2],
        major_axis_mm: [f64; 2],
        minor_radius_mm: f64,
        start_parameter: f64,
        end_parameter: f64,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingCurve {
    pub geometry: DrawingCurveGeometry,
    pub hidden: bool,
}

pub fn local(view: &DrawingView, point: Vec3) -> Result<[f64; 2], ModelError> {
    if matches!(view.kind, DrawingViewKind::Slice) {
        view.project(point)
    } else {
        Ok([point.x, point.y])
    }
}
fn vector(view: &DrawingView, value: Vec3) -> Result<[f64; 2], ModelError> {
    let projected = if matches!(view.kind, DrawingViewKind::Slice) {
        let frame = view.frame()?;
        [
            dot(value, frame.x_axis),
            dot(value, cross(frame.direction, frame.x_axis)),
        ]
    } else {
        [value.x, value.y]
    };
    let result = projected.map(|v| v * view.scale);
    if !finite_pair(result) {
        return Err(ModelError::new(
            "analytic drawing vector exceeds finite limits",
        ));
    }
    Ok(result)
}
pub fn convert(
    view: &DrawingView,
    curve: AnalyticCurve,
) -> Result<DrawingCurveGeometry, ModelError> {
    match curve {
        AnalyticCurve::Line { start, end } => Ok(DrawingCurveGeometry::Line {
            start_mm: view.paper(local(view, start)?)?,
            end_mm: view.paper(local(view, end)?)?,
        }),
        AnalyticCurve::Conic {
            center,
            major,
            minor,
            first,
            last,
        } => {
            let major_axis_mm = vector(view, major)?;
            let minor_axis = vector(view, minor)?;
            let a = major_axis_mm[0].hypot(major_axis_mm[1]);
            let b = minor_axis[0].hypot(minor_axis[1]);
            let x = major_axis_mm.map(|v| v / a);
            let y = minor_axis.map(|v| v / b);
            if !a.is_finite()
                || !b.is_finite()
                || b <= 0.0
                || a <= 0.0
                || a < b * (1.0 - 1e-10)
                || (x[0] * y[0] + x[1] * y[1]).abs() > 1e-10
            {
                return Err(ModelError::new(
                    "analytic drawing conic needs finite orthogonal nonzero axes",
                ));
            }
            let sign = if x[0] * y[1] - x[1] * y[0] < 0.0 {
                -1.0
            } else {
                1.0
            };
            let (first, last) = (first * sign, last * sign);
            let span = (last - first).abs();
            if span <= 0.0 || span > TAU + 64.0 * f64::EPSILON * TAU {
                return Err(ModelError::new(
                    "analytic drawing conic span must be positive and at most one turn",
                ));
            }
            let full = (span - TAU).abs() <= 64.0 * f64::EPSILON * TAU;
            let start_parameter = if full {
                0.0
            } else {
                first.min(last).rem_euclid(TAU)
            };
            let center_mm = view.paper(local(view, center)?)?;
            for i in 0..2 {
                let extent = major_axis_mm[i].hypot(minor_axis[i]);
                if !(center_mm[i].abs() + extent).is_finite() {
                    return Err(ModelError::new(
                        "analytic drawing conic extent exceeds finite limits",
                    ));
                }
            }
            Ok(DrawingCurveGeometry::Ellipse {
                center_mm,
                major_axis_mm,
                minor_radius_mm: b.min(a),
                start_parameter,
                end_parameter: start_parameter + if full { TAU } else { span },
            })
        }
    }
}
pub fn charge(
    vertices: &mut usize,
    count: usize,
    options: DrawingRenderOptions,
) -> Result<(), ModelError> {
    *vertices = vertices
        .checked_add(count)
        .ok_or_else(|| ModelError::new("drawing vertex count overflow"))?;
    if *vertices > options.maximum_vertices {
        return Err(ModelError::new("drawing exceeds export vertex budget"));
    }
    Ok(())
}
