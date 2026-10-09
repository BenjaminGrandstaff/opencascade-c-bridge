#ifndef OCCT_BRIDGE_INTERNAL_HPP
#define OCCT_BRIDGE_INTERNAL_HPP

/*
 * Internal declarations shared by the bridge sources. Not part of the C ABI;
 * the library hides these symbols.
 */

#include "occt_bridge.h"
#include "shape_validator.hpp"

#include <BRepTools_History.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListIteratorOfListOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Face.hxx>
#include <GProp_GProps.hxx>
#include <gp_Vec.hxx>

#include <cstdint>
#include <exception>
#include <mutex>
#include <new>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

struct occt_bridge_history_entry {
    TopoDS_Shape source;
    std::vector<TopoDS_Shape> generated;
    std::vector<TopoDS_Shape> modified;
    bool deleted = false;
};

/*
 * Operation history of one result: explicit per-source records for general
 * operations, or for a rigid move only the moved source and its location,
 * from which any source subshape's counterpart is computed on demand. The
 * located form keeps placed copies O(1) in memory instead of O(subshapes).
 */
struct occt_bridge_operation_history {
    std::vector<occt_bridge_history_entry> entries;
    TopoDS_Shape located_source;
    TopLoc_Location location;
};

/* One failure diagnostic; see occt_bridge_diagnostic_t. */
struct occt_bridge_diagnostic_record {
    occt_bridge_diagnostic_kind_t kind = 0;
    int32_t code = 0;
    int64_t input_index = -1;
    std::string name;
    TopoDS_Shape shape;
};

struct occt_bridge_session {
    mutable std::mutex mutex;
    std::unordered_map<occt_bridge_shape_id_t, TopoDS_Shape> shapes;
    std::unordered_map<occt_bridge_shape_id_t, occt_bridge_operation_history> histories;
    occt_bridge_shape_id_t next_shape_id = 1;
    std::string last_error;
    /* Warnings from the most recent call; cleared when the next call starts. */
    std::string last_warnings;
    /* Diagnostics from the most recent call, capped; cleared like warnings. */
    std::vector<occt_bridge_diagnostic_record> last_diagnostics;
    size_t omitted_diagnostics = 0;
    occt_bridge_session_options_t options{1, 0, 0.0};
};

