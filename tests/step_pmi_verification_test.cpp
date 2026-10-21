/* Read a real AP242 export against changed expectations. Losses must stay
 * specific to the missing information, and read items cannot be claimed twice. */
#include "bridge_internal.hpp"
#include "step_pmi.hpp"
#include <XCAFApp_Application.hxx>

#include <algorithm>
#include <array>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <iterator>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>

namespace {
using namespace occt_bridge_internal;
int failures = 0;
size_t comparisons = 0;
size_t exports = 0;
size_t validation_checks = 0;

void check(bool condition, const char* test, const char* detail) {
    if (!condition) {
        (void)std::fprintf(stderr, "FAILED: %s: %s\n", test, detail);
        ++failures;
    }
}

void require(occt_bridge_status_t status) {
    if (status != OCCT_BRIDGE_OK) throw std::runtime_error("PMI fixture construction failed");
}

struct Losses {
    std::array<uint32_t, 1> datums{};
    std::array<uint32_t, 3> tolerances{0, 0, UINT32_MAX};
    std::array<uint32_t, 1> sizes{};
    std::array<uint32_t, 2> views{};
    std::array<uint8_t, 2> view_datums{};
    std::array<std::array<uint8_t, 2>, 2> view_tolerances{};
    std::array<uint8_t, 2> view_sizes{};
    uint32_t dataset = 0;

    void poison() {
        datums.fill(UINT32_MAX);
        tolerances.fill(UINT32_MAX);
        sizes.fill(UINT32_MAX);
        views.fill(UINT32_MAX);
        view_datums.fill(UINT8_MAX);
        for (auto& values : view_tolerances) values.fill(UINT8_MAX);
        view_sizes.fill(UINT8_MAX);
        dataset = UINT32_MAX;
    }

    void compare(const Losses& expected, const char* test) const {
        check(datums == expected.datums, test, "datum masks");
        check(tolerances == expected.tolerances, test, "tolerance masks / unused output slot");
        check(sizes == expected.sizes, test, "size masks");
        check(views == expected.views, test, "view masks");
        check(view_datums == expected.view_datums, test, "datum visibility");
        check(view_tolerances == expected.view_tolerances, test, "tolerance visibility");
        check(view_sizes == expected.view_sizes, test, "size visibility");
        check(dataset == expected.dataset, test, "dataset mask");
    }
};

struct Fixture {
    using SessionOwner = std::unique_ptr<occt_bridge_session_t, decltype(&occt_bridge_session_destroy)>;
    SessionOwner session{nullptr, occt_bridge_session_destroy};
    Losses losses;
    std::vector<const TopoDS_Shape*> shapes;
    std::array<occt_bridge_step_component_t, 2> components{};
    std::array<size_t, 1> face{0};
    std::array<size_t, 2> width_faces{0, 1};
    std::array<size_t, 2> reversed_faces{1, 0};
    std::array<double, 6> points{0, 0, 20, 5, 0, 20};
    std::array<uint32_t, 1> polylines{2};
    std::array<occt_bridge_step_presentation_t, 4> drawings{};
    std::array<occt_bridge_step_datum_t, 1> datums{};
    std::array<occt_bridge_step_tolerance_t, 3> tolerances{};
    std::array<occt_bridge_step_datum_reference_t, 1> references{};
    std::array<occt_bridge_step_size_t, 1> sizes{};
    std::array<size_t, 1> datum_indices{0};
    std::array<size_t, 2> tolerance_indices{0, 1};
    std::array<size_t, 1> size_indices{0};
    std::array<size_t, 1> plate_only{0};
    std::array<size_t, 1> pin_only{1};
    std::array<size_t, 2> both_parts{0, 1};
    occt_bridge_step_clipping_plane_t clipping{{0, 0, 5}, {0, 0, 1}};
    std::array<occt_bridge_step_view_t, 2> views{};
    occt_bridge_step_approval_t approval{"CHECKED", "Maya", "2026-10-21"};
    occt_bridge_step_dataset_t dataset{};
    occt_bridge_step_pmi_t pmi{};

