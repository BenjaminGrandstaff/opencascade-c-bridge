#ifndef OCCT_BRIDGE_H
#define OCCT_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#define OCCT_BRIDGE_ABI_VERSION 38u
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
 *   hollowing, draft, sewing, and STEP and BREP import with BRepCheck; an invalid
 *   result returns OCCT_BRIDGE_INVALID_GEOMETRY instead of being stored.
 * heal_invalid_results: repair an invalid result with shape fixing and store
 *   it when the repair is valid, recording a warning; requires validation.
 *   Operation history is carried through the repair.
 * boolean_fuzzy_tolerance: treat boolean input faces and edges closer than
 *   this distance as coincident; 0 performs exact booleans.
 */
/*
 * Structured failure diagnostics. A fillet, chamfer, offset, hollow, draft, or
 * boolean that fails in the kernel, and any operation or import whose result
 * fails validation, records what OCCT reported about the cause. Diagnostics
 * describe the most recent call and are cleared when the next call starts;
 * the diagnostic queries and occt_bridge_shape_release leave them intact.
 * At most OCCT_BRIDGE_MAX_DIAGNOSTICS are kept per call, and the last error
 * notes how many were omitted.
 */
#define OCCT_BRIDGE_MAX_DIAGNOSTICS 64u

typedef int32_t occt_bridge_diagnostic_kind_t;
enum {
    /* A selected edge whose fillet contour failed; code is a ChFiDS_ErrorStatus. */
    OCCT_BRIDGE_DIAGNOSTIC_FILLET_EDGE = 1,
    /* A vertex where fillet contours could not be joined; code is 0. */
    OCCT_BRIDGE_DIAGNOSTIC_FILLET_VERTEX = 2,
    /*
     * A selected fillet or chamfer edge whose contour fails even when built
     * alone. Chamfers report no faulty contours, and fillets sometimes name
     * none, so failing contours are found by rebuilding each one; code is 0
     * and the name is the OCCT exception the rebuild raised, if any.
     */
    OCCT_BRIDGE_DIAGNOSTIC_ISOLATED_EDGE = 3,
    /* Offset or hollow failure; code is a BRepOffset_Error, shape the input subshape OCCT blamed. */
    OCCT_BRIDGE_DIAGNOSTIC_OFFSET = 4,
    /* A boolean error or warning alert; the name is its OCCT alert key, code is 0. */
    OCCT_BRIDGE_DIAGNOSTIC_BOOLEAN_ALERT = 5,
    /* A subshape of a rejected result that failed validation; code is a BRepCheck_Status. */
    OCCT_BRIDGE_DIAGNOSTIC_INVALID_SUBSHAPE = 6,
    /* Draft failure; code is Draft_ErrorStatus, shape is OCCT's problematic subshape. */
    OCCT_BRIDGE_DIAGNOSTIC_DRAFT = 7
};

typedef struct occt_bridge_diagnostic {
    occt_bridge_diagnostic_kind_t kind;
    /* OCCT's enumeration value for the kind; its name is available by index. */
    int32_t code;
    /*
     * Index into the call's selection (fillet or chamfer edges, hollow/draft
     * faces) or boolean operand (0 for left or object, 1 for right or
     * tool) that the diagnostic concerns; -1 when it concerns none.
     */
    int64_t input_index;
    /* Nonzero when the diagnostic names a subshape. */
    int has_shape;
} occt_bridge_diagnostic_t;

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

OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_session_diagnostic_count(
    occt_bridge_session_t* session,
    size_t* out_count
);
/*
 * Reads one diagnostic. When out_shape is not NULL and the diagnostic names
 * a subshape, a new session-owned handle to it is returned (each call makes
 * another); otherwise out_shape receives OCCT_BRIDGE_INVALID_SHAPE_ID. The
 * subshape may belong to the input or to a rejected result.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_session_diagnostic_at(
    occt_bridge_session_t* session,
    size_t index,
    occt_bridge_diagnostic_t* out_diagnostic,
    occt_bridge_shape_id_t* out_shape
);
/*
 * Copies OCCT's name for a diagnostic's code, such as ChFiDS_WalkingFailure,
 * BRepOffset_C0Geometry, BOPAlgo_AlertBOPNotAllowed, or BRepCheck_NotClosed,
 * and returns the required buffer size including the terminator; 0 for an
 * index out of range.
 */