namespace occt_bridge_internal {

// Builds translated closure without storing a handle; callers hold the lock.
occt_bridge_status_t build_open_profile_face(
    occt_bridge_session_t* session,
    const TopoDS_Shape& value,
    const gp_Vec& translation,
    TopoDS_Face& face,
    std::vector<occt_bridge_history_entry>& entries);

bool finite(double value);

bool finite(const occt_bridge_vec3_t& value);

occt_bridge_status_t fail(
    occt_bridge_session_t* session,
    occt_bridge_status_t status,
    std::string message);

occt_bridge_status_t succeed(occt_bridge_session_t* session);

void add_warning(occt_bridge_session_t* session, const std::string& warning);

void add_diagnostic(occt_bridge_session_t* session, occt_bridge_diagnostic_record record);

const TopoDS_Shape* find_shape(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t id);

occt_bridge_status_t store_shape(
    occt_bridge_session_t* session,
    const TopoDS_Shape& shape,
    occt_bridge_shape_id_t* out_shape);

void append_unique_shape(std::vector<TopoDS_Shape>& shapes, const TopoDS_Shape& candidate);

std::vector<TopoDS_Shape> history_sources(const std::vector<const TopoDS_Shape*>& roots);

occt_bridge_status_t store_shape_with_entries(
    occt_bridge_session_t* session,
    const TopoDS_Shape& shape,
    occt_bridge_shape_id_t* out_shape,
    std::vector<occt_bridge_history_entry> entries);

/* Explicit subset extraction, retaining only parent history targets in it. */
occt_bridge_status_t store_subshape_with_history(
    occt_bridge_session_t* session, occt_bridge_shape_id_t parent_id,
    const TopoDS_Shape& selected, occt_bridge_shape_id_t* out_shape);

/*
 * Appends `replacement`, or its subshapes of `type` when healing replaced a
 * shape with a container (a face split into a compound of faces), so history
 * stays type-consistent.
 */
void append_same_type(std::vector<TopoDS_Shape>& shapes, const TopoDS_Shape& replacement, TopAbs_ShapeEnum type);

/* Maps history targets through a healing reshape; removed targets drop out. */
void map_through_reshape(std::vector<TopoDS_Shape>& targets, const opencascade::handle<BRepTools_History>& reshape);

/*
 * Composes operation history with the healing that followed it, so sources
 * still lead to the faces, edges, and vertices of the healed result. A
 * source the operation left untouched but healing replaced becomes modified.
 */
void compose_with_reshape(std::vector<occt_bridge_history_entry>& history, const opencascade::handle<BRepTools_History>& reshape);

std::string check_status_name(BRepCheck_Status status);

const char* shape_type_noun(TopAbs_ShapeEnum type);

/*
 * Appends a summary of the call's diagnostics to a failure message, such as
 * "fillet construction failed: ChFiDS_StartsolFailure on selection 0".
 */
std::string with_diagnostics(const occt_bridge_session_t* session, std::string message);

/*
 * Records every validation failure of `shape`, outermost subshapes first,
 * with BRepCheck's status for the subshape alone and within its parents.
 * O(subshapes); runs only for results being rejected.
 */
void record_invalid_subshapes(
    occt_bridge_session_t* session,
    const ShapeValidator& analyzer,
    const TopoDS_Shape& shape);

/*
 * Validates an operation result when the session asks for it and, if it is
 * invalid and healing is enabled, repairs it with shape fixing. Replaces
 * `shape` (and maps `history`) with the healed result and records a warning;
 * fails with OCCT_BRIDGE_INVALID_GEOMETRY when the result stays invalid.
 * Costs one BRepCheck pass, plus a fixing pass only for invalid results.
 */
occt_bridge_status_t check_result(
    occt_bridge_session_t* session,
    const std::string& operation,
    TopoDS_Shape& shape,
    std::vector<occt_bridge_history_entry>* history);

bool contains_topology(const TopoDS_Shape& shape, TopAbs_ShapeEnum type);

/*
 * Shared integration for every measurement. All-analytic shapes use
 * fixed-order integration, exact to roundoff there. With any freeform face or
 * edge, volume uses span-aware Gauss-Kronrod integration (1e-7 relative error
 * target) and area adaptive integration (1e-9): fixed order was 20% low on a lofted
 * B-spline airfoil and 0.03% low on a prism of a spline-bounded sketch.
 * Centers and inertia of freeform shapes use adaptive Gauss integration
 * (about 1e-4 relative on spline-bounded faces), because span-aware moments
 * cost seconds per blend surface. measure_volume_moments returns the
 * adaptive error estimate, 0 on the fixed-order path.
 */
double measure_volume(const TopoDS_Shape& shape, bool only_closed = false);
double measure_volume_moments(
    const TopoDS_Shape& shape,
    GProp_GProps& properties,
    bool only_closed = false);
void measure_surface_properties(const TopoDS_Shape& shape, GProp_GProps& properties);

bool is_descendant(
    const TopoDS_Shape& parent,
    const TopoDS_Shape& candidate,
    TopAbs_ShapeEnum type);

bool belongs_to(const TopoDS_Shape& parent, const TopoDS_Shape& candidate);

/* The single shell of a sewing result, or a null shell when there is not exactly one. */
TopoDS_Shell single_shell(const TopoDS_Shape& sewed);

/*
 * Runs an entry point under the session lock with exceptions contained.
 * Calls start with no warnings or diagnostics unless `keep_diagnostics` is
 * set, which the diagnostic queries use to read the previous call's.
 */
template <typename Function>
occt_bridge_status_t guarded(
    occt_bridge_session_t* session,
    Function&& function,
    bool keep_diagnostics = false) noexcept {
    if (session == nullptr) {
        return OCCT_BRIDGE_INVALID_ARGUMENT;
    }
    try {
        std::lock_guard<std::mutex> lock(session->mutex);
        if (!keep_diagnostics) {
            session->last_warnings.clear();
            session->last_diagnostics.clear();
            session->omitted_diagnostics = 0;
        }
        const occt_bridge_status_t status = function();
        if (status != OCCT_BRIDGE_OK && !keep_diagnostics && session->omitted_diagnostics != 0) {
            session->last_error += " (" + std::to_string(session->omitted_diagnostics)
                + " more diagnostics omitted)";
        }
        return status;
    } catch (const Standard_Failure& error) {
        const char* message = error.GetMessageString();
        return fail(
            session,
            OCCT_BRIDGE_KERNEL_ERROR,
            message == nullptr ? "Open Cascade operation failed" : message);
    } catch (const std::bad_alloc&) {
        return fail(session, OCCT_BRIDGE_ALLOCATION_FAILED, "allocation failed");
    } catch (const std::exception& error) {
        return fail(session, OCCT_BRIDGE_INTERNAL_ERROR, error.what());
    } catch (...) {
        return fail(session, OCCT_BRIDGE_INTERNAL_ERROR, "unknown internal error");
    }
}

template <typename Operation>
std::vector<occt_bridge_history_entry> collect_history(
    Operation& operation,
    const std::vector<const TopoDS_Shape*>& roots) {
    std::vector<occt_bridge_history_entry> history;
    for (const TopoDS_Shape& source : history_sources(roots)) {
        occt_bridge_history_entry entry;
        entry.source = source;
        const TopTools_ListOfShape& generated = operation.Generated(source);
        for (TopTools_ListIteratorOfListOfShape iterator(generated); iterator.More(); iterator.Next()) {
            append_unique_shape(entry.generated, iterator.Value());
        }
        const TopTools_ListOfShape& modified = operation.Modified(source);
        for (TopTools_ListIteratorOfListOfShape iterator(modified); iterator.More(); iterator.Next()) {
            append_unique_shape(entry.modified, iterator.Value());
        }
        entry.deleted = operation.IsDeleted(source);
        history.push_back(std::move(entry));
    }
    return history;
}

template <typename Operation>
occt_bridge_status_t store_shape_with_history(
    occt_bridge_session_t* session,
    const TopoDS_Shape& shape,
    occt_bridge_shape_id_t* out_shape,
    Operation& operation,
    const std::vector<const TopoDS_Shape*>& roots) {
    return store_shape_with_entries(session, shape, out_shape, collect_history(operation, roots));
}

/* OCCT's name for an enumeration value, or "<prefix><value>" if unknown. */
template <size_t Count>
std::string enum_name(const char* const (&names)[Count], int32_t value, const char* prefix) {
    if (value >= 0 && static_cast<size_t>(value) < Count) {
        return names[value];
    }
    return std::string(prefix) + std::to_string(value);
}

/* Checks, optionally heals, and stores an operation result with history. */
template <typename Operation>
occt_bridge_status_t store_checked_result(
    occt_bridge_session_t* session,
    const std::string& name,
    TopoDS_Shape shape,
    occt_bridge_shape_id_t* out_shape,
    Operation& operation,
    const std::vector<const TopoDS_Shape*>& roots) {
    std::vector<occt_bridge_history_entry> history = collect_history(operation, roots);
    const occt_bridge_status_t status = check_result(session, name, shape, &history);
    if (status != OCCT_BRIDGE_OK) {
        return status;
    }
    return store_shape_with_entries(session, shape, out_shape, std::move(history));
}

/* Unique subshapes of `type` in occt_bridge_shape_subshapes order, so batch
 * query indices correspond. */
inline TopTools_IndexedMapOfShape ordered(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
    TopTools_IndexedMapOfShape result;
    for (TopExp_Explorer explorer(shape, type); explorer.More(); explorer.Next()) {
        result.Add(explorer.Current());
    }
    return result;
}

// Shared buffer protocol of the batch queries: a null buffer with capacity 0
// reports the count; otherwise the buffer must fit every result and is
// written only on success.
template <typename Value>
inline occt_bridge_status_t check_buffer(
    occt_bridge_session_t* session,
    const Value* buffer,
    size_t capacity,
    size_t count,
    size_t* out_count,
    bool& query_only) {
    query_only = buffer == nullptr;
    if (query_only) {
        *out_count = count;
        return succeed(session);
    }
    if (capacity < count) {
        *out_count = count;
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "output capacity is insufficient");
    }
    return OCCT_BRIDGE_OK;
}

} // namespace occt_bridge_internal

#endif
