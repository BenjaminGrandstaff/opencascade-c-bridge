/*
 * Argument-validation and error-path conformance test for the C ABI. Each
 * case passes exactly one invalid argument, so every clause of a validation
 * condition is exercised and must report the documented status.
 */
#include "occt_bridge.h"

#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static int failures = 0;

static void expect_status(
    occt_bridge_status_t actual,
    occt_bridge_status_t expected,
    const char* call,
    int line) {
    if (actual != expected) {
        (void)fprintf(stderr, "line %d: %s returned %d, expected %d\n", line, call, actual, expected);
        ++failures;
    }
}

#define EXPECT(call, expected) expect_status((call), (expected), #call, __LINE__)

enum {
    OK = OCCT_BRIDGE_OK,
    ARG = OCCT_BRIDGE_INVALID_ARGUMENT,
    MISSING = OCCT_BRIDGE_SHAPE_NOT_FOUND,
    GEOMETRY = OCCT_BRIDGE_INVALID_GEOMETRY,
    IO = OCCT_BRIDGE_IO_ERROR
};

static const occt_bridge_shape_id_t unknown = UINT64_C(999999);

static occt_bridge_vec3_t vec(double x, double y, double z) {
    const occt_bridge_vec3_t value = {x, y, z};
    return value;
}

static occt_bridge_shape_id_t require_shape(occt_bridge_status_t status, occt_bridge_shape_id_t shape, int line) {
    if (status != OK || shape == OCCT_BRIDGE_INVALID_SHAPE_ID) {
        (void)fprintf(stderr, "line %d: fixture construction failed with %d\n", line, status);
        abort();
    }
    return shape;
}

static occt_bridge_status_t fixture_status = OK;

/* The comma operator runs the call before its output id is read. */
#define SHAPE(call, out) (fixture_status = (call), require_shape(fixture_status, (out), __LINE__))

static occt_bridge_shape_id_t subshape(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t type,
    size_t index) {
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;
    return SHAPE(occt_bridge_shape_subshape_at(session, shape, type, index, &out), out);
}

static void session_and_diagnostics(occt_bridge_session_t* session) {
    occt_bridge_session_t* created = NULL;
    size_t count = 0;
    char buffer[16] = {0};

    EXPECT(occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION, NULL), ARG);
    EXPECT(occt_bridge_session_shape_count(session, NULL), ARG);
    EXPECT(occt_bridge_session_shape_count(NULL, &count), ARG);
    {
        occt_bridge_session_options_t options = {1, 0, 0.0};
        const occt_bridge_session_options_t not_flag = {2, 0, 0.0};
        const occt_bridge_session_options_t heal_flag = {1, 2, 0.0};
        const occt_bridge_session_options_t negative = {1, 0, -1.0};
        const occt_bridge_session_options_t not_finite = {1, 0, NAN};
        const occt_bridge_session_options_t heal_unvalidated = {0, 1, 0.0};
        EXPECT(occt_bridge_session_get_options(session, NULL), ARG);
        EXPECT(occt_bridge_session_set_options(session, NULL), ARG);
        EXPECT(occt_bridge_session_set_options(session, &not_flag), ARG);
        EXPECT(occt_bridge_session_set_options(session, &heal_flag), ARG);
        EXPECT(occt_bridge_session_set_options(session, &negative), ARG);
        EXPECT(occt_bridge_session_set_options(session, &not_finite), ARG);
        EXPECT(occt_bridge_session_set_options(session, &heal_unvalidated), ARG);
        EXPECT(occt_bridge_session_get_options(session, &options), OK);
        if (occt_bridge_session_last_warnings(NULL, buffer, sizeof(buffer)) != 0) {
            (void)fprintf(stderr, "last_warnings without a session must be empty\n");
            ++failures;
        }
    }
    EXPECT(occt_bridge_session_clear(NULL), ARG);
    EXPECT(occt_bridge_create_box(NULL, vec(0, 0, 0), vec(1, 1, 1), NULL), ARG);
    EXPECT(occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION, &created), OK);
    occt_bridge_session_destroy(created);
    occt_bridge_session_destroy(NULL);

    if (occt_bridge_session_last_error(NULL, buffer, sizeof(buffer)) != 0) {
        (void)fprintf(stderr, "last_error without a session must be empty\n");
        ++failures;
    }
    EXPECT(occt_bridge_session_shape_count(session, NULL), ARG);
    if (occt_bridge_session_last_error(session, NULL, 0) == 0
        || occt_bridge_session_last_error(session, buffer, 0) == 0) {
        (void)fprintf(stderr, "last_error must report the message length without a buffer\n");
        ++failures;
    }
    for (occt_bridge_status_t status = OK; status <= OCCT_BRIDGE_INTERNAL_ERROR + 1; ++status) {
        if (occt_bridge_status_string(status) == NULL) {
            (void)fprintf(stderr, "status %d has no string\n", status);
            ++failures;
        }
    }
}