OCCT_BRIDGE_API size_t occt_bridge_session_diagnostic_name(
    const occt_bridge_session_t* session,
    size_t index,
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

/* A line (kind 0, middle ignored) or circular arc (kind 1, passing through
 * middle). Arc points must be distinct and non-collinear. Segments must be
 * ordered and connected within kernel tolerance. closed is 0 or 1; 1 requires
 * closure, 0 permits open or closed wires. At least one segment is required.
 * This constructs a wire, not necessarily a planar or simple face boundary. */
typedef struct occt_bridge_wire_segment {
    int kind;
    occt_bridge_vec3_t start;
    occt_bridge_vec3_t middle;
    occt_bridge_vec3_t end;
} occt_bridge_wire_segment_t;

OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_segment_wire(
    occt_bridge_session_t* session,
    const occt_bridge_wire_segment_t* segments,
    size_t segment_count,
    int closed,
    occt_bridge_shape_id_t* out_shape
);

/* Creates a closed circular wire in the plane described by its normal. */
/* One segment of occt_bridge_create_curve_wire, reading point_count points
 * from `points` starting at first_point: a line (kind 0, 2 points), a circular
 * arc through its middle point (kind 1, 3 points), or a B-spline interpolated
 * through its points in order (kind 2, 2 or more). Spline flags: bit 0 uses
 * start_tangent and bit 1 end_tangent as the curve's end directions (any
 * nonzero length); bit 2 makes it periodic, a smooth closed loop through its
 * points that ends where it starts, with no corner. Other kinds ignore flags. */
typedef struct occt_bridge_curve_segment {
    int32_t kind;
    int32_t flags;
    size_t first_point;
    size_t point_count;
    occt_bridge_vec3_t start_tangent;
    occt_bridge_vec3_t end_tangent;
} occt_bridge_curve_segment_t;

/* Like occt_bridge_create_segment_wire, with spline segments. Consecutive
 * segments must meet within kernel tolerance, and consecutive points within a
 * segment must be distinct. closed is 0 or 1; 1 requires the last segment to
 * end where the first starts. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_curve_wire(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    size_t point_count,
    const occt_bridge_curve_segment_t* segments,
    size_t segment_count,
    int closed,
    occt_bridge_shape_id_t* out_shape
);

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

/* Closes one valid open wire with a translated reversed copy and straight
 * endpoint bridges. Offset must be finite/nonzero; the boundary must be planar
 * and non-self-intersecting. Records original/translated edge ancestry.
 * Leaves the input unchanged; returns a new planar face handle. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_open_profile_face(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t wire,
    occt_bridge_vec3_t offset,
    occt_bridge_shape_id_t* out_shape
);

/* Extends a straight open chain perpendicular to direction to its first body
 * contact within maximum_length. Direction is normalized. The entire translated
 * chain must meet the first contact; partial contacts and profiles already on
 * or inside the body fail. The body must contain one valid solid. Returns a
 * simple planar closure face with profile ancestry; leaves inputs unchanged. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_open_profile_face_to_next(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t wire,
    occt_bridge_shape_id_t body,
    occt_bridge_vec3_t direction,
    double maximum_length,
    occt_bridge_shape_id_t* out_shape
);

/* Extrudes a face along a finite, nonzero direction. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_prism_from_face(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    occt_bridge_vec3_t direction,
    occt_bridge_shape_id_t* out_shape
);

/* Revolves a face about origin/axis. Axis is finite and nonzero; angle is in
 * radians, finite, nonzero, and within [-2*pi, 2*pi]. Negative angles reverse
 * the sweep. Records generated/modified topology and applies session result
 * validation options. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_revolve_from_face(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis,
    double angle_radians,
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
 * Like occt_bridge_create_loft, but each section is one non-periodic B-spline
 * interpolated through its points and closed back to its first point: smooth
 * everywhere except a corner at the first point (such as an airfoil trailing
 * edge). Consecutive points, including last-to-first, must not coincide.
 * ruled = 1 keeps straight lines between sections; 0 also smooths across them.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_create_spline_loft(
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
/*
 * Linear radius evolution along each selected open tangent contour, from
 * OCCT's first spine vertex to its last (not necessarily the selected edge's
 * orientation). Both radii must be finite and positive. Tangent neighbors
 * may be included. Duplicate edges and closed contours are rejected.
 * Inputs are unchanged; validation, healing, and fillet diagnostics/history
 * are the same as for constant-radius fillets.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_variable_fillet(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* edges,
    size_t edge_count,
    double start_radius,
    double end_radius,
    occt_bridge_shape_id_t* out_shape
);
typedef struct occt_bridge_fillet_station {
    double position; /* Relative contour parameter in [0,1]. */
    double radius;
} occt_bridge_fillet_station_t;

