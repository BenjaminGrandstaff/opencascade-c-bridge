/*
 * Signed face radius bounds and edge concavity, for minimum-radius checks.
 * Exact on planes, cylinders, cones, spheres, and tori; sampled elsewhere.
 */

#include "bridge_internal.hpp"

#include <BRepAdaptor_Surface.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepOffset_Analyse.hxx>
#include <BRepOffset_Interval.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <gp_Cone.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Torus.hxx>

#include <algorithm>
#include <cmath>
#include <limits>

using namespace occt_bridge_internal;

namespace {

constexpr double kInfinity = std::numeric_limits<double>::infinity();
constexpr uint32_t kMaximumSamplesPerDirection = 1024;

occt_bridge_vec3_t vec3(const gp_Pnt& point) {
    return occt_bridge_vec3_t{point.X(), point.Y(), point.Z()};
}

class RadiusBounds {
public:
    RadiusBounds(const TopoDS_Face& face, const BRepAdaptor_Surface& surface)
        : face_(face),
          surface_(surface),
          // A reversed face's outward normal opposes the surface normal.
          outward_(face.Orientation() == TopAbs_REVERSED ? -1.0 : 1.0) {
        BRepTools::UVBounds(face, u_min_, u_max_, v_min_, v_max_);
        result_.convex_radius = kInfinity;
        result_.concave_radius = kInfinity;
        result_.exact = 1;
    }

    // Both principal curvatures at (u, v). Curvature toward the outward normal
    // is concave (material surrounds the center); away from it is convex.
    void consider(double u, double v) {
        BRepLProp_SLProps properties(surface_, u, v, 2, Precision::Confusion());
        if (!properties.IsCurvatureDefined()) {
            return;
        }
        const gp_Pnt point = properties.Value();
        for (const double curvature : {properties.MinCurvature(), properties.MaxCurvature()}) {
            const double signed_curvature = outward_ * curvature;
            if (!std::isfinite(signed_curvature) || std::abs(signed_curvature) <= 1e-300) {
                continue;
            }
            const double radius = 1.0 / std::abs(signed_curvature);
            if (signed_curvature > 0.0) {
                attain(result_.concave_radius, result_.concave_point, radius, point);
            } else {
                attain(result_.convex_radius, result_.convex_point, radius, point);
            }
        }
    }

    // A parameter on the face at constant v, so witnesses lie on the face.
    double inside_u(double v) const {
        constexpr int kProbes = 33;
        BRepClass_FaceClassifier classifier;
        for (int index = 0; index <= kProbes; ++index) {
            const double u = u_min_ + (u_max_ - u_min_) * (index + 0.5) / (kProbes + 1);
            classifier.Perform(face_, gp_Pnt2d(u, v), Precision::PConfusion());
            if (classifier.State() == TopAbs_IN || classifier.State() == TopAbs_ON) {
                return u;
            }
        }
        return 0.5 * (u_min_ + u_max_);
    }

    void consider_at_v(double v) { consider(inside_u(v), v); }

    void constant_curvature() { consider_at_v(0.5 * (v_min_ + v_max_)); }

    // Circumferential radius |r(v)| / cos(a) is linear in v, so its minimum
    // over the face is at a v bound, or zero at an apex inside the range.
    void cone() {
        const gp_Cone cone = surface_.Cone();
        const double sine = std::sin(cone.SemiAngle());
        const double first = cone.RefRadius() + v_min_ * sine;
        const double last = cone.RefRadius() + v_max_ * sine;
        consider_at_v(v_min_);
        consider_at_v(v_max_);
        if ((first <= 0.0) != (last <= 0.0)) {
            const double apex = -cone.RefRadius() / sine;
            const gp_Pnt point = surface_.Value(inside_u(apex), apex);
            // The apex keeps the sign the rest of the face has.
            if (result_.convex_radius < kInfinity) {
                attain(result_.convex_radius, result_.convex_point, 0.0, point);
            }
            if (result_.concave_radius < kInfinity) {
                attain(result_.concave_radius, result_.concave_point, 0.0, point);
            }
        }
    }

    // The minor radius is constant; the other principal curvature
    // cos v / (R + r cos v) has stationary points only at multiples of pi.
    void torus() {
        consider_at_v(v_min_);
        consider_at_v(v_max_);
        const auto first = static_cast<long long>(std::ceil(v_min_ / M_PI));
        for (long long turn = first; static_cast<double>(turn) * M_PI < v_max_; ++turn) {
            consider_at_v(static_cast<double>(turn) * M_PI);
        }
    }

    void sampled(uint32_t per_direction) {
        result_.exact = 0;
        BRepClass_FaceClassifier classifier;
        for (uint32_t i = 0; i < per_direction; ++i) {
            const double u = u_min_ + (u_max_ - u_min_) * i / (per_direction - 1);
            for (uint32_t j = 0; j < per_direction; ++j) {
                const double v = v_min_ + (v_max_ - v_min_) * j / (per_direction - 1);
                classifier.Perform(face_, gp_Pnt2d(u, v), Precision::PConfusion());
                if (classifier.State() == TopAbs_OUT) {
                    continue;
                }
                ++result_.samples;
                consider(u, v);
            }
        }
    }

