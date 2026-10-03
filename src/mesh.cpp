/* Tagged face triangulation on a private copy of the source topology. */
#include "bridge_internal.hpp"
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <algorithm>
#include <cmath>

using namespace occt_bridge_internal;
namespace {
bool valid_options(const occt_bridge_mesh_options_t& options) {
    return std::isfinite(options.linear_deflection) && options.linear_deflection > 0.0
        && std::isfinite(options.angular_deflection) && options.angular_deflection >= 0.01
        && options.angular_deflection <= std::acos(-1.0)
        && options.maximum_triangles > 0 && options.maximum_triangles <= 1000000;
}

bool safe_resolution(const TopoDS_Shape& shape, double deflection) {
    Bnd_Box bounds;
    BRepBndLib::AddOptimal(shape, bounds, Standard_False, Standard_False);
    if (bounds.IsVoid() || bounds.IsOpen()) { return false; }
    double xmin = 0.0, ymin = 0.0, zmin = 0.0, xmax = 0.0, ymax = 0.0, zmax = 0.0;
    bounds.Get(xmin, ymin, zmin, xmax, ymax, zmax);
    const double span = std::max({xmax - xmin, ymax - ymin, zmax - zmin});
    return std::isfinite(span) && deflection >= std::max(1e-7, span * 1e-5);
}

bool degenerate(const occt_bridge_mesh_triangle_t& triangle) {
    const auto& a = triangle.points[0];
    const auto& b = triangle.points[1];
    const auto& c = triangle.points[2];
    const double scale = std::max({std::abs(b.x-a.x), std::abs(b.y-a.y), std::abs(b.z-a.z),
        std::abs(c.x-a.x), std::abs(c.y-a.y), std::abs(c.z-a.z)});
    if (scale == 0.0) { return true; }
    const gp_Vec ab((b.x-a.x)/scale, (b.y-a.y)/scale, (b.z-a.z)/scale);
    const gp_Vec ac((c.x-a.x)/scale, (c.y-a.y)/scale, (c.z-a.z)/scale);
    const gp_Vec bc((c.x-b.x)/scale, (c.y-b.y)/scale, (c.z-b.z)/scale);
    // Periodic poles may carry triangles with coincident/roundoff-separated
    // nodes. Omit those rather than publish undefined or unstable normals.
    return std::min({ab.SquareMagnitude(), ac.SquareMagnitude(), bc.SquareMagnitude()}) <= 1e-24
        || ab.Crossed(ac).SquareMagnitude() <= 1e-28;
}

occt_bridge_status_t append_face(
    occt_bridge_session_t* session, const TopoDS_Face& face, size_t face_index,
    size_t maximum, std::vector<occt_bridge_mesh_triangle_t>& result) {
    TopLoc_Location location;
    const Handle(Poly_Triangulation) mesh = BRep_Tool::Triangulation(face, location);
    if (mesh.IsNull() || !mesh->HasGeometry()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "face has no complete triangulation");
    }
    const gp_Trsf transform = location.Transformation();
    const bool reverse = (face.Orientation() == TopAbs_REVERSED) != transform.IsNegative();
    for (int index = 1; index <= mesh->NbTriangles(); ++index) {
        int first = 0, second = 0, third = 0;
        mesh->Triangle(index).Get(first, second, third);
        if (reverse) { std::swap(second, third); }
        occt_bridge_mesh_triangle_t triangle{};
        triangle.face_index = face_index;
        const int nodes[3] = {first, second, third};
        for (size_t vertex = 0; vertex < 3; ++vertex) {
            const gp_Pnt point = mesh->Node(nodes[vertex]).Transformed(transform);
            triangle.points[vertex] = {point.X(), point.Y(), point.Z()};
            if (!finite(triangle.points[vertex])) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "mesh contains a nonfinite vertex");
            }
        }
        if (degenerate(triangle)) { continue; }
        if (result.size() == maximum) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "surface mesh exceeds triangle budget");
        }
        result.push_back(triangle);
    }
    return OCCT_BRIDGE_OK;
}

