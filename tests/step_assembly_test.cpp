/* Round-trips a STEP assembly through OCCT's XCAF reader: structure, shared
 * parts, names, and colors survive. */
#include "occt_bridge.h"

#include <Quantity_Color.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <TCollection_AsciiString.hxx>
#include <TDF_Label.hxx>
#include <TDF_LabelSequence.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Trsf.hxx>

#include <cmath>
#include <cstdio>
#include <set>
#include <string>

namespace {

int failures = 0;

void check(bool condition, const char* what) {
    if (!condition) {
        (void)std::fprintf(stderr, "FAILED: %s\n", what);
        ++failures;
    }
}

std::string name_of(const TDF_Label& label) {
    Handle(TDataStd_Name) name;
    if (!label.FindAttribute(TDataStd_Name::GetID(), name)) {
        return {};
    }
    return TCollection_AsciiString(name->Get()).ToCString();
}

// The single child component of `assembly` named `name`, or a null label.
TDF_Label child(const TDF_Label& assembly, const std::string& name) {
    TDF_LabelSequence components;
    XCAFDoc_ShapeTool::GetComponents(assembly, components);
    for (int index = 1; index <= components.Length(); ++index) {
        if (name_of(components.Value(index)) == name) {
            return components.Value(index);
        }
    }
    return {};
}

// The assembly or part a component refers to.
TDF_Label referred(const TDF_Label& component) {
    TDF_Label target;
    XCAFDoc_ShapeTool::GetReferredShape(component, target);
    return target;
}

bool same_transform(const gp_Trsf& left, const gp_Trsf& right) {
    for (int row = 1; row <= 3; ++row) {
        for (int column = 1; column <= 4; ++column) {
            if (std::abs(left.Value(row, column) - right.Value(row, column)) > 1e-9) {
                return false;
            }
        }
    }
    return true;
}

// Faces of `part` whose surface color has the given sRGB green channel.
int faces_colored(const Handle(TDocStd_Document)& document, const TDF_Label& part, double green) {
    const Handle(XCAFDoc_ColorTool) colors = XCAFDoc_DocumentTool::ColorTool(document->Main());
    int colored = 0;
    for (TopExp_Explorer faces(XCAFDoc_ShapeTool::GetShape(part), TopAbs_FACE); faces.More(); faces.Next()) {
        Quantity_Color color;
        if (colors->GetColor(faces.Current(), XCAFDoc_ColorSurf, color)) {
            double r = 0;
            double g = 0;
            double b = 0;
            color.Values(r, g, b, Quantity_TOC_sRGB);
            colored += std::abs(g - green) < 1e-3 ? 1 : 0;
        }
    }
    return colored;
}

// Reads the tree written by nested() back through XCAF and checks nesting,
// names, recomposed placements, and the pin's face color.
void check_tree(const std::string& file) {
    Handle(XCAFApp_Application) application = XCAFApp_Application::GetApplication();
    Handle(TDocStd_Document) document;
    application->NewDocument("MDTV-XCAF", document);
    STEPCAFControl_Reader reader;
    reader.SetNameMode(Standard_True);
    reader.SetColorMode(Standard_True);
    check(reader.ReadFile(file.c_str()) == IFSelect_RetDone && reader.Transfer(document) == Standard_True, "read tree");
    const Handle(XCAFDoc_ShapeTool) shapes = XCAFDoc_DocumentTool::ShapeTool(document->Main());
    TDF_LabelSequence roots;
    shapes->GetFreeShapes(roots);
    check(roots.Length() == 1 && name_of(roots.Value(1)) == "plane", "tree root");
    if (roots.Length() == 1) {
        const TDF_Label wing = child(roots.Value(1), "wing");
        const TDF_Label outer = child(roots.Value(1), "outer block");
        check(!wing.IsNull() && !outer.IsNull(), "root holds the wing and the outer block");
        const TDF_Label wing_assembly = wing.IsNull() ? TDF_Label() : referred(wing);
        check(!wing_assembly.IsNull() && XCAFDoc_ShapeTool::IsAssembly(wing_assembly), "wing is a sub-assembly");
        const TDF_Label flap = wing_assembly.IsNull() ? TDF_Label() : child(wing_assembly, "flap");
        const TDF_Label inner_block = wing_assembly.IsNull() ? TDF_Label() : child(wing_assembly, "inner block");
        check(!flap.IsNull() && !inner_block.IsNull(), "wing holds the flap and the inner block");
        const TDF_Label flap_assembly = flap.IsNull() ? TDF_Label() : referred(flap);
        const TDF_Label flap_pin = flap_assembly.IsNull() ? TDF_Label() : child(flap_assembly, "flap pin");
        check(!flap_pin.IsNull(), "flap holds the pin");
        if (!inner_block.IsNull() && !flap_pin.IsNull()) {
            const gp_Trsf wing_place = XCAFDoc_ShapeTool::GetLocation(wing).Transformation();
            const gp_Trsf inner_world =
                wing_place.Multiplied(XCAFDoc_ShapeTool::GetLocation(inner_block).Transformation());
            gp_Trsf inner_expected;
            inner_expected.SetTranslation(gp_Vec(120, 0, 0));
            check(same_transform(inner_world, inner_expected), "inner block keeps its model placement");
            const gp_Trsf pin_world = wing_place
                .Multiplied(XCAFDoc_ShapeTool::GetLocation(flap).Transformation())
                .Multiplied(XCAFDoc_ShapeTool::GetLocation(flap_pin).Transformation());
            gp_Trsf pin_expected;
            pin_expected.SetTranslation(gp_Vec(130, 60, 5));
            check(same_transform(pin_world, pin_expected), "flap pin keeps its model placement");
            // Exactly one face of the pin part carries the face color.
            check(faces_colored(document, referred(flap_pin), 0.6) == 1, "one pin face carries the face color");
        }
    }
    application->Close(document);
}

// Writes wing (translated) > flap (rotated a quarter turn, translated) with
// one block at the top level, one in the wing, and the pin in the flap, then
// checks the nesting and that chained locations reproduce each shape's own.
// The top-level block is unplaced and shares its part with the wing's: OCCT
// once named the wrong occurrence in that case.
void nested(occt_bridge_session_t* session, const std::string& file) {
    occt_bridge_shape_id_t block = 0;
    occt_bridge_shape_id_t inner = 0;
    occt_bridge_shape_id_t pin = 0;
    occt_bridge_shape_id_t placed_pin = 0;
    check(occt_bridge_create_box(session, {0, 0, 0}, {10, 20, 30}, &block) == OCCT_BRIDGE_OK, "tree box");
    check(occt_bridge_translate(session, block, {120, 0, 0}, &inner) == OCCT_BRIDGE_OK, "tree inner copy");
    check(occt_bridge_create_cylinder(session, {0, 0, 0}, {0, 0, 1}, 2, 40, &pin) == OCCT_BRIDGE_OK, "tree pin");
    check(occt_bridge_translate(session, pin, {130, 60, 5}, &placed_pin) == OCCT_BRIDGE_OK, "tree pin placed");
    // The flap turns a quarter turn about z, then moves 50 along y.
    const occt_bridge_step_node_t nodes[] = {
        {"wing", OCCT_BRIDGE_STEP_ROOT, {1, 0, 0, 100, 0, 1, 0, 0, 0, 0, 1, 0}},
        {"flap", 0, {0, -1, 0, 0, 1, 0, 0, 50, 0, 0, 1, 0}},
    };
    const occt_bridge_step_component_t components[] = {
        {block, "outer block", "block", 0, {0, 0, 0}},
        {inner, "inner block", "block", 0, {0, 0, 0}},
        {placed_pin, "flap pin", "pin", 0, {0, 0, 0}},
    };
    const size_t members[] = {OCCT_BRIDGE_STEP_ROOT, 0, 1};
    size_t parts = 0;
    // The pin's first face (its side) is colored over its uncolored part.
    const occt_bridge_step_face_color_t face_colors[] = {{2, 0, {0.1, 0.6, 0.3}}};
    check(occt_bridge_step_save_assembly_tree(
              session, file.c_str(), "plane", nodes, 2, components, members, 3, face_colors, 1, &parts)
            == OCCT_BRIDGE_OK && parts == 2, "save assembly tree");

    // Forward parents, unknown or empty nodes, and missing arrays are refused.
    occt_bridge_step_node_t forward[] = {nodes[0], nodes[1]};
    forward[0].parent = 1;
    check(occt_bridge_step_save_assembly_tree(session, file.c_str(), "plane", forward, 2, components, members, 3, nullptr, 0, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "forward parent");
    const size_t unknown_node[] = {OCCT_BRIDGE_STEP_ROOT, 0, 7};
    check(occt_bridge_step_save_assembly_tree(session, file.c_str(), "plane", nodes, 2, components, unknown_node, 3, nullptr, 0, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "unknown node");
    const size_t empty_flap[] = {OCCT_BRIDGE_STEP_ROOT, 0, 0};
    check(occt_bridge_step_save_assembly_tree(session, file.c_str(), "plane", nodes, 2, components, empty_flap, 3, nullptr, 0, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "empty sub-assembly");
    check(occt_bridge_step_save_assembly_tree(session, file.c_str(), "plane", nodes, 2, components, nullptr, 3, nullptr, 0, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "missing memberships");
    occt_bridge_step_node_t skewed[] = {nodes[0], nodes[1]};
    skewed[1].transform[0] = 2.0;
    check(occt_bridge_step_save_assembly_tree(session, file.c_str(), "plane", skewed, 2, components, members, 3, nullptr, 0, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "scaling transform");
    occt_bridge_step_node_t mirrored[] = {nodes[0], nodes[1]};
    mirrored[0].transform[10] = -1.0;
    check(occt_bridge_step_save_assembly_tree(session, file.c_str(), "plane", mirrored, 2, components, members, 3, nullptr, 0, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "mirroring transform");

    const occt_bridge_step_face_color_t no_face[] = {{2, 3, {0.1, 0.6, 0.3}}};
    check(occt_bridge_step_save_assembly_tree(
              session, file.c_str(), "plane", nodes, 2, components, members, 3, no_face, 1, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "face beyond the pin's three");
    const occt_bridge_step_face_color_t no_component[] = {{3, 0, {0.1, 0.6, 0.3}}};
    check(occt_bridge_step_save_assembly_tree(
              session, file.c_str(), "plane", nodes, 2, components, members, 3, no_component, 1, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "unknown component");
    const occt_bridge_step_face_color_t too_bright[] = {{2, 0, {0.1, 1.6, 0.3}}};
    check(occt_bridge_step_save_assembly_tree(
              session, file.c_str(), "plane", nodes, 2, components, members, 3, too_bright, 1, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "face color outside [0, 1]");
    check(occt_bridge_step_save_assembly_tree(
              session, file.c_str(), "plane", nodes, 2, components, members, 3, nullptr, 1, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "missing face colors");

    check_tree(file);
    (void)std::remove(file.c_str());
}

}  // namespace

int run(const std::string& file) {
    occt_bridge_session_t* session = nullptr;
    if (occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION, &session) != OCCT_BRIDGE_OK) {
        return 1;
    }
    occt_bridge_shape_id_t block = 0;
    occt_bridge_shape_id_t left = 0;
    occt_bridge_shape_id_t right = 0;
    occt_bridge_shape_id_t pin = 0;
    check(occt_bridge_create_box(session, {0, 0, 0}, {10, 20, 30}, &block) == OCCT_BRIDGE_OK, "box");
    check(occt_bridge_translate(session, block, {-50, 0, 0}, &left) == OCCT_BRIDGE_OK, "left copy");
    check(occt_bridge_translate(session, block, {50, 0, 0}, &right) == OCCT_BRIDGE_OK, "right copy");
    check(occt_bridge_create_cylinder(session, {0, 0, 0}, {0, 0, 1}, 2, 40, &pin) == OCCT_BRIDGE_OK, "pin");

    const occt_bridge_step_component_t components[] = {
        {left, "left block", "block", 1, {0.8, 0.2, 0.1}},
        {right, "right block", "block", 0, {0, 0, 0}},
        {pin, "center pin", "pin", 1, {0.2, 0.4, 0.9}},
    };
    size_t parts = 0;
    check(occt_bridge_step_save_assembly(session, file.c_str(), "fixture", components, 3, &parts) == OCCT_BRIDGE_OK,
        "save assembly");
    check(parts == 2, "two parts: the copies share the block");

    // Argument errors write nothing and report no parts.
    occt_bridge_step_component_t unnamed = components[0];
    unnamed.name = "";
    check(occt_bridge_step_save_assembly(session, file.c_str(), "fixture", &unnamed, 1, &parts)
            == OCCT_BRIDGE_INVALID_ARGUMENT && parts == 0, "empty component name");
    occt_bridge_step_component_t bright = components[0];
    bright.color[1] = 1.5;
    check(occt_bridge_step_save_assembly(session, file.c_str(), "fixture", &bright, 1, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "color outside [0, 1]");
    check(occt_bridge_step_save_assembly(session, file.c_str(), "", components, 3, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "empty assembly name");
    check(occt_bridge_step_save_assembly(session, file.c_str(), "fixture", components, 0, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "no components");
    occt_bridge_step_component_t missing = components[0];
    missing.shape = 999999;
    check(occt_bridge_step_save_assembly(session, file.c_str(), "fixture", &missing, 1, nullptr)
            == OCCT_BRIDGE_SHAPE_NOT_FOUND, "unknown shape");

    Handle(XCAFApp_Application) application = XCAFApp_Application::GetApplication();
    Handle(TDocStd_Document) document;
    application->NewDocument("MDTV-XCAF", document);
    STEPCAFControl_Reader reader;
    reader.SetNameMode(Standard_True);
    reader.SetColorMode(Standard_True);
    check(reader.ReadFile(file.c_str()) == IFSelect_RetDone, "read STEP");
    check(reader.Transfer(document) == Standard_True, "transfer STEP");
    const Handle(XCAFDoc_ShapeTool) shapes = XCAFDoc_DocumentTool::ShapeTool(document->Main());
    const Handle(XCAFDoc_ColorTool) colors = XCAFDoc_DocumentTool::ColorTool(document->Main());
    TDF_LabelSequence roots;
    shapes->GetFreeShapes(roots);
    check(roots.Length() == 1, "one root");
    if (roots.Length() == 1) {
        const TDF_Label assembly = roots.Value(1);
        check(XCAFDoc_ShapeTool::IsAssembly(assembly), "root is an assembly");
        check(name_of(assembly) == "fixture", "assembly name");
        TDF_LabelSequence placed;
        XCAFDoc_ShapeTool::GetComponents(assembly, placed);
        check(placed.Length() == 3, "three components");
        std::set<std::string> component_names;
        std::set<int> referred;
        for (int index = 1; index <= placed.Length(); ++index) {
            component_names.insert(name_of(placed.Value(index)));
            TDF_Label part;
            if (XCAFDoc_ShapeTool::GetReferredShape(placed.Value(index), part)) {
                referred.insert(part.Tag());
                const std::string part_name = name_of(part);
                check(part_name == "block" || part_name == "pin", "part names");
                Quantity_Color color;
                if (part_name == "block") {
                    check(colors->GetColor(part, XCAFDoc_ColorSurf, color), "block color stored");
                    double r = 0;
                    double g = 0;
                    double b = 0;
                    color.Values(r, g, b, Quantity_TOC_sRGB);
                    check(std::abs(r - 0.8) < 1e-3 && std::abs(g - 0.2) < 1e-3 && std::abs(b - 0.1) < 1e-3,
                        "block color is the first component's");
                }
            }
        }
        check(referred.size() == 2, "components refer to two shared parts");
        check(component_names == std::set<std::string>{"left block", "right block", "center pin"}, "component names");
    }
    application->Close(document);
    (void)std::remove(file.c_str());
    nested(session, file);
    occt_bridge_session_destroy(session);
    if (failures == 0) {
        (void)std::puts("PASS: STEP assembly structure, sub-assemblies, shared parts, names, and colors round-trip");
    }
    return failures == 0 ? 0 : 1;
}

int main(int argc, char** argv) {
    try {
        return run(argc > 1 ? argv[1] : "step-assembly-test.step");
    } catch (...) {
        (void)std::fputs("FAILED: unexpected exception\n", stderr);
        return 1;
    }
}