/* Interpolates at least two ordered finite radius stations over each selected
 * open contour. Positions must start at 0, end at 1, and strictly increase;
 * radii must be positive. Interpolation is OCCT's smooth law, not piecewise
 * linear interpolation. Spine direction: 0 = kernel order, 1 = reversed order,
 * 2 = start at endpoint nearest finite start_point (ties fail). One law per
 * contour, including tangent neighbors. Duplicate edges/closed contours fail.
 * Inputs, diagnostics/history, validation/healing follow variable_fillet. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_variable_fillet_stations(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* edges,
    size_t edge_count,
    const occt_bridge_fillet_station_t* stations,
    size_t station_count,
    int32_t spine_direction,
    occt_bridge_vec3_t start_point,
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

/*
 * Tapers selected descendant planar/cylindrical/conical faces. Tangential
 * neighbors may also be tapered. The neutral plane is given by origin/normal.
 * Pull direction chooses the side where positive angles remove material;
 * negative angles add material. All vectors must be finite and directions
 * nonzero; 0 < abs(angle_radians) < pi/2. Duplicate selections are rejected.
 * Input is unchanged; result validation and operation history are retained.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_draft(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* faces,
    size_t face_count,
    occt_bridge_vec3_t neutral_origin,
    occt_bridge_vec3_t neutral_normal,
    occt_bridge_vec3_t pull_direction,
    double angle_radians,
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

/*
 * Transforms create new handles and leave the input shape unchanged.
 * Translation and rotation are rigid: the result shares the input's geometry
 * and differs only by a location, so placed copies cost a handle rather than
 * a deep copy, and their history maps each input subshape to its moved
 * counterpart. Scaling copies geometry.
 */
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

typedef struct occt_bridge_mass_properties {
    double volume;
    occt_bridge_vec3_t center;
    double inertia[9]; /* Row-major central tensor in model XYZ; unit density. */
    double relative_volume_error; /* OCCT adaptive quadrature error estimate. */
} occt_bridge_mass_properties_t;

typedef struct occt_bridge_distance_result {
    double distance;
    occt_bridge_vec3_t first;
    occt_bridge_vec3_t second;
} occt_bridge_distance_result_t;

/* Valid solid geometry; unit-density volume/center/central inertia. Coordinate
 * units u give volume u^3, center u, and inertia u^5. Uses adaptive BREP surface
 * integration (requested relative volume error 1e-9), without triangulation. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_mass_properties(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_mass_properties_t* out_properties
);

/* Minimum separation in model units and one witness point on each shape.
 * Intersecting/touching/contained shapes have zero separation. Does not
 * distinguish interference from contact; use boolean common volume for that.
 * Uses exact BREP geometry; creates no persistent handles. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_distance(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t first,
    occt_bridge_shape_id_t second,
    occt_bridge_distance_result_t* out_distance
);

/* Volume shared by two valid solid shapes; zero for separation and contact.
 * Uses a non-destructive BREP boolean and adaptive volume integration. No
 * handles are created and both inputs retain their geometry and history. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_overlap_volume(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t first,
    occt_bridge_shape_id_t second,
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
/*
 * Returns a new handle sharing result's geometry, retaining direct history and
 * tracing intermediate's sources through result's history. Both inputs remain
 * unchanged. Intermediate must be a direct input to result; both need history.
 * Generated ancestry stays generated through subsequent modifications;
 * modified ancestry followed by generation becomes generated. Deleted sources
 * may still generate topology. Temporary intermediate handles may then be freed.
 */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_compose_history(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t intermediate,
    occt_bridge_shape_id_t* out_shape
);
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
/*
 * Releases a handle for automatic cleanup in language bindings. Unlike
 * occt_bridge_shape_remove it ignores unknown or already released handles
 * and leaves the last error, warnings, and diagnostics untouched, so releasing temporaries
 * between a call and reading its diagnostics loses nothing. Never throws;
 * a null session is ignored.
 */
OCCT_BRIDGE_API void occt_bridge_shape_release(
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

/* Orthographic hidden-line removal. Direction points toward the viewer;
 * x_axis is the image's rightward axis and must be perpendicular to direction.
 * Output compounds contain visible/hidden sharp, smooth, and outline edges in
 * local XY coordinates (z=0). Inputs are preserved; superimposed edges may remain.
 * Both outputs are required, distinct, and zeroed on failure. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_orthographic_projection(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t direction,
    occt_bridge_vec3_t x_axis,
    occt_bridge_shape_id_t* out_visible,
    occt_bridge_shape_id_t* out_hidden
);

/* Uniform normalized-parameter samples of a finite edge's exact curve.
 * Output is an approximation for export, not a certified chordal tolerance.
 * Caller provides point_count entries; 2 <= point_count <= 100000.
 * Samples follow the edge's topological orientation. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_edge_sample_points(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    size_t point_count,
    occt_bridge_vec3_t* out_points
);

typedef struct {
    double linear_deflection;
    double angular_deflection;
    size_t maximum_triangles;
} occt_bridge_mesh_options_t;

typedef struct {
    /* Zero-based index in shape_subshapes(FACE), not a persistent shape ID. */
    size_t face_index;
    occt_bridge_vec3_t points[3];
} occt_bridge_mesh_triangle_t;

