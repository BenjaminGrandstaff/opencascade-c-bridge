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

static void expect_true(int condition, const char* message, int line) {
    if (!condition) {
        (void)fprintf(stderr, "line %d: %s\n", line, message);
        ++failures;
    }
}

#define EXPECT_TRUE(condition, message) expect_true((condition), (message), __LINE__)

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
    {
        occt_bridge_diagnostic_t diagnostic;
        occt_bridge_shape_id_t shape = UINT64_C(7);
        EXPECT(occt_bridge_session_diagnostic_count(NULL, &count), ARG);
        EXPECT(occt_bridge_session_diagnostic_count(session, NULL), ARG);
        EXPECT(occt_bridge_session_diagnostic_count(session, &count), OK);
        EXPECT(occt_bridge_session_diagnostic_at(NULL, 0, &diagnostic, &shape), ARG);
        EXPECT(occt_bridge_session_diagnostic_at(session, 0, NULL, &shape), ARG);
        EXPECT(occt_bridge_session_diagnostic_at(session, count, &diagnostic, &shape), ARG);
        if (shape != OCCT_BRIDGE_INVALID_SHAPE_ID) {
            (void)fprintf(stderr, "diagnostic_at must clear out_shape on a range error\n");
            ++failures;
        }
        if (occt_bridge_session_diagnostic_name(NULL, 0, buffer, sizeof(buffer)) != 0
            || occt_bridge_session_diagnostic_name(session, count, buffer, sizeof(buffer)) != 0) {
            (void)fprintf(stderr, "diagnostic_name without a session or diagnostic must be empty\n");
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

    occt_bridge_wire_segment_t segments[] = {
        {1, {1, 0, 0}, {0, 1, 0}, {-1, 0, 0}},
        {0, {-1, 0, 0}, {NAN, NAN, NAN}, {1, 0, 0}}
    };
    EXPECT(occt_bridge_create_segment_wire(NULL, segments, 2, 1, &out), ARG);
    EXPECT(occt_bridge_create_segment_wire(session, segments, 2, 1, NULL), ARG);
    EXPECT(occt_bridge_create_segment_wire(session, NULL, 2, 1, &out), ARG);
    EXPECT(occt_bridge_create_segment_wire(session, segments, 0, 1, &out), ARG);
    EXPECT(occt_bridge_create_segment_wire(session, segments, 2, 2, &out), ARG);
    EXPECT(occt_bridge_create_segment_wire(session, segments, 1, 1, &out), ARG);
    EXPECT(occt_bridge_create_segment_wire(session, segments, 1, 0, &out), OK);
    EXPECT(occt_bridge_create_segment_wire(session, segments, 2, 1, &out), OK);
    segments[0].kind = 2;
    EXPECT(occt_bridge_create_segment_wire(session, segments, 2, 1, &out), ARG);
    segments[0].kind = 1;
    segments[0].middle = nan_x;
    EXPECT(occt_bridge_create_segment_wire(session, segments, 2, 1, &out), ARG);
    segments[0].middle = zero;
    EXPECT(occt_bridge_create_segment_wire(session, segments, 2, 1, &out), GEOMETRY);
    segments[0].middle = vec(0, 1, 0);
    segments[1].start = zero;
    EXPECT(occt_bridge_create_segment_wire(session, segments, 2, 1, &out), ARG);

    /* A spline arch from (1,0,0) over (0,1,0) to (-1,0,0), closed by a line. */
    const occt_bridge_vec3_t curve_points[] = {{1, 0, 0}, {0, 1, 0}, {-1, 0, 0}, {-1, 0, 0}, {1, 0, 0}};
    occt_bridge_curve_segment_t curves[] = {
        {2, 0, 0, 3, {0, 0, 0}, {0, 0, 0}},
        {0, 0, 3, 2, {0, 0, 0}, {0, 0, 0}}
    };
    EXPECT(occt_bridge_create_curve_wire(NULL, curve_points, 5, curves, 2, 1, &out), ARG);
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 2, 1, NULL), ARG);
    EXPECT(occt_bridge_create_curve_wire(session, NULL, 5, curves, 2, 1, &out), ARG);
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, NULL, 2, 1, &out), ARG);
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 0, 1, &out), ARG);
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 2, 2, &out), ARG);
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 4, curves, 2, 1, &out), ARG);
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 1, 1, &out), ARG);
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 2, 1, &out), OK);
    curves[0].flags = 8;
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 2, 1, &out), ARG);
    curves[0].flags = 1;
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 2, 1, &out), ARG);
    curves[0].start_tangent = vec(0, 1, 0);
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 2, 1, &out), OK);
    curves[0].flags = 0;
    curves[1].point_count = 3;
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 2, 1, &out), ARG);
    curves[1].point_count = 2;
    curves[1].first_point = SIZE_MAX;
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 2, 1, &out), ARG);
    curves[1].first_point = 3;
    curves[0].kind = 3;
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 2, 1, &out), ARG);
    curves[0].kind = 2;
    curves[0].point_count = 1;
    EXPECT(occt_bridge_create_curve_wire(session, curve_points, 5, curves, 1, 0, &out), ARG);

    occt_bridge_shape_id_t rim = OCCT_BRIDGE_INVALID_SHAPE_ID;
    occt_bridge_shape_id_t path = OCCT_BRIDGE_INVALID_SHAPE_ID;
    const occt_bridge_vec3_t straight[] = {{0, 0, 0}, {10, 0, 0}};
    SHAPE(occt_bridge_create_circle_wire(session, zero, vec(1, 0, 0), 1, &rim), rim);
    SHAPE(occt_bridge_create_polyline_wire(session, straight, 2, 0, &path), path);
    EXPECT(occt_bridge_sweep(NULL, rim, path, OCCT_BRIDGE_SWEEP_CORRECTED_FRENET, zero, &out), ARG);
    EXPECT(occt_bridge_sweep(session, rim, path, OCCT_BRIDGE_SWEEP_CORRECTED_FRENET, zero, NULL), ARG);
    EXPECT(occt_bridge_sweep(session, rim, path, -1, zero, &out), ARG);
    EXPECT(occt_bridge_sweep(session, rim, path, 4, zero, &out), ARG);
    EXPECT(occt_bridge_sweep(session, rim, path, OCCT_BRIDGE_SWEEP_BINORMAL, zero, &out), ARG);
    EXPECT(occt_bridge_sweep(session, rim, path, OCCT_BRIDGE_SWEEP_BINORMAL, nan_x, &out), ARG);
    EXPECT(occt_bridge_sweep(session, unknown, path, OCCT_BRIDGE_SWEEP_FIXED, zero, &out), MISSING);
    EXPECT(occt_bridge_sweep(session, rim, unknown, OCCT_BRIDGE_SWEEP_FIXED, zero, &out), MISSING);
    EXPECT(occt_bridge_sweep(session, rim, path, OCCT_BRIDGE_SWEEP_BINORMAL, vec(0, 0, 1), &out), OK);
    segments[1].start = segments[1].end;
    EXPECT(occt_bridge_create_segment_wire(session, segments, 2, 1, &out), ARG);
    segments[0].start = nan_x;
    EXPECT(occt_bridge_create_segment_wire(session, segments, 1, 0, &out), ARG);
    segments[0].start = vec(1, 0, 0);
    segments[0].end = nan_x;
    EXPECT(occt_bridge_create_segment_wire(session, segments, 1, 0, &out), ARG);
    EXPECT(out, OCCT_BRIDGE_INVALID_SHAPE_ID);

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

    EXPECT(occt_bridge_create_revolve_from_face(NULL, face, zero, up, 1, &out), ARG);
    EXPECT(occt_bridge_create_revolve_from_face(session, face, zero, up, 1, NULL), ARG);
    EXPECT(occt_bridge_create_revolve_from_face(session, face, nan_x, up, 1, &out), ARG);
    EXPECT(occt_bridge_create_revolve_from_face(session, face, zero, nan_x, 1, &out), ARG);
    EXPECT(occt_bridge_create_revolve_from_face(session, face, zero, zero, 1, &out), ARG);
    EXPECT(occt_bridge_create_revolve_from_face(session, face, zero, up, NAN, &out), ARG);
    EXPECT(occt_bridge_create_revolve_from_face(session, face, zero, up, 0, &out), ARG);
    EXPECT(occt_bridge_create_revolve_from_face(session, face, zero, up, 7, &out), ARG);
    EXPECT(occt_bridge_create_revolve_from_face(session, face, zero, up, -7, &out), ARG);
    EXPECT(occt_bridge_create_revolve_from_face(session, unknown, zero, up, 1, &out), MISSING);
    EXPECT(occt_bridge_create_revolve_from_face(session, box, zero, up, 1, &out), GEOMETRY);

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
    EXPECT(occt_bridge_create_spline_loft(session, sections, counts, 2, 1, 0, NULL), ARG);
    EXPECT(occt_bridge_create_spline_loft(session, NULL, counts, 2, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_spline_loft(session, sections, counts, 1, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_spline_loft(session, sections, counts, 2, 1, 2, &out), ARG);
    EXPECT(occt_bridge_create_spline_loft(session, sections, overflowing_counts, 2, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_spline_loft(session, bad_sections, counts, 2, 1, 0, &out), ARG);
    EXPECT(occt_bridge_create_spline_loft(session, sections, counts, 2, 1, 1, &out), OK);
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

    EXPECT(occt_bridge_variable_fillet(NULL, box, edges, 1, 0.5, 1.0, &out), ARG);
    EXPECT(occt_bridge_variable_fillet(session, box, edges, 1, 0.5, 1.0, NULL), ARG);
    EXPECT(occt_bridge_variable_fillet(session, box, NULL, 1, 0.5, 1.0, &out), ARG);
    EXPECT(occt_bridge_variable_fillet(session, box, edges, 0, 0.5, 1.0, &out), ARG);
    EXPECT(occt_bridge_variable_fillet(session, box, edges, 1, NAN, 1.0, &out), ARG);
    EXPECT(occt_bridge_variable_fillet(session, box, edges, 1, 0.5, NAN, &out), ARG);
    EXPECT(occt_bridge_variable_fillet(session, box, edges, 1, 0, 1.0, &out), ARG);
    EXPECT(occt_bridge_variable_fillet(session, box, edges, 1, 0.5, 0, &out), ARG);
    EXPECT(occt_bridge_variable_fillet(session, unknown, edges, 1, 0.5, 1.0, &out), MISSING);
    EXPECT(occt_bridge_variable_fillet(session, box, missing_list, 1, 0.5, 1.0, &out), MISSING);
    EXPECT(occt_bridge_variable_fillet(session, box, wrong_edges, 1, 0.5, 1.0, &out), GEOMETRY);
    EXPECT(occt_bridge_variable_fillet(session, box, foreign_edges, 1, 0.5, 1.0, &out), GEOMETRY);
    {
        const occt_bridge_shape_id_t duplicate_edges[] = {edges[0], edges[0]};
        EXPECT(occt_bridge_variable_fillet(session, box, duplicate_edges, 2, 0.5, 1.0, &out), ARG);
        if (out != OCCT_BRIDGE_INVALID_SHAPE_ID) {
            ++failures;
        }
    }

    EXPECT(occt_bridge_chamfer(session, box, edges, 1, 0.5, NULL), ARG);
    EXPECT(occt_bridge_chamfer(session, box, NULL, 1, 0.5, &out), ARG);
    EXPECT(occt_bridge_chamfer(session, box, edges, 0, 0.5, &out), ARG);
    EXPECT(occt_bridge_chamfer(session, box, edges, 1, NAN, &out), ARG);
    EXPECT(occt_bridge_chamfer(session, box, edges, 1, 0, &out), ARG);
    EXPECT(occt_bridge_chamfer(session, unknown, edges, 1, 0.5, &out), MISSING);
    EXPECT(occt_bridge_chamfer(session, box, missing_list, 1, 0.5, &out), MISSING);
    EXPECT(occt_bridge_chamfer(session, box, wrong_edges, 1, 0.5, &out), GEOMETRY);
    EXPECT(occt_bridge_chamfer(session, box, foreign_edges, 1, 0.5, &out), GEOMETRY);

    EXPECT(occt_bridge_unify_same_domain(session, box, 1e-7, 1e-9, NULL), ARG);
    EXPECT(occt_bridge_unify_same_domain(session, box, NAN, 1e-9, &out), ARG);
    EXPECT(occt_bridge_unify_same_domain(session, box, 1e-7, 0.0, &out), ARG);
    EXPECT(occt_bridge_unify_same_domain(session, box, 1e-7, INFINITY, &out), ARG);
    EXPECT(occt_bridge_unify_same_domain(session, unknown, 1e-7, 1e-9, &out), MISSING);
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
    EXPECT(occt_bridge_mirror(session, box, zero, up, NULL), ARG);
    EXPECT(occt_bridge_mirror(session, unknown, zero, up, &out), MISSING);
    EXPECT(occt_bridge_mirror(session, box, nan_x, up, &out), ARG);
    EXPECT(occt_bridge_mirror(session, box, zero, nan_x, &out), ARG);
    EXPECT(occt_bridge_mirror(session, box, zero, zero, &out), ARG);
    EXPECT_TRUE(out == OCCT_BRIDGE_INVALID_SHAPE_ID, "failed mirror clears output");
    EXPECT(occt_bridge_mirror(session, box, zero, up, &out), OK);
    occt_bridge_shape_release(session, out);
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
    EXPECT(occt_bridge_shape_subshape_with_history(session, box, OCCT_BRIDGE_SHAPE_FACE, 0, NULL), ARG);
    EXPECT(occt_bridge_shape_subshape_with_history(session, box, 99, 0, &out), ARG);
    EXPECT(occt_bridge_shape_subshape_with_history(session, unknown, OCCT_BRIDGE_SHAPE_FACE, 0, &out), MISSING);
    EXPECT(occt_bridge_shape_subshape_with_history(session, box, OCCT_BRIDGE_SHAPE_FACE, 6, &out), ARG);
    EXPECT_TRUE(out == OCCT_BRIDGE_INVALID_SHAPE_ID, "failed history extraction clears output");
    EXPECT(occt_bridge_shape_subshape_with_history(session, box, OCCT_BRIDGE_SHAPE_FACE, 0, &out), OK);
    occt_bridge_shape_release(session, out);

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

    EXPECT(occt_bridge_shape_faces_are_tangent_within(session, box, face, second_face, 0.01, NULL), ARG);
    EXPECT(occt_bridge_shape_faces_are_tangent_within(session, box, face, second_face, 0.0, &flag), ARG);
    EXPECT(occt_bridge_shape_faces_are_tangent_within(session, box, face, second_face, -0.1, &flag), ARG);
    EXPECT(occt_bridge_shape_faces_are_tangent_within(session, box, face, second_face, 1.6, &flag), ARG);
    EXPECT(occt_bridge_shape_faces_are_tangent_within(session, box, face, second_face, NAN, &flag), ARG);
    EXPECT(occt_bridge_shape_faces_are_tangent_within(session, box, unknown, second_face, 0.01, &flag), MISSING);
    EXPECT(occt_bridge_shape_faces_are_tangent_within(session, box, edge, second_face, 0.01, &flag), ARG);
    EXPECT(occt_bridge_shape_faces_are_tangent_within(session, box, face, foreign_face, 0.01, &flag), ARG);
    flag = 1;
    EXPECT(occt_bridge_shape_faces_are_tangent_within(session, box, face, second_face, 0.01, &flag), OK);
    EXPECT_TRUE(flag == 0, "perpendicular box faces measured tangent");
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

    EXPECT(occt_bridge_shape_compose_history(NULL, result, box, &out), ARG);
    EXPECT(occt_bridge_shape_compose_history(session, result, box, NULL), ARG);
    EXPECT(occt_bridge_shape_compose_history(session, unknown, box, &out), MISSING);
    EXPECT(occt_bridge_shape_compose_history(session, result, unknown, &out), MISSING);
    EXPECT(occt_bridge_shape_compose_history(session, box, tool, &out), ARG);
    EXPECT(occt_bridge_shape_compose_history(session, result, box, &out), ARG);
    EXPECT(occt_bridge_shape_compose_history(session, result, result, &out), ARG);
    {
        const occt_bridge_shape_id_t moved =
            SHAPE(occt_bridge_translate(session, outsider, vec(1, 0, 0), &out), out);
        EXPECT(occt_bridge_shape_compose_history(session, result, moved, &out), ARG);
        if (out != OCCT_BRIDGE_INVALID_SHAPE_ID) {
            (void)fprintf(stderr, "history composition must clear failed output\n");
            ++failures;
        }
    }

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

static void open_profile(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;
    const occt_bridge_vec3_t points[] = { {0, 0, 0}, {4, 0, 0} };
    const occt_bridge_shape_id_t wire = SHAPE(occt_bridge_create_polyline_wire(session, points, 2, 0, &out), out);
    const occt_bridge_shape_id_t box = SHAPE(occt_bridge_create_box(session, vec(0,0,0), vec(1,1,1), &out), out);
    EXPECT(occt_bridge_create_open_profile_face(NULL, wire, vec(0,3,0), &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face(session, wire, vec(0,3,0), NULL), ARG);
    EXPECT(occt_bridge_create_open_profile_face(session, unknown, vec(0,3,0), &out), MISSING);
    EXPECT(occt_bridge_create_open_profile_face(session, box, vec(0,3,0), &out), GEOMETRY);
    EXPECT(occt_bridge_create_open_profile_face(session, wire, vec(0,0,0), &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face(session, wire, vec(NAN,3,0), &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face(session, wire, vec(0,INFINITY,0), &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face(session, wire, vec(0,3,NAN), &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face(session, wire, vec(1,0,0), &out), GEOMETRY);
    EXPECT(occt_bridge_create_open_profile_face(session, wire, vec(0,3,0), &out), OK);
}

static void open_profile_to_next(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;
    const occt_bridge_vec3_t points[] = { {0, 2, 3}, {4, 2, 3} };
    const occt_bridge_shape_id_t wire = SHAPE(occt_bridge_create_polyline_wire(session, points, 2, 0, &out), out);
    const occt_bridge_shape_id_t box = SHAPE(occt_bridge_create_box(session, vec(0,0,0), vec(4,4,1), &out), out);
    EXPECT(occt_bridge_create_open_profile_face_to_next(NULL, wire, box, vec(0,0,-1), 3, &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, box, vec(0,0,-1), 3, NULL), ARG);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, unknown, box, vec(0,0,-1), 3, &out), MISSING);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, unknown, vec(0,0,-1), 3, &out), MISSING);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, box, box, vec(0,0,-1), 3, &out), GEOMETRY);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, wire, vec(0,0,-1), 3, &out), GEOMETRY);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, box, vec(0,0,0), 3, &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, box, vec(NAN,0,-1), 3, &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, box, vec(0,INFINITY,-1), 3, &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, box, vec(0,0,NAN), 3, &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, box, vec(0,0,-1), NAN, &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, box, vec(0,0,-1), 0, &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, box, vec(0,0,-1), INFINITY, &out), ARG);
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, box, vec(0,0,-1), 1, &out), GEOMETRY);
    if (out != OCCT_BRIDGE_INVALID_SHAPE_ID) {
        ++failures;
    }
    EXPECT(occt_bridge_create_open_profile_face_to_next(session, wire, box, vec(0,0,-1), 2, &out), OK);
}

