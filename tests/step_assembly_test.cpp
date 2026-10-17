/* Round-trips a STEP assembly through OCCT's XCAF reader: structure, shared
 * parts, names, colors, and semantic AP242 GD&T survive. */
#include "occt_bridge.h"

#include <Quantity_Color.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <TCollection_AsciiString.hxx>
#include <XCAFDoc_View.hxx>
#include <XCAFView_Object.hxx>
#include <TDF_Label.hxx>
#include <TDF_LabelSequence.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <XCAFDoc_ClippingPlaneTool.hxx>
#include <XCAFDoc_ViewTool.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Trsf.hxx>

#include <cmath>
#include <cstdio>
#include <fstream>
#include <iterator>
#include <set>
#include <string>
#include <vector>

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

// Indices of a plate's faces: bottom, top, left (x = 0), right, front (y = 0)
// and its holes, from their pull ranges and radii.
struct PlateFaces {
    size_t bottom = SIZE_MAX;
    size_t top = SIZE_MAX;
    size_t left = SIZE_MAX;
    size_t right = SIZE_MAX;
    size_t front = SIZE_MAX;
    std::vector<size_t> holes;
};

PlateFaces plate_faces(occt_bridge_session_t* session, occt_bridge_shape_id_t plate) {
    size_t count = 0;
    occt_bridge_shape_face_radius_bounds(session, plate, 2, nullptr, 0, &count);
    std::vector<occt_bridge_face_radius_bounds_t> radii(count);
    std::vector<occt_bridge_face_pull_range_t> x(count);
    std::vector<occt_bridge_face_pull_range_t> y(count);
    std::vector<occt_bridge_face_pull_range_t> z(count);
    check(occt_bridge_shape_face_radius_bounds(session, plate, 2, radii.data(), count, &count) == OCCT_BRIDGE_OK
            && occt_bridge_shape_face_pull_ranges(session, plate, {1, 0, 0}, x.data(), count, &count) == OCCT_BRIDGE_OK
            && occt_bridge_shape_face_pull_ranges(session, plate, {0, 1, 0}, y.data(), count, &count) == OCCT_BRIDGE_OK
            && occt_bridge_shape_face_pull_ranges(session, plate, {0, 0, 1}, z.data(), count, &count) == OCCT_BRIDGE_OK,
        "plate face analysis");
    PlateFaces faces;
    const auto at = [](const occt_bridge_face_pull_range_t& range, double value) {
        return std::abs(range.minimum - value) < 1e-9 && std::abs(range.maximum - value) < 1e-9;
    };
    for (size_t index = 0; index < count; ++index) {
        if (std::isfinite(radii[index].concave_radius)) {
            faces.holes.push_back(index);
        } else if (at(z[index], -1)) {
            faces.bottom = index;
        } else if (at(z[index], 1)) {
            faces.top = index;
        } else if (at(x[index], -1)) {
            faces.left = index;
        } else if (at(x[index], 1)) {
            faces.right = index;
        } else if (at(y[index], -1)) {
            faces.front = index;
        }
    }
    return faces;
}

