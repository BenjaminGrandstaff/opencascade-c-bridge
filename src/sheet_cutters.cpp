/*
 * Cutters for sheet-metal holes that cross a bend: an outline laid on the
 * bend's cylinder and pushed radially through the sheet.
 */

#include "bridge_internal.hpp"

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepFill.hxx>
#include <BRepLib.hxx>
#include <GCE2d_MakeArcOfCircle.hxx>
#include <Geom2dConvert.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pnt2d.hxx>

#include <cmath>
#include <vector>

using namespace occt_bridge_internal;

namespace {

// One outline segment as a curve in the cylinder's (angle, height)
// parameters: lines stay lines; a circular arc becomes its rational B-spline
// with poles scaled along the circumference, an exact ellipse arc.
Handle(Geom2d_Curve) parameter_curve(const occt_bridge_wire_segment_t& segment, double radius) {
    const auto map = [&](const occt_bridge_vec3_t& p) { return gp_Pnt2d(p.x / radius, p.y); };
    if (segment.kind == 0) {
        const gp_Pnt2d a = map(segment.start);
        const gp_Pnt2d b = map(segment.end);
        if (a.Distance(b) <= Precision::Confusion()) {
            return nullptr;
        }
        const Handle(Geom2d_Line) line = new Geom2d_Line(a, gp_Dir2d(gp_Vec2d(a, b)));
        return new Geom2d_TrimmedCurve(line, 0.0, a.Distance(b));
    }
    const auto developed = [](const occt_bridge_vec3_t& p) { return gp_Pnt2d(p.x, p.y); };
    GCE2d_MakeArcOfCircle arc(developed(segment.start), developed(segment.middle), developed(segment.end));
    if (!arc.IsDone()) {
        return nullptr;
    }
    Handle(Geom2d_BSplineCurve) spline = Geom2dConvert::CurveToBSplineCurve(arc.Value());
    for (int index = 1; index <= spline->NbPoles(); ++index) {
        const gp_Pnt2d pole = spline->Pole(index);
        spline->SetPole(index, gp_Pnt2d(pole.X() / radius, pole.Y()));
    }
    return spline;
}

}  // namespace

// O(segments): two wires of the outline on concentric cylinders, a ruled
// face between each pair of edges (whose rulings are radial), the two
// cylinder faces, sewn into one solid.
occt_bridge_status_t occt_bridge_create_radial_cutter(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis,
    occt_bridge_vec3_t start,
    double radius,
    double inner,
    double outer,
    const occt_bridge_wire_segment_t* segments,
    size_t segment_count,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr || (segments == nullptr && segment_count != 0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "segments or out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(origin) || !finite(axis) || !finite(start) || !std::isfinite(radius) || !std::isfinite(inner)
            || !std::isfinite(outer) || !(inner > 0.0) || !(radius > inner) || !(outer > radius)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT,
                "radial cutter needs finite vectors and 0 < inner < radius < outer");
        }
        const gp_Vec along(axis.x, axis.y, axis.z);
        const gp_Vec from(start.x, start.y, start.z);
        if (along.Magnitude() <= Precision::Confusion() || from.Magnitude() <= Precision::Confusion()
            || std::abs(along.Normalized().Dot(from.Normalized())) > 1e-9) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT,
                "radial cutter axis and start must be nonzero and perpendicular");
        }
        if (segment_count < 2 || segment_count > 10000) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "radial cutter needs 2 to 10000 segments");
        }
        const gp_Ax3 frame(gp_Pnt(origin.x, origin.y, origin.z), gp_Dir(along), gp_Dir(from));
        const Handle(Geom_CylindricalSurface) inside = new Geom_CylindricalSurface(frame, inner);
        const Handle(Geom_CylindricalSurface) outside = new Geom_CylindricalSurface(frame, outer);
        double lowest = 0.0;
        double highest = 0.0;
        BRepBuilderAPI_MakeWire inner_wire;
        BRepBuilderAPI_MakeWire outer_wire;
        std::vector<TopoDS_Edge> inner_edges;
        std::vector<TopoDS_Edge> outer_edges;
        for (size_t index = 0; index < segment_count; ++index) {
            const auto& segment = segments[index];
            if ((segment.kind != 0 && segment.kind != 1) || !finite(segment.start) || !finite(segment.end)
                || (segment.kind == 1 && !finite(segment.middle))) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "radial cutter segments are lines or arcs of finite points");
            }
            for (const auto& p : {segment.start, segment.end}) {
                lowest = index == 0 ? p.x : std::min(lowest, p.x);
                highest = index == 0 ? p.x : std::max(highest, p.x);
            }
            const Handle(Geom2d_Curve) curve = parameter_curve(segment, radius);
            if (curve.IsNull()) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "radial cutter segment is degenerate");
            }
            BRepBuilderAPI_MakeEdge on_inside(curve, inside);
            BRepBuilderAPI_MakeEdge on_outside(curve, outside);
            if (!on_inside.IsDone() || !on_outside.IsDone()) {
                return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "radial cutter edge could not be built");
            }
            TopoDS_Edge a = on_inside.Edge();
            TopoDS_Edge b = on_outside.Edge();
            BRepLib::BuildCurves3d(a);
            BRepLib::BuildCurves3d(b);
            inner_wire.Add(a);
            outer_wire.Add(b);
            if (!inner_wire.IsDone() || !outer_wire.IsDone()) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "radial cutter outline is not connected");
            }
            inner_edges.push_back(inner_wire.Edge());
            outer_edges.push_back(outer_wire.Edge());
        }
        if (highest - lowest >= 2.0 * M_PI * radius) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "radial cutter outline must span less than a turn");
        }
        BRepBuilderAPI_MakeFace inner_face(inside, inner_wire.Wire(), Standard_True);
        BRepBuilderAPI_MakeFace outer_face(outside, outer_wire.Wire(), Standard_True);
        if (!inner_face.IsDone() || !outer_face.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "radial cutter faces could not be built");
        }
        BRepBuilderAPI_Sewing sewing(1e-6);
        sewing.Add(inner_face.Face());
        sewing.Add(outer_face.Face());
        for (size_t index = 0; index < inner_edges.size(); ++index) {
            const TopoDS_Face side = BRepFill::Face(inner_edges[index], outer_edges[index]);
            if (side.IsNull()) {
                return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "radial cutter side could not be built");
            }
            sewing.Add(side);
        }
        sewing.Perform();
        TopExp_Explorer shells(sewing.SewedShape(), TopAbs_SHELL);
        if (!shells.More()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "radial cutter faces did not close");
        }
        BRepBuilderAPI_MakeSolid solid(TopoDS::Shell(shells.Current()));
        if (!solid.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "radial cutter could not be made solid");
        }
        TopoDS_Solid closed = solid.Solid();
        BRepLib::OrientClosedSolid(closed);
        TopoDS_Shape result = closed;
        std::vector<occt_bridge_history_entry> history;
        if (const occt_bridge_status_t status = check_result(session, "radial cutter", result, &history);
            status != OCCT_BRIDGE_OK) {
            return status;
        }
        return store_shape(session, result, out_shape);
    });
}