static void station_fillet(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;
    const occt_bridge_shape_id_t body = SHAPE(occt_bridge_create_box(session, vec(0,0,0), vec(10,10,10), &out), out);
    const occt_bridge_shape_id_t edge = subshape(session, body, OCCT_BRIDGE_SHAPE_EDGE, 0);
    const occt_bridge_fillet_station_t good[] = { {0,1}, {0.5,2}, {1,1.5} };
    EXPECT(occt_bridge_variable_fillet_stations(NULL,body,&edge,1,good,3,0,vec(0,0,0),&out),ARG);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&edge,1,good,3,0,vec(0,0,0),NULL),ARG);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,NULL,1,good,3,0,vec(0,0,0),&out),ARG);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&edge,0,good,3,0,vec(0,0,0),&out),ARG);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&edge,1,NULL,3,0,vec(0,0,0),&out),ARG);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&edge,1,good,1,0,vec(0,0,0),&out),ARG);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&edge,1,good,SIZE_MAX,0,vec(0,0,0),&out),ARG);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&edge,1,good,3,-1,vec(0,0,0),&out),ARG);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&edge,1,good,3,3,vec(0,0,0),&out),ARG);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&edge,1,good,3,2,vec(NAN,0,0),&out),ARG);
    EXPECT(occt_bridge_variable_fillet_stations(session,unknown,&edge,1,good,3,0,vec(0,0,0),&out),MISSING);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&unknown,1,good,3,0,vec(0,0,0),&out),MISSING);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&body,1,good,3,0,vec(0,0,0),&out),GEOMETRY);
    EXPECT(occt_bridge_variable_fillet_stations(session,body,&edge,1,good,3,0,vec(0,0,0),&out),OK);
}

