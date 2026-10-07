/* Exact orthographic hidden-line removal and bounded curve sampling. */
#include "bridge_internal.hpp"

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepLib.hxx>
#include <BRep_Builder.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_Algo.hxx>
#include <HLRBRep_HLRToShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>

#include <algorithm>
#include <cmath>
#include <limits>

using namespace occt_bridge_internal;

namespace {
bool direction_of(const occt_bridge_vec3_t& value, gp_Dir& result) {
    if (!finite(value)) { return false; }
    const double maximum = std::max({std::abs(value.x), std::abs(value.y), std::abs(value.z)});
    if (maximum == 0.0) { return false; }
    result = gp_Dir(value.x / maximum, value.y / maximum, value.z / maximum);
    return true;
}

TopoDS_Compound projected_edges(HLRBRep_HLRToShape& filter, bool visible) {
    TopoDS_Compound compound;
    BRep_Builder builder;
    builder.MakeCompound(compound);
    const std::vector<TopoDS_Shape> groups = visible
        ? std::vector<TopoDS_Shape>{filter.VCompound(), filter.Rg1LineVCompound(), filter.OutLineVCompound()}
        : std::vector<TopoDS_Shape>{filter.HCompound(), filter.Rg1LineHCompound(), filter.OutLineHCompound()};
    for (const auto& group : groups) {
        if (!group.IsNull()) { builder.Add(compound, group); }
    }
    BRepLib::BuildCurves3d(compound);
    return compound;
}
}

// HLR compares edges against hiding faces: kernel cost can be O(edges * faces),
// with geometry-dependent intersection work. Input copy and output storage are
// O(topology + projected edges); no searches over model instances occur here.
occt_bridge_status_t occt_bridge_orthographic_projection(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t origin, occt_bridge_vec3_t direction, occt_bridge_vec3_t x_axis,
    occt_bridge_shape_id_t* out_visible, occt_bridge_shape_id_t* out_hidden) {
    if (out_visible != nullptr) { *out_visible = 0; }
    if (out_hidden != nullptr) { *out_hidden = 0; }
    return guarded(session, [&] {
        if (out_visible == nullptr || out_hidden == nullptr || out_visible == out_hidden) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "projection outputs must be distinct nonnull pointers");
        }
        gp_Dir normal;
        gp_Dir right;
        if (!finite(origin) || !direction_of(direction, normal) || !direction_of(x_axis, right)
            || std::abs(normal.Dot(right)) > 1e-9) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "projection needs finite origin and perpendicular nonzero axes");
        }
        const auto* value = find_shape(session, shape);
        if (value == nullptr) { return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found"); }
        if (!contains_topology(*value, TopAbs_EDGE) || !ShapeValidator(*value).IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "projection requires valid geometry");
        }
        // HLR adds outline/pcurve data. Copy geometry to preserve caller shapes.
        BRepBuilderAPI_Copy copy(*value, Standard_True, Standard_False);
        Handle(HLRBRep_Algo) algorithm = new HLRBRep_Algo;
        algorithm->Add(copy.Shape());
        algorithm->Projector(HLRAlgo_Projector(gp_Ax2(gp_Pnt(origin.x, origin.y, origin.z), normal, right)));
        algorithm->Update();
        algorithm->Hide();
        HLRBRep_HLRToShape filter(algorithm);
        const auto visible = projected_edges(filter, true);
        const auto hidden = projected_edges(filter, false);
        const auto status = store_shape(session, visible, out_visible);
        if (status != OCCT_BRIDGE_OK) { return status; }
        try {
            const auto hidden_status = store_shape(session, hidden, out_hidden);
            if (hidden_status == OCCT_BRIDGE_OK) { return hidden_status; }
            session->shapes.erase(*out_visible);
            *out_visible = 0;
            return hidden_status;
        } catch (...) {
            session->shapes.erase(*out_visible);
            *out_visible = 0;
            throw;
        }
    });
}

