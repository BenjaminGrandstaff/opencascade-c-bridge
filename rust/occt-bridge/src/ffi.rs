//! Raw declarations of the C ABI.

use super::*;

pub(crate) const ABI_VERSION: u32 = 27;

#[repr(C)]
pub(crate) struct RawVec3 {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) z: f64,
}

#[repr(C)]
pub(crate) struct RawWireSegment {
    pub(crate) kind: c_int,
    pub(crate) start: RawVec3,
    pub(crate) middle: RawVec3,
    pub(crate) end: RawVec3,
}

#[repr(C)]
pub(crate) struct RawBounds {
    pub(crate) min: RawVec3,
    pub(crate) max: RawVec3,
}

#[repr(C)]
pub(crate) struct RawLightDesc {
    pub(crate) light_type: c_int,
    pub(crate) position: RawVec3,
    pub(crate) direction: RawVec3,
    pub(crate) color: RawVec3,
    pub(crate) intensity: f64,
    pub(crate) range: f64,
    pub(crate) spot_angle_degrees: f64,
    pub(crate) cast_shadows: c_int,
}

#[repr(C)]
pub(crate) struct RawWallTorchResult {
    pub(crate) fixture_shape: RawShapeId,
    pub(crate) flame_shape: RawShapeId,
    pub(crate) light: RawLightDesc,
}

pub(crate) type RawShapeId = u64;

pub(crate) type RawStatus = c_int;

pub(crate) const OK: RawStatus = 0;

