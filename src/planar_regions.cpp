/* Exact bounded planar cells. Split a temporary enclosing face by native
 * curves and discard the exterior cell touching its frame. */
#include "bridge_internal.hpp"
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>
#include <algorithm>
#include <array>
#include <cmath>
#include <sstream>
#include <vector>

using namespace occt_bridge_internal;

extern "C" occt_bridge_status_t occt_bridge_discover_planar_regions(
    occt_bridge_session_t* session, const occt_bridge_shape_id_t* wires, size_t wire_count,
    occt_bridge_vec3_t origin, occt_bridge_vec3_t normal, occt_bridge_vec3_t x_axis,
    const occt_bridge_vec3_t* seeds, size_t seed_count, occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (wires == nullptr || wire_count == 0 || wire_count > 1000 || seed_count > 1000
            || (seed_count != 0 && seeds == nullptr) || !finite(origin) || !finite(normal) || !finite(x_axis)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid planar region inputs or limits");
        }
        gp_Vec n(normal.x, normal.y, normal.z), x(x_axis.x, x_axis.y, x_axis.z);
        if (n.Magnitude() <= Precision::Confusion() || x.Magnitude() <= Precision::Confusion()
            || std::abs(n.Normalized().Dot(x.Normalized())) > 1e-9) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "region plane axes must be nonzero and perpendicular");
        }
        const gp_Ax3 frame(to_point(origin), gp_Dir(n), gp_Dir(x));
        gp_Trsf to_local;
        to_local.SetTransformation(frame);
        for (size_t i = 0; i < seed_count; ++i) {
            if (!finite(seeds[i]) || std::abs(to_point(seeds[i]).Transformed(to_local).Z()) > 1e-6) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "region seeds must lie in the plane");
            }
        }
        Bnd_Box bounds;
        TopTools_ListOfShape tools;
        TopTools_IndexedMapOfShape edges;
        for (size_t i = 0; i < wire_count; ++i) {
            const auto* wire = find_shape(session, wires[i]);
            if (wire == nullptr) return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "region wire was not found");
            if (wire->ShapeType() != TopAbs_WIRE && wire->ShapeType() != TopAbs_EDGE) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "region inputs must be edges or wires");
            }
            TopExp::MapShapes(*wire, TopAbs_EDGE, edges);
            if (edges.Extent() > 10000) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "region inputs exceed 10000 edges");
            Bnd_Box local;
            BRepBndLib::AddOptimal(BRepBuilderAPI_Transform(*wire, to_local).Shape(), local, false, false);
            if (local.IsVoid()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region input is empty");
            double xmin, ymin, zmin, xmax, ymax, zmax;
            local.Get(xmin, ymin, zmin, xmax, ymax, zmax);
            if (std::abs(zmin) > 1e-6 || std::abs(zmax) > 1e-6) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region curves must be coplanar");
            }
            bounds.Add(local);
            tools.Append(*wire);
        }
        if (edges.IsEmpty()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region inputs have no edges");
        double xmin, ymin, zmin, xmax, ymax, zmax;
        bounds.Get(xmin, ymin, zmin, xmax, ymax, zmax);
        const double margin = std::max(1.0, std::max(xmax - xmin, ymax - ymin) * 0.1);
        xmin -= margin; ymin -= margin; xmax += margin; ymax += margin;
        const TopoDS_Face enclosing = BRepBuilderAPI_MakeFace(gp_Pln(frame), xmin, xmax, ymin, ymax).Face();
        BRepAlgoAPI_Splitter split;
        TopTools_ListOfShape arguments;
        arguments.Append(enclosing);
        split.SetArguments(arguments);
        split.SetTools(tools);
        split.SetNonDestructive(true);
        split.Build();
        if (!split.IsDone() || split.HasErrors()) {
            std::ostringstream errors;
            split.DumpErrors(errors);
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region splitting failed: " + errors.str());
        }
        std::vector<TopoDS_Face> cells;
        std::vector<std::array<double, 4>> cell_bounds;
        std::vector<TopoDS_Compound> boundaries;
        BRep_Builder builder;
        for (TopExp_Explorer it(split.Shape(), TopAbs_FACE); it.More(); it.Next()) {
            Bnd_Box local;
            BRepBndLib::AddOptimal(BRepBuilderAPI_Transform(it.Current(), to_local).Shape(), local, false, false);
            double a, b, c, d, e, f;
            local.Get(a, b, c, d, e, f);
            if (a <= xmin + 1e-6 || b <= ymin + 1e-6 || d >= xmax - 1e-6 || e >= ymax - 1e-6) continue;
            const TopoDS_Face cell = TopoDS::Face(it.Current());
            if (!BRepCheck_Analyzer(cell).IsValid()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "discovered region is invalid");
            cells.push_back(cell);
            cell_bounds.push_back({a, b, d, e});
            TopoDS_Compound boundary;
            builder.MakeCompound(boundary);
            for (TopExp_Explorer edge(cell, TopAbs_EDGE); edge.More(); edge.Next()) builder.Add(boundary, edge.Current());
            boundaries.push_back(boundary);
            if (cells.size() > 1000) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region discovery exceeds 1000 cells");
        }
        if (cells.empty()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region curves enclose no bounded cells");
        std::vector<bool> selected(cells.size(), seed_count == 0);
        for (size_t s = 0; s < seed_count; ++s) {
            size_t found = cells.size();
            const gp_Pnt seed = to_point(seeds[s]);
            const gp_Pnt local = seed.Transformed(to_local);
            const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(seed).Vertex();
            for (size_t i = 0; i < cells.size(); ++i) {
                const auto& box = cell_bounds[i];
                if (local.X() < box[0] - 1e-7 || local.Y() < box[1] - 1e-7
                    || local.X() > box[2] + 1e-7 || local.Y() > box[3] + 1e-7) continue;
                BRepExtrema_DistShapeShape distance(vertex, boundaries[i]);
                if (!distance.IsDone()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region boundary distance failed");
                if (distance.Value() <= 1e-7) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region seed lies on a boundary");
                const auto state = BRepClass_FaceClassifier(cells[i], seed, 1e-7).State();
                if (state == TopAbs_ON) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region seed lies on a boundary");
                if (state == TopAbs_IN) {
                    if (found != cells.size()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region seed is ambiguous");
                    found = i;
                }
            }
            if (found == cells.size()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region seed lies outside every bounded cell");
            selected[found] = true;
        }
        TopoDS_Compound result;
        builder.MakeCompound(result);
        for (size_t i = 0; i < cells.size(); ++i) if (selected[i]) builder.Add(result, cells[i]);
        return store_shape(session, result, out_shape);
    });
}
