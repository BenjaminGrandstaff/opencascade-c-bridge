/*
 * Wires from polylines, line/arc segments, splines, circles, and ellipses.
 */

#include "bridge_internal.hpp"

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir.hxx>
#include <gp_Elips.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <Precision.hxx>
#include <TColgp_Array1OfVec.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TColStd_HArray1OfBoolean.hxx>
#include <TopoDS_Wire.hxx>

#include <algorithm>
#include <cmath>
#include <limits>

using namespace occt_bridge_internal;

namespace {

const char* wire_segment_error(const occt_bridge_wire_segment_t& segment) {
    if ((segment.kind != 0 && segment.kind != 1) || !finite(segment.start)
        || !finite(segment.end) || (segment.kind == 1 && !finite(segment.middle))) {
        return "invalid wire segment";
    }
    if (to_point(segment.start).Distance(to_point(segment.end)) <= Precision::Confusion()) {
        return "wire segment endpoints coincide";
    }
    return nullptr;
}

const char* add_wire_segment(BRepBuilderAPI_MakeWire& wire,
                             const occt_bridge_wire_segment_t& segment) {
    const gp_Pnt start = to_point(segment.start);
    const gp_Pnt end = to_point(segment.end);
    if (segment.kind == 1) {
        GC_MakeArcOfCircle arc(start, to_point(segment.middle), end);
        if (!arc.IsDone()) {
            return "invalid circular arc";
        }
        BRepBuilderAPI_MakeEdge edge(arc.Value());
        if (!edge.IsDone()) {
            return "arc edge construction failed";
        }
        wire.Add(edge.Edge());
    } else {
        BRepBuilderAPI_MakeEdge edge(start, end);
        if (!edge.IsDone()) {
            return "line edge construction failed";
        }
        wire.Add(edge.Edge());
    }
    return wire.IsDone() ? nullptr : "segment wire construction failed";
}

/* Rejects non-finite points, zero-length segments, and a repeated closing point. */
const char* polyline_point_error(const occt_bridge_vec3_t* points, size_t point_count, bool closed) {
    for (size_t index = 0; index < point_count; ++index) {
        if (!finite(points[index])) {
            return "wire point is not finite";
        }
        if (index != 0 && to_point(points[index - 1]).Distance(to_point(points[index]))
                <= std::numeric_limits<double>::epsilon()) {
            return "wire contains a zero-length segment";
        }
    }
    if (closed && to_point(points[0]).Distance(to_point(points[point_count - 1]))
            <= std::numeric_limits<double>::epsilon()) {
        return "closed wire must not repeat its first point";
    }
    return nullptr;
}

constexpr int32_t kStartTangent = 1;
constexpr int32_t kEndTangent = 2;
constexpr int32_t kPeriodic = 4;

// Validates one curve segment against the point buffer; null when valid.
const char* curve_segment_error(
    const occt_bridge_curve_segment_t& segment,
    const occt_bridge_vec3_t* points,
    size_t point_count) {
    const size_t expected = segment.kind == 0 ? 2 : segment.kind == 1 ? 3 : 0;
    if (segment.kind < 0 || segment.kind > 2
        || (expected != 0 && segment.point_count != expected)
        || (segment.kind == 2 && (segment.point_count < 2 || (segment.flags & ~7) != 0))
        || segment.first_point > point_count
        || segment.point_count > point_count - segment.first_point) {
        return "invalid curve segment";
    }
    const auto* first = points + segment.first_point;
    if (!std::all_of(first, first + segment.point_count, [](const auto& p) { return finite(p); })) {
        return "curve segment point is not finite";
    }
    for (size_t index = 1; index < segment.point_count; ++index) {
        if (to_point(first[index]).Distance(to_point(first[index - 1])) <= Precision::Confusion()) {
            return "consecutive curve segment points coincide";
        }
    }
    if (segment.kind == 2) {
        for (const auto& [flag, tangent] : {std::pair{kStartTangent, segment.start_tangent},
                                            std::pair{kEndTangent, segment.end_tangent}}) {
            if ((segment.flags & flag) != 0
                && (!finite(tangent) || gp_Vec(tangent.x, tangent.y, tangent.z).Magnitude() <= Precision::Confusion())) {
                return "spline end tangent must be finite and nonzero";
            }
        }
        if ((segment.flags & kPeriodic) != 0 && segment.point_count < 3) {
            return "a periodic spline needs at least three points";
        }
    }
    return nullptr;
}

gp_Pnt segment_start(const occt_bridge_curve_segment_t& segment, const occt_bridge_vec3_t* points) {
    return to_point(points[segment.first_point]);
}

gp_Pnt segment_end(const occt_bridge_curve_segment_t& segment, const occt_bridge_vec3_t* points) {
    const bool periodic = segment.kind == 2 && (segment.flags & kPeriodic) != 0;
    return to_point(points[segment.first_point + (periodic ? 0 : segment.point_count - 1)]);
}

const char* add_spline_segment(
    BRepBuilderAPI_MakeWire& wire,
    const occt_bridge_curve_segment_t& segment,
    const occt_bridge_vec3_t* points) {
    const int count = static_cast<int>(segment.point_count);
    Handle(TColgp_HArray1OfPnt) poles = new TColgp_HArray1OfPnt(1, count);
    for (int index = 0; index < count; ++index) {
        poles->SetValue(index + 1, to_point(points[segment.first_point + static_cast<size_t>(index)]));
    }
    const bool periodic = (segment.flags & kPeriodic) != 0;
    GeomAPI_Interpolate interpolate(poles, periodic ? Standard_True : Standard_False, Precision::Confusion());
    if ((segment.flags & (kStartTangent | kEndTangent)) != 0) {
        TColgp_Array1OfVec tangents(1, count);
        Handle(TColStd_HArray1OfBoolean) used = new TColStd_HArray1OfBoolean(1, count, Standard_False);
        for (int index = 1; index <= count; ++index) {
            tangents.SetValue(index, gp_Vec(0.0, 0.0, 0.0));
        }
        if ((segment.flags & kStartTangent) != 0) {
            const auto& t = segment.start_tangent;
            tangents.SetValue(1, gp_Vec(t.x, t.y, t.z));
            used->SetValue(1, Standard_True);
        }
        if ((segment.flags & kEndTangent) != 0) {
            const auto& t = segment.end_tangent;
            tangents.SetValue(count, gp_Vec(t.x, t.y, t.z));
            used->SetValue(count, Standard_True);
        }
        interpolate.Load(tangents, used, Standard_True);
    }
    interpolate.Perform();
    if (!interpolate.IsDone()) {
        return "spline interpolation failed";
    }
    BRepBuilderAPI_MakeEdge edge(interpolate.Curve());
    if (!edge.IsDone()) {
        return "spline edge construction failed";
    }
    wire.Add(edge.Edge());
    return wire.IsDone() ? nullptr : "spline edge does not connect";
}

// Appends a validated line, arc, or spline segment; null on success.
const char* add_curve_segment(
    BRepBuilderAPI_MakeWire& wire,
    const occt_bridge_curve_segment_t& segment,
    const occt_bridge_vec3_t* points) {
    if (segment.kind == 2) {
        return add_spline_segment(wire, segment, points);
    }
    const auto* first = points + segment.first_point;
    const occt_bridge_wire_segment_t simple{
        segment.kind, first[0], segment.kind == 1 ? first[1] : first[0],
        first[segment.point_count - 1]};
    return add_wire_segment(wire, simple);
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_create_polyline_wire(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    size_t point_count,
    int closed,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const size_t minimum_points = closed == 1 ? 3 : 2;
        if (points == nullptr || (closed != 0 && closed != 1) || point_count < minimum_points) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid polyline-wire parameters");
        }
        if (const char* error = polyline_point_error(points, point_count, closed == 1)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
        }
        BRepBuilderAPI_MakePolygon builder;
        for (size_t index = 0; index < point_count; ++index) {
            builder.Add(to_point(points[index]));
        }
        if (closed == 1) {
            builder.Close();
        }
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "polyline wire construction failed");
        }
        return store_shape(session, builder.Wire(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_segment_wire(
    occt_bridge_session_t* session,
    const occt_bridge_wire_segment_t* segments,
    size_t segment_count,
    int closed,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (segments == nullptr || segment_count == 0 || (closed != 0 && closed != 1)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid segment-wire parameters");
        }
        BRepBuilderAPI_MakeWire wire;
        for (size_t index = 0; index < segment_count; ++index) {
            const auto& segment = segments[index];
            if (const char* error = wire_segment_error(segment)) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
            }
            if (index > 0 && to_point(segment.start).Distance(to_point(segments[index - 1].end)) > Precision::Confusion()) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "wire segments are disconnected");
            }
            if (const char* error = add_wire_segment(wire, segment)) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
            }
        }
        if (closed == 1 && to_point(segments[0].start).Distance(to_point(segments[segment_count - 1].end)) > Precision::Confusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "segment wire is not closed");
        }
        return store_shape(session, wire.Wire(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_curve_wire(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    size_t point_count,
    const occt_bridge_curve_segment_t* segments,
    size_t segment_count,
    int closed,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (points == nullptr || segments == nullptr || segment_count == 0
            || (closed != 0 && closed != 1)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid curve-wire parameters");
        }
        BRepBuilderAPI_MakeWire wire;
        for (size_t index = 0; index < segment_count; ++index) {
            const auto& segment = segments[index];
            if (const char* error = curve_segment_error(segment, points, point_count)) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
            }
            if (index > 0
                && segment_start(segment, points).Distance(segment_end(segments[index - 1], points))
                    > Precision::Confusion()) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curve segments are disconnected");
            }
            if (const char* error = add_curve_segment(wire, segment, points)) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
            }
        }
        if (closed == 1
            && segment_start(segments[0], points).Distance(segment_end(segments[segment_count - 1], points))
                > Precision::Confusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curve wire is not closed");
        }
        return store_shape(session, wire.Wire(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_circle_wire(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t center,
    occt_bridge_vec3_t normal,
    double radius,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(center) || !finite(normal) || !std::isfinite(radius) || radius <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid circle-wire parameters");
        }
        const gp_Vec direction(normal.x, normal.y, normal.z);
        if (direction.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "circle normal must be nonzero");
        }
        BRepBuilderAPI_MakeEdge edge(
            gp_Circ(
                gp_Ax2(to_point(center), gp_Dir(direction)),
                radius));
        if (!edge.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "circle edge construction failed");
        }
        BRepBuilderAPI_MakeWire wire(edge.Edge());
        if (!wire.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "circle wire construction failed");
        }
        return store_shape(session, wire.Wire(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_ellipse_wire(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t center,
    occt_bridge_vec3_t normal,
    double major_radius,
    double minor_radius,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(center) || !finite(normal) || !std::isfinite(major_radius)
            || !std::isfinite(minor_radius) || minor_radius <= 0.0
            || major_radius < minor_radius) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid ellipse-wire parameters");
        }
        const gp_Vec direction(normal.x, normal.y, normal.z);
        if (direction.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "ellipse normal must be nonzero");
        }
        BRepBuilderAPI_MakeEdge edge(gp_Elips(
            gp_Ax2(to_point(center), gp_Dir(direction)),
            major_radius,
            minor_radius));
        if (!edge.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "ellipse edge construction failed");
        }
        BRepBuilderAPI_MakeWire wire(edge.Edge());
        if (!wire.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "ellipse wire construction failed");
        }
        return store_shape(session, wire.Wire(), out_shape);
    });
}

}  // extern "C"
