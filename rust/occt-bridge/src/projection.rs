//! Orthographic hidden-line removal and curve sampling for drawing exports.
use super::*;

/// Exact located line or conic. Conic points are center + major*cos(t) +
/// minor*sin(t); parameters follow edge orientation (possibly descending).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnalyticCurve {
    Line {
        start: Vec3,
        end: Vec3,
    },
    Conic {
        center: Vec3,
        major: Vec3,
        minor: Vec3,
        first: f64,
        last: f64,
    },
}

/// One exact rational Bezier span, parameterized on [0, 1].
#[derive(Clone, Debug, PartialEq)]
pub struct BezierSpan {
    pub poles: Vec<Vec3>,
    pub weights: Vec<f64>,
}

impl Session {
    /// Exact non-destructive solid clipping by an infinite plane. The retained
    /// side includes the plane. Result may be empty and keeps input ancestry.
    pub fn clip_by_plane<'a>(
        &'a self,
        shape: &Shape<'_>,
        origin: Vec3,
        normal: Vec3,
        keep_positive: bool,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(shape)?;
        let mut out = 0;
        // SAFETY: Valid session-owned input and a writable output handle.
        self.check(unsafe {
            occt_bridge_clip_by_plane(
                self.raw.as_ptr(),
                shape.id,
                origin.into(),
                normal.into(),
                i32::from(keep_positive),
                &mut out,
            )
        })?;
        Ok(self.shape(out))
    }
    /// Exact HLR with a private geometry copy, preserving the input. Output
    /// compounds contain XY edges (z=0), including sharp/smooth/outline edges.
    /// Kernel work can scale with edge/face pairs; coincident lines may remain.
    pub fn orthographic_projection<'a>(
        &'a self,
        shape: &Shape<'_>,
        frame: ProjectionFrame,
    ) -> Result<ProjectedEdges<'a>, BridgeError> {
        self.validate_shape(shape)?;
        let mut visible = 0;
        let mut hidden = 0;
        // SAFETY: Valid session-owned input and distinct writable outputs.
        self.check(unsafe {
            occt_bridge_orthographic_projection(
                self.raw.as_ptr(),
                shape.id,
                frame.origin.into(),
                frame.direction.into(),
                frame.x_axis.into(),
                &mut visible,
                &mut hidden,
            )
        })?;
        Ok(ProjectedEdges {
            visible: self.shape(visible),
            hidden: self.shape(hidden),
        })
    }

    /// O(1) time/storage; unsupported types return None without sampling.
    pub fn edge_analytic_curve(
        &self,
        edge: &Shape<'_>,
    ) -> Result<Option<AnalyticCurve>, BridgeError> {
        self.validate_shape(edge)?;
        let mut out = RawAnalyticCurve::default();
        // SAFETY: Session-owned input and initialized writable output record.
        self.check(unsafe {
            occt_bridge_edge_analytic_curve(self.raw.as_ptr(), edge.id, &mut out)
        })?;
        Ok(match out.kind {
            1 => Some(AnalyticCurve::Line {
                start: out.origin.into(),
                end: out.x_vector.into(),
            }),
            2 | 3 => Some(AnalyticCurve::Conic {
                center: out.origin.into(),
                major: out.x_vector.into(),
                minor: out.y_vector.into(),
                first: out.first,
                last: out.last,
            }),
            _ => None,
        })
    }

    /// Convert a finite standard edge to exact rational Bezier spans. Two kernel
    /// conversions (count/fill); returned storage is bounded by maximum_poles.
    /// Other/offset curves return an empty vector. Inputs remain unchanged.
    pub fn edge_bezier_spans(
        &self,
        edge: &Shape<'_>,
        maximum_poles: usize,
    ) -> Result<Vec<BezierSpan>, BridgeError> {
        self.validate_shape(edge)?;
        let mut count = 0;
        // SAFETY: Valid session input, writable count and null count-query buffer.
        self.check(unsafe {
            occt_bridge_edge_bezier_poles(
                self.raw.as_ptr(),
                edge.id,
                maximum_poles,
                ptr::null_mut(),
                0,
                &mut count,
            )
        })?;
        let mut raw: Vec<_> = (0..count).map(|_| RawBezierPole::default()).collect();
        // SAFETY: Count-query sized, initialized output buffer and writable count.
        self.check(unsafe {
            occt_bridge_edge_bezier_poles(
                self.raw.as_ptr(),
                edge.id,
                maximum_poles,
                raw.as_mut_ptr(),
                raw.len(),
                &mut count,
            )
        })?;
        let mut spans: Vec<BezierSpan> = Vec::new();
        for pole in raw.into_iter().take(count) {
            if pole.span_index == spans.len() {
                spans.push(BezierSpan {
                    poles: Vec::new(),
                    weights: Vec::new(),
                });
            }
            let span = spans
                .last_mut()
                .expect("native poles have contiguous span indices");
            span.poles.push(pole.point.into());
            span.weights.push(pole.weight);
        }
        Ok(spans)
    }

    /// Uniform normalized-parameter points along an exact curve, following edge
    /// orientation. O(count) time/storage, 2..=100000 points; no chordal tolerance
    /// guarantee. Useful for bounded-resolution polyline export.
    pub fn edge_sample_points(
        &self,
        edge: &Shape<'_>,
        count: usize,
    ) -> Result<Vec<Vec3>, BridgeError> {
        self.validate_shape(edge)?;
        if !(2..=100_000).contains(&count) {
            return Err(BridgeError {
                status: 1,
                category: "invalid argument".into(),
                message: "edge sampling needs 2–100000 points".into(),
                diagnostics: Vec::new(),
            });
        }
        let mut points = (0..count)
            .map(|_| RawVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            })
            .collect::<Vec<_>>();
        // SAFETY: Valid session-owned edge and count initialized writable entries.
        self.check(unsafe {
            occt_bridge_edge_sample_points(self.raw.as_ptr(), edge.id, count, points.as_mut_ptr())
        })?;
        Ok(points.into_iter().map(Vec3::from).collect())
    }
}
