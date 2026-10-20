/*
 * Per-face range of the outward normal's component along a pull direction,
 * for draft checks. Exact on planes, cylinders, cones, spheres, and tori
 * where the extremes lie on the face; other faces are refined numerically
 * from a grid and their boundary curves (ABI 85).
 */

#include "bridge_internal.hpp"

#include <BRepAdaptor_Surface.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Curve.hxx>
#include <TopExp_Explorer.hxx>
#include <Precision.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include <algorithm>
#include <array>
#include <cmath>
#include <limits>
#include <vector>

using namespace occt_bridge_internal;

namespace {

// Largest disagreement between the fitted form and an evaluated normal
// before a face is reported inexact.
constexpr double kFitTolerance = 1e-9;

struct Extreme {
    double value;
    double u;
    double v;
};

struct Range {
    Extreme low;
    Extreme high;
};

// Extremes of a cos t + b sin t + c over [first, last]: the ends and the
// stationary points atan2(b, a) + k pi inside.
Range trig_extremes(double a, double b, double c, double first, double last) {
    const auto value = [&](double t) { return a * std::cos(t) + b * std::sin(t) + c; };
    Range range{{value(first), first, 0.0}, {value(first), first, 0.0}};
    const auto consider = [&](double t) {
        const double candidate = value(t);
        if (candidate < range.low.value) {
            range.low = {candidate, t, 0.0};
        }
        if (candidate > range.high.value) {
            range.high = {candidate, t, 0.0};
        }
    };
    consider(last);
    const double phase = std::atan2(b, a);
    const auto turn = static_cast<long long>(std::ceil((first - phase) / M_PI));
    for (long long k = turn; phase + static_cast<double>(k) * M_PI <= last; ++k) {
        consider(phase + static_cast<double>(k) * M_PI);
    }
    return range;
}

class PullRange {
public:
    PullRange(const TopoDS_Face& face, const BRepAdaptor_Surface& surface, const gp_Vec& pull)
        : face_(face),
          surface_(surface),
          pull_(pull),
          // A reversed face's outward normal opposes the surface normal.
          outward_(face.Orientation() == TopAbs_REVERSED ? -1.0 : 1.0) {
        BRepTools::UVBounds(face, u_min_, u_max_, v_min_, v_max_);
    }

    void plane() {
        const double v = 0.5 * (v_min_ + v_max_);
        const double u = inside_u(v);
        const double value = projection(u, v);
        if (std::isfinite(value)) {
            finish({{value, u, v}, {value, u, v}}, true);
        }
    }

    // Cylinders and cones: the normal turns with u alone, and a connected
    // face covers its whole u range, so the range over u is the face's.
    void turning_with_u() {
        for (const double v : {0.5 * (v_min_ + v_max_), v_min_, v_max_}) {
            double a = 0.0;
            double b = 0.0;
            double c = 0.0;
            if (fit(v, a, b, c) && independent_of_v(a, b, c)) {
                Range range = trig_extremes(a, b, c, u_min_, u_max_);
                range.low.v = inside_v(range.low.u);
                range.high.v = inside_v(range.high.u);
                finish(range, true);
                return;
            }
        }
    }

    // Spheres and tori: the normal is cos v (radial in u) + sin v (axis), so
    // projection = cos v (a cos u + b sin u) + c sin v. Exact when both
    // extremes over the UV rectangle lie on the face.
    void sphere_like() {
        double a = 0.0;
        double b = 0.0;
        double offset = 0.0;
        if (!fit(0.0, a, b, offset) || std::abs(offset) > kFitTolerance) {
            return;
        }
        const double c = std::sqrt(2.0) * projection(0.0, M_PI / 4.0) - a;
        const auto model = [&](double u, double v) {
            return std::cos(v) * (a * std::cos(u) + b * std::sin(u)) + c * std::sin(v);
        };
        if (!std::isfinite(c) || std::abs(model(1.0, 0.3) - projection(1.0, 0.3)) > kFitTolerance) {
            return;
        }
        const Range around = trig_extremes(a, b, 0.0, u_min_, u_max_);
        constexpr double kInfinity = std::numeric_limits<double>::infinity();
        Range range{{kInfinity, 0.0, 0.0}, {-kInfinity, 0.0, 0.0}};
        // Between multiples of pi/2 + k pi, cos v keeps its sign, so the
        // u extreme that minimizes (or maximizes) is fixed on each piece.
        std::vector<double> cuts{v_min_};
        const auto first = static_cast<long long>(std::ceil((v_min_ - M_PI_2) / M_PI));
        for (long long k = first; M_PI_2 + static_cast<double>(k) * M_PI < v_max_; ++k) {
            cuts.push_back(M_PI_2 + static_cast<double>(k) * M_PI);
        }
        cuts.push_back(v_max_);
        for (size_t piece = 0; piece + 1 < cuts.size(); ++piece) {
            const double from = cuts[piece];
            const double to = cuts[piece + 1];
            const bool positive = std::cos(0.5 * (from + to)) >= 0.0;
            const Extreme& low_u = positive ? around.low : around.high;
            const Extreme& high_u = positive ? around.high : around.low;
            const Range low = trig_extremes(low_u.value, c, 0.0, from, to);
            const Range high = trig_extremes(high_u.value, c, 0.0, from, to);
            if (low.low.value < range.low.value) {
                range.low = {low.low.value, low_u.u, low.low.u};
            }
            if (high.high.value > range.high.value) {
                range.high = {high.high.value, high_u.u, high.high.u};
            }
        }
        // Pulled along the axis, the projection ignores u: any u on the face
        // at the extreme v attains it.
        if (std::hypot(a, b) <= kFitTolerance) {
            for (Extreme* end : {&range.low, &range.high}) {
                if (!on_face(end->u, end->v)) {
                    end->u = inside_u(end->v);
                }
            }
        }
        finish(range, on_face(range.low.u, range.low.v) && on_face(range.high.u, range.high.v));
    }

