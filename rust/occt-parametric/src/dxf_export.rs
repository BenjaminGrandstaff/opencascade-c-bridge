//! DXF export of a planar face: its outer boundary on layer `OUTLINE`, its
//! inner boundaries on `HOLES`, and optional bend lines on `BEND`, in
//! millimeters, for laser, waterjet and plasma cutting.
//!
//! Curves stay exact: lines are LINE, circles and arcs CIRCLE and ARC,
//! ellipses ELLIPSE, and any other curve one rational SPLINE per exact Bézier
//! span of the kernel's edge. Coordinates are the face's plane seen from its
//! outward side: X right and Y up for faces facing ±Z (as seen from +Z),
//! otherwise X horizontal and Y toward +Z. O(edges + spans) time.
use crate::{
    FaceSelector, FeatureDefinition, FeatureOperation, GraphRegeneration, InstanceGraph,
    InstanceOutputRef, ModelError, PartInstanceSelection, evaluate_resolved_expression,
};
use occt_bridge::{AnalyticCurve, Session, Shape, ShapeType, Vec3};
use serde::Serialize;
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};
use std::fmt::Write as _;

/// Most poles one edge may convert to before export is refused.
const MAX_EDGE_POLES: usize = 100_000;

/// What a DXF export wrote.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DxfExportReport {
    /// Entities written by DXF type.
    pub entities: BTreeMap<String, usize>,
    /// Entities per layer.
    pub layers: BTreeMap<String, usize>,
    /// The face's area, which the exported outline encloses.
    pub area_mm2: f64,
    /// Extent of the exported geometry in plane coordinates (an ellipse
    /// counts its major radius all round, a spline its control points).
    pub bounds_mm: [[f64; 2]; 2],
    /// Plane frame: the origin of plane coordinates and their axes, in model
    /// coordinates.
    pub origin_mm: [f64; 3],
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}
fn unit(a: Vec3) -> Option<Vec3> {
    let l = dot(a, a).sqrt();
    (l.is_finite() && l > 1e-12).then(|| Vec3::new(a.x / l, a.y / l, a.z / l))
}

/// The plane coordinates of a face: origin and in-plane axes.
struct Frame {
    origin: Vec3,
    x: Vec3,
    y: Vec3,
}

impl Frame {
    fn new(normal: Vec3, on_plane: Vec3) -> Result<Self, ModelError> {
        let n = unit(normal).ok_or_else(|| ModelError::new("DXF face has no normal"))?;
        let z = Vec3::new(0.0, 0.0, 1.0);
        let (x, y) = if n.z.abs() > 1.0 - 1e-9 {
            (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0))
        } else {
            let x = unit(cross(z, n)).expect("n is not along Z");
            (x, cross(n, x))
        };
        // Plane coordinates measure from the plane's point nearest the
        // model origin, so a face at z = h keeps its XY coordinates.
        let d = dot(on_plane, n);
        let origin = Vec3::new(n.x * d, n.y * d, n.z * d);
        Ok(Self { origin, x, y })
    }
    fn point(&self, p: Vec3) -> [f64; 2] {
        let d = Vec3::new(
            p.x - self.origin.x,
            p.y - self.origin.y,
            p.z - self.origin.z,
        );
        [dot(d, self.x), dot(d, self.y)]
    }
    fn vector(&self, v: Vec3) -> [f64; 2] {
        [dot(v, self.x), dot(v, self.y)]
    }
}

/// Collects entities and their extent.
struct Writer {
    body: String,
    report: DxfExportReport,
    lo: [f64; 2],
    hi: [f64; 2],
}

impl Writer {
    fn count(&mut self, entity: &str, layer: &str) {
        *self.report.entities.entry(entity.into()).or_default() += 1;
        *self.report.layers.entry(layer.into()).or_default() += 1;
    }
    fn extend(&mut self, p: [f64; 2]) {
        self.lo = [self.lo[0].min(p[0]), self.lo[1].min(p[1])];
        self.hi = [self.hi[0].max(p[0]), self.hi[1].max(p[1])];
    }
    fn line(&mut self, layer: &str, a: [f64; 2], b: [f64; 2]) {
        write!(
            self.body,
            "0\nLINE\n8\n{layer}\n10\n{}\n20\n{}\n30\n0.0\n11\n{}\n21\n{}\n31\n0.0\n",
            a[0], a[1], b[0], b[1]
        )
        .unwrap();
        self.extend(a);
        self.extend(b);
        self.count("LINE", layer);
    }
}

