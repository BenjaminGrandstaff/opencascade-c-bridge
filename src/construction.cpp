/*
 * Construction from profiles: faces, prisms, revolutions, tubes, lofts,
 * and compounds.
 */

#include "bridge_internal.hpp"

#include <BRep_Builder.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepOffsetAPI_MakePipe.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <Precision.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Wire.hxx>

#include <algorithm>
#include <cmath>
#include <limits>

using namespace occt_bridge_internal;

namespace {

TopoDS_Wire polygon_wire(const occt_bridge_vec3_t* points, size_t point_count) {
    BRepBuilderAPI_MakePolygon wire;
    for (size_t index = 0; index < point_count; ++index) {
        wire.Add(to_point(points[index]));
    }
    wire.Close();
    return wire.IsDone() ? wire.Wire() : TopoDS_Wire{};
}

// One non-periodic B-spline through every point and back to the first, so
// the curve is smooth everywhere except at the first point (a sharp corner
// such as an airfoil trailing edge). Null when consecutive points coincide.
TopoDS_Wire spline_wire(const occt_bridge_vec3_t* points, size_t point_count) {
    Handle(TColgp_HArray1OfPnt) poles = new TColgp_HArray1OfPnt(1, static_cast<int>(point_count) + 1);
    for (size_t index = 0; index <= point_count; ++index) {
        const auto& point = points[index % point_count];
        poles->SetValue(static_cast<int>(index) + 1, to_point(point));
    }
    for (int index = 1; index <= poles->Length() - 1; ++index) {
        if (poles->Value(index).Distance(poles->Value(index + 1)) <= Precision::Confusion()) {
            return TopoDS_Wire{};
        }
    }
    GeomAPI_Interpolate interpolate(poles, Standard_False, Precision::Confusion());
    interpolate.Perform();
    if (!interpolate.IsDone()) {
        return TopoDS_Wire{};
    }
    BRepBuilderAPI_MakeEdge edge(interpolate.Curve());
    if (!edge.IsDone()) {
        return TopoDS_Wire{};
    }
    BRepBuilderAPI_MakeWire wire(edge.Edge());
    return wire.IsDone() ? wire.Wire() : TopoDS_Wire{};
}

/* Builds the straight-segment sweep path; on failure records the error and returns its status. */
occt_bridge_status_t make_tube_spine(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* path_points,
    size_t point_count,
    TopoDS_Wire& spine) {
    BRepBuilderAPI_MakeWire spine_builder;
    for (size_t index = 1; index < point_count; ++index) {
        const gp_Pnt first = to_point(path_points[index - 1]);
        const gp_Pnt second = to_point(path_points[index]);
        if (first.Distance(second) <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "tube path contains a zero-length segment");
        }
        BRepBuilderAPI_MakeEdge edge(first, second);
        if (!edge.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "tube path edge construction failed");
        }
        spine_builder.Add(edge.Edge());
    }
    if (!spine_builder.IsDone()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "tube path wire construction failed");
    }
    spine = spine_builder.Wire();
    return OCCT_BRIDGE_OK;
}

/* Validates section sizes (at least three points, no overflow) and point finiteness. */
occt_bridge_status_t validate_loft_points(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    const size_t* section_point_counts,
    size_t section_count) {
    size_t total_points = 0;
    for (size_t section = 0; section < section_count; ++section) {
        const size_t count = section_point_counts[section];
        if (count < 3 || total_points > std::numeric_limits<size_t>::max() - count) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid loft section size");
        }
        total_points += count;
    }
    if (!std::all_of(points, points + total_points, [](const occt_bridge_vec3_t& point) {
            return finite(point);
        })) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "loft point is not finite");
    }
    return OCCT_BRIDGE_OK;
}

// Shared by polygon and spline lofts; `section_wire` builds one closed section.
occt_bridge_status_t build_loft(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    const size_t* section_point_counts,
    size_t section_count,
    int make_solid,
    int ruled,
    TopoDS_Wire (*section_wire)(const occt_bridge_vec3_t*, size_t),
    occt_bridge_shape_id_t* out_shape) {
    if (out_shape == nullptr) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
    }
    *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    if (points == nullptr || section_point_counts == nullptr || section_count < 2
        || (make_solid != 0 && make_solid != 1) || (ruled != 0 && ruled != 1)) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid loft parameters");
    }
    const occt_bridge_status_t points_status =
        validate_loft_points(session, points, section_point_counts, section_count);
    if (points_status != OCCT_BRIDGE_OK) {
        return points_status;
    }
    BRepOffsetAPI_ThruSections loft(
        make_solid ? Standard_True : Standard_False,
        ruled ? Standard_True : Standard_False);
    size_t offset = 0;
    for (size_t section = 0; section < section_count; ++section) {
        const size_t count = section_point_counts[section];
        const TopoDS_Wire wire = section_wire(points + offset, count);
        if (wire.IsNull()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "loft section could not be closed");
        }
        loft.AddWire(wire);
        offset += count;
    }
    loft.CheckCompatibility(Standard_True);
    loft.Build();
    if (!loft.IsDone()) {
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "loft construction failed");
    }
    const TopoDS_Shape shape = loft.Shape();
    if (shape.IsNull()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "loft produced a null shape");
    }
    const ShapeValidator analyzer(shape);
    if (!analyzer.IsValid()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "loft produced an invalid BREP");
    }
    return store_shape(session, shape, out_shape);
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_create_face_from_wire(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t wire,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const TopoDS_Shape* value = find_shape(session, wire);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "wire was not found");
        }
        if (value->ShapeType() != TopAbs_WIRE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not a wire");
        }
        BRepBuilderAPI_MakeFace builder(TopoDS::Wire(*value));
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "wire does not define a planar face");
        }
        return store_shape_with_history(
            session,
            builder.Face(),
            out_shape,
            builder,
            {value});
    });
}

