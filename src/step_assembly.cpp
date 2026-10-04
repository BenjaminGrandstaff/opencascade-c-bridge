/*
 * STEP assembly export through OCCT's XCAF document: one named assembly whose
 * named components place shared, named, colored parts.
 */

#include "bridge_internal.hpp"

#include <IFSelect_ReturnStatus.hxx>
#include <Quantity_Color.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <TCollection_ExtendedString.hxx>
#include <TDF_Label.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <TopLoc_Location.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>

#include <algorithm>
#include <cmath>
#include <iterator>
#include <map>
#include <utility>

using namespace occt_bridge_internal;

namespace {

const char* component_error(const occt_bridge_step_component_t& component) {
    if (component.name == nullptr || component.name[0] == '\0' || component.part_name == nullptr
        || component.part_name[0] == '\0') {
        return "STEP components need nonempty component and part names";
    }
    if (component.has_color != 0 && component.has_color != 1) {
        return "has_color must be 0 or 1";
    }
    const auto outside = [](double channel) { return !std::isfinite(channel) || channel < 0.0 || channel > 1.0; };
    if (component.has_color == 1 && std::any_of(std::begin(component.color), std::end(component.color), outside)) {
        return "STEP colors must be sRGB channels in [0, 1]";
    }
    return nullptr;
}

void set_name(const TDF_Label& label, const char* name) {
    TDataStd_Name::Set(label, TCollection_ExtendedString(name, Standard_True));
}

// Closes the XCAF document however the export ends.
struct DocumentGuard {
    Handle(XCAFApp_Application) application;
    Handle(TDocStd_Document) document;
    ~DocumentGuard() {
        if (!document.IsNull() && document->IsOpened()) {
            application->Close(document);
        }
    }
};

// Placed shapes for every component, or an error status.
occt_bridge_status_t find_components(
    occt_bridge_session_t* session,
    const occt_bridge_step_component_t* components,
    size_t component_count,
    std::vector<const TopoDS_Shape*>& shapes) {
    shapes.reserve(component_count);
    for (size_t index = 0; index < component_count; ++index) {
        if (const char* error = component_error(components[index])) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
        }
        const auto* shape = find_shape(session, components[index].shape);
        if (shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "STEP component shape was not found");
        }
        shapes.push_back(shape);
    }
    return OCCT_BRIDGE_OK;
}

// Fills the document's assembly and returns the number of distinct parts.
size_t build_assembly(
    const Handle(TDocStd_Document)& document,
    const char* assembly_name,
    const occt_bridge_step_component_t* components,
    const std::vector<const TopoDS_Shape*>& shapes) {
    const Handle(XCAFDoc_ShapeTool) shape_tool = XCAFDoc_DocumentTool::ShapeTool(document->Main());
    const Handle(XCAFDoc_ColorTool) color_tool = XCAFDoc_DocumentTool::ColorTool(document->Main());
    const TDF_Label assembly = shape_tool->NewShape();
    set_name(assembly, assembly_name);
    std::map<std::pair<const TopoDS_TShape*, TopAbs_Orientation>, TDF_Label> parts;
    for (size_t index = 0; index < shapes.size(); ++index) {
        const auto& component = components[index];
        const TopoDS_Shape& placed = *shapes[index];
        const TopoDS_Shape local = placed.Located(TopLoc_Location());
        const auto key = std::make_pair(local.TShape().get(), local.Orientation());
        auto found = parts.find(key);
        if (found == parts.end()) {
            const TDF_Label part = shape_tool->AddShape(local, Standard_False);
            set_name(part, component.part_name);
            if (component.has_color == 1) {
                color_tool->SetColor(
                    part,
                    Quantity_Color(component.color[0], component.color[1], component.color[2], Quantity_TOC_sRGB),
                    XCAFDoc_ColorSurf);
            }
            found = parts.emplace(key, part).first;
        }
        set_name(shape_tool->AddComponent(assembly, found->second, placed.Location()), component.name);
    }
    shape_tool->UpdateAssemblies();
    return parts.size();
}

}  // namespace

// O(components) labels plus one STEP transfer; parts are deduplicated by the
// placed shapes' shared underlying geometry and orientation.
occt_bridge_status_t occt_bridge_step_save_assembly(
    occt_bridge_session_t* session,
    const char* path,
    const char* assembly_name,
    const occt_bridge_step_component_t* components,
    size_t component_count,
    size_t* out_part_count) {
    return guarded(session, [&] {
        if (out_part_count != nullptr) {
            *out_part_count = 0;
        }
        if (path == nullptr || path[0] == '\0' || assembly_name == nullptr || assembly_name[0] == '\0') {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "path and assembly name must be nonempty");
        }
        if (components == nullptr || component_count == 0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "a STEP assembly needs at least one component");
        }
        std::vector<const TopoDS_Shape*> shapes;
        if (const auto status = find_components(session, components, component_count, shapes);
            status != OCCT_BRIDGE_OK) {
            return status;
        }
        DocumentGuard guard{XCAFApp_Application::GetApplication(), {}};
        guard.application->NewDocument("MDTV-XCAF", guard.document);
        const size_t parts = build_assembly(guard.document, assembly_name, components, shapes);
        STEPCAFControl_Writer writer;
        writer.SetNameMode(Standard_True);
        writer.SetColorMode(Standard_True);
        if (!writer.Transfer(guard.document, STEPControl_AsIs)) {
            return fail(session, OCCT_BRIDGE_IO_ERROR, "failed to transfer the assembly to STEP");
        }
        if (writer.Write(path) != IFSelect_RetDone) {
            return fail(session, OCCT_BRIDGE_IO_ERROR, "failed to write STEP file");
        }
        if (out_part_count != nullptr) {
            *out_part_count = parts;
        }
        return succeed(session);
    });
}
