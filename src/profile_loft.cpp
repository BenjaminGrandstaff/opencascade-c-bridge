/* Native lofts between immutable, closed planar profile wires. */
#include "bridge_internal.hpp"

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRep_Tool.hxx>
#include <TopTools_MapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Wire.hxx>
#include <TopoDS_Face.hxx>

using namespace occt_bridge_internal;

namespace {
occt_bridge_status_t validate_profile(
    occt_bridge_session_t* session, const TopoDS_Shape& value,
    TopTools_MapOfShape& seen) {
    if (value.ShapeType() != TopAbs_WIRE || !BRep_Tool::IsClosed(value)
        || !ShapeValidator(value).IsValid() || !seen.Add(value)) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "loft sections must be distinct valid closed wires");
    }
    BRepBuilderAPI_MakeFace face(TopoDS::Wire(value), Standard_True);
    if (!face.IsDone() || !ShapeValidator(face.Face()).IsValid()
        || BRepAdaptor_Surface(face.Face()).GetType() != GeomAbs_Plane) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "loft section must define one valid planar region");
    }
    return OCCT_BRIDGE_OK;
}
}

occt_bridge_status_t occt_bridge_create_loft_from_wires(
    occt_bridge_session_t* session, const occt_bridge_shape_id_t* sections,
    size_t section_count, int make_solid, int ruled,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (sections == nullptr || section_count < 2 || section_count > 1000
            || (make_solid != 0 && make_solid != 1) || (ruled != 0 && ruled != 1)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "wire loft needs 2-1000 sections and valid flags");
        }
        BRepOffsetAPI_ThruSections loft(make_solid != 0, ruled != 0);
        // OCCT compatibility may align/split wires. It must do so on copies.
        loft.SetMutableInput(Standard_False);
        loft.CheckCompatibility(Standard_True);
        TopTools_MapOfShape seen;
        std::vector<const TopoDS_Shape*> roots;
        roots.reserve(section_count);
        for (size_t i = 0; i < section_count; ++i) {
            const auto* value = find_shape(session, sections[i]);
            if (value == nullptr) {
                return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "loft section was not found");
            }
            const auto status = validate_profile(session, *value, seen);
            if (status != OCCT_BRIDGE_OK) {
                return status;
            }
            loft.AddWire(TopoDS::Wire(*value));
            roots.push_back(value);
        }
        // Input/topology indexing is linear; native compatibility and surface
        // fitting depend on section count, edge correspondence and curve degree.
        loft.Build();
        if (!loft.IsDone() || loft.Shape().IsNull()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "profile wire loft failed");
        }
        if ((make_solid != 0 && loft.Shape().ShapeType() != TopAbs_SOLID)
            || !ShapeValidator(loft.Shape()).IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "profile loft did not produce valid requested topology");
        }
        return store_checked_result(session, "profile loft", loft.Shape(), out_shape, loft, roots);
    });
}
