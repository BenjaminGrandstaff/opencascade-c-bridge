/*
 * Wires from polylines, line/arc segments, splines, circles, and ellipses.
 */

#include "bridge_internal.hpp"

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRep_Tool.hxx>
#include <ElCLib.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom_Ellipse.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom_BSplineCurve.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColStd_Array1OfInteger.hxx>
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
#include <TopoDS_Vertex.hxx>
#include <TopoDS.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <Geom_Curve.hxx>

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
constexpr int32_t kNonClamped = 8;
constexpr int32_t kClockwise = 1;

/* Native ellipse through kind-4 points (center, major vertex, minor vertex),
 * with its axes ordered so the major radius is not smaller; null on error.
 * The normal is (major − center) × (minor − center) whichever axis is longer. */
const char* segment_ellipse(const occt_bridge_vec3_t* p, gp_Elips& ellipse) {
    const gp_Pnt center = to_point(p[0]);
    const gp_Vec u(center, to_point(p[1])), v(center, to_point(p[2]));
    const double a = u.Magnitude(), b = v.Magnitude();
    if (a <= Precision::Confusion() || b <= Precision::Confusion()) {
        return "elliptical arc needs positive semi-axes";
    }
    if (std::abs(u.Dot(v)) > 1e-9 * a * b) {
        return "elliptical arc axes are not perpendicular";
    }
    const gp_Dir normal(u.Crossed(v));
    ellipse = a >= b ? gp_Elips(gp_Ax2(center, normal, gp_Dir(u)), a, b)
                     : gp_Elips(gp_Ax2(center, normal, gp_Dir(v)), b, a);
    return nullptr;
}

/* Exact trimmed ellipse from start to end (kind 4), counterclockwise about
 * the ellipse normal unless flagged clockwise. Ends must lie on the ellipse
 * within 1e-7 of the minor radius. */
const char* add_elliptical_arc(
    BRepBuilderAPI_MakeWire& wire,
    const occt_bridge_curve_segment_t& segment,
    const occt_bridge_vec3_t* points) {
    const auto* p = points + segment.first_point;
    gp_Elips ellipse;
    if (const char* error = segment_ellipse(p, ellipse)) return error;
    const gp_Pnt start = to_point(p[3]), end = to_point(p[4]);
    const auto off_curve = [&](const gp_Pnt& point) {
        const gp_Vec d(ellipse.Location(), point);
        const gp_Ax2& frame = ellipse.Position();
        const double x = d.Dot(gp_Vec(frame.XDirection())) / ellipse.MajorRadius();
        const double y = d.Dot(gp_Vec(frame.YDirection())) / ellipse.MinorRadius();
        return std::abs(std::hypot(x, y) - 1.0) * ellipse.MinorRadius() > 1e-7
            || std::abs(d.Dot(gp_Vec(frame.Direction()))) > 1e-7;
    };
    if (off_curve(start) || off_curve(end)) return "elliptical arc ends are not on the ellipse";
    if (start.Distance(end) <= Precision::Confusion()) return "elliptical arc ends coincide";
    const opencascade::handle<Geom_Ellipse> curve = new Geom_Ellipse(ellipse);
    const double u1 = ElCLib::Parameter(ellipse, start), u2 = ElCLib::Parameter(ellipse, end);
    opencascade::handle<Geom_TrimmedCurve> arc;
    if ((segment.flags & kClockwise) == 0) {
        arc = new Geom_TrimmedCurve(curve, u1, u2, Standard_True, Standard_True);
    } else {
        arc = new Geom_TrimmedCurve(curve, u2, u1, Standard_True, Standard_True);
        arc->Reverse();
    }
    BRepBuilderAPI_MakeEdge edge(arc);
    if (!edge.IsDone()) return "elliptical arc edge construction failed";
    wire.Add(edge.Edge());
    return wire.IsDone() ? nullptr : "elliptical arc does not connect";
}

// Validate descriptor bounds before reading any point, knot or weight buffer.
const char* curve_descriptor_error(const occt_bridge_curve_segment_t& segment, size_t point_count) {
    const size_t expected = segment.kind == 0 ? 2 : segment.kind == 1 ? 3 : segment.kind == 4 ? 5 : 0;
    if (segment.kind < 0 || segment.kind > 4
        || (expected != 0 && segment.point_count != expected)
        || (segment.kind == 2 && (segment.point_count < 2 || (segment.flags & ~7) != 0))
        || (segment.kind == 4 && (segment.flags & ~kClockwise) != 0)
        || segment.first_point > point_count
        || segment.point_count > point_count - segment.first_point) {
        return "invalid curve segment";
    }
    return nullptr;
}

