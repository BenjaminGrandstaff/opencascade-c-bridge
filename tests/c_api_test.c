#include "occt_bridge.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void require_true(int condition, const char* message) {
    if (!condition) {
        (void)fprintf(stderr, "test failure: %s\n", message);
        abort();
    }
}

static void require_ok(const occt_bridge_session_t* session, occt_bridge_status_t status) {
    if (status != OCCT_BRIDGE_OK) {
        char error[512] = {0};
        (void)occt_bridge_session_last_error(session, error, sizeof(error));
        (void)fprintf(stderr, "%s: %s\n", occt_bridge_status_string(status), error);
        abort();
    }
}

static int close_enough(double left, double right) {
    /* OCCT expands BRep bounding boxes by geometric tolerance. */
    return fabs(left - right) < 1e-6;
}

static void test_step_round_trip(occt_bridge_session_t* session) {
    const char* path = "c-api-test-output.step";
    occt_bridge_shape_id_t source = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_box(
        session, (occt_bridge_vec3_t){-2.0, 3.0, 5.0},
        (occt_bridge_vec3_t){10.0, 20.0, 30.0}, &source));
    occt_bridge_bounds_t source_bounds;
    double source_volume = 0.0;
    require_ok(session, occt_bridge_shape_bounds(session, source, &source_bounds));
    require_ok(session, occt_bridge_shape_volume(session, source, &source_volume));
    require_ok(session, occt_bridge_step_save(session, source, path));

    occt_bridge_shape_id_t loaded = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_step_load(session, path, &loaded));
    int is_valid = 0;
    occt_bridge_shape_type_t type = 0;
    occt_bridge_bounds_t loaded_bounds;
    double loaded_volume = 0.0;
    size_t faces = 0;
    size_t edges = 0;
    require_ok(session, occt_bridge_shape_is_valid(session, loaded, &is_valid));
    require_ok(session, occt_bridge_shape_type(session, loaded, &type));
    require_ok(session, occt_bridge_shape_bounds(session, loaded, &loaded_bounds));
    require_ok(session, occt_bridge_shape_volume(session, loaded, &loaded_volume));
    require_ok(session, occt_bridge_shape_subshape_count(
        session, loaded, OCCT_BRIDGE_SHAPE_FACE, &faces));
    require_ok(session, occt_bridge_shape_subshape_count(
        session, loaded, OCCT_BRIDGE_SHAPE_EDGE, &edges));
    require_true(is_valid == 1, "STEP round-trip validity");
    require_true(type == OCCT_BRIDGE_SHAPE_SOLID, "STEP round-trip topology type");
    require_true(faces == 6 && edges == 12, "STEP round-trip topology counts");
    require_true(close_enough(source_bounds.min.x, loaded_bounds.min.x), "STEP minimum x");
    require_true(close_enough(source_bounds.min.y, loaded_bounds.min.y), "STEP minimum y");
    require_true(close_enough(source_bounds.min.z, loaded_bounds.min.z), "STEP minimum z");
    require_true(close_enough(source_bounds.max.x, loaded_bounds.max.x), "STEP maximum x");
    require_true(close_enough(source_bounds.max.y, loaded_bounds.max.y), "STEP maximum y");
    require_true(close_enough(source_bounds.max.z, loaded_bounds.max.z), "STEP maximum z");
    require_true(fabs(source_volume - loaded_volume) < 1e-6, "STEP round-trip volume");

    size_t count = 0;
    require_ok(session, occt_bridge_session_shape_count(session, &count));
    require_true(count == 2, "STEP round-trip handle count");
    require_ok(session, occt_bridge_shape_remove(session, loaded));
    require_ok(session, occt_bridge_shape_remove(session, source));
    require_true(remove(path) == 0, "remove STEP test output");
}

static void test_stl_export(occt_bridge_session_t* session) {
    const char* path = "c-api-test-output.stl";
    occt_bridge_shape_id_t box = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_box(
        session, (occt_bridge_vec3_t){0.0, 0.0, 0.0},
        (occt_bridge_vec3_t){10.0, 20.0, 30.0}, &box));
    require_ok(session, occt_bridge_stl_save(session, box, path, 0.1, 0.5, 1));

    FILE* file = fopen(path, "rb");
    if (file == NULL) {
        (void)fprintf(stderr, "test failure: open binary STL output\n");
        abort();
    }
    unsigned char header[84] = {0};
    require_true(fread(header, 1, sizeof(header), file) == sizeof(header), "read STL header");
    const uint32_t triangles = (uint32_t)header[80]
        | ((uint32_t)header[81] << 8u)
        | ((uint32_t)header[82] << 16u)
        | ((uint32_t)header[83] << 24u);
    require_true(triangles == 12u, "box STL triangle count");
    require_true(fseek(file, 0, SEEK_END) == 0, "seek STL end");
    const long size = ftell(file);
    require_true(size == 84L + 50L * (long)triangles, "binary STL byte count");
    const int close_status = fclose(file);
    file = NULL;
    require_true(close_status == 0, "close STL output");

    size_t count = 0;
    require_ok(session, occt_bridge_session_shape_count(session, &count));
    require_true(count == 1, "STL export creates no shape handles");
    require_ok(session, occt_bridge_shape_remove(session, box));
    require_true(remove(path) == 0, "remove STL test output");
}

static occt_bridge_shape_id_t square_face(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t a,
    occt_bridge_vec3_t b,
    occt_bridge_vec3_t c,
    occt_bridge_vec3_t d) {
    const occt_bridge_vec3_t corners[] = {a, b, c, d};
    occt_bridge_shape_id_t wire = OCCT_BRIDGE_INVALID_SHAPE_ID;
    occt_bridge_shape_id_t face = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_polyline_wire(session, corners, 4, 1, &wire));
    require_ok(session, occt_bridge_create_face_from_wire(session, wire, &face));
    require_ok(session, occt_bridge_shape_remove(session, wire));
    return face;
}

