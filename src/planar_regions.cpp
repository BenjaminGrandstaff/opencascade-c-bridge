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

namespace {

struct RegionBox {
    double xmin = 0.0;
    double ymin = 0.0;
    double xmax = 0.0;
    double ymax = 0.0;
};

struct RegionCell {
    TopoDS_Face face;
    std::array<double, 4> bounds{};
    TopoDS_Compound boundary;
};

const char* region_seeds_error(const occt_bridge_vec3_t* seeds, size_t count, const gp_Trsf& to_local) {
    for (size_t i = 0; i < count; ++i) {
        if (!finite(seeds[i]) || std::abs(to_point(seeds[i]).Transformed(to_local).Z()) > 1e-6)
            return "region seeds must lie in the plane";
    }
    return nullptr;
}

occt_bridge_status_t collect_region_inputs(
    occt_bridge_session_t* session, const occt_bridge_shape_id_t* wires, size_t count,
    const gp_Trsf& to_local, Bnd_Box& bounds, TopTools_ListOfShape& tools) {
    TopTools_IndexedMapOfShape edges;
    for (size_t i = 0; i < count; ++i) {
        const auto* wire = find_shape(session, wires[i]);
        if (wire == nullptr) return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "region wire was not found");
        if (wire->ShapeType() != TopAbs_WIRE && wire->ShapeType() != TopAbs_EDGE)
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "region inputs must be edges or wires");
        TopExp::MapShapes(*wire, TopAbs_EDGE, edges);
        if (edges.Extent() > 10000) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "region inputs exceed 10000 edges");
        Bnd_Box local;
        BRepBndLib::AddOptimal(BRepBuilderAPI_Transform(*wire, to_local).Shape(), local, false, false);
        if (local.IsVoid()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region input is empty");
        double xmin, ymin, zmin, xmax, ymax, zmax;
        local.Get(xmin, ymin, zmin, xmax, ymax, zmax);
        if (std::abs(zmin) > 1e-6 || std::abs(zmax) > 1e-6)
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region curves must be coplanar");
        bounds.Add(local);
        tools.Append(*wire);
    }
    if (edges.IsEmpty()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "region inputs have no edges");
    return OCCT_BRIDGE_OK;
}

RegionBox region_frame_bounds(const Bnd_Box& bounds) {
    double xmin, ymin, zmin, xmax, ymax, zmax;
    bounds.Get(xmin, ymin, zmin, xmax, ymax, zmax);
    const double margin = std::max(1.0, std::max(xmax - xmin, ymax - ymin) * 0.1);
    return {xmin - margin, ymin - margin, xmax + margin, ymax + margin};
}

const char* collect_region_cells(const TopoDS_Shape& split, const gp_Trsf& to_local,
                                 const RegionBox& frame, std::vector<RegionCell>& cells) {
    BRep_Builder builder;
    for (TopExp_Explorer it(split, TopAbs_FACE); it.More(); it.Next()) {
        Bnd_Box local;
        BRepBndLib::AddOptimal(BRepBuilderAPI_Transform(it.Current(), to_local).Shape(), local, false, false);
        double a, b, c, d, e, f;
        local.Get(a, b, c, d, e, f);
        // The exterior cell touches the temporary enclosing frame.
        if (a <= frame.xmin + 1e-6 || b <= frame.ymin + 1e-6
            || d >= frame.xmax - 1e-6 || e >= frame.ymax - 1e-6) continue;
        const TopoDS_Face cell = TopoDS::Face(it.Current());
        if (!BRepCheck_Analyzer(cell).IsValid()) return "discovered region is invalid";
        TopoDS_Compound boundary;
        builder.MakeCompound(boundary);
        for (TopExp_Explorer edge(cell, TopAbs_EDGE); edge.More(); edge.Next()) builder.Add(boundary, edge.Current());
        cells.push_back({cell, {a, b, d, e}, boundary});
        if (cells.size() > 1000) return "region discovery exceeds 1000 cells";
    }
    return cells.empty() ? "region curves enclose no bounded cells" : nullptr;
}

occt_bridge_status_t split_region_cells(
    occt_bridge_session_t* session, const gp_Ax3& frame, const gp_Trsf& to_local,
    const Bnd_Box& bounds, const TopTools_ListOfShape& tools, std::vector<RegionCell>& cells) {
    const RegionBox box = region_frame_bounds(bounds);
    const TopoDS_Face enclosing = BRepBuilderAPI_MakeFace(gp_Pln(frame), box.xmin, box.xmax, box.ymin, box.ymax).Face();
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
    if (const char* error = collect_region_cells(split.Shape(), to_local, box, cells))
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
    return OCCT_BRIDGE_OK;
}