    Fixture() {
        occt_bridge_session_t* raw = nullptr;
        require(occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION, &raw));
        session.reset(raw);
        occt_bridge_shape_id_t box = 0, pin = 0;
        require(occt_bridge_create_box(raw, {0, 0, 0}, {10, 20, 10}, &box));
        require(occt_bridge_create_cylinder(raw, {30, 0, 0}, {0, 0, 1}, 2, 8, &pin));
        shapes = {&raw->shapes.at(box), &raw->shapes.at(pin)};
        components = {{{box, "plate", "plate", 0, {0, 0, 0}}, {pin, "pin", "pin", 0, {0, 0, 0}}}};
        const std::array<const char*, 4> names{"datum", "flatness", "second flatness", "width"};
        for (size_t i = 0; i < drawings.size(); ++i) {
            drawings[i] = {names[i], points.data(), 2, polylines.data(), 1, {0, 0, 20}, {0, 0, 1}, {1, 0, 0}};
        }
        datums[0] = {"A", {0, face.data(), 1}, &drawings[0]};
        tolerances[0] = {{0, face.data(), 1}, OCCT_BRIDGE_GDT_FLATNESS, .05, 0,
            OCCT_BRIDGE_MATERIAL_RFS, 0, 0, nullptr, 0, &drawings[1]};
        tolerances[1] = tolerances[0];
        tolerances[1].value_mm = .08;
        tolerances[1].presentation = &drawings[2];
        tolerances[2] = tolerances[0];
        tolerances[2].presentation = nullptr;
        sizes[0] = {{0, width_faces.data(), 2}, OCCT_BRIDGE_STEP_SIZE_WIDTH, 9.9, 10.1, &drawings[3]};
        views[0] = {"TOP", {0, 0, -1}, {0, 1, 0}, datum_indices.data(), 1, tolerance_indices.data(), 2,
            &losses.view_datums[0], losses.view_tolerances[0].data(), &clipping, 1, plate_only.data(), 1,
            size_indices.data(), 1, &losses.view_sizes[0]};
        views[1] = {"FRONT", {0, -1, 0}, {0, 0, 1}, datum_indices.data(), 1, tolerance_indices.data(), 2,
            &losses.view_datums[1], losses.view_tolerances[1].data(), nullptr, 0, both_parts.data(), 2,
            size_indices.data(), 1, &losses.view_sizes[1]};
        dataset = {"PMI-TEST", "Example Works", "model", "A", "Initial", &approval, 1, &losses.dataset};
        pmi = {datums.data(), 1, tolerances.data(), 2, sizes.data(), 1, losses.datums.data(), losses.tolerances.data(),
            losses.sizes.data(), views.data(), 2, losses.views.data(), &dataset};
    }

    void write(const char* path, const char* test = "export baseline") {
        ++exports;
        require(occt_bridge_step_save_assembly_pmi(session.get(), path, "fixture", nullptr, 0,
            components.data(), nullptr, components.size(), nullptr, 0, &pmi, nullptr));
        losses.compare(Losses{}, test);
    }

    void verify(const char* path, const char* test, const Losses& expected) {
        losses.poison();
        check(verify_pmi(path, pmi, shapes), test, "file read-back");
        losses.compare(expected, test);
    }
};

template <typename Change>
void verify_case(const char* path, const char* name, Change change, const Losses& expected = {}) {
    Fixture fixture;
    ++comparisons;
    change(fixture);
    fixture.verify(path, name, expected);
}

void omit_sizes(Fixture& f) {
    f.pmi.size_count = 0;
    for (auto& view : f.views) view.size_count = 0;
}

void omit_annotations(Fixture& f) {
    omit_sizes(f);
    f.pmi.datum_count = f.pmi.tolerance_count = 0;
    for (auto& view : f.views) view.datum_count = view.tolerance_count = 0;
}

template <typename Change>
void export_case(const char* path, const char* name, Change change) {
    Fixture f;
    change(f);
    const std::string output = std::string(path) + "." + name;
    f.write(output.c_str(), name);
    (void)std::remove(output.c_str());
}

