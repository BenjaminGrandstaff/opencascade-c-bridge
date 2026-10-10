/*
 * Sweeps a planar profile along a path wire with a chosen orientation rule.
 */

#include "bridge_internal.hpp"

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepLib.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <gp_Ax3.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepLProp_CLProps.hxx>
#include <BRep_Tool.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopoDS_Vertex.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepTools.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>

#include <algorithm>
#include <cmath>
#include <limits>
#include <string>

using namespace occt_bridge_internal;

namespace {

// The profile's single boundary wire, and whether the sweep is a solid: a face
// or closed wire makes a solid, an open wire a swept surface. Null on failure.
const char* profile_wire(const TopoDS_Shape& profile, TopoDS_Wire& wire, bool& solid) {
    if (profile.ShapeType() == TopAbs_FACE) {
        int wires = 0;
        for (TopExp_Explorer explorer(profile, TopAbs_WIRE); explorer.More(); explorer.Next()) {
            ++wires;
        }
        if (wires != 1) {
            return "sweep profile faces must have one boundary and no holes";
        }
        wire = BRepTools::OuterWire(TopoDS::Face(profile));
        solid = true;
        return wire.IsNull() ? "sweep profile face has no boundary" : nullptr;
    }
    if (profile.ShapeType() != TopAbs_WIRE) {
        return "sweep profile must be a wire or a face";
    }
    wire = TopoDS::Wire(profile);
    solid = BRep_Tool::IsClosed(wire) == Standard_True;
    return nullptr;
}

const char* path_wire(const TopoDS_Shape& path, TopoDS_Wire& wire) {
    if (path.ShapeType() == TopAbs_EDGE) {
        BRepBuilderAPI_MakeWire builder(TopoDS::Edge(path));
        if (!builder.IsDone()) {
            return "sweep path edge could not form a wire";
        }
        wire = builder.Wire();
        return nullptr;
    }
    if (path.ShapeType() != TopAbs_WIRE) {
        return "sweep path must be an edge or a wire";
    }
    wire = TopoDS::Wire(path);
    return nullptr;
}

constexpr int kSamplesPerEdge = 64;

// Smallest radius of curvature along the path: exact for lines and circles,
// sampled at kSamplesPerEdge parameters per edge otherwise.
double smallest_bend_radius(const TopoDS_Wire& path) {
    double smallest = std::numeric_limits<double>::infinity();
    for (TopExp_Explorer explorer(path, TopAbs_EDGE); explorer.More(); explorer.Next()) {
        const BRepAdaptor_Curve curve(TopoDS::Edge(explorer.Current()));
        if (curve.GetType() == GeomAbs_Line) {
            continue;
        }
        if (curve.GetType() == GeomAbs_Circle) {
            smallest = std::min(smallest, curve.Circle().Radius());
            continue;
        }
        BRepLProp_CLProps properties(curve, 2, Precision::Confusion());
        for (int index = 0; index <= kSamplesPerEdge; ++index) {
            const double t = curve.FirstParameter()
                + (curve.LastParameter() - curve.FirstParameter()) * index / kSamplesPerEdge;
            properties.SetParameter(t);
            const double curvature = properties.Curvature();
            if (curvature > Precision::Confusion()) {
                smallest = std::min(smallest, 1.0 / curvature);
            }
        }
    }
    return smallest;
}

// Farthest sampled profile point from the path's start.
double profile_reach(const TopoDS_Wire& profile, const gp_Pnt& start) {
    double farthest = 0.0;
    for (TopExp_Explorer explorer(profile, TopAbs_EDGE); explorer.More(); explorer.Next()) {
        const BRepAdaptor_Curve curve(TopoDS::Edge(explorer.Current()));
        for (int index = 0; index <= kSamplesPerEdge; ++index) {
            const double t = curve.FirstParameter()
                + (curve.LastParameter() - curve.FirstParameter()) * index / kSamplesPerEdge;
            farthest = std::max(farthest, curve.Value(t).Distance(start));
        }
    }
    return farthest;
}

// The kernel's validity check does not see a tube folding through itself
// where the path bends tighter than the profile is wide. Empty when clear.
std::string folding_error(const TopoDS_Wire& section, const TopoDS_Wire& spine) {
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(spine, first, last);
    const double bend = smallest_bend_radius(spine);
    const double reach = first.IsNull() ? 0.0 : profile_reach(section, BRep_Tool::Pnt(first));
    if (reach < bend) {
        return {};
    }
    return "sweep profile reaches " + std::to_string(reach) + " from the path start, but the path bends with radius "
        + std::to_string(bend) + "; the swept shape would intersect itself";
}

const char* orientation_error(occt_bridge_sweep_orientation_t orientation, const occt_bridge_vec3_t& binormal) {
    if (orientation < OCCT_BRIDGE_SWEEP_CORRECTED_FRENET || orientation > OCCT_BRIDGE_SWEEP_FIXED) {
        return "unknown sweep orientation";
    }
    if (orientation == OCCT_BRIDGE_SWEEP_BINORMAL
        && (!finite(binormal)
            || gp_Vec(binormal.x, binormal.y, binormal.z).Magnitude() <= std::numeric_limits<double>::epsilon())) {
        return "sweep binormal must be finite and nonzero";
    }
    return nullptr;
}

void set_orientation(
    BRepOffsetAPI_MakePipeShell& builder,
    occt_bridge_sweep_orientation_t orientation,
    const gp_Vec& binormal) {
    switch (orientation) {
        case OCCT_BRIDGE_SWEEP_FRENET: builder.SetMode(Standard_True); break;
        case OCCT_BRIDGE_SWEEP_BINORMAL: builder.SetMode(gp_Dir(binormal)); break;
        case OCCT_BRIDGE_SWEEP_FIXED: builder.SetMode(gp_Ax2()); break;
        default: builder.SetMode(Standard_False); break;
    }
}

}  // namespace