    // Freeform faces: every on-face point of a kGrid x kGrid lattice over
    // the UV bounds and kEdgeSamples points on each boundary pcurve, then
    // each extreme refined by a compass search that stays on the face,
    // halving its step down to 1e-12 of the UV span. Finds the extremes of
    // smooth faces to rounding, but a lattice can miss a narrow extreme, so
    // the result is marked refined (2), not exact.
    void freeform() {
        constexpr int kGrid = 32;
        constexpr int kEdgeSamples = 64;
        constexpr double kInfinity = std::numeric_limits<double>::infinity();
        Range range{{kInfinity, 0.0, 0.0}, {-kInfinity, 0.0, 0.0}};
        const auto consider = [&](double u, double v) {
            const double value = projection(u, v);
            if (!std::isfinite(value)) {
                return;
            }
            if (value < range.low.value) {
                range.low = {value, u, v};
            }
            if (value > range.high.value) {
                range.high = {value, u, v};
            }
        };
        for (int i = 0; i <= kGrid; ++i) {
            for (int j = 0; j <= kGrid; ++j) {
                const double u = u_min_ + (u_max_ - u_min_) * i / kGrid;
                const double v = v_min_ + (v_max_ - v_min_) * j / kGrid;
                if (on_face(u, v)) {
                    consider(u, v);
                }
            }
        }
        for (TopExp_Explorer edges(face_, TopAbs_EDGE); edges.More(); edges.Next()) {
            double first = 0.0;
            double last = 0.0;
            const Handle(Geom2d_Curve) curve =
                BRep_Tool::CurveOnSurface(TopoDS::Edge(edges.Current()), face_, first, last);
            if (curve.IsNull() || !std::isfinite(first) || !std::isfinite(last)) {
                continue;
            }
            for (int k = 0; k <= kEdgeSamples; ++k) {
                const gp_Pnt2d uv = curve->Value(first + (last - first) * k / kEdgeSamples);
                consider(uv.X(), uv.Y());
            }
        }
        if (!std::isfinite(range.low.value) || !std::isfinite(range.high.value)) {
            return;
        }
        refine(range.low, -1.0);
        refine(range.high, 1.0);
        finish(range, true);
        result_.exact = 2;
    }

    const occt_bridge_face_pull_range_t& result() const { return result_; }

private:
    double projection(double u, double v) const {
        BRepLProp_SLProps properties(surface_, u, v, 1, Precision::Confusion());
        if (!properties.IsNormalDefined()) {
            return NAN;
        }
        return outward_ * gp_Vec(properties.Normal()).Dot(pull_);
    }

    static double evaluate(double a, double b, double c, double u, double /*v*/) {
        return a * std::cos(u) + b * std::sin(u) + c;
    }

    // Fits projection(u, v) = a cos u + b sin u + c at fixed v.
    bool fit(double v, double& a, double& b, double& c) const {
        const double start = projection(0.0, v);
        const double quarter = projection(M_PI_2, v);
        const double half = projection(M_PI, v);
        if (!std::isfinite(start) || !std::isfinite(quarter) || !std::isfinite(half)) {
            return false;
        }
        c = 0.5 * (start + half);
        a = 0.5 * (start - half);
        b = quarter - c;
        return std::abs(evaluate(a, b, c, 1.0, v) - projection(1.0, v)) <= kFitTolerance;
    }

    // The fitted form also holds at the face's v ends where the normal is
    // defined (a cone apex has none).
    bool independent_of_v(double a, double b, double c) const {
        const std::array<double, 2> ends{v_min_, v_max_};
        return std::none_of(ends.begin(), ends.end(), [&](double v) {
            const double value = projection(1.0, v);
            return std::isfinite(value) && std::abs(evaluate(a, b, c, 1.0, v) - value) > kFitTolerance;
        });
    }