/* Six independent unit squares share no topology until they are sewn. */
static void test_sewing_and_solids(occt_bridge_session_t* session) {
    const occt_bridge_vec3_t p000 = {0, 0, 0}, p100 = {1, 0, 0}, p110 = {1, 1, 0}, p010 = {0, 1, 0};
    const occt_bridge_vec3_t p001 = {0, 0, 1}, p101 = {1, 0, 1}, p111 = {1, 1, 1}, p011 = {0, 1, 1};
    const occt_bridge_shape_id_t faces[] = {
        square_face(session, p000, p010, p110, p100),
        square_face(session, p001, p101, p111, p011),
        square_face(session, p000, p100, p101, p001),
        square_face(session, p010, p011, p111, p110),
        square_face(session, p000, p001, p011, p010),
        square_face(session, p100, p110, p111, p101),
    };
    occt_bridge_shape_id_t sewn = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_sew(session, faces, 6, 1e-6, &sewn));
    occt_bridge_shape_type_t type = 0;
    require_ok(session, occt_bridge_shape_type(session, sewn, &type));
    require_true(type == OCCT_BRIDGE_SHAPE_SHELL, "six joined squares sew into a shell");
    size_t count = 0;
    require_ok(session, occt_bridge_shape_subshape_count(session, sewn, OCCT_BRIDGE_SHAPE_EDGE, &count));
    require_true(count == 12, "sewing merges shared cube edges");
    require_ok(session, occt_bridge_shape_history_count(
        session, sewn, faces[0], OCCT_BRIDGE_HISTORY_MODIFIED, &count));
    require_true(count == 1, "each input face maps to one sewn face");

    occt_bridge_shape_id_t solid = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_make_solid(session, sewn, &solid));
    double volume = 0.0;
    require_ok(session, occt_bridge_shape_volume(session, solid, &volume));
    require_true(close_enough(volume, 1.0), "sewn cube encloses unit volume");
    occt_bridge_bounds_t exact;
    require_ok(session, occt_bridge_shape_exact_bounds(session, solid, &exact));
    require_true(
        fabs(exact.max.x - exact.min.x - 1.0) < 1e-12 && fabs(exact.max.z - exact.min.z - 1.0) < 1e-12,
        "exact bounds of the sewn cube have no tolerance padding");
    int is_valid = 0;
    require_ok(session, occt_bridge_shape_is_valid(session, solid, &is_valid));
    require_true(is_valid == 1, "sewn cube solid is valid");

    occt_bridge_shape_id_t outer = OCCT_BRIDGE_INVALID_SHAPE_ID;
    occt_bridge_shape_id_t inner = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_box(session, (occt_bridge_vec3_t){0, 0, 0},
        (occt_bridge_vec3_t){10, 10, 10}, &outer));
    require_ok(session, occt_bridge_create_box(session, (occt_bridge_vec3_t){2, 2, 2},
        (occt_bridge_vec3_t){2, 2, 2}, &inner));
    const occt_bridge_shape_id_t boundaries[] = {inner, outer};
    require_ok(session, occt_bridge_make_solid_from_shells(session, boundaries, 2, &solid));
    require_ok(session, occt_bridge_shape_volume(session, solid, &volume));
    require_true(close_enough(volume, 992.0), "inner shell subtracts a void from the outer shell");
    require_ok(session, occt_bridge_shape_subshape_count(
        session, solid, OCCT_BRIDGE_SHAPE_SHELL, &count));
    require_true(count == 2, "void solid retains outer and inner shells");
    require_ok(session, occt_bridge_shape_is_valid(session, solid, &is_valid));
    require_true(is_valid == 1, "multi-shell solid is valid");

    occt_bridge_shape_id_t open_shell = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_sew(session, faces, 5, 1e-6, &open_shell));
    require_true(
        occt_bridge_make_solid(session, open_shell, &solid) == OCCT_BRIDGE_INVALID_GEOMETRY,
        "an open shell does not make a solid");
}

/* Result validation is on by default; options round-trip; warnings clear. */
static void test_session_options(occt_bridge_session_t* session) {
    occt_bridge_session_options_t options;
    require_ok(session, occt_bridge_session_get_options(session, &options));
    require_true(
        options.validate_results == 1 && options.heal_invalid_results == 0
            && options.boolean_fuzzy_tolerance == 0.0,
        "default session options validate without healing or fuzziness");
    occt_bridge_session_options_t healing = {1, 1, 1e-5};
    require_ok(session, occt_bridge_session_set_options(session, &healing));
    require_ok(session, occt_bridge_session_get_options(session, &options));
    require_true(
        options.heal_invalid_results == 1 && options.boolean_fuzzy_tolerance == 1e-5,
        "session options round-trip");
    occt_bridge_session_options_t defaults = {1, 0, 0.0};
    require_ok(session, occt_bridge_session_set_options(session, &defaults));
    /* Releasing keeps the last error and ignores unknown or released handles. */
    occt_bridge_shape_id_t released = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_box(
        session, (occt_bridge_vec3_t){0, 0, 0}, (occt_bridge_vec3_t){1, 1, 1}, &released));
    size_t before = 0;
    require_ok(session, occt_bridge_session_shape_count(session, &before));
    require_true(
        occt_bridge_shape_remove(session, 999999) == OCCT_BRIDGE_SHAPE_NOT_FOUND,
        "removing an unknown handle reports not found");
    char error[64] = {0};
    const size_t error_size = occt_bridge_session_last_error(session, error, sizeof(error));
    occt_bridge_shape_release(session, released);
    occt_bridge_shape_release(session, released);
    occt_bridge_shape_release(session, 999999);
    occt_bridge_shape_release(NULL, released);
    require_true(
        occt_bridge_session_last_error(session, NULL, 0) == error_size && error_size > 1,
        "release leaves the last error untouched");
    size_t after = 0;
    require_ok(session, occt_bridge_session_shape_count(session, &after));
    require_true(after + 1 == before, "release frees exactly the released handle");
    char warnings[64] = {0};
    require_true(
        occt_bridge_session_last_warnings(session, warnings, sizeof(warnings)) == 1 && warnings[0] == '\0',
        "a successful call without warnings leaves none");
}

static occt_bridge_shape_id_t make_box(occt_bridge_session_t* session, double x, double size) {
    occt_bridge_shape_id_t box = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_box(
        session, (occt_bridge_vec3_t){x, 0, 0}, (occt_bridge_vec3_t){size, size, size}, &box));
    return box;
}

static size_t diagnostic_count(occt_bridge_session_t* session) {
    size_t count = 0;
    require_ok(session, occt_bridge_session_diagnostic_count(session, &count));
    return count;
}

static int error_mentions(const occt_bridge_session_t* session, const char* text) {
    char error[512] = {0};
    (void)occt_bridge_session_last_error(session, error, sizeof(error));
    return strstr(error, text) != NULL;
}

static int diagnostic_named(const occt_bridge_session_t* session, size_t index, const char* name) {
    char buffer[128] = {0};
    const size_t size = occt_bridge_session_diagnostic_name(session, index, buffer, sizeof(buffer));
    return size == strlen(name) + 1 && strcmp(buffer, name) == 0;
}