const char* bspline_knot_error(const occt_bridge_curve_segment_t& segment,
    bool periodic, bool non_clamped, size_t& sum) {
    for (size_t i = 0; i < segment.knot_count; ++i) {
        const bool boundary = (i == 0 || i + 1 == segment.knot_count);
        const bool clamped_end = !periodic && !non_clamped && (i == 0 || i + 1 == segment.knot_count);
        const int maximum = !periodic && boundary ? segment.degree + 1 : segment.degree;
        if (!std::isfinite(segment.knots[i]) || (i != 0 && segment.knots[i] <= segment.knots[i-1])
            || segment.multiplicities[i] < 1 || segment.multiplicities[i] > maximum
            || (clamped_end && segment.multiplicities[i] != maximum)) {
            return "invalid clamped B-spline knots or multiplicities";
        }
        sum += static_cast<size_t>(segment.multiplicities[i]);
    }
    return nullptr;
}

const char* explicit_bspline_error(const occt_bridge_curve_segment_t& segment) {
    const bool periodic = (segment.flags & kPeriodic) != 0;
    const bool non_clamped = (segment.flags & kNonClamped) != 0;
    if ((periodic && non_clamped) || (segment.flags & ~(kPeriodic | kNonClamped)) != 0 || segment.degree < 1 || segment.degree > Geom_BSplineCurve::MaxDegree()
        || segment.point_count < (static_cast<size_t>(segment.degree) + 1) || segment.point_count > 10000
        || segment.knot_count < 2 || segment.knot_count > 10002 || segment.knots == nullptr || segment.multiplicities == nullptr
        || (segment.weight_count != 0 && (segment.weight_count != segment.point_count || segment.weights == nullptr))) {
        return "invalid explicit B-spline sizes or degree";
    }
    size_t sum = 0;
    if (const char* error = bspline_knot_error(segment, periodic, non_clamped, sum)) return error;
    if (periodic && segment.multiplicities[0] != segment.multiplicities[segment.knot_count-1]) {
        return "periodic B-spline endpoint multiplicities must match";
    }
    const size_t expected_sum = segment.point_count + (periodic ? static_cast<size_t>(segment.multiplicities[segment.knot_count-1]) : static_cast<size_t>(segment.degree) + 1);
    if (sum != expected_sum) {
        return "B-spline multiplicity sum does not match poles and degree";
    }
    for (size_t i = 0; i < segment.weight_count; ++i) {
        if (!std::isfinite(segment.weights[i]) || segment.weights[i] <= 0.) return "B-spline weights must be positive and finite";
    }
    return nullptr;
}

const char* interpolated_spline_error(const occt_bridge_curve_segment_t& segment) {
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
    return nullptr;
}