#[link(name = "occt_bridge")]
unsafe extern "C" {
    pub(crate) fn occt_bridge_abi_version() -> u32;
    pub(crate) fn occt_bridge_status_string(status: RawStatus) -> *const c_char;
    pub(crate) fn occt_bridge_session_create(version: u32, out: *mut *mut c_void) -> RawStatus;
    pub(crate) fn occt_bridge_session_destroy(session: *mut c_void);
    pub(crate) fn occt_bridge_session_clear(session: *mut c_void) -> RawStatus;
    pub(crate) fn occt_bridge_session_shape_count(
        session: *mut c_void,
        out: *mut usize,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_session_last_error(
        session: *const c_void,
        buffer: *mut c_char,
        capacity: usize,
    ) -> usize;
    pub(crate) fn occt_bridge_session_last_warnings(
        session: *const c_void,
        buffer: *mut c_char,
        capacity: usize,
    ) -> usize;
    pub(crate) fn occt_bridge_session_diagnostic_count(
        session: *mut c_void,
        out: *mut usize,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_session_diagnostic_at(
        session: *mut c_void,
        index: usize,
        out_diagnostic: *mut RawDiagnostic,
        out_shape: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_session_diagnostic_name(
        session: *const c_void,
        index: usize,
        buffer: *mut c_char,
        capacity: usize,
    ) -> usize;
    pub(crate) fn occt_bridge_session_get_options(
        session: *mut c_void,
        out: *mut RawSessionOptions,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_session_set_options(
        session: *mut c_void,
        options: *const RawSessionOptions,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_box(
        session: *mut c_void,
        origin: RawVec3,
        size: RawVec3,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_cylinder(
        session: *mut c_void,
        origin: RawVec3,
        axis: RawVec3,
        radius: f64,
        height: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_cone(
        session: *mut c_void,
        origin: RawVec3,
        axis: RawVec3,
        base_radius: f64,
        top_radius: f64,
        height: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_sphere(
        session: *mut c_void,
        center: RawVec3,
        radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_polyline_wire(
        session: *mut c_void,
        points: *const RawVec3,
        point_count: usize,
        closed: c_int,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_segment_wire(
        session: *mut c_void,
        segments: *const RawWireSegment,
        count: usize,
        closed: c_int,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_circle_wire(
        session: *mut c_void,
        center: RawVec3,
        normal: RawVec3,
        radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_ellipse_wire(
        session: *mut c_void,
        center: RawVec3,
        normal: RawVec3,
        major_radius: f64,
        minor_radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_face_from_wire(
        session: *mut c_void,
        wire: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_prism_from_face(
        session: *mut c_void,
        face: RawShapeId,
        direction: RawVec3,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_revolve_from_face(
        session: *mut c_void,
        face: RawShapeId,
        origin: RawVec3,
        axis: RawVec3,
        angle_radians: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_polygon_prism(
        session: *mut c_void,
        points: *const RawVec3,
        point_count: usize,
        direction: RawVec3,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_faceted_stone(
        session: *mut c_void,
        bottom_points: *const RawVec3,
        top_points: *const RawVec3,
        point_count: usize,
        top_center: RawVec3,
        bottom_chamfer: f64,
        top_fillet: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_wall_torch(
        session: *mut c_void,
        wall_anchor: RawVec3,
        wall_normal: RawVec3,
        scale: f64,
        out: *mut RawWallTorchResult,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_polyline_tube(
        session: *mut c_void,
        path_points: *const RawVec3,
        point_count: usize,
        radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_loft(
        session: *mut c_void,
        points: *const RawVec3,
        section_point_counts: *const usize,
        section_count: usize,
        make_solid: c_int,
        ruled: c_int,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_sew(
        session: *mut c_void,
        shapes: *const RawShapeId,
        shape_count: usize,
        tolerance: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_make_solid(
        session: *mut c_void,
        shell: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_make_solid_from_shells(
        session: *mut c_void,
        shells: *const RawShapeId,
        shell_count: usize,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_compound(
        session: *mut c_void,
        shapes: *const RawShapeId,
        shape_count: usize,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_fuse(
        session: *mut c_void,
        left: RawShapeId,
        right: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_cut(
        session: *mut c_void,
        object: RawShapeId,
        tool: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_common(
        session: *mut c_void,
        left: RawShapeId,
        right: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_fillet(
        session: *mut c_void,
        shape: RawShapeId,
        edges: *const RawShapeId,
        edge_count: usize,
        radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_chamfer(
        session: *mut c_void,
        shape: RawShapeId,
        edges: *const RawShapeId,
        edge_count: usize,
        distance: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_offset(
        session: *mut c_void,
        shape: RawShapeId,
        offset: f64,
        tolerance: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_hollow(
        session: *mut c_void,
        shape: RawShapeId,
        faces_to_remove: *const RawShapeId,
        face_count: usize,
        thickness: f64,
        tolerance: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_translate(
        session: *mut c_void,
        shape: RawShapeId,
        offset: RawVec3,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_rotate(
        session: *mut c_void,
        shape: RawShapeId,
        axis_origin: RawVec3,
        axis_direction: RawVec3,
        angle_radians: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_scale(
        session: *mut c_void,
        shape: RawShapeId,
        center: RawVec3,
        factor: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_bounds(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut RawBounds,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_exact_bounds(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut RawBounds,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_duplicate(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_type(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_subshape_count(
        session: *mut c_void,
        shape: RawShapeId,
        subshape_type: c_int,
        out: *mut usize,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_subshape_at(
        session: *mut c_void,
        shape: RawShapeId,
        subshape_type: c_int,
        index: usize,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_surface_area(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut f64,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_volume(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut f64,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_center_of_mass(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut RawVec3,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_face_normal(
        session: *mut c_void,
        face: RawShapeId,
        out: *mut RawVec3,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_face_is_planar(
        session: *mut c_void,
        face: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_edge_length(
        session: *mut c_void,
        edge: RawShapeId,
        out: *mut f64,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_edge_circle_radius(
        session: *mut c_void,
        edge: RawShapeId,
        out_is_circle: *mut c_int,
        out_radius: *mut f64,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_edge_curvature(
        session: *mut c_void,
        edge: RawShapeId,
        out_is_defined: *mut c_int,
        out_curvature: *mut f64,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_edge_curvature_range(
        session: *mut c_void,
        edge: RawShapeId,
        sample_count: usize,
        out_minimum_curvature: *mut f64,
        out_maximum_curvature: *mut f64,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_edge_curvature_extrema(
        session: *mut c_void,
        edge: RawShapeId,
        relative_tolerance: f64,
        out_minimum: *mut f64,
        out_minimum_lower_bound: *mut f64,
        out_maximum: *mut f64,
        out_maximum_upper_bound: *mut f64,
        out_is_exact: *mut c_int,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_is_adjacent(
        session: *mut c_void,
        parent: RawShapeId,
        first: RawShapeId,
        second: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_is_same(
        session: *mut c_void,
        first: RawShapeId,
        second: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_faces_are_tangent(
        session: *mut c_void,
        parent: RawShapeId,
        first_face: RawShapeId,
        second_face: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_history_count(
        session: *mut c_void,
        result: RawShapeId,
        source: RawShapeId,
        relation: c_int,
        out: *mut usize,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_history_at(
        session: *mut c_void,
        result: RawShapeId,
        source: RawShapeId,
        relation: c_int,
        index: usize,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_history_is_deleted(
        session: *mut c_void,
        result: RawShapeId,
        source: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_is_valid(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_remove(session: *mut c_void, shape: RawShapeId) -> RawStatus;
    pub(crate) fn occt_bridge_shape_release(session: *mut c_void, shape: RawShapeId);
    pub(crate) fn occt_bridge_brep_save(
        session: *mut c_void,
        shape: RawShapeId,
        path: *const c_char,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_brep_load(
        session: *mut c_void,
        path: *const c_char,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_step_save(
        session: *mut c_void,
        shape: RawShapeId,
        path: *const c_char,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_step_load(
        session: *mut c_void,
        path: *const c_char,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_stl_save(
        session: *mut c_void,
        shape: RawShapeId,
        path: *const c_char,
        linear_deflection: f64,
        angular_deflection_radians: f64,
        binary: c_int,
    ) -> RawStatus;
}
