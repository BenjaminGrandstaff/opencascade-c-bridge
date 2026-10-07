/* Exact finite curve conversion to adjacent rational Bezier spans. */
#include "bridge_internal.hpp"
#include <BRepAdaptor_Curve.hxx>
#include <BRep_Tool.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_BSplineCurveToBezierCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <TopoDS.hxx>
#include <algorithm>
#include <cmath>

using namespace occt_bridge_internal;

namespace {
using Poles = std::vector<occt_bridge_bezier_pole_t>;

occt_bridge_status_t append_span(
    occt_bridge_session_t* session, const Handle(Geom_BezierCurve)& arc,
    bool reverse, size_t span, size_t maximum_poles, Poles& poles) {
    const auto count = static_cast<size_t>(arc->NbPoles());
    if (count > maximum_poles - poles.size()) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Bezier export exceeds output pole budget");
    }
    for (int index = 1; index <= arc->NbPoles(); ++index) {
        const int pole_index = reverse ? arc->NbPoles() + 1 - index : index;
        const auto point = arc->Pole(pole_index);
        const double weight = arc->Weight(pole_index);
        const occt_bridge_vec3_t position{point.X(), point.Y(), point.Z()};
        if (!finite(position) || !std::isfinite(weight) || weight <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "Bezier poles need finite coordinates and positive weights");
        }
        poles.push_back({span, position, weight});
    }
    return OCCT_BRIDGE_OK;
}

// O(P) pole validation/copy/storage for P returned poles, plus kernel knot
// insertion/conversion costs depending on degree and knot multiplicities.
occt_bridge_status_t collect_poles(
    occt_bridge_session_t* session, const TopoDS_Edge& edge,
    size_t maximum_poles, Poles& poles) {
    BRepAdaptor_Curve adaptor(edge);
    if (adaptor.GetType() == GeomAbs_OtherCurve || adaptor.GetType() == GeomAbs_OffsetCurve) {
        return OCCT_BRIDGE_OK;
    }
    if (adaptor.GetType() == GeomAbs_BSplineCurve && static_cast<size_t>(adaptor.NbPoles()) > maximum_poles) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Bezier export exceeds input pole budget");
    }
    double first = 0.0, last = 0.0;
    const auto curve = BRep_Tool::Curve(edge, first, last);
    if (curve.IsNull() || !std::isfinite(first) || !std::isfinite(last) || last <= first) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "Bezier export needs a finite positive edge range");
    }
    const Handle(Geom_TrimmedCurve) trimmed = new Geom_TrimmedCurve(curve, first, last);
    const auto spline = GeomConvert::CurveToBSplineCurve(trimmed);
    GeomConvert_BSplineCurveToBezierCurve converter(spline);
    const int spans = converter.NbArcs();
    if (static_cast<size_t>(spans) > maximum_poles / 2) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Bezier export exceeds span budget");
    }
    const bool reverse = edge.Orientation() == TopAbs_REVERSED;
    for (int span = 0; span < spans; ++span) {
        const auto arc = converter.Arc(reverse ? spans - span : span + 1);
        const auto status = append_span(session, arc, reverse, static_cast<size_t>(span), maximum_poles, poles);
        if (status != OCCT_BRIDGE_OK) { return status; }
    }
    return OCCT_BRIDGE_OK;
}
}

// Returned poles/storage are bounded; inputs are copied, never modified.
occt_bridge_status_t occt_bridge_edge_bezier_poles(
    occt_bridge_session_t* session, occt_bridge_shape_id_t edge, size_t maximum_poles,
    occt_bridge_bezier_pole_t* out_poles, size_t capacity, size_t* out_count) {
    if (out_count != nullptr) { *out_count = 0; }
    return guarded(session, [&] {
        if (out_count == nullptr || maximum_poles < 2 || maximum_poles > 1000000
            || (out_poles == nullptr && capacity != 0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Bezier export needs a count and a 2–1000000 pole budget");
        }
        const auto* value = find_shape(session, edge);
        if (value == nullptr) { return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found"); }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Bezier export requires an edge");
        }
        Poles poles;
        const auto status = collect_poles(session, TopoDS::Edge(*value), maximum_poles, poles);
        if (status != OCCT_BRIDGE_OK) { return status; }
        *out_count = poles.size();
        if (out_poles == nullptr && capacity == 0) { return succeed(session); }
        if (capacity < poles.size()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Bezier pole buffer is too small");
        }
        std::copy(poles.begin(), poles.end(), out_poles);
        return succeed(session);
    });
}