void writer_cases(const char* path) {
    export_case(path, "no-size-dimensions", [](Fixture& f) { omit_sizes(f); });
    export_case(path, "no-saved-views", [](Fixture& f) { f.pmi.view_count = 0; });
    export_case(path, "dataset-only", [](Fixture& f) {
        omit_annotations(f);
        f.pmi.view_count = 0;
    });
    export_case(path, "camera-only", [](Fixture& f) {
        omit_annotations(f);
        f.pmi.dataset = nullptr;
        for (auto& view : f.views) view.part_count = 0;
    });
    const auto diameter_zone = [](Fixture& f) {
        omit_sizes(f);
        f.tolerances[0].characteristic = OCCT_BRIDGE_GDT_POSITION;
        f.tolerances[0].target.component = 1;
        f.tolerances[0].diameter = 1;
    };
    export_case(path, "diameter-zone", diameter_zone);
    export_case(path, "projected-zone", [&](Fixture& f) {
        diameter_zone(f);
        f.tolerances[0].projected_mm = 12;
    });
    export_case(path, "shared-part-representations", [](Fixture& f) {
        f.components[1].shape = f.components[0].shape;
        f.shapes[1] = f.shapes[0];
    });
    export_case(path, "compound-part-representation", [](Fixture& f) {
        omit_annotations(f);
        const std::array<occt_bridge_shape_id_t, 2> children{f.components[0].shape, f.components[1].shape};
        occt_bridge_shape_id_t compound = 0;
        require(occt_bridge_create_compound(f.session.get(), children.data(), children.size(), &compound));
        f.components[0].shape = compound;
        f.shapes[0] = &f.session->shapes.at(compound);
        f.views[1].parts = f.plate_only.data();
        f.views[1].part_count = 1;
    });
}

void annotation_cases(const char* path) {
    verify_case(path, "unchanged", [](Fixture&) {});
    verify_case(path, "best candidates in reversed value order", [](Fixture& f) {
        std::swap(f.tolerances[0].value_mm, f.tolerances[1].value_mm);
    });
    Losses expected;
    expected.tolerances[0] = OCCT_BRIDGE_PMI_LOST_VALUE;
    verify_case(path, "changed tolerance value", [](Fixture& f) { f.tolerances[0].value_mm = .06; }, expected);
    expected.tolerances[0] = OCCT_BRIDGE_PMI_LOST_ITEM;
    verify_case(path, "missing characteristic", [](Fixture& f) { f.tolerances[0].characteristic = OCCT_BRIDGE_GDT_STRAIGHTNESS; }, expected);
    expected = {};
    expected.tolerances[2] = OCCT_BRIDGE_PMI_LOST_ITEM;
    verify_case(path, "one read tolerance cannot serve two expectations", [](Fixture& f) { f.pmi.tolerance_count = 3; }, expected);
    expected = {};
    expected.datums[0] = OCCT_BRIDGE_PMI_LOST_ITEM;
    verify_case(path, "missing datum", [](Fixture& f) { f.datums[0].label = "Z"; }, expected);
    expected = {};
    expected.sizes[0] = OCCT_BRIDGE_PMI_LOST_VALUE;
    verify_case(path, "changed size limits", [](Fixture& f) { f.sizes[0].lower_mm = 9.8; }, expected);
    verify_case(path, "width faces may be reversed", [](Fixture& f) { f.sizes[0].target.faces = f.reversed_faces.data(); });
}

void presentation_cases(const char* path) {
    Losses expected;
    expected.tolerances[0] = OCCT_BRIDGE_PMI_LOST_PRESENTATION;
    expected.view_tolerances[0][0] = expected.view_tolerances[1][0] = 1;
    verify_case(path, "missing named presentation", [](Fixture& f) { f.drawings[1].name = "missing"; }, expected);
    expected.view_tolerances[0][0] = expected.view_tolerances[1][0] = 0;
    verify_case(path, "changed presentation geometry", [](Fixture& f) { f.points[3] = 6; }, [&] {
        // All four presentations share the changed point array.
        Losses all = expected;
        all.datums[0] = OCCT_BRIDGE_PMI_LOST_PRESENTATION;
        all.tolerances[1] = OCCT_BRIDGE_PMI_LOST_PRESENTATION;
        all.sizes[0] = OCCT_BRIDGE_PMI_LOST_PRESENTATION;
        return all;
    }());
}