/// Angle of `p` about `c` in [0, 2π).
fn angle(c: [f64; 2], p: [f64; 2]) -> f64 {
    (p[1] - c[1]).atan2(p[0] - c[0]).rem_euclid(TAU)
}
/// Whether `m` lies on the counterclockwise sweep from `a` to `b`.
fn on_sweep(a: f64, b: f64, m: f64) -> bool {
    (m - a).rem_euclid(TAU) < (b - a).rem_euclid(TAU)
}

/// The DXF of a generated output: the face `face` selects, or its largest
/// planar face (the first of equal ones), with a sheet-metal flat pattern's
/// bend lines when `bend_lines`. O(faces + edges + spans).
pub fn output_to_dxf(
    session: &Session,
    graph: &InstanceGraph<'_>,
    generation: &GraphRegeneration<'_>,
    output: &InstanceOutputRef,
    face: Option<&FaceSelector>,
    bend_lines: bool,
) -> Result<(String, DxfExportReport), ModelError> {
    let result = generation
        .result(&output.instance)
        .ok_or_else(|| ModelError::new(format!("unknown instance '{}'", output.instance)))?;
    let shape = result
        .shape(&output.output)
        .ok_or_else(|| ModelError::new(format!("output '{}' is not available", output.output)))?;
    let faces = match face {
        Some(selector) => graph.resolve(&output.instance)?.select_faces(
            session,
            result,
            &output.output,
            selector,
        )?,
        None => {
            let mut planar = Vec::new();
            for face in session.subshapes(shape, ShapeType::Face)? {
                if session.face_is_planar(&face)? {
                    planar.push((session.surface_area(&face)?, face));
                }
            }
            let best = planar
                .iter()
                .map(|(a, _)| *a)
                .fold(f64::NEG_INFINITY, f64::max);
            planar
                .into_iter()
                .filter(|(a, _)| *a >= best * (1.0 - 1e-9))
                .take(1)
                .map(|(_, f)| f)
                .collect()
        }
    };
    let [face] = faces.as_slice() else {
        return Err(ModelError::new(format!(
            "DXF export of {}/{} needs one planar face; the selection gave {}",
            output.instance,
            output.output,
            faces.len()
        )));
    };
    let bends = if bend_lines {
        flat_bend_lines(graph, output)?
    } else {
        Vec::new()
    };
    face_to_dxf(session, face, &bends)
}

/// The bend lines of a sheet-metal flat-pattern output, placed like its
/// instance; empty for any other output.
pub fn flat_bend_lines(
    graph: &InstanceGraph<'_>,
    output: &InstanceOutputRef,
) -> Result<Vec<[Vec3; 2]>, ModelError> {
    let instance = graph.resolve(&output.instance)?;
    let features = &instance.definition.features;
    let find = |id: &str| features.iter().find(|f| f.id == id);
    let Some(FeatureDefinition {
        operation:
            FeatureOperation::SheetMetalFlat {
                input,
                neutral_factor,
                bend_table,
            },
        ..
    }) = find(&output.output)
    else {
        return Ok(Vec::new());
    };
    let Some(FeatureDefinition {
        operation: FeatureOperation::SheetMetal { definition },
        ..
    }) = find(input)
    else {
        return Ok(Vec::new());
    };
    let parameters = instance.resolved_parameters()?;
    let factor = evaluate_resolved_expression(neutral_factor, &parameters)?.value;
    definition
        .flat_bend_lines(&parameters, factor, bend_table.as_deref())?
        .into_iter()
        .map(|[a, b]| {
            Ok([
                graph.instance_point(&output.instance, a)?,
                graph.instance_point(&output.instance, b)?,
            ])
        })
        .collect()
}

