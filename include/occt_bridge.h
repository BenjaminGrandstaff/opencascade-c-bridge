#ifndef OCCT_BRIDGE_H
#define OCCT_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#define OCCT_BRIDGE_ABI_VERSION 23u
#define OCCT_BRIDGE_INVALID_SHAPE_ID UINT64_C(0)

#if defined(_WIN32) && defined(OCCT_BRIDGE_BUILD_SHARED)
#define OCCT_BRIDGE_API __declspec(dllexport)
#elif defined(_WIN32)
#define OCCT_BRIDGE_API __declspec(dllimport)
#elif defined(__GNUC__) || defined(__clang__)
#define OCCT_BRIDGE_API __attribute__((visibility("default")))
#else
#define OCCT_BRIDGE_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct occt_bridge_session occt_bridge_session_t;
typedef uint64_t occt_bridge_shape_id_t;

typedef int32_t occt_bridge_shape_type_t;
enum {
    OCCT_BRIDGE_SHAPE_COMPOUND = 1,
    OCCT_BRIDGE_SHAPE_COMPSOLID = 2,
    OCCT_BRIDGE_SHAPE_SOLID = 3,
    OCCT_BRIDGE_SHAPE_SHELL = 4,
    OCCT_BRIDGE_SHAPE_FACE = 5,
    OCCT_BRIDGE_SHAPE_WIRE = 6,
    OCCT_BRIDGE_SHAPE_EDGE = 7,
    OCCT_BRIDGE_SHAPE_VERTEX = 8
};

typedef int32_t occt_bridge_history_relation_t;
enum {
    OCCT_BRIDGE_HISTORY_GENERATED = 1,
    OCCT_BRIDGE_HISTORY_MODIFIED = 2
};

typedef struct occt_bridge_vec3 {
    double x;
    double y;
    double z;
} occt_bridge_vec3_t;

typedef struct occt_bridge_bounds {
    occt_bridge_vec3_t min;
    occt_bridge_vec3_t max;
} occt_bridge_bounds_t;

typedef int32_t occt_bridge_light_type_t;
enum {
    OCCT_BRIDGE_LIGHT_AMBIENT = 0,
    OCCT_BRIDGE_LIGHT_DIRECTIONAL = 1,
    OCCT_BRIDGE_LIGHT_POSITIONAL = 2,
    OCCT_BRIDGE_LIGHT_SPOTLIGHT = 3
};

typedef struct occt_bridge_light_desc {
    occt_bridge_light_type_t type;
    occt_bridge_vec3_t position;
    occt_bridge_vec3_t direction;
    occt_bridge_vec3_t color;
    double intensity;
    double range;
    double spot_angle_degrees;
    int cast_shadows;
} occt_bridge_light_desc_t;

typedef struct occt_bridge_wall_torch_result {
    occt_bridge_shape_id_t fixture_shape;
    occt_bridge_shape_id_t flame_shape;
    occt_bridge_light_desc_t light;
} occt_bridge_wall_torch_result_t;

typedef int32_t occt_bridge_status_t;
enum {
    OCCT_BRIDGE_OK = 0,
    OCCT_BRIDGE_INVALID_ARGUMENT = 1,
    OCCT_BRIDGE_UNSUPPORTED_ABI = 2,
    OCCT_BRIDGE_SHAPE_NOT_FOUND = 3,
    OCCT_BRIDGE_INVALID_GEOMETRY = 4,
    OCCT_BRIDGE_IO_ERROR = 5,
    OCCT_BRIDGE_KERNEL_ERROR = 6,
    OCCT_BRIDGE_ALLOCATION_FAILED = 7,
    OCCT_BRIDGE_INTERNAL_ERROR = 8
};

/*
 * Per-session behavior for kernel results. Defaults: validate_results = 1,
 * heal_invalid_results = 0, boolean_fuzzy_tolerance = 0.0.
 *
 * validate_results: check results of booleans, fillets, chamfers, offsets,
 *   hollowing, sewing, and STEP and BREP import with BRepCheck; an invalid
 *   result returns OCCT_BRIDGE_INVALID_GEOMETRY instead of being stored.
 * heal_invalid_results: repair an invalid result with shape fixing and store
 *   it when the repair is valid, recording a warning; requires validation.
 *   Operation history is carried through the repair.
 * boolean_fuzzy_tolerance: treat boolean input faces and edges closer than
 *   this distance as coincident; 0 performs exact booleans.
 */