void view_cases(const char* path) {
    Losses expected;
    expected.views[0] = OCCT_BRIDGE_PMI_VIEW_LOST_CAMERA;
    verify_case(path, "changed camera", [](Fixture& f) { f.views[0].up[0] = 1; }, expected);
    expected.views[0] = OCCT_BRIDGE_PMI_VIEW_LOST_CLIPPING;
    verify_case(path, "changed clipping", [](Fixture& f) { f.clipping.origin[2] = 6; }, expected);
    expected.views[0] = OCCT_BRIDGE_PMI_VIEW_LOST_PARTS;
    verify_case(path, "changed visible part", [](Fixture& f) { f.views[0].parts = f.pin_only.data(); }, expected);
    verify_case(path, "missing visible parts", [](Fixture& f) { f.views[0].part_count = 0; }, expected);
    expected.views[0] = OCCT_BRIDGE_PMI_VIEW_LOST;
    expected.view_datums[0] = expected.view_sizes[0] = 1;
    expected.view_tolerances[0].fill(1);
    verify_case(path, "missing view", [](Fixture& f) { f.views[0].name = "missing"; }, expected);
    expected = {};
    expected.views.fill(UINT32_MAX);
    verify_case(path, "visibility without view loss output", [](Fixture& f) { f.pmi.out_view_losses = nullptr; }, expected);

}

void dataset_cases(const char* path) {
    Losses expected;
    expected.dataset = OCCT_BRIDGE_DATASET_LOST_IDENTIFIER;
    verify_case(path, "changed identifier", [](Fixture& f) { f.dataset.identifier = "other"; }, expected);
    expected.dataset = OCCT_BRIDGE_DATASET_LOST_REVISION;
    verify_case(path, "changed revision", [](Fixture& f) { f.dataset.revision = "B"; }, expected);
    expected.dataset = OCCT_BRIDGE_DATASET_LOST_AUTHORITY;
    verify_case(path, "changed authority", [](Fixture& f) { f.dataset.authority = "drawing"; }, expected);
    expected.dataset = OCCT_BRIDGE_DATASET_LOST_APPROVALS;
    verify_case(path, "changed approval", [](Fixture& f) { f.approval.date = "2026-10-20"; }, expected);
    verify_case(path, "changed approver", [](Fixture& f) { f.approval.name = "Other"; }, expected);
    verify_case(path, "changed approval role", [](Fixture& f) { f.approval.role = "APPROVED"; }, expected);
    verify_case(path, "unexpected approval", [](Fixture& f) { f.dataset.approval_count = 0; }, expected);
    expected.dataset = OCCT_BRIDGE_DATASET_LOST_REVISION;
    verify_case(path, "changed revision description", [](Fixture& f) { f.dataset.description = "Other revision"; }, expected);
    verify_case(path, "optional revision description", [](Fixture& f) { f.dataset.description = nullptr; });
    expected.dataset = UINT32_MAX;
    verify_case(path, "optional dataset loss output", [](Fixture& f) { f.dataset.out_losses = nullptr; }, expected);

}

void validation_is(const Fixture& fixture, const char* expected, const char* test) {
    ++validation_checks;
    const char* error = pmi_error(fixture.pmi, fixture.shapes);
    check(expected == nullptr ? error == nullptr : error != nullptr && std::strcmp(error, expected) == 0,
        test, expected == nullptr ? "valid request" : expected);
}

void tolerance_reference_precedence() {
    Fixture f;
    auto& tolerance = f.tolerances[0];
    tolerance.characteristic = -1;
    tolerance.value_mm = std::numeric_limits<double>::quiet_NaN();
    tolerance.datum_count = 1;
    f.references[0] = {1, 3, 9, 2};
    f.datums[0].target.component = 1;
    const auto error_is = [&](const char* expected) { validation_is(f, expected, "tolerance reference order"); };
    error_is("unknown geometric characteristic");
    tolerance.characteristic = OCCT_BRIDGE_GDT_FLATNESS;
    error_is("a tolerance value must be finite and positive");
    tolerance.value_mm = .05;
    error_is("tolerance datum references are missing");
    tolerance.datums = f.references.data();
    error_is("a tolerance references an unknown datum");
    f.references[0].datum = 0;
    error_is("datum compartments are 0, 1 and 2");
    f.references[0].compartment = 0;
    error_is("unknown datum material boundary");
    f.references[0].boundary = OCCT_BRIDGE_DATUM_RFS;
    error_is("translation must be 0 or 1");
    f.references[0].translation = 0;
    error_is("a tolerance's datums must lie on its own part");
    f.datums[0].target.component = 0;
    f.references[0].compartment = 1;
    error_is("datum compartments must fill from the primary");
    f.references[0].compartment = 0;
    error_is(nullptr);
}