    const occt_bridge_face_radius_bounds_t& result() const { return result_; }

private:
    static void attain(double& radius, occt_bridge_vec3_t& at, double candidate, const gp_Pnt& point) {
        if (candidate < radius) {
            radius = candidate;
            at = vec3(point);
        }
    }

    const TopoDS_Face& face_;
    const BRepAdaptor_Surface& surface_;
    double outward_;
    double u_min_ = 0.0;
    double u_max_ = 0.0;
    double v_min_ = 0.0;
    double v_max_ = 0.0;
    occt_bridge_face_radius_bounds_t result_{};
};

occt_bridge_face_radius_bounds_t face_radius_bounds(const TopoDS_Face& face, uint32_t per_direction) {
    const BRepAdaptor_Surface surface(face);
    RadiusBounds bounds(face, surface);
    switch (surface.GetType()) {
        case GeomAbs_Plane: break;
        case GeomAbs_Cylinder:
        case GeomAbs_Sphere: bounds.constant_curvature(); break;
        case GeomAbs_Cone: bounds.cone(); break;
        case GeomAbs_Torus: bounds.torus(); break;
        default: bounds.sampled(per_direction); break;
    }
    return bounds.result();
}

occt_bridge_edge_concavity_t concavity(const BRepOffset_Analyse& analysis, const TopoDS_Edge& edge) {
    if (BRep_Tool::Degenerated(edge) || !analysis.HasAncestor(edge)) {
        return OCCT_BRIDGE_EDGE_OTHER;
    }
    bool convex = false;
    bool concave = false;
    bool smooth = false;
    for (const auto& interval : analysis.Type(edge)) {
        switch (interval.Type()) {
            case ChFiDS_Convex: convex = true; break;
            case ChFiDS_Concave: concave = true; break;
            case ChFiDS_Mixed: convex = concave = true; break;
            case ChFiDS_Tangential: smooth = true; break;
            default: break;
        }
    }
    if (convex && concave) {
        return OCCT_BRIDGE_EDGE_MIXED;
    }
    if (convex) {
        return OCCT_BRIDGE_EDGE_CONVEX;
    }
    if (concave) {
        return OCCT_BRIDGE_EDGE_CONCAVE;
    }
    return smooth ? OCCT_BRIDGE_EDGE_SMOOTH : OCCT_BRIDGE_EDGE_OTHER;
}

}  // namespace

// O(faces) for analytic faces plus O(samples^2 * face classification) for
// each sampled face. No handles or geometry copies are created.
occt_bridge_status_t occt_bridge_shape_face_radius_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    uint32_t samples_per_direction,
    occt_bridge_face_radius_bounds_t* out_bounds,
    size_t capacity,
    size_t* out_count) {
    if (out_count != nullptr) { *out_count = 0; }
    return guarded(session, [&] {
        if (out_count == nullptr || (out_bounds == nullptr && capacity != 0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "output buffer is invalid");
        }
        if (samples_per_direction < 2 || samples_per_direction > kMaximumSamplesPerDirection) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "samples_per_direction must be in [2, 1024]");
        }
        const auto* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        const auto faces = ordered(*value, TopAbs_FACE);
        const auto count = static_cast<size_t>(faces.Extent());
        bool query_only = false;
        if (const auto status = check_buffer(session, out_bounds, capacity, count, out_count, query_only);
            status != OCCT_BRIDGE_OK || query_only) {
            return status;
        }
        std::vector<occt_bridge_face_radius_bounds_t> results;
        results.reserve(count);
        for (int index = 1; index <= faces.Extent(); ++index) {
            results.push_back(face_radius_bounds(TopoDS::Face(faces(index)), samples_per_direction));
        }
        std::copy(results.begin(), results.end(), out_bounds);
        *out_count = count;
        return succeed(session);
    });
}

// One BRepOffset_Analyse pass over the shape, then O(1) per edge interval list.
occt_bridge_status_t occt_bridge_shape_edge_concavities(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double tangency_radians,
    occt_bridge_edge_concavity_t* out_concavities,
    size_t capacity,
    size_t* out_count) {
    if (out_count != nullptr) { *out_count = 0; }
    return guarded(session, [&] {
        if (out_count == nullptr || (out_concavities == nullptr && capacity != 0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "output buffer is invalid");
        }
        if (!std::isfinite(tangency_radians) || tangency_radians <= 0.0 || tangency_radians >= M_PI_2) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "tangency_radians must be in (0, pi/2)");
        }
        const auto* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        const auto edges = ordered(*value, TopAbs_EDGE);
        const auto count = static_cast<size_t>(edges.Extent());
        bool query_only = false;
        if (const auto status = check_buffer(session, out_concavities, capacity, count, out_count, query_only);
            status != OCCT_BRIDGE_OK || query_only) {
            return status;
        }
        std::vector<occt_bridge_edge_concavity_t> results;
        results.reserve(count);
        if (count != 0) {
            const BRepOffset_Analyse analysis(*value, tangency_radians);
            for (int index = 1; index <= edges.Extent(); ++index) {
                results.push_back(concavity(analysis, TopoDS::Edge(edges(index))));
            }
        }
        std::copy(results.begin(), results.end(), out_concavities);
        *out_count = count;
        return succeed(session);
    });
}
