/*
 * Geometric queries: bounds, area, volume, centre of mass, face normals and
 * planarity, edge length and circle radius, and face tangency.
 */

#include "bridge_internal.hpp"

#include <Bnd_Box.hxx>
#include <BRep_Tool.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepTools.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <GProp_GProps.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopTools_IndexedMapOfShape.hxx>

#include <cmath>
#include <limits>

using namespace occt_bridge_internal;

namespace {

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

/*
 * True when the faces share an edge with G1-or-better continuity. A recorded
 * continuity is authoritative; an edge without one is measured at
 * angular_tolerance radians, or treated as sharp when the tolerance is
 * negative. O(edges of both faces) plus one sampled check per unrecorded
 * shared edge.
 */
bool faces_share_tangent_edge(const TopoDS_Face& first, const TopoDS_Face& second, double angular_tolerance) {
    TopTools_IndexedMapOfShape first_edges;
    TopExp::MapShapes(first, TopAbs_EDGE, first_edges);
    TopTools_IndexedMapOfShape checked;
    for (TopExp_Explorer second_edges(second, TopAbs_EDGE); second_edges.More(); second_edges.Next()) {
        const TopoDS_Edge edge = TopoDS::Edge(second_edges.Current());
        if (!first_edges.Contains(edge) || checked.Add(edge) == 0 || BRep_Tool::Degenerated(edge)) {
            continue;
        }
        GeomAbs_Shape continuity = GeomAbs_C0;
        if (BRep_Tool::HasContinuity(edge, first, second)) {
            continuity = BRep_Tool::Continuity(edge, first, second);
        } else if (angular_tolerance >= 0.0) {
            continuity = BRepLib::ContinuityOfFaces(edge, first, second, angular_tolerance);
        }
        if (continuity >= GeomAbs_G1) {
            return true;
        }
    }
    return false;
}

// Shared argument checks and evaluation for both tangency entry points.
occt_bridge_status_t faces_tangent(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t parent,
    occt_bridge_shape_id_t first_face,
    occt_bridge_shape_id_t second_face,
    double angular_tolerance,
    int* out_are_tangent) {
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
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "tangency faces must belong to the parent");
    }
    if (!first_shape->IsSame(*second_shape)
        && faces_share_tangent_edge(TopoDS::Face(*first_shape), TopoDS::Face(*second_shape), angular_tolerance)) {
        *out_are_tangent = 1;
    }
    return succeed(session);
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_shape_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_bounds_t* out_bounds) {
    return guarded(session, [&] { return shape_bounds(session, shape, out_bounds, false); });
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
        measure_surface_properties(*value, properties);
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
        *out_volume = std::abs(measure_volume(*value));
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
            measure_volume_moments(*value, properties);
        } else if (contains_topology(*value, TopAbs_FACE)) {
            measure_surface_properties(*value, properties);
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

occt_bridge_status_t occt_bridge_shape_faces_are_tangent(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t parent,
    occt_bridge_shape_id_t first_face,
    occt_bridge_shape_id_t second_face,
    int* out_are_tangent) {
    return guarded(session, [&] {
        return faces_tangent(session, parent, first_face, second_face, -1.0, out_are_tangent);
    });
}

occt_bridge_status_t occt_bridge_shape_faces_are_tangent_within(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t parent,
    occt_bridge_shape_id_t first_face,
    occt_bridge_shape_id_t second_face,
    double angular_tolerance,
    int* out_are_tangent) {
    return guarded(session, [&] {
        if (!std::isfinite(angular_tolerance) || angular_tolerance <= 0.0 || angular_tolerance >= M_PI_2) {
            if (out_are_tangent != nullptr) {
                *out_are_tangent = 0;
            }
            return fail(
                session, OCCT_BRIDGE_INVALID_ARGUMENT, "angular tolerance must be in (0, pi/2) radians");
        }
        return faces_tangent(session, parent, first_face, second_face, angular_tolerance, out_are_tangent);
    });
}

}  // extern "C"
