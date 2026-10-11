/* Finite line/spline roots in rational Bernstein form, including even roots.
 * Derivative roots partition [0,1] into monotone intervals. No curve sampling. */
#include "bridge_internal.hpp"
#include <BRepAdaptor_Curve.hxx>
#include <BRep_Tool.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_BSplineCurveToBezierCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <TopoDS.hxx>
#include <algorithm>
#include <cmath>
#include <limits>
#include <numeric>
#include <iterator>

using namespace occt_bridge_internal;
namespace {
using Polynomial = std::vector<long double>;
constexpr size_t maximum_work = 1000000;
constexpr double contact_tolerance = 1e-9;

struct RootWork {
    size_t used = 0;
    bool exhausted = false;
    bool uncertain = false;
    bool charge(size_t amount) {
        if (amount > maximum_work - used) { exhausted = true; return false; }
        used += amount;
        return true;
    }
};
long double evaluate(Polynomial values, long double t, RootWork& work) {
    if (!work.charge(values.size() * values.size())) { return 0.; }
    for (size_t n = values.size(); n > 1; --n) {
        for (size_t i = 0; i + 1 < n; ++i) {
            values[i] = (1. - t) * values[i] + t * values[i + 1];
        }
    }
    return values[0];
}
void append_root(std::vector<long double>& roots, long double t) {
    if (roots.empty() || roots.back() != t) { roots.push_back(t); }
}
bool normalize(Polynomial& values) {
    const auto scale = std::accumulate(values.begin(), values.end(), 0.L,
        [](long double current, long double value) { return std::max(current, std::abs(value)); });
    if (scale == 0.) { return false; }
    std::transform(values.begin(), values.end(), values.begin(), [scale](long double value) { return value / scale; });
    return true;
}
long double bracket_root(const Polynomial& values, long double a, long double b, long double fa, RootWork& work) {
    auto lo = a, hi = b, flo = fa;
    for (int step = 0; step < 80 && !work.exhausted; ++step) {
        const auto mid = (lo + hi) * 0.5L;
        if (mid == lo || mid == hi) { break; }
        const auto fm = evaluate(values, mid, work);
        if (fm == 0.) { lo = hi = mid; break; }
        if ((flo < 0.) == (fm < 0.)) { lo = mid; flo = fm; } else { hi = mid; }
    }
    return (lo + hi) * 0.5L;
}
std::vector<long double> partition_roots(const Polynomial& values, const std::vector<long double>& critical, RootWork& work) {
    std::vector<long double> result;
    const auto bounds = std::minmax_element(values.begin(), values.end());
    if (*bounds.first > 0. || *bounds.second < 0.) { return result; }
    std::vector<long double> partition{0.};
    std::copy_if(critical.begin(), critical.end(), std::back_inserter(partition), [](long double t) { return t > 0. && t < 1.; });
    partition.push_back(1.);
    // Preserve nonzero signs: nearby crossings must not become a single touch.
    long double a = partition[0], fa = evaluate(values, a, work);
    if (fa == 0.) { append_root(result, a); }
    for (size_t i = 1; i < partition.size() && !work.exhausted; ++i) {
        const auto b = partition[i];
        const auto fb = evaluate(values, b, work);
        const auto roundoff = 32. * std::numeric_limits<long double>::epsilon() * values.size() * values.size();
        if (b != 1. && fb != 0. && std::abs(fb) <= roundoff) { work.uncertain = true; }
        if ((fa < 0. && fb > 0.) || (fa > 0. && fb < 0.)) {
            append_root(result, bracket_root(values, a, b, fa, work));
        }
        if (fb == 0.) { append_root(result, b); }
        a = b; fa = fb;
    }
    return result;
}
std::vector<long double> roots(Polynomial values, RootWork& work) {
    std::vector<Polynomial> derivatives;
    while (normalize(values)) {
        derivatives.push_back(values);
        if (values.size() == 1) { break; }
        Polynomial derivative;
        for (size_t i = 0; i + 1 < values.size(); ++i) {
            derivative.push_back(static_cast<long double>(values.size() - 1) * (values[i + 1] - values[i]));
        }
        values = std::move(derivative);
    }
    std::vector<long double> result;
    for (auto it = derivatives.rbegin(); it != derivatives.rend() && !work.exhausted; ++it) {
        result = partition_roots(*it, result, work);
    }
    return result;
}
occt_bridge_status_t collinear_span(occt_bridge_session_t* session, const Handle(Geom_BezierCurve)& arc,
    const gp_Pnt& origin, const gp_Vec& unit, double line_length) {
    double lo = std::numeric_limits<double>::infinity(), hi = -lo;
    for (int i = 1; i <= arc->NbPoles(); ++i) {
        const auto along = gp_Vec(origin, arc->Pole(i)).Dot(unit);
        lo = std::min(lo, along); hi = std::max(hi, along);
    }
    if (hi < 0. || lo > line_length) { return OCCT_BRIDGE_OK; }
    return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "line-spline intersection has collinear overlap or unresolved collinear contact");
}