static void measurements(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t out = OCCT_BRIDGE_INVALID_SHAPE_ID;
    const occt_bridge_shape_id_t body = SHAPE(occt_bridge_create_box(session, vec(0,0,0), vec(2,3,4), &out), out);
    const occt_bridge_shape_id_t edge = subshape(session,body,OCCT_BRIDGE_SHAPE_EDGE,0);
    occt_bridge_mass_properties_t properties;
    occt_bridge_distance_result_t distance;
    double volume = 0;
    EXPECT(occt_bridge_shape_mass_properties(NULL,body,&properties),ARG);
    EXPECT(occt_bridge_shape_mass_properties(session,body,NULL),ARG);
    EXPECT(occt_bridge_shape_mass_properties(session,unknown,&properties),MISSING);
    EXPECT(occt_bridge_shape_mass_properties(session,edge,&properties),GEOMETRY);
    EXPECT(occt_bridge_shape_mass_properties(session,body,&properties),OK);
    EXPECT(occt_bridge_shape_distance(NULL,body,body,&distance),ARG);
    EXPECT(occt_bridge_shape_distance(session,body,body,NULL),ARG);
    EXPECT(occt_bridge_shape_distance(session,unknown,body,&distance),MISSING);
    EXPECT(occt_bridge_shape_distance(session,body,unknown,&distance),MISSING);
    EXPECT(occt_bridge_shape_distance(session,body,edge,&distance),OK);
    EXPECT(occt_bridge_shape_overlap_volume(NULL,body,body,&volume),ARG);
    EXPECT(occt_bridge_shape_overlap_volume(session,body,body,NULL),ARG);
    EXPECT(occt_bridge_shape_overlap_volume(session,unknown,body,&volume),MISSING);
    EXPECT(occt_bridge_shape_overlap_volume(session,body,unknown,&volume),MISSING);
    EXPECT(occt_bridge_shape_overlap_volume(session,edge,body,&volume),GEOMETRY);
    EXPECT(occt_bridge_shape_overlap_volume(session,body,edge,&volume),GEOMETRY);
    EXPECT(occt_bridge_shape_overlap_volume(session,body,body,&volume),OK);

    occt_bridge_face_radius_bounds_t bounds[6];
    occt_bridge_edge_concavity_t concavities[12];
    size_t count = 99;
    EXPECT(occt_bridge_shape_face_radius_bounds(NULL,body,8,bounds,6,&count),ARG);
    EXPECT(occt_bridge_shape_face_radius_bounds(session,body,8,bounds,6,NULL),ARG);
    EXPECT(occt_bridge_shape_face_radius_bounds(session,body,8,NULL,6,&count),ARG);
    EXPECT(occt_bridge_shape_face_radius_bounds(session,body,1,bounds,6,&count),ARG);
    EXPECT(occt_bridge_shape_face_radius_bounds(session,body,1025,bounds,6,&count),ARG);
    EXPECT(occt_bridge_shape_face_radius_bounds(session,unknown,8,bounds,6,&count),MISSING);
    EXPECT(occt_bridge_shape_face_radius_bounds(session,body,8,bounds,5,&count),ARG);
    EXPECT_TRUE(count == 6, "insufficient capacity reports the face count");
    count = 0;
    EXPECT(occt_bridge_shape_face_radius_bounds(session,body,8,NULL,0,&count),OK);
    EXPECT_TRUE(count == 6, "a null buffer queries the face count");
    EXPECT(occt_bridge_shape_face_radius_bounds(session,body,8,bounds,6,&count),OK);
    EXPECT_TRUE(isinf(bounds[0].convex_radius) && isinf(bounds[0].concave_radius)
        && bounds[0].exact == 1 && bounds[0].samples == 0, "planar faces are exact and flat");
    occt_bridge_face_pull_range_t pulls[6];
    const occt_bridge_vec3_t up = {0, 0, 2};
    EXPECT(occt_bridge_shape_face_pull_ranges(NULL,body,up,pulls,6,&count),ARG);
    EXPECT(occt_bridge_shape_face_pull_ranges(session,body,up,pulls,6,NULL),ARG);
    EXPECT(occt_bridge_shape_face_pull_ranges(session,body,up,NULL,6,&count),ARG);
    EXPECT(occt_bridge_shape_face_pull_ranges(session,body,(occt_bridge_vec3_t){0,0,0},pulls,6,&count),ARG);
    EXPECT(occt_bridge_shape_face_pull_ranges(session,body,(occt_bridge_vec3_t){NAN,0,1},pulls,6,&count),ARG);
    EXPECT(occt_bridge_shape_face_pull_ranges(session,unknown,up,pulls,6,&count),MISSING);
    EXPECT(occt_bridge_shape_face_pull_ranges(session,body,up,pulls,5,&count),ARG);
    EXPECT_TRUE(count == 6, "insufficient capacity reports the face count");
    count = 0;
    EXPECT(occt_bridge_shape_face_pull_ranges(session,body,up,NULL,0,&count),OK);
    EXPECT_TRUE(count == 6, "a null buffer queries the face count");
    EXPECT(occt_bridge_shape_face_pull_ranges(session,body,up,pulls,6,&count),OK);
    {
        int sides = 0;
        int caps = 0;
        for (size_t index = 0; index < 6; ++index) {
            EXPECT_TRUE(pulls[index].exact == 1 && pulls[index].minimum == pulls[index].maximum,
                "box faces have exact constant pull projections");
            sides += fabs(pulls[index].minimum) < 1e-12 ? 1 : 0;
            caps += fabs(fabs(pulls[index].minimum) - 1.0) < 1e-12 ? 1 : 0;
        }
        EXPECT_TRUE(sides == 4 && caps == 2, "a box has four sides along the pull and two caps");
    }
    EXPECT(occt_bridge_shape_edge_concavities(NULL,body,1e-6,concavities,12,&count),ARG);
    EXPECT(occt_bridge_shape_edge_concavities(session,body,1e-6,concavities,12,NULL),ARG);
    EXPECT(occt_bridge_shape_edge_concavities(session,body,1e-6,NULL,12,&count),ARG);
    EXPECT(occt_bridge_shape_edge_concavities(session,body,0.0,concavities,12,&count),ARG);
    EXPECT(occt_bridge_shape_edge_concavities(session,body,NAN,concavities,12,&count),ARG);
    EXPECT(occt_bridge_shape_edge_concavities(session,body,2.0,concavities,12,&count),ARG);
    EXPECT(occt_bridge_shape_edge_concavities(session,unknown,1e-6,concavities,12,&count),MISSING);
    EXPECT(occt_bridge_shape_edge_concavities(session,body,1e-6,concavities,11,&count),ARG);
    EXPECT_TRUE(count == 12, "insufficient capacity reports the edge count");
    EXPECT(occt_bridge_shape_edge_concavities(session,body,1e-6,concavities,12,&count),OK);
    for (size_t index = 0; index < 12; ++index) {
        EXPECT_TRUE(concavities[index] == OCCT_BRIDGE_EDGE_CONVEX, "box edges are convex");
    }
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

static void draft(occt_bridge_session_t* session) {
    const occt_bridge_vec3_t zero = {0, 0, 0};
    const occt_bridge_vec3_t up = {0, 0, 1};
    const occt_bridge_vec3_t nan_x = {NAN, 0, 1};
    occt_bridge_shape_id_t body = 0, face = 0, edge = 0, out = 0;
    EXPECT(occt_bridge_create_box(session, zero, vec(10, 10, 10), &body), OK);
    EXPECT(occt_bridge_shape_subshape_at(session, body, OCCT_BRIDGE_SHAPE_FACE, 0, &face), OK);
    EXPECT(occt_bridge_shape_subshape_at(session, body, OCCT_BRIDGE_SHAPE_EDGE, 0, &edge), OK);
    const occt_bridge_shape_id_t duplicate[] = {face, face};
    EXPECT(occt_bridge_draft(NULL, body, &face, 1, zero, up, up, 0.1, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 1, zero, up, up, 0.1, NULL), ARG);
    EXPECT(occt_bridge_draft(session, body, NULL, 1, zero, up, up, 0.1, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 0, zero, up, up, 0.1, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 1, nan_x, up, up, 0.1, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 1, zero, zero, up, 0.1, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 1, zero, nan_x, up, 0.1, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 1, zero, up, zero, 0.1, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 1, zero, up, nan_x, 0.1, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 1, zero, up, up, 0, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 1, zero, up, up, NAN, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 1, zero, up, up, 2, &out), ARG);
    EXPECT(occt_bridge_draft(session, unknown, &face, 1, zero, up, up, 0.1, &out), MISSING);
    EXPECT(occt_bridge_draft(session, body, &unknown, 1, zero, up, up, 0.1, &out), MISSING);
    EXPECT(occt_bridge_draft(session, body, &edge, 1, zero, up, up, 0.1, &out), GEOMETRY);
    EXPECT(occt_bridge_draft(session, body, duplicate, 2, zero, up, up, 0.1, &out), ARG);
    EXPECT(occt_bridge_draft(session, body, &face, 1, zero, up, up, 0.1, &out), OK);
    occt_bridge_shape_release(session, out);
    occt_bridge_shape_release(session, edge);
    occt_bridge_shape_release(session, face);
    occt_bridge_shape_release(session, body);
}

static void projection(occt_bridge_session_t* session) {
    const occt_bridge_vec3_t zero = {0, 0, 0};
    const occt_bridge_vec3_t up = {0, 0, 1};
    const occt_bridge_vec3_t right = {1, 0, 0};
    const occt_bridge_vec3_t nan_x = {NAN, 0, 0};
    occt_bridge_shape_id_t body = 0, edge = 0, visible = 0, hidden = 0;
    occt_bridge_vec3_t points[3];
    EXPECT(occt_bridge_create_box(session, zero, right, &body), ARG);
    EXPECT(occt_bridge_create_box(session, zero, (occt_bridge_vec3_t){1, 1, 1}, &body), OK);
    EXPECT(occt_bridge_orthographic_projection(NULL, body, zero, up, right, &visible, &hidden), ARG);
    EXPECT(occt_bridge_orthographic_projection(session, body, zero, up, right, NULL, &hidden), ARG);
    EXPECT(occt_bridge_orthographic_projection(session, body, zero, up, right, &visible, NULL), ARG);
    EXPECT(occt_bridge_orthographic_projection(session, body, zero, up, right, &visible, &visible), ARG);
    EXPECT(occt_bridge_orthographic_projection(session, body, nan_x, up, right, &visible, &hidden), ARG);
    EXPECT(occt_bridge_orthographic_projection(session, body, zero, zero, right, &visible, &hidden), ARG);
    EXPECT(occt_bridge_orthographic_projection(session, body, zero, up, zero, &visible, &hidden), ARG);
    EXPECT(occt_bridge_orthographic_projection(session, body, zero, up, up, &visible, &hidden), ARG);
    EXPECT(occt_bridge_orthographic_projection(session, UINT64_MAX, zero, up, right, &visible, &hidden), MISSING);
    EXPECT(occt_bridge_orthographic_projection(session, body, zero, up, right, &visible, &hidden), OK);
    EXPECT(occt_bridge_shape_subshape_at(session, visible, OCCT_BRIDGE_SHAPE_EDGE, 0, &edge), OK);
    EXPECT(occt_bridge_edge_sample_points(NULL, edge, 3, points), ARG);
    EXPECT(occt_bridge_edge_sample_points(session, edge, 3, NULL), ARG);
    EXPECT(occt_bridge_edge_sample_points(session, edge, 1, points), ARG);
    EXPECT(occt_bridge_edge_sample_points(session, edge, SIZE_MAX, points), ARG);
    EXPECT(occt_bridge_edge_sample_points(session, UINT64_MAX, 3, points), MISSING);
    EXPECT(occt_bridge_edge_sample_points(session, body, 3, points), ARG);
    EXPECT(occt_bridge_edge_sample_points(session, edge, 3, points), OK);
    occt_bridge_analytic_curve_t analytic = {0};
    EXPECT(occt_bridge_edge_analytic_curve(NULL, edge, &analytic), ARG);
    EXPECT(occt_bridge_edge_analytic_curve(session, edge, NULL), ARG);
    EXPECT(occt_bridge_edge_analytic_curve(session, UINT64_MAX, &analytic), MISSING);
    EXPECT(occt_bridge_edge_analytic_curve(session, body, &analytic), ARG);
    EXPECT(occt_bridge_edge_analytic_curve(session, edge, &analytic), OK);
    if (analytic.kind != 1) { ++failures; }
    size_t pole_count = 99;
    occt_bridge_bezier_pole_t poles[2];
    EXPECT(occt_bridge_edge_bezier_poles(NULL, edge, 10, NULL, 0, &pole_count), ARG);
    EXPECT_TRUE(pole_count == 0, "Bezier failed count is zero");
    EXPECT(occt_bridge_edge_bezier_poles(session, edge, 10, NULL, 0, NULL), ARG);
    EXPECT(occt_bridge_edge_bezier_poles(session, edge, 1, NULL, 0, &pole_count), ARG);
    EXPECT(occt_bridge_edge_bezier_poles(session, edge, 1000001, NULL, 0, &pole_count), ARG);
    EXPECT(occt_bridge_edge_bezier_poles(session, edge, 10, NULL, 1, &pole_count), ARG);
    EXPECT(occt_bridge_edge_bezier_poles(session, UINT64_MAX, 10, NULL, 0, &pole_count), MISSING);
    EXPECT(occt_bridge_edge_bezier_poles(session, body, 10, NULL, 0, &pole_count), ARG);
    EXPECT(occt_bridge_edge_bezier_poles(session, edge, 10, NULL, 0, &pole_count), OK);
    EXPECT_TRUE(pole_count == 2, "line Bezier count is two");
    poles[0].weight = -7;
    EXPECT(occt_bridge_edge_bezier_poles(session, edge, 10, poles, 1, &pole_count), ARG);
    EXPECT_TRUE(pole_count == 2 && poles[0].weight == -7, "undersized Bezier output is unchanged");
    EXPECT(occt_bridge_edge_bezier_poles(session, edge, 10, poles, 2, &pole_count), OK);
    EXPECT_TRUE(poles[0].span_index == 0 && poles[1].weight == 1, "line Bezier data");
    occt_bridge_shape_id_t clipped = 0;
    EXPECT(occt_bridge_clip_by_plane(NULL, body, zero, up, 1, &clipped), ARG);
    EXPECT(occt_bridge_clip_by_plane(session, body, zero, up, 1, NULL), ARG);
    EXPECT(occt_bridge_clip_by_plane(session, body, nan_x, up, 1, &clipped), ARG);
    EXPECT(occt_bridge_clip_by_plane(session, body, zero, zero, 1, &clipped), ARG);
    EXPECT(occt_bridge_clip_by_plane(session, body, zero, nan_x, 1, &clipped), ARG);
    EXPECT(occt_bridge_clip_by_plane(session, body, zero, up, -1, &clipped), ARG);
    EXPECT(occt_bridge_clip_by_plane(session, body, zero, up, 2, &clipped), ARG);
    EXPECT(occt_bridge_clip_by_plane(session, UINT64_MAX, zero, up, 1, &clipped), MISSING);
    EXPECT(occt_bridge_clip_by_plane(session, edge, zero, up, 1, &clipped), GEOMETRY);
    EXPECT(occt_bridge_clip_by_plane(session, body, zero, up, 1, &clipped), OK);
    occt_bridge_shape_release(session, clipped);
    size_t count = 0;
    occt_bridge_shape_id_t edges[12];
    for (size_t index = 0; index < 12; ++index) { edges[index] = UINT64_MAX; }
    EXPECT(occt_bridge_shape_subshapes(NULL, body, OCCT_BRIDGE_SHAPE_EDGE, NULL, 0, &count), ARG);
    EXPECT(occt_bridge_shape_subshapes(session, body, OCCT_BRIDGE_SHAPE_EDGE, NULL, 0, NULL), ARG);
    EXPECT(occt_bridge_shape_subshapes(session, body, 99, NULL, 0, &count), ARG);
    EXPECT(occt_bridge_shape_subshapes(session, UINT64_MAX, OCCT_BRIDGE_SHAPE_EDGE, NULL, 0, &count), MISSING);
    EXPECT(occt_bridge_shape_subshapes(session, body, OCCT_BRIDGE_SHAPE_EDGE, NULL, 1, &count), ARG);
    EXPECT(occt_bridge_shape_subshapes(session, body, OCCT_BRIDGE_SHAPE_EDGE, NULL, 0, &count), OK);
    if (count != 12) { ++failures; }
    EXPECT(occt_bridge_shape_subshapes(session, body, OCCT_BRIDGE_SHAPE_EDGE, edges, 1, &count), ARG);
    if (count != 12 || edges[0] != UINT64_MAX) { ++failures; }
    EXPECT(occt_bridge_shape_subshapes(session, body, OCCT_BRIDGE_SHAPE_EDGE, edges, 12, &count), OK);
    for (size_t index = 0; index < count; ++index) { occt_bridge_shape_release(session, edges[index]); }
    occt_bridge_shape_release(session, edge);
    occt_bridge_shape_release(session, visible);
    occt_bridge_shape_release(session, hidden);
    occt_bridge_shape_release(session, body);
}

static void surface_mesh(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t body = 0;
    EXPECT(occt_bridge_create_box(session, vec(0,0,0), vec(10,20,30), &body), OK);
    occt_bridge_mesh_options_t options = {0.1, 0.3, 1000000};
    size_t count = 123;
    occt_bridge_mesh_triangle_t triangles[12] = {{0}};
    triangles[0].face_index = 123;
    EXPECT(occt_bridge_surface_mesh(NULL, body, options, NULL, 0, &count), ARG);
    if (!(count == 0)) { ++failures; }
    EXPECT(occt_bridge_surface_mesh(session, body, options, NULL, 0, NULL), ARG);
    EXPECT(occt_bridge_surface_mesh(session, body, options, NULL, 1, &count), ARG);
    EXPECT(occt_bridge_surface_mesh(session, UINT64_MAX, options, NULL, 0, &count), MISSING);
    EXPECT(occt_bridge_surface_mesh(session, body, options, NULL, 0, &count), OK);
    if (!(count == 12)) { ++failures; }
    EXPECT(occt_bridge_surface_mesh(session, body, options, triangles, 1, &count), ARG);
    if (!(count == 12 && triangles[0].face_index == 123)) { ++failures; }
    EXPECT(occt_bridge_surface_mesh(session, body, options, triangles, 12, &count), OK);
    options.maximum_triangles = 1;
    triangles[0].face_index = 123;
    EXPECT(occt_bridge_surface_mesh(session, body, options, triangles, 12, &count), ARG);
    if (!(count == 0 && triangles[0].face_index == 123)) { ++failures; }
    occt_bridge_shape_id_t face = 0;
    EXPECT(occt_bridge_shape_subshape_at(session, body, OCCT_BRIDGE_SHAPE_FACE, 2, &face), OK);
    size_t index = 123;
    EXPECT(occt_bridge_subshape_indices(session, body, OCCT_BRIDGE_SHAPE_FACE, &face, 1, &index), OK);
    if (!(index == 2)) { ++failures; }
    index = 123;
    EXPECT(occt_bridge_subshape_indices(session, body, OCCT_BRIDGE_SHAPE_EDGE, &face, 1, &index), ARG);
    if (!(index == 123)) { ++failures; }
    EXPECT(occt_bridge_subshape_indices(session, body, 999, &face, 1, &index), ARG);
    EXPECT(occt_bridge_subshape_indices(session, body, OCCT_BRIDGE_SHAPE_FACE, NULL, 1, &index), ARG);
    EXPECT(occt_bridge_subshape_indices(session, body, OCCT_BRIDGE_SHAPE_FACE, &face, 1, NULL), ARG);
    EXPECT(occt_bridge_subshape_indices(session, UINT64_MAX, OCCT_BRIDGE_SHAPE_FACE, NULL, 0, NULL), MISSING);
    EXPECT(occt_bridge_subshape_indices(session, body, OCCT_BRIDGE_SHAPE_FACE, NULL, 0, NULL), OK);
    occt_bridge_shape_id_t invalid = UINT64_MAX;
    EXPECT(occt_bridge_subshape_indices(session, body, OCCT_BRIDGE_SHAPE_FACE, &invalid, 1, &index), MISSING);
    /* The lenient lookup reports a candidate outside the map instead of failing. */
    {
        const occt_bridge_shape_id_t both[] = {face, body};
        size_t found[2] = {123, 123};
        EXPECT(occt_bridge_subshape_lookup(session, body, OCCT_BRIDGE_SHAPE_FACE, both, 2, found), OK);
        EXPECT_TRUE(found[0] == 2 && found[1] == OCCT_BRIDGE_NOT_FOUND, "lookup marks the missing candidate");
        EXPECT(occt_bridge_subshape_lookup(session, body, OCCT_BRIDGE_SHAPE_FACE, &invalid, 1, found), MISSING);
        EXPECT(occt_bridge_subshape_lookup(session, body, 999, both, 2, found), ARG);
    }
    occt_bridge_shape_release(session, face);
    occt_bridge_shape_release(session, body);
}

static void sketch_edits(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t wire=0,box=0,out=0;
    const occt_bridge_vec3_t z=vec(0,0,0),normal=vec(0,0,1),axis=vec(1,0,0);
    const occt_bridge_vec3_t points[]={vec(0,0,0),vec(10,0,0)};
    EXPECT(occt_bridge_create_polyline_wire(session,points,2,0,&wire),OK);
    EXPECT(occt_bridge_create_box(session,z,vec(1,1,1),&box),OK);
    EXPECT(occt_bridge_create_ellipse_wire_axes(session,z,normal,axis,6,3,NULL),ARG);
    EXPECT(occt_bridge_create_ellipse_wire_axes(session,z,z,axis,6,3,&out),ARG);
    EXPECT(occt_bridge_create_ellipse_wire_axes(session,z,normal,normal,6,3,&out),ARG);
    EXPECT(occt_bridge_create_ellipse_wire_axes(session,z,normal,axis,2,3,&out),ARG);
    EXPECT(occt_bridge_create_ellipse_wire_axes(session,z,normal,axis,6,0,&out),ARG);
    EXPECT(occt_bridge_trim_curve(session,wire,0,1,NULL),ARG);
    EXPECT(occt_bridge_trim_curve(session,unknown,0,1,&out),MISSING);
    EXPECT(occt_bridge_trim_curve(session,wire,NAN,1,&out),ARG);
    EXPECT(occt_bridge_trim_curve(session,wire,-1,1,&out),ARG);
    EXPECT(occt_bridge_trim_curve(session,wire,0,2,&out),ARG);
    EXPECT(occt_bridge_trim_curve(session,wire,.8,.2,&out),ARG);
    EXPECT(occt_bridge_trim_curve(session,box,0,1,&out),ARG);
    EXPECT(occt_bridge_extend_curve(session,wire,1,1,NULL),ARG);
    EXPECT(occt_bridge_extend_curve(session,unknown,1,1,&out),MISSING);
    EXPECT(occt_bridge_extend_curve(session,wire,-1,1,&out),ARG);
    EXPECT(occt_bridge_extend_curve(session,wire,0,0,&out),ARG);
    EXPECT(occt_bridge_extend_curve(session,wire,NAN,1,&out),ARG);
    EXPECT(occt_bridge_extend_curve(session,box,1,1,&out),ARG);
    EXPECT(occt_bridge_join_wires(session,&wire,1,0,NULL),ARG);
    EXPECT(occt_bridge_join_wires(session,NULL,1,0,&out),ARG);
    EXPECT(occt_bridge_join_wires(session,&wire,0,0,&out),ARG);
    EXPECT(occt_bridge_join_wires(session,&wire,1,2,&out),ARG);
    EXPECT(occt_bridge_join_wires(session,&unknown,1,0,&out),MISSING);
    EXPECT(occt_bridge_join_wires(session,&box,1,0,&out),ARG);
    EXPECT(occt_bridge_offset_wire(session,wire,normal,1,0,NULL),ARG);
    EXPECT(occt_bridge_offset_wire(session,unknown,normal,1,0,&out),MISSING);
    EXPECT(occt_bridge_offset_wire(session,box,normal,1,0,&out),ARG);
    EXPECT(occt_bridge_offset_wire(session,wire,z,1,0,&out),ARG);
    EXPECT(occt_bridge_offset_wire(session,wire,normal,0,0,&out),ARG);
    EXPECT(occt_bridge_offset_wire(session,wire,normal,1,2,&out),ARG);
    EXPECT(occt_bridge_curve_closest_point(session,wire,z,NULL),ARG);
    occt_bridge_vec3_t nearest;
    EXPECT(occt_bridge_curve_closest_point(session,unknown,z,&nearest),MISSING);
    EXPECT(occt_bridge_curve_closest_point(session,wire,vec(NAN,0,0),&nearest),ARG);
    EXPECT(occt_bridge_curve_closest_point(session,wire,vec(5,3,0),&nearest),OK);
    EXPECT_TRUE(fabs(nearest.x-5)<1e-9 && fabs(nearest.y)<1e-9,"curve projection point");
    int closed=0;
    EXPECT(occt_bridge_wire_is_closed(session,wire,NULL),ARG);
    EXPECT(occt_bridge_wire_is_closed(session,unknown,&closed),MISSING);
    EXPECT(occt_bridge_wire_is_closed(session,box,&closed),ARG);
    EXPECT(occt_bridge_wire_is_closed(session,wire,&closed),OK);
    EXPECT_TRUE(closed==0,"line wire must be open");
    occt_bridge_shape_release(session,wire);occt_bridge_shape_release(session,box);
}

static void extrusion_limits(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t wire=0,profile=0,limit=0,box=0,out=0;
    const occt_bridge_vec3_t zero=vec(0,0,0),up=vec(0,0,1),travel=vec(0,0,30);
    EXPECT(occt_bridge_create_circle_wire(session,zero,up,2,&wire),OK);
    EXPECT(occt_bridge_create_face_from_wire(session,wire,&profile),OK);
    occt_bridge_shape_release(session,wire);
    EXPECT(occt_bridge_create_circle_wire(session,vec(0,0,10),up,10,&wire),OK);
    EXPECT(occt_bridge_create_face_from_wire(session,wire,&limit),OK);
    EXPECT(occt_bridge_create_box(session,zero,vec(1,1,1),&box),OK);
    EXPECT(occt_bridge_create_prism_until_face(session,profile,travel,limit,NULL),ARG);
    EXPECT(occt_bridge_create_prism_until_face(session,profile,zero,limit,&out),ARG);
    EXPECT(occt_bridge_create_prism_until_face(session,profile,vec(NAN,0,1),limit,&out),ARG);
    EXPECT(occt_bridge_create_prism_until_face(session,unknown,travel,limit,&out),MISSING);
    EXPECT(occt_bridge_create_prism_until_face(session,profile,travel,unknown,&out),MISSING);
    EXPECT(occt_bridge_create_prism_until_face(session,box,travel,limit,&out),GEOMETRY);
    EXPECT(occt_bridge_create_prism_until_face(session,profile,travel,box,&out),GEOMETRY);
    EXPECT(occt_bridge_create_prism_until_face(session,profile,travel,limit,&out),OK);
    double length=0;
    occt_bridge_vec3_t point;
    EXPECT(occt_bridge_shape_ray_first_hit(session,out,zero,up,30,NULL,&length),ARG);
    EXPECT(occt_bridge_shape_ray_first_hit(session,out,zero,up,30,&point,NULL),ARG);
    EXPECT(occt_bridge_shape_ray_first_hit(session,out,zero,zero,30,&point,&length),ARG);
    EXPECT(occt_bridge_shape_ray_first_hit(session,out,vec(NAN,0,0),up,30,&point,&length),ARG);
    EXPECT(occt_bridge_shape_ray_first_hit(session,out,zero,up,0,&point,&length),ARG);
    EXPECT(occt_bridge_shape_ray_first_hit(session,out,zero,up,NAN,&point,&length),ARG);
    EXPECT(occt_bridge_shape_ray_first_hit(session,unknown,zero,up,30,&point,&length),MISSING);
    EXPECT(occt_bridge_shape_ray_first_hit(session,out,zero,up,5,&point,&length),GEOMETRY);
    EXPECT_TRUE(length==0 && point.x==0 && point.y==0 && point.z==0,"failed ray clears outputs");
    EXPECT(occt_bridge_shape_ray_first_hit(session,out,zero,up,30,&point,&length),OK);
    EXPECT_TRUE(fabs(length-10)<1e-7 && fabs(point.z-10)<1e-7,"native ray physical distance");
    occt_bridge_shape_release(session,out);
    occt_bridge_shape_release(session,wire);
    occt_bridge_shape_release(session,profile);
    occt_bridge_shape_release(session,limit);
    occt_bridge_shape_release(session,box);
}

static void profile_lofts(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t a=0,b=0,open=0,box=0,out=0;
    const occt_bridge_vec3_t points[]={vec(0,0,10),vec(2,0,10)};
    EXPECT(occt_bridge_create_circle_wire(session,vec(0,0,0),vec(0,0,1),2,&a),OK);
    EXPECT(occt_bridge_create_circle_wire(session,vec(0,0,10),vec(0,0,1),4,&b),OK);
    EXPECT(occt_bridge_create_polyline_wire(session,points,2,0,&open),OK);
    EXPECT(occt_bridge_create_box(session,vec(0,0,0),vec(1,1,1),&box),OK);
    const occt_bridge_shape_id_t good[]={a,b},duplicate[]={a,a},missing[]={a,unknown},bad[]={a,box},unclosed[]={a,open};
    EXPECT(occt_bridge_create_loft_from_wires(session,good,2,1,1,NULL),ARG);
    EXPECT(occt_bridge_create_loft_from_wires(session,NULL,2,1,1,&out),ARG);
    EXPECT(occt_bridge_create_loft_from_wires(session,good,1,1,1,&out),ARG);
    EXPECT(occt_bridge_create_loft_from_wires(session,good,1001,1,1,&out),ARG);
    EXPECT(occt_bridge_create_loft_from_wires(session,good,2,2,1,&out),ARG);
    EXPECT(occt_bridge_create_loft_from_wires(session,good,2,1,2,&out),ARG);
    EXPECT(occt_bridge_create_loft_from_wires(session,duplicate,2,1,1,&out),GEOMETRY);
    EXPECT(occt_bridge_create_loft_from_wires(session,missing,2,1,1,&out),MISSING);
    EXPECT(occt_bridge_create_loft_from_wires(session,bad,2,1,1,&out),GEOMETRY);
    EXPECT(occt_bridge_create_loft_from_wires(session,unclosed,2,1,1,&out),GEOMETRY);
    EXPECT_TRUE(out==0,"failed loft clears output");
    EXPECT(occt_bridge_create_loft_from_wires(session,good,2,1,1,&out),OK);
    occt_bridge_shape_release(session,out);
    occt_bridge_shape_release(session,a);
    occt_bridge_shape_release(session,b);
    occt_bridge_shape_release(session,open);
    occt_bridge_shape_release(session,box);
}

int main(void) {
    occt_bridge_session_t* session = NULL;
    if (occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION, &session) != OK) {
        (void)fprintf(stderr, "session creation failed\n");
        return EXIT_FAILURE;
    }
    session_and_diagnostics(session);
    primitives(session);
    sketch_edits(session);
    extrusion_limits(session);
    profile_lofts(session);
    wires_and_faces(session);
    recipes_sweeps_and_lofts(session);
    combinations_and_features(session);
    draft(session);
    queries(session);
    topology_relations(session);
    history(session);
    open_profile(session);
    open_profile_to_next(session);
    station_fillet(session);
    measurements(session);
    projection(session);
    surface_mesh(session);
    persistence(session);
    occt_bridge_session_destroy(session);

    if (failures != 0) {
        (void)fprintf(stderr, "%d error-path expectation(s) failed\n", failures);
        return EXIT_FAILURE;
    }
    return EXIT_SUCCESS;
}