// One pipe-shell build: O(path edges x profile edges) generated faces.
// Operation history maps each profile edge to the faces it generates.
occt_bridge_status_t occt_bridge_sweep(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t profile,
    occt_bridge_shape_id_t path,
    occt_bridge_sweep_orientation_t orientation,
    occt_bridge_vec3_t binormal,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (const char* error = orientation_error(orientation, binormal)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
        }
        const gp_Vec direction(binormal.x, binormal.y, binormal.z);
        const auto* profile_shape = find_shape(session, profile);
        const auto* path_shape = find_shape(session, path);
        if (profile_shape == nullptr || path_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "sweep profile or path was not found");
        }
        TopoDS_Wire section;
        TopoDS_Wire spine;
        bool solid = false;
        if (const char* error = profile_wire(*profile_shape, section, solid)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
        }
        if (const char* error = path_wire(*path_shape, spine)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
        }
        if (const std::string error = folding_error(section, spine); !error.empty()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
        }
        BRepOffsetAPI_MakePipeShell builder(spine);
        set_orientation(builder, orientation, direction);
        // Sharp path corners are mitered rather than left open.
        builder.SetTransitionMode(BRepBuilderAPI_RightCorner);
        builder.Add(section);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "sweep construction failed");
        }
        if (solid && !builder.MakeSolid()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "swept surface could not close into a solid");
        }
        const TopoDS_Shape shape = builder.Shape();
        if (shape.IsNull() || !ShapeValidator(shape).IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY,
                "sweep produced an invalid shape; the profile may self-intersect along the path");
        }
        return store_shape_with_history(session, shape, out_shape, builder, {profile_shape});
    });
}

// A helix is a straight line on a cylinder: angle advances 2*pi per pitch of
// height. Each turn is an edge keeping that exact 2D curve on the exact
// cylinder with a 3D B-spline within Precision::Confusion(), so sweeps and
// measurements see a helix to kernel tolerance. O(turns) edges.
occt_bridge_status_t occt_bridge_create_helix_wire(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis,
    occt_bridge_vec3_t start_direction,
    double radius,
    double pitch,
    double turns,
    int left_handed,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(origin) || !finite(axis) || !finite(start_direction)
            || !std::isfinite(radius) || !std::isfinite(pitch) || !std::isfinite(turns)
            || radius <= 0.0 || pitch <= 0.0
            || turns <= 0.0 || turns > 10000.0) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "helix needs a finite positive radius and pitch and 0 < turns <= 10000");
        }
        const gp_Vec along(axis.x, axis.y, axis.z);
        const gp_Vec start(start_direction.x, start_direction.y, start_direction.z);
        if (along.Magnitude() <= Precision::Confusion()
            || start.Magnitude() <= Precision::Confusion()
            || along.IsParallel(start, Precision::Angular())) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "helix axis and start direction must be nonzero and not parallel");
        }
        // The start direction is made perpendicular to the axis; the helix
        // starts at origin + radius * that direction.
        const gp_Dir normal(along);
        const gp_Dir x_direction(start - normal.XYZ() * start.Dot(gp_Vec(normal)));
        const gp_Ax3 frame(to_point(origin), normal, x_direction);
        const Handle(Geom_CylindricalSurface) cylinder =
            new Geom_CylindricalSurface(frame, radius);
        const double sweep = (left_handed != 0 ? -2.0 : 2.0) * M_PI;
        const gp_Dir2d direction(sweep, pitch);
        const Handle(Geom2d_Line) line = new Geom2d_Line(gp_Pnt2d(0.0, 0.0), direction);
        // One edge per turn keeps every edge's B-spline (and every swept face)
        // small, so booleans against swept helices stay roughly linear in the
        // number of turns instead of meeting one huge surface.
        // Equal parts of at most one turn, so no sliver edge is left over.
        const double length = turns * std::hypot(sweep, pitch);
        const int edges = std::max(1, static_cast<int>(std::ceil(turns - 1e-9)));
        const double step = length / edges;
        BRepBuilderAPI_MakeWire wire;
        for (int index = 0; index < edges; ++index) {
            const double end = index + 1 == edges ? length : (index + 1) * step;
            BRepBuilderAPI_MakeEdge edge(line, cylinder, index * step, end);
            if (!edge.IsDone()) {
                return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "helix edge could not be built");
            }
            TopoDS_Edge built = edge.Edge();
            if (!BRepLib::BuildCurves3d(built, Precision::Confusion(), GeomAbs_C2, 14, 30)) {
                return fail(
                    session, OCCT_BRIDGE_KERNEL_ERROR, "helix curve could not be approximated");
            }
            wire.Add(built);
            if (!wire.IsDone()) {
                return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "helix wire could not be built");
            }
        }
        return store_shape(session, wire.Wire(), out_shape);
    });
}
