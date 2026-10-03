/* Bounded, uniform first-contact closure for straight open rib profiles. */
#include "bridge_internal.hpp"

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax3.hxx>
#include <gp_Trsf.hxx>

#include <algorithm>
#include <cmath>
#include <limits>

using namespace occt_bridge_internal;

namespace {

// Rotate into a frame at the source endpoint: Z is advance distance. Geometry
// bounds, not tessellation or sampled rays, find the earliest contact.
Bnd_Box advance_bounds(const TopoDS_Shape& shape, const gp_Trsf& frame) {
    BRepBuilderAPI_Transform placed(shape, frame);
    Bnd_Box bounds;
    BRepBndLib::AddOptimal(placed.Shape(), bounds, Standard_False, Standard_False);
    return bounds;
}

double edge_length(const TopoDS_Shape& shape) {
    GProp_GProps properties;
    BRepGProp::LinearProperties(shape, properties);
    return properties.Mass();
}

template <typename Operation>
void support_boolean(Operation& operation, const TopoDS_Shape& first, const TopoDS_Shape& second) {
    TopTools_ListOfShape arguments, tools;
    arguments.Append(first);
    tools.Append(second);
    operation.SetArguments(arguments);
    operation.SetTools(tools);
    // Search must not accumulate pcurves or tolerance changes on reusable inputs.
    operation.SetNonDestructive(Standard_True);
    operation.Build();
}

occt_bridge_status_t first_contact(
    occt_bridge_session_t* session,
    const TopoDS_Shape& body,
    const TopoDS_Shape& wire,
    const gp_Dir& direction,
    const TopoDS_Face& sweep,
    double maximum_length,
    double& distance) {
    TopoDS_Vertex start, end;
    TopExp::Vertices(TopoDS::Wire(wire), start, end);
    gp_Trsf frame;
    frame.SetTransformation(gp_Ax3(BRep_Tool::Pnt(start), direction));
    const auto source_bounds = advance_bounds(wire, frame);
    const double tolerance = std::max(Precision::Confusion(),
        64.0 * std::numeric_limits<double>::epsilon() * maximum_length);
    if (source_bounds.CornerMax().Z() - source_bounds.CornerMin().Z() > tolerance) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY,
            "extend-to-next requires a straight profile perpendicular to its advance direction");
    }
    BRepAlgoAPI_Common contact;
    support_boolean(contact, sweep, body);
    if (!contact.IsDone() || contact.HasErrors()) {
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "rib support intersection failed");
    }
    BRepAlgoAPI_Section boundary(sweep, body, Standard_False);
    boundary.SetNonDestructive(Standard_True);
    boundary.Build();
    if (!boundary.IsDone() || boundary.HasErrors()) {
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "rib support boundary intersection failed");
    }
    auto bounds = advance_bounds(contact.Shape(), frame);
    bounds.Add(advance_bounds(boundary.Shape(), frame));
    if (bounds.IsVoid()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "no rib support within maximum length");
    }
    distance = bounds.CornerMin().Z();
    if (distance <= tolerance || distance > maximum_length + tolerance) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY,
            "rib profile must start clear of the body and reach a positive-distance support");
    }
    gp_Trsf translation;
    translation.SetTranslation(gp_Vec(direction) * distance);
    const auto translated = wire.Moved(TopLoc_Location(translation));
    BRepAlgoAPI_Cut unsupported;
    support_boolean(unsupported, translated, body);
    if (!unsupported.IsDone() || unsupported.HasErrors()) {
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "rib support coverage check failed");
    }
    if (edge_length(unsupported.Shape()) > tolerance) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY,
            "first rib contact must support the entire translated profile");
    }
    return OCCT_BRIDGE_OK;
}

bool one_valid_solid(const TopoDS_Shape& body) {
    int solids = 0;
    for (TopExp_Explorer explorer(body, TopAbs_SOLID); explorer.More(); explorer.Next()) {
        ++solids;
    }
    return solids == 1 && BRepCheck_Analyzer(body).IsValid();
}

} // namespace

// A fixed number of kernel intersections/constructions, bounded by source/body
// topology. No iterative distance search, mesh sampling, or unbounded sweep.
occt_bridge_status_t occt_bridge_create_open_profile_face_to_next(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t wire,
    occt_bridge_shape_id_t body,
    occt_bridge_vec3_t direction,
    double maximum_length,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const double magnitude = std::max({std::abs(direction.x), std::abs(direction.y), std::abs(direction.z)});
        if (!finite(direction) || magnitude == 0.0
            || !occt_bridge_internal::finite(maximum_length) || maximum_length <= Precision::Confusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT,
                "rib advance must be finite/nonzero and maximum length finite/positive");
        }
        const auto* source = find_shape(session, wire);
        const auto* support = find_shape(session, body);
        if (source == nullptr || support == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "rib profile or body was not found");
        }
        if (source->ShapeType() != TopAbs_WIRE || !one_valid_solid(*support)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY,
                "extend-to-next requires an open wire and one valid body solid");
        }
        // Validate the open chain before first_contact examines its endpoint.
        TopoDS_Face face;
        std::vector<occt_bridge_history_entry> entries;
        const gp_Dir axis(direction.x / magnitude, direction.y / magnitude, direction.z / magnitude);
        auto status = build_open_profile_face(session, *source, gp_Vec(axis) * maximum_length, face, entries);
        if (status != OCCT_BRIDGE_OK) {
            return status;
        }
        double distance = 0.0;
        status = first_contact(session, *support, *source, axis, face, maximum_length, distance);
        if (status != OCCT_BRIDGE_OK) {
            return status;
        }
        status = build_open_profile_face(session, *source, gp_Vec(axis) * distance, face, entries);
        if (status != OCCT_BRIDGE_OK) {
            return status;
        }
        return store_shape_with_entries(session, face, out_shape, std::move(entries));
    });
}