occt_bridge_status_t append_contacts(occt_bridge_session_t* session,
    const Handle(Geom_BezierCurve)& arc, const gp_Pnt& origin, const gp_Vec& unit,
    double line_length, size_t maximum_points, RootWork& work,
    std::vector<occt_bridge_vec3_t>& points) {
    Polynomial coefficients;
    double maximum_weight = 0.;
    for (int i = 1; i <= arc->NbPoles(); ++i) { maximum_weight = std::max(maximum_weight, arc->Weight(i)); }
    long double maximum_distance = 0.;
    for (int i = 1; i <= arc->NbPoles(); ++i) {
        const auto p = arc->Pole(i);
        const long double dx = static_cast<long double>(p.X()) - origin.X();
        const long double dy = static_cast<long double>(p.Y()) - origin.Y();
        const auto distance = dx * unit.Y() - dy * unit.X();
        maximum_distance = std::max(maximum_distance, std::abs(distance));
        coefficients.push_back(distance * arc->Weight(i) / maximum_weight);
    }
    if (maximum_distance <= contact_tolerance || !normalize(coefficients)) {
        return collinear_span(session, arc, origin, unit, line_length);
    }
    const auto parameters = roots(std::move(coefficients), work);
    if (work.exhausted) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "line-spline intersection exceeds 1000000 root-work units");
    }
    if (work.uncertain) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "line-spline tangent roots cannot be resolved at coefficient precision");
    }
    long double previous = -1.;
    for (auto parameter : parameters) {
        if (previous >= 0. && parameter != previous && static_cast<double>(parameter) == static_cast<double>(previous)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "line-spline roots cannot be separated at native parameter precision");
        }
        previous = parameter;
        const auto p = arc->Value(static_cast<double>(parameter));
        const gp_Vec delta(origin, p);
        const double along = delta.Dot(unit);
        const double distance = std::abs(delta.X() * unit.Y() - delta.Y() * unit.X());
        if (distance > contact_tolerance) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "line-spline root cannot be resolved within contact precision");
        }
        if (along < 0. || along > line_length) { continue; }
        const occt_bridge_vec3_t point{p.X(), p.Y(), 0.};
        if (!finite(point)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "line-spline intersection produced nonfinite coordinates");
        }
        const bool duplicate = std::any_of(points.begin(), points.end(), [&](const auto& q) {
            return std::hypot(point.x - q.x, point.y - q.y) <= 1e-12;
        });
        if (!duplicate) { points.push_back(point); }
        if (points.size() > maximum_points) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "intersection exceeds point budget");
        }
    }
    return OCCT_BRIDGE_OK;
}
}

namespace occt_bridge_internal {
occt_bridge_status_t line_spline_intersections(occt_bridge_session_t* session,
    const TopoDS_Edge& first, const TopoDS_Edge& second, size_t maximum_points,
    std::vector<occt_bridge_vec3_t>& points, bool& handled) {
    const BRepAdaptor_Curve a(first), b(second);
    const auto spline = [](GeomAbs_CurveType type) { return type == GeomAbs_BSplineCurve || type == GeomAbs_BezierCurve; };
    handled = (a.GetType() == GeomAbs_Line && spline(b.GetType()))
        || (b.GetType() == GeomAbs_Line && spline(a.GetType()));
    if (!handled) { return OCCT_BRIDGE_OK; }
    const auto& line = a.GetType() == GeomAbs_Line ? a : b;
    const auto& curved = a.GetType() == GeomAbs_Line ? second : first;
    const auto origin = line.Value(line.FirstParameter());
    gp_Vec unit(origin, line.Value(line.LastParameter()));
    const double length = unit.Magnitude();
    if (!std::isfinite(length) || length <= contact_tolerance) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "line-spline intersection needs a nondegenerate finite line");
    }
    unit /= length;
    double first_parameter = 0., last_parameter = 0.;
    const auto curve = BRep_Tool::Curve(curved, first_parameter, last_parameter);
    const Handle(Geom_TrimmedCurve) trimmed = new Geom_TrimmedCurve(curve, first_parameter, last_parameter);
    const auto basis = GeomConvert::CurveToBSplineCurve(trimmed);
    GeomConvert_BSplineCurveToBezierCurve converter(basis);
    RootWork work;
    for (int i = 1; i <= converter.NbArcs(); ++i) {
        const auto status = append_contacts(session, converter.Arc(i), origin, unit, length, maximum_points, work, points);
        if (status != OCCT_BRIDGE_OK) { return status; }
    }
    return OCCT_BRIDGE_OK;
}
}