void view_validation_precedence() {
    Fixture f;
    auto& view = f.views[0];
    view.datums = nullptr;
    view.clipping_plane_count = 17;
    f.clipping.normal[0] = 2;
    view.parts = nullptr;
    f.datum_indices[0] = 1;
    view.sizes = nullptr;
    f.tolerance_indices[0] = 2;
    const auto error_is = [&](const char* expected) { validation_is(f, expected, "view validation order"); };
    error_is("view annotation arrays are missing");
    view.datums = f.datum_indices.data();
    error_is("a view has at most 16 clipping planes");
    view.clipping_plane_count = 1;
    error_is("a clipping plane needs a finite origin and a unit normal");
    f.clipping.normal[0] = 0;
    error_is("view part arrays are missing");
    view.parts = f.plate_only.data();
    error_is("a view shows an unknown datum");
    f.datum_indices[0] = 0;
    error_is("view annotation arrays are missing");
    view.sizes = f.size_indices.data();
    error_is("a view shows an unknown tolerance");
    f.tolerance_indices[0] = 0;
    error_is(nullptr);
}

// Remove a complete association record from this controlled STEP fixture.
// Referenced entities remain, so verification must detect the missing link.
std::string without_record(std::string text, const std::string& type) {
    const size_t at = text.find(" = " + type + "(");
    if (at == std::string::npos) throw std::runtime_error("STEP record not found");
    const size_t newline = text.rfind('\n', at);
    const size_t begin = newline == std::string::npos ? 0 : newline + 1;
    const size_t end = text.find(';', at);
    if (end == std::string::npos) throw std::runtime_error("STEP record is incomplete");
    text.erase(begin, end - begin + 1);
    return text;
}

void verify_altered_file(const std::string& path, const std::string& text, const char* test, const Losses& expected) {
    std::ofstream stream(path, std::ios::binary);
    stream << text;
    stream.close();
    if (!stream) throw std::runtime_error("cannot write altered STEP fixture");
    Fixture fixture;
    ++comparisons;
    fixture.verify(path.c_str(), test, expected);
    (void)std::remove(path.c_str());
}

void approval_file_cases(const char* path) {
    std::ifstream stream(path, std::ios::binary);
    const std::string text((std::istreambuf_iterator<char>(stream)), std::istreambuf_iterator<char>());
    if (stream.bad() || text.empty()) throw std::runtime_error("cannot read STEP fixture");
    const std::string altered = std::string(path) + ".altered";
    Losses expected;
    expected.dataset = OCCT_BRIDGE_DATASET_LOST_APPROVALS;
    for (const char* type : {"APPROVAL_DATE_TIME", "APPROVAL_PERSON_ORGANIZATION", "APPLIED_APPROVAL_ASSIGNMENT"}) {
        verify_altered_file(altered, without_record(text, type), type, expected);
    }
    std::string rejected = text;
    const std::string approved = "APPROVAL_STATUS('approved')";
    const size_t at = rejected.find(approved);
    if (at == std::string::npos) throw std::runtime_error("approval status not found");
    rejected.replace(at, approved.size(), "APPROVAL_STATUS('rejected')");
    verify_altered_file(altered, rejected, "rejected approval status", expected);
}

std::string replace_first(std::string text, const std::string& from, const std::string& to) {
    const size_t at = text.find(from);
    if (at == std::string::npos) throw std::runtime_error("STEP fixture text not found");
    text.replace(at, from.size(), to);
    return text;
}

