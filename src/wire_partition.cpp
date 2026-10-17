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

occt_bridge_status_t occt_bridge_partition_profile_wire(
    occt_bridge_session_t* session, occt_bridge_shape_id_t input,
    const occt_bridge_vec3_t* points, size_t point_count,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (!out_shape) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!points || point_count == 0 || point_count > 128)
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "profile partition needs 1-128 split points");
        for (size_t i = 0; i < point_count; ++i) {
            const auto& p = points[i];
            if (!std::isfinite(p.x) || !std::isfinite(p.y) || !std::isfinite(p.z))
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "split points must be finite");
            for (size_t j = 0; j < i; ++j)
                if (gp_Pnt(p.x,p.y,p.z).Distance(gp_Pnt(points[j].x,points[j].y,points[j].z)) <= 1e-6)
                    return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "split points must be distinct");
        }
        const auto* source = find_shape(session, input);
        if (!source) return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "partition profile was not found");
        if (source->ShapeType() != TopAbs_WIRE || !BRep_Tool::IsClosed(*source) || !ShapeValidator(*source).IsValid())
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "partition requires one valid closed planar wire");
        BRepBuilderAPI_Copy copy(*source);
        std::vector<TopoDS_Edge> original, edges;
        std::vector<Handle(Geom_Curve)> curves;
        std::vector<std::vector<double>> cuts;
        for (BRepTools_WireExplorer it(TopoDS::Wire(*source)); it.More(); it.Next()) {
            original.push_back(it.Current());
            edges.push_back(TopoDS::Edge(copy.ModifiedShape(it.Current())));
            edges.back().Orientation(it.Current().Orientation());
            Standard_Real first, last;
            TopLoc_Location location;
            auto base = BRep_Tool::Curve(edges.back(), location, first, last);
            if (base.IsNull() || !std::isfinite(first) || !std::isfinite(last) || last <= first)
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "partition edge has no finite curve domain");
            curves.push_back(location.IsIdentity() ? base : Handle(Geom_Curve)::DownCast(base->Transformed(location.Transformation())));
            cuts.push_back({first, last});
        }
        if (edges.empty() || edges.size() > 128)
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "partition supports 1-128 boundary edges");
        for (size_t p = 0; p < point_count; ++p) {
            const gp_Pnt point(points[p].x, points[p].y, points[p].z);
            // An existing vertex is already a boundary partition; retain it unchanged.
            bool vertex = false;
            for (const auto& edge : edges)
                for (const auto& v : {TopExp::FirstVertex(edge), TopExp::LastVertex(edge)})
                    if (!v.IsNull() && BRep_Tool::Pnt(v).Distance(point) <= 1e-6) vertex = true;
            if (vertex) continue;
            size_t matches = 0, selected = 0;
            double parameter = 0.;
            for (size_t e = 0; e < edges.size(); ++e) {
                GeomAPI_ProjectPointOnCurve projection(point, curves[e], cuts[e][0], cuts[e][1]);
                for (int j = 1; j <= projection.NbPoints(); ++j) {
                    if (projection.Distance(j) > 1e-6) continue;
                    ++matches; selected = e; parameter = projection.Parameter(j);
                }
            }
            if (matches != 1)
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "split point must match exactly one boundary curve location");
            cuts[selected].push_back(parameter);
        }
        size_t count = 0;
        for (auto& edge_cuts : cuts) { std::sort(edge_cuts.begin(), edge_cuts.end()); count += edge_cuts.size() - 1; }
        if (count > 128) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "partition exceeds 128 resulting edges");
        TopoDS_Wire result;
        BRep_Builder builder;
        builder.MakeWire(result);
        std::vector<occt_bridge_history_entry> history;
        history.push_back({*source, {}, {}, false});
        for (size_t e = 0; e < edges.size(); ++e) {
            std::vector<TopoDS_Edge> pieces;
            if (cuts[e].size() == 2) pieces.push_back(edges[e]);
            else {
                std::vector<TopoDS_Vertex> vertices{TopExp::FirstVertex(edges[e])};
                for (size_t i = 1; i + 1 < cuts[e].size(); ++i)
                    vertices.push_back(BRepBuilderAPI_MakeVertex(curves[e]->Value(cuts[e][i])).Vertex());
                vertices.push_back(TopExp::LastVertex(edges[e]));
                for (size_t i = 0; i + 1 < cuts[e].size(); ++i) {
                    BRepBuilderAPI_MakeEdge piece(curves[e], vertices[i], vertices[i+1], cuts[e][i], cuts[e][i+1]);
                    if (!piece.IsDone()) return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "exact boundary split failed");
                    pieces.push_back(piece.Edge());
                }
                if (edges[e].Orientation() == TopAbs_REVERSED) {
                    std::reverse(pieces.begin(), pieces.end());
                    for (auto& piece : pieces) piece.Reverse();
                }
            }
            std::vector<TopoDS_Shape> mapped;
            for (const auto& piece : pieces) { builder.Add(result, piece); mapped.push_back(piece); }
            history.push_back({original[e], {}, mapped, false});
        }
        result.Closed(Standard_True);
        BRepBuilderAPI_MakeFace face(result, Standard_True);
        if (!BRep_Tool::IsClosed(result) || !ShapeValidator(result).IsValid() || !face.IsDone() || !ShapeValidator(face.Face()).IsValid())
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "partition did not produce one valid closed planar boundary");
        history[0].modified.push_back(result);
        return store_shape_with_entries(session, result, out_shape, std::move(history));
    });
}