static void primitives(occt_bridge_session_t* session) {
    const occt_bridge_vec3_t zero = vec(0, 0, 0);
    const occt_bridge_vec3_t up = vec(0, 0, 1);
    const occt_bridge_vec3_t nan_x = vec(NAN, 0, 0);
    const occt_bridge_vec3_t nan_y = vec(0, NAN, 1);
    const occt_bridge_vec3_t nan_z = vec(0, 0, NAN);
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;

    EXPECT(occt_bridge_create_box(session, zero, vec(1, 1, 1), NULL), ARG);
    EXPECT(occt_bridge_create_box(session, nan_x, vec(1, 1, 1), &out), ARG);
    EXPECT(occt_bridge_create_box(session, nan_y, vec(1, 1, 1), &out), ARG);
    EXPECT(occt_bridge_create_box(session, nan_z, vec(1, 1, 1), &out), ARG);
    EXPECT(occt_bridge_create_box(session, zero, nan_x, &out), ARG);
    EXPECT(occt_bridge_create_box(session, zero, vec(0, 1, 1), &out), ARG);
    EXPECT(occt_bridge_create_box(session, zero, vec(1, 0, 1), &out), ARG);
    EXPECT(occt_bridge_create_box(session, zero, vec(1, 1, 0), &out), ARG);

    EXPECT(occt_bridge_create_cylinder(session, zero, up, 1, 1, NULL), ARG);
    EXPECT(occt_bridge_create_cylinder(session, nan_x, up, 1, 1, &out), ARG);
    EXPECT(occt_bridge_create_cylinder(session, zero, nan_x, 1, 1, &out), ARG);
    EXPECT(occt_bridge_create_cylinder(session, zero, up, NAN, 1, &out), ARG);
    EXPECT(occt_bridge_create_cylinder(session, zero, up, 1, NAN, &out), ARG);
    EXPECT(occt_bridge_create_cylinder(session, zero, up, 0, 1, &out), ARG);
    EXPECT(occt_bridge_create_cylinder(session, zero, up, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_cylinder(session, zero, zero, 1, 1, &out), ARG);

    EXPECT(occt_bridge_create_cone(session, zero, up, 1, 0.5, 1, NULL), ARG);
    EXPECT(occt_bridge_create_cone(session, nan_x, up, 1, 0.5, 1, &out), ARG);
    EXPECT(occt_bridge_create_cone(session, zero, nan_x, 1, 0.5, 1, &out), ARG);
    EXPECT(occt_bridge_create_cone(session, zero, up, NAN, 0.5, 1, &out), ARG);
    EXPECT(occt_bridge_create_cone(session, zero, up, 1, NAN, 1, &out), ARG);
    EXPECT(occt_bridge_create_cone(session, zero, up, 1, 0.5, NAN, &out), ARG);
    EXPECT(occt_bridge_create_cone(session, zero, up, -1, 0.5, 1, &out), ARG);
    EXPECT(occt_bridge_create_cone(session, zero, up, 1, -0.5, 1, &out), ARG);
    EXPECT(occt_bridge_create_cone(session, zero, up, 0, 0, 1, &out), ARG);
    EXPECT(occt_bridge_create_cone(session, zero, up, 1, 0.5, 0, &out), ARG);
    EXPECT(occt_bridge_create_cone(session, zero, zero, 1, 0.5, 1, &out), ARG);
    EXPECT(occt_bridge_create_cone(session, zero, up, 0, 0.5, 1, &out), OK);

    EXPECT(occt_bridge_create_sphere(session, zero, 1, NULL), ARG);
    EXPECT(occt_bridge_create_sphere(session, nan_x, 1, &out), ARG);
    EXPECT(occt_bridge_create_sphere(session, zero, NAN, &out), ARG);
    EXPECT(occt_bridge_create_sphere(session, zero, 0, &out), ARG);
}

static void wires_and_faces(occt_bridge_session_t* session) {
    const occt_bridge_vec3_t zero = vec(0, 0, 0);
    const occt_bridge_vec3_t up = vec(0, 0, 1);
    const occt_bridge_vec3_t nan_x = vec(NAN, 0, 0);
    const occt_bridge_vec3_t square[] = {{0, 0, 0}, {1, 0, 0}, {1, 1, 0}, {0, 1, 0}};
    const occt_bridge_vec3_t repeated_end[] = {{0, 0, 0}, {1, 0, 0}, {1, 1, 0}, {0, 0, 0}};
    const occt_bridge_vec3_t duplicate[] = {{0, 0, 0}, {0, 0, 0}, {1, 1, 0}};
    const occt_bridge_vec3_t non_finite[] = {{0, 0, 0}, {NAN, 0, 0}, {1, 1, 0}};
    const occt_bridge_vec3_t twisted[] = {{0, 0, 0}, {1, 0, 0}, {1, 1, 1}, {0, 1, 0}};
    const occt_bridge_vec3_t collinear[] = {{0, 0, 0}, {1, 0, 0}, {2, 0, 0}};
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;

    EXPECT(occt_bridge_create_polyline_wire(session, square, 4, 1, NULL), ARG);
    EXPECT(occt_bridge_create_polyline_wire(session, NULL, 4, 1, &out), ARG);
    EXPECT(occt_bridge_create_polyline_wire(session, square, 4, 2, &out), ARG);
    EXPECT(occt_bridge_create_polyline_wire(session, square, 2, 1, &out), ARG);
    EXPECT(occt_bridge_create_polyline_wire(session, non_finite, 3, 0, &out), ARG);
    EXPECT(occt_bridge_create_polyline_wire(session, duplicate, 3, 0, &out), ARG);
    EXPECT(occt_bridge_create_polyline_wire(session, repeated_end, 4, 1, &out), ARG);
    EXPECT(occt_bridge_create_polyline_wire(session, repeated_end, 4, 0, &out), OK);

    EXPECT(occt_bridge_create_circle_wire(session, zero, up, 1, NULL), ARG);
    EXPECT(occt_bridge_create_circle_wire(session, nan_x, up, 1, &out), ARG);
    EXPECT(occt_bridge_create_circle_wire(session, zero, nan_x, 1, &out), ARG);
    EXPECT(occt_bridge_create_circle_wire(session, zero, up, NAN, &out), ARG);
    EXPECT(occt_bridge_create_circle_wire(session, zero, up, 0, &out), ARG);
    EXPECT(occt_bridge_create_circle_wire(session, zero, zero, 1, &out), ARG);

    EXPECT(occt_bridge_create_ellipse_wire(session, zero, up, 2, 1, NULL), ARG);
    EXPECT(occt_bridge_create_ellipse_wire(session, nan_x, up, 2, 1, &out), ARG);
    EXPECT(occt_bridge_create_ellipse_wire(session, zero, nan_x, 2, 1, &out), ARG);
    EXPECT(occt_bridge_create_ellipse_wire(session, zero, up, NAN, 1, &out), ARG);
    EXPECT(occt_bridge_create_ellipse_wire(session, zero, up, 2, NAN, &out), ARG);
    EXPECT(occt_bridge_create_ellipse_wire(session, zero, up, 2, 0, &out), ARG);
    EXPECT(occt_bridge_create_ellipse_wire(session, zero, up, 1, 2, &out), ARG);
    EXPECT(occt_bridge_create_ellipse_wire(session, zero, zero, 2, 1, &out), ARG);

    const occt_bridge_shape_id_t box =
        SHAPE(occt_bridge_create_box(session, zero, vec(1, 1, 1), &out), out);
    const occt_bridge_shape_id_t twisted_wire =
        SHAPE(occt_bridge_create_polyline_wire(session, twisted, 4, 1, &out), out);
    EXPECT(occt_bridge_create_face_from_wire(session, twisted_wire, NULL), ARG);
    EXPECT(occt_bridge_create_face_from_wire(session, unknown, &out), MISSING);
    EXPECT(occt_bridge_create_face_from_wire(session, box, &out), GEOMETRY);
    EXPECT(occt_bridge_create_face_from_wire(session, twisted_wire, &out), GEOMETRY);

    const occt_bridge_shape_id_t face = subshape(session, box, OCCT_BRIDGE_SHAPE_FACE, 0);
    EXPECT(occt_bridge_create_prism_from_face(session, face, up, NULL), ARG);
    EXPECT(occt_bridge_create_prism_from_face(session, face, nan_x, &out), ARG);
    EXPECT(occt_bridge_create_prism_from_face(session, face, zero, &out), ARG);
    EXPECT(occt_bridge_create_prism_from_face(session, unknown, up, &out), MISSING);
    EXPECT(occt_bridge_create_prism_from_face(session, box, up, &out), GEOMETRY);

    EXPECT(occt_bridge_create_polygon_prism(session, square, 4, up, NULL), ARG);
    EXPECT(occt_bridge_create_polygon_prism(session, NULL, 4, up, &out), ARG);
    EXPECT(occt_bridge_create_polygon_prism(session, square, 2, up, &out), ARG);
    EXPECT(occt_bridge_create_polygon_prism(session, square, 4, nan_x, &out), ARG);
    EXPECT(occt_bridge_create_polygon_prism(session, square, 4, zero, &out), ARG);
    EXPECT(occt_bridge_create_polygon_prism(session, non_finite, 3, up, &out), ARG);
    EXPECT(occt_bridge_create_polygon_prism(session, collinear, 3, up, &out), GEOMETRY);
}

static void recipes_sweeps_and_lofts(occt_bridge_session_t* session) {
    const occt_bridge_vec3_t zero = vec(0, 0, 0);
    const occt_bridge_vec3_t nan_x = vec(NAN, 0, 0);
    const occt_bridge_vec3_t bottom[] = {{-2, -2, 0}, {2, -2, 0}, {2, 2, 0}, {-2, 2, 0}};
    const occt_bridge_vec3_t top[] = {{-2, -2, 3}, {2, -2, 3}, {2, 2, 3}, {-2, 2, 3}};
    const occt_bridge_vec3_t bad_bottom[] = {{NAN, -2, 0}, {2, -2, 0}, {2, 2, 0}, {-2, 2, 0}};
    const occt_bridge_vec3_t bad_top[] = {{NAN, -2, 3}, {2, -2, 3}, {2, 2, 3}, {-2, 2, 3}};
    const occt_bridge_vec3_t center = vec(0, 0, 4);
    const occt_bridge_vec3_t path[] = {{0, 0, 0}, {0, 0, 5}, {3, 0, 8}};
    const occt_bridge_vec3_t bad_path[] = {{0, 0, 0}, {NAN, 0, 5}};
    const occt_bridge_vec3_t stalled_path[] = {{0, 0, 0}, {0, 0, 0}, {0, 0, 5}};
    const occt_bridge_vec3_t sections[] = {
        {0, 0, 0}, {2, 0, 0}, {2, 2, 0}, {0, 2, 0},
        {0, 0, 3}, {2, 0, 3}, {2, 2, 3}, {0, 2, 3},
    };
    const occt_bridge_vec3_t bad_sections[] = {
        {0, 0, 0}, {2, 0, 0}, {2, 2, 0}, {0, 2, 0},
        {0, 0, 3}, {NAN, 0, 3}, {2, 2, 3}, {0, 2, 3},
    };
    const size_t counts[] = {4, 4};
    const size_t short_counts[] = {4, 2};
    const size_t overflowing_counts[] = {SIZE_MAX, 3};
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;
    occt_bridge_wall_torch_result_t torch;

    EXPECT(occt_bridge_create_faceted_stone(session, bottom, top, 4, center, 0, 0, NULL), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, NULL, top, 4, center, 0, 0, &out), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, NULL, 4, center, 0, 0, &out), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, top, 2, center, 0, 0, &out), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, top, 4, nan_x, 0, 0, &out), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, top, 4, center, NAN, 0, &out), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, top, 4, center, 0, NAN, &out), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, top, 4, center, -1, 0, &out), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, top, 4, center, 0, -1, &out), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, bad_bottom, top, 4, center, 0, 0, &out), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, bad_top, 4, center, 0, 0, &out), ARG);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, top, 4, center, 10, 0, &out), GEOMETRY);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, top, 4, center, 0, 10, &out), GEOMETRY);
    EXPECT(occt_bridge_create_faceted_stone(session, bottom, top, 4, center, 0, 0, &out), OK);

    EXPECT(occt_bridge_create_wall_torch(session, zero, vec(1, 0, 0), 1, NULL), ARG);
    EXPECT(occt_bridge_create_wall_torch(session, nan_x, vec(1, 0, 0), 1, &torch), ARG);
    EXPECT(occt_bridge_create_wall_torch(session, zero, nan_x, 1, &torch), ARG);
    EXPECT(occt_bridge_create_wall_torch(session, zero, vec(1, 0, 0), NAN, &torch), ARG);
    EXPECT(occt_bridge_create_wall_torch(session, zero, vec(1, 0, 0), 0, &torch), ARG);
    EXPECT(occt_bridge_create_wall_torch(session, zero, vec(0, 0, 1), 1, &torch), ARG);

    EXPECT(occt_bridge_create_polyline_tube(session, path, 3, 0.5, NULL), ARG);
    EXPECT(occt_bridge_create_polyline_tube(session, NULL, 3, 0.5, &out), ARG);
    EXPECT(occt_bridge_create_polyline_tube(session, path, 1, 0.5, &out), ARG);
    EXPECT(occt_bridge_create_polyline_tube(session, path, 3, NAN, &out), ARG);
    EXPECT(occt_bridge_create_polyline_tube(session, path, 3, 0, &out), ARG);
    EXPECT(occt_bridge_create_polyline_tube(session, bad_path, 2, 0.5, &out), ARG);
    EXPECT(occt_bridge_create_polyline_tube(session, stalled_path, 3, 0.5, &out), ARG);

    EXPECT(occt_bridge_create_loft(session, sections, counts, 2, 1, 0, NULL), ARG);
    EXPECT(occt_bridge_create_loft(session, NULL, counts, 2, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_loft(session, sections, NULL, 2, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_loft(session, sections, counts, 1, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_loft(session, sections, counts, 2, 2, 0, &out), ARG);
    EXPECT(occt_bridge_create_loft(session, sections, counts, 2, 1, 2, &out), ARG);
    EXPECT(occt_bridge_create_loft(session, sections, short_counts, 2, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_loft(session, sections, overflowing_counts, 2, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_loft(session, bad_sections, counts, 2, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_loft(session, sections, counts, 2, 0, 1, &out), OK);
}

static void combinations_and_features(occt_bridge_session_t* session) {
    const occt_bridge_vec3_t zero = vec(0, 0, 0);
    const occt_bridge_vec3_t up = vec(0, 0, 1);
    const occt_bridge_vec3_t nan_x = vec(NAN, 0, 0);
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;

    const occt_bridge_shape_id_t box =
        SHAPE(occt_bridge_create_box(session, zero, vec(4, 4, 4), &out), out);
    const occt_bridge_shape_id_t other =
        SHAPE(occt_bridge_create_box(session, vec(10, 0, 0), vec(1, 1, 1), &out), out);
    const occt_bridge_shape_id_t edge = subshape(session, box, OCCT_BRIDGE_SHAPE_EDGE, 0);
    const occt_bridge_shape_id_t face = subshape(session, box, OCCT_BRIDGE_SHAPE_FACE, 0);
    const occt_bridge_shape_id_t other_edge = subshape(session, other, OCCT_BRIDGE_SHAPE_EDGE, 0);
    const occt_bridge_shape_id_t other_face = subshape(session, other, OCCT_BRIDGE_SHAPE_FACE, 0);
    const occt_bridge_shape_id_t missing_list[] = {unknown};
    const occt_bridge_shape_id_t edges[] = {edge};
    const occt_bridge_shape_id_t faces[] = {face};
    const occt_bridge_shape_id_t wrong_edges[] = {face};
    const occt_bridge_shape_id_t foreign_edges[] = {other_edge};
    const occt_bridge_shape_id_t wrong_faces[] = {edge};
    const occt_bridge_shape_id_t foreign_faces[] = {other_face};

    EXPECT(occt_bridge_create_compound(session, edges, 1, NULL), ARG);
    EXPECT(occt_bridge_create_compound(session, NULL, 1, &out), ARG);
    EXPECT(occt_bridge_create_compound(session, edges, 0, &out), ARG);
    EXPECT(occt_bridge_create_compound(session, missing_list, 1, &out), MISSING);

    EXPECT(occt_bridge_sew(session, faces, 1, 1e-6, NULL), ARG);
    EXPECT(occt_bridge_sew(session, NULL, 1, 1e-6, &out), ARG);
    EXPECT(occt_bridge_sew(session, faces, 0, 1e-6, &out), ARG);
    EXPECT(occt_bridge_sew(session, faces, 1, NAN, &out), ARG);
    EXPECT(occt_bridge_sew(session, faces, 1, 0, &out), ARG);
    EXPECT(occt_bridge_sew(session, missing_list, 1, 1e-6, &out), MISSING);
    EXPECT(occt_bridge_sew(session, wrong_faces, 1, 1e-6, &out), GEOMETRY);
    EXPECT(occt_bridge_make_solid(session, box, NULL), ARG);
    EXPECT(occt_bridge_make_solid(session, unknown, &out), MISSING);
    EXPECT(occt_bridge_make_solid(session, face, &out), GEOMETRY);
    EXPECT(occt_bridge_make_solid(session, box, &out), OK);
    EXPECT(occt_bridge_make_solid_from_shells(session, faces, 1, NULL), ARG);
    EXPECT(occt_bridge_make_solid_from_shells(session, NULL, 1, &out), ARG);
    EXPECT(occt_bridge_make_solid_from_shells(session, faces, 0, &out), ARG);
    EXPECT(occt_bridge_make_solid_from_shells(session, missing_list, 1, &out), MISSING);
    EXPECT(occt_bridge_make_solid_from_shells(session, faces, 1, &out), GEOMETRY);
    EXPECT(occt_bridge_make_solid_from_shells(session, &box, 1, &out), OK);

    EXPECT(occt_bridge_fuse(session, box, other, NULL), ARG);
    EXPECT(occt_bridge_fuse(session, unknown, other, &out), MISSING);
    EXPECT(occt_bridge_cut(session, box, unknown, &out), MISSING);
    EXPECT(occt_bridge_common(session, unknown, unknown, &out), MISSING);

    EXPECT(occt_bridge_fillet(session, box, edges, 1, 0.5, NULL), ARG);
    EXPECT(occt_bridge_fillet(session, box, NULL, 1, 0.5, &out), ARG);
    EXPECT(occt_bridge_fillet(session, box, edges, 0, 0.5, &out), ARG);
    EXPECT(occt_bridge_fillet(session, box, edges, 1, NAN, &out), ARG);
    EXPECT(occt_bridge_fillet(session, box, edges, 1, 0, &out), ARG);
    EXPECT(occt_bridge_fillet(session, unknown, edges, 1, 0.5, &out), MISSING);
    EXPECT(occt_bridge_fillet(session, box, missing_list, 1, 0.5, &out), MISSING);
    EXPECT(occt_bridge_fillet(session, box, wrong_edges, 1, 0.5, &out), GEOMETRY);
    EXPECT(occt_bridge_fillet(session, box, foreign_edges, 1, 0.5, &out), GEOMETRY);

    EXPECT(occt_bridge_chamfer(session, box, edges, 1, 0.5, NULL), ARG);
    EXPECT(occt_bridge_chamfer(session, box, NULL, 1, 0.5, &out), ARG);
    EXPECT(occt_bridge_chamfer(session, box, edges, 0, 0.5, &out), ARG);
    EXPECT(occt_bridge_chamfer(session, box, edges, 1, NAN, &out), ARG);
    EXPECT(occt_bridge_chamfer(session, box, edges, 1, 0, &out), ARG);
    EXPECT(occt_bridge_chamfer(session, unknown, edges, 1, 0.5, &out), MISSING);
    EXPECT(occt_bridge_chamfer(session, box, missing_list, 1, 0.5, &out), MISSING);
    EXPECT(occt_bridge_chamfer(session, box, wrong_edges, 1, 0.5, &out), GEOMETRY);
    EXPECT(occt_bridge_chamfer(session, box, foreign_edges, 1, 0.5, &out), GEOMETRY);

    EXPECT(occt_bridge_offset(session, box, 0.5, 1e-6, NULL), ARG);
    EXPECT(occt_bridge_offset(session, box, NAN, 1e-6, &out), ARG);
    EXPECT(occt_bridge_offset(session, box, 0, 1e-6, &out), ARG);
    EXPECT(occt_bridge_offset(session, box, 0.5, NAN, &out), ARG);
    EXPECT(occt_bridge_offset(session, box, 0.5, 0, &out), ARG);
    EXPECT(occt_bridge_offset(session, unknown, 0.5, 1e-6, &out), MISSING);

    EXPECT(occt_bridge_hollow(session, box, faces, 1, -0.5, 1e-6, NULL), ARG);
    EXPECT(occt_bridge_hollow(session, box, NULL, 1, -0.5, 1e-6, &out), ARG);
    EXPECT(occt_bridge_hollow(session, box, faces, 0, -0.5, 1e-6, &out), ARG);
    EXPECT(occt_bridge_hollow(session, box, faces, 1, NAN, 1e-6, &out), ARG);
    EXPECT(occt_bridge_hollow(session, box, faces, 1, 0, 1e-6, &out), ARG);
    EXPECT(occt_bridge_hollow(session, box, faces, 1, -0.5, NAN, &out), ARG);
    EXPECT(occt_bridge_hollow(session, box, faces, 1, -0.5, 0, &out), ARG);
    EXPECT(occt_bridge_hollow(session, unknown, faces, 1, -0.5, 1e-6, &out), MISSING);
    EXPECT(occt_bridge_hollow(session, box, missing_list, 1, -0.5, 1e-6, &out), MISSING);
    EXPECT(occt_bridge_hollow(session, box, wrong_faces, 1, -0.5, 1e-6, &out), GEOMETRY);
    EXPECT(occt_bridge_hollow(session, box, foreign_faces, 1, -0.5, 1e-6, &out), GEOMETRY);

    EXPECT(occt_bridge_translate(session, box, up, NULL), ARG);
    EXPECT(occt_bridge_translate(session, box, nan_x, &out), ARG);
    EXPECT(occt_bridge_translate(session, unknown, up, &out), MISSING);
    EXPECT(occt_bridge_rotate(session, box, nan_x, up, 1, &out), ARG);
    EXPECT(occt_bridge_rotate(session, box, zero, nan_x, 1, &out), ARG);
    EXPECT(occt_bridge_rotate(session, box, zero, up, NAN, &out), ARG);
    EXPECT(occt_bridge_rotate(session, box, zero, zero, 1, &out), ARG);
    EXPECT(occt_bridge_scale(session, box, nan_x, 2, &out), ARG);
    EXPECT(occt_bridge_scale(session, box, zero, NAN, &out), ARG);
    EXPECT(occt_bridge_scale(session, box, zero, 0, &out), ARG);
    EXPECT(occt_bridge_shape_duplicate(session, box, NULL), ARG);
    EXPECT(occt_bridge_shape_duplicate(session, unknown, &out), MISSING);
}

static void queries(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;
    occt_bridge_shape_type_t type = 0;
    occt_bridge_bounds_t bounds;
    occt_bridge_vec3_t point;
    size_t count = 0;
    double value = 0.0;
    double second_value = 0.0;
    double third_value = 0.0;
    double fourth_value = 0.0;
    int flag = 0;

    const occt_bridge_shape_id_t box =
        SHAPE(occt_bridge_create_box(session, vec(0, 0, 0), vec(1, 1, 1), &out), out);
    const occt_bridge_shape_id_t cylinder = SHAPE(
        occt_bridge_create_cylinder(session, vec(0, 0, 0), vec(0, 0, 1), 1, 2, &out), out);
    const occt_bridge_shape_id_t shapes[] = {box};
    const occt_bridge_shape_id_t compound =
        SHAPE(occt_bridge_create_compound(session, shapes, 1, &out), out);
    const occt_bridge_shape_id_t face = subshape(session, box, OCCT_BRIDGE_SHAPE_FACE, 0);
    const occt_bridge_shape_id_t edge = subshape(session, box, OCCT_BRIDGE_SHAPE_EDGE, 0);
    const occt_bridge_shape_id_t vertex = subshape(session, box, OCCT_BRIDGE_SHAPE_VERTEX, 0);

    /* Every public topology kind maps in both directions. */
    for (occt_bridge_shape_type_t kind = OCCT_BRIDGE_SHAPE_COMPOUND; kind <= OCCT_BRIDGE_SHAPE_VERTEX; ++kind) {
        EXPECT(occt_bridge_shape_subshape_count(session, compound, kind, &count), OK);
        if (count > 0) {
            const occt_bridge_shape_id_t part = subshape(session, compound, kind, 0);
            EXPECT(occt_bridge_shape_type(session, part, &type), OK);
            if (type != kind) {
                (void)fprintf(stderr, "subshape kind %d reported as %d\n", kind, type);
                ++failures;
            }
        }
    }
    EXPECT(occt_bridge_shape_type(session, box, NULL), ARG);
    EXPECT(occt_bridge_shape_type(session, unknown, &type), MISSING);
    EXPECT(occt_bridge_shape_subshape_count(session, box, OCCT_BRIDGE_SHAPE_FACE, NULL), ARG);
    EXPECT(occt_bridge_shape_subshape_count(session, box, 99, &count), ARG);
    EXPECT(occt_bridge_shape_subshape_count(session, unknown, OCCT_BRIDGE_SHAPE_FACE, &count), MISSING);
    EXPECT(occt_bridge_shape_subshape_at(session, box, OCCT_BRIDGE_SHAPE_FACE, 0, NULL), ARG);
    EXPECT(occt_bridge_shape_subshape_at(session, box, 99, 0, &out), ARG);
    EXPECT(occt_bridge_shape_subshape_at(session, unknown, OCCT_BRIDGE_SHAPE_FACE, 0, &out), MISSING);
    EXPECT(occt_bridge_shape_subshape_at(session, box, OCCT_BRIDGE_SHAPE_FACE, 6, &out), ARG);

    EXPECT(occt_bridge_shape_bounds(session, box, NULL), ARG);
    EXPECT(occt_bridge_shape_bounds(session, unknown, &bounds), MISSING);
    EXPECT(occt_bridge_shape_exact_bounds(session, box, NULL), ARG);
    EXPECT(occt_bridge_shape_exact_bounds(session, unknown, &bounds), MISSING);
    EXPECT(occt_bridge_shape_surface_area(session, box, NULL), ARG);
    EXPECT(occt_bridge_shape_surface_area(session, unknown, &value), MISSING);
    EXPECT(occt_bridge_shape_surface_area(session, edge, &value), GEOMETRY);
    EXPECT(occt_bridge_shape_volume(session, box, NULL), ARG);
    EXPECT(occt_bridge_shape_volume(session, unknown, &value), MISSING);
    EXPECT(occt_bridge_shape_volume(session, face, &value), GEOMETRY);
    EXPECT(occt_bridge_shape_center_of_mass(session, box, NULL), ARG);
    EXPECT(occt_bridge_shape_center_of_mass(session, unknown, &point), MISSING);
    EXPECT(occt_bridge_shape_center_of_mass(session, vertex, &point), GEOMETRY);
    EXPECT(occt_bridge_shape_center_of_mass(session, edge, &point), OK);

    EXPECT(occt_bridge_shape_face_normal(session, face, NULL), ARG);
    EXPECT(occt_bridge_shape_face_normal(session, unknown, &point), MISSING);
    EXPECT(occt_bridge_shape_face_normal(session, edge, &point), GEOMETRY);
    EXPECT(occt_bridge_shape_face_is_planar(session, face, NULL), ARG);
    EXPECT(occt_bridge_shape_face_is_planar(session, unknown, &flag), MISSING);
    EXPECT(occt_bridge_shape_face_is_planar(session, edge, &flag), GEOMETRY);
    const occt_bridge_shape_id_t lateral = subshape(session, cylinder, OCCT_BRIDGE_SHAPE_FACE, 0);
    EXPECT(occt_bridge_shape_face_is_planar(session, lateral, &flag), OK);
    if (flag != 0) {
        (void)fprintf(stderr, "cylinder lateral face reported planar\n");
        ++failures;
    }

    EXPECT(occt_bridge_shape_edge_length(session, edge, NULL), ARG);
    EXPECT(occt_bridge_shape_edge_length(session, unknown, &value), MISSING);
    EXPECT(occt_bridge_shape_edge_length(session, face, &value), GEOMETRY);
    EXPECT(occt_bridge_shape_edge_circle_radius(session, edge, NULL, &value), ARG);
    EXPECT(occt_bridge_shape_edge_circle_radius(session, edge, &flag, NULL), ARG);
    EXPECT(occt_bridge_shape_edge_circle_radius(session, unknown, &flag, &value), MISSING);
    EXPECT(occt_bridge_shape_edge_circle_radius(session, face, &flag, &value), GEOMETRY);
    EXPECT(occt_bridge_shape_edge_curvature(session, edge, NULL, &value), ARG);
    EXPECT(occt_bridge_shape_edge_curvature(session, edge, &flag, NULL), ARG);
    EXPECT(occt_bridge_shape_edge_curvature(session, unknown, &flag, &value), MISSING);
    EXPECT(occt_bridge_shape_edge_curvature_range(session, edge, 5, NULL, &value), ARG);
    EXPECT(occt_bridge_shape_edge_curvature_range(session, edge, 5, &value, NULL), ARG);
    EXPECT(occt_bridge_shape_edge_curvature_range(session, edge, 1, &value, &second_value), ARG);
    EXPECT(occt_bridge_shape_edge_curvature_range(session, edge, 100001, &value, &second_value), ARG);
    EXPECT(occt_bridge_shape_edge_curvature_range(session, unknown, 5, &value, &second_value), MISSING);

    EXPECT(occt_bridge_shape_edge_curvature_extrema(
               session, edge, 1e-6, NULL, &second_value, &third_value, &fourth_value, &flag), ARG);
    EXPECT(occt_bridge_shape_edge_curvature_extrema(
               session, edge, 1e-6, &value, NULL, &third_value, &fourth_value, &flag), ARG);
    EXPECT(occt_bridge_shape_edge_curvature_extrema(
               session, edge, 1e-6, &value, &second_value, NULL, &fourth_value, &flag), ARG);
    EXPECT(occt_bridge_shape_edge_curvature_extrema(
               session, edge, 1e-6, &value, &second_value, &third_value, NULL, &flag), ARG);
    EXPECT(occt_bridge_shape_edge_curvature_extrema(
               session, edge, 1e-6, &value, &second_value, &third_value, &fourth_value, NULL), ARG);
    EXPECT(occt_bridge_shape_edge_curvature_extrema(
               session, edge, NAN, &value, &second_value, &third_value, &fourth_value, &flag), ARG);
    EXPECT(occt_bridge_shape_edge_curvature_extrema(
               session, unknown, 1e-6, &value, &second_value, &third_value, &fourth_value, &flag), MISSING);

    EXPECT(occt_bridge_shape_is_valid(session, box, NULL), ARG);
    EXPECT(occt_bridge_shape_is_valid(session, unknown, &flag), MISSING);
    EXPECT(occt_bridge_shape_remove(session, unknown), MISSING);
}

static void topology_relations(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;
    int flag = 0;

    const occt_bridge_shape_id_t box =
        SHAPE(occt_bridge_create_box(session, vec(0, 0, 0), vec(1, 1, 1), &out), out);
    const occt_bridge_shape_id_t other =
        SHAPE(occt_bridge_create_box(session, vec(5, 0, 0), vec(1, 1, 1), &out), out);
    const occt_bridge_shape_id_t face = subshape(session, box, OCCT_BRIDGE_SHAPE_FACE, 0);
    const occt_bridge_shape_id_t second_face = subshape(session, box, OCCT_BRIDGE_SHAPE_FACE, 1);
    const occt_bridge_shape_id_t edge = subshape(session, box, OCCT_BRIDGE_SHAPE_EDGE, 0);
    const occt_bridge_shape_id_t vertex = subshape(session, box, OCCT_BRIDGE_SHAPE_VERTEX, 0);
    const occt_bridge_shape_id_t foreign_face = subshape(session, other, OCCT_BRIDGE_SHAPE_FACE, 0);

    EXPECT(occt_bridge_shape_is_adjacent(session, box, face, edge, NULL), ARG);
    EXPECT(occt_bridge_shape_is_adjacent(session, unknown, face, edge, &flag), MISSING);
    EXPECT(occt_bridge_shape_is_adjacent(session, box, unknown, edge, &flag), MISSING);
    EXPECT(occt_bridge_shape_is_adjacent(session, box, face, unknown, &flag), MISSING);
    EXPECT(occt_bridge_shape_is_adjacent(session, box, foreign_face, edge, &flag), ARG);
    EXPECT(occt_bridge_shape_is_adjacent(session, box, face, foreign_face, &flag), ARG);
    EXPECT(occt_bridge_shape_is_adjacent(session, box, edge, face, &flag), OK);
    EXPECT(occt_bridge_shape_is_adjacent(session, box, edge, vertex, &flag), ARG);

    EXPECT(occt_bridge_shape_is_same(session, face, face, NULL), ARG);
    EXPECT(occt_bridge_shape_is_same(session, unknown, face, &flag), MISSING);
    EXPECT(occt_bridge_shape_is_same(session, face, unknown, &flag), MISSING);

    EXPECT(occt_bridge_shape_faces_are_tangent(session, box, face, second_face, NULL), ARG);
    EXPECT(occt_bridge_shape_faces_are_tangent(session, unknown, face, second_face, &flag), MISSING);
    EXPECT(occt_bridge_shape_faces_are_tangent(session, box, unknown, second_face, &flag), MISSING);
    EXPECT(occt_bridge_shape_faces_are_tangent(session, box, face, unknown, &flag), MISSING);
    EXPECT(occt_bridge_shape_faces_are_tangent(session, box, edge, second_face, &flag), ARG);
    EXPECT(occt_bridge_shape_faces_are_tangent(session, box, foreign_face, second_face, &flag), ARG);
    EXPECT(occt_bridge_shape_faces_are_tangent(session, box, face, foreign_face, &flag), ARG);
    EXPECT(occt_bridge_shape_faces_are_tangent(session, box, face, face, &flag), OK);
}

static void history(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;
    size_t count = 0;
    int flag = 0;

    const occt_bridge_shape_id_t box =
        SHAPE(occt_bridge_create_box(session, vec(0, 0, 0), vec(2, 2, 2), &out), out);
    const occt_bridge_shape_id_t tool =
        SHAPE(occt_bridge_create_box(session, vec(1, 1, 1), vec(2, 2, 2), &out), out);
    const occt_bridge_shape_id_t outsider =
        SHAPE(occt_bridge_create_box(session, vec(9, 9, 9), vec(1, 1, 1), &out), out);
    const occt_bridge_shape_id_t result = SHAPE(occt_bridge_cut(session, box, tool, &out), out);
    const occt_bridge_history_relation_t modified = OCCT_BRIDGE_HISTORY_MODIFIED;

    EXPECT(occt_bridge_shape_history_count(session, result, box, modified, NULL), ARG);
    EXPECT(occt_bridge_shape_history_count(session, unknown, box, modified, &count), MISSING);
    EXPECT(occt_bridge_shape_history_count(session, result, unknown, modified, &count), MISSING);
    EXPECT(occt_bridge_shape_history_count(session, box, tool, modified, &count), ARG);
    EXPECT(occt_bridge_shape_history_count(session, result, outsider, modified, &count), ARG);
    EXPECT(occt_bridge_shape_history_count(session, result, box, 99, &count), ARG);

    EXPECT(occt_bridge_shape_history_at(session, result, box, modified, 0, NULL), ARG);
    EXPECT(occt_bridge_shape_history_at(session, unknown, box, modified, 0, &out), MISSING);
    EXPECT(occt_bridge_shape_history_at(session, result, unknown, modified, 0, &out), MISSING);
    EXPECT(occt_bridge_shape_history_at(session, box, tool, modified, 0, &out), ARG);
    EXPECT(occt_bridge_shape_history_at(session, result, outsider, modified, 0, &out), ARG);
    EXPECT(occt_bridge_shape_history_at(session, result, box, 99, 0, &out), ARG);
    EXPECT(occt_bridge_shape_history_at(session, result, box, modified, 1000000, &out), ARG);

    EXPECT(occt_bridge_shape_history_is_deleted(session, result, box, NULL), ARG);
    EXPECT(occt_bridge_shape_history_is_deleted(session, unknown, box, &flag), MISSING);
    EXPECT(occt_bridge_shape_history_is_deleted(session, result, unknown, &flag), MISSING);
    EXPECT(occt_bridge_shape_history_is_deleted(session, box, tool, &flag), ARG);
    EXPECT(occt_bridge_shape_history_is_deleted(session, result, outsider, &flag), ARG);
}

static void persistence(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;
    const occt_bridge_shape_id_t box =
        SHAPE(occt_bridge_create_box(session, vec(0, 0, 0), vec(1, 1, 1), &out), out);

    EXPECT(occt_bridge_brep_save(session, box, NULL), ARG);
    EXPECT(occt_bridge_brep_save(session, box, ""), ARG);
    EXPECT(occt_bridge_brep_save(session, unknown, "unused.brep"), MISSING);
    EXPECT(occt_bridge_brep_save(session, box, "/nonexistent-directory/shape.brep"), IO);
    EXPECT(occt_bridge_brep_load(session, "unused.brep", NULL), ARG);
    EXPECT(occt_bridge_brep_load(session, NULL, &out), ARG);
    EXPECT(occt_bridge_brep_load(session, "", &out), ARG);
    EXPECT(occt_bridge_brep_load(session, "/nonexistent-directory/shape.brep", &out), IO);

    EXPECT(occt_bridge_step_save(session, box, NULL), ARG);
    EXPECT(occt_bridge_step_save(session, box, ""), ARG);
    EXPECT(occt_bridge_step_save(session, unknown, "unused.step"), MISSING);
    EXPECT(occt_bridge_step_save(session, box, "/nonexistent-directory/shape.step"), IO);
    EXPECT(occt_bridge_step_load(session, "unused.step", NULL), ARG);
    EXPECT(occt_bridge_step_load(session, NULL, &out), ARG);
    EXPECT(occt_bridge_step_load(session, "", &out), ARG);
    EXPECT(occt_bridge_step_load(session, "/nonexistent-directory/shape.step", &out), IO);
    FILE* malformed = fopen("c-api-malformed.step", "wb");
    if (malformed == NULL) {
        abort();
    }
    (void)fputs("not a STEP file\n", malformed);
    (void)fclose(malformed);
    EXPECT(occt_bridge_step_load(session, "c-api-malformed.step", &out), IO);
    if (remove("c-api-malformed.step") != 0) {
        abort();
    }

    EXPECT(occt_bridge_stl_save(session, box, NULL, 0.1, 0.5, 1), ARG);
    EXPECT(occt_bridge_stl_save(session, box, "", 0.1, 0.5, 1), ARG);
    EXPECT(occt_bridge_stl_save(session, unknown, "unused.stl", 0.1, 0.5, 1), MISSING);
    EXPECT(occt_bridge_stl_save(session, box, "unused.stl", 0.0, 0.5, 1), ARG);
    EXPECT(occt_bridge_stl_save(session, box, "unused.stl", -1.0, 0.5, 1), ARG);
    EXPECT(occt_bridge_stl_save(session, box, "unused.stl", NAN, 0.5, 1), ARG);
    EXPECT(occt_bridge_stl_save(session, box, "unused.stl", 0.1, 0.0, 1), ARG);
    EXPECT(occt_bridge_stl_save(session, box, "unused.stl", 0.1, INFINITY, 1), ARG);
    EXPECT(occt_bridge_stl_save(session, box, "unused.stl", 0.1, 0.5, 2), ARG);
    EXPECT(occt_bridge_stl_save(
        session, box, "/nonexistent-directory/shape.stl", 0.1, 0.5, 1), IO);
}

int main(void) {
    occt_bridge_session_t* session = NULL;
    if (occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION, &session) != OK) {
        (void)fprintf(stderr, "session creation failed\n");
        return EXIT_FAILURE;
    }
    session_and_diagnostics(session);
    primitives(session);
    wires_and_faces(session);
    recipes_sweeps_and_lofts(session);
    combinations_and_features(session);
    queries(session);
    topology_relations(session);
    history(session);
    persistence(session);
    occt_bridge_session_destroy(session);

    if (failures != 0) {
        (void)fprintf(stderr, "%d error-path expectation(s) failed\n", failures);
        return EXIT_FAILURE;
    }
    return EXIT_SUCCESS;
}