/* Non-destructive tessellation on a private copy. Deflections are absolute
 * model units and radians. Linear deflection must be >= max(1e-7, span*1e-5);
 * angular deflection is [0.01, pi]. maximum_triangles is [1, 1000000].
 * NULL/0 queries required count. Insufficient capacity reports that count;
 * other failures report zero. Caller data changes only on complete success.
 * The triangle limit bounds returned data, not OCCT's meshing workspace.
 * Winding follows oriented faces; disconnected face vertices are not welded. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_surface_mesh(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    occt_bridge_mesh_options_t options,
    occt_bridge_mesh_triangle_t* out_triangles, size_t capacity,
    size_t* out_count
);

/* Resolve candidate subshapes to zero-based unique topology indices in one
 * map traversal. All candidates must belong to the requested topology map.
 * Buffers change only on complete success; count zero accepts NULL buffers. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_subshape_indices(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t type, const occt_bridge_shape_id_t* candidates,
    size_t count, size_t* out_indices
);

/* Non-destructive clipping of valid solid geometry by an infinite plane.
 * keep_positive is 1 to retain the normal's side, 0 for the opposite side.
 * The closed result may be empty; original-source history is retained. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_clip_by_plane(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t plane_origin,
    occt_bridge_vec3_t plane_normal,
    int keep_positive,
    occt_bridge_shape_id_t* out_shape
);

/* Unique subshapes in the same order as subshape_at, indexed once per call.
 * Null buffer with capacity 0 queries the required count without handles.
 * Otherwise capacity must fit every result. Insufficient capacity reports the
 * required count and leaves the buffer unchanged; other failures report 0.
 * Success transfers ownership of count independent handles to the caller.
 * On failure no new handles survive. O(topology + count) time/storage. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_subshapes(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t type,
    occt_bridge_shape_id_t* out_shapes,
    size_t capacity,
    size_t* out_count
);

/* Smallest principal radii of one face, signed by its outward normal (the
 * surface normal, reversed for a reversed face). Convex: the face curves away
 * from the outward normal (outside of a cylinder or fillet). Concave: toward it
 * (a bore or inside fillet). INFINITY when the face never curves that way. */
typedef struct occt_bridge_face_radius_bounds {
    double convex_radius;
    double concave_radius;
    occt_bridge_vec3_t convex_point; /* Where convex_radius is attained. */
    occt_bridge_vec3_t concave_point;
    int32_t exact;    /* 1 on planes, cylinders, cones, spheres, tori; 0 sampled. */
    uint32_t samples; /* Evaluated in-face samples; 0 when exact. */
} occt_bridge_face_radius_bounds_t;

/* One entry per unique face, in occt_bridge_shape_subshapes order. Analytic
 * faces are exact: cone radii use the face's v range (0 at an included apex)
 * and tori their stationary latitudes. Other surfaces are sampled on a
 * samples_per_direction^2 UV grid restricted to the face, so a smaller radius
 * between samples can be missed. samples_per_direction is in [2, 1024].
 * Buffer protocol as occt_bridge_shape_subshapes; no handles are created. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_face_radius_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    uint32_t samples_per_direction,
    occt_bridge_face_radius_bounds_t* out_bounds,
    size_t capacity,
    size_t* out_count
);

typedef int32_t occt_bridge_edge_concavity_t;
enum {
    OCCT_BRIDGE_EDGE_SMOOTH = 0,  /* Adjacent faces meet tangentially. */
    OCCT_BRIDGE_EDGE_CONVEX = 1,  /* Sharp outside corner. */
    OCCT_BRIDGE_EDGE_CONCAVE = 2, /* Sharp inside corner. */
    OCCT_BRIDGE_EDGE_MIXED = 3,   /* Convex along part, concave along part. */
    OCCT_BRIDGE_EDGE_OTHER = 4    /* Free boundary, degenerate, or non-manifold. */
};

/* Classifies each unique edge, in occt_bridge_shape_subshapes order, with
 * OCCT's BRepOffset_Analyse: faces meeting within tangency_radians, in
 * (0, pi/2), are smooth. Buffer protocol as occt_bridge_shape_subshapes. */
OCCT_BRIDGE_API occt_bridge_status_t occt_bridge_shape_edge_concavities(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double tangency_radians,
    occt_bridge_edge_concavity_t* out_concavities,
    size_t capacity,
    size_t* out_count
);

#ifdef __cplusplus
}
#endif

#endif
