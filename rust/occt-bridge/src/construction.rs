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

    /// Builds a wire from lines, arcs, interpolated splines and explicit B-splines.
    /// Consecutive
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
            let (mut degree, mut knot_count, mut weight_count) = (0, 0, 0);
            let (mut knots_ptr, mut weights_ptr) = (std::ptr::null(), std::ptr::null());
            let mut mults_ptr = std::ptr::null();
            let (kind, flags, start_tangent, end_tangent) = match segment {
                CurveSegment::Line { start, end } => {
                    points.extend([RawVec3::from(*start), RawVec3::from(*end)]);
                    (0, 0, zero, zero)
                }
                CurveSegment::Arc { start, middle, end } => {
                    points.extend([*start, *middle, *end].map(RawVec3::from));
                    (1, 0, zero, zero)
                }
                CurveSegment::EllipseArc {
                    center,
                    major,
                    minor,
                    start,
                    end,
                    clockwise,
                } => {
                    points.extend([*center, *major, *minor, *start, *end].map(RawVec3::from));
                    (4, i32::from(*clockwise), zero, zero)
                }
                CurveSegment::BSpline {
                    non_clamped,
                    periodic,
                    poles,
                    degree: d,
                    knots,
                    multiplicities,
                    weights,
                } => {
                    if knots.len() != multiplicities.len() {
                        return Err(BridgeError {
                            status: 1,
                            category: "invalid argument".into(),
                            message: "B-spline knot/multiplicity counts differ".into(),
                            diagnostics: Vec::new(),
                        });
                    }
                    points.extend(poles.iter().map(|p| RawVec3::from(*p)));
                    degree = *d;
                    knot_count = knots.len();
                    weight_count = weights.len();
                    knots_ptr = knots.as_ptr();
                    mults_ptr = multiplicities.as_ptr();
                    weights_ptr = weights.as_ptr();
                    (
                        3,
                        (i32::from(*periodic) << 2) | (i32::from(*non_clamped) << 3),
                        zero,
                        zero,
                    )
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
                degree,
                knot_count,
                knots: knots_ptr,
                multiplicities: mults_ptr,
                weight_count,
                weights: weights_ptr,
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

    /// Exact bounded cells of coplanar curves (ABI 67). Select all cells with
    /// empty seeds, or cells strictly containing the given world-space seeds.
    pub fn discover_planar_regions<'a>(
        &'a self,
        wires: &[&Shape<'_>],
        origin: Vec3,
        normal: Vec3,
        x_axis: Vec3,
        seeds: &[Vec3],
    ) -> Result<Shape<'a>, BridgeError> {
        for wire in wires {
            self.validate_shape(wire)?;
        }
        let ids: Vec<_> = wires.iter().map(|wire| wire.id).collect();
        let seeds: Vec<RawVec3> = seeds.iter().copied().map(Into::into).collect();
        let mut shape = 0;
        // SAFETY: Validated handles and live input/output buffers.
        self.check(unsafe {
            occt_bridge_discover_planar_regions(
                self.raw.as_ptr(),
                ids.as_ptr(),
                ids.len(),
                origin.into(),
                normal.into(),
                x_axis.into(),
                seeds.as_ptr(),
                seeds.len(),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_face_from_wire<'a>(&'a self, wire: &Shape<'_>) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(wire, |out| unsafe {
            occt_bridge_create_face_from_wire(self.raw.as_ptr(), wire.id, out)
        })
    }

    /// Planar face from a closed outer wire and closed coplanar inner wires,
    /// without booleans. Holes must lie strictly inside, apart from each other
    /// and the boundary by more than 1e-7 mm, and must not nest.
    pub fn create_face_with_holes<'a>(
        &'a self,
        outer: &Shape<'_>,
        inner: &[&Shape<'_>],
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(outer)?;
        for wire in inner {
            self.validate_shape(wire)?;
        }
        let ids = inner.iter().map(|s| s.id).collect::<Vec<_>>();
        let mut shape = 0;
        // SAFETY: Validated handles; the ID buffer outlives the call.
        self.check(unsafe {
            occt_bridge_create_face_with_holes(
                self.raw.as_ptr(),
                outer.id,
                ids.as_ptr(),
                ids.len(),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
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

    /// Reorder/reverse a closed planar boundary, preserving exact curves and ancestry.
    pub fn align_profile_wire<'a>(
        &'a self,
        input: &Shape<'_>,
        start: Option<Vec3>,
        reverse: bool,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(input)?;
        let start = start.map(RawVec3::from);
        let mut result = 0;
        // SAFETY: input is session-owned; the optional point and output live through the call.
        self.check(unsafe {
            occt_bridge_align_profile_wire(
                self.raw.as_ptr(),
                input.id,
                start.as_ref().map_or(std::ptr::null(), |p| p),
                reverse.into(),
                &mut result,
            )
        })?;
        Ok(self.shape(result))
    }

    /// Exact rational cubic Hermite loft; derivatives use millimetres per station interval.
    pub fn create_constrained_loft<'a>(
        &'a self,
        sections: &[&Shape<'_>],
        start: Option<Vec3>,
        end: Option<Vec3>,
    ) -> Result<Shape<'a>, BridgeError> {
        for section in sections {
            self.validate_shape(section)?;
        }
        let ids = sections.iter().map(|s| s.id).collect::<Vec<_>>();
        let start = start.map(RawVec3::from);
        let end = end.map(RawVec3::from);
        let mut result = 0;
        // SAFETY: all input handles are session-owned; optional vectors and slices live through the call.
        self.check(unsafe {
            occt_bridge_create_constrained_loft(
                self.raw.as_ptr(),
                ids.as_ptr(),
                ids.len(),
                start.as_ref().map_or(std::ptr::null(), |v| v),
                end.as_ref().map_or(std::ptr::null(), |v| v),
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

    /// Groups children without consuming them, retaining their identity and
    /// unioned operation histories. Indexed merge cost follows topology/relations.
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
    /// Round a remaining internal corner of an open or closed planar wire whose
    /// neighbors are lines, circular arcs, ellipses or splines.
    pub fn fillet_wire_corner<'a>(
        &'a self,
        shape: &Shape<'_>,
        corner: Vec3,
        radius: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shape, |out| unsafe {
            occt_bridge_fillet_wire_corner(self.raw.as_ptr(), shape.id, corner.into(), radius, out)
        })
    }
    /// Chamfer a remaining corner; distances follow incoming/outgoing wire traversal.
    pub fn chamfer_wire_corner<'a>(
        &'a self,
        shape: &Shape<'_>,
        corner: Vec3,
        incoming: f64,
        outgoing: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shape, |out| unsafe {
            occt_bridge_chamfer_wire_corner(
                self.raw.as_ptr(),
                shape.id,
                corner.into(),
                incoming,
                outgoing,
                out,
            )
        })
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
    /// Keeps the part of a one-edge curve from `from` to `to` along its
    /// traversal (ABI 64). Both points must be on the curve within 1e-6; a
    /// closed periodic curve wraps through its seam.
    pub fn trim_curve_between<'a>(
        &'a self,
        shape: &Shape<'_>,
        from: Vec3,
        to: Vec3,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shape, |out| unsafe {
            occt_bridge_trim_curve_between(self.raw.as_ptr(), shape.id, from.into(), to.into(), out)
        })
    }
    /// Curvature vector (curvature times principal normal; zero where
    /// straight) of a one-edge curve at its point nearest `point` (ABI 65).
    pub fn curve_curvature_at(&self, shape: &Shape<'_>, point: Vec3) -> Result<Vec3, BridgeError> {
        self.validate_shape(shape)?;
        let mut out = RawVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        // SAFETY: The session, validated shape and output pointer are valid.
        self.check(unsafe {
            occt_bridge_curve_curvature_at(self.raw.as_ptr(), shape.id, point.into(), &mut out)
        })?;
        Ok(out.into())
    }
    /// Path of the installed scalable font `family` in `style`, matched
    /// strictly through fontconfig (ABI 66).
    pub fn font_file(&self, family: &str, style: FontStyle) -> Result<String, BridgeError> {
        let family = std::ffi::CString::new(family).map_err(|_| BridgeError {
            status: 1,
            category: "invalid argument".into(),
            message: "font family contains NUL".into(),
            diagnostics: Vec::new(),
        })?;
        let mut length = 0;
        // SAFETY: Valid NUL-terminated family; a null buffer queries the length.
        self.check(unsafe {
            occt_bridge_font_file(
                self.raw.as_ptr(),
                family.as_ptr(),
                style as c_int,
                std::ptr::null_mut(),
                0,
                &mut length,
            )
        })?;
        let mut buffer = vec![0u8; length + 1];
        // SAFETY: The buffer holds the path and its NUL terminator.
        self.check(unsafe {
            occt_bridge_font_file(
                self.raw.as_ptr(),
                family.as_ptr(),
                style as c_int,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut length,
            )
        })?;
        buffer.truncate(length);
        Ok(String::from_utf8_lossy(&buffer).into_owned())
    }

    /// One line of text as a compound of planar glyph faces (ABI 66):
    /// exact outlines from `font_file` with em size `height`, the baseline
    /// along `x_axis` from `origin` with `y_axis` up.
    #[allow(clippy::too_many_arguments)]
    pub fn create_text(
        &self,
        text: &str,
        font_file: &str,
        height: f64,
        origin: Vec3,
        x_axis: Vec3,
        y_axis: Vec3,
        align: TextAlign,
    ) -> Result<Shape<'_>, BridgeError> {
        let path = std::ffi::CString::new(font_file).map_err(|_| BridgeError {
            status: 1,
            category: "invalid argument".into(),
            message: "font path contains NUL".into(),
            diagnostics: Vec::new(),
        })?;
        let mut shape = 0;
        // SAFETY: The text slice and path outlive the call; output is writable.
        self.check(unsafe {
            occt_bridge_create_text(
                self.raw.as_ptr(),
                text.as_ptr().cast(),
                text.len(),
                path.as_ptr(),
                height,
                origin.into(),
                x_axis.into(),
                y_axis.into(),
                align as c_int,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
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
    /// Isolated XY-plane crossings/touches of two finite edges/one-edge wires.
    /// Line–spline pairs use rational Bernstein roots with 1000000 root-work units.
    /// Rejects collinear contacts/overlaps, unresolved other tangent regions,
    /// coefficient/parameter uncertainty, degree above 25, over 10000 poles,
    /// over 10001 knots, over 10000 span pairs, and the 1–1000 point budget.
    /// Two bounded native queries; no stored shapes or source mutation.
    pub fn planar_curve_intersections(
        &self,
        first: &Shape<'_>,
        second: &Shape<'_>,
        maximum_points: usize,
    ) -> Result<Vec<Vec3>, BridgeError> {
        self.validate_shape(first)?;
        self.validate_shape(second)?;
        let mut count = 0;
        // SAFETY: Owned handles, valid count pointer; null/zero requests the count.
        self.check(unsafe {
            occt_bridge_planar_curve_intersections(
                self.raw.as_ptr(),
                first.id,
                second.id,
                maximum_points,
                std::ptr::null_mut(),
                0,
                &mut count,
            )
        })?;
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut points = vec![RawVec3::default(); count];
        // SAFETY: Output vector has the capacity returned by the count query.
        self.check(unsafe {
            occt_bridge_planar_curve_intersections(
                self.raw.as_ptr(),
                first.id,
                second.id,
                maximum_points,
                points.as_mut_ptr(),
                points.len(),
                &mut count,
            )
        })?;
        points.truncate(count);
        Ok(points.into_iter().map(Into::into).collect())
    }
    /// Bounded native closest point and oriented first derivative. Requires
    /// one edge or a one-edge wire; zero tangents and C0 knot contacts fail.
    pub fn curve_closest_data(
        &self,
        shape: &Shape<'_>,
        point: Vec3,
    ) -> Result<(Vec3, Vec3), BridgeError> {
        self.validate_shape(shape)?;
        let (mut closest, mut tangent) = (RawVec3::default(), RawVec3::default());
        // SAFETY: Validated session/handle, by-value query and writable outputs.
        self.check(unsafe {
            occt_bridge_curve_closest_data(
                self.raw.as_ptr(),
                shape.id,
                point.into(),
                &mut closest,
                &mut tangent,
            )
        })?;
        Ok((closest.into(), tangent.into()))
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
    /// Unique closest finite point and its oriented active-domain fraction.
    /// Includes endpoints, canonicalizes a closed seam to zero and rejects
    /// equally near distinct parameters. One edge; at most 1000 native extrema.
    pub fn curve_closest_fraction(
        &self,
        shape: &Shape<'_>,
        hint: Vec3,
    ) -> Result<(f64, Vec3), BridgeError> {
        self.validate_shape(shape)?;
        let mut fraction = 0.;
        let mut point = RawVec3::default();
        // SAFETY: Valid session-owned shape and distinct writable outputs.
        self.check(unsafe {
            occt_bridge_curve_closest_fraction(
                self.raw.as_ptr(),
                shape.id,
                hint.into(),
                &mut fraction,
                &mut point,
            )
        })?;
        Ok((fraction, point.into()))
    }
    /// Exact point at a normalized active parameter, following edge orientation.
    /// Requires one edge; the fraction is not normalized arc length.
    pub fn curve_point_at_fraction(
        &self,
        shape: &Shape<'_>,
        fraction: f64,
    ) -> Result<Vec3, BridgeError> {
        self.validate_shape(shape)?;
        let mut out = RawVec3::default();
        // SAFETY: Validated session/shape and writable output vector.
        self.check(unsafe {
            occt_bridge_curve_point_at_fraction(self.raw.as_ptr(), shape.id, fraction, &mut out)
        })?;
        Ok(out.into())
    }
    /// Located native active endpoints and unnormalized first derivatives,
    /// following edge orientation. Requires one edge or a one-edge wire.
    pub fn curve_end_data(
        &self,
        shape: &Shape<'_>,
    ) -> Result<(Vec3, Vec3, Vec3, Vec3), BridgeError> {
        self.validate_shape(shape)?;
        let (mut a, mut b, mut u, mut v) = (
            RawVec3::default(),
            RawVec3::default(),
            RawVec3::default(),
            RawVec3::default(),
        );
        // SAFETY: Validated session/handle and four writable output vectors.
        self.check(unsafe {
            occt_bridge_curve_end_data(self.raw.as_ptr(), shape.id, &mut a, &mut b, &mut u, &mut v)
        })?;
        Ok((a.into(), b.into(), u.into(), v.into()))
    }
    pub fn wire_is_closed(&self, shape: &Shape<'_>) -> Result<bool, BridgeError> {
        self.validate_shape(shape)?;
        let mut out = 0;
        // SAFETY: Validated handle and writable output flag.
        self.check(unsafe { occt_bridge_wire_is_closed(self.raw.as_ptr(), shape.id, &mut out) })?;
        Ok(out != 0)
    }
}
