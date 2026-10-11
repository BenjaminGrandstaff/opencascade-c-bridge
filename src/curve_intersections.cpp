/* Finite XY-plane curve crossings. No stored shapes or sampled polylines. */
#include "bridge_internal.hpp"
#include <BRepAdaptor_Curve.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI.hxx>
#include <Geom2dAPI_InterCurveCurve.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <TopoDS.hxx>
#include <gp_Pln.hxx>
#include <algorithm>
#include <cmath>

using namespace occt_bridge_internal;
namespace {
constexpr double tolerance = 1e-9;
constexpr size_t maximum_span_pairs = 10000;

TopoDS_Edge single_edge(const TopoDS_Shape& shape) {
    if (shape.ShapeType() == TopAbs_EDGE) { return TopoDS::Edge(shape); }
    if (shape.ShapeType() != TopAbs_WIRE) { return {}; }
    TopoDS_Edge edge;
    size_t count = 0;
    for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
        edge = TopoDS::Edge(it.Current());
        ++count;
    }
    return count == 1 ? edge : TopoDS_Edge();
}

bool planar_point(const gp_Pnt& p) {
    return std::isfinite(p.X()) && std::isfinite(p.Y())
        && std::isfinite(p.Z()) && std::abs(p.Z()) <= tolerance;
}

