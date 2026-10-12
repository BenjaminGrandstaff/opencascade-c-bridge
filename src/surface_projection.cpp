/*
 * Nearest points and outward normals on a set of faces, for comparing
 * measured points with their nominal surfaces.
 */

#include "bridge_internal.hpp"

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Geom_Surface.hxx>
#include <Precision.hxx>
#include <ShapeAnalysis_Surface.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt2d.hxx>

#include <algorithm>
#include <array>
#include <cmath>
#include <limits>
#include <memory>
#include <vector>

using namespace occt_bridge_internal;

namespace {

constexpr size_t kMaximumFaces = 100000;
constexpr size_t kMaximumPoints = 10000000;

struct ProjectedFace {
    TopoDS_Face face;
    Handle(Geom_Surface) surface;
    Handle(ShapeAnalysis_Surface) analysis;
    std::unique_ptr<BRepTopAdaptor_FClass2d> classifier;
    BRepGProp_Face properties;
    std::array<double, 6> box{};
    gp_Pnt2d last;
    bool has_last = false;
};

// Lower bound of the distance from `p` to anything inside the box.
double box_distance(const std::array<double, 6>& box, const gp_Pnt& p) {
    const double c[3] = {p.X(), p.Y(), p.Z()};
    double sum = 0.0;
    for (int axis = 0; axis < 3; ++axis) {
        const double below = box[axis] - c[axis];
        const double above = c[axis] - box[axis + 3];
        const double gap = std::max({below, above, 0.0});
        sum += gap * gap;
    }
    return std::sqrt(sum);
}

struct Candidate {
    double distance = std::numeric_limits<double>::infinity();
    gp_Pnt point;
    double u = 0.0;
    double v = 0.0;
};

// Nearest point of one trimmed face: the surface projection when it lands
// inside the face, else an exact point-to-face distance (boundary included).
bool nearest_on(ProjectedFace& face, const gp_Pnt& p, Candidate& out) {
    const double tolerance = Precision::Confusion();
    const gp_Pnt2d uv = face.has_last ? face.analysis->NextValueOfUV(face.last, p, tolerance)
                                      : face.analysis->ValueOfUV(p, tolerance);
    if (face.classifier->Perform(uv) != TopAbs_OUT) {
        face.last = uv;
        face.has_last = true;
        out.point = face.surface->Value(uv.X(), uv.Y());
        out.distance = p.Distance(out.point);
        out.u = uv.X();
        out.v = uv.Y();
        return true;
    }
    BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), face.face);
    if (!distance.IsDone() || distance.NbSolution() == 0) {
        return false;
    }
    out.point = distance.PointOnShape2(1);
    out.distance = distance.Value();
    if (distance.SupportTypeShape2(1) == BRepExtrema_IsInFace) {
        distance.ParOnFaceS2(1, out.u, out.v);
    } else {
        // A boundary point: its parameters on the surface give the normal.
        const gp_Pnt2d boundary = face.analysis->ValueOfUV(out.point, tolerance);
        out.u = boundary.X();
        out.v = boundary.Y();
    }
    return true;
}

// Loads each face's surface, projector, boundary classifier and bounds.
occt_bridge_status_t load_faces(
    occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* faces,
    std::vector<ProjectedFace>& projected) {
    for (size_t i = 0; i < projected.size(); ++i) {
        const auto* shape = find_shape(session, faces[i]);
        if (shape == nullptr) return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "face was not found");
        if (shape->ShapeType() != TopAbs_FACE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not a face");
        }
        auto& face = projected[i];
        face.face = TopoDS::Face(*shape);
        face.surface = BRep_Tool::Surface(face.face);
        if (face.surface.IsNull()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "face has no surface");
        face.analysis = new ShapeAnalysis_Surface(face.surface);
        face.classifier = std::make_unique<BRepTopAdaptor_FClass2d>(face.face, Precision::PConfusion());
        face.properties.Load(face.face);
        Bnd_Box box;
        BRepBndLib::Add(face.face, box);
        box.Get(face.box[0], face.box[1], face.box[2], face.box[3], face.box[4], face.box[5]);
    }
    return OCCT_BRIDGE_OK;
}

// Nearest location of `p` over the faces, starting with `previous` (the last
// winner) so bounding-box pruning skips most faces. Returns the winning face
// index, or the face count when no face gives a usable point and normal.
size_t project_point(
    std::vector<ProjectedFace>& projected,
    const gp_Pnt& p,
    size_t previous,
    occt_bridge_face_projection_t& out) {
    const size_t count = projected.size();
    Candidate best;
    size_t winner = count;
    for (size_t step = 0; step < count; ++step) {
        const size_t i = (previous + step) % count;
        if (box_distance(projected[i].box, p) > best.distance) continue;
        Candidate candidate;
        if (nearest_on(projected[i], p, candidate) && candidate.distance < best.distance) {
            best = candidate;
            winner = i;
        }
    }
    if (winner == count) return count;
    gp_Pnt on;
    gp_Vec normal;
    // Oriented with the face, so out of a solid's material.
    projected[winner].properties.Normal(best.u, best.v, on, normal);
    const double magnitude = normal.Magnitude();
    if (!(magnitude > 1e-12) || !std::isfinite(magnitude)) return count;
    normal.Divide(magnitude);
    out = occt_bridge_face_projection_t{
        occt_bridge_vec3_t{best.point.X(), best.point.Y(), best.point.Z()},
        occt_bridge_vec3_t{normal.X(), normal.Y(), normal.Z()},
        best.distance,
        static_cast<uint32_t>(winner)};
    return winner;
}

}  // namespace

extern "C" {

// O(points * faces) in the worst case; faces whose bounding boxes are
// farther than the best candidate so far are skipped, and each face reuses
// its previous parameters as the next projection's start. No handles are
// created.
occt_bridge_status_t occt_bridge_faces_project_points(
    occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* faces,
    size_t face_count,
    const occt_bridge_vec3_t* points,
    size_t point_count,
    occt_bridge_face_projection_t* out_projections) {
    return guarded(session, [&] {
        if (faces == nullptr || face_count == 0 || face_count > kMaximumFaces
            || (points == nullptr && point_count != 0) || point_count > kMaximumPoints
            || (out_projections == nullptr && point_count != 0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT,
                        "face projection needs 1 to 100000 faces, at most 10000000 points and an output buffer");
        }
        std::vector<ProjectedFace> projected(face_count);
        if (const auto status = load_faces(session, faces, projected); status != OCCT_BRIDGE_OK) {
            return status;
        }
        std::vector<occt_bridge_face_projection_t> results(point_count);
        size_t previous = 0;
        for (size_t index = 0; index < point_count; ++index) {
            if (!finite(points[index])) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "projected points must be finite");
            }
            previous = project_point(projected, to_point(points[index]), previous, results[index]);
            if (previous == face_count) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY,
                            "a point has no nearest face location with a defined normal");
            }
        }
        std::copy(results.begin(), results.end(), out_projections);
        return succeed(session);
    });
}

}  // extern "C"