static void test_failure_diagnostics(occt_bridge_session_t* session) {
    const occt_bridge_shape_id_t box = make_box(session, 0, 10);
    occt_bridge_shape_id_t edges[12];
    for (size_t index = 0; index < 12; ++index) {
        require_ok(session, occt_bridge_shape_subshape_at(
            session, box, OCCT_BRIDGE_SHAPE_EDGE, index, &edges[index]));
    }
    occt_bridge_shape_id_t result = OCCT_BRIDGE_INVALID_SHAPE_ID;

    /* A fillet wider than the faces names the selected edge and its contour status. */
    require_true(
        occt_bridge_fillet(session, box, &edges[3], 1, 20.0, &result) == OCCT_BRIDGE_KERNEL_ERROR,
        "oversized fillet fails in the kernel");
    require_true(diagnostic_count(session) == 1, "oversized fillet reports one faulty edge");
    occt_bridge_diagnostic_t diagnostic;
    occt_bridge_shape_id_t culprit = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_session_diagnostic_at(session, 0, &diagnostic, &culprit));
    require_true(
        diagnostic.kind == OCCT_BRIDGE_DIAGNOSTIC_FILLET_EDGE && diagnostic.input_index == 0
            && diagnostic.has_shape == 1 && culprit != OCCT_BRIDGE_INVALID_SHAPE_ID,
        "fillet diagnostic names the selected edge");
    require_true(diagnostic_count(session) == 1, "diagnostic queries keep the diagnostics");
    require_true(diagnostic_named(session, 0, "ChFiDS_StartsolFailure"), "fillet code uses OCCT's name");
    require_true(diagnostic.code == 3, "fillet code is OCCT's ChFiDS_ErrorStatus value");
    require_true(
        error_mentions(session, "fillet construction failed: ChFiDS_StartsolFailure on selection 0"),
        "fillet error summarizes the diagnostic, even after handles were created");
    occt_bridge_shape_id_t no_shape = UINT64_C(7);
    require_ok(session, occt_bridge_session_diagnostic_at(session, 0, &diagnostic, NULL));
    require_true(
        occt_bridge_session_diagnostic_at(session, 1, &diagnostic, &no_shape) == OCCT_BRIDGE_INVALID_ARGUMENT
            && no_shape == OCCT_BRIDGE_INVALID_SHAPE_ID,
        "diagnostic index is range-checked");
    require_true(
        occt_bridge_session_diagnostic_name(session, 1, NULL, 0) == 0,
        "diagnostic name index is range-checked");
    /* Any other call starts afresh, so compare identity last. */
    int same = 0;
    require_ok(session, occt_bridge_shape_is_same(session, culprit, edges[3], &same));
    require_true(same == 1, "fillet diagnostic shape is the selected edge");
    require_true(diagnostic_count(session) == 0, "other calls clear diagnostics");
    occt_bridge_shape_release(session, culprit);

    /* Chamfers report no faulty contours; rebuilding each one alone finds them. */
    const occt_bridge_shape_id_t chamfer_edges[2] = {edges[0], edges[5]};
    require_true(
        occt_bridge_chamfer(session, box, chamfer_edges, 2, 20.0, &result) == OCCT_BRIDGE_KERNEL_ERROR,
        "oversized chamfer fails in the kernel");
    require_true(diagnostic_count(session) == 2, "each failing chamfer contour is reported");
    for (size_t index = 0; index < 2; ++index) {
        require_ok(session, occt_bridge_session_diagnostic_at(session, index, &diagnostic, NULL));
        require_true(
            diagnostic.kind == OCCT_BRIDGE_DIAGNOSTIC_ISOLATED_EDGE
                && diagnostic.input_index == (int64_t)index && diagnostic.has_shape == 1,
            "chamfer diagnostics follow the selection order");
    }

    /* The next call starts without diagnostics. */
    const occt_bridge_shape_id_t other = make_box(session, 20, 10);
    require_true(diagnostic_count(session) == 0, "a successful call clears diagnostics");

    /* Results rejected by validation name their invalid subshapes. */
    require_true(
        occt_bridge_fillet(session, box, edges, 12, 6.0, &result) == OCCT_BRIDGE_INVALID_GEOMETRY,
        "self-intersecting fillet result is rejected");
    const size_t invalid = diagnostic_count(session);
    require_true(invalid > 1, "invalid fillet reports its invalid subshapes");
    require_true(invalid <= OCCT_BRIDGE_MAX_DIAGNOSTICS, "diagnostics are capped");
    require_true(error_mentions(session, "fillet produced an invalid shape: BRepCheck_"), "invalid result error summary");
    /* Read every diagnostic before inspecting shapes, which starts a new call. */
    occt_bridge_shape_id_t self_intersecting = OCCT_BRIDGE_INVALID_SHAPE_ID;
    for (size_t index = 0; index < invalid; ++index) {
        occt_bridge_shape_id_t subshape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        require_ok(session, occt_bridge_session_diagnostic_at(session, index, &diagnostic, &subshape));
        require_true(
            diagnostic.kind == OCCT_BRIDGE_DIAGNOSTIC_INVALID_SUBSHAPE && diagnostic.input_index == -1
                && subshape != OCCT_BRIDGE_INVALID_SHAPE_ID,
            "invalid subshape diagnostics carry the subshape");
        if (self_intersecting == OCCT_BRIDGE_INVALID_SHAPE_ID
            && diagnostic_named(session, index, "BRepCheck_SelfIntersectingWire")) {
            self_intersecting = subshape;
        } else {
            occt_bridge_shape_release(session, subshape);
        }
    }
    require_true(self_intersecting != OCCT_BRIDGE_INVALID_SHAPE_ID, "a self-intersecting wire is reported");
    occt_bridge_shape_type_t type = 0;
    require_ok(session, occt_bridge_shape_type(session, self_intersecting, &type));
    require_true(type == OCCT_BRIDGE_SHAPE_WIRE, "the self-intersecting subshape is a wire");
    occt_bridge_shape_release(session, self_intersecting);

    /* Offsets report OCCT's offset error code. */
    const occt_bridge_shape_id_t pair[2] = {box, other};
    occt_bridge_shape_id_t separate = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_compound(session, pair, 2, &separate));
    require_true(
        occt_bridge_offset(session, separate, 1.0, 1e-6, &result) == OCCT_BRIDGE_KERNEL_ERROR,
        "offset of disconnected solids fails");
    require_true(diagnostic_count(session) == 1, "offset reports its error code");
    require_ok(session, occt_bridge_session_diagnostic_at(session, 0, &diagnostic, NULL));
    require_true(
        diagnostic.kind == OCCT_BRIDGE_DIAGNOSTIC_OFFSET && diagnostic.code == 5
            && diagnostic_named(session, 0, "BRepOffset_NotConnectedShell"),
        "offset diagnostic uses BRepOffset_Error");

    /* Boolean alerts carry OCCT's alert key. */
    occt_bridge_shape_id_t face = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_shape_subshape_at(session, other, OCCT_BRIDGE_SHAPE_FACE, 0, &face));
    require_true(
        occt_bridge_fuse(session, box, face, &result) == OCCT_BRIDGE_KERNEL_ERROR,
        "fusing a solid with a face is not allowed");
    require_true(diagnostic_count(session) >= 1, "boolean failure reports its alerts");
    require_ok(session, occt_bridge_session_diagnostic_at(session, 0, &diagnostic, NULL));
    require_true(
        diagnostic.kind == OCCT_BRIDGE_DIAGNOSTIC_BOOLEAN_ALERT
            && diagnostic_named(session, 0, "BOPAlgo_AlertBOPNotAllowed"),
        "boolean diagnostic is OCCT's alert");
    require_true(error_mentions(session, "fuse operation failed: BOPAlgo_AlertBOPNotAllowed"), "boolean error summary");

    /* Isolation blames only the contour that fails alone: a 5 mm chamfer fits
       the vertical edge of a 4 mm thick plate but not its top edges. */
    occt_bridge_shape_id_t plate = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_box(
        session, (occt_bridge_vec3_t){0, 0, 0}, (occt_bridge_vec3_t){30, 30, 4}, &plate));
    occt_bridge_shape_id_t plate_edges[2] = {OCCT_BRIDGE_INVALID_SHAPE_ID, OCCT_BRIDGE_INVALID_SHAPE_ID};
    size_t plate_edge_count = 0;
    require_ok(session, occt_bridge_shape_subshape_count(session, plate, OCCT_BRIDGE_SHAPE_EDGE, &plate_edge_count));
    for (size_t index = 0; index < plate_edge_count; ++index) {
        occt_bridge_shape_id_t edge = OCCT_BRIDGE_INVALID_SHAPE_ID;
        double length = 0.0;
        require_ok(session, occt_bridge_shape_subshape_at(session, plate, OCCT_BRIDGE_SHAPE_EDGE, index, &edge));
        require_ok(session, occt_bridge_shape_edge_length(session, edge, &length));
        const size_t slot = close_enough(length, 4.0) ? 0 : 1;
        if (plate_edges[slot] == OCCT_BRIDGE_INVALID_SHAPE_ID) {
            plate_edges[slot] = edge;
        } else {
            occt_bridge_shape_release(session, edge);
        }
    }
    require_true(
        occt_bridge_chamfer(session, plate, plate_edges, 2, 5.0, &result) == OCCT_BRIDGE_KERNEL_ERROR,
        "chamfer wider than the plate fails");
    require_true(diagnostic_count(session) == 1, "only the failing contour is reported");
    require_ok(session, occt_bridge_session_diagnostic_at(session, 0, &diagnostic, NULL));
    require_true(
        diagnostic.kind == OCCT_BRIDGE_DIAGNOSTIC_ISOLATED_EDGE && diagnostic.input_index == 1,
        "isolation names the top edge, not the vertical one");

    /* Many failing contours: diagnostics are capped and isolation is bounded. */
    enum { BLOCKS = 70 };
    occt_bridge_shape_id_t blocks[BLOCKS];
    for (size_t index = 0; index < BLOCKS; ++index) {
        blocks[index] = make_box(session, 20.0 * (double)index, 10);
    }
    occt_bridge_shape_id_t row = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_compound(session, blocks, BLOCKS, &row));
    occt_bridge_shape_id_t block_edges[BLOCKS];
    for (size_t index = 0; index < BLOCKS; ++index) {
        require_ok(session, occt_bridge_shape_subshape_at(
            session, blocks[index], OCCT_BRIDGE_SHAPE_EDGE, 0, &block_edges[index]));
    }
    require_true(
        occt_bridge_chamfer(session, row, block_edges, BLOCKS, 20.0, &result) == OCCT_BRIDGE_KERNEL_ERROR,
        "oversized chamfer on every block fails");
    require_true(
        diagnostic_count(session) == OCCT_BRIDGE_MAX_DIAGNOSTICS,
        "isolation stops after the bounded number of contours");
    char warnings[256] = {0};
    (void)occt_bridge_session_last_warnings(session, warnings, sizeof(warnings));
    require_true(strstr(warnings, "first 64 of 70 contours") != NULL, "bounded isolation is reported");
    require_true(
        occt_bridge_fillet(session, row, block_edges, BLOCKS, 20.0, &result) == OCCT_BRIDGE_KERNEL_ERROR,
        "oversized fillet on every block fails");
    require_true(diagnostic_count(session) == OCCT_BRIDGE_MAX_DIAGNOSTICS, "fillet diagnostics are capped");
    require_true(error_mentions(session, "(6 more diagnostics omitted)"), "omitted diagnostics are counted");
    for (size_t index = 0; index < BLOCKS; ++index) {
        occt_bridge_shape_release(session, block_edges[index]);
        occt_bridge_shape_release(session, blocks[index]);
    }
    occt_bridge_shape_release(session, row);
    occt_bridge_shape_release(session, plate_edges[0]);
    occt_bridge_shape_release(session, plate_edges[1]);
    occt_bridge_shape_release(session, plate);

    for (size_t index = 0; index < 12; ++index) {
        occt_bridge_shape_release(session, edges[index]);
    }
    occt_bridge_shape_release(session, face);
    occt_bridge_shape_release(session, separate);
    occt_bridge_shape_release(session, other);
    occt_bridge_shape_release(session, box);
}