occt_bridge_status_t planar_curve(occt_bridge_session_t* session,
    const TopoDS_Shape& shape, Handle(Geom2d_Curve)& out, size_t& spans) {
    const auto edge = single_edge(shape);
    if (edge.IsNull()) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intersection needs an edge or one-edge wire");
    }
    const BRepAdaptor_Curve adaptor(edge);
    double first = 0., last = 0.;
    const auto curve = BRep_Tool::Curve(edge, first, last);
    if (curve.IsNull() || !std::isfinite(first) || !std::isfinite(last) || last <= first) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "intersection needs finite positive curve ranges");
    }
    bool planar = false;
    spans = 1;
    switch (adaptor.GetType()) {
    case GeomAbs_Line:
        planar = planar_point(adaptor.Value(first)) && planar_point(adaptor.Value(last));
        break;
    case GeomAbs_Circle: {
        const auto c = adaptor.Circle();
        const auto axes = c.Position();
        planar = std::abs(c.Location().Z()) + c.Radius()
            * (std::abs(axes.XDirection().Z()) + std::abs(axes.YDirection().Z())) <= tolerance;
        break;
    }
    case GeomAbs_Ellipse: {
        const auto c = adaptor.Ellipse();
        const auto axes = c.Position();
        planar = std::abs(c.Location().Z()) + c.MajorRadius() * std::abs(axes.XDirection().Z())
            + c.MinorRadius() * std::abs(axes.YDirection().Z()) <= tolerance;
        break;
    }
    case GeomAbs_BSplineCurve: {
        const auto spline = adaptor.BSpline();
        if (spline->NbPoles() > 10000 || spline->NbKnots() > 10001 || spline->Degree() > 25) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intersection exceeds spline input budget");
        }
        planar = true;
        for (int i = 1; i <= spline->NbPoles(); ++i) { planar = planar && planar_point(spline->Pole(i)); }
        spans = static_cast<size_t>(spline->NbKnots() - 1);
        break;
    }
    case GeomAbs_BezierCurve: {
        const auto bezier = adaptor.Bezier();
        if (bezier->Degree() > 25) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intersection exceeds Bezier degree budget");
        }
        planar = true;
        for (int i = 1; i <= bezier->NbPoles(); ++i) { planar = planar && planar_point(bezier->Pole(i)); }
        break;
    }
    default:
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intersection needs lines, circles, ellipses or polynomial/rational splines");
    }
    if (!planar) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intersection curves must lie in the XY plane");
    }
    const gp_Pln plane(gp_Pnt(0., 0., 0.), gp_Dir(0., 0., 1.));
    const auto converted = GeomAPI::To2d(curve, plane);
    if (converted.IsNull()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "intersection curve conversion failed");
    }
    out = new Geom2d_TrimmedCurve(converted, first, last);
    return OCCT_BRIDGE_OK;
}
occt_bridge_status_t collect_crossings(occt_bridge_session_t* session,
    const TopoDS_Shape& first, const TopoDS_Shape& second, size_t maximum_points,
    std::vector<occt_bridge_vec3_t>& points) {
    Handle(Geom2d_Curve) a, b;
    size_t a_spans = 0, b_spans = 0;
    auto status = planar_curve(session, first, a, a_spans);
    if (status != OCCT_BRIDGE_OK) { return status; }
    status = planar_curve(session, second, b, b_spans);
    if (status != OCCT_BRIDGE_OK) { return status; }
    if (a_spans == 0 || b_spans == 0 || a_spans > maximum_span_pairs / b_spans) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intersection exceeds 10000 spline span pairs");
    }
    bool handled = false;
    status = line_spline_intersections(session, single_edge(first), single_edge(second), maximum_points, points, handled);
    if (status != OCCT_BRIDGE_OK) { return status; }
    if (handled) {
        std::sort(points.begin(), points.end(), [](const auto& p, const auto& q) {
            return p.x < q.x || (p.x == q.x && p.y < q.y);
        });
        return OCCT_BRIDGE_OK;
    }
    const Geom2dAPI_InterCurveCurve intersection(a, b, tolerance);
    if (!intersection.Intersector().IsDone()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "native curve intersection did not converge");
    }
    if (intersection.NbSegments() != 0) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "intersection has overlap or unresolved tangential regions");
    }
    if (static_cast<size_t>(intersection.NbPoints()) > maximum_points) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intersection exceeds point budget");
    }
    for (int i = 1; i <= intersection.NbPoints(); ++i) {
        const auto p = intersection.Point(i);
        const occt_bridge_vec3_t point{p.X(), p.Y(), 0.};
        if (!finite(point)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "intersection produced nonfinite coordinates");
        }
        // Collapse seam/repeated reports only at numerical identity scale;
        // distinct nearby crossings still make a hint ambiguous.
        const bool duplicate = std::any_of(points.begin(), points.end(), [&](const auto& q) {
            return std::hypot(point.x - q.x, point.y - q.y) <= 1e-12;
        });
        if (!duplicate) { points.push_back(point); }
    }
    std::sort(points.begin(), points.end(), [](const auto& p, const auto& q) {
        return p.x < q.x || (p.x == q.x && p.y < q.y);
    });
    return OCCT_BRIDGE_OK;
}

}

occt_bridge_status_t occt_bridge_planar_curve_intersections(
    occt_bridge_session_t* session, occt_bridge_shape_id_t first_id,
    occt_bridge_shape_id_t second_id, size_t maximum_points,
    occt_bridge_vec3_t* out_points, size_t capacity, size_t* out_count) {
    if (out_count != nullptr) { *out_count = 0; }
    return guarded(session, [&] {
        if (out_count == nullptr || maximum_points == 0 || maximum_points > 1000
            || (out_points == nullptr && capacity != 0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intersection needs a count and a 1-1000 point budget");
        }
        const auto* first = find_shape(session, first_id);
        const auto* second = find_shape(session, second_id);
        if (first == nullptr || second == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "intersection curve was not found");
        }
        std::vector<occt_bridge_vec3_t> points;
        const auto status = collect_crossings(session, *first, *second, maximum_points, points);
        if (status != OCCT_BRIDGE_OK) { return status; }
        *out_count = points.size();
        if (out_points == nullptr && capacity == 0) { return succeed(session); }
        if (capacity < points.size()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intersection point buffer is too small");
        }
        std::copy(points.begin(), points.end(), out_points);
        return succeed(session);
    });
}
