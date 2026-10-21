/* Exact boundary partitions for explicit constrained-loft correspondence. */
#include "bridge_internal.hpp"
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <Geom_Curve.hxx>
#include <TopExp.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <algorithm>
#include <cmath>

using namespace occt_bridge_internal;

namespace {

const char* partition_points_error(const occt_bridge_vec3_t* points, size_t count) {
    if (!points || count == 0 || count > 128) return "profile partition needs 1-128 split points";
    for (size_t i = 0; i < count; ++i) {
        const auto& p = points[i];
        if (!std::isfinite(p.x) || !std::isfinite(p.y) || !std::isfinite(p.z))
            return "split points must be finite";
        for (size_t j = 0; j < i; ++j)
            if (gp_Pnt(p.x,p.y,p.z).Distance(gp_Pnt(points[j].x,points[j].y,points[j].z)) <= 1e-6)
                return "split points must be distinct";
    }
    return nullptr;
}

struct PartitionEdge {
    TopoDS_Edge original;
    TopoDS_Edge copied;
    Handle(Geom_Curve) curve;
    // The first two entries stay the original domain until all points have
    // been projected. Sort only after collecting every requested cut.
    std::vector<double> cuts;
};

const char* copy_partition_edges(const TopoDS_Shape& source, std::vector<PartitionEdge>& edges) {
    BRepBuilderAPI_Copy copy(source);
    for (BRepTools_WireExplorer it(TopoDS::Wire(source)); it.More(); it.Next()) {
        TopoDS_Edge edge = TopoDS::Edge(copy.ModifiedShape(it.Current()));
        edge.Orientation(it.Current().Orientation());
        Standard_Real first, last;
        TopLoc_Location location;
        auto base = BRep_Tool::Curve(edge, location, first, last);
        if (base.IsNull() || !std::isfinite(first) || !std::isfinite(last) || last <= first)
            return "partition edge has no finite curve domain";
        auto curve = location.IsIdentity() ? base : Handle(Geom_Curve)::DownCast(base->Transformed(location.Transformation()));
        edges.push_back({it.Current(), edge, curve, {first, last}});
    }
    return nullptr;
}

bool partition_vertex(const std::vector<PartitionEdge>& edges, const gp_Pnt& point) {
    bool vertex = false;
    for (const auto& edge : edges)
        for (const auto& v : {TopExp::FirstVertex(edge.copied), TopExp::LastVertex(edge.copied)})
            if (!v.IsNull() && BRep_Tool::Pnt(v).Distance(point) <= 1e-6) vertex = true;
    return vertex;
}

const char* project_partition_point(std::vector<PartitionEdge>& edges, const gp_Pnt& point) {
    size_t matches = 0, selected = 0;
    double parameter = 0.;
    for (size_t e = 0; e < edges.size(); ++e) {
        const auto& edge = edges[e];
        GeomAPI_ProjectPointOnCurve projection(point, edge.curve, edge.cuts[0], edge.cuts[1]);
        for (int j = 1; j <= projection.NbPoints(); ++j) {
            if (projection.Distance(j) > 1e-6) continue;
            ++matches; selected = e; parameter = projection.Parameter(j);
        }
    }
    if (matches != 1) return "split point must match exactly one boundary curve location";
    edges[selected].cuts.push_back(parameter);
    return nullptr;
}

const char* collect_partition_cuts(std::vector<PartitionEdge>& edges,
                                   const occt_bridge_vec3_t* points, size_t count) {
    for (size_t p = 0; p < count; ++p) {
        const gp_Pnt point(points[p].x, points[p].y, points[p].z);
        // An existing vertex is already a boundary partition; retain it unchanged.
        if (partition_vertex(edges, point)) continue;
        if (const char* error = project_partition_point(edges, point)) return error;
    }
    return nullptr;
}

size_t sort_partition_cuts(std::vector<PartitionEdge>& edges) {
    size_t count = 0;
    for (auto& edge : edges) {
        std::sort(edge.cuts.begin(), edge.cuts.end());
        count += edge.cuts.size() - 1;
    }
    return count;
}

const char* split_partition_edge(const PartitionEdge& edge, std::vector<TopoDS_Edge>& pieces) {
    if (edge.cuts.size() == 2) {
        pieces.push_back(edge.copied);
        return nullptr;
    }
    std::vector<TopoDS_Vertex> vertices{TopExp::FirstVertex(edge.copied)};
    for (size_t i = 1; i + 1 < edge.cuts.size(); ++i)
        vertices.push_back(BRepBuilderAPI_MakeVertex(edge.curve->Value(edge.cuts[i])).Vertex());
    vertices.push_back(TopExp::LastVertex(edge.copied));
    for (size_t i = 0; i + 1 < edge.cuts.size(); ++i) {
        BRepBuilderAPI_MakeEdge piece(edge.curve, vertices[i], vertices[i+1], edge.cuts[i], edge.cuts[i+1]);
        if (!piece.IsDone()) return "exact boundary split failed";
        pieces.push_back(piece.Edge());
    }
    if (edge.copied.Orientation() == TopAbs_REVERSED) {
        std::reverse(pieces.begin(), pieces.end());
        for (auto& piece : pieces) piece.Reverse();
    }
    return nullptr;
}

const char* build_partition_wire(const std::vector<PartitionEdge>& edges, TopoDS_Wire& result,
                                 std::vector<occt_bridge_history_entry>& history) {
    BRep_Builder builder;
    builder.MakeWire(result);
    for (const auto& edge : edges) {
        std::vector<TopoDS_Edge> pieces;
        if (const char* error = split_partition_edge(edge, pieces)) return error;
        std::vector<TopoDS_Shape> mapped;
        for (const auto& piece : pieces) {
            builder.Add(result, piece);
            mapped.push_back(piece);
        }
        history.push_back({edge.original, {}, mapped, false});
    }
    result.Closed(Standard_True);
    return nullptr;
}

occt_bridge_status_t partition_profile(
    occt_bridge_session_t* session, occt_bridge_shape_id_t input,
    const occt_bridge_vec3_t* points, size_t point_count,
    occt_bridge_shape_id_t* out_shape) {
    if (!out_shape) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
    *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    if (const char* error = partition_points_error(points, point_count))
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
    const auto* source = find_shape(session, input);
    if (!source) return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "partition profile was not found");
    if (source->ShapeType() != TopAbs_WIRE || !BRep_Tool::IsClosed(*source) || !ShapeValidator(*source).IsValid())
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "partition requires one valid closed planar wire");
    std::vector<PartitionEdge> edges;
    if (const char* error = copy_partition_edges(*source, edges))
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
    if (edges.empty() || edges.size() > 128)
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "partition supports 1-128 boundary edges");
    if (const char* error = collect_partition_cuts(edges, points, point_count))
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
    if (sort_partition_cuts(edges) > 128)
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "partition exceeds 128 resulting edges");
    TopoDS_Wire result;
    std::vector<occt_bridge_history_entry> history;
    history.push_back({*source, {}, {}, false});
    if (const char* error = build_partition_wire(edges, result, history))
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, error);
    BRepBuilderAPI_MakeFace face(result, Standard_True);
    if (!BRep_Tool::IsClosed(result) || !ShapeValidator(result).IsValid() || !face.IsDone() || !ShapeValidator(face.Face()).IsValid())
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "partition did not produce one valid closed planar boundary");
    history[0].modified.push_back(result);
    return store_shape_with_entries(session, result, out_shape, std::move(history));
}

}  // namespace

occt_bridge_status_t occt_bridge_partition_profile_wire(
    occt_bridge_session_t* session, occt_bridge_shape_id_t input,
    const occt_bridge_vec3_t* points, size_t point_count,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] { return partition_profile(session, input, points, point_count, out_shape); });
}