/*
 * A block fused flush against a cylinder meets it tangentially along new edges
 * that the boolean records no continuity for: only measuring finds them.
 */
static void test_inferred_tangency(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t cylinder = OCCT_BRIDGE_INVALID_SHAPE_ID;
    occt_bridge_shape_id_t block = OCCT_BRIDGE_INVALID_SHAPE_ID;
    occt_bridge_shape_id_t stadium = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_cylinder(
        session, (occt_bridge_vec3_t){0, 0, 0}, (occt_bridge_vec3_t){0, 0, 1}, 5.0, 10.0, &cylinder));
    require_ok(session, occt_bridge_create_box(
        session, (occt_bridge_vec3_t){0, -5, 0}, (occt_bridge_vec3_t){20, 10, 10}, &block));
    require_ok(session, occt_bridge_fuse(session, cylinder, block, &stadium));
    size_t face_count = 0;
    require_ok(session, occt_bridge_shape_subshape_count(session, stadium, OCCT_BRIDGE_SHAPE_FACE, &face_count));
    occt_bridge_shape_id_t faces[16] = {0};
    require_true(face_count <= 16, "stadium face count");
    for (size_t index = 0; index < face_count; ++index) {
        require_ok(session, occt_bridge_shape_subshape_at(
            session, stadium, OCCT_BRIDGE_SHAPE_FACE, index, &faces[index]));
    }
    int recorded = 0;
    int measured = 0;
    for (size_t first = 0; first < face_count; ++first) {
        for (size_t second = first + 1; second < face_count; ++second) {
            int tangent = 0;
            require_ok(session, occt_bridge_shape_faces_are_tangent(
                session, stadium, faces[first], faces[second], &tangent));
            recorded += tangent;
            require_ok(session, occt_bridge_shape_faces_are_tangent_within(
                session, stadium, faces[first], faces[second], 1e-3, &tangent));
            measured += tangent;
        }
    }
    require_true(recorded == 0, "the fuse records no continuity on its new edges");
    /* Both flat sides meet the round end; on the top and on the bottom the
     * split disc's halves meet each other and the block's notched face. */
    require_true(measured == 6, "measuring finds the side and cap junctions");

    /* Merging same-domain faces leaves one top, one bottom, two flat sides,
     * the flat end, and one round end, at the same volume. */
    occt_bridge_shape_id_t unified = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_unify_same_domain(session, stadium, 1e-7, 1e-9, &unified));
    size_t unified_faces = 0;
    require_ok(session, occt_bridge_shape_subshape_count(session, unified, OCCT_BRIDGE_SHAPE_FACE, &unified_faces));
    require_true(face_count == 10 && unified_faces == 6, "unify merges the split faces");
    double before = 0.0;
    double after = 0.0;
    require_ok(session, occt_bridge_shape_volume(session, stadium, &before));
    require_ok(session, occt_bridge_shape_volume(session, unified, &after));
    require_true(close_enough(before, after), "unify keeps the volume");
    /* No face is deleted; at least the three top and three bottom pieces are
     * modified into their merged faces. */
    size_t modified = 0;
    for (size_t index = 0; index < face_count; ++index) {
        size_t records = 0;
        int deleted = 1;
        require_ok(session, occt_bridge_shape_history_count(
            session, unified, faces[index], OCCT_BRIDGE_HISTORY_MODIFIED, &records));
        require_ok(session, occt_bridge_shape_history_is_deleted(session, unified, faces[index], &deleted));
        require_true(deleted == 0, "unify deletes no face");
        modified += records > 0 ? 1 : 0;
    }
    require_true(modified >= 6, "unify records the merged pieces as modified");
    occt_bridge_shape_id_t ignored = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_true(occt_bridge_unify_same_domain(session, stadium, 0.0, 1e-9, &ignored)
        == OCCT_BRIDGE_INVALID_ARGUMENT && ignored == OCCT_BRIDGE_INVALID_SHAPE_ID, "unify rejects a zero tolerance");
    require_true(occt_bridge_unify_same_domain(session, stadium, 1e-7, 2.0, &ignored)
        == OCCT_BRIDGE_INVALID_ARGUMENT, "unify rejects a wide angle");
    require_ok(session, occt_bridge_shape_remove(session, unified));
    for (size_t index = 0; index < face_count; ++index) {
        require_ok(session, occt_bridge_shape_remove(session, faces[index]));
    }
    require_ok(session, occt_bridge_shape_remove(session, stadium));
    require_ok(session, occt_bridge_shape_remove(session, block));
    require_ok(session, occt_bridge_shape_remove(session, cylinder));
}

