/*
 * Sweeps a planar profile along a path wire with a chosen orientation rule.
 */

#include "bridge_internal.hpp"

#include <BRepBuilderAPI_MakeWire.hxx>
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
