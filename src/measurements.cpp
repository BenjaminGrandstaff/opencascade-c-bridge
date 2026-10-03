/* Solid inertial properties and minimum distance between exact BREP shapes. */
#include "bridge_internal.hpp"

#include <BRepBndLib.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRep_Builder.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <gp_Mat.hxx>
#include <gp_Trsf.hxx>
#include <TopoDS_Compound.hxx>

#include <cmath>

using namespace occt_bridge_internal;

// Bounds and validation traverse topology; adaptive surface integration cost
// depends on face geometry. A shared, rebased shape in an identity-location
// compound prevents cancellation in the global property accumulator. O(1)
// wrapper/placement storage; no copied geometry or persistent handles.
occt_bridge_status_t occt_bridge_shape_mass_properties(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_mass_properties_t* out_properties) {
    return guarded(session, [&] {
        if (out_properties == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_properties is null");
        }
        *out_properties = {};
        const auto* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        if (!contains_topology(*value, TopAbs_SOLID) || !ShapeValidator(*value).IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "mass properties require valid solid geometry");
        }
        Bnd_Box bounds;
        BRepBndLib::AddOptimal(*value, bounds, Standard_False, Standard_False);
        const auto minimum = bounds.CornerMin();
        const auto maximum = bounds.CornerMax();
        // Keep the integration reference near the body to avoid subtracting
        // huge world-coordinate moments when obtaining central inertia.
        const gp_Pnt reference(minimum.X() + 0.5 * (maximum.X() - minimum.X()),
            minimum.Y() + 0.5 * (maximum.Y() - minimum.Y()),
            minimum.Z() + 0.5 * (maximum.Z() - minimum.Z()));
        gp_Trsf rebase;
        rebase.SetTranslation(gp_Vec(-reference.X(), -reference.Y(), -reference.Z()));
        // BRepGProp resets its accumulator to the root shape's location.
        // Give it an identity root, keeping both geometry and moments local.
        TopoDS_Compound local;
        BRep_Builder builder;
        builder.MakeCompound(local);
        builder.Add(local, value->Moved(TopLoc_Location(rebase)));
        GProp_GProps properties;
        const double error = BRepGProp::VolumeProperties(local, properties, 1e-9, Standard_True);
        const double volume = properties.Mass();
        if (!std::isfinite(volume) || volume <= 0.0 || !std::isfinite(error)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "solid has no finite positive volume");
        }
        const auto center = properties.CentreOfMass();
        const auto inertia = properties.MatrixOfInertia();
        out_properties->volume = volume;
        out_properties->center = {center.X() + reference.X(), center.Y() + reference.Y(), center.Z() + reference.Z()};
        out_properties->relative_volume_error = error;
        for (int row = 1; row <= 3; ++row) {
            for (int column = 1; column <= 3; ++column) {
                out_properties->inertia[(row - 1) * 3 + column - 1] = inertia.Value(row, column);
            }
        }
        return succeed(session);
    });
}

// OCCT indexes geometric subshapes and solves extrema on candidate pairs.
// Worst-case work depends on pairs of input subshapes, not graph size.
occt_bridge_status_t occt_bridge_shape_distance(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t first,
    occt_bridge_shape_id_t second,
    occt_bridge_distance_result_t* out_distance) {
    return guarded(session, [&] {
        if (out_distance == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_distance is null");
        }
        *out_distance = {};
        const auto* a = find_shape(session, first);
        const auto* b = find_shape(session, second);
        if (a == nullptr || b == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "distance input was not found");
        }
        BRepExtrema_DistShapeShape distance(*a, *b);
        if (!distance.IsDone() || distance.NbSolution() == 0) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shapes have no measurable minimum distance");
        }
        const auto& on_first = distance.PointOnShape1(1);
        const auto& on_second = distance.PointOnShape2(1);
        out_distance->distance = distance.Value();
        out_distance->first = {on_first.X(), on_first.Y(), on_first.Z()};
        out_distance->second = {on_second.X(), on_second.Y(), on_second.Z()};
        return succeed(session);
    });
}

// Non-destructive boolean intersection; topology/geometry-dependent kernel
// work, no persistent result handles or changes to reusable input geometry.
occt_bridge_status_t occt_bridge_shape_overlap_volume(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t first,
    occt_bridge_shape_id_t second,
    double* out_volume) {
    return guarded(session, [&] {
        if (out_volume == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_volume is null");
        }
        *out_volume = 0.0;
        const auto* a = find_shape(session, first);
        const auto* b = find_shape(session, second);
        if (a == nullptr || b == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "overlap input was not found");
        }
        if (!contains_topology(*a, TopAbs_SOLID) || !contains_topology(*b, TopAbs_SOLID)
            || !ShapeValidator(*a).IsValid() || !ShapeValidator(*b).IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "overlap requires two valid solid shapes");
        }
        TopTools_ListOfShape arguments, tools;
        arguments.Append(*a);
        tools.Append(*b);
        BRepAlgoAPI_Common common;
        common.SetArguments(arguments);
        common.SetTools(tools);
        common.SetNonDestructive(Standard_True);
        common.Build();
        if (!common.IsDone() || common.HasErrors()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "overlap intersection failed");
        }
        if (contains_topology(common.Shape(), TopAbs_SOLID)) {
            GProp_GProps properties;
            BRepGProp::VolumeProperties(common.Shape(), properties, 1e-9, Standard_True);
            *out_volume = std::abs(properties.Mass());
        }
        return succeed(session);
    });
}
