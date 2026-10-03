/* Explicit translated closure of an open planar profile; no support inference. */
#include "bridge_internal.hpp"
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Wire.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <TopExp.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Wire.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <Precision.hxx>
#include <gp_Trsf.hxx>

using namespace occt_bridge_internal;

namespace {

bool open_chain(const TopoDS_Wire& wire, TopoDS_Vertex& start, TopoDS_Vertex& end) {
    TopExp::Vertices(wire, start, end);
    if (start.IsNull() || end.IsNull() || start.IsSame(end) || !BRepCheck_Analyzer(wire).IsValid()) {
        return false;
    }
    TopTools_IndexedMapOfShape edges;
    TopExp::MapShapes(wire, TopAbs_EDGE, edges);
    int count = 0;
    for (BRepTools_WireExplorer explorer(wire); explorer.More(); explorer.Next()) {
        ++count;
    }
    return count > 0 && count == edges.Extent();
}

std::vector<occt_bridge_history_entry> strip_history(
    const TopoDS_Wire& source,
    const TopoDS_Face& face,
    const TopLoc_Location& location,
    const TopoDS_Vertex& start,
    const TopoDS_Vertex& end,
    const TopoDS_Edge& start_cap,
    const TopoDS_Edge& end_cap) {
    TopTools_IndexedMapOfShape sources;
    TopExp::MapShapes(source, sources);
    std::vector<occt_bridge_history_entry> entries;
    for (int index = 1; index <= sources.Extent(); ++index) {
        const auto& shape = sources(index);
        occt_bridge_history_entry entry{shape, {}, {}, false};
        if (shape.ShapeType() == TopAbs_WIRE) {
            entry.generated.push_back(face);
            entry.deleted = true;
        } else {
            entry.generated.push_back(shape.Moved(location));
            if (shape.IsSame(start)) {
                entry.generated.push_back(start_cap);
            }
            if (shape.IsSame(end)) {
                entry.generated.push_back(end_cap);
            }
        }
        entries.push_back(std::move(entry));
    }
    return entries;
}

} // namespace

/* Construction/history indexing is O(edges); OCCT's geometric self-intersection
 * check is O(edges²) in the worst case, needed to reject crossing boundaries.
 * Storage is O(edges). No graph or support-face searches are performed. */
occt_bridge_status_t occt_bridge_internal::build_open_profile_face(
    occt_bridge_session_t* session,
    const TopoDS_Shape& value,
    const gp_Vec& translation,
    TopoDS_Face& face,
    std::vector<occt_bridge_history_entry>& entries) {
    if (value.ShapeType() != TopAbs_WIRE) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "open profile requires a wire");
    }
    const auto source = TopoDS::Wire(value);
    TopoDS_Vertex start, end;
    if (!open_chain(source, start, end)) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "profile must be one valid open chain");
    }
    gp_Trsf transform;
    transform.SetTranslation(translation);
    const TopLoc_Location location(transform);
    BRepBuilderAPI_MakeEdge start_builder(TopoDS::Vertex(start.Moved(location)), start);
    BRepBuilderAPI_MakeEdge end_builder(end, TopoDS::Vertex(end.Moved(location)));
    BRepBuilderAPI_MakeWire boundary;
    boundary.Add(source);
    boundary.Add(end_builder.Edge());
    std::vector<TopoDS_Edge> edges;
    for (BRepTools_WireExplorer explorer(source); explorer.More(); explorer.Next()) {
        edges.push_back(explorer.Current());
    }
    for (auto iterator = edges.rbegin(); iterator != edges.rend(); ++iterator) {
        boundary.Add(TopoDS::Edge(iterator->Moved(location).Reversed()));
    }
    boundary.Add(start_builder.Edge());
    if (!boundary.IsDone()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "open profile closure is disconnected");
    }
    BRepBuilderAPI_MakeFace builder(boundary.Wire(), Standard_True);
    if (!builder.IsDone()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "open profile closure must define a planar face");
    }
    face = builder.Face();
    TopoDS_Edge first, second;
    BRepCheck_Wire checker(boundary.Wire());
    if (checker.SelfIntersect(face, first, second) != BRepCheck_NoError
        || !BRepCheck_Analyzer(face).IsValid()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "open profile closure is invalid or self-intersecting");
    }
    entries = strip_history(source, face, location, start, end, start_builder.Edge(), end_builder.Edge());
    return OCCT_BRIDGE_OK;
}

occt_bridge_status_t occt_bridge_create_open_profile_face(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t wire,
    occt_bridge_vec3_t offset,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const gp_Vec translation(offset.x, offset.y, offset.z);
        if (!finite(offset) || translation.SquareMagnitude() <= Precision::SquareConfusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "profile closure offset must be finite and nonzero");
        }
        const auto* value = find_shape(session, wire);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "profile wire was not found");
        }
        TopoDS_Face face;
        std::vector<occt_bridge_history_entry> entries;
        const auto status = build_open_profile_face(session, *value, translation, face, entries);
        if (status != OCCT_BRIDGE_OK) {
            return status;
        }
        return store_shape_with_entries(session, face, out_shape, std::move(entries));
    });
}
