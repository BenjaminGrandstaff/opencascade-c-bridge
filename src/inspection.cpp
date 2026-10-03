/*
 * Shape inspection: topology, measurements, adjacency, tangency, operation
 * history, and validity.
 */

#include "bridge_internal.hpp"

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepGProp.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include <algorithm>
#include <cmath>
#include <limits>

using namespace occt_bridge_internal;

namespace {

bool bridge_shape_type(occt_bridge_shape_type_t type, TopAbs_ShapeEnum& out_type) {
    switch (type) {
        case OCCT_BRIDGE_SHAPE_COMPOUND: out_type = TopAbs_COMPOUND; return true;
        case OCCT_BRIDGE_SHAPE_COMPSOLID: out_type = TopAbs_COMPSOLID; return true;
        case OCCT_BRIDGE_SHAPE_SOLID: out_type = TopAbs_SOLID; return true;
        case OCCT_BRIDGE_SHAPE_SHELL: out_type = TopAbs_SHELL; return true;
        case OCCT_BRIDGE_SHAPE_FACE: out_type = TopAbs_FACE; return true;
        case OCCT_BRIDGE_SHAPE_WIRE: out_type = TopAbs_WIRE; return true;
        case OCCT_BRIDGE_SHAPE_EDGE: out_type = TopAbs_EDGE; return true;
        case OCCT_BRIDGE_SHAPE_VERTEX: out_type = TopAbs_VERTEX; return true;
        default: return false;
    }
}

occt_bridge_shape_type_t public_shape_type(TopAbs_ShapeEnum type) {
    switch (type) {
        case TopAbs_COMPOUND: return OCCT_BRIDGE_SHAPE_COMPOUND;
        case TopAbs_COMPSOLID: return OCCT_BRIDGE_SHAPE_COMPSOLID;
        case TopAbs_SOLID: return OCCT_BRIDGE_SHAPE_SOLID;
        case TopAbs_SHELL: return OCCT_BRIDGE_SHAPE_SHELL;
        case TopAbs_FACE: return OCCT_BRIDGE_SHAPE_FACE;
        case TopAbs_WIRE: return OCCT_BRIDGE_SHAPE_WIRE;
        case TopAbs_EDGE: return OCCT_BRIDGE_SHAPE_EDGE;
        case TopAbs_VERTEX: return OCCT_BRIDGE_SHAPE_VERTEX;
        default: return 0;
    }
}

TopTools_IndexedMapOfShape descendant_shapes(
    const TopoDS_Shape& shape,
    TopAbs_ShapeEnum type) {
    TopTools_IndexedMapOfShape result;
    for (TopExp_Explorer explorer(shape, type); explorer.More(); explorer.Next()) {
        result.Add(explorer.Current());
    }
    return result;
}

bool shares_descendant(
    const TopoDS_Shape& first,
    const TopoDS_Shape& second,
    TopAbs_ShapeEnum type) {
    for (TopExp_Explorer explorer(first, type); explorer.More(); explorer.Next()) {
        if (is_descendant(second, explorer.Current(), type)) {
            return true;
        }
    }
    return false;
}

/*
 * Resolves `source` against a result's history. For a rigid move a source
 * subshape is modified into itself moved by the location (O(source size) to
 * confirm membership); explicit records are searched directly.
 */
bool resolve_history_entry(
    const occt_bridge_operation_history& history,
    const TopoDS_Shape& source,
    occt_bridge_history_entry& out_entry) {
    if (!history.located_source.IsNull()) {
        if (!belongs_to(history.located_source, source)) {
            return false;
        }
        out_entry = occt_bridge_history_entry{source, {}, {source.Moved(history.location)}, false};
        return true;
    }
    const auto found = std::find_if(
        history.entries.begin(),
        history.entries.end(),
        [&](const occt_bridge_history_entry& entry) { return entry.source.IsSame(source); });
    if (found == history.entries.end()) {
        return false;
    }
    out_entry = *found;
    return true;
}

const std::vector<TopoDS_Shape>* history_relation_shapes(
    const occt_bridge_history_entry& entry,
    occt_bridge_history_relation_t relation) {
    switch (relation) {
        case OCCT_BRIDGE_HISTORY_GENERATED: return &entry.generated;
        case OCCT_BRIDGE_HISTORY_MODIFIED: return &entry.modified;
        default: return nullptr;
    }
}

/*
 * Axis-aligned bounds of a stored shape. Exact bounds follow the geometry
 * without enlarging the box by shape tolerances; the default bounds keep
 * OCCT's tolerance-padded box.
 */
occt_bridge_status_t shape_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_bounds_t* out_bounds,
    bool exact) {
    if (out_bounds == nullptr) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_bounds is null");
    }
    const TopoDS_Shape* value = find_shape(session, shape);
    if (value == nullptr) {
        return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
    }
    Bnd_Box bounds;
    if (exact) {
        BRepBndLib::AddOptimal(*value, bounds, Standard_False, Standard_False);
    } else {
        BRepBndLib::Add(*value, bounds);
    }
    if (bounds.IsVoid() || bounds.IsOpen()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape has no finite bounds");
    }
    bounds.Get(
        out_bounds->min.x, out_bounds->min.y, out_bounds->min.z,
        out_bounds->max.x, out_bounds->max.y, out_bounds->max.z);
    return succeed(session);
}

