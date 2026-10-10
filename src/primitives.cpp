/*
 * Primitive solids: boxes, cylinders, cones, and spheres.
 */

#include "bridge_internal.hpp"

#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include <cmath>
#include <limits>

using namespace occt_bridge_internal;

extern "C" {

occt_bridge_status_t occt_bridge_create_box(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t size,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(origin) || !finite(size) || size.x <= 0.0 || size.y <= 0.0 || size.z <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "box size must be finite and positive");
        }
        BRepPrimAPI_MakeBox builder(
            to_point(origin), size.x, size.y, size.z);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "box construction failed");
        }
        return store_shape(session, builder.Shape(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_cylinder(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis,
    double radius,
    double height,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(origin) || !finite(axis) || !std::isfinite(radius) || !std::isfinite(height)
            || radius <= 0.0 || height <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid cylinder parameters");
        }
        const gp_Vec direction(axis.x, axis.y, axis.z);
        if (direction.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "cylinder axis must be nonzero");
        }
        BRepPrimAPI_MakeCylinder builder(
            gp_Ax2(to_point(origin), gp_Dir(direction)), radius, height);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "cylinder construction failed");
        }
        return store_shape(session, builder.Shape(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_cone(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis,
    double base_radius,
    double top_radius,
    double height,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(origin) || !finite(axis) || !std::isfinite(base_radius)
            || !std::isfinite(top_radius) || !std::isfinite(height)
            || base_radius < 0.0 || top_radius < 0.0
            || (base_radius == 0.0 && top_radius == 0.0) || height <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid cone parameters");
        }
        const gp_Vec direction(axis.x, axis.y, axis.z);
        if (direction.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "cone axis must be nonzero");
        }
        BRepPrimAPI_MakeCone builder(
            gp_Ax2(to_point(origin), gp_Dir(direction)),
            base_radius,
            top_radius,
            height);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "cone construction failed");
        }
        return store_shape(session, builder.Shape(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_sphere(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t center,
    double radius,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(center) || !std::isfinite(radius) || radius <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid sphere parameters");
        }
        BRepPrimAPI_MakeSphere builder(to_point(center), radius);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "sphere construction failed");
        }
        return store_shape(session, builder.Shape(), out_shape);
    });
}

}  // extern "C"
