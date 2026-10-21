#include "bridge_internal.hpp"
#include <Geom_BSplineCurve.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <algorithm>
#include <array>
#include <cmath>
#include <numeric>
#include <optional>

using namespace occt_bridge_internal;

namespace {

bool bounded_basis(const occt_bridge_bspline_basis_t& basis) {
    return basis.degree >= 1 && basis.degree <= 25
        && (basis.periodic == 0 || basis.periodic == 1)
        && basis.pole_count >= static_cast<size_t>(basis.degree) + 1 && basis.pole_count <= 256
        && basis.knot_count >= 2 && basis.knot_count <= 514
        && basis.knots != nullptr && basis.multiplicities != nullptr;
}

bool complete_output(const occt_bridge_bspline_refinement_t& output, size_t pole_count) {
    return output.knots != nullptr && output.multiplicities != nullptr && output.coefficients != nullptr
        && output.knot_capacity >= 514 && output.coefficient_capacity >= 256 * pole_count;
}

const char* request_error(const occt_bridge_bspline_basis_t* basis,
                          const occt_bridge_bspline_refinement_t* output,
                          int32_t action, double knot, int32_t target_degree) {
    if (basis == nullptr || output == nullptr || !bounded_basis(*basis)
        || !complete_output(*output, basis->pole_count) || (action != 0 && action != 1)) {
        return "Refinement requires a bounded basis and complete output buffers";
    }
    if (action == 0 && !std::isfinite(knot)) {
        return "Inserted knot must be finite";
    }
    if (action == 1 && (target_degree <= basis->degree || target_degree > 25)) {
        return "Elevated degree must increase and be at most 25";
    }
    return nullptr;
}

bool consistent_cardinality(const occt_bridge_bspline_basis_t& basis, size_t sum) {
    const size_t last = basis.knot_count - 1;
    const size_t extra = basis.periodic ? static_cast<size_t>(basis.multiplicities[last])
                                      : static_cast<size_t>(basis.degree) + 1;
    return sum == basis.pole_count + extra
        && (!basis.periodic || basis.multiplicities[0] == basis.multiplicities[last]);
}

const char* read_knots(const occt_bridge_bspline_basis_t& basis,
                       TColStd_Array1OfReal& knots, TColStd_Array1OfInteger& mults) {
    size_t sum = 0;
    for (size_t i = 0; i < basis.knot_count; ++i) {
        const double value = basis.knots[i];
        const int m = basis.multiplicities[i];
        const bool boundary = i == 0 || i + 1 == basis.knot_count;
        if (!std::isfinite(value) || (i > 0 && value <= basis.knots[i - 1])
            || m < 1 || m > basis.degree + (boundary && !basis.periodic ? 1 : 0)) {
            return "Invalid refinement knots or multiplicities";
        }
        knots.SetValue(static_cast<int>(i + 1), value);
        mults.SetValue(static_cast<int>(i + 1), m);
        sum += static_cast<size_t>(m);
    }
    return consistent_cardinality(basis, sum) ? nullptr : "Refinement basis cardinality is inconsistent";
}

// Pack three columns of the identity matrix into polynomial XYZ poles.
// Refining these curves yields coefficients for arbitrary homogeneous poles.
Handle(Geom_BSplineCurve) identity_curve(const occt_bridge_bspline_basis_t& basis,
                                        const TColStd_Array1OfReal& knots,
                                        const TColStd_Array1OfInteger& mults, size_t column) {
    TColgp_Array1OfPnt poles(1, static_cast<int>(basis.pole_count));
    for (size_t i = 0; i < basis.pole_count; ++i) {
        poles.SetValue(static_cast<int>(i + 1),
            gp_Pnt(i == column ? 1. : 0., i == column + 1 ? 1. : 0., i == column + 2 ? 1. : 0.));
    }
    return new Geom_BSplineCurve(poles, knots, mults, basis.degree, basis.periodic != 0);
}

const char* refine_curve(const Handle(Geom_BSplineCurve)& curve, const occt_bridge_bspline_basis_t& basis,
                         int32_t action, double knot, int32_t target_degree) {
    if (action == 0) {
        if (knot <= curve->FirstParameter() || knot >= curve->LastParameter()) {
            return "Insert a knot strictly inside the active curve domain";
        }
        curve->InsertKnot(knot, 1, 0., true);
        if (curve->NbPoles() != static_cast<int>(basis.pole_count + 1)) {
            return "Knot insertion must add one pole; the knot may already have maximum multiplicity";
        }
    } else {
        // IncreaseDegree may retain inactive outer knots that do not satisfy
        // constructor cardinality. Clamp the active interval without changing
        // its parameter values or exact geometry before degree elevation.
        if (!basis.periodic && (basis.multiplicities[0] != basis.degree + 1
            || basis.multiplicities[basis.knot_count - 1] != basis.degree + 1)) {
            const double first = curve->FirstParameter(), last = curve->LastParameter();
            curve->Segment(first, last, 0.);
        }
        curve->IncreaseDegree(target_degree);
    }
    return curve->NbPoles() > 256 || curve->NbKnots() > 514 ? "Refinement exceeds the 256-pole limit" : nullptr;
}

const char* matching_basis_error(const Handle(Geom_BSplineCurve)& curve,
                                 const Handle(Geom_BSplineCurve)& reference) {
    if (curve->NbPoles() != reference->NbPoles() || curve->NbKnots() != reference->NbKnots()
        || curve->Degree() != reference->Degree()) {
        return "Refinement basis must be independent of polynomial coordinates";
    }
    for (int i = 1; i <= curve->NbKnots(); ++i) {
        if (curve->Knot(i) != reference->Knot(i) || curve->Multiplicity(i) != reference->Multiplicity(i)) {
            return "Refinement knot data must be independent of polynomial coordinates";
        }
    }
    return nullptr;
}

const char* read_coefficients(const Handle(Geom_BSplineCurve)& curve, size_t column,
                              size_t pole_count, std::vector<double>& coefficients) {
    for (int row = 1; row <= curve->NbPoles(); ++row) {
        const auto pole = curve->Pole(row);
        const std::array<double, 3> values{pole.X(), pole.Y(), pole.Z()};
        for (size_t axis = 0; axis < 3 && column + axis < pole_count; ++axis) {
            const double value = values[axis];
            if (!std::isfinite(value) || value < -1e-12) {
                return "Refinement needs finite nonnegative homogeneous coefficients";
            }
            coefficients[static_cast<size_t>(row - 1) * pole_count + column + axis] = std::max(0., value);
        }
    }
    return nullptr;
}

const char* normalize_coefficients(size_t rows, size_t columns, std::vector<double>& coefficients) {
    for (size_t row = 0; row < rows; ++row) {
        auto* begin = coefficients.data() + row * columns;
        const double total = std::accumulate(begin, begin + columns, 0.);
        if (!std::isfinite(total) || std::abs(total - 1.) > 1e-10) {
            return "Refinement coefficients must form an affine combination";
        }
        for (size_t i = 0; i < columns; ++i) {
            begin[i] /= total;
        }
    }
    return nullptr;
}

struct Refinement {
    Handle(Geom_BSplineCurve) curve;
    std::vector<double> coefficients;
};

struct RefinementError {
    occt_bridge_status_t status;
    const char* message;
};

std::optional<RefinementError> refine_basis(const occt_bridge_bspline_basis_t& basis,
    const TColStd_Array1OfReal& knots, const TColStd_Array1OfInteger& mults,
    int32_t action, double knot, int32_t target_degree, Refinement& result) {
    Handle(Geom_BSplineCurve) reference;
    for (size_t column = 0; column < basis.pole_count; column += 3) {
        result.curve = identity_curve(basis, knots, mults, column);
        if (const char* error = refine_curve(result.curve, basis, action, knot, target_degree)) {
            return RefinementError{OCCT_BRIDGE_INVALID_ARGUMENT, error};
        }
        if (column == 0) {
            reference = result.curve;
            result.coefficients.resize(static_cast<size_t>(result.curve->NbPoles()) * basis.pole_count);
        } else if (const char* error = matching_basis_error(result.curve, reference)) {
            return RefinementError{OCCT_BRIDGE_INVALID_GEOMETRY, error};
        }
        if (const char* error = read_coefficients(result.curve, column, basis.pole_count, result.coefficients)) {
            return RefinementError{OCCT_BRIDGE_INVALID_GEOMETRY, error};
        }
    }
    if (const char* error = normalize_coefficients(static_cast<size_t>(result.curve->NbPoles()),
                                                  basis.pole_count, result.coefficients)) {
        return RefinementError{OCCT_BRIDGE_INVALID_GEOMETRY, error};
    }
    return std::nullopt;
}

void publish_refinement(const Refinement& result, occt_bridge_bspline_refinement_t& output) {
    for (int i = 1; i <= result.curve->NbKnots(); ++i) {
        output.knots[i - 1] = result.curve->Knot(i);
        output.multiplicities[i - 1] = result.curve->Multiplicity(i);
    }
    std::copy(result.coefficients.begin(), result.coefficients.end(), output.coefficients);
    output.degree = result.curve->Degree();
    output.pole_count = static_cast<size_t>(result.curve->NbPoles());
    output.knot_count = static_cast<size_t>(result.curve->NbKnots());
}

}  // namespace

occt_bridge_status_t occt_bridge_bspline_refine(
    occt_bridge_session_t* session, const occt_bridge_bspline_basis_t* basis,
    int32_t action, double knot, int32_t target_degree,
    occt_bridge_bspline_refinement_t* out_result) {
    if (out_result != nullptr) {
        out_result->degree = 0;
        out_result->pole_count = out_result->knot_count = 0;
    }
    return guarded(session, [&] {
        if (const char* error = request_error(basis, out_result, action, knot, target_degree)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
        }
        TColStd_Array1OfReal knots(1, static_cast<int>(basis->knot_count));
        TColStd_Array1OfInteger mults(1, static_cast<int>(basis->knot_count));
        if (const char* error = read_knots(*basis, knots, mults)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
        }
        Refinement result;
        if (const auto error = refine_basis(*basis, knots, mults, action, knot, target_degree, result)) {
            return fail(session, error->status, error->message);
        }
        publish_refinement(result, *out_result);
        return succeed(session);
    });
}