/// Writes a planar face as DXF. `bend_lines` (model coordinates) go on layer
/// `BEND`.
pub fn face_to_dxf(
    session: &Session,
    face: &Shape<'_>,
    bend_lines: &[[Vec3; 2]],
) -> Result<(String, DxfExportReport), ModelError> {
    if session.shape_type(face)? != ShapeType::Face || !session.face_is_planar(face)? {
        return Err(ModelError::new("DXF export needs one planar face"));
    }
    let frame = Frame::new(session.face_normal(face)?, session.center_of_mass(face)?)?;
    let outer = session.face_boundary(face, 0)?;
    let mut writer = Writer {
        body: String::new(),
        report: DxfExportReport {
            area_mm2: session.surface_area(face)?,
            // Adding zero turns a negative zero positive.
            origin_mm: [
                frame.origin.x + 0.0,
                frame.origin.y + 0.0,
                frame.origin.z + 0.0,
            ],
            x_axis: [frame.x.x, frame.x.y, frame.x.z],
            y_axis: [frame.y.x, frame.y.y, frame.y.z],
            ..Default::default()
        },
        lo: [f64::INFINITY; 2],
        hi: [f64::NEG_INFINITY; 2],
    };
    for wire in session.subshapes(face, ShapeType::Wire)? {
        let layer = if session.is_same(&wire, &outer)? {
            "OUTLINE"
        } else {
            "HOLES"
        };
        for edge in session.subshapes(&wire, ShapeType::Edge)? {
            write_edge(session, &edge, &frame, layer, &mut writer)?;
        }
    }
    // A flat pattern gives each bend line on both faces of the sheet; they
    // coincide in the plane and are written once.
    let mut written: Vec<[[f64; 2]; 2]> = Vec::new();
    let tolerance = 1e-9 * (1.0 + writer.hi[0] - writer.lo[0] + writer.hi[1] - writer.lo[1]);
    for [a, b] in bend_lines {
        let (a, b) = (frame.point(*a), frame.point(*b));
        let same = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) <= tolerance;
        if written
            .iter()
            .any(|[c, d]| (same(a, *c) && same(b, *d)) || (same(a, *d) && same(b, *c)))
        {
            continue;
        }
        written.push([a, b]);
        writer.line("BEND", a, b);
    }
    if writer.report.entities.is_empty() {
        return Err(ModelError::new("DXF face has no edges"));
    }
    writer.report.bounds_mm = [writer.lo, writer.hi];
    let mut text = String::from(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$INSUNITS\n70\n4\n9\n$MEASUREMENT\n70\n1\n0\nENDSEC\n",
    );
    text += "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n3\n";
    for (layer, color) in [("OUTLINE", 7), ("HOLES", 5), ("BEND", 1)] {
        write!(
            text,
            "0\nLAYER\n2\n{layer}\n70\n0\n62\n{color}\n6\nCONTINUOUS\n"
        )
        .unwrap();
    }
    text += "0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n";
    text += &writer.body;
    text += "0\nENDSEC\n0\nEOF\n";
    Ok((text, writer.report))
}