occt_bridge_status_t occt_bridge_create_prism_from_face(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    occt_bridge_vec3_t direction,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(direction)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "prism direction must be finite");
        }
        const gp_Vec vector(direction.x, direction.y, direction.z);
        if (vector.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "prism direction must be nonzero");
        }
        const TopoDS_Shape* value = find_shape(session, face);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "face was not found");
        }
        if (value->ShapeType() != TopAbs_FACE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not a face");
        }
        BRepPrimAPI_MakePrism builder(TopoDS::Face(*value), vector, Standard_True);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "prism construction failed");
        }
        return store_shape_with_history(
            session,
            builder.Shape(),
            out_shape,
            builder,
            {value});
    });
}

occt_bridge_status_t occt_bridge_create_revolve_from_face(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis,
    double angle_radians,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(origin) || !finite(axis)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "revolve origin and axis must be finite");
        }
        if (!std::isfinite(angle_radians) || angle_radians == 0.0
            || std::abs(angle_radians) > 2.0 * std::acos(-1.0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "revolve angle must be nonzero and within one revolution");
        }
        const gp_Vec direction(axis.x, axis.y, axis.z);
        if (direction.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "revolve axis must be nonzero");
        }
        const TopoDS_Shape* value = find_shape(session, face);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "face was not found");
        }
        if (value->ShapeType() != TopAbs_FACE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not a face");
        }
        BRepPrimAPI_MakeRevol builder(TopoDS::Face(*value),
            gp_Ax1(to_point(origin), gp_Dir(direction)), angle_radians, Standard_True);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "revolve construction failed");
        }
        return store_checked_result(session, "revolve", builder.Shape(), out_shape, builder, {value});
    });
}

occt_bridge_status_t occt_bridge_create_polygon_prism(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    size_t point_count,
    occt_bridge_vec3_t direction,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (points == nullptr || point_count < 3 || !finite(direction)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "a polygon prism requires at least three finite points");
        }
        const double direction_length = std::hypot(direction.x, std::hypot(direction.y, direction.z));
        if (direction_length <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "prism direction must be nonzero");
        }
        BRepBuilderAPI_MakePolygon polygon;
        for (size_t index = 0; index < point_count; ++index) {
            if (!finite(points[index])) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "polygon point is not finite");
            }
            polygon.Add(to_point(points[index]));
        }
        polygon.Close();
        if (!polygon.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "polygon wire could not be closed");
        }
        BRepBuilderAPI_MakeFace face(polygon.Wire());
        if (!face.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "polygon does not define a planar face");
        }
        BRepPrimAPI_MakePrism prism(
            face.Face(), gp_Vec(direction.x, direction.y, direction.z), Standard_True);
        prism.Build();
        if (!prism.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "prism construction failed");
        }
        /* Degenerate outlines, such as collinear points, sweep to invalid zero-volume solids. */
        const ShapeValidator analyzer(prism.Shape());
        if (!analyzer.IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "polygon prism is not a valid BREP");
        }
        return store_shape(session, prism.Shape(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_polyline_tube(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* path_points,
    size_t point_count,
    double radius,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (path_points == nullptr || point_count < 2 || !std::isfinite(radius) || radius <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid polyline-tube parameters");
        }
        if (!std::all_of(path_points, path_points + point_count, [](const occt_bridge_vec3_t& point) {
                return finite(point);
            })) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "tube path point is not finite");
        }
        TopoDS_Wire spine;
        const occt_bridge_status_t spine_status = make_tube_spine(session, path_points, point_count, spine);
        if (spine_status != OCCT_BRIDGE_OK) {
            return spine_status;
        }

        const gp_Pnt start = to_point(path_points[0]);
        const gp_Vec initial_tangent(start, to_point(path_points[1]));
        const gp_Circ profile_circle(gp_Ax2(start, gp_Dir(initial_tangent)), radius);
        BRepBuilderAPI_MakeEdge profile_edge(profile_circle);
        BRepBuilderAPI_MakeWire profile_wire(profile_edge.Edge());
        if (!profile_edge.IsDone() || !profile_wire.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "tube profile construction failed");
        }

        BRepOffsetAPI_MakePipe pipe(spine, profile_wire.Wire());
        pipe.Build();
        if (!pipe.IsDone() || pipe.Shape().IsNull()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "polyline tube sweep failed");
        }
        const ShapeValidator analyzer(pipe.Shape());
        if (!analyzer.IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "polyline tube is not a valid BREP");
        }
        return store_shape(session, pipe.Shape(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_loft(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    const size_t* section_point_counts,
    size_t section_count,
    int make_solid,
    int ruled,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return build_loft(session, points, section_point_counts, section_count, make_solid,
            ruled, polygon_wire, out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_spline_loft(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    const size_t* section_point_counts,
    size_t section_count,
    int make_solid,
    int ruled,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return build_loft(session, points, section_point_counts, section_count, make_solid,
            ruled, spline_wire, out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_compound(
    occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* shapes,
    size_t shape_count,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (shapes == nullptr || shape_count == 0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "compound requires at least one shape");
        }
        TopoDS_Compound compound;
        BRep_Builder builder;
        builder.MakeCompound(compound);
        for (size_t index = 0; index < shape_count; ++index) {
            const TopoDS_Shape* shape = find_shape(session, shapes[index]);
            if (shape == nullptr) {
                return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "compound child shape was not found");
            }
            builder.Add(compound, *shape);
        }
        return store_compound_with_history(session, compound, shapes, shape_count, out_shape);
    });
}

}  // extern "C"
