//! Exact drawing geometry and kernel-based detail trimming.
use super::*;
use occt_bridge::AnalyticCurve;
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

fn local(view: &DrawingView, point: Vec3) -> Result<[f64; 2], ModelError> {
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
fn convert(view: &DrawingView, curve: AnalyticCurve) -> Result<DrawingCurveGeometry, ModelError> {
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
fn charge(
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

/// O(E + P + V) returned storage for analytic edges, Bezier poles and SVG
/// approximation vertices, plus kernel conversion/trimming work. Subdivision
/// costs O(d²) per node and is work-bounded. Preflight bounds edge handles.
pub(super) fn append(
    session: &Session,
    view: &DrawingView,
    shape: &Shape<'_>,
    hidden: bool,
    options: DrawingRenderOptions,
    vertices: &mut usize,
    drawing: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    if session.subshape_count(shape, ShapeType::Edge)? == 0 {
        return Ok(());
    }
    let clipped = view
        .detail
        .map(|detail| clip_shape(session, view, shape, detail))
        .transpose()?;
    let shape = clipped.as_ref().unwrap_or(shape);
    let mut work = 0;
    let count = session.subshape_count(shape, ShapeType::Edge)?;
    if count > options.maximum_vertices.saturating_sub(*vertices) / 2 {
        return Err(ModelError::new("drawing exceeds export vertex budget"));
    }
    for edge in session.subshapes(shape, ShapeType::Edge)? {
        if let Some(curve) = session.edge_analytic_curve(&edge)? {
            let geometry = convert(view, curve)?;
            charge(
                vertices,
                if matches!(geometry, DrawingCurveGeometry::Line { .. }) {
                    2
                } else {
                    4
                },
                options,
            )?;
            drawing.curves.push(DrawingCurve { geometry, hidden });
        } else {
            let remaining = options
                .maximum_vertices
                .saturating_sub(*vertices)
                .min(1_000_000);
            let spans = session.edge_bezier_spans(&edge, remaining)?;
            if spans.is_empty() {
                return Err(ModelError::new(
                    "exact drawing export does not support this edge curve type",
                ));
            }
            for span in spans {
                let poles_mm: Vec<_> = span
                    .poles
                    .into_iter()
                    .map(|p| view.paper(local(view, p)?))
                    .collect::<Result<_, _>>()?;
                charge(vertices, poles_mm.len(), options)?;
                let svg_points_mm = bezier::svg_points(
                    &poles_mm,
                    &span.weights,
                    options.curve_tolerance_mm,
                    options.maximum_vertices.saturating_sub(*vertices),
                    &mut work,
                )?;
                charge(vertices, svg_points_mm.len(), options)?;
                drawing.curves.push(DrawingCurve {
                    geometry: DrawingCurveGeometry::Bezier {
                        poles_mm,
                        weights: span.weights,
                        svg_points_mm,
                    },
                    hidden,
                });
            }
        }
    }
    Ok(())
}

/// Kernel Common trims exact curves against a finite rectangular face. Boolean
/// costs depend on edge/face topology; one operation per view, no sampled search.
fn clip_shape<'a>(
    session: &'a Session,
    view: &DrawingView,
    shape: &Shape<'_>,
    detail: DrawingDetail,
) -> Result<Shape<'a>, ModelError> {
    let corners = [
        detail.minimum_mm,
        [detail.maximum_mm[0], detail.minimum_mm[1]],
        detail.maximum_mm,
        [detail.minimum_mm[0], detail.maximum_mm[1]],
    ];
    let points = if matches!(view.kind, DrawingViewKind::Slice) {
        let frame = view.frame()?;
        let up = cross(frame.direction, frame.x_axis);
        corners.map(|[x, y]| {
            Vec3::new(
                frame.origin.x + frame.x_axis.x * x + up.x * y,
                frame.origin.y + frame.x_axis.y * x + up.y * y,
                frame.origin.z + frame.x_axis.z * x + up.z * y,
            )
        })
    } else {
        corners.map(|[x, y]| Vec3::new(x, y, 0.0))
    };
    let wire = session.create_polyline_wire(&points, true)?;
    let face = session.create_face_from_wire(&wire)?;
    // Clip only existing edges: clipping filled section faces would introduce
    // the rectangle perimeter as false model geometry.
    let edges = session.subshapes(shape, ShapeType::Edge)?;
    let boundary = session.create_compound(&edges.iter().collect::<Vec<_>>())?;
    Ok(session.common(&boundary, &face)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_exact_arcs_and_splines_keep_handedness_and_budgets() {
        let view = DrawingView {
            id: "mixed".into(),
            outputs: vec![],
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [0.0, 0.0],
            scale: 1.0,
            show_hidden: false,
            kind: DrawingViewKind::Slice,
            detail: None,
            material_hatching: Default::default(),
            hatching: None,
        };
        // A left-handed basis and descending parameters select the same upper
        // semicircle as a right-handed basis with ascending parameters.
        let geometry = convert(
            &view,
            AnalyticCurve::Conic {
                center: Vec3::new(0.0, 0.0, 0.0),
                major: Vec3::new(5.0, 0.0, 0.0),
                minor: Vec3::new(0.0, -5.0, 0.0),
                first: 0.0,
                last: -std::f64::consts::PI,
            },
        )
        .unwrap();
        let DrawingCurveGeometry::Ellipse {
            start_parameter,
            end_parameter,
            ..
        } = geometry
        else {
            panic!()
        };
        assert_eq!(start_parameter, 0.0);
        assert_eq!(end_parameter, std::f64::consts::PI);
        let session = Session::new().unwrap();
        let wire = session
            .create_curve_wire(
                &[
                    occt_bridge::CurveSegment::Arc {
                        start: Vec3::new(5.0, 0.0, 0.0),
                        middle: Vec3::new(0.0, 5.0, 0.0),
                        end: Vec3::new(-5.0, 0.0, 0.0),
                    },
                    occt_bridge::CurveSegment::Spline {
                        points: vec![
                            Vec3::new(-5.0, 0.0, 0.0),
                            Vec3::new(-4.0, -1.0, 0.0),
                            Vec3::new(-3.0, -4.0, 0.0),
                        ],
                        start_tangent: None,
                        end_tangent: None,
                        periodic: false,
                    },
                ],
                false,
            )
            .unwrap();
        let mut drawing = GeneratedDrawing {
            curves: vec![],
            gdt_lines: vec![],
            gdt_labels: vec![],
            sheet_lines: vec![],
            sheet_labels: vec![],
            id: "mixed".into(),
            title: "Mixed".into(),
            paper_size_mm: [100.0, 100.0],
            polylines: vec![],
            guides: vec![],
            hatches: vec![],
            labels: vec![],
            metadata: Default::default(),
            generated_variants: 0,
        };
        let options = DrawingRenderOptions {
            curve_tolerance_mm: 0.01,
            exact_curves: true,
            curve_samples: 17,
            maximum_vertices: 7,
        };
        let mut vertices = 0;
        append(
            &session,
            &view,
            &wire,
            true,
            options,
            &mut vertices,
            &mut drawing,
        )
        .unwrap();
        assert_eq!(vertices, 7);
        assert_eq!(drawing.curves.len(), 2);
        assert!(drawing.polylines.is_empty());
        assert!(drawing.to_dxf().contains("0\nSPLINE\n"));
        assert!(drawing.to_svg().contains(" Q "));
        assert!(drawing.to_dxf().contains("0\nARC\n"));
        assert!(drawing.to_dxf().contains("50\n0\n51\n180\n"));
        assert!(drawing.to_svg().contains("stroke-dasharray=\"2 1\""));
        assert!(
            append(
                &session,
                &view,
                &wire,
                false,
                DrawingRenderOptions {
                    maximum_vertices: 6,
                    ..options
                },
                &mut 0,
                &mut drawing
            )
            .is_err()
        );
        drawing.curves[0].geometry = DrawingCurveGeometry::Ellipse {
            center_mm: [0.0, 0.0],
            major_axis_mm: [5.0, 0.0],
            minor_radius_mm: 2.0,
            start_parameter: 1.5 * std::f64::consts::PI,
            end_parameter: 2.5 * std::f64::consts::PI,
        };
        let dxf = drawing.to_dxf();
        assert!(dxf.contains(&format!(
            "41\n{}\n42\n{}\n",
            1.5 * std::f64::consts::PI,
            0.5 * std::f64::consts::PI
        )));
        let overflow = AnalyticCurve::Conic {
            center: Vec3::new(f64::MAX, 0.0, 0.0),
            major: Vec3::new(f64::MAX, 0.0, 0.0),
            minor: Vec3::new(0.0, f64::MAX, 0.0),
            first: 0.0,
            last: TAU,
        };
        assert!(convert(&view, overflow).is_err());
        let mut detail = view.clone();
        detail.detail = Some(DrawingDetail {
            minimum_mm: [-2.0, 4.5],
            maximum_mm: [2.0, 6.0],
        });
        drawing.curves.clear();
        append(
            &session,
            &detail,
            &wire,
            false,
            DrawingRenderOptions {
                maximum_vertices: 1000,
                ..options
            },
            &mut 0,
            &mut drawing,
        )
        .unwrap();
        assert_eq!(drawing.curves.len(), 1);
        let DrawingCurveGeometry::Ellipse {
            center_mm,
            major_axis_mm,
            minor_radius_mm,
            start_parameter,
            end_parameter,
        } = drawing.curves[0].geometry
        else {
            panic!("detail introduced a crop edge")
        };
        let radius = major_axis_mm[0].hypot(major_axis_mm[1]);
        for t in [start_parameter, end_parameter] {
            let p = [
                center_mm[0] + major_axis_mm[0] * t.cos()
                    - major_axis_mm[1] / radius * minor_radius_mm * t.sin(),
                center_mm[1]
                    + major_axis_mm[1] * t.cos()
                    + major_axis_mm[0] / radius * minor_radius_mm * t.sin(),
            ];
            assert!(
                (p[0].abs() < 1e-6 || (p[0] - 4.0).abs() < 1e-6) && (0.0..=1.5).contains(&p[1]),
                "{p:?}"
            );
        }
        drop(wire);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
