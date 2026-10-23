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

struct RadialOutline {
    double lowest = 0.0;
    double highest = 0.0;
    BRepBuilderAPI_MakeWire inner_wire;
    BRepBuilderAPI_MakeWire outer_wire;
    std::vector<TopoDS_Edge> inner_edges;
    std::vector<TopoDS_Edge> outer_edges;
};

occt_bridge_status_t append_radial_segment(occt_bridge_session_t* session,
    const occt_bridge_wire_segment_t& segment, size_t index, double radius,
    const Handle(Geom_CylindricalSurface)& inside, const Handle(Geom_CylindricalSurface)& outside,
    RadialOutline& outline) {
    if ((segment.kind != 0 && segment.kind != 1) || !finite(segment.start) || !finite(segment.end)
        || (segment.kind == 1 && !finite(segment.middle))) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "radial cutter segments are lines or arcs of finite points");
    }
    for (const auto& p : {segment.start, segment.end}) {
        outline.lowest = index == 0 ? p.x : std::min(outline.lowest, p.x);
        outline.highest = index == 0 ? p.x : std::max(outline.highest, p.x);
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
    outline.inner_wire.Add(a);
    outline.outer_wire.Add(b);
    if (!outline.inner_wire.IsDone() || !outline.outer_wire.IsDone()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "radial cutter outline is not connected");
    }
    outline.inner_edges.push_back(outline.inner_wire.Edge());
    outline.outer_edges.push_back(outline.outer_wire.Edge());
    return OCCT_BRIDGE_OK;
}

occt_bridge_status_t sew_radial_outline(occt_bridge_session_t* session,
    const Handle(Geom_CylindricalSurface)& inside, const Handle(Geom_CylindricalSurface)& outside,
    RadialOutline& outline, TopoDS_Shape& result) {
    BRepBuilderAPI_MakeFace inner_face(inside, outline.inner_wire.Wire(), Standard_True);
    BRepBuilderAPI_MakeFace outer_face(outside, outline.outer_wire.Wire(), Standard_True);
    if (!inner_face.IsDone() || !outer_face.IsDone()) {
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "radial cutter faces could not be built");
    }
    BRepBuilderAPI_Sewing sewing(1e-6);
    sewing.Add(inner_face.Face());
    sewing.Add(outer_face.Face());
    for (size_t index = 0; index < outline.inner_edges.size(); ++index) {
        const TopoDS_Face side = BRepFill::Face(outline.inner_edges[index], outline.outer_edges[index]);
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
    result = closed;
    return OCCT_BRIDGE_OK;
}

occt_bridge_status_t create_radial_cutter(occt_bridge_session_t* session,
    occt_bridge_vec3_t origin, occt_bridge_vec3_t axis, occt_bridge_vec3_t start,
    double radius, double inner, double outer, const occt_bridge_wire_segment_t* segments,
    size_t segment_count, occt_bridge_shape_id_t* out_shape) {
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
    RadialOutline outline;
    for (size_t index = 0; index < segment_count; ++index) {
        const auto status = append_radial_segment(session, segments[index], index, radius, inside, outside, outline);
        if (status != OCCT_BRIDGE_OK) return status;
    }
    if (outline.highest - outline.lowest >= 2.0 * M_PI * radius) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "radial cutter outline must span less than a turn");
    }
    TopoDS_Shape result;
    const auto sew_status = sew_radial_outline(session, inside, outside, outline, result);
    if (sew_status != OCCT_BRIDGE_OK) return sew_status;
    std::vector<occt_bridge_history_entry> history;
    if (const occt_bridge_status_t status = check_result(session, "radial cutter", result, &history);
        status != OCCT_BRIDGE_OK) {
        return status;
    }
    return store_shape(session, result, out_shape);
}

}  // namespace

// O(segments): mapped cylinder outlines, radial sides, then one sewn solid.
occt_bridge_status_t occt_bridge_create_radial_cutter(
    occt_bridge_session_t* session, occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis, occt_bridge_vec3_t start,
    double radius, double inner, double outer,
    const occt_bridge_wire_segment_t* segments, size_t segment_count,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return create_radial_cutter(session, origin, axis, start, radius, inner, outer, segments, segment_count, out_shape);
    });
}