fn write_edge(
    session: &Session,
    edge: &Shape<'_>,
    frame: &Frame,
    layer: &str,
    writer: &mut Writer,
) -> Result<(), ModelError> {
    match session.edge_analytic_curve(edge)? {
        Some(AnalyticCurve::Line { start, end }) => {
            writer.line(layer, frame.point(start), frame.point(end));
        }
        Some(AnalyticCurve::Conic {
            center,
            major,
            minor,
            first,
            last,
        }) => {
            let c = frame.point(center);
            let (a, b) = (frame.vector(major), frame.vector(minor));
            let at = |t: f64| {
                [
                    c[0] + a[0] * t.cos() + b[0] * t.sin(),
                    c[1] + a[1] * t.cos() + b[1] * t.sin(),
                ]
            };
            let (ra, rb) = (a[0].hypot(a[1]), b[0].hypot(b[1]));
            let full = ((last - first).abs() - TAU).abs() < 1e-9;
            let (start, middle, end) = (at(first), at((first + last) / 2.0), at(last));
            if (ra - rb).abs() <= 1e-9 * ra.max(1.0) {
                if full {
                    write!(
                        writer.body,
                        "0\nCIRCLE\n8\n{layer}\n10\n{}\n20\n{}\n30\n0.0\n40\n{ra}\n",
                        c[0], c[1]
                    )
                    .unwrap();
                    writer.extend([c[0] - ra, c[1] - ra]);
                    writer.extend([c[0] + ra, c[1] + ra]);
                    writer.count("CIRCLE", layer);
                } else {
                    let (s, m, e) = (angle(c, start), angle(c, middle), angle(c, end));
                    let (s, e) = if on_sweep(s, e, m) { (s, e) } else { (e, s) };
                    write!(
                        writer.body,
                        "0\nARC\n8\n{layer}\n10\n{}\n20\n{}\n30\n0.0\n40\n{ra}\n50\n{}\n51\n{}\n",
                        c[0],
                        c[1],
                        s.to_degrees(),
                        e.to_degrees()
                    )
                    .unwrap();
                    // The sweep's extent: its ends and any axis it crosses.
                    writer.extend(start);
                    writer.extend(end);
                    for k in 0..4 {
                        let q = k as f64 * PI / 2.0;
                        if on_sweep(s, e, q) {
                            writer.extend([c[0] + ra * q.cos(), c[1] + ra * q.sin()]);
                        }
                    }
                    writer.count("ARC", layer);
                }
            } else {
                // DXF major axis, ratio and parameters: P = C + M cos p + m sin p
                // with m the major turned a quarter counterclockwise, scaled.
                let (major_axis, ratio) = if ra >= rb { (a, rb / ra) } else { (b, ra / rb) };
                let m = [-major_axis[1] * ratio, major_axis[0] * ratio];
                let parameter = |p: [f64; 2]| {
                    let d = [p[0] - c[0], p[1] - c[1]];
                    let cos = (d[0] * major_axis[0] + d[1] * major_axis[1])
                        / (major_axis[0].powi(2) + major_axis[1].powi(2));
                    let sin = (d[0] * m[0] + d[1] * m[1]) / (m[0].powi(2) + m[1].powi(2));
                    sin.atan2(cos).rem_euclid(TAU)
                };
                let (s, e) = if full {
                    (0.0, TAU)
                } else {
                    let (s, mid, e) = (parameter(start), parameter(middle), parameter(end));
                    if on_sweep(s, e, mid) { (s, e) } else { (e, s) }
                };
                write!(
                    writer.body,
                    "0\nELLIPSE\n8\n{layer}\n10\n{}\n20\n{}\n30\n0.0\n11\n{}\n21\n{}\n31\n0.0\n40\n{ratio}\n41\n{s}\n42\n{}\n",
                    c[0], c[1], major_axis[0], major_axis[1], if e <= s { e + TAU } else { e }
                )
                .unwrap();
                let r = ra.max(rb);
                writer.extend([c[0] - r, c[1] - r]);
                writer.extend([c[0] + r, c[1] + r]);
                writer.count("ELLIPSE", layer);
            }
        }
        None => {
            let spans = session.edge_bezier_spans(edge, MAX_EDGE_POLES)?;
            if spans.is_empty() {
                return Err(ModelError::new(
                    "DXF export met an edge it cannot convert to exact curves",
                ));
            }
            for span in spans {
                let degree = span.poles.len() - 1;
                let rational = span.weights.iter().any(|w| (w - 1.0).abs() > 1e-12);
                write!(
                    writer.body,
                    "0\nSPLINE\n8\n{layer}\n210\n0.0\n220\n0.0\n230\n1.0\n70\n{}\n71\n{degree}\n72\n{}\n73\n{}\n74\n0\n",
                    8 + if rational { 4 } else { 0 },
                    2 * (degree + 1),
                    span.poles.len()
                )
                .unwrap();
                for k in 0..2 * (degree + 1) {
                    write!(writer.body, "40\n{}\n", if k <= degree { 0 } else { 1 }).unwrap();
                }
                // Weights come as one group before the control points.
                if rational {
                    for weight in &span.weights {
                        write!(writer.body, "41\n{weight}\n").unwrap();
                    }
                }
                for pole in &span.poles {
                    let p = frame.point(*pole);
                    write!(writer.body, "10\n{}\n20\n{}\n30\n0.0\n", p[0], p[1]).unwrap();
                    writer.extend(p);
                }
                writer.count("SPLINE", layer);
            }
        }
    }
    Ok(())
}
