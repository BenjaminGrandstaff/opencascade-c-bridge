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
    occt_bridge_session_destroy(session);
    if (failures == 0) {
        (void)std::puts("PASS: STEP assembly structure, shared parts, names, and colors round-trip");
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
