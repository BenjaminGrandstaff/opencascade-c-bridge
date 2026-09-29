#include "occt_bridge.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>

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

    int is_valid = 0;
    int history_deleted = 0;
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

    require_true(
        occt_bridge_create_cylinder(
            session,
            (occt_bridge_vec3_t){0.0, 0.0, 0.0},
            (occt_bridge_vec3_t){0.0, 0.0, 0.0},
            1.0,
            1.0,
            &invalid) == OCCT_BRIDGE_INVALID_ARGUMENT,
        "zero cylinder axis rejection");

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

    occt_bridge_session_destroy(session);
    puts("C ABI smoke test passed");
    return 0;
}
