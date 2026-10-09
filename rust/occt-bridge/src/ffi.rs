//! Raw declarations of the C ABI.

use super::*;

pub(crate) const ABI_VERSION: u32 = 51;

#[repr(C)]
#[derive(Default)]
pub(crate) struct RawBezierPole {
    pub(crate) span_index: usize,
    pub(crate) point: RawVec3,
    pub(crate) weight: f64,
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct RawAnalyticCurve {
    pub(crate) kind: c_int,
    pub(crate) origin: RawVec3,
    pub(crate) x_vector: RawVec3,
    pub(crate) y_vector: RawVec3,
    pub(crate) first: f64,
    pub(crate) last: f64,
}

#[repr(C)]
pub(crate) struct RawMeshOptions {
    pub(crate) linear_deflection: f64,
    pub(crate) angular_deflection: f64,
    pub(crate) maximum_triangles: usize,
}

#[repr(C)]
pub(crate) struct RawMeshTriangle {
    pub(crate) face_index: usize,
    pub(crate) points: [RawVec3; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct RawVec3 {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) z: f64,
}

#[repr(C)]
pub(crate) struct RawFilletStation {
    pub(crate) position: f64,
    pub(crate) radius: f64,
}

#[repr(C)]
pub(crate) struct RawWireSegment {
    pub(crate) kind: c_int,
    pub(crate) start: RawVec3,
    pub(crate) middle: RawVec3,
    pub(crate) end: RawVec3,
}

#[repr(C)]
pub(crate) struct RawCurveSegment {
    pub(crate) kind: i32,
    pub(crate) flags: i32,
    pub(crate) first_point: usize,
    pub(crate) point_count: usize,
    pub(crate) start_tangent: RawVec3,
    pub(crate) end_tangent: RawVec3,
}

#[repr(C)]
pub(crate) struct RawStepComponent {
    pub(crate) shape: RawShapeId,
    pub(crate) name: *const std::os::raw::c_char,
    pub(crate) part_name: *const std::os::raw::c_char,
    pub(crate) has_color: i32,
    pub(crate) color: [f64; 3],
}

#[repr(C)]
pub(crate) struct RawStepFaceColor {
    pub(crate) component: usize,
    pub(crate) face: usize,
    pub(crate) color: [f64; 3],
}

#[repr(C)]
pub(crate) struct RawStepNode {
    pub(crate) name: *const std::os::raw::c_char,
    pub(crate) parent: usize,
    pub(crate) transform: [f64; 12],
}

#[repr(C)]
pub(crate) struct RawMassProperties {
    pub(crate) volume: f64,
    pub(crate) center: RawVec3,
    pub(crate) inertia: [f64; 9],
    pub(crate) relative_volume_error: f64,
}

#[repr(C)]
pub(crate) struct RawDistanceResult {
    pub(crate) distance: f64,
    pub(crate) first: RawVec3,
    pub(crate) second: RawVec3,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawFaceRadiusBounds {
    pub(crate) convex_radius: f64,
    pub(crate) concave_radius: f64,
    pub(crate) convex_point: RawVec3,
    pub(crate) concave_point: RawVec3,
    pub(crate) exact: i32,
    pub(crate) samples: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawFacePullRange {
    pub(crate) minimum: f64,
    pub(crate) maximum: f64,
    pub(crate) minimum_point: RawVec3,
    pub(crate) maximum_point: RawVec3,
    pub(crate) exact: i32,
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
    pub fn occt_bridge_create_ellipse_wire_axes(
        s: *mut c_void,
        center: RawVec3,
        normal: RawVec3,
        major_axis: RawVec3,
        major: f64,
        minor: f64,
        out: *mut u64,
    ) -> i32;
    pub fn occt_bridge_trim_curve(
        s: *mut c_void,
        shape: u64,
        first: f64,
        last: f64,
        out: *mut u64,
    ) -> i32;
    pub fn occt_bridge_extend_curve(
        s: *mut c_void,
        shape: u64,
        start: f64,
        end: f64,
        out: *mut u64,
    ) -> i32;
    pub fn occt_bridge_join_wires(
        s: *mut c_void,
        shapes: *const u64,
        count: usize,
        closed: i32,
        out: *mut u64,
    ) -> i32;
    pub fn occt_bridge_offset_wire(
        s: *mut c_void,
        shape: u64,
        normal: RawVec3,
        distance: f64,
        join: i32,
        out: *mut u64,
    ) -> i32;
    pub fn occt_bridge_curve_closest_point(
        s: *mut c_void,
        shape: u64,
        point: RawVec3,
        out: *mut RawVec3,
    ) -> i32;
    pub fn occt_bridge_wire_is_closed(s: *mut c_void, shape: u64, out: *mut i32) -> i32;

    pub(crate) fn occt_bridge_subshape_indices(
        session: *mut c_void,
        shape: u64,
        kind: c_int,
        candidates: *const u64,
        count: usize,
        indices: *mut usize,
    ) -> c_int;
    pub(crate) fn occt_bridge_surface_mesh(
        session: *mut c_void,
        shape: u64,
        options: RawMeshOptions,
        triangles: *mut RawMeshTriangle,
        capacity: usize,
        count: *mut usize,
    ) -> c_int;
    pub(crate) fn occt_bridge_shape_subshapes(
        session: *mut c_void,
        shape: RawShapeId,
        kind: c_int,
        out: *mut RawShapeId,
        capacity: usize,
        count: *mut usize,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_subshape_lookup(
        session: *mut c_void,
        shape: u64,
        kind: c_int,
        candidates: *const u64,
        count: usize,
        indices: *mut usize,
    ) -> c_int;
    pub(crate) fn occt_bridge_shape_face_radius_bounds(
        session: *mut c_void,
        shape: RawShapeId,
        samples_per_direction: u32,
        out: *mut RawFaceRadiusBounds,
        capacity: usize,
        count: *mut usize,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_face_pull_ranges(
        session: *mut c_void,
        shape: RawShapeId,
        pull_direction: RawVec3,
        out: *mut RawFacePullRange,
        capacity: usize,
        count: *mut usize,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_edge_concavities(
        session: *mut c_void,
        shape: RawShapeId,
        tangency_radians: f64,
        out: *mut i32,
        capacity: usize,
        count: *mut usize,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_clip_by_plane(
        session: *mut c_void,
        shape: RawShapeId,
        origin: RawVec3,
        normal: RawVec3,
        keep_positive: c_int,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_orthographic_projection(
        session: *mut c_void,
        shape: RawShapeId,
        origin: RawVec3,
        direction: RawVec3,
        x_axis: RawVec3,
        visible: *mut RawShapeId,
        hidden: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_edge_bezier_poles(
        session: *mut c_void,
        edge: RawShapeId,
        maximum_poles: usize,
        poles: *mut RawBezierPole,
        capacity: usize,
        count: *mut usize,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_edge_analytic_curve(
        session: *mut c_void,
        edge: RawShapeId,
        curve: *mut RawAnalyticCurve,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_edge_sample_points(
        session: *mut c_void,
        edge: RawShapeId,
        count: usize,
        points: *mut RawVec3,
    ) -> RawStatus;
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
    pub(crate) fn occt_bridge_create_curve_wire(
        session: *mut c_void,
        points: *const RawVec3,
        point_count: usize,
        segments: *const RawCurveSegment,
        segment_count: usize,
        closed: c_int,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_step_save_assembly_tree(
        session: *mut c_void,
        path: *const std::os::raw::c_char,
        assembly_name: *const std::os::raw::c_char,
        nodes: *const RawStepNode,
        node_count: usize,
        components: *const RawStepComponent,
        component_nodes: *const usize,
        component_count: usize,
        face_colors: *const RawStepFaceColor,
        face_color_count: usize,
        out_part_count: *mut usize,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_sweep(
        session: *mut c_void,
        profile: RawShapeId,
        path: RawShapeId,
        orientation: i32,
        binormal: RawVec3,
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
    pub(crate) fn occt_bridge_create_prism_until_face(
        session: *mut c_void,
        profile: RawShapeId,
        direction: RawVec3,
        limiting_face: RawShapeId,
        out_shape: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_ray_first_hit(
        session: *mut c_void,
        shape: RawShapeId,
        origin: RawVec3,
        direction: RawVec3,
        maximum_length: f64,
        out_point: *mut RawVec3,
        out_length: *mut f64,
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
    pub(crate) fn occt_bridge_create_loft_from_wires(
        session: *mut c_void,
        sections: *const RawShapeId,
        section_count: usize,
        make_solid: c_int,
        ruled: c_int,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_spline_loft(
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
    pub(crate) fn occt_bridge_variable_fillet(
        session: *mut c_void,
        shape: RawShapeId,
        edges: *const RawShapeId,
        edge_count: usize,
        start_radius: f64,
        end_radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_variable_fillet_stations(
        session: *mut c_void,
        shape: RawShapeId,
        edges: *const RawShapeId,
        edge_count: usize,
        stations: *const RawFilletStation,
        station_count: usize,
        spine_direction: c_int,
        start_point: RawVec3,
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
    pub(crate) fn occt_bridge_unify_same_domain(
        session: *mut c_void,
        shape: RawShapeId,
        linear_tolerance: f64,
        angular_tolerance: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_offset(
        session: *mut c_void,
        shape: RawShapeId,
        offset: f64,
        tolerance: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_draft(
        session: *mut c_void,
        shape: RawShapeId,
        faces: *const RawShapeId,
        face_count: usize,
        neutral_origin: RawVec3,
        neutral_normal: RawVec3,
        pull_direction: RawVec3,
        angle_radians: f64,
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
    pub(crate) fn occt_bridge_mirror(
        session: *mut c_void,
        shape: RawShapeId,
        origin: RawVec3,
        normal: RawVec3,
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
    pub(crate) fn occt_bridge_shape_subshape_with_history(
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
    pub(crate) fn occt_bridge_shape_mass_properties(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut RawMassProperties,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_distance(
        session: *mut c_void,
        first: RawShapeId,
        second: RawShapeId,
        out: *mut RawDistanceResult,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_overlap_volume(
        session: *mut c_void,
        first: RawShapeId,
        second: RawShapeId,
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
    pub(crate) fn occt_bridge_shape_faces_are_tangent_within(
        session: *mut c_void,
        parent: RawShapeId,
        first_face: RawShapeId,
        second_face: RawShapeId,
        angular_tolerance: f64,
        out: *mut c_int,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_open_profile_face_to_next(
        session: *mut c_void,
        wire: RawShapeId,
        body: RawShapeId,
        direction: RawVec3,
        maximum_length: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_create_open_profile_face(
        session: *mut c_void,
        wire: RawShapeId,
        offset: RawVec3,
        out: *mut RawShapeId,
    ) -> RawStatus;
    pub(crate) fn occt_bridge_shape_compose_history(
        session: *mut c_void,
        result: RawShapeId,
        intermediate: RawShapeId,
        out_shape: *mut RawShapeId,
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
