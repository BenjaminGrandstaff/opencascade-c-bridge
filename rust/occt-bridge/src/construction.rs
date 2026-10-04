//! Shape construction: primitives, wires, faces, sweeps, lofts, sewing,
//! solids, compounds, and recipes.

use super::*;

impl Session {
    pub fn create_box(&self, origin: Vec3, size: Vec3) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: `self.raw` and the output pointer are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_box(self.raw.as_ptr(), origin.into(), size.into(), &mut shape)
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_cylinder(
        &self,
        origin: Vec3,
        axis: Vec3,
        radius: f64,
        height: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_cylinder(
                self.raw.as_ptr(),
                origin.into(),
                axis.into(),
                radius,
                height,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_cone(
        &self,
        origin: Vec3,
        axis: Vec3,
        base_radius: f64,
        top_radius: f64,
        height: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_cone(
                self.raw.as_ptr(),
                origin.into(),
                axis.into(),
                base_radius,
                top_radius,
                height,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_sphere(&self, center: Vec3, radius: f64) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; the center is passed by value.
        self.check(unsafe {
            occt_bridge_create_sphere(self.raw.as_ptr(), center.into(), radius, &mut shape)
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_polyline_wire(
        &self,
        points: &[Vec3],
        closed: bool,
    ) -> Result<Shape<'_>, BridgeError> {
        let points: Vec<RawVec3> = points.iter().copied().map(Into::into).collect();
        let mut shape = 0;
        // SAFETY: The point slice, session, and output remain valid for the call.
        self.check(unsafe {
            occt_bridge_create_polyline_wire(
                self.raw.as_ptr(),
                points.as_ptr(),
                points.len(),
                i32::from(closed),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    /// Creates an ordered, connected wire of exact lines and circular arcs.
    /// `closed` requires closure when true; false permits either open or closed
    /// wires. Planarity and absence of self-intersections are not required.
    pub fn create_segment_wire(
        &self,
        segments: &[WireSegment],
        closed: bool,
    ) -> Result<Shape<'_>, BridgeError> {
        let raw = segments
            .iter()
            .map(|segment| match *segment {
                WireSegment::Line { start, end } => RawWireSegment {
                    kind: 0,
                    start: start.into(),
                    middle: start.into(),
                    end: end.into(),
                },
                WireSegment::Arc { start, middle, end } => RawWireSegment {
                    kind: 1,
                    start: start.into(),
                    middle: middle.into(),
                    end: end.into(),
                },
            })
            .collect::<Vec<_>>();
        let mut shape = 0;
        // SAFETY: The segment slice, session and output remain valid during the call.
        self.check(unsafe {
            occt_bridge_create_segment_wire(
                self.raw.as_ptr(),
                raw.as_ptr(),
                raw.len(),
                i32::from(closed),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_circle_wire(
        &self,
        center: Vec3,
        normal: Vec3,
        radius: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_circle_wire(
                self.raw.as_ptr(),
                center.into(),
                normal.into(),
                radius,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_ellipse_wire(
        &self,
        center: Vec3,
        normal: Vec3,
        major_radius: f64,
        minor_radius: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_ellipse_wire(
                self.raw.as_ptr(),
                center.into(),
                normal.into(),
                major_radius,
                minor_radius,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    /// Close an open wire using a translated reversed copy and straight end
    /// bridges. The resulting boundary must be planar and non-self-intersecting.
    /// Original edges and their translated counterparts retain ancestry.
    pub fn create_open_profile_face<'a>(
        &'a self,
        wire: &Shape<'_>,
        offset: Vec3,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(wire, |out| unsafe {
            occt_bridge_create_open_profile_face(self.raw.as_ptr(), wire.id, offset.into(), out)
        })
    }

    /// Close a straight open chain by advancing perpendicularly to its first
    /// body contact, bounded by maximum_length. The whole translated chain
    /// must meet that contact. Inputs remain unchanged; ancestry is retained.
    pub fn create_open_profile_face_to_next<'a>(
        &'a self,
        wire: &Shape<'_>,
        body: &Shape<'_>,
        direction: Vec3,
        maximum_length: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(body)?;
        self.derived_shape(wire, |out| unsafe {
            occt_bridge_create_open_profile_face_to_next(
                self.raw.as_ptr(),
                wire.id,
                body.id,
                direction.into(),
                maximum_length,
                out,
            )
        })
    }

    pub fn create_face_from_wire<'a>(&'a self, wire: &Shape<'_>) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(wire, |out| unsafe {
            occt_bridge_create_face_from_wire(self.raw.as_ptr(), wire.id, out)
        })
    }

    pub fn create_prism_from_face<'a>(
        &'a self,
        face: &Shape<'_>,
        direction: Vec3,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(face, |out| unsafe {
            occt_bridge_create_prism_from_face(self.raw.as_ptr(), face.id, direction.into(), out)
        })
    }

    /// Revolves a face by a signed angle in radians, up to one full turn.
    pub fn create_revolve_from_face<'a>(
        &'a self,
        face: &Shape<'_>,
        origin: Vec3,
        axis: Vec3,
        angle_radians: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(face, |out| unsafe {
            occt_bridge_create_revolve_from_face(
                self.raw.as_ptr(),
                face.id,
                origin.into(),
                axis.into(),
                angle_radians,
                out,
            )
        })
    }

    pub fn create_polygon_prism(
        &self,
        points: &[Vec3],
        direction: Vec3,
    ) -> Result<Shape<'_>, BridgeError> {
        let raw_points: Vec<RawVec3> = points.iter().copied().map(Into::into).collect();
        let mut shape = 0;
        // SAFETY: The point slice and output remain valid for the duration of the call.
        self.check(unsafe {
            occt_bridge_create_polygon_prism(
                self.raw.as_ptr(),
                raw_points.as_ptr(),
                raw_points.len(),
                direction.into(),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    #[deprecated(note = "use occt_recipes::create_faceted_stone")]
    pub fn create_faceted_stone(
        &self,
        bottom_points: &[Vec3],
        top_points: &[Vec3],
        top_center: Vec3,
        bottom_chamfer: f64,
        top_fillet: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        if bottom_points.len() != top_points.len() {
            return Err(BridgeError {
                status: 1,
                category: "invalid argument".into(),
                message: "bottom and top rings must have the same point count".into(),
                diagnostics: Vec::new(),
            });
        }
        let bottom: Vec<RawVec3> = bottom_points.iter().copied().map(Into::into).collect();
        let top: Vec<RawVec3> = top_points.iter().copied().map(Into::into).collect();
        let mut shape = 0;
        // SAFETY: Both point arrays and the output remain valid for the call.
        self.check(unsafe {
            occt_bridge_create_faceted_stone(
                self.raw.as_ptr(),
                bottom.as_ptr(),
                top.as_ptr(),
                bottom.len(),
                top_center.into(),
                bottom_chamfer,
                top_fillet,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    #[deprecated(note = "use occt_recipes::create_wall_torch")]
    pub fn create_wall_torch(
        &self,
        wall_anchor: Vec3,
        wall_normal: Vec3,
        scale: f64,
    ) -> Result<WallTorch<'_>, BridgeError> {
        let mut result = RawWallTorchResult {
            fixture_shape: 0,
            flame_shape: 0,
            light: RawLightDesc {
                light_type: 0,
                position: RawVec3::from(Vec3::new(0.0, 0.0, 0.0)),
                direction: RawVec3::from(Vec3::new(0.0, 0.0, 0.0)),
                color: RawVec3::from(Vec3::new(0.0, 0.0, 0.0)),
                intensity: 0.0,
                range: 0.0,
                spot_angle_degrees: 0.0,
                cast_shadows: 0,
            },
        };
        // SAFETY: The session and output pointer are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_wall_torch(
                self.raw.as_ptr(),
                wall_anchor.into(),
                wall_normal.into(),
                scale,
                &mut result,
            )
        })?;
        Ok(WallTorch {
            fixture: self.shape(result.fixture_shape),
            flame: self.shape(result.flame_shape),
            light: LightDesc {
                light_type: match result.light.light_type {
                    0 => LightType::Ambient,
                    1 => LightType::Directional,
                    2 => LightType::Positional,
                    3 => LightType::Spotlight,
                    value => {
                        return Err(BridgeError {
                            status: 8,
                            category: "internal error".into(),
                            message: format!("library returned unknown light type {value}"),
                            diagnostics: Vec::new(),
                        });
                    }
                },
                position: result.light.position.into(),
                direction: result.light.direction.into(),
                color: result.light.color.into(),
                intensity: result.light.intensity,
                range: result.light.range,
                spot_angle_degrees: result.light.spot_angle_degrees,
                cast_shadows: result.light.cast_shadows != 0,
            },
        })
    }

    pub fn create_polyline_tube(
        &self,
        path_points: &[Vec3],
        radius: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let points: Vec<RawVec3> = path_points.iter().copied().map(Into::into).collect();
        let mut shape = 0;
        // SAFETY: The point slice and output remain valid for the duration of the call.
        self.check(unsafe {
            occt_bridge_create_polyline_tube(
                self.raw.as_ptr(),
                points.as_ptr(),
                points.len(),
                radius,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_loft(
        &self,
        sections: &[&[Vec3]],
        make_solid: bool,
        ruled: bool,
    ) -> Result<Shape<'_>, BridgeError> {
        self.loft_with(sections, make_solid, ruled, occt_bridge_create_loft)
    }

    /// Lofts through sections that are each one B-spline interpolated through
    /// their points and closed back to the first point: smooth except for a
    /// corner at the first point, such as an airfoil trailing edge. `ruled`
    /// keeps straight lines between sections.
    pub fn create_spline_loft(
        &self,
        sections: &[&[Vec3]],
        make_solid: bool,
        ruled: bool,
    ) -> Result<Shape<'_>, BridgeError> {
        self.loft_with(sections, make_solid, ruled, occt_bridge_create_spline_loft)
    }

    fn loft_with(
        &self,
        sections: &[&[Vec3]],
        make_solid: bool,
        ruled: bool,
        loft: unsafe extern "C" fn(
            *mut c_void,
            *const RawVec3,
            *const usize,
            usize,
            c_int,
            c_int,
            *mut RawShapeId,
        ) -> RawStatus,
    ) -> Result<Shape<'_>, BridgeError> {
        let counts: Vec<usize> = sections.iter().map(|section| section.len()).collect();
        let points: Vec<RawVec3> = sections
            .iter()
            .flat_map(|section| section.iter().copied())
            .map(Into::into)
            .collect();
        let mut shape = 0;
        // SAFETY: Flattened points, counts, and output remain valid for the call.
        self.check(unsafe {
            loft(
                self.raw.as_ptr(),
                points.as_ptr(),
                counts.as_ptr(),
                counts.len(),
                i32::from(make_solid),
                i32::from(ruled),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    /// Sews faces and shells whose edges lie within `tolerance` into a shell,
    /// or a compound of shells and free faces when not everything joins.
    /// Records modified and deleted history for the inputs.
    pub fn sew(&self, shapes: &[&Shape<'_>], tolerance: f64) -> Result<Shape<'_>, BridgeError> {
        for shape in shapes {
            self.validate_shape(shape)?;
        }
        let ids: Vec<RawShapeId> = shapes.iter().map(|shape| shape.id).collect();
        let mut sewn = 0;
        // SAFETY: The ID slice and output remain valid for the call.
        self.check(unsafe {
            occt_bridge_sew(
                self.raw.as_ptr(),
                ids.as_ptr(),
                ids.len(),
                tolerance,
                &mut sewn,
            )
        })?;
        Ok(self.shape(sewn))
    }

    /// Builds an outward-oriented solid from a shape with exactly one closed shell.
    pub fn make_solid<'a>(&'a self, shell: &Shape<'_>) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shell, |out| unsafe {
            occt_bridge_make_solid(self.raw.as_ptr(), shell.id, out)
        })
    }

    /// Builds one solid from the largest outer boundary and any enclosed void
    /// boundaries found in `shells`.
    pub fn make_solid_from_shells(&self, shells: &[&Shape<'_>]) -> Result<Shape<'_>, BridgeError> {
        for shell in shells {
            self.validate_shape(shell)?;
        }
        let ids: Vec<RawShapeId> = shells.iter().map(|shell| shell.id).collect();
        let mut solid = 0;
        // SAFETY: The ID slice and output remain valid for the duration of the call.
        self.check(unsafe {
            occt_bridge_make_solid_from_shells(
                self.raw.as_ptr(),
                ids.as_ptr(),
                ids.len(),
                &mut solid,
            )
        })?;
        Ok(self.shape(solid))
    }

    pub fn create_compound(&self, shapes: &[&Shape<'_>]) -> Result<Shape<'_>, BridgeError> {
        for shape in shapes {
            self.validate_shape(shape)?;
        }
        let ids: Vec<RawShapeId> = shapes.iter().map(|shape| shape.id).collect();
        let mut compound = 0;
        // SAFETY: The ID slice and output remain valid for the call.
        self.check(unsafe {
            occt_bridge_create_compound(self.raw.as_ptr(), ids.as_ptr(), ids.len(), &mut compound)
        })?;
        Ok(self.shape(compound))
    }
}
