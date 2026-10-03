//! Orthographic hidden-line removal and curve sampling for drawing exports.
use super::*;

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