/* A box with its first face and edge, shared by the box sections. */
struct box_fixture {
    occt_bridge_shape_id_t box;
    occt_bridge_shape_id_t face;
    occt_bridge_shape_id_t edge;
};

/* Creation, rejection, and topology and geometry queries on one box. */
static struct box_fixture test_box_queries(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t box = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_box(
        session,
        (occt_bridge_vec3_t){1.0, 2.0, 3.0},
        (occt_bridge_vec3_t){10.0, 20.0, 30.0},
        &box));
    occt_bridge_shape_id_t duplicate_box = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_shape_duplicate(session, box, &duplicate_box));
    require_ok(session, occt_bridge_shape_remove(session, duplicate_box));

    occt_bridge_shape_id_t invalid = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_true(
        occt_bridge_create_box(
            session,
            (occt_bridge_vec3_t){0.0, 0.0, 0.0},
            (occt_bridge_vec3_t){0.0, 1.0, 1.0},
            &invalid) == OCCT_BRIDGE_INVALID_ARGUMENT,
        "invalid box rejection");
    char diagnostic[128] = {0};
    require_true(
        occt_bridge_session_last_error(session, diagnostic, sizeof(diagnostic)) > 1,
        "invalid box diagnostic");

    occt_bridge_bounds_t bounds;
    require_ok(session, occt_bridge_shape_bounds(session, box, &bounds));
    require_true(close_enough(bounds.min.x, 1.0), "minimum x bound");
    require_true(close_enough(bounds.min.y, 2.0), "minimum y bound");
    require_true(close_enough(bounds.min.z, 3.0), "minimum z bound");
    require_true(close_enough(bounds.max.x, 11.0), "maximum x bound");
    require_true(close_enough(bounds.max.y, 22.0), "maximum y bound");
    require_true(close_enough(bounds.max.z, 33.0), "maximum z bound");

    occt_bridge_shape_type_t shape_type = 0;
    require_ok(session, occt_bridge_shape_type(session, box, &shape_type));
    require_true(shape_type == OCCT_BRIDGE_SHAPE_SOLID, "box topology type");
    size_t face_count = 0;
    size_t edge_count = 0;
    size_t vertex_count = 0;
    require_ok(session, occt_bridge_shape_subshape_count(
        session, box, OCCT_BRIDGE_SHAPE_FACE, &face_count));
    require_ok(session, occt_bridge_shape_subshape_count(
        session, box, OCCT_BRIDGE_SHAPE_EDGE, &edge_count));
    require_ok(session, occt_bridge_shape_subshape_count(
        session, box, OCCT_BRIDGE_SHAPE_VERTEX, &vertex_count));
    require_true(face_count == 6, "box face count");
    require_true(edge_count == 12, "box edge count");
    require_true(vertex_count == 8, "box vertex count");
    occt_bridge_shape_id_t first_face = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_shape_subshape_at(
        session, box, OCCT_BRIDGE_SHAPE_FACE, 0, &first_face));
    require_ok(session, occt_bridge_shape_type(session, first_face, &shape_type));
    require_true(shape_type == OCCT_BRIDGE_SHAPE_FACE, "extracted face topology type");
    occt_bridge_vec3_t face_normal;
    require_ok(session, occt_bridge_shape_face_normal(session, first_face, &face_normal));
    require_true(
        close_enough(
            sqrt(face_normal.x * face_normal.x
                + face_normal.y * face_normal.y
                + face_normal.z * face_normal.z),
            1.0),
        "face normal is unit length");
    int face_is_planar = 0;
    require_ok(session, occt_bridge_shape_face_is_planar(session, first_face, &face_is_planar));
    require_true(face_is_planar == 1, "box face is planar");
    double area = 0.0;
    double volume = 0.0;
    require_ok(session, occt_bridge_shape_surface_area(session, box, &area));
    require_ok(session, occt_bridge_shape_volume(session, box, &volume));
    require_true(close_enough(area, 2200.0), "box surface area");
    require_true(close_enough(volume, 6000.0), "box volume");
    occt_bridge_vec3_t center_of_mass;
    require_ok(session, occt_bridge_shape_center_of_mass(session, box, &center_of_mass));
    require_true(close_enough(center_of_mass.x, 6.0), "box center of mass x");
    require_true(close_enough(center_of_mass.y, 12.0), "box center of mass y");
    require_true(close_enough(center_of_mass.z, 18.0), "box center of mass z");
    require_true(
        occt_bridge_shape_subshape_at(
            session, box, OCCT_BRIDGE_SHAPE_FACE, face_count, &invalid)
            == OCCT_BRIDGE_INVALID_ARGUMENT,
        "out-of-range subshape rejection");

    occt_bridge_shape_id_t first_edge = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_shape_subshape_at(
        session, box, OCCT_BRIDGE_SHAPE_EDGE, 0, &first_edge));
    double edge_length = 0.0;
    int edge_is_circle = 0;
    double edge_radius = 0.0;
    require_ok(session, occt_bridge_shape_edge_length(session, first_edge, &edge_length));
    require_true(edge_length > 0.0, "box edge has positive length");
    require_ok(session, occt_bridge_shape_edge_circle_radius(
        session, first_edge, &edge_is_circle, &edge_radius));
    require_true(edge_is_circle == 0 && edge_radius == 0.0, "box edge is not circular");
    int edge_curvature_is_defined = 0;
    double edge_curvature = -1.0;
    require_ok(session, occt_bridge_shape_edge_curvature(
        session, first_edge, &edge_curvature_is_defined, &edge_curvature));
    require_true(
        edge_curvature_is_defined == 1 && close_enough(edge_curvature, 0.0),
        "straight box edge has zero curvature");
    int is_adjacent = 0;
    require_ok(session, occt_bridge_shape_is_adjacent(
        session, box, first_face, first_edge, &is_adjacent));
    require_true(is_adjacent == 1, "first box face and edge are adjacent");
    occt_bridge_shape_id_t second_face = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_shape_subshape_at(
        session, box, OCCT_BRIDGE_SHAPE_FACE, 1, &second_face));
    int faces_are_tangent = 1;
    require_ok(session, occt_bridge_shape_faces_are_tangent(
        session, box, first_face, second_face, &faces_are_tangent));
    require_true(faces_are_tangent == 0, "distinct box faces are not tangent");
    int shapes_are_same = 0;
    require_ok(session, occt_bridge_shape_is_same(
        session, first_face, first_face, &shapes_are_same));
    require_true(shapes_are_same == 1, "shape identity recognizes the same face");
    require_ok(session, occt_bridge_shape_is_same(
        session, first_face, second_face, &shapes_are_same));
    require_true(shapes_are_same == 0, "shape identity distinguishes faces");
    require_ok(session, occt_bridge_shape_remove(session, second_face));
    return (struct box_fixture){box, first_face, first_edge};
}