const char* select_region_seed(const std::vector<RegionCell>& cells, const gp_Pnt& seed,
                               const gp_Trsf& to_local, size_t& found) {
    found = cells.size();
    const gp_Pnt local = seed.Transformed(to_local);
    const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(seed).Vertex();
    for (size_t i = 0; i < cells.size(); ++i) {
        const auto& box = cells[i].bounds;
        if (local.X() < box[0] - 1e-7 || local.Y() < box[1] - 1e-7
            || local.X() > box[2] + 1e-7 || local.Y() > box[3] + 1e-7) continue;
        BRepExtrema_DistShapeShape distance(vertex, cells[i].boundary);
        if (!distance.IsDone()) return "region boundary distance failed";
        if (distance.Value() <= 1e-7) return "region seed lies on a boundary";
        const auto state = BRepClass_FaceClassifier(cells[i].face, seed, 1e-7).State();
        if (state == TopAbs_ON) return "region seed lies on a boundary";
        if (state == TopAbs_IN) {
            if (found != cells.size()) return "region seed is ambiguous";
            found = i;
        }
    }
    return found == cells.size() ? "region seed lies outside every bounded cell" : nullptr;
}

const char* selected_region_cells(const std::vector<RegionCell>& cells, const gp_Trsf& to_local,
                                  const occt_bridge_vec3_t* seeds, size_t count, TopoDS_Compound& result) {
    std::vector<bool> selected(cells.size(), count == 0);
    for (size_t s = 0; s < count; ++s) {
        size_t found = cells.size();
        if (const char* error = select_region_seed(cells, to_point(seeds[s]), to_local, found)) return error;
        selected[found] = true;
    }
    BRep_Builder builder;
    builder.MakeCompound(result);
    for (size_t i = 0; i < cells.size(); ++i) if (selected[i]) builder.Add(result, cells[i].face);
    return nullptr;
}

occt_bridge_status_t discover_regions(
    occt_bridge_session_t* session, const occt_bridge_shape_id_t* wires, size_t wire_count,
    const occt_bridge_vec3_t& origin, const occt_bridge_vec3_t& normal, const occt_bridge_vec3_t& x_axis,
    const occt_bridge_vec3_t* seeds, size_t seed_count, occt_bridge_shape_id_t* out_shape) {
    if (out_shape == nullptr) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
    *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    if (wires == nullptr || wire_count == 0 || wire_count > 1000 || seed_count > 1000
        || (seed_count != 0 && seeds == nullptr) || !finite(origin) || !finite(normal) || !finite(x_axis))
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid planar region inputs or limits");
    gp_Vec n(normal.x, normal.y, normal.z), x(x_axis.x, x_axis.y, x_axis.z);
    if (n.Magnitude() <= Precision::Confusion() || x.Magnitude() <= Precision::Confusion()
        || std::abs(n.Normalized().Dot(x.Normalized())) > 1e-9)
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "region plane axes must be nonzero and perpendicular");
    const gp_Ax3 frame(to_point(origin), gp_Dir(n), gp_Dir(x));
    gp_Trsf to_local;
    to_local.SetTransformation(frame);
    if (const char* error = region_seeds_error(seeds, seed_count, to_local))
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
    Bnd_Box bounds;
    TopTools_ListOfShape tools;
    if (const auto status = collect_region_inputs(session, wires, wire_count, to_local, bounds, tools); status != OCCT_BRIDGE_OK)
        return status;
    std::vector<RegionCell> cells;
    if (const auto status = split_region_cells(session, frame, to_local, bounds, tools, cells); status != OCCT_BRIDGE_OK)
        return status;
    TopoDS_Compound result;
    if (const char* error = selected_region_cells(cells, to_local, seeds, seed_count, result))
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
    return store_shape(session, result, out_shape);
}

}  // namespace

extern "C" occt_bridge_status_t occt_bridge_discover_planar_regions(
    occt_bridge_session_t* session, const occt_bridge_shape_id_t* wires, size_t wire_count,
    occt_bridge_vec3_t origin, occt_bridge_vec3_t normal, occt_bridge_vec3_t x_axis,
    const occt_bridge_vec3_t* seeds, size_t seed_count, occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return discover_regions(session, wires, wire_count, origin, normal, x_axis, seeds, seed_count, out_shape);
    });
}