// Validates one curve segment against the point buffer; null when valid.
const char* curve_segment_error(
    const occt_bridge_curve_segment_t& segment,
    const occt_bridge_vec3_t* points,
    size_t point_count) {
    if (const char* error = curve_descriptor_error(segment, point_count)) return error;
    const auto* first = points + segment.first_point;
    if (!std::all_of(first, first + segment.point_count, [](const auto& p) { return finite(p); })) {
        return "curve segment point is not finite";
    }
    for (size_t index = 1; index < segment.point_count; ++index) {
        if (segment.kind != 3 && segment.kind != 4 && to_point(first[index]).Distance(to_point(first[index - 1])) <= Precision::Confusion()) {
            return "consecutive curve segment points coincide";
        }
    }
    if (segment.kind == 3) return explicit_bspline_error(segment);
    if (segment.kind == 2) return interpolated_spline_error(segment);
    return nullptr;
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

const char* add_explicit_bspline(BRepBuilderAPI_MakeWire& wire,
    const occt_bridge_curve_segment_t& segment, const occt_bridge_vec3_t* points) {
    const int count = static_cast<int>(segment.point_count);
    const int knot_count = static_cast<int>(segment.knot_count);
    TColgp_Array1OfPnt poles(1, count);
    TColStd_Array1OfReal weights(1, count), knots(1, knot_count);
    TColStd_Array1OfInteger mults(1, knot_count);
    for (int i = 1; i <= count; ++i) {
        poles.SetValue(i, to_point(points[segment.first_point + static_cast<size_t>(i-1)]));
        weights.SetValue(i, segment.weight_count == 0 ? 1. : segment.weights[i-1]);
    }
    for (int i = 1; i <= knot_count; ++i) { knots.SetValue(i, segment.knots[i-1]); mults.SetValue(i, segment.multiplicities[i-1]); }
    Handle(Geom_BSplineCurve) curve = new Geom_BSplineCurve(poles, weights, knots, mults, segment.degree, (segment.flags & kPeriodic) != 0 ? Standard_True : Standard_False);
    BRepBuilderAPI_MakeEdge edge(curve);
    if (!edge.IsDone()) return "explicit B-spline edge construction failed";
    wire.Add(edge.Edge());
    return wire.IsDone() ? nullptr : "explicit B-spline edge does not connect";
}

// Appends a validated line, arc, or spline segment; null on success.
const char* add_curve_segment(
    BRepBuilderAPI_MakeWire& wire,
    const occt_bridge_curve_segment_t& segment,
    const occt_bridge_vec3_t* points) {
    if (segment.kind == 3) return add_explicit_bspline(wire, segment, points);
    if (segment.kind == 4) return add_elliptical_arc(wire, segment, points);
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
        gp_Pnt first_point, previous_end;
        for (size_t index = 0; index < segment_count; ++index) {
            const auto& segment = segments[index];
            if (const char* error = curve_segment_error(segment, points, point_count)) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
            }
            if (segment.kind == 3 && (segment.flags & kPeriodic) != 0 && (segment_count != 1 || closed != 1)) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "periodic explicit B-spline requires a standalone closed wire");
            }
            BRepBuilderAPI_MakeWire piece;
            if (const char* error = add_curve_segment(piece, segment, points)) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
            }
            TopoDS_Vertex a, b;
            TopExp::Vertices(piece.Wire(), a, b);
            if (a.IsNull() || b.IsNull()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve segment has no native endpoints");
            const gp_Pnt start = BRep_Tool::Pnt(a), end = BRep_Tool::Pnt(b);
            if (index == 0) first_point = start;
            else if (start.Distance(previous_end) > Precision::Confusion()) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curve segments are disconnected");
            }
            previous_end = end;
            wire.Add(piece.Wire());
            if (!wire.IsDone()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve wire construction failed");
        }
        if (closed == 1 && first_point.Distance(previous_end) > Precision::Confusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curve wire is not closed");
        }
        if (closed == 1 && !BRep_Tool::IsClosed(wire.Wire())) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve wire did not form a closed native boundary");
        }
        return store_shape(session, wire.Wire(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_curve_point_at_fraction(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    double fraction, occt_bridge_vec3_t* point) {
    return guarded(session, [&] {
        if (!point) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curve point output is null");
        *point = {0., 0., 0.};
        if (!std::isfinite(fraction) || fraction < 0. || fraction > 1.)
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curve fraction must be in [0, 1]");
        const auto* value = find_shape(session, shape);
        if (!value) return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "curve shape was not found");
        TopoDS_Edge edge;
        if (value->ShapeType() == TopAbs_EDGE) edge = TopoDS::Edge(*value);
        else if (value->ShapeType() == TopAbs_WIRE) {
            TopExp_Explorer it(*value, TopAbs_EDGE);
            if (!it.More()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve wire has no edge");
            edge = TopoDS::Edge(it.Current()); it.Next();
            if (it.More()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve point query requires one edge");
        } else return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve point query requires an edge or wire");
        Standard_Real first, last;
        TopLoc_Location location;
        Handle(Geom_Curve) curve = BRep_Tool::Curve(edge, location, first, last);
        if (curve.IsNull() || !std::isfinite(first) || !std::isfinite(last))
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge has no finite native curve domain");
        if (edge.Orientation() == TopAbs_REVERSED) fraction = 1. - fraction;
        const double u = fraction == 0. ? first : fraction == 1. ? last : (1. - fraction) * first + fraction * last;
        gp_Pnt p = curve->Value(u);
        p.Transform(location.Transformation());
        *point = {p.X(), p.Y(), p.Z()};
        if (!finite(*point)) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve point is not finite");
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_curve_end_data(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t* start, occt_bridge_vec3_t* end,
    occt_bridge_vec3_t* start_tangent, occt_bridge_vec3_t* end_tangent) {
    return guarded(session, [&] {
        if (!start || !end || !start_tangent || !end_tangent) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curve endpoint outputs are null");
        *start = *end = *start_tangent = *end_tangent = {0.,0.,0.};
        const auto* value = find_shape(session, shape);
        if (!value) return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "curve shape was not found");
        TopoDS_Edge edge;
        if (value->ShapeType() == TopAbs_EDGE) edge = TopoDS::Edge(*value);
        else if (value->ShapeType() == TopAbs_WIRE) {
            TopExp_Explorer it(*value, TopAbs_EDGE);
            if (!it.More()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve wire has no edge");
            edge = TopoDS::Edge(it.Current()); it.Next();
            if (it.More()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve endpoint query requires one edge");
        } else return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve endpoint query requires an edge or wire");
        Standard_Real first, last;
        TopLoc_Location location;
        Handle(Geom_Curve) curve = BRep_Tool::Curve(edge, location, first, last);
        if (curve.IsNull()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge has no native curve");
        gp_Pnt a, b; gp_Vec u, v;
        curve->D1(first, a, u); curve->D1(last, b, v);
        a.Transform(location.Transformation()); b.Transform(location.Transformation());
        u.Transform(location.Transformation()); v.Transform(location.Transformation());
        if (edge.Orientation() == TopAbs_REVERSED) { std::swap(a,b); std::swap(u,v); u.Reverse(); v.Reverse(); }
        *start = {a.X(),a.Y(),a.Z()}; *end = {b.X(),b.Y(),b.Z()};
        *start_tangent = {u.X(),u.Y(),u.Z()}; *end_tangent = {v.X(),v.Y(),v.Z()};
        if (!finite(*start) || !finite(*end) || !finite(*start_tangent) || !finite(*end_tangent)) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "curve endpoint data is not finite");
        return succeed(session);
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