/* Fillet, chamfer, offset, hollow, and boolean history on the box. */
static void test_box_operations(occt_bridge_session_t* session, struct box_fixture fixture) {
    const occt_bridge_shape_id_t box = fixture.box;
    occt_bridge_shape_id_t first_face = fixture.face;
    occt_bridge_shape_id_t first_edge = fixture.edge;
    int is_valid = 0;
    int history_deleted = 0;
    occt_bridge_bounds_t bounds;
    double volume = 0.0;
    occt_bridge_shape_id_t filleted = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_fillet(session, box, &first_edge, 1, 1.0, &filleted));
    require_ok(session, occt_bridge_shape_is_valid(session, filleted, &is_valid));
    require_true(is_valid == 1, "selected-edge fillet validity");
    occt_bridge_shape_id_t chamfered = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_chamfer(session, box, &first_edge, 1, 1.0, &chamfered));
    require_ok(session, occt_bridge_shape_is_valid(session, chamfered, &is_valid));
    require_true(is_valid == 1, "selected-edge chamfer validity");

    occt_bridge_shape_id_t offset_box = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_offset(session, box, 1.0, 1e-6, &offset_box));
    require_ok(session, occt_bridge_shape_is_valid(session, offset_box, &is_valid));
    require_true(is_valid == 1, "offset validity");
    require_ok(session, occt_bridge_shape_bounds(session, offset_box, &bounds));
    require_true(bounds.min.x < 1.0, "positive offset expands minimum x");
    require_true(bounds.max.z > 33.0, "positive offset expands maximum z");

    occt_bridge_shape_id_t hollow_box = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_hollow(
        session, box, &first_face, 1, -1.0, 1e-6, &hollow_box));
    require_ok(session, occt_bridge_shape_is_valid(session, hollow_box, &is_valid));
    require_true(is_valid == 1, "hollow validity");

    occt_bridge_shape_id_t deleting_tool = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_box(
        session,
        (occt_bridge_vec3_t){0.0, 1.0, 2.0},
        (occt_bridge_vec3_t){6.0, 22.0, 32.0},
        &deleting_tool));
    occt_bridge_shape_id_t deletion_cut = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_cut(session, box, deleting_tool, &deletion_cut));
    require_ok(session, occt_bridge_shape_history_is_deleted(
        session, deletion_cut, first_face, &history_deleted));
    require_true(history_deleted == 1, "fully removed cut face is deleted");

    occt_bridge_shape_id_t overlap_box = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_box(
        session,
        (occt_bridge_vec3_t){6.0, 12.0, 18.0},
        (occt_bridge_vec3_t){10.0, 10.0, 20.0},
        &overlap_box));
    occt_bridge_shape_id_t common = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_common(session, box, overlap_box, &common));
    require_ok(session, occt_bridge_shape_volume(session, common, &volume));
    require_true(close_enough(volume, 750.0), "boolean common volume");
}

/* Cylinders, cones, and spheres, and a degenerate axis rejection. */
static void test_primitives(occt_bridge_session_t* session) {
    occt_bridge_shape_id_t invalid = OCCT_BRIDGE_INVALID_SHAPE_ID;
    int is_valid = 0;
    occt_bridge_bounds_t bounds;
    occt_bridge_shape_id_t cylinder = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_cylinder(
        session,
        (occt_bridge_vec3_t){0.0, 0.0, 0.0},
        (occt_bridge_vec3_t){0.0, 0.0, 1.0},
        2.0,
        5.0,
        &cylinder));
    require_ok(session, occt_bridge_shape_is_valid(session, cylinder, &is_valid));
    require_true(is_valid == 1, "cylinder validity");

    occt_bridge_shape_id_t cone = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_cone(
        session,
        (occt_bridge_vec3_t){0.0, 0.0, 0.0},
        (occt_bridge_vec3_t){0.0, 1.0, 0.0},
        3.0,
        1.0,
        8.0,
        &cone));
    require_ok(session, occt_bridge_shape_is_valid(session, cone, &is_valid));
    require_true(is_valid == 1, "cone validity");

    occt_bridge_shape_id_t sphere = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_sphere(
        session, (occt_bridge_vec3_t){5.0, 6.0, 7.0}, 4.0, &sphere));
    require_ok(session, occt_bridge_shape_bounds(session, sphere, &bounds));
    require_true(close_enough(bounds.min.x, 1.0), "sphere minimum x bound");
    require_true(close_enough(bounds.max.z, 11.0), "sphere maximum z bound");

    require_true(
        occt_bridge_create_cylinder(
            session,
            (occt_bridge_vec3_t){0.0, 0.0, 0.0},
            (occt_bridge_vec3_t){0.0, 0.0, 0.0},
            1.0,
            1.0,
            &invalid) == OCCT_BRIDGE_INVALID_ARGUMENT,
        "zero cylinder axis rejection");
}

/* Rigid and scaling transforms, with located face history. */
static void test_transforms(occt_bridge_session_t* session, struct box_fixture fixture) {
    const occt_bridge_shape_id_t box = fixture.box;
    const occt_bridge_shape_id_t first_face = fixture.face;
    int history_deleted = 0;
    occt_bridge_bounds_t bounds;
    occt_bridge_shape_id_t translated = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_translate(
        session, box, (occt_bridge_vec3_t){100.0, -2.0, 7.0}, &translated));
    require_ok(session, occt_bridge_shape_bounds(session, translated, &bounds));
    require_true(close_enough(bounds.min.x, 101.0), "translated minimum x bound");
    require_true(close_enough(bounds.min.y, 0.0), "translated minimum y bound");
    require_true(close_enough(bounds.min.z, 10.0), "translated minimum z bound");
    size_t modified_count = 0;
    require_ok(session, occt_bridge_shape_history_count(
        session,
        translated,
        first_face,
        OCCT_BRIDGE_HISTORY_MODIFIED,
        &modified_count));
    require_true(modified_count == 1, "translated face has one modified result");
    history_deleted = 1;
    require_ok(session, occt_bridge_shape_history_is_deleted(
        session, translated, first_face, &history_deleted));
    require_true(history_deleted == 0, "translated face is not deleted");
    occt_bridge_shape_id_t translated_face = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_shape_history_at(
        session,
        translated,
        first_face,
        OCCT_BRIDGE_HISTORY_MODIFIED,
        0,
        &translated_face));
    occt_bridge_bounds_t source_face_bounds;
    occt_bridge_bounds_t translated_face_bounds;
    require_ok(session, occt_bridge_shape_bounds(session, first_face, &source_face_bounds));
    require_ok(session, occt_bridge_shape_bounds(
        session, translated_face, &translated_face_bounds));
    require_true(
        close_enough(translated_face_bounds.min.x, source_face_bounds.min.x + 100.0),
        "history face translated in x");
    require_true(
        close_enough(translated_face_bounds.min.y, source_face_bounds.min.y - 2.0),
        "history face translated in y");
    require_true(
        close_enough(translated_face_bounds.min.z, source_face_bounds.min.z + 7.0),
        "history face translated in z");

    occt_bridge_shape_id_t rotated = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_rotate(
        session,
        box,
        (occt_bridge_vec3_t){0.0, 0.0, 0.0},
        (occt_bridge_vec3_t){0.0, 0.0, 1.0},
        acos(-1.0) / 2.0,
        &rotated));
    require_ok(session, occt_bridge_shape_bounds(session, rotated, &bounds));
    require_true(close_enough(bounds.min.x, -22.0), "rotated minimum x bound");
    require_true(close_enough(bounds.max.y, 11.0), "rotated maximum y bound");

    occt_bridge_shape_id_t scaled = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_scale(
        session, box, (occt_bridge_vec3_t){0.0, 0.0, 0.0}, 2.0, &scaled));
    require_ok(session, occt_bridge_shape_bounds(session, scaled, &bounds));
    require_true(close_enough(bounds.min.x, 2.0), "scaled minimum x bound");
    require_true(close_enough(bounds.max.z, 66.0), "scaled maximum z bound");
}

