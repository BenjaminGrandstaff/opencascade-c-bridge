/* Exact finite-face termination of planar profile prisms. */
#include "bridge_internal.hpp"

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>
#include <GProp_GProps.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <gp_Vec.hxx>
#include <gp_Lin.hxx>

#include <algorithm>
#include <cmath>

using namespace occt_bridge_internal;

namespace {
double area(const TopoDS_Shape& shape) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(shape, props);
    return props.Mass();
}


double shared_area(const TopoDS_Shape& solid, const TopoDS_Shape& face) {
    BRepAlgoAPI_Common common;
    TopTools_ListOfShape args, tools;
    args.Append(solid);
    tools.Append(face);
    common.SetArguments(args);
    common.SetTools(tools);
    common.SetNonDestructive(Standard_True);
    common.Build();
    if (!common.IsDone() || common.HasErrors()) {
        throw Standard_Failure("extrusion cap intersection failed");
    }
    return area(common.Shape());
}

void retained(std::vector<TopoDS_Shape>& values, const TopTools_IndexedMapOfShape& output) {
    values.erase(std::remove_if(values.begin(), values.end(), [&](const TopoDS_Shape& s) {
        return !output.Contains(s);
    }), values.end());
}

std::vector<TopoDS_Shape> descendants(
    const std::vector<TopoDS_Shape>& before, BRepAlgoAPI_Splitter& splitter,
    const TopTools_IndexedMapOfShape& output) {
    std::vector<TopoDS_Shape> result;
    for (const auto& source : before) {
        if (output.Contains(source)) {
            append_unique_shape(result, source);
        }
        const auto& modified = splitter.Modified(source);
        for (TopTools_ListIteratorOfListOfShape it(modified); it.More(); it.Next()) {
            if (output.Contains(it.Value())) {
                append_unique_shape(result, it.Value());
            }
        }
    }
    return result;
}
}