    // Compass search: move to the best on-face neighbor at the current step
    // while it improves sign * projection, else halve the step.
    void refine(Extreme& extreme, double sign) const {
        double step_u = (u_max_ - u_min_) / 32.0;
        double step_v = (v_max_ - v_min_) / 32.0;
        const double floor_u = 1e-12 * std::max(1.0, u_max_ - u_min_);
        const double floor_v = 1e-12 * std::max(1.0, v_max_ - v_min_);
        for (int iteration = 0; iteration < 400 && (step_u > floor_u || step_v > floor_v); ++iteration) {
            Extreme best = extreme;
            const std::array<std::array<double, 2>, 4> moves{{{step_u, 0.0}, {-step_u, 0.0}, {0.0, step_v}, {0.0, -step_v}}};
            for (const auto& move : moves) {
                const double u = std::clamp(extreme.u + move[0], u_min_, u_max_);
                const double v = std::clamp(extreme.v + move[1], v_min_, v_max_);
                const double value = projection(u, v);
                if (std::isfinite(value) && sign * value > sign * best.value && on_face(u, v)) {
                    best = {value, u, v};
                }
            }
            if (best.u == extreme.u && best.v == extreme.v) {
                step_u *= 0.5;
                step_v *= 0.5;
            } else {
                extreme = best;
            }
        }
    }

    bool on_face(double u, double v) const {
        BRepClass_FaceClassifier classifier;
        classifier.Perform(face_, gp_Pnt2d(u, v), Precision::PConfusion());
        return classifier.State() == TopAbs_IN || classifier.State() == TopAbs_ON;
    }

    // A parameter on the face along a constant-v or constant-u line, so
    // witnesses lie on the face; the middle when no probe lands.
    double inside_u(double v) const { return probe(u_min_, u_max_, [&](double u) { return on_face(u, v); }); }
    double inside_v(double u) const { return probe(v_min_, v_max_, [&](double v) { return on_face(u, v); }); }

    template <typename Inside>
    static double probe(double first, double last, Inside inside) {
        constexpr int kProbes = 33;
        for (int index = 0; index <= kProbes; ++index) {
            const double t = first + (last - first) * (index + 0.5) / (kProbes + 1);
            if (inside(t)) {
                return t;
            }
        }
        return 0.5 * (first + last);
    }

    void finish(const Range& range, bool exact) {
        result_.minimum = range.low.value;
        result_.maximum = range.high.value;
        const gp_Pnt low = surface_.Value(range.low.u, range.low.v);
        const gp_Pnt high = surface_.Value(range.high.u, range.high.v);
        result_.minimum_point = {low.X(), low.Y(), low.Z()};
        result_.maximum_point = {high.X(), high.Y(), high.Z()};
        result_.exact = exact ? 1 : 0;
    }

    const TopoDS_Face& face_;
    const BRepAdaptor_Surface& surface_;
    gp_Vec pull_;
    double outward_;
    double u_min_ = 0.0;
    double u_max_ = 0.0;
    double v_min_ = 0.0;
    double v_max_ = 0.0;
    occt_bridge_face_pull_range_t result_{};
};

occt_bridge_face_pull_range_t face_pull_range(const TopoDS_Face& face, const gp_Vec& pull) {
    const BRepAdaptor_Surface surface(face);
    PullRange range(face, surface, pull);
    switch (surface.GetType()) {
        case GeomAbs_Plane: range.plane(); break;
        case GeomAbs_Cylinder:
        case GeomAbs_Cone: range.turning_with_u(); break;
        case GeomAbs_Sphere:
        case GeomAbs_Torus: range.sphere_like(); break;
        default: break;
    }
    if (range.result().exact == 0) {
        range.freeform();
    }
    return range.result();
}

}  // namespace

// O(faces), each a fixed number of normal evaluations and classifications.
// No handles or geometry copies are created.
occt_bridge_status_t occt_bridge_shape_face_pull_ranges(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t pull_direction,
    occt_bridge_face_pull_range_t* out_ranges,
    size_t capacity,
    size_t* out_count) {
    if (out_count != nullptr) { *out_count = 0; }
    return guarded(session, [&] {
        if (out_count == nullptr || (out_ranges == nullptr && capacity != 0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "output buffer is invalid");
        }
        const gp_Vec pull(pull_direction.x, pull_direction.y, pull_direction.z);
        const double magnitude = pull.Magnitude();
        if (!std::isfinite(magnitude) || magnitude <= Precision::Confusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "pull direction must be finite and nonzero");
        }
        const auto* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        const auto faces = ordered(*value, TopAbs_FACE);
        const auto count = static_cast<size_t>(faces.Extent());
        bool query_only = false;
        if (const auto status = check_buffer(session, out_ranges, capacity, count, out_count, query_only);
            status != OCCT_BRIDGE_OK || query_only) {
            return status;
        }
        std::vector<occt_bridge_face_pull_range_t> results;
        results.reserve(count);
        for (int index = 1; index <= faces.Extent(); ++index) {
            results.push_back(face_pull_range(TopoDS::Face(faces(index)), pull / magnitude));
        }
        std::copy(results.begin(), results.end(), out_ranges);
        *out_count = count;
        return succeed(session);
    });
}
