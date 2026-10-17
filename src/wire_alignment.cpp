/* Exact start anchors and traversal reversal on immutable closed profile wires. */
#include "bridge_internal.hpp"
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <Geom_Curve.hxx>
#include <TopExp.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Wire.hxx>
#include <algorithm>
#include <cmath>

using namespace occt_bridge_internal;

occt_bridge_status_t occt_bridge_align_profile_wire(
    occt_bridge_session_t* session, occt_bridge_shape_id_t input,
    const occt_bridge_vec3_t* start_point, int reverse,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (!out_shape) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if ((reverse != 0 && reverse != 1) || (start_point &&
            (!std::isfinite(start_point->x) || !std::isfinite(start_point->y) || !std::isfinite(start_point->z))))
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "profile alignment needs a finite anchor and a valid reverse flag");
        const auto* source = find_shape(session, input);
        if (!source) return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "alignment profile was not found");
        if (source->ShapeType() != TopAbs_WIRE || !BRep_Tool::IsClosed(*source) || !ShapeValidator(*source).IsValid())
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "alignment requires one valid closed wire");
        // Face construction may add pcurves: work only on copied topology/geometry.
        BRepBuilderAPI_Copy copy(*source);
        std::vector<TopoDS_Edge> original, copied, edges;
        for (BRepTools_WireExplorer it(TopoDS::Wire(*source)); it.More(); it.Next()) {
            original.push_back(it.Current());
            copied.push_back(TopoDS::Edge(copy.ModifiedShape(it.Current())));
        }
        if (original.empty() || original.size() > 128)
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "alignment supports 1-128 boundary edges");
        edges = copied;
        if (reverse) {
            std::reverse(edges.begin(), edges.end());
            for (auto& edge : edges) edge.Reverse();
        }
        if (start_point) {
            const gp_Pnt anchor(start_point->x, start_point->y, start_point->z);
            size_t selected = edges.size(), matches = 0;
            for (size_t i = 0; i < edges.size(); ++i) {
                const auto vertex = TopExp::FirstVertex(edges[i], Standard_True);
                if (!vertex.IsNull() && BRep_Tool::Pnt(vertex).Distance(anchor) <= 1e-6) {
                    selected = i;
                    ++matches;
                }
            }
            if (matches > 1)
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "alignment start point matches multiple boundary vertices");
            if (matches == 1) {
                std::rotate(edges.begin(), edges.begin() + selected, edges.end());
            } else if (edges.size() == 1) {
                Standard_Real first, last;
                TopLoc_Location location;
                auto base = BRep_Tool::Curve(edges[0], location, first, last);
                if (base.IsNull() || !base->IsPeriodic() ||
                    std::abs(last - first - base->Period()) > 1e-8 * std::max(1.0, base->Period()))
                    return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "continuous seam anchors require one full periodic boundary edge");
                auto curve = Handle(Geom_Curve)::DownCast(base->Transformed(location.Transformation()));
                GeomAPI_ProjectPointOnCurve projection(anchor, curve, first, last);
                if (projection.NbPoints() == 0 || projection.LowerDistance() > 1e-6)
                    return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "alignment start point is not on the boundary");
                const double parameter = projection.LowerDistanceParameter();
                BRepBuilderAPI_MakeEdge builder(curve, parameter, parameter + curve->Period());
                if (!builder.IsDone())
                    return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "periodic seam construction failed");
                const auto orientation = edges[0].Orientation();
                edges[0] = builder.Edge();
                edges[0].Orientation(orientation);
            } else {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "alignment start point must match an existing boundary vertex");
            }
        }
        TopoDS_Wire wire;
        BRep_Builder builder;
        builder.MakeWire(wire);
        for (const auto& edge : edges) builder.Add(wire, edge);
        wire.Closed(Standard_True);
        BRepBuilderAPI_MakeFace face(wire, Standard_True);
        if (!ShapeValidator(wire).IsValid() || !face.IsDone() || !ShapeValidator(face.Face()).IsValid())
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "aligned profile is not a valid closed planar boundary");
        std::vector<occt_bridge_history_entry> history;
        history.push_back({*source, {}, {wire}, false});
        for (size_t i = 0; i < original.size(); ++i) {
            std::vector<TopoDS_Shape> mapped;
            for (const auto& edge : edges) if (copied[i].IsSame(edge)) mapped.push_back(edge);
            if (original.size() == 1 && mapped.empty()) mapped.push_back(edges[0]);
            history.push_back({original[i], {}, mapped, mapped.empty()});
        }
        return store_shape_with_entries(session, wire, out_shape, std::move(history));
    });
}