// O(1) analytic extraction; adaptor applies edge locations to geometry.
occt_bridge_status_t occt_bridge_edge_analytic_curve(
    occt_bridge_session_t* session, occt_bridge_shape_id_t edge,
    occt_bridge_analytic_curve_t* out_curve) {
    if (out_curve != nullptr) { *out_curve = {}; }
    return guarded(session, [&] {
        if (out_curve == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "analytic curve output is null");
        }
        const auto* value = find_shape(session, edge);
        if (value == nullptr) { return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found"); }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "analytic curve requires an edge");
        }
        BRepAdaptor_Curve curve(TopoDS::Edge(*value));
        occt_bridge_analytic_curve_t result{};
        result.first = curve.FirstParameter();
        result.last = curve.LastParameter();
        if (!std::isfinite(result.first) || !std::isfinite(result.last) || result.last <= result.first) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "analytic edge needs a finite positive range");
        }
        if (value->Orientation() == TopAbs_REVERSED) { std::swap(result.first, result.last); }
        if (curve.GetType() == GeomAbs_Line) {
            result.kind = 1;
            const auto start = curve.Value(result.first);
            const auto end = curve.Value(result.last);
            result.origin = {start.X(), start.Y(), start.Z()};
            result.x_vector = {end.X(), end.Y(), end.Z()};
        } else if (curve.GetType() == GeomAbs_Circle || curve.GetType() == GeomAbs_Ellipse) {
            gp_Ax2 axes;
            double major = 0.0, minor = 0.0;
            if (curve.GetType() == GeomAbs_Circle) {
                const auto circle = curve.Circle();
                axes = circle.Position();
                major = minor = circle.Radius();
                result.kind = 2;
            } else {
                const auto ellipse = curve.Ellipse();
                axes = ellipse.Position();
                major = ellipse.MajorRadius();
                minor = ellipse.MinorRadius();
                result.kind = 3;
            }
            const auto center = axes.Location();
            const auto x = axes.XDirection();
            const auto y = axes.YDirection();
            result.origin = {center.X(), center.Y(), center.Z()};
            result.x_vector = {x.X() * major, x.Y() * major, x.Z() * major};
            result.y_vector = {y.X() * minor, y.Y() * minor, y.Z() * minor};
        } else {
            return succeed(session);
        }
        if (!finite(result.origin) || !finite(result.x_vector) || !finite(result.y_vector)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "analytic curve exceeds finite coordinates");
        }
        *out_curve = result;
        return succeed(session);
    });
}

// Bounded O(point_count) time/storage; all evaluations finish before output is
// written. Uniform parameter samples make no chordal-error guarantee.
occt_bridge_status_t occt_bridge_edge_sample_points(
    occt_bridge_session_t* session, occt_bridge_shape_id_t edge, size_t point_count,
    occt_bridge_vec3_t* out_points) {
    return guarded(session, [&] {
        if (out_points == nullptr || point_count < 2 || point_count > 100000) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "edge sampling needs an output buffer and 2–100000 points");
        }
        const auto* value = find_shape(session, edge);
        if (value == nullptr) { return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found"); }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "sampling requires an edge");
        }
        BRepAdaptor_Curve curve(TopoDS::Edge(*value));
        const double first = curve.FirstParameter();
        const double last = curve.LastParameter();
        if (!std::isfinite(first) || !std::isfinite(last) || last <= first) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge curve must have a finite positive parameter range");
        }
        std::vector<occt_bridge_vec3_t> points;
        points.reserve(point_count);
        for (size_t index = 0; index < point_count; ++index) {
            double fraction = static_cast<double>(index) / static_cast<double>(point_count - 1);
            if (value->Orientation() == TopAbs_REVERSED) { fraction = 1.0 - fraction; }
            const auto point = curve.Value((1.0 - fraction) * first + fraction * last);
            const occt_bridge_vec3_t sampled{point.X(), point.Y(), point.Z()};
            if (!finite(sampled)) { return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "sample point is not finite"); }
            points.push_back(sampled);
        }
        std::copy(points.begin(), points.end(), out_points);
        return succeed(session);
    });
}

// Exact non-destructive half-space intersection, with geometry-dependent
// boolean cost and O(input/result topology) history indexing/storage.
occt_bridge_status_t occt_bridge_clip_by_plane(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t plane_origin, occt_bridge_vec3_t plane_normal,
    int keep_positive, occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) { return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null"); }
        *out_shape = 0;
        gp_Dir normal;
        if (!finite(plane_origin) || !direction_of(plane_normal, normal) || (keep_positive != 0 && keep_positive != 1)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "plane clipping needs finite origin, nonzero normal, and a valid side");
        }
        const auto* value = find_shape(session, shape);
        if (value == nullptr) { return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found"); }
        if (!contains_topology(*value, TopAbs_SOLID) || !ShapeValidator(*value).IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "plane clipping requires valid solid geometry");
        }
        const gp_Pnt origin(plane_origin.x, plane_origin.y, plane_origin.z);
        const double maximum = std::max({std::abs(plane_origin.x), std::abs(plane_origin.y), std::abs(plane_origin.z)});
        const double offset = std::max(1.0, maximum * (64.0 * std::numeric_limits<double>::epsilon()));
        const auto reference = origin.Translated(gp_Vec(normal) * (keep_positive != 0 ? offset : -offset));
        if (!finite(occt_bridge_vec3_t{reference.X(), reference.Y(), reference.Z()})) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "plane reference exceeds finite coordinates");
        }
        const BRepBuilderAPI_MakeFace face(gp_Pln(origin, normal));
        const BRepPrimAPI_MakeHalfSpace half_space(face.Face(), reference);
        TopTools_ListOfShape arguments, tools;
        arguments.Append(*value);
        tools.Append(half_space.Solid());
        BRepAlgoAPI_Common common;
        common.SetArguments(arguments);
        common.SetTools(tools);
        common.SetNonDestructive(Standard_True);
        common.Build();
        if (!common.IsDone() || common.HasErrors()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "plane clipping intersection failed");
        }
        return store_checked_result(session, "plane clipping", common.Shape(), out_shape, common, {value});
    });
}