// A plate with two holes and a pin with a datum of its own, written with
// datums, tolerances and sizes that all read back.
void pmi(occt_bridge_session_t* session, const std::string& file) {
    occt_bridge_shape_id_t box = 0;
    occt_bridge_shape_id_t first_hole = 0;
    occt_bridge_shape_id_t second_hole = 0;
    occt_bridge_shape_id_t drilled = 0;
    occt_bridge_shape_id_t plate = 0;
    occt_bridge_shape_id_t pin = 0;
    check(occt_bridge_create_box(session, {0, 0, 0}, {100, 50, 20}, &box) == OCCT_BRIDGE_OK
            && occt_bridge_create_cylinder(session, {30, 25, -1}, {0, 0, 1}, 5, 22, &first_hole) == OCCT_BRIDGE_OK
            && occt_bridge_create_cylinder(session, {70, 25, -1}, {0, 0, 1}, 5, 22, &second_hole) == OCCT_BRIDGE_OK
            && occt_bridge_cut(session, box, first_hole, &drilled) == OCCT_BRIDGE_OK
            && occt_bridge_cut(session, drilled, second_hole, &plate) == OCCT_BRIDGE_OK
            && occt_bridge_create_cylinder(session, {0, 0, 40}, {0, 0, 1}, 4, 30, &pin) == OCCT_BRIDGE_OK,
        "PMI shapes");
    const PlateFaces faces = plate_faces(session, plate);
    check(faces.bottom != SIZE_MAX && faces.top != SIZE_MAX && faces.left != SIZE_MAX && faces.right != SIZE_MAX
            && faces.front != SIZE_MAX && faces.holes.size() == 2,
        "plate faces found");
    if (failures != 0) {
        return;
    }
    const occt_bridge_step_component_t components[] = {
        {plate, "plate", "plate", 0, {0, 0, 0}},
        {pin, "pin", "pin", 0, {0, 0, 0}},
    };
    const size_t bottom[] = {faces.bottom};
    const size_t top[] = {faces.top};
    const size_t left[] = {faces.left};
    const size_t front[] = {faces.front};
    const size_t sides[] = {faces.left, faces.right};
    const size_t pin_side[] = {0};
    // Presentations in the plane above the plate, facing up: a box with a
    // diagonal, and a three-point leader.
    const double box_points[] = {0, 0, 30, 20, 0, 30, 20, 8, 30, 0, 8, 30, 0, 0, 30, 0, 0, 30, 20, 8, 30};
    const uint32_t box_polylines[] = {5, 2};
    const double leader_points[] = {40, 10, 30, 45, 15, 30, 60, 15, 30};
    const uint32_t leader_polylines[] = {3};
    const auto drawn = [&](const char* name, bool box, double x) {
        occt_bridge_step_presentation_t presentation{
            name, box ? box_points : leader_points, box ? 7u : 3u, box ? box_polylines : leader_polylines,
            box ? 2u : 1u, {x, 0, 30}, {0, 0, 1}, {1, 0, 0}};
        return presentation;
    };
    const occt_bridge_step_presentation_t datum_a = drawn("datum A", true, 0);
    const occt_bridge_step_presentation_t datum_e = drawn("datum E", false, 40);
    const occt_bridge_step_presentation_t flatness = drawn("flatness", false, 40);
    const occt_bridge_step_presentation_t holes = drawn("hole position", true, 0);
    const occt_bridge_step_datum_t datums[] = {
        {"A", {0, bottom, 1}, &datum_a},
        {"B", {0, left, 1}, nullptr},
        {"C", {0, front, 1}, nullptr},
        {"D", {1, pin_side, 1}, nullptr},
        // No tolerance references E; it still reads back.
        {"E", {0, top, 1}, &datum_e},
    };
    const occt_bridge_step_datum_reference_t abc[] = {
        {0, 0, OCCT_BRIDGE_DATUM_RFS, 0},
        {1, 1, OCCT_BRIDGE_DATUM_MMB, 0},
        {2, 2, OCCT_BRIDGE_DATUM_RFS, 1},
    };
    const occt_bridge_step_datum_reference_t common[] = {
        {1, 0, OCCT_BRIDGE_DATUM_RFS, 0},
        {2, 0, OCCT_BRIDGE_DATUM_RFS, 0},
    };
    const occt_bridge_step_datum_reference_t pin_axis[] = {{3, 0, OCCT_BRIDGE_DATUM_RFS, 0}};
    occt_bridge_step_tolerance_t tolerances[] = {
        {{0, top, 1}, OCCT_BRIDGE_GDT_FLATNESS, 0.05, 0, OCCT_BRIDGE_MATERIAL_RFS, OCCT_BRIDGE_GDT_FREE_STATE, 0, nullptr, 0,
            &flatness},
        {{0, faces.holes.data(), 2}, OCCT_BRIDGE_GDT_POSITION, 0.2, 1, OCCT_BRIDGE_MATERIAL_MMC, 0, 0, abc, 3, &holes},
        {{0, faces.holes.data(), 2}, OCCT_BRIDGE_GDT_POSITION, 0.4, 1, OCCT_BRIDGE_MATERIAL_RFS, 0, 15, abc, 1, nullptr},
        {{0, top, 1}, OCCT_BRIDGE_GDT_PERPENDICULARITY, 0.07, 0, OCCT_BRIDGE_MATERIAL_RFS,
            OCCT_BRIDGE_GDT_TANGENT_PLANE | OCCT_BRIDGE_GDT_STATISTICAL, 0, common, 2, nullptr},
        {{1, pin_side, 1}, OCCT_BRIDGE_GDT_TOTAL_RUNOUT, 0.03, 0, OCCT_BRIDGE_MATERIAL_RFS, 0, 0, pin_axis, 1, nullptr},
    };
    occt_bridge_step_size_t sizes[] = {
        {{0, faces.holes.data(), 2}, OCCT_BRIDGE_STEP_SIZE_DIAMETER, 9.9, 10.2},
        {{0, sides, 2}, OCCT_BRIDGE_STEP_SIZE_WIDTH, 99.9, 100.1},
    };
    uint32_t datum_losses[5] = {9, 9, 9, 9, 9};
    uint32_t tolerance_losses[5] = {9, 9, 9, 9, 9};
    uint32_t size_losses[2] = {9, 9};
    // Two saved views: looking down on both datums and both drawn
    // tolerances, and from the front on the hole position alone.
    const size_t top_datums[] = {0, 4};
    const size_t top_tolerances[] = {0, 1};
    const size_t front_tolerances[] = {1};
    uint8_t top_datums_lost[2] = {9, 9};
    uint8_t top_tolerances_lost[2] = {9, 9};
    uint8_t front_tolerances_lost[1] = {9};
    const occt_bridge_step_clipping_plane_t section[] = {{{30, 25, 0}, {0, 1, 0}}, {{0, 0, 10}, {0, 0, -1}}};
    // The top view shows the plate alone; the front view both parts.
    const size_t plate_only[] = {0};
    const size_t both_parts[] = {0, 1};
    const occt_bridge_step_view_t views[] = {
        {"TOP", {0, 0, -1}, {0, 2, 0}, top_datums, 2, top_tolerances, 2, top_datums_lost, top_tolerances_lost,
            nullptr, 0, plate_only, 1},
        // Up need not be square to the direction. A section through the
        // first hole, clipped to the plate's lower half.
        {"FRONT", {0, 1, 0}, {0, 0.5, 1}, nullptr, 0, front_tolerances, 1, nullptr, front_tolerances_lost, section, 2,
            both_parts, 2},
    };
    uint32_t view_losses[2] = {9, 9};
    // Released at revision B with two approvals; the model is the authority.
    const occt_bridge_step_approval_t approvals[] = {
        {"CHECKED", "R. Lee", "2026-10-01"},
        {"APPROVED", "M. Ortiz", "2026-10-02"},
    };
    uint32_t dataset_lost = 99;
    const occt_bridge_step_dataset_t dataset{
        "PLT-1001", "Example Works", "authority: model", "B", "Hole positions tightened", approvals, 2,
        &dataset_lost};
    occt_bridge_step_pmi_t pmi{
        datums, 5, tolerances, 5, sizes, 2, datum_losses, tolerance_losses, size_losses, views, 2, view_losses, &dataset};
    size_t parts = 0;
    check(occt_bridge_step_save_assembly_pmi(
              session, file.c_str(), "fixture", nullptr, 0, components, nullptr, 2, nullptr, 0, &pmi, &parts)
            == OCCT_BRIDGE_OK && parts == 2,
        "save assembly PMI");
    for (const uint32_t loss : datum_losses) {
        check(loss == 0, "every datum reads back on its faces");
    }
    check(tolerance_losses[0] == 0 && tolerance_losses[1] == 0 && tolerance_losses[3] == 0
            && tolerance_losses[4] == 0, "tolerances read back");
    check(tolerance_losses[2] == 0, "the projected zone reads back");
    {
        // One projected zone, and none of the runout zones OCCT's writer
        // gives every diameter zone.
        std::ifstream stream(file);
        const std::string text((std::istreambuf_iterator<char>(stream)), std::istreambuf_iterator<char>());
        size_t projected = 0;
        for (size_t at = text.find("PROJECTED_ZONE_DEFINITION("); at != std::string::npos;
             at = text.find("PROJECTED_ZONE_DEFINITION(", at + 1)) {
            ++projected;
        }
        check(projected == 1 && text.find("RUNOUT_ZONE_DEFINITION(") == std::string::npos,
            "one projected zone and no runout zones written");
    }
    check(size_losses[0] == 0 && size_losses[1] == 0, "sizes read back");
    check(view_losses[0] == 0 && view_losses[1] == 0, "views read back with their cameras");
    check(dataset_lost == 0, "the dataset's identification and approvals read back");
    {
        std::ifstream stream(file);
        const std::string text((std::istreambuf_iterator<char>(stream)), std::istreambuf_iterator<char>());
        check(text.find("PRODUCT('PLT-1001'") != std::string::npos
                && text.find("PRODUCT_DEFINITION_FORMATION('B','Hole positions tightened'") != std::string::npos
                && text.find("APPROVAL_ROLE('CHECKED')") != std::string::npos,
            "the root product carries the dataset");
    }
    check(top_datums_lost[0] == 0 && top_datums_lost[1] == 0 && top_tolerances_lost[0] == 0
            && top_tolerances_lost[1] == 0 && front_tolerances_lost[0] == 0,
        "views show their annotations");
    {
        Handle(XCAFApp_Application) application = XCAFApp_Application::GetApplication();
        Handle(TDocStd_Document) document;
        application->NewDocument("MDTV-XCAF", document);
        STEPCAFControl_Reader reader;
        reader.SetGDTMode(Standard_True);
        reader.SetViewMode(Standard_True);
        check(reader.ReadFile(file.c_str()) == IFSelect_RetDone && reader.Transfer(document), "read views");
        TDF_LabelSequence read_views;
        XCAFDoc_DocumentTool::ViewTool(document->Main())->GetViewLabels(read_views);
        check(read_views.Length() == 2, "two saved views");
        TDF_LabelSequence planes;
        XCAFDoc_DocumentTool::ClippingPlaneTool(document->Main())->GetClippingPlanes(planes);
        check(planes.Length() == 2, "the section's two clipping planes");
        size_t shown[2] = {0, 0};
        for (const TDF_Label& view : read_views) {
            Handle(XCAFDoc_View) attribute;
            TDF_LabelSequence parts;
            XCAFDoc_DocumentTool::ViewTool(document->Main())->GetRefShapeLabel(view, parts);
            if (view.FindAttribute(XCAFDoc_View::GetID(), attribute)) {
                const bool top = TCollection_AsciiString(attribute->GetObject()->Name()->String()) == "TOP";
                shown[top ? 0 : 1] = static_cast<size_t>(parts.Length());
            }
        }
        check(shown[0] == 1 && shown[1] == 2, "each view shows its parts");
        application->Close(document);
    }

    // Tolerances without size dimensions keep their millimetre values,
    // which OCCT's writer alone would write in metres.
    const occt_bridge_step_view_t flat_view{
        "TOP", {0, 0, -1}, {0, 1, 0}, nullptr, 0, top_tolerances, 1, nullptr, nullptr, nullptr, 0, nullptr, 0};
    uint32_t alone_losses[1] = {9};
    uint32_t alone_view[1] = {9};
    occt_bridge_step_pmi_t alone{
        nullptr, 0, tolerances, 1, nullptr, 0, nullptr, alone_losses, nullptr, &flat_view, 1, alone_view, nullptr};
    check(occt_bridge_step_save_assembly_pmi(
              session, file.c_str(), "fixture", nullptr, 0, components, nullptr, 2, nullptr, 0, &alone, nullptr)
            == OCCT_BRIDGE_OK && alone_losses[0] == 0 && alone_view[0] == 0,
        "a tolerance alone reads back with its value and view");
    occt_bridge_step_tolerance_t undrawn = tolerances[3];
    uint32_t undrawn_losses[1] = {9};
    occt_bridge_step_pmi_t plain{
        datums, 3, &undrawn, 1, nullptr, 0, nullptr, undrawn_losses, nullptr, nullptr, 0, nullptr, nullptr};
    check(occt_bridge_step_save_assembly_pmi(
              session, file.c_str(), "fixture", nullptr, 0, components, nullptr, 2, nullptr, 0, &plain, nullptr)
            == OCCT_BRIDGE_OK && undrawn_losses[0] == 0,
        "an undrawn tolerance alone reads back with its value");

    // Invalid PMI writes nothing.
    (void)std::remove(file.c_str());
    const auto refused = [&](occt_bridge_step_pmi_t bad, const char* what) {
        check(occt_bridge_step_save_assembly_pmi(
                  session, file.c_str(), "fixture", nullptr, 0, components, nullptr, 2, nullptr, 0, &bad, nullptr)
                == OCCT_BRIDGE_INVALID_ARGUMENT, what);
    };
    occt_bridge_step_tolerance_t unknown = tolerances[0];
    unknown.characteristic = 12;
    refused({datums, 4, &unknown, 1, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr}, "unknown characteristic");
    const occt_bridge_step_datum_reference_t secondary_only[] = {{1, 1, OCCT_BRIDGE_DATUM_RFS, 0}};
    occt_bridge_step_tolerance_t gap = tolerances[3];
    gap.datums = secondary_only;
    gap.datum_count = 1;
    refused({datums, 4, &gap, 1, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr}, "secondary without a primary");
    occt_bridge_step_tolerance_t foreign = tolerances[0];
    foreign.datums = pin_axis;
    foreign.datum_count = 1;
    refused({datums, 4, &foreign, 1, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr}, "datum on another part");
    const occt_bridge_step_datum_t twice[] = {datums[0], datums[0]};
    refused({twice, 2, nullptr, 0, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr}, "repeated datum label");
    const occt_bridge_step_datum_t shared[] = {datums[0], {"A", {1, pin_side, 1}, nullptr}};
    refused({shared, 2, nullptr, 0, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr}, "one label on two parts");
    occt_bridge_step_size_t flat = sizes[0];
    flat.target = {0, top, 1};
    refused({nullptr, 0, nullptr, 0, &flat, 1, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr}, "diameter on a plane");
    occt_bridge_step_size_t three = sizes[1];
    const size_t three_faces[] = {faces.left, faces.right, faces.top};
    three.target = {0, three_faces, 3};
    refused({nullptr, 0, nullptr, 0, &three, 1, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr}, "width over three faces");
    occt_bridge_step_size_t across = sizes[1];
    const size_t square[] = {faces.left, faces.front};
    across.target = {0, square, 2};
    refused({nullptr, 0, nullptr, 0, &across, 1, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr}, "width between square planes");
    const size_t beyond[] = {99};
    occt_bridge_step_tolerance_t missing = tolerances[0];
    missing.target = {0, beyond, 1};
    refused({nullptr, 0, &missing, 1, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr}, "face beyond the plate's");
    const occt_bridge_step_view_t bare{
        "TOP", {0, 0, -1}, {0, 1, 0}, nullptr, 0, front_tolerances, 1, nullptr, nullptr, nullptr, 0, nullptr, 0};
    refused({nullptr, 0, tolerances, 2, nullptr, 0, nullptr, nullptr, nullptr, &bare, 1, nullptr, nullptr},
        "view of an annotation without a presentation");
    const occt_bridge_step_view_t level{
        "TOP", {0, 0, -1}, {0, 0, 2}, nullptr, 0, nullptr, 0, nullptr, nullptr, nullptr, 0, nullptr, 0};
    refused({nullptr, 0, nullptr, 0, nullptr, 0, nullptr, nullptr, nullptr, &level, 1, nullptr, nullptr}, "up along the view");
    const occt_bridge_step_clipping_plane_t long_normal[] = {{{0, 0, 0}, {0, 2, 0}}};
    const size_t unknown_part[] = {2};
    occt_bridge_step_view_t stray = views[1];
    stray.parts = unknown_part;
    stray.part_count = 1;
    refused({datums, 5, tolerances, 2, nullptr, 0, nullptr, nullptr, nullptr, &stray, 1, nullptr, nullptr},
        "a view shows an unknown component");
    const size_t repeated_part[] = {1, 1};
    stray.parts = repeated_part;
    stray.part_count = 2;
    refused({datums, 5, tolerances, 2, nullptr, 0, nullptr, nullptr, nullptr, &stray, 1, nullptr, nullptr},
        "a component at most once");
    stray.parts = nullptr;
    refused({datums, 5, tolerances, 2, nullptr, 0, nullptr, nullptr, nullptr, &stray, 1, nullptr, nullptr},
        "view part arrays are missing");
    occt_bridge_step_view_t loose = views[1];
    loose.clipping_planes = long_normal;
    loose.clipping_plane_count = 1;
    refused({datums, 5, tolerances, 2, nullptr, 0, nullptr, nullptr, nullptr, &loose, 1, nullptr, nullptr},
        "clipping plane without a unit normal");
    occt_bridge_step_dataset_t unreleased = dataset;
    unreleased.revision = nullptr;
    refused({datums, 5, tolerances, 2, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, &unreleased},
        "approvals on an unreleased dataset");
    const occt_bridge_step_approval_t undated[] = {{"APPROVED", "M. Ortiz", "2026-02-30"}};
    occt_bridge_step_dataset_t misdated = dataset;
    misdated.approvals = undated;
    misdated.approval_count = 1;
    refused({datums, 5, tolerances, 2, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, &misdated},
        "approval on an impossible date");
    const occt_bridge_step_view_t two_views[] = {views[0], views[0]};
    refused({datums, 5, tolerances, 2, nullptr, 0, nullptr, nullptr, nullptr, two_views, 2, nullptr, nullptr},
        "repeated view name");
    occt_bridge_step_tolerance_t same_name = tolerances[1];
    same_name.presentation = &flatness;
    occt_bridge_step_tolerance_t named_twice[] = {tolerances[0], same_name};
    refused({datums, 3, named_twice, 2, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr},
        "repeated presentation name");
    occt_bridge_step_presentation_t short_count = flatness;
    short_count.point_count = 2;
    occt_bridge_step_tolerance_t miscounted = tolerances[0];
    miscounted.presentation = &short_count;
    refused({nullptr, 0, &miscounted, 1, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr},
        "polylines not using every point");
    occt_bridge_step_presentation_t tilted = flatness;
    tilted.normal[0] = 1;
    occt_bridge_step_tolerance_t off_plane = tolerances[0];
    off_plane.presentation = &tilted;
    refused({nullptr, 0, &off_plane, 1, nullptr, 0, nullptr, nullptr, nullptr, nullptr, 0, nullptr, nullptr},
        "annotation plane without a unit normal");
    check(std::fopen(file.c_str(), "r") == nullptr, "nothing written");
    check(occt_bridge_step_save_assembly_pmi(
              session, file.c_str(), "fixture", nullptr, 0, components, nullptr, 2, nullptr, 0, nullptr, nullptr)
            == OCCT_BRIDGE_INVALID_ARGUMENT, "missing PMI");
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
    pmi(session, file);
    occt_bridge_session_destroy(session);
    if (failures == 0) {
        (void)std::puts("PASS: STEP assembly structure, sub-assemblies, shared parts, names, colors, and PMI round-trip");
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