occt_bridge_status_t triangulate(
    occt_bridge_session_t* session, const TopoDS_Shape& shape,
    const occt_bridge_mesh_options_t& options,
    std::vector<occt_bridge_mesh_triangle_t>& result) {
    TopTools_IndexedMapOfShape faces;
    TopExp::MapShapes(shape, TopAbs_FACE, faces);
    if (faces.IsEmpty() || !ShapeValidator(shape).IsValid()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "surface mesh requires valid faces");
    }
    if (!safe_resolution(shape, options.linear_deflection)) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "linear deflection is below the bounded mesh resolution floor");
    }
    BRepBuilderAPI_Copy copy(shape, Standard_True, Standard_False);
    BRepMesh_IncrementalMesh mesher(copy.Shape(), options.linear_deflection, Standard_False,
        options.angular_deflection, Standard_False);
    if (!mesher.IsDone() || mesher.GetStatusFlags() != 0) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "OCCT failed to complete surface meshing");
    }
    for (int index = 1; index <= faces.Extent(); ++index) {
        const auto& original = faces.FindKey(index);
        const auto copied_face = TopoDS::Face(copy.ModifiedShape(original).Oriented(original.Orientation()));
        const auto status = append_face(session, copied_face, static_cast<size_t>(index - 1),
            options.maximum_triangles, result);
        if (status != OCCT_BRIDGE_OK) { return status; }
    }
    if (result.empty()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "surface mesh contains no nondegenerate triangles");
    }
    return OCCT_BRIDGE_OK;
}
}

// Count and fill calls each compute a fresh mesh. Returned buffers are O(T),
// traversal O(faces + T); adaptive meshing adds OCCT geometry-dependent cost.
occt_bridge_status_t occt_bridge_surface_mesh(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    occt_bridge_mesh_options_t options, occt_bridge_mesh_triangle_t* out_triangles,
    size_t capacity, size_t* out_count) {
    if (out_count != nullptr) { *out_count = 0; }
    return guarded(session, [&] {
        if (out_count == nullptr || !valid_options(options)
            || (out_triangles == nullptr && capacity != 0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid surface mesh options or output buffer");
        }
        const auto* value = find_shape(session, shape);
        if (value == nullptr) { return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found"); }
        std::vector<occt_bridge_mesh_triangle_t> triangles;
        const auto status = triangulate(session, *value, options, triangles);
        if (status != OCCT_BRIDGE_OK) { return status; }
        *out_count = triangles.size();
        if (out_triangles == nullptr) { return succeed(session); }
        if (capacity < triangles.size()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "surface mesh output capacity is insufficient");
        }
        std::copy(triangles.begin(), triangles.end(), out_triangles);
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_subshape_indices(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t type, const occt_bridge_shape_id_t* candidates,
    size_t count, size_t* out_indices) {
    return guarded(session, [&] {
        if (type < OCCT_BRIDGE_SHAPE_COMPOUND || type > OCCT_BRIDGE_SHAPE_VERTEX
            || (count != 0 && (candidates == nullptr || out_indices == nullptr))) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid subshape index type or buffers");
        }
        const auto* source = find_shape(session, shape);
        if (source == nullptr) { return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found"); }
        TopTools_IndexedMapOfShape subshapes;
        TopExp::MapShapes(*source, static_cast<TopAbs_ShapeEnum>(static_cast<int>(type) - 1), subshapes);
        std::vector<size_t> indices;
        indices.reserve(count);
        for (size_t candidate = 0; candidate < count; ++candidate) {
            const auto* value = find_shape(session, candidates[candidate]);
            if (value == nullptr) { return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "candidate shape was not found"); }
            const int index = subshapes.FindIndex(*value);
            if (index == 0) { return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "candidate is not a subshape of the requested type"); }
            indices.push_back(static_cast<size_t>(index - 1));
        }
        std::copy(indices.begin(), indices.end(), out_indices);
        return succeed(session);
    });
}