/* True when the faces share an edge with recorded G1-or-better continuity. */
bool faces_share_tangent_edge(const TopoDS_Face& first, const TopoDS_Face& second) {
    for (TopExp_Explorer first_edges(first, TopAbs_EDGE); first_edges.More(); first_edges.Next()) {
        const TopoDS_Edge edge = TopoDS::Edge(first_edges.Current());
        for (TopExp_Explorer second_edges(second, TopAbs_EDGE); second_edges.More(); second_edges.Next()) {
            if (edge.IsSame(second_edges.Current()) && BRep_Tool::HasContinuity(edge, first, second)
                && BRep_Tool::Continuity(edge, first, second) >= GeomAbs_G1) {
                return true;
            }
        }
    }
    return false;
}

occt_bridge_status_t store_subshape_handles(
    occt_bridge_session_t* session, const TopTools_IndexedMapOfShape& shapes,
    occt_bridge_shape_id_t* out_shapes, size_t* out_count) {
    const auto count = static_cast<size_t>(shapes.Extent());
    std::vector<occt_bridge_shape_id_t> ids(count, 0);
    const auto release = [&] { for (const auto id : ids) { if (id != 0) { session->shapes.erase(id); } } };
    try {
        for (size_t index = 0; index < count; ++index) {
            const auto status = store_shape(session, shapes.FindKey(static_cast<Standard_Integer>(index + 1)), &ids[index]);
            if (status != OCCT_BRIDGE_OK) { release(); return status; }
        }
    } catch (...) { release(); throw; }
    std::copy(ids.begin(), ids.end(), out_shapes);
    *out_count = count;
    return succeed(session);
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_shape_duplicate(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        const TopoDS_Shape copied_shape = *value;
        const auto history = session->histories.find(shape);
        const bool has_history = history != session->histories.end();
        occt_bridge_operation_history copied_history;
        if (has_history) {
            copied_history = history->second;
        }
        const occt_bridge_status_t status = store_shape(session, copied_shape, out_shape);
        if (status != OCCT_BRIDGE_OK || !has_history) {
            return status;
        }
        try {
            session->histories.emplace(*out_shape, std::move(copied_history));
        } catch (...) {
            session->shapes.erase(*out_shape);
            *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
            throw;
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_type(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t* out_type) {
    return guarded(session, [&] {
        if (out_type == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_type is null");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        *out_type = public_shape_type(value->ShapeType());
        if (*out_type == 0) {
            return fail(session, OCCT_BRIDGE_INTERNAL_ERROR, "shape has an unknown topology type");
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_subshape_count(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t subshape_type,
    size_t* out_count) {
    return guarded(session, [&] {
        if (out_count == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_count is null");
        }
        *out_count = 0;
        TopAbs_ShapeEnum topology_type = TopAbs_SHAPE;
        if (!bridge_shape_type(subshape_type, topology_type)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "unknown subshape type");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        *out_count = static_cast<size_t>(descendant_shapes(*value, topology_type).Extent());
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_subshape_at(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t subshape_type,
    size_t index,
    occt_bridge_shape_id_t* out_subshape) {
    return guarded(session, [&] {
        if (out_subshape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_subshape is null");
        }
        *out_subshape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        TopAbs_ShapeEnum topology_type = TopAbs_SHAPE;
        if (!bridge_shape_type(subshape_type, topology_type)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "unknown subshape type");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        const TopTools_IndexedMapOfShape subshapes = descendant_shapes(*value, topology_type);
        if (index >= static_cast<size_t>(subshapes.Extent())) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "subshape index is out of range");
        }
        return store_shape(
            session,
            subshapes.FindKey(static_cast<Standard_Integer>(index + 1)),
            out_subshape);
    });
}

occt_bridge_status_t occt_bridge_shape_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_bounds_t* out_bounds) {
    return guarded(session, [&] { return shape_bounds(session, shape, out_bounds, false); });
}

occt_bridge_status_t occt_bridge_shape_subshapes(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t type, occt_bridge_shape_id_t* out_shapes,
    size_t capacity, size_t* out_count) {
    if (out_count != nullptr) { *out_count = 0; }
    return guarded(session, [&] {
        if (out_count == nullptr) { return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_count is null"); }
        TopAbs_ShapeEnum topology_type = TopAbs_SHAPE;
        if (!bridge_shape_type(type, topology_type) || (out_shapes == nullptr && capacity != 0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "subshape type or output buffer is invalid");
        }
        const auto* value = find_shape(session, shape);
        if (value == nullptr) { return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found"); }
        const auto shapes = descendant_shapes(*value, topology_type);
        const auto count = static_cast<size_t>(shapes.Extent());
        if (out_shapes == nullptr) { *out_count = count; return succeed(session); }
        if (capacity < count) {
            *out_count = count;
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "subshape output capacity is insufficient");
        }
        return store_subshape_handles(session, shapes, out_shapes, out_count);
    });
}

occt_bridge_status_t occt_bridge_shape_exact_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_bounds_t* out_bounds) {
    return guarded(session, [&] { return shape_bounds(session, shape, out_bounds, true); });
}

occt_bridge_status_t occt_bridge_shape_surface_area(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double* out_area) {
    return guarded(session, [&] {
        if (out_area == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_area is null");
        }
        *out_area = 0.0;
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        if (!contains_topology(*value, TopAbs_FACE)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape contains no faces");
        }
        GProp_GProps properties;
        BRepGProp::SurfaceProperties(*value, properties);
        *out_area = std::abs(properties.Mass());
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_volume(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double* out_volume) {
    return guarded(session, [&] {
        if (out_volume == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_volume is null");
        }
        *out_volume = 0.0;
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        if (!contains_topology(*value, TopAbs_SOLID)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape contains no solids");
        }
        GProp_GProps properties;
        BRepGProp::VolumeProperties(*value, properties);
        *out_volume = std::abs(properties.Mass());
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_center_of_mass(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t* out_center) {
    return guarded(session, [&] {
        if (out_center == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_center is null");
        }
        *out_center = {};
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        GProp_GProps properties;
        if (contains_topology(*value, TopAbs_SOLID)) {
            BRepGProp::VolumeProperties(*value, properties);
        } else if (contains_topology(*value, TopAbs_FACE)) {
            BRepGProp::SurfaceProperties(*value, properties);
        } else if (contains_topology(*value, TopAbs_EDGE)) {
            BRepGProp::LinearProperties(*value, properties);
        } else {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_GEOMETRY,
                "shape has no measurable solid, face, or edge topology");
        }
        if (std::abs(properties.Mass()) <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape has zero measure");
        }
        const gp_Pnt center = properties.CentreOfMass();
        *out_center = {center.X(), center.Y(), center.Z()};
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_face_normal(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    occt_bridge_vec3_t* out_normal) {
    return guarded(session, [&] {
        if (out_normal == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_normal is null");
        }
        *out_normal = {};
        const TopoDS_Shape* value = find_shape(session, face);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "face was not found");
        }
        if (value->ShapeType() != TopAbs_FACE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not a face");
        }
        const TopoDS_Face topology_face = TopoDS::Face(*value);
        double first_u = 0.0;
        double last_u = 0.0;
        double first_v = 0.0;
        double last_v = 0.0;
        BRepTools::UVBounds(topology_face, first_u, last_u, first_v, last_v);
        if (!std::isfinite(first_u) || !std::isfinite(last_u)
            || !std::isfinite(first_v) || !std::isfinite(last_v)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "face has no finite UV bounds");
        }
        BRepAdaptor_Surface surface(topology_face, Standard_True);
        BRepLProp_SLProps properties(
            surface,
            (first_u + last_u) * 0.5,
            (first_v + last_v) * 0.5,
            1,
            Precision::Confusion());
        if (!properties.IsNormalDefined()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "face normal is undefined");
        }
        gp_Dir normal = properties.Normal();
        if (topology_face.Orientation() == TopAbs_REVERSED) {
            normal.Reverse();
        }
        *out_normal = {normal.X(), normal.Y(), normal.Z()};
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_face_is_planar(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    int* out_is_planar) {
    return guarded(session, [&] {
        if (out_is_planar == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_is_planar is null");
        }
        *out_is_planar = 0;
        const TopoDS_Shape* value = find_shape(session, face);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "face was not found");
        }
        if (value->ShapeType() != TopAbs_FACE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not a face");
        }
        const BRepAdaptor_Surface surface(TopoDS::Face(*value), Standard_True);
        *out_is_planar = surface.GetType() == GeomAbs_Plane ? 1 : 0;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_edge_length(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    double* out_length) {
    return guarded(session, [&] {
        if (out_length == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_length is null");
        }
        *out_length = 0.0;
        const TopoDS_Shape* value = find_shape(session, edge);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found");
        }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not an edge");
        }
        GProp_GProps properties;
        BRepGProp::LinearProperties(*value, properties);
        const double length = std::abs(properties.Mass());
        if (!std::isfinite(length) || length <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge has zero or invalid length");
        }
        *out_length = length;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_edge_circle_radius(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    int* out_is_circle,
    double* out_radius) {
    return guarded(session, [&] {
        if (out_is_circle == nullptr || out_radius == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "circle query output is null");
        }
        *out_is_circle = 0;
        *out_radius = 0.0;
        const TopoDS_Shape* value = find_shape(session, edge);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found");
        }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not an edge");
        }
        const BRepAdaptor_Curve curve(TopoDS::Edge(*value));
        if (curve.GetType() == GeomAbs_Circle) {
            *out_is_circle = 1;
            *out_radius = curve.Circle().Radius();
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_is_adjacent(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t parent,
    occt_bridge_shape_id_t first,
    occt_bridge_shape_id_t second,
    int* out_is_adjacent) {
    return guarded(session, [&] {
        if (out_is_adjacent == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_is_adjacent is null");
        }
        *out_is_adjacent = 0;
        const TopoDS_Shape* parent_shape = find_shape(session, parent);
        const TopoDS_Shape* first_shape = find_shape(session, first);
        const TopoDS_Shape* second_shape = find_shape(session, second);
        if (parent_shape == nullptr || first_shape == nullptr || second_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "adjacency shape was not found");
        }
        if (!belongs_to(*parent_shape, *first_shape) || !belongs_to(*parent_shape, *second_shape)) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "adjacency shapes must belong to the parent");
        }
        const TopAbs_ShapeEnum first_type = first_shape->ShapeType();
        const TopAbs_ShapeEnum second_type = second_shape->ShapeType();
        bool adjacent = false;
        if (first_shape->IsSame(*second_shape)) {
            adjacent = false;
        } else if (first_type == TopAbs_FACE && second_type == TopAbs_EDGE) {
            adjacent = is_descendant(*first_shape, *second_shape, TopAbs_EDGE);
        } else if (first_type == TopAbs_EDGE && second_type == TopAbs_FACE) {
            adjacent = is_descendant(*second_shape, *first_shape, TopAbs_EDGE);
        } else if (first_type == TopAbs_FACE && second_type == TopAbs_FACE) {
            adjacent = shares_descendant(*first_shape, *second_shape, TopAbs_EDGE);
        } else if (first_type == TopAbs_EDGE && second_type == TopAbs_EDGE) {
            adjacent = shares_descendant(*first_shape, *second_shape, TopAbs_VERTEX);
        } else {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "unsupported adjacency topology pair");
        }
        *out_is_adjacent = adjacent ? 1 : 0;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_is_same(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t first,
    occt_bridge_shape_id_t second,
    int* out_is_same) {
    return guarded(session, [&] {
        if (out_is_same == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_is_same is null");
        }
        *out_is_same = 0;
        const TopoDS_Shape* first_shape = find_shape(session, first);
        const TopoDS_Shape* second_shape = find_shape(session, second);
        if (first_shape == nullptr || second_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "identity shape was not found");
        }
        *out_is_same = first_shape->IsSame(*second_shape) ? 1 : 0;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_faces_are_tangent(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t parent,
    occt_bridge_shape_id_t first_face,
    occt_bridge_shape_id_t second_face,
    int* out_are_tangent) {
    return guarded(session, [&] {
        if (out_are_tangent == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_are_tangent is null");
        }
        *out_are_tangent = 0;
        const TopoDS_Shape* parent_shape = find_shape(session, parent);
        const TopoDS_Shape* first_shape = find_shape(session, first_face);
        const TopoDS_Shape* second_shape = find_shape(session, second_face);
        if (parent_shape == nullptr || first_shape == nullptr || second_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "tangency shape was not found");
        }
        if (first_shape->ShapeType() != TopAbs_FACE || second_shape->ShapeType() != TopAbs_FACE) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "tangency requires two faces");
        }
        if (!belongs_to(*parent_shape, *first_shape) || !belongs_to(*parent_shape, *second_shape)) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "tangency faces must belong to the parent");
        }
        if (!first_shape->IsSame(*second_shape)
            && faces_share_tangent_edge(TopoDS::Face(*first_shape), TopoDS::Face(*second_shape))) {
            *out_are_tangent = 1;
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_history_count(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t source,
    occt_bridge_history_relation_t relation,
    size_t* out_count) {
    return guarded(session, [&] {
        if (out_count == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_count is null");
        }
        *out_count = 0;
        const TopoDS_Shape* result_shape = find_shape(session, result);
        const TopoDS_Shape* source_shape = find_shape(session, source);
        if (result_shape == nullptr || source_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "history shape was not found");
        }
        static_cast<void>(result_shape);
        const auto result_history = session->histories.find(result);
        if (result_history == session->histories.end()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "result shape has no operation history");
        }
        occt_bridge_history_entry resolved;
        if (!resolve_history_entry(result_history->second, *source_shape, resolved)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "source is not an input to the result operation");
        }
        const occt_bridge_history_entry* entry = &resolved;
        const std::vector<TopoDS_Shape>* related = history_relation_shapes(*entry, relation);
        if (related == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "unknown history relation");
        }
        *out_count = related->size();
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_history_at(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t source,
    occt_bridge_history_relation_t relation,
    size_t index,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const TopoDS_Shape* result_shape = find_shape(session, result);
        const TopoDS_Shape* source_shape = find_shape(session, source);
        if (result_shape == nullptr || source_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "history shape was not found");
        }
        static_cast<void>(result_shape);
        const auto result_history = session->histories.find(result);
        if (result_history == session->histories.end()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "result shape has no operation history");
        }
        occt_bridge_history_entry resolved;
        if (!resolve_history_entry(result_history->second, *source_shape, resolved)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "source is not an input to the result operation");
        }
        const occt_bridge_history_entry* entry = &resolved;
        const std::vector<TopoDS_Shape>* related = history_relation_shapes(*entry, relation);
        if (related == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "unknown history relation");
        }
        if (index >= related->size()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "history index is out of range");
        }
        return store_shape(session, (*related)[index], out_shape);
    });
}

occt_bridge_status_t occt_bridge_shape_history_is_deleted(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t source,
    int* out_is_deleted) {
    return guarded(session, [&] {
        if (out_is_deleted == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_is_deleted is null");
        }
        *out_is_deleted = 0;
        const TopoDS_Shape* result_shape = find_shape(session, result);
        const TopoDS_Shape* source_shape = find_shape(session, source);
        if (result_shape == nullptr || source_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "history shape was not found");
        }
        static_cast<void>(result_shape);
        const auto result_history = session->histories.find(result);
        if (result_history == session->histories.end()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "result shape has no operation history");
        }
        occt_bridge_history_entry resolved;
        if (!resolve_history_entry(result_history->second, *source_shape, resolved)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "source is not an input to the result operation");
        }
        const occt_bridge_history_entry* entry = &resolved;
        *out_is_deleted = entry->deleted ? 1 : 0;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_is_valid(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    int* out_is_valid) {
    return guarded(session, [&] {
        if (out_is_valid == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_is_valid is null");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        const ShapeValidator analyzer(*value);
        *out_is_valid = analyzer.IsValid() ? 1 : 0;
        return succeed(session);
    });
}

}  // extern "C"
