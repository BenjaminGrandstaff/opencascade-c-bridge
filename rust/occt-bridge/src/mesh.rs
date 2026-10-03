//! Caller-owned triangulation, preserving the exact input geometry and mesh.
use super::*;

impl Session {
    /// O(topology + candidates) lookup; all results publish atomically.
    pub fn subshape_indices(
        &self,
        shape: &Shape<'_>,
        kind: ShapeType,
        candidates: &[&Shape<'_>],
    ) -> Result<Vec<usize>, BridgeError> {
        self.validate_shape(shape)?;
        for candidate in candidates {
            self.validate_shape(candidate)?;
        }
        let ids: Vec<_> = candidates.iter().map(|shape| shape.id).collect();
        let mut indices = vec![0; ids.len()];
        // SAFETY: Valid session-owned shapes and matching initialized buffers.
        self.check(unsafe {
            occt_bridge_subshape_indices(
                self.raw.as_ptr(),
                shape.id,
                kind as c_int,
                ids.as_ptr(),
                ids.len(),
                indices.as_mut_ptr(),
            )
        })?;
        Ok(indices)
    }
    /// Tessellate on private copies, with oriented triangles and source face
    /// indices. Count and fill each mesh once. The returned triangle budget does
    /// not bound OCCT's meshing workspace. Linear deflection must be at least
    /// max(1e-7, bounding span * 1e-5); angular deflection is 0.01..=pi radians.
    pub fn surface_mesh(
        &self,
        shape: &Shape<'_>,
        options: MeshOptions,
    ) -> Result<Vec<MeshTriangle>, BridgeError> {
        self.validate_shape(shape)?;
        let raw_options = || RawMeshOptions {
            linear_deflection: options.linear_deflection,
            angular_deflection: options.angular_deflection_radians,
            maximum_triangles: options.maximum_triangles,
        };
        let mut count = 0;
        // SAFETY: Valid session-owned shape, writable count, null count-query buffer.
        self.check(unsafe {
            occt_bridge_surface_mesh(
                self.raw.as_ptr(),
                shape.id,
                raw_options(),
                std::ptr::null_mut(),
                0,
                &mut count,
            )
        })?;
        let mut triangles = (0..count)
            .map(|_| RawMeshTriangle {
                face_index: 0,
                points: std::array::from_fn(|_| RawVec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                }),
            })
            .collect::<Vec<_>>();
        // SAFETY: Initialized writable entries matching the reported capacity.
        self.check(unsafe {
            occt_bridge_surface_mesh(
                self.raw.as_ptr(),
                shape.id,
                raw_options(),
                triangles.as_mut_ptr(),
                triangles.len(),
                &mut count,
            )
        })?;
        triangles.truncate(count);
        Ok(triangles
            .into_iter()
            .map(|triangle| MeshTriangle {
                face_index: triangle.face_index,
                points: triangle
                    .points
                    .map(|point| Vec3::new(point.x, point.y, point.z)),
            })
            .collect())
    }
}