/* Wires, faces, prisms with generated history, and curve curvature. */
static void test_profiles(occt_bridge_session_t* session) {
    int is_valid = 0;
    occt_bridge_shape_type_t shape_type = 0;
    occt_bridge_bounds_t bounds;
    const occt_bridge_vec3_t rectangle[] = {
        {0.0, 0.0, 0.0},
        {6.0, 0.0, 0.0},
        {6.0, 4.0, 0.0},
        {0.0, 4.0, 0.0},
    };
    occt_bridge_shape_id_t rectangle_wire = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_polyline_wire(
        session,
        rectangle,
        sizeof(rectangle) / sizeof(rectangle[0]),
        1,
        &rectangle_wire));
    occt_bridge_shape_id_t rectangle_face = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_face_from_wire(
        session, rectangle_wire, &rectangle_face));
    occt_bridge_shape_id_t rectangle_edge = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_shape_subshape_at(
        session, rectangle_face, OCCT_BRIDGE_SHAPE_EDGE, 0, &rectangle_edge));
    occt_bridge_shape_id_t rectangle_prism = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_prism_from_face(
        session,
        rectangle_face,
        (occt_bridge_vec3_t){0.0, 0.0, 3.0},
        &rectangle_prism));
    require_ok(session, occt_bridge_shape_bounds(session, rectangle_prism, &bounds));
    require_true(close_enough(bounds.max.x, 6.0), "face prism maximum x bound");
    require_true(close_enough(bounds.max.y, 4.0), "face prism maximum y bound");
    require_true(close_enough(bounds.max.z, 3.0), "face prism maximum z bound");
    require_ok(session, occt_bridge_shape_is_valid(session, rectangle_wire, &is_valid));
    require_true(is_valid == 1, "polyline wire validity");
    require_ok(session, occt_bridge_shape_is_valid(session, rectangle_face, &is_valid));
    require_true(is_valid == 1, "wire face validity");
    require_ok(session, occt_bridge_shape_is_valid(session, rectangle_prism, &is_valid));
    require_true(is_valid == 1, "face prism validity");
    size_t generated_count = 0;
    require_ok(session, occt_bridge_shape_history_count(
        session,
        rectangle_prism,
        rectangle_edge,
        OCCT_BRIDGE_HISTORY_GENERATED,
        &generated_count));
    require_true(generated_count == 1, "prism edge generates one lateral face");
    occt_bridge_shape_id_t generated_face = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_shape_history_at(
        session,
        rectangle_prism,
        rectangle_edge,
        OCCT_BRIDGE_HISTORY_GENERATED,
        0,
        &generated_face));
    require_ok(session, occt_bridge_shape_type(session, generated_face, &shape_type));
    require_true(shape_type == OCCT_BRIDGE_SHAPE_FACE, "prism generated face type");

    occt_bridge_shape_id_t circle_wire = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_circle_wire(
        session,
        (occt_bridge_vec3_t){10.0, 20.0, 30.0},
        (occt_bridge_vec3_t){0.0, 0.0, 1.0},
        5.0,
        &circle_wire));
    occt_bridge_shape_id_t circle_face = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_face_from_wire(session, circle_wire, &circle_face));
    require_ok(session, occt_bridge_shape_bounds(session, circle_face, &bounds));
    require_true(close_enough(bounds.min.x, 5.0), "circle face minimum x bound");
    require_true(close_enough(bounds.max.y, 25.0), "circle face maximum y bound");

    occt_bridge_shape_id_t ellipse_wire = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_ellipse_wire(
        session,
        (occt_bridge_vec3_t){0.0, 0.0, 0.0},
        (occt_bridge_vec3_t){0.0, 0.0, 1.0},
        4.0,
        2.0,
        &ellipse_wire));
    occt_bridge_shape_id_t ellipse_edge = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_shape_subshape_at(
        session, ellipse_wire, OCCT_BRIDGE_SHAPE_EDGE, 0, &ellipse_edge));
    double minimum_curvature = 0.0;
    double maximum_curvature = 0.0;
    require_ok(session, occt_bridge_shape_edge_curvature_range(
        session, ellipse_edge, 5, &minimum_curvature, &maximum_curvature));
    require_true(close_enough(minimum_curvature, 0.125), "ellipse minimum sampled curvature");
    require_true(close_enough(maximum_curvature, 1.0), "ellipse maximum sampled curvature");
    double minimum_lower_bound = 0.0;
    double maximum_upper_bound = 0.0;
    int is_exact = 0;
    require_ok(session, occt_bridge_shape_edge_curvature_extrema(
        session, ellipse_edge, 1e-6, &minimum_curvature, &minimum_lower_bound,
        &maximum_curvature, &maximum_upper_bound, &is_exact));
    require_true(is_exact == 1, "ellipse curvature extrema are exact");
    require_true(close_enough(minimum_curvature, 0.125), "ellipse exact minimum curvature");
    require_true(close_enough(maximum_curvature, 1.0), "ellipse exact maximum curvature");
    require_true(
        minimum_lower_bound == minimum_curvature && maximum_upper_bound == maximum_curvature,
        "exact curvature bounds coincide");
    require_true(
        occt_bridge_shape_edge_curvature_extrema(
            session, ellipse_edge, 0.0, &minimum_curvature, &minimum_lower_bound,
            &maximum_curvature, &maximum_upper_bound, &is_exact) == OCCT_BRIDGE_INVALID_ARGUMENT,
        "curvature extrema reject zero tolerance");
    require_true(
        occt_bridge_shape_edge_curvature_extrema(
            session, ellipse_edge, 1e-6, NULL, &minimum_lower_bound,
            &maximum_curvature, &maximum_upper_bound, &is_exact) == OCCT_BRIDGE_INVALID_ARGUMENT,
        "curvature extrema reject null output");
    require_ok(session, occt_bridge_shape_remove(session, ellipse_edge));
    require_ok(session, occt_bridge_shape_remove(session, ellipse_wire));
}

