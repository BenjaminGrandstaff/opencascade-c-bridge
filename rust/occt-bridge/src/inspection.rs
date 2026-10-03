//! Shape inspection: topology, measurements, curvature, adjacency, history,
//! and validity.

use super::*;

impl Session {
    /// Axis-aligned bounds, enlarged by shape tolerances as OCCT reports them.
    pub fn bounds(&self, shape: &Shape<'_>) -> Result<Bounds, BridgeError> {
        self.query_bounds(shape, occt_bridge_shape_bounds)
    }

    /// Axis-aligned bounds that follow the geometry without tolerance
    /// enlargement; use these to measure lengths.
    pub fn exact_bounds(&self, shape: &Shape<'_>) -> Result<Bounds, BridgeError> {
        self.query_bounds(shape, occt_bridge_shape_exact_bounds)
    }

    pub(crate) fn query_bounds(
        &self,
        shape: &Shape<'_>,
        query: unsafe extern "C" fn(*mut c_void, RawShapeId, *mut RawBounds) -> RawStatus,
    ) -> Result<Bounds, BridgeError> {
        self.validate_shape(shape)?;
        let mut bounds = RawBounds {
            min: RawVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            max: RawVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
        };
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe { query(self.raw.as_ptr(), shape.id, &mut bounds) })?;
        Ok(Bounds {
            min: bounds.min.into(),
            max: bounds.max.into(),
        })
    }

    pub fn shape_type(&self, shape: &Shape<'_>) -> Result<ShapeType, BridgeError> {
        self.validate_shape(shape)?;
        let mut shape_type = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_type(self.raw.as_ptr(), shape.id, &mut shape_type)
        })?;
        match shape_type {
            1 => Ok(ShapeType::Compound),
            2 => Ok(ShapeType::CompSolid),
            3 => Ok(ShapeType::Solid),
            4 => Ok(ShapeType::Shell),
            5 => Ok(ShapeType::Face),
            6 => Ok(ShapeType::Wire),
            7 => Ok(ShapeType::Edge),
            8 => Ok(ShapeType::Vertex),
            value => Err(BridgeError {
                status: 8,
                category: "internal error".into(),
                message: format!("library returned unknown shape type {value}"),
                diagnostics: Vec::new(),
            }),
        }
    }

    pub fn duplicate<'a>(&'a self, shape: &Shape<'_>) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shape, |out| unsafe {
            occt_bridge_shape_duplicate(self.raw.as_ptr(), shape.id, out)
        })
    }

    pub fn subshape_count(
        &self,
        shape: &Shape<'_>,
        subshape_type: ShapeType,
    ) -> Result<usize, BridgeError> {
        self.validate_shape(shape)?;
        let mut count = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_subshape_count(
                self.raw.as_ptr(),
                shape.id,
                subshape_type as c_int,
                &mut count,
            )
        })?;
        Ok(count)
    }

    pub fn subshape<'a>(
        &'a self,
        shape: &Shape<'_>,
        subshape_type: ShapeType,
        index: usize,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(shape)?;
        let mut subshape = 0;
        // SAFETY: The session and output pointers are valid; the C layer checks the index.
        self.check(unsafe {
            occt_bridge_shape_subshape_at(
                self.raw.as_ptr(),
                shape.id,
                subshape_type as c_int,
                index,
                &mut subshape,
            )
        })?;
        Ok(self.shape(subshape))
    }

    pub fn surface_area(&self, shape: &Shape<'_>) -> Result<f64, BridgeError> {
        self.validate_shape(shape)?;
        let mut area = 0.0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_surface_area(self.raw.as_ptr(), shape.id, &mut area)
        })?;
        Ok(area)
    }

    pub fn volume(&self, shape: &Shape<'_>) -> Result<f64, BridgeError> {
        self.validate_shape(shape)?;
        let mut volume = 0.0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe { occt_bridge_shape_volume(self.raw.as_ptr(), shape.id, &mut volume) })?;
        Ok(volume)
    }

    pub fn center_of_mass(&self, shape: &Shape<'_>) -> Result<Vec3, BridgeError> {
        self.validate_shape(shape)?;
        let mut center = RawVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_center_of_mass(self.raw.as_ptr(), shape.id, &mut center)
        })?;
        Ok(center.into())
    }

    pub fn face_normal(&self, face: &Shape<'_>) -> Result<Vec3, BridgeError> {
        self.validate_shape(face)?;
        let mut normal = RawVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        // SAFETY: The session, validated face handle, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_face_normal(self.raw.as_ptr(), face.id, &mut normal)
        })?;
        Ok(normal.into())
    }

    pub fn face_is_planar(&self, face: &Shape<'_>) -> Result<bool, BridgeError> {
        self.validate_shape(face)?;
        let mut planar = 0;
        // SAFETY: The session, validated face handle, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_face_is_planar(self.raw.as_ptr(), face.id, &mut planar)
        })?;
        Ok(planar != 0)
    }

    pub fn edge_length(&self, edge: &Shape<'_>) -> Result<f64, BridgeError> {
        self.validate_shape(edge)?;
        let mut length = 0.0;
        // SAFETY: The session, validated edge handle, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_edge_length(self.raw.as_ptr(), edge.id, &mut length)
        })?;
        Ok(length)
    }

    pub fn edge_circle_radius(&self, edge: &Shape<'_>) -> Result<Option<f64>, BridgeError> {
        self.validate_shape(edge)?;
        let mut is_circle = 0;
        let mut radius = 0.0;
        // SAFETY: The session, validated edge handle, and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_edge_circle_radius(
                self.raw.as_ptr(),
                edge.id,
                &mut is_circle,
                &mut radius,
            )
        })?;
        Ok((is_circle != 0).then_some(radius))
    }

    pub fn edge_curvature(&self, edge: &Shape<'_>) -> Result<Option<f64>, BridgeError> {
        self.validate_shape(edge)?;
        let mut is_defined = 0;
        let mut curvature = 0.0;
        // SAFETY: The session, validated edge handle, and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_edge_curvature(
                self.raw.as_ptr(),
                edge.id,
                &mut is_defined,
                &mut curvature,
            )
        })?;
        Ok((is_defined != 0).then_some(curvature))
    }

    /// Exact (line, conic) or error-bounded (Bezier, B-spline) curvature
    /// extrema over the full edge.
    pub fn edge_curvature_extrema(
        &self,
        edge: &Shape<'_>,
        relative_tolerance: f64,
    ) -> Result<CurvatureExtrema, BridgeError> {
        self.validate_shape(edge)?;
        let mut extrema = CurvatureExtrema {
            minimum: 0.0,
            minimum_lower_bound: 0.0,
            maximum: 0.0,
            maximum_upper_bound: 0.0,
            is_exact: false,
        };
        let mut is_exact = 0;
        // SAFETY: The session, validated edge handle, and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_edge_curvature_extrema(
                self.raw.as_ptr(),
                edge.id,
                relative_tolerance,
                &mut extrema.minimum,
                &mut extrema.minimum_lower_bound,
                &mut extrema.maximum,
                &mut extrema.maximum_upper_bound,
                &mut is_exact,
            )
        })?;
        extrema.is_exact = is_exact != 0;
        Ok(extrema)
    }

    pub fn edge_curvature_range(
        &self,
        edge: &Shape<'_>,
        sample_count: usize,
    ) -> Result<(f64, f64), BridgeError> {
        self.validate_shape(edge)?;
        let mut minimum = 0.0;
        let mut maximum = 0.0;
        // SAFETY: The session, validated edge handle, and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_edge_curvature_range(
                self.raw.as_ptr(),
                edge.id,
                sample_count,
                &mut minimum,
                &mut maximum,
            )
        })?;
        Ok((minimum, maximum))
    }

    pub fn is_adjacent(
        &self,
        parent: &Shape<'_>,
        first: &Shape<'_>,
        second: &Shape<'_>,
    ) -> Result<bool, BridgeError> {
        self.validate_shape(parent)?;
        self.validate_shape(first)?;
        self.validate_shape(second)?;
        let mut adjacent = 0;
        // SAFETY: The session, validated shape handles, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_is_adjacent(
                self.raw.as_ptr(),
                parent.id,
                first.id,
                second.id,
                &mut adjacent,
            )
        })?;
        Ok(adjacent != 0)
    }

    pub fn is_same(&self, first: &Shape<'_>, second: &Shape<'_>) -> Result<bool, BridgeError> {
        self.validate_shape(first)?;
        self.validate_shape(second)?;
        let mut same = 0;
        // SAFETY: The session, validated handles, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_is_same(self.raw.as_ptr(), first.id, second.id, &mut same)
        })?;
        Ok(same != 0)
    }

    pub fn faces_are_tangent(
        &self,
        parent: &Shape<'_>,
        first_face: &Shape<'_>,
        second_face: &Shape<'_>,
    ) -> Result<bool, BridgeError> {
        self.validate_shape(parent)?;
        self.validate_shape(first_face)?;
        self.validate_shape(second_face)?;
        let mut tangent = 0;
        // SAFETY: The session, validated handles, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_faces_are_tangent(
                self.raw.as_ptr(),
                parent.id,
                first_face.id,
                second_face.id,
                &mut tangent,
            )
        })?;
        Ok(tangent != 0)
    }

    pub fn history_count(
        &self,
        result: &Shape<'_>,
        source: &Shape<'_>,
        relation: HistoryRelation,
    ) -> Result<usize, BridgeError> {
        self.validate_shape(result)?;
        self.validate_shape(source)?;
        let mut count = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_history_count(
                self.raw.as_ptr(),
                result.id,
                source.id,
                relation as c_int,
                &mut count,
            )
        })?;
        Ok(count)
    }

    pub fn history<'a>(
        &'a self,
        result: &Shape<'_>,
        source: &Shape<'_>,
        relation: HistoryRelation,
        index: usize,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(result)?;
        self.validate_shape(source)?;
        let mut history_shape = 0;
        // SAFETY: The session and output pointers are valid; the C layer checks the index.
        self.check(unsafe {
            occt_bridge_shape_history_at(
                self.raw.as_ptr(),
                result.id,
                source.id,
                relation as c_int,
                index,
                &mut history_shape,
            )
        })?;
        Ok(self.shape(history_shape))
    }

    pub fn history_is_deleted(
        &self,
        result: &Shape<'_>,
        source: &Shape<'_>,
    ) -> Result<bool, BridgeError> {
        self.validate_shape(result)?;
        self.validate_shape(source)?;
        let mut deleted = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_history_is_deleted(
                self.raw.as_ptr(),
                result.id,
                source.id,
                &mut deleted,
            )
        })?;
        Ok(deleted != 0)
    }

    pub fn is_valid(&self, shape: &Shape<'_>) -> Result<bool, BridgeError> {
        self.validate_shape(shape)?;
        let mut valid = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe { occt_bridge_shape_is_valid(self.raw.as_ptr(), shape.id, &mut valid) })?;
        Ok(valid != 0)
    }
}
