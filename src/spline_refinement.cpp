#include "bridge_internal.hpp"
#include <Geom_BSplineCurve.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <algorithm>
#include <cmath>

using namespace occt_bridge_internal;

occt_bridge_status_t occt_bridge_bspline_refine(
    occt_bridge_session_t* session, const occt_bridge_bspline_basis_t* basis,
    int32_t action, double knot, int32_t target_degree,
    occt_bridge_bspline_refinement_t* out) {
    if (out != nullptr) { out->degree = 0; out->pole_count = out->knot_count = 0; }
    return guarded(session, [&] {
        if (basis == nullptr || out == nullptr || basis->degree < 1 || basis->degree > 25
            || (basis->periodic != 0 && basis->periodic != 1)
            || basis->pole_count < static_cast<size_t>(basis->degree + 1) || basis->pole_count > 256
            || basis->knot_count < 2 || basis->knot_count > 514
            || basis->knots == nullptr || basis->multiplicities == nullptr
            || out->knots == nullptr || out->multiplicities == nullptr || out->coefficients == nullptr
            || out->knot_capacity < 514 || out->coefficient_capacity < 256 * basis->pole_count
            || (action != 0 && action != 1)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Refinement requires a bounded basis and complete output buffers");
        }
        if (action == 0 && !std::isfinite(knot)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Inserted knot must be finite");
        }
        if (action == 1 && (target_degree <= basis->degree || target_degree > 25)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Elevated degree must increase and be at most 25");
        }
        TColStd_Array1OfReal knots(1, static_cast<int>(basis->knot_count));
        TColStd_Array1OfInteger mults(1, static_cast<int>(basis->knot_count));
        size_t sum = 0;
        for (size_t i = 0; i < basis->knot_count; ++i) {
            const auto value = basis->knots[i];
            const int m = basis->multiplicities[i];
            const bool boundary = i == 0 || i + 1 == basis->knot_count;
            if (!std::isfinite(value) || (i > 0 && value <= basis->knots[i - 1])
                || m < 1 || m > basis->degree + (boundary && !basis->periodic ? 1 : 0)) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Invalid refinement knots or multiplicities");
            }
            knots.SetValue(static_cast<int>(i + 1), value); mults.SetValue(static_cast<int>(i + 1), m);
            sum += static_cast<size_t>(m);
        }
        if (sum != basis->pole_count + (basis->periodic ? static_cast<size_t>(basis->multiplicities[basis->knot_count - 1]) : static_cast<size_t>(basis->degree + 1))
            || (basis->periodic && basis->multiplicities[0] != basis->multiplicities[basis->knot_count - 1])) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Refinement basis cardinality is inconsistent");
        }
        std::vector<double> coefficients;
        Handle(Geom_BSplineCurve) refined, reference;
        for (size_t column = 0; column < basis->pole_count; column += 3) {
            TColgp_Array1OfPnt poles(1, static_cast<int>(basis->pole_count));
            for (size_t i = 0; i < basis->pole_count; ++i) {
                poles.SetValue(static_cast<int>(i + 1), gp_Pnt(i == column ? 1. : 0., i == column + 1 ? 1. : 0., i == column + 2 ? 1. : 0.));
            }
            refined = new Geom_BSplineCurve(poles, knots, mults, basis->degree, basis->periodic != 0);
            if (action == 0) {
                if (knot <= refined->FirstParameter() || knot >= refined->LastParameter()) {
                    return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Insert a knot strictly inside the active curve domain");
                }
                refined->InsertKnot(knot, 1, 0., true);
                if (refined->NbPoles() != static_cast<int>(basis->pole_count + 1)) {
                    return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Knot insertion must add one pole; the knot may already have maximum multiplicity");
                }
            } else {
                // IncreaseDegree may retain inactive outer knots that do not
                // satisfy constructor cardinality. Segment the active interval
                // first, preserving its parameter values and exact geometry.
                if (!basis->periodic && (basis->multiplicities[0] != basis->degree + 1
                    || basis->multiplicities[basis->knot_count - 1] != basis->degree + 1)) {
                    const double first = refined->FirstParameter(), last = refined->LastParameter();
                    refined->Segment(first, last, 0.);
                }
                refined->IncreaseDegree(target_degree);
            }
            if (refined->NbPoles() > 256 || refined->NbKnots() > 514) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "Refinement exceeds the 256-pole limit");
            }
            if (column == 0) {
                reference = refined;
                coefficients.resize(static_cast<size_t>(refined->NbPoles()) * basis->pole_count);
            } else {
                if (refined->NbPoles() != reference->NbPoles() || refined->NbKnots() != reference->NbKnots()
                    || refined->Degree() != reference->Degree()) {
                    return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "Refinement basis must be independent of polynomial coordinates");
                }
                for (int i = 1; i <= refined->NbKnots(); ++i) {
                    if (refined->Knot(i) != reference->Knot(i) || refined->Multiplicity(i) != reference->Multiplicity(i)) {
                        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "Refinement knot data must be independent of polynomial coordinates");
                    }
                }
            }
            for (int row = 1; row <= refined->NbPoles(); ++row) {
                const auto pole = refined->Pole(row);
                for (size_t axis = 0; axis < 3 && column + axis < basis->pole_count; ++axis) {
                    const double value = pole.Coord(static_cast<int>(axis + 1));
                    if (!std::isfinite(value) || value < -1e-12) {
                        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "Refinement needs finite nonnegative homogeneous coefficients");
                    }
                    coefficients[static_cast<size_t>(row - 1) * basis->pole_count + column + axis] = std::max(0., value);
                }
            }
        }
        for (int row = 0; row < refined->NbPoles(); ++row) {
            const auto begin = coefficients.begin() + static_cast<size_t>(row) * basis->pole_count;
            double total = 0.; for (size_t i = 0; i < basis->pole_count; ++i) { total += begin[i]; }
            if (!std::isfinite(total) || std::abs(total - 1.) > 1e-10) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "Refinement coefficients must form an affine combination");
            }
            for (size_t i = 0; i < basis->pole_count; ++i) { begin[i] /= total; }
        }
        for (int i = 1; i <= refined->NbKnots(); ++i) {
            out->knots[i - 1] = refined->Knot(i); out->multiplicities[i - 1] = refined->Multiplicity(i);
        }
        std::copy(coefficients.begin(), coefficients.end(), out->coefficients);
        out->degree = refined->Degree(); out->pole_count = static_cast<size_t>(refined->NbPoles()); out->knot_count = static_cast<size_t>(refined->NbKnots());
        return succeed(session);
    });
}