/* Polygon prisms with boolean cut and fuse; returns the cut for persistence. */
static occt_bridge_shape_id_t test_booleans(occt_bridge_session_t* session) {
    int is_valid = 0;
    const occt_bridge_vec3_t outline[] = {
        {0.0, 0.0, 0.0},
        {8.0, -1.0, 0.0},
        {11.0, 5.0, 0.0},
        {5.0, 10.0, 0.0},
        {-2.0, 6.0, 0.0},
    };
    occt_bridge_shape_id_t prism = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_polygon_prism(
        session, outline, sizeof(outline) / sizeof(outline[0]),
        (occt_bridge_vec3_t){0.0, 0.0, 4.0}, &prism));

    require_ok(session, occt_bridge_shape_is_valid(session, prism, &is_valid));
    require_true(is_valid == 1, "polygon prism validity");

    occt_bridge_shape_id_t cutter = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_box(
        session,
        (occt_bridge_vec3_t){4.0, 4.0, -1.0},
        (occt_bridge_vec3_t){2.0, 2.0, 8.0},
        &cutter));
    occt_bridge_shape_id_t cut = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_cut(session, prism, cutter, &cut));
    require_ok(session, occt_bridge_shape_is_valid(session, cut, &is_valid));
    require_true(is_valid == 1, "boolean result validity");

    occt_bridge_shape_id_t fused = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_fuse(session, prism, cutter, &fused));
    require_ok(session, occt_bridge_shape_is_valid(session, fused, &is_valid));
    require_true(is_valid == 1, "fuse result validity");
    return cut;
}

/* Faceted stone, loft, compound, wall torch, and polyline tube. */
static void test_recipes_and_lofts(occt_bridge_session_t* session) {
    int is_valid = 0;
    const occt_bridge_vec3_t stone_bottom[] = {
        {-20.0, -14.0, 0.0}, {-7.0, -21.0, 0.0}, {17.0, -18.0, 0.0},
        {23.0, -2.0, 0.0}, {17.0, 17.0, 0.0}, {-3.0, 22.0, 0.0}, {-24.0, 9.0, 0.0},
    };
    const occt_bridge_vec3_t stone_top[] = {
        {-18.0, -12.5, 6.4}, {-6.0, -19.0, 7.1}, {15.0, -16.0, 6.7},
        {20.5, -1.5, 7.5}, {15.0, 15.0, 6.8}, {-2.5, 19.5, 7.8}, {-21.0, 8.0, 6.6},
    };
    occt_bridge_shape_id_t natural_stone = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_faceted_stone(
        session,
        stone_bottom,
        stone_top,
        sizeof(stone_bottom) / sizeof(stone_bottom[0]),
        (occt_bridge_vec3_t){0.0, 0.5, 9.0},
        0.8,
        0.7,
        &natural_stone));
    require_ok(session, occt_bridge_shape_is_valid(session, natural_stone, &is_valid));
    require_true(is_valid == 1, "faceted stone validity");

    const occt_bridge_vec3_t loft_points[] = {
        {0.0, 0.0, -1.0}, {10.0, 0.0, -1.0}, {10.0, 0.0, 1.0}, {0.0, 0.0, 1.0},
        {2.0, 20.0, -0.5}, {8.0, 20.0, -0.5}, {8.0, 20.0, 0.5}, {2.0, 20.0, 0.5},
    };
    const size_t loft_counts[] = {4, 4};
    occt_bridge_shape_id_t loft = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_loft(
        session, loft_points, loft_counts, 2, 1, 0, &loft));
    require_ok(session, occt_bridge_shape_is_valid(session, loft, &is_valid));
    require_true(is_valid == 1, "loft validity");

    const occt_bridge_shape_id_t children[] = {loft, natural_stone};
    occt_bridge_shape_id_t compound = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_compound(
        session, children, sizeof(children) / sizeof(children[0]), &compound));
    require_ok(session, occt_bridge_shape_is_valid(session, compound, &is_valid));
    require_true(is_valid == 1, "compound validity");

    occt_bridge_wall_torch_result_t torch;
    require_ok(session, occt_bridge_create_wall_torch(
        session,
        (occt_bridge_vec3_t){0.0, 100.0, 125.0},
        (occt_bridge_vec3_t){1.0, 0.0, 0.0},
        1.0,
        &torch));
    require_ok(session, occt_bridge_shape_is_valid(session, torch.fixture_shape, &is_valid));
    require_true(is_valid == 1, "torch fixture validity");
    require_ok(session, occt_bridge_shape_is_valid(session, torch.flame_shape, &is_valid));
    require_true(is_valid == 1, "torch flame validity");
    require_true(torch.light.position.x > 40.0, "torch light anchored in flame");
    require_true(torch.light.type == OCCT_BRIDGE_LIGHT_POSITIONAL, "torch uses point light");
    require_true(torch.light.spot_angle_degrees == 0.0, "torch has no spotlight cone");
    require_true(torch.light.cast_shadows == 0, "point-light shadow maps are disabled");

    const occt_bridge_vec3_t tube_path[] = {
        {0.0, 0.0, 0.0}, {20.0, 0.0, 0.0}, {30.0, 10.0, 0.0}, {30.0, 25.0, 5.0},
    };
    occt_bridge_shape_id_t tube = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_create_polyline_tube(
        session, tube_path, sizeof(tube_path) / sizeof(tube_path[0]), 2.0, &tube));
    require_ok(session, occt_bridge_shape_is_valid(session, tube, &is_valid));
    require_true(is_valid == 1, "polyline tube validity");
}

int main(void) {
    require_true(occt_bridge_abi_version() == OCCT_BRIDGE_ABI_VERSION, "ABI version");

    occt_bridge_session_t* rejected = NULL;
    require_true(
        occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION + 1, &rejected)
            == OCCT_BRIDGE_UNSUPPORTED_ABI,
        "unsupported ABI rejection");
    require_true(rejected == NULL, "unsupported ABI returns no session");

    occt_bridge_session_t* session = NULL;
    require_ok(session, occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION, &session));
    require_true(session != NULL, "session created");

    const struct box_fixture box = test_box_queries(session);
    test_box_operations(session, box);
    test_primitives(session);
    test_transforms(session, box);
    test_profiles(session);
    const occt_bridge_shape_id_t cut = test_booleans(session);
    test_recipes_and_lofts(session);

    require_ok(session, occt_bridge_brep_save(session, cut, "c-api-test-output.brep"));
    occt_bridge_shape_id_t loaded = OCCT_BRIDGE_INVALID_SHAPE_ID;
    require_ok(session, occt_bridge_brep_load(session, "c-api-test-output.brep", &loaded));

    size_t count = 0;
    require_ok(session, occt_bridge_session_shape_count(session, &count));
    require_true(count == 36, "shape count after load");

    require_ok(session, occt_bridge_shape_remove(session, loaded));
    require_ok(session, occt_bridge_session_clear(session));
    require_ok(session, occt_bridge_session_shape_count(session, &count));
    require_true(count == 0, "shape count after clear");

    test_step_round_trip(session);
    test_stl_export(session);
    test_sewing_and_solids(session);
    test_inferred_tangency(session);
    test_session_options(session);
    test_failure_diagnostics(session);
    occt_bridge_session_destroy(session);
    puts("C ABI smoke test passed");
    return 0;
}
