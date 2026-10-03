//! Modeling operations: booleans, fillets, chamfers, offsets, hollowing,
//! and transforms.

use super::*;

impl Session {
    pub fn fuse<'a>(
        &'a self,
        left: &Shape<'_>,
        right: &Shape<'_>,
    ) -> Result<Shape<'a>, BridgeError> {
        self.boolean(left, right, occt_bridge_fuse)
    }

    pub fn cut<'a>(
        &'a self,
        object: &Shape<'_>,
        tool: &Shape<'_>,
    ) -> Result<Shape<'a>, BridgeError> {
        self.boolean(object, tool, occt_bridge_cut)
    }

    pub fn common<'a>(
        &'a self,
        left: &Shape<'_>,
        right: &Shape<'_>,
    ) -> Result<Shape<'a>, BridgeError> {
        self.boolean(left, right, occt_bridge_common)
    }

    pub fn fillet<'a>(
        &'a self,
        shape: &Shape<'_>,
        edges: &[&Shape<'_>],
        radius: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.selected_operation(shape, edges, |ids, out| unsafe {
            occt_bridge_fillet(
                self.raw.as_ptr(),
                shape.id,
                ids.as_ptr(),
                ids.len(),
                radius,
                out,
            )
        })
    }

    /// Linearly varies radius from OCCT's first to last spine vertex of each
    /// selected open tangent contour. Both radii are finite positive lengths.
    /// Tangent neighbors may be included; duplicate edges and closed contours
    /// fail. Contour direction is defined by OCCT, not by edge orientation.
    pub fn variable_fillet<'a>(
        &'a self,
        shape: &Shape<'_>,
        edges: &[&Shape<'_>],
        start_radius: f64,
        end_radius: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.selected_operation(shape, edges, |ids, out| unsafe {
            occt_bridge_variable_fillet(
                self.raw.as_ptr(),
                shape.id,
                ids.as_ptr(),
                ids.len(),
                start_radius,
                end_radius,
                out,
            )
        })
    }

    /// OCCT smooth interpolation through ordered stations covering [0,1].
    /// Positive finite radii and a unique starting end are required. Setup is
    /// O(stations * selected contours); kernel blend cost is geometry dependent.
    pub fn variable_fillet_stations<'a>(
        &'a self,
        shape: &Shape<'_>,
        edges: &[&Shape<'_>],
        stations: &[FilletRadiusStation],
        direction: FilletSpineDirection,
    ) -> Result<Shape<'a>, BridgeError> {
        let values = stations
            .iter()
            .map(|station| RawFilletStation {
                position: station.position,
                radius: station.radius,
            })
            .collect::<Vec<_>>();
        let (direction, point) = match direction {
            FilletSpineDirection::Kernel => (0, Vec3::new(0.0, 0.0, 0.0)),
            FilletSpineDirection::Reversed => (1, Vec3::new(0.0, 0.0, 0.0)),
            FilletSpineDirection::FromPoint(point) => (2, point),
        };
        self.selected_operation(shape, edges, |ids, out| unsafe {
            occt_bridge_variable_fillet_stations(
                self.raw.as_ptr(),
                shape.id,
                ids.as_ptr(),
                ids.len(),
                values.as_ptr(),
                values.len(),
                direction,
                point.into(),
                out,
            )
        })
    }

    pub fn chamfer<'a>(
        &'a self,
        shape: &Shape<'_>,
        edges: &[&Shape<'_>],
        distance: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.selected_operation(shape, edges, |ids, out| unsafe {
            occt_bridge_chamfer(
                self.raw.as_ptr(),
                shape.id,
                ids.as_ptr(),
                ids.len(),
                distance,
                out,
            )
        })
    }

    pub fn offset<'a>(
        &'a self,
        shape: &Shape<'_>,
        offset: f64,
        tolerance: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shape, |out| unsafe {
            occt_bridge_offset(self.raw.as_ptr(), shape.id, offset, tolerance, out)
        })
    }

    /// Taper selected faces; tangential neighbors may also be modified.
    pub fn draft<'a>(
        &'a self,
        shape: &Shape<'_>,
        faces: &[&Shape<'_>],
        options: DraftOptions,
    ) -> Result<Shape<'a>, BridgeError> {
        self.selected_operation(shape, faces, |ids, out| unsafe {
            occt_bridge_draft(
                self.raw.as_ptr(),
                shape.id,
                ids.as_ptr(),
                ids.len(),
                options.neutral_origin.into(),
                options.neutral_normal.into(),
                options.pull_direction.into(),
                options.angle_radians,
                out,
            )
        })
    }

    pub fn hollow<'a>(
        &'a self,
        shape: &Shape<'_>,
        faces_to_remove: &[&Shape<'_>],
        thickness: f64,
        tolerance: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.selected_operation(shape, faces_to_remove, |ids, out| unsafe {
            occt_bridge_hollow(
                self.raw.as_ptr(),
                shape.id,
                ids.as_ptr(),
                ids.len(),
                thickness,
                tolerance,
                out,
            )
        })
    }

    pub fn translate<'a>(
        &'a self,
        shape: &Shape<'_>,
        offset: Vec3,
    ) -> Result<Shape<'a>, BridgeError> {
        self.transform(shape, |out| unsafe {
            occt_bridge_translate(self.raw.as_ptr(), shape.id, offset.into(), out)
        })
    }

    pub fn rotate<'a>(
        &'a self,
        shape: &Shape<'_>,
        axis_origin: Vec3,
        axis_direction: Vec3,
        angle_radians: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.transform(shape, |out| unsafe {
            occt_bridge_rotate(
                self.raw.as_ptr(),
                shape.id,
                axis_origin.into(),
                axis_direction.into(),
                angle_radians,
                out,
            )
        })
    }

    pub fn scale<'a>(
        &'a self,
        shape: &Shape<'_>,
        center: Vec3,
        factor: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.transform(shape, |out| unsafe {
            occt_bridge_scale(self.raw.as_ptr(), shape.id, center.into(), factor, out)
        })
    }

    pub(crate) fn boolean<'a>(
        &'a self,
        left: &Shape<'_>,
        right: &Shape<'_>,
        operation: unsafe extern "C" fn(
            *mut c_void,
            RawShapeId,
            RawShapeId,
            *mut RawShapeId,
        ) -> RawStatus,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(left)?;
        self.validate_shape(right)?;
        let mut shape = 0;
        // SAFETY: All pointers and handles are passed to the validating C boundary.
        self.check(unsafe { operation(self.raw.as_ptr(), left.id, right.id, &mut shape) })?;
        Ok(self.shape(shape))
    }

    pub(crate) fn transform<'a>(
        &'a self,
        shape: &Shape<'_>,
        operation: impl FnOnce(*mut RawShapeId) -> RawStatus,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(shape)?;
        let mut transformed = 0;
        self.check(operation(&mut transformed))?;
        Ok(self.shape(transformed))
    }

    pub(crate) fn derived_shape<'a>(
        &'a self,
        input: &Shape<'_>,
        operation: impl FnOnce(*mut RawShapeId) -> RawStatus,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(input)?;
        let mut result = 0;
        self.check(operation(&mut result))?;
        Ok(self.shape(result))
    }

    pub(crate) fn selected_operation<'a>(
        &'a self,
        shape: &Shape<'_>,
        selections: &[&Shape<'_>],
        operation: impl FnOnce(&[RawShapeId], *mut RawShapeId) -> RawStatus,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(shape)?;
        for selection in selections {
            self.validate_shape(selection)?;
        }
        let ids: Vec<RawShapeId> = selections.iter().map(|selection| selection.id).collect();
        let mut result = 0;
        self.check(operation(&ids, &mut result))?;
        Ok(self.shape(result))
    }
}
