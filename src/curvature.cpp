/*
 * Edge curvature: midpoint, sampled range, and exact or error-bounded
 * extrema.
 */

#include "bridge_internal.hpp"

#include <BRepAdaptor_Curve.hxx>
#include <BRepLProp_CLProps.hxx>
#include <GeomConvert_BSplineCurveToBezierCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <Precision.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TopoDS.hxx>

#include <algorithm>
#include <cmath>
#include <limits>
#include <queue>

using namespace occt_bridge_internal;

namespace {

struct CurvatureExtrema {
    double minimum = std::numeric_limits<double>::infinity();
    double minimum_lower = std::numeric_limits<double>::infinity();
    double maximum = 0.0;
    double maximum_upper = 0.0;
    bool exact = true;

    void attain(double curvature) {
        minimum = std::min(minimum, curvature);
        maximum = std::max(maximum, curvature);
        minimum_lower = std::min(minimum_lower, curvature);
        maximum_upper = std::max(maximum_upper, curvature);
    }
};

/* Scalar polynomial in Bernstein form on [0, 1]; degree is size() - 1. */
using Bernstein = std::vector<double>;

class BinomialTable {
public:
    double operator()(size_t n, size_t k) {
        while (rows_.size() <= n) {
            const size_t row = rows_.size();
            std::vector<double> values(row + 1, 1.0);
            for (size_t index = 1; index < row; ++index) {
                values[index] = rows_[row - 1][index - 1] + rows_[row - 1][index];
            }
            rows_.push_back(std::move(values));
        }
        return rows_[n][k];
    }

private:
    std::vector<std::vector<double>> rows_;
};

Bernstein bernstein_derivative(const Bernstein& value) {
    const size_t degree = value.size() - 1;
    if (degree == 0) {
        return Bernstein{0.0};
    }
    Bernstein result(degree);
    for (size_t index = 0; index < degree; ++index) {
        result[index] = static_cast<double>(degree) * (value[index + 1] - value[index]);
    }
    return result;
}

Bernstein bernstein_product(const Bernstein& left, const Bernstein& right, BinomialTable& binomial) {
    const size_t m = left.size() - 1;
    const size_t n = right.size() - 1;
    Bernstein result(m + n + 1, 0.0);
    for (size_t i = 0; i <= m; ++i) {
        for (size_t j = 0; j <= n; ++j) {
            result[i + j] += binomial(m, i) * binomial(n, j) * left[i] * right[j];
        }
    }
    for (size_t k = 0; k <= m + n; ++k) {
        result[k] /= binomial(m + n, k);
    }
    return result;
}

Bernstein bernstein_combine(const Bernstein& left, double scale, const Bernstein& right) {
    Bernstein result(left.size());
    for (size_t index = 0; index < left.size(); ++index) {
        result[index] = left[index] + scale * right[index];
    }
    return result;
}

/* Homogeneous Bezier piece: point numerator coordinates and weight. */
struct BezierPiece {
    Bernstein x, y, z, w;
    size_t depth = 0;
};

struct PieceBounds {
    double lower = 0.0;
    double upper = std::numeric_limits<double>::infinity();
    double start = 0.0;
    double end = 0.0;
    bool start_defined = false;
    bool end_defined = false;
};

/*
 * For C = P / w with D = P'w - Pw', curvature is |D x D'| w^2 / |D|^3.
 * Every factor is a Bernstein polynomial, whose coefficients bound it and
 * whose end coefficients equal its end values.
 */
PieceBounds bound_piece(const BezierPiece& piece, BinomialTable& binomial) {
    const Bernstein dw = bernstein_derivative(piece.w);
    const Bernstein* coordinates[] = {&piece.x, &piece.y, &piece.z};
    Bernstein d[3];
    Bernstein dd[3];
    for (int axis = 0; axis < 3; ++axis) {
        const Bernstein& p = *coordinates[axis];
        d[axis] = bernstein_combine(
            bernstein_product(bernstein_derivative(p), piece.w, binomial),
            -1.0,
            bernstein_product(p, dw, binomial));
        dd[axis] = bernstein_derivative(d[axis]);
    }
    Bernstein cross[3];
    for (int axis = 0; axis < 3; ++axis) {
        const int a = (axis + 1) % 3;
        const int b = (axis + 2) % 3;
        cross[axis] = bernstein_combine(
            bernstein_product(d[a], dd[b], binomial),
            -1.0,
            bernstein_product(d[b], dd[a], binomial));
    }
    Bernstein numerator = bernstein_product(cross[0], cross[0], binomial);
    Bernstein speed = bernstein_product(d[0], d[0], binomial);
    for (int axis = 1; axis < 3; ++axis) {
        numerator = bernstein_combine(numerator, 1.0, bernstein_product(cross[axis], cross[axis], binomial));
        speed = bernstein_combine(speed, 1.0, bernstein_product(d[axis], d[axis], binomial));
    }
    const Bernstein weight_squared = bernstein_product(piece.w, piece.w, binomial);
    /* Squared curvature is ratio / cube: both have degree 12n - 6. */
    const Bernstein ratio = bernstein_product(
        numerator,
        bernstein_product(weight_squared, weight_squared, binomial),
        binomial);
    const Bernstein cube = bernstein_product(bernstein_product(speed, speed, binomial), speed, binomial);

    PieceBounds bounds;
    const auto curvature_value = [&](size_t index) {
        return std::sqrt(std::max(ratio[index], 0.0) / cube[index]);
    };
    const double floor = std::numeric_limits<double>::min();
    const size_t last = cube.size() - 1;
    bounds.start_defined = cube.front() > floor;
    bounds.end_defined = cube.back() > floor;
    if (bounds.start_defined) {
        bounds.start = curvature_value(0);
    }
    if (bounds.end_defined) {
        bounds.end = curvature_value(last);
    }
    /*
     * With positive denominator coefficients, a Bernstein ratio is a convex
     * combination of its coefficient ratios, which converge quadratically.
     */
    if (ratio.size() != cube.size()
        || std::any_of(cube.begin(), cube.end(), [&](double value) { return !(value > floor); })) {
        return bounds;
    }
    bounds.lower = std::numeric_limits<double>::infinity();
    bounds.upper = 0.0;
    for (size_t index = 0; index <= last; ++index) {
        const double value = curvature_value(index);
        bounds.lower = std::min(bounds.lower, value);
        bounds.upper = std::max(bounds.upper, value);
    }
    return bounds;
}

void split_bernstein(const Bernstein& value, Bernstein& left, Bernstein& right) {
    Bernstein work = value;
    const size_t count = value.size();
    left.assign(count, 0.0);
    right.assign(count, 0.0);
    for (size_t level = 0; level < count; ++level) {
        left[level] = work[0];
        right[count - 1 - level] = work[count - 1 - level];
        for (size_t index = 0; index + 1 < count - level; ++index) {
            work[index] = 0.5 * (work[index] + work[index + 1]);
        }
    }
}

std::pair<BezierPiece, BezierPiece> split_piece(const BezierPiece& piece) {
    std::pair<BezierPiece, BezierPiece> halves;
    split_bernstein(piece.x, halves.first.x, halves.second.x);
    split_bernstein(piece.y, halves.first.y, halves.second.y);
    split_bernstein(piece.z, halves.first.z, halves.second.z);
    split_bernstein(piece.w, halves.first.w, halves.second.w);
    halves.first.depth = halves.second.depth = piece.depth + 1;
    return halves;
}

BezierPiece homogeneous_piece(const Handle(Geom_BezierCurve)& curve) {
    const int count = curve->NbPoles();
    TColgp_Array1OfPnt poles(1, count);
    curve->Poles(poles);
    BezierPiece piece;
    for (int index = 1; index <= count; ++index) {
        const double weight = curve->IsRational() ? curve->Weight(index) : 1.0;
        piece.x.push_back(poles(index).X() * weight);
        piece.y.push_back(poles(index).Y() * weight);
        piece.z.push_back(poles(index).Z() * weight);
        piece.w.push_back(weight);
    }
    return piece;
}

bool bound_polynomial_curvature(
    std::vector<BezierPiece> initial,
    double relative_tolerance,
    CurvatureExtrema& extrema,
    std::string& message) {
    constexpr size_t max_depth = 60;
    constexpr size_t max_splits = 200000;
    /* Radius 1e10 model units: below this, curvature is numerically straight. */
    constexpr double absolute_tolerance = 1e-10;
    BinomialTable binomial;
    struct Entry {
        double excess = 0.0;
        BezierPiece piece;
        PieceBounds bounds;
        bool operator<(const Entry& other) const { return excess < other.excess; }
    };
    extrema = CurvatureExtrema{};
    extrema.exact = false;
    std::vector<Entry> pending;
    const auto record = [&](const PieceBounds& bounds) -> bool {
        if (!bounds.start_defined || !bounds.end_defined) {
            return false;
        }
        extrema.attain(bounds.start);
        extrema.attain(bounds.end);
        return true;
    };
    for (BezierPiece& piece : initial) {
        const PieceBounds bounds = bound_piece(piece, binomial);
        if (!record(bounds)) {
            message = "edge curvature is undefined at a Bezier segment end";
            return false;
        }
        pending.push_back(Entry{0.0, std::move(piece), bounds});
    }
    const auto tolerance = [&] {
        return std::max(relative_tolerance * extrema.maximum, absolute_tolerance);
    };
    const auto excess = [&](const PieceBounds& bounds) {
        return std::max(bounds.upper - extrema.maximum, extrema.minimum - bounds.lower);
    };
    std::priority_queue<Entry> queue;
    for (Entry& entry : pending) {
        entry.excess = excess(entry.bounds);
        queue.push(std::move(entry));
    }
    size_t splits = 0;
    while (!queue.empty()) {
        Entry top = queue.top();
        const double current = excess(top.bounds);
        if (current < top.excess) {
            queue.pop();
            top.excess = current;
            queue.push(std::move(top));
            continue;
        }
        if (current <= tolerance()) {
            break;
        }
        queue.pop();
        if (top.piece.depth >= max_depth || ++splits > max_splits) {
            message = std::isfinite(top.bounds.upper)
                ? "edge curvature bounds did not converge"
                : "edge curvature is unbounded or undefined on the edge";
            return false;
        }
        auto halves = split_piece(top.piece);
        for (BezierPiece* half : {&halves.first, &halves.second}) {
            const PieceBounds bounds = bound_piece(*half, binomial);
            if (!record(bounds)) {
                message = "edge curvature is undefined at an interior point";
                return false;
            }
            queue.push(Entry{excess(bounds), std::move(*half), bounds});
        }
    }
    while (!queue.empty()) {
        const PieceBounds& bounds = queue.top().bounds;
        extrema.minimum_lower = std::min(extrema.minimum_lower, bounds.lower);
        extrema.maximum_upper = std::max(extrema.maximum_upper, bounds.upper);
        queue.pop();
    }
    return true;
}

/*
 * Returns the range ends plus up to three consecutive multiples of step inside it.
 * Critical values alternate between two kinds with period 2 * step, so two
 * consecutive multiples cover every extremum without iterating the range.
 */
std::vector<double> critical_parameters(double first, double last, double step) {
    std::vector<double> parameters{first, last};
    const double lowest = std::ceil(first / step);
    for (int offset = 0; offset < 3; ++offset) {
        const double parameter = (lowest + offset) * step;
        if (parameter > first && parameter < last) {
            parameters.push_back(parameter);
        }
    }
    return parameters;
}

/* Conic curvature from closed forms at the analytic critical parameters. */
bool analytic_curvature_extrema(
    const BRepAdaptor_Curve& curve,
    double first,
    double last,
    CurvatureExtrema& extrema) {
    const auto with_vertex = [&] {
        std::vector<double> parameters{first, last};
        if (first < 0.0 && last > 0.0) {
            parameters.push_back(0.0);
        }
        return parameters;
    };
    switch (curve.GetType()) {
    case GeomAbs_Line:
        extrema.attain(0.0);
        return true;
    case GeomAbs_Circle:
        extrema.attain(1.0 / curve.Circle().Radius());
        return true;
    case GeomAbs_Ellipse: {
        const double a = curve.Ellipse().MajorRadius();
        const double b = curve.Ellipse().MinorRadius();
        for (const double t : critical_parameters(first, last, M_PI_2)) {
            const double s = std::sin(t);
            const double c = std::cos(t);
            extrema.attain(a * b / std::pow(a * a * s * s + b * b * c * c, 1.5));
        }
        return true;
    }
    case GeomAbs_Parabola: {
        const double focal = curve.Parabola().Focal();
        for (const double u : with_vertex()) {
            extrema.attain((0.5 / focal) / std::pow(1.0 + u * u / (4.0 * focal * focal), 1.5));
        }
        return true;
    }
    case GeomAbs_Hyperbola: {
        const double a = curve.Hyperbola().MajorRadius();
        const double b = curve.Hyperbola().MinorRadius();
        for (const double u : with_vertex()) {
            const double sh = std::sinh(u);
            const double ch = std::cosh(u);
            extrema.attain(a * b / std::pow(a * a * sh * sh + b * b * ch * ch, 1.5));
        }
        return true;
    }
    default:
        return false;
    }
}

/* Trimmed Bezier or B-spline edge geometry as homogeneous Bezier pieces. */
std::vector<BezierPiece> bezier_pieces(const BRepAdaptor_Curve& curve, double first, double last) {
    std::vector<BezierPiece> pieces;
    if (curve.GetType() == GeomAbs_BezierCurve) {
        Handle(Geom_BezierCurve) bezier = Handle(Geom_BezierCurve)::DownCast(curve.Bezier()->Copy());
        bezier->Segment(first, last);
        pieces.push_back(homogeneous_piece(bezier));
        return pieces;
    }
    Handle(Geom_BSplineCurve) spline = Handle(Geom_BSplineCurve)::DownCast(curve.BSpline()->Copy());
    spline->Segment(first, last);
    GeomConvert_BSplineCurveToBezierCurve converter(spline);
    for (int index = 1; index <= converter.NbArcs(); ++index) {
        pieces.push_back(homogeneous_piece(converter.Arc(index)));
    }
    return pieces;
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_shape_edge_curvature(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    int* out_is_defined,
    double* out_curvature) {
    return guarded(session, [&] {
        if (out_is_defined == nullptr || out_curvature == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curvature query output is null");
        }
        *out_is_defined = 0;
        *out_curvature = 0.0;
        const TopoDS_Shape* value = find_shape(session, edge);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found");
        }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not an edge");
        }
        const BRepAdaptor_Curve curve(TopoDS::Edge(*value));
        const double first = curve.FirstParameter();
        const double last = curve.LastParameter();
        if (!std::isfinite(first) || !std::isfinite(last)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge has no finite parameter range");
        }
        BRepLProp_CLProps properties(
            curve,
            (first + last) * 0.5,
            2,
            Precision::Confusion());
        if (properties.IsTangentDefined()) {
            const double curvature = std::abs(properties.Curvature());
            if (!std::isfinite(curvature)) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge curvature is not finite");
            }
            *out_is_defined = 1;
            *out_curvature = curvature;
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_edge_curvature_range(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    size_t sample_count,
    double* out_minimum_curvature,
    double* out_maximum_curvature) {
    return guarded(session, [&] {
        if (out_minimum_curvature == nullptr || out_maximum_curvature == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curvature-range output is null");
        }
        *out_minimum_curvature = 0.0;
        *out_maximum_curvature = 0.0;
        if (sample_count < 2 || sample_count > 100000) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curvature sample count must be 2..100000");
        }
        const TopoDS_Shape* value = find_shape(session, edge);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found");
        }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not an edge");
        }
        const BRepAdaptor_Curve curve(TopoDS::Edge(*value));
        const double first = curve.FirstParameter();
        const double last = curve.LastParameter();
        if (!std::isfinite(first) || !std::isfinite(last)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge has no finite parameter range");
        }
        double minimum = std::numeric_limits<double>::infinity();
        double maximum = 0.0;
        for (size_t index = 0; index < sample_count; ++index) {
            const double fraction = static_cast<double>(index) / static_cast<double>(sample_count - 1);
            BRepLProp_CLProps properties(
                curve,
                first + (last - first) * fraction,
                2,
                Precision::Confusion());
            if (!properties.IsTangentDefined()) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge curvature is undefined at a sample");
            }
            const double curvature = std::abs(properties.Curvature());
            if (!std::isfinite(curvature)) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge curvature is not finite");
            }
            minimum = std::min(minimum, curvature);
            maximum = std::max(maximum, curvature);
        }
        *out_minimum_curvature = minimum;
        *out_maximum_curvature = maximum;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_edge_curvature_extrema(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    double relative_tolerance,
    double* out_minimum,
    double* out_minimum_lower_bound,
    double* out_maximum,
    double* out_maximum_upper_bound,
    int* out_is_exact) {
    return guarded(session, [&] {
        if (out_minimum == nullptr || out_minimum_lower_bound == nullptr || out_maximum == nullptr
            || out_maximum_upper_bound == nullptr || out_is_exact == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curvature-extrema output is null");
        }
        *out_minimum = *out_minimum_lower_bound = *out_maximum = *out_maximum_upper_bound = 0.0;
        *out_is_exact = 0;
        // NOLINTNEXTLINE(readability-simplify-boolean-expr): the negated form also rejects NaN.
        if (!(relative_tolerance > 0.0 && relative_tolerance <= 1.0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curvature relative tolerance must be in (0, 1]");
        }
        const TopoDS_Shape* value = find_shape(session, edge);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found");
        }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not an edge");
        }
        const BRepAdaptor_Curve curve(TopoDS::Edge(*value));
        const double first = curve.FirstParameter();
        const double last = curve.LastParameter();
        if (!std::isfinite(first) || !std::isfinite(last) || first >= last) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge has no finite parameter range");
        }

        CurvatureExtrema extrema;
        const GeomAbs_CurveType type = curve.GetType();
        if (type == GeomAbs_BezierCurve || type == GeomAbs_BSplineCurve) {
            std::string message;
            if (!bound_polynomial_curvature(bezier_pieces(curve, first, last), relative_tolerance, extrema, message)) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, std::move(message));
            }
        } else if (!analytic_curvature_extrema(curve, first, last, extrema)) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_GEOMETRY,
                "curvature extrema require a line, conic, Bezier, or B-spline edge");
        }
        if (!std::isfinite(extrema.minimum_lower) || !std::isfinite(extrema.maximum_upper)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge curvature is not finite");
        }
        *out_minimum = extrema.minimum;
        *out_minimum_lower_bound = extrema.minimum_lower;
        *out_maximum = extrema.maximum;
        *out_maximum_upper_bound = extrema.maximum_upper;
        *out_is_exact = extrema.exact ? 1 : 0;
        return succeed(session);
    });
}

}  // extern "C"
