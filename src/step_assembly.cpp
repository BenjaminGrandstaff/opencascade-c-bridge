/*
 * STEP assembly export through OCCT's XCAF document: one named assembly,
 * optionally with nested named sub-assemblies, whose named components place
 * shared, named, colored parts.
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
#include <gp_Trsf.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>

#include <algorithm>
#include <cmath>
#include <iterator>
#include <map>
#include <utility>
#include <vector>

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

const char* node_error(const occt_bridge_step_node_t& node, size_t index) {
    if (node.name == nullptr || node.name[0] == '\0') {
        return "STEP sub-assemblies need nonempty names";
    }
    if (node.parent != OCCT_BRIDGE_STEP_ROOT && node.parent >= index) {
        return "a STEP sub-assembly's parent must be the root or an earlier node";
    }
    const auto& m = node.transform;
    if (!std::all_of(std::begin(m), std::end(m), [](double value) { return std::isfinite(value); })) {
        return "STEP sub-assembly transforms must be finite";
    }
    // Rows of R are orthonormal and right-handed.
    constexpr double kTolerance = 1e-9;
    for (size_t row = 0; row < 3; ++row) {
        for (size_t other = 0; other < 3; ++other) {
            const double dot = m[4 * row] * m[4 * other] + m[4 * row + 1] * m[4 * other + 1]
                + m[4 * row + 2] * m[4 * other + 2];
            if (std::abs(dot - (row == other ? 1.0 : 0.0)) > kTolerance) {
                return "STEP sub-assembly transforms must be rigid";
            }
        }
    }
    const double determinant = m[0] * (m[5] * m[10] - m[6] * m[9]) - m[1] * (m[4] * m[10] - m[6] * m[8])
        + m[2] * (m[4] * m[9] - m[5] * m[8]);
    if (determinant <= 0.0) {
        return "STEP sub-assembly transforms must not mirror";
    }
    return nullptr;
}

// A node's placement in its parent.
gp_Trsf local_placement(const occt_bridge_step_node_t& node) {
    const auto& m = node.transform;
    gp_Trsf transform;
    transform.SetValues(m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8], m[9], m[10], m[11]);
    return transform;
}

// Checks nodes and component memberships: every node must hold a component
// directly or through descendants, so no empty sub-assembly is written.
const char* tree_error(
    const occt_bridge_step_node_t* nodes,
    size_t node_count,
    const size_t* component_nodes,
    size_t component_count) {
    if (node_count != 0 && (nodes == nullptr || component_nodes == nullptr)) {
        return "STEP sub-assemblies need node and membership arrays";
    }
    std::vector<size_t> members(node_count, 0);
    for (size_t index = 0; index < node_count; ++index) {
        if (const char* error = node_error(nodes[index], index)) {
            return error;
        }
    }
    for (size_t index = 0; component_nodes != nullptr && index < component_count; ++index) {
        const size_t node = component_nodes[index];
        if (node == OCCT_BRIDGE_STEP_ROOT) {
            continue;
        }
        if (node >= node_count) {
            return "a STEP component names an unknown sub-assembly";
        }
        ++members[node];
    }
    // Parents precede children, so one reverse pass accumulates subtrees.
    for (size_t index = node_count; index-- > 0;) {
        if (members[index] == 0) {
            return "every STEP sub-assembly must contain a component";
        }
        if (nodes[index].parent != OCCT_BRIDGE_STEP_ROOT) {
            members[nodes[index].parent] += members[index];
        }
    }
    return nullptr;
}

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

// Fills the document's assembly, sub-assemblies, and components, and returns
// the number of distinct parts.
size_t build_assembly(
    const Handle(TDocStd_Document)& document,
    const char* assembly_name,
    const occt_bridge_step_node_t* nodes,
    size_t node_count,
    const occt_bridge_step_component_t* components,
    const size_t* component_nodes,
    const std::vector<const TopoDS_Shape*>& shapes) {
    const Handle(XCAFDoc_ShapeTool) shape_tool = XCAFDoc_DocumentTool::ShapeTool(document->Main());
    const Handle(XCAFDoc_ColorTool) color_tool = XCAFDoc_DocumentTool::ColorTool(document->Main());
    const TDF_Label assembly = shape_tool->NewShape();
    set_name(assembly, assembly_name);
    std::vector<TDF_Label> node_labels(node_count);
    std::vector<gp_Trsf> node_world(node_count);
    for (size_t index = 0; index < node_count; ++index) {
        const auto& node = nodes[index];
        const gp_Trsf local = local_placement(node);
        const bool top = node.parent == OCCT_BRIDGE_STEP_ROOT;
        node_world[index] = top ? local : node_world[node.parent].Multiplied(local);
        node_labels[index] = shape_tool->NewShape();
        set_name(node_labels[index], node.name);
        set_name(
            shape_tool->AddComponent(top ? assembly : node_labels[node.parent], node_labels[index], TopLoc_Location(local)),
            node.name);
    }
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
        const size_t node = component_nodes == nullptr ? OCCT_BRIDGE_STEP_ROOT : component_nodes[index];
        TDF_Label parent = assembly;
        TopLoc_Location location = placed.Location();
        if (node != OCCT_BRIDGE_STEP_ROOT) {
            parent = node_labels[node];
            location = TopLoc_Location(node_world[node]).Inverted().Multiplied(location);
        }
        // OCCT's name writer confuses an unlocated occurrence of a shared
        // part with the part itself and names the wrong occurrence; an
        // explicit identity transform keeps every occurrence distinct.
        if (location.IsIdentity()) {
            location = TopLoc_Location(gp_Trsf());
        }
        set_name(shape_tool->AddComponent(parent, found->second, location), component.name);
    }
    shape_tool->UpdateAssemblies();
    return parts.size();
}

occt_bridge_status_t save_tree(
    occt_bridge_session_t* session,
    const char* path,
    const char* assembly_name,
    const occt_bridge_step_node_t* nodes,
    size_t node_count,
    const occt_bridge_step_component_t* components,
    const size_t* component_nodes,
    size_t component_count,
    size_t* out_part_count) {
    if (out_part_count != nullptr) {
        *out_part_count = 0;
    }
    if (path == nullptr || path[0] == '\0' || assembly_name == nullptr || assembly_name[0] == '\0') {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "path and assembly name must be nonempty");
    }
    if (components == nullptr || component_count == 0) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "a STEP assembly needs at least one component");
    }
    if (const char* error = tree_error(nodes, node_count, component_nodes, component_count)) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
    }
    std::vector<const TopoDS_Shape*> shapes;
    if (const auto status = find_components(session, components, component_count, shapes);
        status != OCCT_BRIDGE_OK) {
        return status;
    }
    DocumentGuard guard{XCAFApp_Application::GetApplication(), {}};
    guard.application->NewDocument("MDTV-XCAF", guard.document);
    const size_t parts = build_assembly(
        guard.document, assembly_name, nodes, node_count, components, component_nodes, shapes);
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
}

}  // namespace

// O(nodes + components) labels plus one STEP transfer; parts are deduplicated
// by the placed shapes' shared underlying geometry and orientation.
occt_bridge_status_t occt_bridge_step_save_assembly(
    occt_bridge_session_t* session,
    const char* path,
    const char* assembly_name,
    const occt_bridge_step_component_t* components,
    size_t component_count,
    size_t* out_part_count) {
    return guarded(session, [&] {
        return save_tree(session, path, assembly_name, nullptr, 0, components, nullptr, component_count, out_part_count);
    });
}

occt_bridge_status_t occt_bridge_step_save_assembly_tree(
    occt_bridge_session_t* session,
    const char* path,
    const char* assembly_name,
    const occt_bridge_step_node_t* nodes,
    size_t node_count,
    const occt_bridge_step_component_t* components,
    const size_t* component_nodes,
    size_t component_count,
    size_t* out_part_count) {
    return guarded(session, [&] {
        return save_tree(
            session, path, assembly_name, nodes, node_count, components, component_nodes, component_count,
            out_part_count);
    });
}