occt_bridge_status_t occt_bridge_create_prism_until_face(
    occt_bridge_session_t* session, occt_bridge_shape_id_t profile,
    occt_bridge_vec3_t direction, occt_bridge_shape_id_t limiting_face,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const gp_Vec travel(direction.x, direction.y, direction.z);
        if (!finite(direction) || travel.Magnitude() <= Precision::Confusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "prism search travel must be finite and nonzero");
        }
        const auto* base = find_shape(session, profile);
        const auto* limit = find_shape(session, limiting_face);
        if (base == nullptr || limit == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "profile or limiting face was not found");
        }
        if (base->ShapeType() != TopAbs_FACE || limit->ShapeType() != TopAbs_FACE
            || !ShapeValidator(*base).IsValid() || !ShapeValidator(*limit).IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "prism limits require valid faces");
        }
        if (BRepAdaptor_Surface(TopoDS::Face(*base)).GetType() != GeomAbs_Plane) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "extrusion profile must be planar");
        }
        BRepPrimAPI_MakePrism prism(TopoDS::Face(*base), travel, Standard_True);
        prism.Build();
        if (!prism.IsDone() || prism.Shape().ShapeType() != TopAbs_SOLID
            || !ShapeValidator(prism.Shape()).IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "profile did not produce a valid search prism");
        }
        TopTools_ListOfShape args, tools;
        args.Append(prism.Shape());
        tools.Append(*limit);
        BRepAlgoAPI_Splitter splitter;
        splitter.SetArguments(args);
        splitter.SetTools(tools);
        splitter.SetNonDestructive(Standard_True);
        splitter.Build();
        if (!splitter.IsDone() || splitter.HasErrors()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "limiting face split failed");
        }
        const double base_area = area(*base);
        const double area_margin = base_area * 1e-9;
        if (!(base_area > 0.0)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "profile area must be positive");
        }
        TopoDS_Shape result;
        const auto solids = ordered(splitter.Shape(), TopAbs_SOLID);
        for (int i = 1; i <= solids.Extent(); ++i) {
            const auto& solid = solids(i);
            const auto faces = ordered(solid, TopAbs_FACE);
            // Unchanged complete base identity is an exact coverage witness.
            // Only a modified base requires a Boolean area comparison.
            if (faces.Contains(prism.FirstShape())
                || std::abs(shared_area(solid, prism.FirstShape()) - base_area) <= area_margin) {
                if (!result.IsNull()) {
                    return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "limiting face has ambiguous base-connected regions");
                }
                result = solid;
            }
        }
        if (result.IsNull() || !ShapeValidator(result).IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "limiting face must retain one complete base region");
        }
        TopTools_IndexedMapOfShape output;
        TopExp::MapShapes(result, output);
        if (output.Contains(prism.LastShape())) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "limiting face does not terminate the complete profile");
        }
        const auto& far_modified = splitter.Modified(prism.LastShape());
        for (TopTools_ListIteratorOfListOfShape it(far_modified); it.More(); it.Next()) {
            if (output.Contains(it.Value()) && area(it.Value()) > area_margin) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "limiting face leaves part of the search end");
            }
        }
        // Splitter history identifies exactly the bounded cap fragments in the
        // chosen solid; avoid repeating its face intersection as a Boolean.
        TopoDS_Compound cap;
        BRep_Builder cap_builder;
        cap_builder.MakeCompound(cap);
        if (output.Contains(*limit)) {
            cap_builder.Add(cap, *limit);
        }
        const auto& cap_modified = splitter.Modified(*limit);
        for (TopTools_ListIteratorOfListOfShape it(cap_modified); it.More(); it.Next()) {
            if (it.Value().ShapeType() == TopAbs_FACE && output.Contains(it.Value())) {
                cap_builder.Add(cap, it.Value());
            }
        }
        if (area(cap) <= area_margin) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "limiting face has no complete cutoff cap");
        }
        // The cap must be strictly separated from the base, including rims;
        // a tangential zero-depth contact is not a positive extrusion extent.
        BRepExtrema_DistShapeShape separation(prism.FirstShape(), cap);
        if (!separation.IsDone() || separation.Value() <= Precision::Confusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "limiting cap touches the extrusion base");
        }
        auto history = collect_history(splitter, {&prism.Shape(), limit});
        for (auto& entry : history) {
            retained(entry.generated, output);
            retained(entry.modified, output);
            entry.deleted = !output.Contains(entry.source) && entry.modified.empty();
        }
        // Preserve generated source-edge ancestry through native splitting.
        for (auto entry : collect_history(prism, {base})) {
            entry.generated = descendants(entry.generated, splitter, output);
            entry.modified = descendants(entry.modified, splitter, output);
            entry.deleted = !output.Contains(entry.source) && entry.modified.empty();
            history.push_back(std::move(entry));
        }
        const auto status = check_result(session, "limited extrusion", result, &history);
        if (status != OCCT_BRIDGE_OK) {
            return status;
        }
        return store_shape_with_entries(session, result, out_shape, std::move(history));
    });
}

occt_bridge_status_t occt_bridge_shape_ray_first_hit(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t origin, occt_bridge_vec3_t direction, double maximum_length,
    occt_bridge_vec3_t* out_point, double* out_length) {
    return guarded(session, [&] {
        if (out_point == nullptr || out_length == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "ray outputs are null");
        }
        *out_point = {0.0, 0.0, 0.0};
        *out_length = 0.0;
        const double axis_scale = std::max({std::abs(direction.x),std::abs(direction.y),std::abs(direction.z)});
        if (!finite(origin) || !finite(direction) || axis_scale == 0.0
            || !std::isfinite(maximum_length) || maximum_length <= Precision::Confusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "ray needs finite origin, nonzero direction and positive search length");
        }
        const auto* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "ray shape was not found");
        }
        IntCurvesFace_ShapeIntersector intersector;
        intersector.Load(*value, Precision::Confusion());
        intersector.Perform(gp_Lin(to_point(origin), gp_Dir(direction.x/axis_scale,direction.y/axis_scale,direction.z/axis_scale)),
                            2.0 * Precision::Confusion(), maximum_length);
        if (!intersector.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "ray intersection failed");
        }
        if (intersector.NbPnt() == 0) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "ray has no forward surface intersection");
        }
        int closest = 1;
        for (int i = 2; i <= intersector.NbPnt(); ++i) {
            if (intersector.WParameter(i) < intersector.WParameter(closest)) {
                closest = i;
            }
        }
        const auto& point = intersector.Pnt(closest);
        *out_point = {point.X(), point.Y(), point.Z()};
        *out_length = intersector.WParameter(closest);
        return succeed(session);
    });
}

