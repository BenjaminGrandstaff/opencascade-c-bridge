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
    /// Sweeps a wire or single-boundary face along an edge or wire path, from
    /// where the profile is placed relative to the path's start. A face or
    /// closed wire makes a solid; an open wire a swept surface. Records the
    /// faces each profile edge generates.
    pub fn sweep(
        &self,
        profile: &Shape<'_>,
        path: &Shape<'_>,
        orientation: SweepOrientation,
    ) -> Result<Shape<'_>, BridgeError> {
        self.validate_shape(profile)?;
        self.validate_shape(path)?;
        let (mode, binormal) = match orientation {
            SweepOrientation::CorrectedFrenet => (0, Vec3::new(0.0, 0.0, 0.0)),
            SweepOrientation::Frenet => (1, Vec3::new(0.0, 0.0, 0.0)),
            SweepOrientation::Binormal(direction) => (2, direction),
            SweepOrientation::Fixed => (3, Vec3::new(0.0, 0.0, 0.0)),
        };
        let mut shape = 0;
        // SAFETY: Validated shapes, a by-value vector, and a writable output.
        self.check(unsafe {
            occt_bridge_sweep(
                self.raw.as_ptr(),
                profile.id,
                path.id,
                mode,
                binormal.into(),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    /// Builds a wire from lines, arcs, and interpolated splines. Consecutive
    /// segments must meet; `closed` requires the wire to end where it starts.
    pub fn create_curve_wire(
        &self,
        segments: &[CurveSegment],
        closed: bool,
    ) -> Result<Shape<'_>, BridgeError> {
        let zero = Vec3::new(0.0, 0.0, 0.0);
        let mut points: Vec<RawVec3> = Vec::new();
        let mut raw = Vec::with_capacity(segments.len());
        for segment in segments {
            let first_point = points.len();
            let (kind, flags, start_tangent, end_tangent) = match segment {
                CurveSegment::Line { start, end } => {
                    points.extend([RawVec3::from(*start), RawVec3::from(*end)]);
                    (0, 0, zero, zero)
                }
                CurveSegment::Arc { start, middle, end } => {
                    points.extend([*start, *middle, *end].map(RawVec3::from));
                    (1, 0, zero, zero)
                }
                CurveSegment::Spline {
                    points: through,
                    start_tangent,
                    end_tangent,
                    periodic,
                } => {
                    points.extend(through.iter().map(|point| RawVec3::from(*point)));
                    let flags = i32::from(start_tangent.is_some())
                        | (i32::from(end_tangent.is_some()) << 1)
                        | (i32::from(*periodic) << 2);
                    (
                        2,
                        flags,
                        start_tangent.unwrap_or(zero),
                        end_tangent.unwrap_or(zero),
                    )
                }
            };
            raw.push(RawCurveSegment {
                kind,
                flags,
                first_point,
                point_count: points.len() - first_point,
                start_tangent: start_tangent.into(),
                end_tangent: end_tangent.into(),
            });
        }
        let mut shape = 0;
        // SAFETY: The point and segment buffers, session, and output remain
        // valid during the call; segment ranges index into `points`.
        self.check(unsafe {
            occt_bridge_create_curve_wire(
                self.raw.as_ptr(),
                points.as_ptr(),
                points.len(),
                raw.as_ptr(),
                raw.len(),
                i32::from(closed),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

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

    /// An exact helical wire (a line on its cylinder) with a 3D curve within
    /// kernel tolerance. Sweep a profile placed at its start along it with
    /// `SweepOrientation::Binormal(axis)` for springs and coils.
    pub fn create_helix_wire(&self, helix: HelixOptions) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; values are passed by value.
        self.check(unsafe {
            occt_bridge_create_helix_wire(
                self.raw.as_ptr(),
                helix.origin.into(),
                helix.axis.into(),
                helix.start_direction.into(),
                helix.radius,
                helix.pitch,
                helix.turns,
                i32::from(helix.left_handed),
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

    /// Terminates a planar profile prism at a complete finite limiting face.
    /// Search travel must extend beyond that face. Partial or zero-depth limits fail.
    pub fn create_prism_until_face<'a>(
        &'a self,
        profile: &Shape<'_>,
        direction: Vec3,
        limiting_face: &Shape<'_>,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(limiting_face)?;
        self.derived_shape(profile, |out| unsafe {
            occt_bridge_create_prism_until_face(
                self.raw.as_ptr(),
                profile.id,
                direction.into(),
                limiting_face.id,
                out,
            )
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

    /// Native loft between immutable closed planar wires, without sampling.
    /// Compatibility may align or split copies of section edges.
    pub fn create_loft_from_wires<'a>(
        &'a self,
        sections: &[&Shape<'_>],
        make_solid: bool,
        ruled: bool,
    ) -> Result<Shape<'a>, BridgeError> {
        for section in sections {
            self.validate_shape(section)?;
        }
        let ids = sections.iter().map(|s| s.id).collect::<Vec<_>>();
        let mut result = 0;
        // SAFETY: All IDs belong to this session; the slice and output are valid.
        self.check(unsafe {
            occt_bridge_create_loft_from_wires(
                self.raw.as_ptr(),
                ids.as_ptr(),
                ids.len(),
                make_solid.into(),
                ruled.into(),
                &mut result,
            )
        })?;
        Ok(self.shape(result))
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

impl Session {
    /// Ellipse with an explicit major-axis direction in its plane.
    pub fn create_ellipse_wire_axes(
        &self,
        center: Vec3,
        normal: Vec3,
        major_axis: Vec3,
        major: f64,
        minor: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: Valid session, by-value inputs and a writable output handle.
        self.check(unsafe {
            occt_bridge_create_ellipse_wire_axes(
                self.raw.as_ptr(),
                center.into(),
                normal.into(),
                major_axis.into(),
                major,
                minor,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }
    pub fn trim_curve<'a>(
        &'a self,
        shape: &Shape<'_>,
        first: f64,
        last: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shape, |out| unsafe {
            occt_bridge_trim_curve(self.raw.as_ptr(), shape.id, first, last, out)
        })
    }
    pub fn extend_curve<'a>(
        &'a self,
        shape: &Shape<'_>,
        start: f64,
        end: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shape, |out| unsafe {
            occt_bridge_extend_curve(self.raw.as_ptr(), shape.id, start, end, out)
        })
    }
    pub fn offset_wire<'a>(
        &'a self,
        shape: &Shape<'_>,
        normal: Vec3,
        distance: f64,
        intersection: bool,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shape, |out| unsafe {
            occt_bridge_offset_wire(
                self.raw.as_ptr(),
                shape.id,
                normal.into(),
                distance,
                i32::from(intersection),
                out,
            )
        })
    }
    pub fn join_wires<'a>(
        &'a self,
        wires: &[&Shape<'_>],
        closed: bool,
    ) -> Result<Shape<'a>, BridgeError> {
        for wire in wires {
            self.validate_shape(wire)?;
        }
        let ids = wires.iter().map(|s| s.id).collect::<Vec<_>>();
        let mut shape = 0;
        // SAFETY: Validated handles and both buffers remain alive for the call.
        self.check(unsafe {
            occt_bridge_join_wires(
                self.raw.as_ptr(),
                ids.as_ptr(),
                ids.len(),
                i32::from(closed),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }
    pub fn curve_closest_point(&self, shape: &Shape<'_>, point: Vec3) -> Result<Vec3, BridgeError> {
        self.validate_shape(shape)?;
        let mut out = RawVec3::default();
        // SAFETY: Validated handle, by-value point and writable output vector.
        self.check(unsafe {
            occt_bridge_curve_closest_point(self.raw.as_ptr(), shape.id, point.into(), &mut out)
        })?;
        Ok(out.into())
    }
    pub fn wire_is_closed(&self, shape: &Shape<'_>) -> Result<bool, BridgeError> {
        self.validate_shape(shape)?;
        let mut out = 0;
        // SAFETY: Validated handle and writable output flag.
        self.check(unsafe { occt_bridge_wire_is_closed(self.raw.as_ptr(), shape.id, &mut out) })?;
        Ok(out != 0)
    }
}