typedef struct occt_bridge_session_options {
    int validate_results;
    int heal_invalid_results;
    double boolean_fuzzy_tolerance;
} occt_bridge_session_options_t;

/* ABI and diagnostics. No function allows a C++ exception to cross this boundary. */
OCCT_BRIDGE_API uint32_t occt_bridge_abi_version(void);
OCCT_BRIDGE_API const char* occt_bridge_status_string(occt_bridge_status_t status);

/* Sessions own every shape handle created within them. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_session_create(
    uint32_t requested_abi_version,
    occt_bridge_session_t** out_session
);
OCCT_BRIDGE_API void occt_bridge_session_destroy(occt_bridge_session_t* session);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_session_clear(
    occt_bridge_session_t* session
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_session_shape_count(
    occt_bridge_session_t* session,
    size_t* out_count
);

/* Returns the required byte count including NUL. A NULL/zero buffer is a size query. */
/*
 * Copies newline-separated warnings from the most recent call (boolean
 * warnings, healed results) and returns the required buffer size including
 * the terminator; an empty string means the call raised no warnings.
 */
OCCT_BRIDGE_API size_t occt_bridge_session_last_warnings(
    const occt_bridge_session_t* session,
    char* buffer,
    size_t buffer_capacity
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_session_get_options(
    occt_bridge_session_t* session,
    occt_bridge_session_options_t* out_options
);
/* Rejects non-flag values, negative or non-finite fuzziness, and healing without validation. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_session_set_options(
    occt_bridge_session_t* session,
    const occt_bridge_session_options_t* options
);
OCCT_BRIDGE_API size_t occt_bridge_session_last_error(
    const occt_bridge_session_t* session,
    char* buffer,
    size_t buffer_capacity
);

/* Primitive construction. Sizes must be finite and strictly positive. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_box(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t size,
    occt_bridge_shape_id_t* out_shape
);

/* Axis directions must be finite and nonzero. Radii and heights are positive. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_cylinder(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis,
    double radius,
    double height,
    occt_bridge_shape_id_t* out_shape
);

/* Either cone radius may be zero, but not both. Height must be positive. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_cone(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis,
    double base_radius,
    double top_radius,
    double height,
    occt_bridge_shape_id_t* out_shape
);

OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_sphere(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t center,
    double radius,
    occt_bridge_shape_id_t* out_shape
);

/* Creates an open or closed wire from connected straight segments. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_polyline_wire(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    size_t point_count,
    int closed,
    occt_bridge_shape_id_t* out_shape
);

/* Creates a closed circular wire in the plane described by its normal. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_circle_wire(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t center,
    occt_bridge_vec3_t normal,
    double radius,
    occt_bridge_shape_id_t* out_shape
);

/* Creates a closed elliptical wire; major_radius must be >= minor_radius. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_ellipse_wire(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t center,
    occt_bridge_vec3_t normal,
    double major_radius,
    double minor_radius,
    occt_bridge_shape_id_t* out_shape
);

/* Creates a planar face from a closed wire handle. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_face_from_wire(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t wire,
    occt_bridge_shape_id_t* out_shape
);

/* Extrudes a face along a finite, nonzero direction. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_prism_from_face(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    occt_bridge_vec3_t direction,
    occt_bridge_shape_id_t* out_shape
);

/* Creates a closed planar polygon face and extrudes it along direction. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_polygon_prism(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    size_t point_count,
    occt_bridge_vec3_t direction,
    occt_bridge_shape_id_t* out_shape
);

/*
 * Legacy compatibility recipe. New application code should use the recipe
 * layer. Builds a faceted natural-stone solid. Bottom and top rings must have the
 * same point count and corresponding winding. The bottom ring must be planar;
 * top points and top_center may have different Z values. bottom_chamfer and
 * top_fillet may be zero to disable those treatments.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_faceted_stone(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* bottom_points,
    const occt_bridge_vec3_t* top_points,
    size_t point_count,
    occt_bridge_vec3_t top_center,
    double bottom_chamfer,
    double top_fillet,
    occt_bridge_shape_id_t* out_shape
);

/*
 * Legacy compatibility recipe. New application code should compose generic
 * primitives in the recipe layer. Builds a wall-mounted torch as separate fixture and flame shapes. wall_anchor
 * is the center of the mounting plate; wall_normal points away from the wall
 * and is projected onto the XY plane. scale must be finite and positive.
 * The returned positional light is located inside the modeled flame.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_wall_torch(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t wall_anchor,
    occt_bridge_vec3_t wall_normal,
    double scale,
    occt_bridge_wall_torch_result_t* out_torch
);

/* Sweeps a circular profile along a connected 3D polyline. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_polyline_tube(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* path_points,
    size_t point_count,
    double radius,
    occt_bridge_shape_id_t* out_shape
);

/*
 * Lofts through two or more closed polygon sections. Points are flattened
 * section-by-section; section_point_counts describes each contiguous section.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_loft(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    const size_t* section_point_counts,
    size_t section_count,
    int make_solid,
    int ruled,
    occt_bridge_shape_id_t* out_shape
);

/*
 * Sews faces and shells whose boundary edges lie within tolerance of each
 * other. Each input may be a face, shell, solid, or compound and must contain
 * at least one face; inputs are not modified. The result is a shell when
 * everything joins and a compound of shells and free faces otherwise.
 * Operation history records modified and deleted vertices, edges, and faces.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_sew(
    occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* shapes,
    size_t shape_count,
    double tolerance,
    occt_bridge_shape_id_t* out_shape
);

/*
 * Builds an outward-oriented solid from a shape containing exactly one
 * closed shell, such as the result of occt_bridge_sew. Open shells, several
 * shells, and invalid results return OCCT_BRIDGE_INVALID_GEOMETRY.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_make_solid(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shell,
    occt_bridge_shape_id_t* out_shape
);

/*
 * Builds a solid from one outer closed shell and zero or more closed shells
 * that bound internal voids. Inputs may be shells or shapes containing shells;
 * the largest enclosed volume is selected as the outer boundary. Every other
 * shell must lie inside it. Open, intersecting, or disjoint boundaries and
 * invalid results return OCCT_BRIDGE_INVALID_GEOMETRY.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_make_solid_from_shells(
    occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* shells,
    size_t shell_count,
    occt_bridge_shape_id_t* out_shape
);

/* Creates an assembly compound without consuming or modifying its children. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_compound(
    occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* shapes,
    size_t shape_count,
    occt_bridge_shape_id_t* out_shape
);

/* Boolean operations create new handles and leave both inputs unchanged. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_fuse(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t left,
    occt_bridge_shape_id_t right,
    occt_bridge_shape_id_t* out_shape
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_cut(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t object,
    occt_bridge_shape_id_t tool,
    occt_bridge_shape_id_t* out_shape
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_common(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t left,
    occt_bridge_shape_id_t right,
    occt_bridge_shape_id_t* out_shape
);

/* Selected edges must be descendant edges of shape. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_fillet(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* edges,
    size_t edge_count,
    double radius,
    occt_bridge_shape_id_t* out_shape
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_chamfer(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* edges,
    size_t edge_count,
    double distance,
    occt_bridge_shape_id_t* out_shape
);

/* Creates a joined skin offset. Offset is signed and nonzero. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_offset(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double offset,
    double tolerance,
    occt_bridge_shape_id_t* out_shape
);

/* Removes selected descendant faces and offsets the remaining skin. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_hollow(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* faces_to_remove,
    size_t face_count,
    double thickness,
    double tolerance,
    occt_bridge_shape_id_t* out_shape
);

/* Transforms create new handles and leave the input shape unchanged. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_translate(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t offset,
    occt_bridge_shape_id_t* out_shape
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_rotate(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t axis_origin,
    occt_bridge_vec3_t axis_direction,
    double angle_radians,
    occt_bridge_shape_id_t* out_shape
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_scale(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t center,
    double factor,
    occt_bridge_shape_id_t* out_shape
);

/* Shape inspection and lifetime management. */
/* Creates another session-owned handle to the same immutable OCCT shape. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_duplicate(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_id_t* out_shape
);

OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_type(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t* out_type
);

/* Descendant traversal excludes the input shape itself and removes duplicates. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_subshape_count(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t subshape_type,
    size_t* out_count
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_subshape_at(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t subshape_type,
    size_t index,
    occt_bridge_shape_id_t* out_subshape
);

/* Axis-aligned bounds, enlarged by shape tolerances as OCCT reports them. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_bounds_t* out_bounds
);

/*
 * Axis-aligned bounds that follow the geometry without tolerance enlargement,
 * for measuring lengths such as a box width or a cylinder height.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_exact_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_bounds_t* out_bounds
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_surface_area(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double* out_area
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_volume(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double* out_volume
);

/* Uses volume, surface, or linear mass in that order, based on topology. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_center_of_mass(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t* out_center
);

/* Returns the oriented unit normal at the center of a face's UV bounds. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_face_normal(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    occt_bridge_vec3_t* out_normal
);

OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_face_is_planar(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    int* out_is_planar
);

OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_edge_length(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    double* out_length
);

/* Succeeds for every edge; out_is_circle determines whether radius is set. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_edge_circle_radius(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    int* out_is_circle,
    double* out_radius
);

/* Returns curvature at the midpoint of the edge's parameter range. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_edge_curvature(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    int* out_is_defined,
    double* out_curvature
);

/* Deterministically samples curvature across the full edge parameter range. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_edge_curvature_range(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    size_t sample_count,
    double* out_minimum_curvature,
    double* out_maximum_curvature
);

/*
 * Computes curvature extrema over the full edge parameter range. The reported
 * minimum and maximum are curvatures of actual edge points; the true extrema
 * lie in [minimum_lower_bound, minimum] and [maximum, maximum_upper_bound].
 * Line and conic edges are evaluated at their analytic critical parameters and
 * report out_is_exact = 1 with coincident bounds. Bezier and B-spline edges,
 * including rational ones, are bounded by adaptive Bernstein subdivision until
 * both gaps are at most relative_tolerance times the maximum curvature, or
 * 1e-10 per model unit if larger (relative_tolerance in (0, 1]). Other curve types, and edges whose curvature
 * is undefined or does not converge, return OCCT_BRIDGE_INVALID_GEOMETRY.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_edge_curvature_extrema(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    double relative_tolerance,
    double* out_minimum,
    double* out_minimum_lower_bound,
    double* out_maximum,
    double* out_maximum_upper_bound,
    int* out_is_exact
);

/*
 * Tests direct topological adjacency within parent. Supported pairs are
 * face/edge incidence, face/face shared edges, and edge/edge shared vertices.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_is_adjacent(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t parent,
    occt_bridge_shape_id_t first,
    occt_bridge_shape_id_t second,
    int* out_is_adjacent
);

/* Tests OCCT topological identity between two session-owned shape handles. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_is_same(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t first,
    occt_bridge_shape_id_t second,
    int* out_is_same
);

/* Tests whether two adjacent faces have recorded G1-or-better continuity. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_faces_are_tangent(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t parent,
    occt_bridge_shape_id_t first_face,
    occt_bridge_shape_id_t second_face,
    int* out_are_tangent
);

/*
 * Operation history is attached to derived result handles. Source may be the
 * original input handle or any still-live handle referring to one of its
 * subshapes. Returned history shapes are new handles owned by the session.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_history_count(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t source,
    occt_bridge_history_relation_t relation,
    size_t* out_count
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_history_at(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t source,
    occt_bridge_history_relation_t relation,
    size_t index,
    occt_bridge_shape_id_t* out_shape
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_history_is_deleted(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t source,
    int* out_is_deleted
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_is_valid(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    int* out_is_valid
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_remove(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape
);

/* BREP persistence. Paths are UTF-8 on platforms where the filesystem supports it. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_brep_save(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const char* path
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_brep_load(
    occt_bridge_session_t* session,
    const char* path,
    occt_bridge_shape_id_t* out_shape
);

/* STEP exchange. Geometry and topology are preserved; OCCT session handles,
 * operation history, and application metadata are not part of STEP files. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_step_save(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const char* path
);
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_step_load(
    occt_bridge_session_t* session,
    const char* path,
    occt_bridge_shape_id_t* out_shape
);

/* STL mesh export. Deflections must be finite and positive. `binary` must be
 * 0 for ASCII STL or 1 for binary STL. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_stl_save(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const char* path,
    double linear_deflection,
    double angular_deflection_radians,
    int binary
);

#ifdef __cplusplus
}
#endif

#endif