void reader_file_cases(const char* path) {
    std::ifstream stream(path, std::ios::binary);
    const std::string text((std::istreambuf_iterator<char>(stream)), std::istreambuf_iterator<char>());
    if (stream.bad() || text.empty()) throw std::runtime_error("cannot read STEP fixture");
    const std::string altered = std::string(path) + ".altered";
    Losses expected;
    expected.datums[0] = OCCT_BRIDGE_PMI_LOST_ITEM;
    verify_altered_file(altered, without_record(text, "SHAPE_ASPECT_RELATIONSHIP"),
                        "missing datum relationship", expected);
    expected.datums[0] = OCCT_BRIDGE_PMI_LOST_PRESENTATION;
    verify_altered_file(altered, without_record(text, "DRAUGHTING_MODEL_ITEM_ASSOCIATION"),
                        "missing presentation link retains view visibility", expected);
    const std::string link = "PMI representation to presentation link";
    verify_altered_file(altered, replace_first(text, link, "unrelated presentation link"),
                        "unrelated presentation link", expected);
    verify_altered_file(altered, replace_first(text, link, "PMI REPRESENTATION TO PRESENTATION LINK"),
                        "case insensitive presentation link", {});
    verify_altered_file(altered, replace_first(text, ",((1,2)))", ",((2,1)))"),
                        "reversed presentation segment", expected);
    verify_altered_file(altered, replace_first(text, ",((1,2)))", ",((0,2)))"),
                        "presentation index below coordinate bounds", expected);
    verify_altered_file(altered, replace_first(text, ",((1,2)))", ",((1,3)))"),
                        "presentation index above coordinate bounds", expected);
    verify_altered_file(altered, replace_first(text, ",((1,2)))", ",((0,1,2)))"),
                        "invalid segment does not discard following valid segment", {});
}

void validation_precedence() {
    Fixture f;
    f.datums[0].label = "";
    f.tolerances[0].value_mm = std::numeric_limits<double>::quiet_NaN();
    f.sizes[0].lower_mm = -1;
    f.drawings[0].name = "";
    f.views[0].name = "";
    f.dataset.identifier = "";
    const auto error_is = [&](const char* expected) { validation_is(f, expected, "validation precedence"); };
    f.pmi.datums = nullptr;
    error_is("PMI arrays are missing");
    f.pmi.datums = f.datums.data();
    error_is("datums need nonempty labels");
    f.datums[0].label = "A";
    error_is("a tolerance value must be finite and positive");
    f.tolerances[0].value_mm = .05;
    error_is("size limits must be finite and positive, lower not above upper");
    f.sizes[0].lower_mm = 9.9;
    error_is("presentations need nonempty names");
    f.drawings[0].name = "datum";
    error_is("views need nonempty names");
    f.views[0].name = "TOP";
    error_is("a dataset needs an identifier, an organization and an authority");
    f.dataset.identifier = "PMI-TEST";
    error_is(nullptr);
}

int run(const char* path) {
    const auto application = XCAFApp_Application::GetApplication();
    const int documents = application->NbDocuments();
    Fixture baseline;
    baseline.write(path);
    writer_cases(path);
    annotation_cases(path);
    presentation_cases(path);
    view_cases(path);
    dataset_cases(path);
    approval_file_cases(path);
    reader_file_cases(path);
    validation_precedence();
    tolerance_reference_precedence();
    view_validation_precedence();
    Fixture unreadable;
    unreadable.losses.poison();
    Losses unchanged;
    unchanged.poison();
    const std::string missing = std::string(path) + ".missing";
    check(!verify_pmi(missing.c_str(), unreadable.pmi, unreadable.shapes), "missing file", "read failure");
    unreadable.losses.compare(unchanged, "missing file leaves outputs untouched");
    check(application->NbDocuments() == documents, "read-back lifecycle", "no leaked XCAF documents");
    (void)std::printf("%zu exports, %zu read-back comparisons, %zu validation checks; %d failures\n",
                       exports, comparisons, validation_checks, failures);
    (void)std::remove(path);
    return failures == 0 ? 0 : 1;
}
}  // namespace

int main(int argc, char** argv) {
    try {
        return run(argc > 1 ? argv[1] : "step-pmi-verification.step");
    } catch (const std::exception& error) {
        (void)std::fprintf(stderr, "FAILED: %s\n", error.what());
    } catch (...) {
        (void)std::fputs("FAILED: unexpected exception\n", stderr);
    }
    return 1;
}
