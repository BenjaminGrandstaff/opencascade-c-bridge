#include "occt_bridge.h"
#include "shape_validator.hpp"

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_ListOfStatus.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepLib.hxx>
#include <BRepLProp_CLProps.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepOffsetAPI_MakePipe.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepTools.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeFix_Shape.hxx>
#include <BRepTools_History.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Tool.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <GeomConvert_BSplineCurveToBezierCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <Standard_Failure.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPControl_Writer.hxx>
#include <STEPControl_StepModelType.hxx>
#include <StlAPI_Writer.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Wire.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopAbs_State.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_AlertWithShape.hxx>
#include <Message_Report.hxx>
#include <Standard_Type.hxx>
#include <Precision.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax1.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>
#include <gp_Hypr.hxx>
#include <gp_Parab.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>
#include <GProp_GProps.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListIteratorOfListOfShape.hxx>
#include <TopTools_ListOfShape.hxx>

#include <algorithm>
#include <cmath>
#include <cstring>
#include <exception>
#include <limits>
#include <mutex>
#include <new>
#include <queue>
#include <sstream>
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

namespace {

using occt_bridge_internal::ShapeValidator;

bool finite(double value) {
    return std::isfinite(value);
}

bool finite(const occt_bridge_vec3_t& value) {
    return finite(value.x) && finite(value.y) && finite(value.z);
}

gp_Pnt to_point(const occt_bridge_vec3_t& value) {
    return {value.x, value.y, value.z};
}

occt_bridge_status_t fail(
    occt_bridge_session_t* session,
    occt_bridge_status_t status,
    std::string message) {
    if (session != nullptr) {
        session->last_error = std::move(message);
    }
    return status;
}

occt_bridge_status_t succeed(occt_bridge_session_t* session) {
    session->last_error.clear();
    return OCCT_BRIDGE_OK;
}

void add_warning(occt_bridge_session_t* session, const std::string& warning) {
    if (!session->last_warnings.empty()) {
        session->last_warnings += '\n';
    }
    session->last_warnings += warning;
}

void add_diagnostic(occt_bridge_session_t* session, occt_bridge_diagnostic_record record) {
    if (session->last_diagnostics.size() < OCCT_BRIDGE_MAX_DIAGNOSTICS) {
        session->last_diagnostics.push_back(std::move(record));
    } else {
        ++session->omitted_diagnostics;
    }
}

size_t copy_text(const std::string& text, char* buffer, size_t buffer_capacity) {
    if (buffer != nullptr && buffer_capacity != 0) {
        const size_t amount = std::min(buffer_capacity - 1, text.size());
        std::memcpy(buffer, text.data(), amount);
        buffer[amount] = '\0';
    }
    return text.size() + 1;
}

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

const TopoDS_Shape* find_shape(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t id) {
    const auto found = session->shapes.find(id);
    return found == session->shapes.end() ? nullptr : &found->second;
}

occt_bridge_status_t store_shape(
    occt_bridge_session_t* session,
    const TopoDS_Shape& shape,
    occt_bridge_shape_id_t* out_shape) {
    if (out_shape == nullptr) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
    }
    *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    if (shape.IsNull()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "operation produced a null shape");
    }
    if (session->next_shape_id == OCCT_BRIDGE_INVALID_SHAPE_ID) {
        return fail(session, OCCT_BRIDGE_INTERNAL_ERROR, "shape handle space exhausted");
    }
    const occt_bridge_shape_id_t id = session->next_shape_id++;
    session->shapes.emplace(id, shape);
    *out_shape = id;
    return succeed(session);
}

void append_unique_shape(std::vector<TopoDS_Shape>& shapes, const TopoDS_Shape& candidate) {
    if (candidate.IsNull()) {
        return;
    }
    const auto duplicate = std::find_if(
        shapes.begin(),
        shapes.end(),
        [&](const TopoDS_Shape& shape) { return shape.IsSame(candidate); });
    if (duplicate == shapes.end()) {
        shapes.push_back(candidate);
    }
}

std::vector<TopoDS_Shape> history_sources(const std::vector<const TopoDS_Shape*>& roots) {
    static constexpr TopAbs_ShapeEnum topology_types[] = {
        TopAbs_COMPOUND,
        TopAbs_COMPSOLID,
        TopAbs_SOLID,
        TopAbs_SHELL,
        TopAbs_FACE,
        TopAbs_WIRE,
        TopAbs_EDGE,
        TopAbs_VERTEX,
    };
    std::vector<TopoDS_Shape> sources;
    for (const TopoDS_Shape* root : roots) {
        append_unique_shape(sources, *root);
        for (TopAbs_ShapeEnum type : topology_types) {
            for (TopExp_Explorer explorer(*root, type); explorer.More(); explorer.Next()) {
                append_unique_shape(sources, explorer.Current());
            }
        }
    }
    return sources;
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

occt_bridge_status_t store_shape_with_entries(
    occt_bridge_session_t* session,
    const TopoDS_Shape& shape,
    occt_bridge_shape_id_t* out_shape,
    std::vector<occt_bridge_history_entry> entries) {
    occt_bridge_operation_history history;
    history.entries = std::move(entries);
    const occt_bridge_status_t status = store_shape(session, shape, out_shape);
    if (status != OCCT_BRIDGE_OK) {
        return status;
    }
    try {
        session->histories.emplace(*out_shape, std::move(history));
    } catch (...) {
        session->shapes.erase(*out_shape);
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        throw;
    }
    return succeed(session);
}

/* Stores a rigidly moved shape with located history. */
occt_bridge_status_t store_located_shape(
    occt_bridge_session_t* session,
    const TopoDS_Shape& source,
    const TopLoc_Location& location,
    occt_bridge_shape_id_t* out_shape) {
    const occt_bridge_status_t status = store_shape(session, source.Moved(location), out_shape);
    if (status != OCCT_BRIDGE_OK) {
        return status;
    }
    try {
        occt_bridge_operation_history history;
        history.located_source = source;
        history.location = location;
        session->histories.emplace(*out_shape, std::move(history));
    } catch (...) {
        session->shapes.erase(*out_shape);
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        throw;
    }
    return succeed(session);
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

/*
 * Appends `replacement`, or its subshapes of `type` when healing replaced a
 * shape with a container (a face split into a compound of faces), so history
 * stays type-consistent.
 */
void append_same_type(std::vector<TopoDS_Shape>& shapes, const TopoDS_Shape& replacement, TopAbs_ShapeEnum type) {
    if (replacement.ShapeType() == type) {
        append_unique_shape(shapes, replacement);
        return;
    }
    for (TopExp_Explorer explorer(replacement, type); explorer.More(); explorer.Next()) {
        append_unique_shape(shapes, explorer.Current());
    }
}

/* Maps history targets through a healing reshape; removed targets drop out. */
void map_through_reshape(std::vector<TopoDS_Shape>& targets, const Handle(BRepTools_History)& reshape) {
    std::vector<TopoDS_Shape> mapped;
    for (const TopoDS_Shape& target : targets) {
        if (!BRepTools_History::IsSupportedType(target)) {
            append_unique_shape(mapped, target);
        } else if (!reshape->IsRemoved(target)) {
            const TopTools_ListOfShape& modified = reshape->Modified(target);
            if (modified.IsEmpty()) {
                append_unique_shape(mapped, target);
            }
            for (TopTools_ListIteratorOfListOfShape iterator(modified); iterator.More(); iterator.Next()) {
                append_same_type(mapped, iterator.Value(), target.ShapeType());
            }
        }
    }
    targets = std::move(mapped);
}

/*
 * Composes operation history with the healing that followed it, so sources
 * still lead to the faces, edges, and vertices of the healed result. A
 * source the operation left untouched but healing replaced becomes modified.
 */
void compose_with_reshape(std::vector<occt_bridge_history_entry>& history, const Handle(BRepTools_History)& reshape) {
    if (reshape.IsNull()) {
        return;
    }
    for (occt_bridge_history_entry& entry : history) {
        const bool untouched = entry.generated.empty() && entry.modified.empty() && !entry.deleted;
        if (untouched && BRepTools_History::IsSupportedType(entry.source)) {
            entry.deleted = reshape->IsRemoved(entry.source);
            entry.modified = {entry.source};
        }
        map_through_reshape(entry.generated, reshape);
        map_through_reshape(entry.modified, reshape);
        if (untouched && entry.modified.size() == 1 && entry.modified.front().IsSame(entry.source)) {
            entry.modified.clear();
        }
    }
}

/* OCCT's name for an enumeration value, or "<prefix><value>" if unknown. */
template <size_t Count>
std::string enum_name(const char* const (&names)[Count], int32_t value, const char* prefix) {
    if (value >= 0 && static_cast<size_t>(value) < Count) {
        return names[value];
    }
    return std::string(prefix) + std::to_string(value);
}

std::string fillet_status_name(ChFiDS_ErrorStatus status) {
    static const char* const names[] = {
        "ChFiDS_Ok",
        "ChFiDS_Error",
        "ChFiDS_WalkingFailure",
        "ChFiDS_StartsolFailure",
        "ChFiDS_TwistedSurface",
    };
    static_assert(ChFiDS_TwistedSurface == 4, "ChFiDS_ErrorStatus changed");
    return enum_name(names, static_cast<int32_t>(status), "ChFiDS_ErrorStatus_");
}

std::string offset_error_name(BRepOffset_Error error) {
    static const char* const names[] = {
        "BRepOffset_NoError",
        "BRepOffset_UnknownError",
        "BRepOffset_BadNormalsOnGeometry",
        "BRepOffset_C0Geometry",
        "BRepOffset_NullOffset",
        "BRepOffset_NotConnectedShell",
        "BRepOffset_CannotTrimEdges",
        "BRepOffset_CannotFuseVertices",
        "BRepOffset_CannotExtentEdge",
        "BRepOffset_UserBreak",
        "BRepOffset_MixedConnectivity",
    };
    static_assert(BRepOffset_MixedConnectivity == 10, "BRepOffset_Error changed");
    return enum_name(names, static_cast<int32_t>(error), "BRepOffset_Error_");
}

std::string check_status_name(BRepCheck_Status status) {
    static const char* const names[] = {
        "BRepCheck_NoError",
        "BRepCheck_InvalidPointOnCurve",
        "BRepCheck_InvalidPointOnCurveOnSurface",
        "BRepCheck_InvalidPointOnSurface",
        "BRepCheck_No3DCurve",
        "BRepCheck_Multiple3DCurve",
        "BRepCheck_Invalid3DCurve",
        "BRepCheck_NoCurveOnSurface",
        "BRepCheck_InvalidCurveOnSurface",
        "BRepCheck_InvalidCurveOnClosedSurface",
        "BRepCheck_InvalidSameRangeFlag",
        "BRepCheck_InvalidSameParameterFlag",
        "BRepCheck_InvalidDegeneratedFlag",
        "BRepCheck_FreeEdge",
        "BRepCheck_InvalidMultiConnexity",
        "BRepCheck_InvalidRange",
        "BRepCheck_EmptyWire",
        "BRepCheck_RedundantEdge",
        "BRepCheck_SelfIntersectingWire",
        "BRepCheck_NoSurface",
        "BRepCheck_InvalidWire",
        "BRepCheck_RedundantWire",
        "BRepCheck_IntersectingWires",
        "BRepCheck_InvalidImbricationOfWires",
        "BRepCheck_EmptyShell",
        "BRepCheck_RedundantFace",
        "BRepCheck_InvalidImbricationOfShells",
        "BRepCheck_UnorientableShape",
        "BRepCheck_NotClosed",
        "BRepCheck_NotConnected",
        "BRepCheck_SubshapeNotInShape",
        "BRepCheck_BadOrientation",
        "BRepCheck_BadOrientationOfSubshape",
        "BRepCheck_InvalidPolygonOnTriangulation",
        "BRepCheck_InvalidToleranceValue",
        "BRepCheck_EnclosedRegion",
        "BRepCheck_CheckFail",
    };
    static_assert(BRepCheck_CheckFail == 36, "BRepCheck_Status changed");
    return enum_name(names, static_cast<int32_t>(status), "BRepCheck_Status_");
}

const char* shape_type_noun(TopAbs_ShapeEnum type) {
    switch (type) {
        case TopAbs_COMPOUND: return "compound";
        case TopAbs_COMPSOLID: return "compsolid";
        case TopAbs_SOLID: return "solid";
        case TopAbs_SHELL: return "shell";
        case TopAbs_FACE: return "face";
        case TopAbs_WIRE: return "wire";
        case TopAbs_EDGE: return "edge";
        case TopAbs_VERTEX: return "vertex";
        default: return "shape";
    }
}

/*
 * Appends a summary of the call's diagnostics to a failure message, such as
 * "fillet construction failed: ChFiDS_StartsolFailure on selection 0".
 */
std::string with_diagnostics(const occt_bridge_session_t* session, std::string message) {
    const std::vector<occt_bridge_diagnostic_record>& diagnostics = session->last_diagnostics;
    if (diagnostics.empty()) {
        return message;
    }
    const occt_bridge_diagnostic_record& first = diagnostics.front();
    message += ": " + (first.name.empty() ? std::string("failure") : first.name);
    if (first.input_index >= 0) {
        message += first.kind == OCCT_BRIDGE_DIAGNOSTIC_BOOLEAN_ALERT ? " on operand " : " on selection ";
        message += std::to_string(first.input_index);
    } else if (!first.shape.IsNull()) {
        message += std::string(" on a ") + shape_type_noun(first.shape.ShapeType());
    }
    const size_t more = diagnostics.size() - 1 + session->omitted_diagnostics;
    if (more != 0) {
        message += " and " + std::to_string(more) + " more";
    }
    return message;
}

/*
 * Records every validation failure of `shape`, outermost subshapes first,
 * with BRepCheck's status for the subshape alone and within its parents.
 * O(subshapes); runs only for results being rejected.
 */
void record_invalid_subshapes(
    occt_bridge_session_t* session,
    const ShapeValidator& analyzer,
    const TopoDS_Shape& shape) {
    static const TopAbs_ShapeEnum types[] = {
        TopAbs_COMPSOLID, TopAbs_SOLID, TopAbs_SHELL, TopAbs_FACE, TopAbs_WIRE, TopAbs_EDGE, TopAbs_VERTEX,
    };
    for (const TopAbs_ShapeEnum type : types) {
        TopTools_IndexedMapOfShape subshapes;
        TopExp::MapShapes(shape, type, subshapes);
        for (int index = 1; index <= subshapes.Extent(); ++index) {
            const Handle(BRepCheck_Result)& result = analyzer.Result(subshapes(index));
            if (result.IsNull()) {
                continue;
            }
            std::vector<BRepCheck_Status> statuses;
            const auto collect = [&statuses](const BRepCheck_ListOfStatus& list) {
                for (BRepCheck_ListIteratorOfListOfStatus status(list); status.More(); status.Next()) {
                    if (status.Value() != BRepCheck_NoError
                        && std::find(statuses.begin(), statuses.end(), status.Value()) == statuses.end()) {
                        statuses.push_back(status.Value());
                    }
                }
            };
            collect(result->Status());
            for (result->InitContextIterator(); result->MoreShapeInContext(); result->NextShapeInContext()) {
                collect(result->StatusOnShape());
            }
            for (const BRepCheck_Status status : statuses) {
                add_diagnostic(
                    session,
                    {OCCT_BRIDGE_DIAGNOSTIC_INVALID_SUBSHAPE,
                     static_cast<int32_t>(status),
                     -1,
                     check_status_name(status),
                     subshapes(index)});
            }
        }
    }
}

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
    std::vector<occt_bridge_history_entry>* history) {
    if (session->options.validate_results == 0) {
        return OCCT_BRIDGE_OK;
    }
    const ShapeValidator analyzer(shape);
    if (analyzer.IsValid()) {
        return OCCT_BRIDGE_OK;
    }
    if (session->options.heal_invalid_results == 0) {
        record_invalid_subshapes(session, analyzer, shape);
        return fail(
            session,
            OCCT_BRIDGE_INVALID_GEOMETRY,
            with_diagnostics(session, operation + " produced an invalid shape"));
    }
    Handle(ShapeFix_Shape) fixer = new ShapeFix_Shape(shape);
    fixer->Perform();
    const TopoDS_Shape healed = fixer->Shape();
    if (healed.IsNull() || !ShapeValidator(healed).IsValid()) {
        record_invalid_subshapes(session, analyzer, shape);
        return fail(
            session,
            OCCT_BRIDGE_INVALID_GEOMETRY,
            with_diagnostics(session, operation + " produced an invalid shape that healing could not repair"));
    }
    if (history != nullptr) {
        compose_with_reshape(*history, fixer->Context()->History());
    }
    shape = healed;
    add_warning(session, operation + " result was invalid and was healed");
    return OCCT_BRIDGE_OK;
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

/* Checks, optionally heals, and stores an imported shape. */
occt_bridge_status_t store_checked_import(
    occt_bridge_session_t* session,
    const std::string& name,
    TopoDS_Shape shape,
    occt_bridge_shape_id_t* out_shape) {
    const occt_bridge_status_t status = check_result(session, name, shape, nullptr);
    if (status != OCCT_BRIDGE_OK) {
        return status;
    }
    return store_shape(session, shape, out_shape);
}

bool belongs_to(const TopoDS_Shape& parent, const TopoDS_Shape& candidate);

/*
 * Records a failed boolean's alerts, errors first, with the operand each
 * alert's shape belongs to. O(alerts * operand subshapes) on failure only.
 */
template <typename Operation>
void record_boolean_alerts(
    occt_bridge_session_t* session,
    const Operation& operation,
    const TopoDS_Shape& left,
    const TopoDS_Shape& right) {
    for (const Message_Gravity gravity : {Message_Fail, Message_Warning}) {
        const Message_ListOfAlert& alerts = operation.GetReport()->GetAlerts(gravity);
        for (Message_ListOfAlert::Iterator alert(alerts); alert.More(); alert.Next()) {
            TopoDS_Shape shape;
            const Handle(TopoDS_AlertWithShape) with_shape = Handle(TopoDS_AlertWithShape)::DownCast(alert.Value());
            if (!with_shape.IsNull()) {
                shape = with_shape->GetShape();
            }
            int64_t operand = -1;
            if (!shape.IsNull()) {
                operand = belongs_to(left, shape) ? 0 : (belongs_to(right, shape) ? 1 : -1);
            }
            add_diagnostic(
                session,
                {OCCT_BRIDGE_DIAGNOSTIC_BOOLEAN_ALERT, 0, operand, alert.Value()->GetMessageKey(), shape});
        }
    }
}

template <typename Operation>
occt_bridge_status_t boolean_operation(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t left,
    occt_bridge_shape_id_t right,
    occt_bridge_shape_id_t* out_shape,
    const char* operation_name) {
    if (out_shape == nullptr) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
    }
    *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    const TopoDS_Shape* left_shape = find_shape(session, left);
    const TopoDS_Shape* right_shape = find_shape(session, right);
    if (left_shape == nullptr || right_shape == nullptr) {
        return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "boolean input shape was not found");
    }
    Operation operation;
    TopTools_ListOfShape arguments;
    TopTools_ListOfShape tools;
    arguments.Append(*left_shape);
    tools.Append(*right_shape);
    operation.SetArguments(arguments);
    operation.SetTools(tools);
    if (session->options.boolean_fuzzy_tolerance > 0.0) {
        operation.SetFuzzyValue(session->options.boolean_fuzzy_tolerance);
    }
    operation.Build();
    if (operation.HasWarnings()) {
        std::ostringstream warnings;
        operation.DumpWarnings(warnings);
        std::istringstream lines(warnings.str());
        for (std::string line; std::getline(lines, line);) {
            if (!line.empty()) {
                add_warning(session, std::string(operation_name) + ": " + line);
            }
        }
    }
    if (!operation.IsDone() || operation.HasErrors()) {
        record_boolean_alerts(session, operation, *left_shape, *right_shape);
        return fail(
            session,
            OCCT_BRIDGE_KERNEL_ERROR,
            with_diagnostics(session, std::string(operation_name) + " operation failed"));
    }
    return store_checked_result(
        session,
        operation_name,
        operation.Shape(),
        out_shape,
        operation,
        {left_shape, right_shape});
}

occt_bridge_status_t transformed_shape(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const gp_Trsf& transform,
    occt_bridge_shape_id_t* out_shape) {
    if (out_shape == nullptr) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
    }
    *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    const TopoDS_Shape* value = find_shape(session, shape);
    if (value == nullptr) {
        return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
    }
    /*
     * Rigid moves (translation, rotation) only attach a location, so placed
     * copies share one geometry instead of each owning a deep copy. Scaling
     * cannot be expressed as a location and still copies geometry.
     */
    const bool rigid = std::abs(std::abs(transform.ScaleFactor()) - 1.0) <= Precision::Confusion();
    if (rigid) {
        return store_located_shape(session, *value, TopLoc_Location(transform), out_shape);
    }
    BRepBuilderAPI_Transform operation(*value, transform, Standard_True);
    operation.Build();
    if (!operation.IsDone() || operation.Shape().IsNull()) {
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "shape transform failed");
    }
    return store_shape_with_history(
        session,
        operation.Shape(),
        out_shape,
        operation,
        {value});
}

bool bridge_shape_type(occt_bridge_shape_type_t type, TopAbs_ShapeEnum& out_type) {
    switch (type) {
        case OCCT_BRIDGE_SHAPE_COMPOUND: out_type = TopAbs_COMPOUND; return true;
        case OCCT_BRIDGE_SHAPE_COMPSOLID: out_type = TopAbs_COMPSOLID; return true;
        case OCCT_BRIDGE_SHAPE_SOLID: out_type = TopAbs_SOLID; return true;
        case OCCT_BRIDGE_SHAPE_SHELL: out_type = TopAbs_SHELL; return true;
        case OCCT_BRIDGE_SHAPE_FACE: out_type = TopAbs_FACE; return true;
        case OCCT_BRIDGE_SHAPE_WIRE: out_type = TopAbs_WIRE; return true;
        case OCCT_BRIDGE_SHAPE_EDGE: out_type = TopAbs_EDGE; return true;
        case OCCT_BRIDGE_SHAPE_VERTEX: out_type = TopAbs_VERTEX; return true;
        default: return false;
    }
}

occt_bridge_shape_type_t public_shape_type(TopAbs_ShapeEnum type) {
    switch (type) {
        case TopAbs_COMPOUND: return OCCT_BRIDGE_SHAPE_COMPOUND;
        case TopAbs_COMPSOLID: return OCCT_BRIDGE_SHAPE_COMPSOLID;
        case TopAbs_SOLID: return OCCT_BRIDGE_SHAPE_SOLID;
        case TopAbs_SHELL: return OCCT_BRIDGE_SHAPE_SHELL;
        case TopAbs_FACE: return OCCT_BRIDGE_SHAPE_FACE;
        case TopAbs_WIRE: return OCCT_BRIDGE_SHAPE_WIRE;
        case TopAbs_EDGE: return OCCT_BRIDGE_SHAPE_EDGE;
        case TopAbs_VERTEX: return OCCT_BRIDGE_SHAPE_VERTEX;
        default: return 0;
    }
}

TopTools_IndexedMapOfShape descendant_shapes(
    const TopoDS_Shape& shape,
    TopAbs_ShapeEnum type) {
    TopTools_IndexedMapOfShape result;
    for (TopExp_Explorer explorer(shape, type); explorer.More(); explorer.Next()) {
        result.Add(explorer.Current());
    }
    return result;
}

bool contains_topology(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
    return shape.ShapeType() == type || TopExp_Explorer(shape, type).More();
}

bool is_descendant(
    const TopoDS_Shape& parent,
    const TopoDS_Shape& candidate,
    TopAbs_ShapeEnum type) {
    for (TopExp_Explorer explorer(parent, type); explorer.More(); explorer.Next()) {
        if (explorer.Current().IsSame(candidate)) {
            return true;
        }
    }
    return false;
}

bool belongs_to(const TopoDS_Shape& parent, const TopoDS_Shape& candidate) {
    return parent.IsSame(candidate) || is_descendant(parent, candidate, candidate.ShapeType());
}

bool shares_descendant(
    const TopoDS_Shape& first,
    const TopoDS_Shape& second,
    TopAbs_ShapeEnum type) {
    for (TopExp_Explorer explorer(first, type); explorer.More(); explorer.Next()) {
        if (is_descendant(second, explorer.Current(), type)) {
            return true;
        }
    }
    return false;
}

/* Runs a kernel step, returning the OCCT exception's type name or "". */
template <typename Step>
std::string perform_reporting_exception(Step&& step) {
    try {
        step();
    } catch (const Standard_Failure& error) {
        return error.DynamicType()->Name();
    }
    return {};
}

/*
 * Fails an offset or hollow with OCCT's offset error code and the input
 * subshape it blamed, mapped to the selected face it matches if any.
 */
occt_bridge_status_t offset_failure(
    occt_bridge_session_t* session,
    const BRepOffsetAPI_MakeOffsetShape& builder,
    const std::string& name,
    const std::string& exception,
    const TopTools_ListOfShape& selection) {
    const BRepOffset_MakeOffset& offset = builder.MakeOffset();
    const BRepOffset_Error error = offset.Error();
    if (error != BRepOffset_NoError) {
        const TopoDS_Shape& bad = offset.GetBadShape();
        int64_t selected = -1;
        int64_t index = 0;
        for (TopTools_ListIteratorOfListOfShape face(selection); face.More(); face.Next(), ++index) {
            if (!bad.IsNull() && face.Value().IsSame(bad)) {
                selected = index;
                break;
            }
        }
        add_diagnostic(
            session,
            {OCCT_BRIDGE_DIAGNOSTIC_OFFSET, static_cast<int32_t>(error), selected, offset_error_name(error), bad});
    }
    std::string message = name + " construction failed";
    if (!exception.empty()) {
        message += " (" + exception + ")";
    }
    return fail(session, OCCT_BRIDGE_KERNEL_ERROR, with_diagnostics(session, message));
}

/*
 * Builds a fillet or chamfer and reports OCCT exceptions as a failed build:
 * returns the exception's type name, or an empty string. Partial builders
 * stay inspectable after either kind of failure.
 */
template <typename Builder>
std::string build_reporting_exception(Builder& builder) {
    return perform_reporting_exception([&] { builder.Build(); });
}

/*
 * Selected edge indices per contour, in selection order; contour 0 holds
 * edges OCCT placed in no contour. O(selected edges).
 */
std::vector<std::vector<size_t>> group_by_contour(const std::vector<int>& contours) {
    const int largest = contours.empty() ? 0 : *std::max_element(contours.begin(), contours.end());
    std::vector<std::vector<size_t>> members(static_cast<size_t>(std::max(largest, 0)) + 1);
    for (size_t index = 0; index < contours.size(); ++index) {
        if (contours[index] > 0) {
            members[static_cast<size_t>(contours[index])].push_back(index);
        }
    }
    return members;
}

/*
 * Records the faulty contours and vertices OCCT reports for a failed fillet.
 * `members` lists each contour's selected edges, read before building
 * because a build that throws forgets them. O(faulty contours + their edges).
 */
void record_faulty_fillet(
    occt_bridge_session_t* session,
    const BRepFilletAPI_MakeFillet& builder,
    const std::vector<TopoDS_Edge>& selection,
    const std::vector<std::vector<size_t>>& members) {
    for (int faulty = 1; faulty <= builder.NbFaultyContours(); ++faulty) {
        const int contour = builder.FaultyContour(faulty);
        if (contour <= 0 || static_cast<size_t>(contour) >= members.size()) {
            continue;
        }
        const ChFiDS_ErrorStatus status = builder.StripeStatus(contour);
        for (const size_t index : members[static_cast<size_t>(contour)]) {
            add_diagnostic(
                session,
                {OCCT_BRIDGE_DIAGNOSTIC_FILLET_EDGE,
                 static_cast<int32_t>(status),
                 static_cast<int64_t>(index),
                 fillet_status_name(status),
                 selection[index]});
        }
    }
    for (int vertex = 1; vertex <= builder.NbFaultyVertices(); ++vertex) {
        add_diagnostic(
            session,
            {OCCT_BRIDGE_DIAGNOSTIC_FILLET_VERTEX, 0, -1, "", builder.FaultyVertex(vertex)});
    }
}

void record_faulty_fillet(
    occt_bridge_session_t*,
    const BRepFilletAPI_MakeChamfer&,
    const std::vector<TopoDS_Edge>&,
    const std::vector<std::vector<size_t>>&) {
    /* OCCT reports no faulty contours for chamfers; isolation finds them. */
}

/* Contours rebuilt alone to locate a failure; bounds the work on failure. */
constexpr size_t MAX_ISOLATED_CONTOURS = 64;

/*
 * Rebuilds each contour of a failed fillet or chamfer on its own and records
 * the selected edges of contours that still fail. Costs one local build per
 * contour, at most MAX_ISOLATED_CONTOURS, and runs only after a failure.
 */
template <typename Builder>
void record_isolated_contours(
    occt_bridge_session_t* session,
    const TopoDS_Shape& shape,
    const std::vector<TopoDS_Edge>& selection,
    const std::vector<std::vector<size_t>>& members,
    double value) {
    const auto selected_contours = static_cast<size_t>(std::count_if(
        members.begin(), members.end(), [](const std::vector<size_t>& edges) { return !edges.empty(); }));
    if (selected_contours > MAX_ISOLATED_CONTOURS) {
        add_warning(
            session,
            "located failures in the first " + std::to_string(MAX_ISOLATED_CONTOURS) + " of "
                + std::to_string(selected_contours) + " contours");
    }
    size_t rebuilt = 0;
    for (size_t contour = 1; contour < members.size() && rebuilt < MAX_ISOLATED_CONTOURS; ++contour) {
        if (members[contour].empty()) {
            continue;
        }
        ++rebuilt;
        Builder alone(shape);
        for (const size_t index : members[contour]) {
            alone.Add(value, selection[index]);
        }
        const std::string exception = build_reporting_exception(alone);
        if (exception.empty() && alone.IsDone() && !alone.Shape().IsNull()) {
            continue;
        }
        for (const size_t index : members[contour]) {
            add_diagnostic(
                session,
                {OCCT_BRIDGE_DIAGNOSTIC_ISOLATED_EDGE, 0, static_cast<int64_t>(index), exception, selection[index]});
        }
    }
}

/*
 * Fillets or chamfers selected edges. On failure, records the faulty
 * contours OCCT reports or, when it names none, the contours that fail on
 * their own, so the error says which selected edges are at fault.
 */
template <typename Builder>
occt_bridge_status_t edge_treatment(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* edges,
    size_t edge_count,
    double value,
    occt_bridge_shape_id_t* out_shape,
    const std::string& name) {
    if (out_shape == nullptr) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
    }
    *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    if (edges == nullptr || edge_count == 0 || !std::isfinite(value) || value <= 0.0) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid " + name + " parameters");
    }
    const TopoDS_Shape* input = find_shape(session, shape);
    if (input == nullptr) {
        return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
    }
    std::vector<TopoDS_Edge> selection;
    selection.reserve(edge_count);
    Builder builder(*input);
    for (size_t index = 0; index < edge_count; ++index) {
        const TopoDS_Shape* edge = find_shape(session, edges[index]);
        if (edge == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, name + " edge was not found");
        }
        if (edge->ShapeType() != TopAbs_EDGE || !is_descendant(*input, *edge, TopAbs_EDGE)) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_GEOMETRY,
                name + " selection is not an edge of the input shape");
        }
        selection.push_back(TopoDS::Edge(*edge));
        builder.Add(value, selection.back());
    }
    std::vector<int> contours(selection.size());
    std::transform(selection.begin(), selection.end(), contours.begin(), [&builder](const TopoDS_Edge& edge) {
        return builder.Contour(edge);
    });
    const std::string exception = build_reporting_exception(builder);
    if (!exception.empty() || !builder.IsDone() || builder.Shape().IsNull()) {
        const std::vector<std::vector<size_t>> members = group_by_contour(contours);
        record_faulty_fillet(session, builder, selection, members);
        if (session->last_diagnostics.empty()) {
            record_isolated_contours<Builder>(session, *input, selection, members, value);
        }
        std::string message = name + " construction failed";
        if (!exception.empty()) {
            message += " (" + exception + ")";
        }
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, with_diagnostics(session, message));
    }
    return store_checked_result(session, name, builder.Shape(), out_shape, builder, {input});
}


/*
 * Resolves `source` against a result's history. For a rigid move a source
 * subshape is modified into itself moved by the location (O(source size) to
 * confirm membership); explicit records are searched directly.
 */
bool resolve_history_entry(
    const occt_bridge_operation_history& history,
    const TopoDS_Shape& source,
    occt_bridge_history_entry& out_entry) {
    if (!history.located_source.IsNull()) {
        if (!belongs_to(history.located_source, source)) {
            return false;
        }
        out_entry = occt_bridge_history_entry{source, {}, {source.Moved(history.location)}, false};
        return true;
    }
    const auto found = std::find_if(
        history.entries.begin(),
        history.entries.end(),
        [&](const occt_bridge_history_entry& entry) { return entry.source.IsSame(source); });
    if (found == history.entries.end()) {
        return false;
    }
    out_entry = *found;
    return true;
}

const std::vector<TopoDS_Shape>* history_relation_shapes(
    const occt_bridge_history_entry& entry,
    occt_bridge_history_relation_t relation) {
    switch (relation) {
        case OCCT_BRIDGE_HISTORY_GENERATED: return &entry.generated;
        case OCCT_BRIDGE_HISTORY_MODIFIED: return &entry.modified;
        default: return nullptr;
    }
}

TopoDS_Face triangle_face(const gp_Pnt& first, const gp_Pnt& second, const gp_Pnt& third) {
    BRepBuilderAPI_MakePolygon wire;
    wire.Add(first);
    wire.Add(second);
    wire.Add(third);
    wire.Close();
    if (!wire.IsDone()) {
        return {};
    }
    BRepBuilderAPI_MakeFace face(wire.Wire());
    return face.IsDone() ? face.Face() : TopoDS_Face{};
}

TopoDS_Face polygon_face(const std::vector<gp_Pnt>& points) {
    BRepBuilderAPI_MakePolygon wire;
    for (const gp_Pnt& point : points) {
        wire.Add(point);
    }
    wire.Close();
    if (!wire.IsDone()) {
        return {};
    }
    BRepBuilderAPI_MakeFace face(wire.Wire());
    return face.IsDone() ? face.Face() : TopoDS_Face{};
}

TopoDS_Wire polygon_wire(const occt_bridge_vec3_t* points, size_t point_count) {
    BRepBuilderAPI_MakePolygon wire;
    for (size_t index = 0; index < point_count; ++index) {
        wire.Add(gp_Pnt(points[index].x, points[index].y, points[index].z));
    }
    wire.Close();
    return wire.IsDone() ? wire.Wire() : TopoDS_Wire{};
}

TopoDS_Shape cylinder_between(const gp_Pnt& start, const gp_Pnt& end, double radius) {
    const gp_Vec axis(start, end);
    const double length = axis.Magnitude();
    if (length <= std::numeric_limits<double>::epsilon()) {
        return {};
    }
    BRepPrimAPI_MakeCylinder cylinder(gp_Ax2(start, gp_Dir(axis)), radius, length);
    cylinder.Build();
    return cylinder.IsDone() ? cylinder.Shape() : TopoDS_Shape{};
}

bool add_ring_band(
    BRepBuilderAPI_Sewing& sewing,
    const std::vector<gp_Pnt>& lower,
    const std::vector<gp_Pnt>& upper) {
    for (size_t index = 0; index < lower.size(); ++index) {
        const size_t next = (index + 1) % lower.size();
        const TopoDS_Face first = triangle_face(lower[index], lower[next], upper[next]);
        const TopoDS_Face second = triangle_face(lower[index], upper[next], upper[index]);
        if (first.IsNull() || second.IsNull()) {
            return false;
        }
        sewing.Add(first);
        sewing.Add(second);
    }
    return true;
}

/* Insets the base ring toward its centroid and raises the side ring by the chamfer. */
bool chamfer_rings(
    const std::vector<gp_Pnt>& bottom,
    double chamfer,
    std::vector<gp_Pnt>& base,
    std::vector<gp_Pnt>& lower_side) {
    double center_x = 0.0;
    double center_y = 0.0;
    for (const gp_Pnt& point : bottom) {
        center_x += point.X();
        center_y += point.Y();
    }
    center_x /= static_cast<double>(bottom.size());
    center_y /= static_cast<double>(bottom.size());
    for (size_t index = 0; index < bottom.size(); ++index) {
        const double dx = center_x - bottom[index].X();
        const double dy = center_y - bottom[index].Y();
        const double length = std::hypot(dx, dy);
        if (length <= chamfer) {
            return false;
        }
        base[index].SetX(bottom[index].X() + chamfer * dx / length);
        base[index].SetY(bottom[index].Y() + chamfer * dy / length);
        lower_side[index].SetZ(bottom[index].Z() + chamfer);
    }
    return true;
}

/* Quarter-round rings from the side wall into the top ring, or the top ring alone. */
bool shoulder_rings(
    const std::vector<gp_Pnt>& top,
    const gp_Pnt& top_center,
    double fillet,
    std::vector<std::vector<gp_Pnt>>& rings) {
    if (fillet <= 0.0) {
        rings.push_back(top);
        return true;
    }
    constexpr size_t segments = 5;
    constexpr double half_pi = 1.57079632679489661923;
    rings.reserve(segments + 1);
    for (size_t step = 0; step <= segments; ++step) {
        const double angle = half_pi * static_cast<double>(step) / static_cast<double>(segments);
        const double horizontal_offset = fillet * std::cos(angle);
        const double vertical_offset = fillet * (1.0 - std::sin(angle));
        std::vector<gp_Pnt> ring;
        ring.reserve(top.size());
        for (const gp_Pnt& point : top) {
            const double dx = point.X() - top_center.X();
            const double dy = point.Y() - top_center.Y();
            const double length = std::hypot(dx, dy);
            if (length <= fillet) {
                return false;
            }
            ring.emplace_back(
                point.X() + horizontal_offset * dx / length,
                point.Y() + horizontal_offset * dy / length,
                point.Z() - vertical_offset);
        }
        rings.push_back(std::move(ring));
    }
    return true;
}

/* Fans the crown ring into triangles meeting at the top center. */
bool add_crown(BRepBuilderAPI_Sewing& sewing, const std::vector<gp_Pnt>& crown_edge, const gp_Pnt& top_center) {
    for (size_t index = 0; index < crown_edge.size(); ++index) {
        const size_t next = (index + 1) % crown_edge.size();
        const TopoDS_Face top_face = triangle_face(crown_edge[index], crown_edge[next], top_center);
        if (top_face.IsNull()) {
            return false;
        }
        sewing.Add(top_face);
    }
    return true;
}

/* The single shell of a sewing result, or a null shell when there is not exactly one. */
TopoDS_Shell single_shell(const TopoDS_Shape& sewed) {
    if (sewed.ShapeType() == TopAbs_SHELL) {
        return TopoDS::Shell(sewed);
    }
    TopExp_Explorer shells(sewed, TopAbs_SHELL);
    if (!shells.More()) {
        return {};
    }
    const TopoDS_Shell shell = TopoDS::Shell(shells.Current());
    shells.Next();
    return shells.More() ? TopoDS_Shell() : shell;
}

TopoDS_Shape build_faceted_solid(
    const std::vector<gp_Pnt>& bottom,
    const std::vector<gp_Pnt>& top,
    const gp_Pnt& top_center,
    double bottom_chamfer,
    double top_fillet) {
    BRepBuilderAPI_Sewing sewing(1.0e-6, Standard_True, Standard_True, Standard_True, Standard_False);

    std::vector<gp_Pnt> base = bottom;
    std::vector<gp_Pnt> lower_side = bottom;
    if (bottom_chamfer > 0.0 && !chamfer_rings(bottom, bottom_chamfer, base, lower_side)) {
        return {};
    }

    TopoDS_Face bottom_face = polygon_face(base);
    if (bottom_face.IsNull()) {
        return {};
    }
    sewing.Add(bottom_face);

    // NOLINTNEXTLINE(readability-suspicious-call-argument): base is the lower ring; lower_side is the raised upper ring.
    if (bottom_chamfer > 0.0 && !add_ring_band(sewing, base, lower_side)) {
        return {};
    }

    std::vector<std::vector<gp_Pnt>> rings;
    if (!shoulder_rings(top, top_center, top_fillet, rings)
        || !add_ring_band(sewing, lower_side, rings.front())) {
        return {};
    }
    for (size_t index = 1; index < rings.size(); ++index) {
        if (!add_ring_band(sewing, rings[index - 1], rings[index])) {
            return {};
        }
    }
    if (!add_crown(sewing, rings.back(), top_center)) {
        return {};
    }

    sewing.Perform();
    const TopoDS_Shell shell = single_shell(sewing.SewedShape());
    if (shell.IsNull()) {
        return {};
    }
    BRepBuilderAPI_MakeSolid solid_builder(shell);
    if (!solid_builder.IsDone()) {
        return {};
    }
    TopoDS_Solid solid = solid_builder.Solid();
    BRepLib::OrientClosedSolid(solid);
    return solid;
}

}  // namespace

extern "C" {

uint32_t occt_bridge_abi_version(void) {
    return OCCT_BRIDGE_ABI_VERSION;
}

const char* occt_bridge_status_string(occt_bridge_status_t status) {
    switch (status) {
        case OCCT_BRIDGE_OK: return "ok";
        case OCCT_BRIDGE_INVALID_ARGUMENT: return "invalid argument";
        case OCCT_BRIDGE_UNSUPPORTED_ABI: return "unsupported ABI version";
        case OCCT_BRIDGE_SHAPE_NOT_FOUND: return "shape not found";
        case OCCT_BRIDGE_INVALID_GEOMETRY: return "invalid geometry";
        case OCCT_BRIDGE_IO_ERROR: return "I/O error";
        case OCCT_BRIDGE_KERNEL_ERROR: return "Open Cascade kernel error";
        case OCCT_BRIDGE_ALLOCATION_FAILED: return "allocation failed";
        case OCCT_BRIDGE_INTERNAL_ERROR: return "internal error";
        default: return "unknown status";
    }
}

occt_bridge_status_t occt_bridge_session_create(
    uint32_t requested_abi_version,
    occt_bridge_session_t** out_session) {
    if (out_session == nullptr) {
        return OCCT_BRIDGE_INVALID_ARGUMENT;
    }
    *out_session = nullptr;
    if (requested_abi_version != OCCT_BRIDGE_ABI_VERSION) {
        return OCCT_BRIDGE_UNSUPPORTED_ABI;
    }
    try {
        *out_session = new (std::nothrow) occt_bridge_session_t();
        return *out_session == nullptr ? OCCT_BRIDGE_ALLOCATION_FAILED : OCCT_BRIDGE_OK;
    } catch (...) {
        return OCCT_BRIDGE_INTERNAL_ERROR;
    }
}

void occt_bridge_session_destroy(occt_bridge_session_t* session) {
    try {
        delete session;
    } catch (...) {  // NOLINT(bugprone-empty-catch): no exception may cross the C ABI and there is no status to report.
    }
}

occt_bridge_status_t occt_bridge_session_clear(occt_bridge_session_t* session) {
    return guarded(session, [&] {
        session->shapes.clear();
        session->histories.clear();
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_session_shape_count(
    occt_bridge_session_t* session,
    size_t* out_count) {
    return guarded(session, [&] {
        if (out_count == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_count is null");
        }
        *out_count = session->shapes.size();
        return succeed(session);
    });
}

size_t occt_bridge_session_last_error(
    const occt_bridge_session_t* session,
    char* buffer,
    size_t buffer_capacity) {
    if (session == nullptr) {
        return 0;
    }
    try {
        std::lock_guard<std::mutex> lock(session->mutex);
        return copy_text(session->last_error, buffer, buffer_capacity);
    } catch (...) {
        return 0;
    }
}

size_t occt_bridge_session_last_warnings(
    const occt_bridge_session_t* session,
    char* buffer,
    size_t buffer_capacity) {
    if (session == nullptr) {
        return 0;
    }
    try {
        std::lock_guard<std::mutex> lock(session->mutex);
        return copy_text(session->last_warnings, buffer, buffer_capacity);
    } catch (...) {
        return 0;
    }
}

occt_bridge_status_t occt_bridge_session_diagnostic_count(
    occt_bridge_session_t* session,
    size_t* out_count) {
    return guarded(
        session,
        [&] {
            if (out_count == nullptr) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_count is null");
            }
            *out_count = session->last_diagnostics.size();
            return occt_bridge_status_t{OCCT_BRIDGE_OK};
        },
        true);
}

occt_bridge_status_t occt_bridge_session_diagnostic_at(
    occt_bridge_session_t* session,
    size_t index,
    occt_bridge_diagnostic_t* out_diagnostic,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(
        session,
        [&] {
            if (out_diagnostic == nullptr) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_diagnostic is null");
            }
            if (out_shape != nullptr) {
                *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
            }
            if (index >= session->last_diagnostics.size()) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "diagnostic index is out of range");
            }
            const occt_bridge_diagnostic_record& record = session->last_diagnostics[index];
            *out_diagnostic = {record.kind, record.code, record.input_index, record.shape.IsNull() ? 0 : 1};
            if (out_shape != nullptr && !record.shape.IsNull()) {
                /* Storing succeeds by clearing the last error; keep the failure's. */
                std::string error = session->last_error;
                const occt_bridge_status_t status = store_shape(session, record.shape, out_shape);
                if (status == OCCT_BRIDGE_OK) {
                    session->last_error = std::move(error);
                }
                return status;
            }
            return occt_bridge_status_t{OCCT_BRIDGE_OK};
        },
        true);
}

size_t occt_bridge_session_diagnostic_name(
    const occt_bridge_session_t* session,
    size_t index,
    char* buffer,
    size_t buffer_capacity) {
    if (session == nullptr) {
        return 0;
    }
    try {
        std::lock_guard<std::mutex> lock(session->mutex);
        if (index >= session->last_diagnostics.size()) {
            return 0;
        }
        return copy_text(session->last_diagnostics[index].name, buffer, buffer_capacity);
    } catch (...) {
        return 0;
    }
}

occt_bridge_status_t occt_bridge_session_get_options(
    occt_bridge_session_t* session,
    occt_bridge_session_options_t* out_options) {
    return guarded(session, [&] {
        if (out_options == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_options is null");
        }
        *out_options = session->options;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_session_set_options(
    occt_bridge_session_t* session,
    const occt_bridge_session_options_t* options) {
    return guarded(session, [&] {
        if (options == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "options is null");
        }
        const auto is_flag = [](int value) { return value == 0 || value == 1; };
        if (!is_flag(options->validate_results) || !is_flag(options->heal_invalid_results)
            || !std::isfinite(options->boolean_fuzzy_tolerance) || options->boolean_fuzzy_tolerance < 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid session options");
        }
        if (options->heal_invalid_results == 1 && options->validate_results == 0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "healing requires result validation");
        }
        session->options = *options;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_create_box(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t size,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(origin) || !finite(size) || size.x <= 0.0 || size.y <= 0.0 || size.z <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "box size must be finite and positive");
        }
        BRepPrimAPI_MakeBox builder(
            gp_Pnt(origin.x, origin.y, origin.z), size.x, size.y, size.z);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "box construction failed");
        }
        return store_shape(session, builder.Shape(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_cylinder(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis,
    double radius,
    double height,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(origin) || !finite(axis) || !std::isfinite(radius) || !std::isfinite(height)
            || radius <= 0.0 || height <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid cylinder parameters");
        }
        const gp_Vec direction(axis.x, axis.y, axis.z);
        if (direction.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "cylinder axis must be nonzero");
        }
        BRepPrimAPI_MakeCylinder builder(
            gp_Ax2(gp_Pnt(origin.x, origin.y, origin.z), gp_Dir(direction)), radius, height);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "cylinder construction failed");
        }
        return store_shape(session, builder.Shape(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_cone(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t axis,
    double base_radius,
    double top_radius,
    double height,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(origin) || !finite(axis) || !std::isfinite(base_radius)
            || !std::isfinite(top_radius) || !std::isfinite(height)
            || base_radius < 0.0 || top_radius < 0.0
            || (base_radius == 0.0 && top_radius == 0.0) || height <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid cone parameters");
        }
        const gp_Vec direction(axis.x, axis.y, axis.z);
        if (direction.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "cone axis must be nonzero");
        }
        BRepPrimAPI_MakeCone builder(
            gp_Ax2(gp_Pnt(origin.x, origin.y, origin.z), gp_Dir(direction)),
            base_radius,
            top_radius,
            height);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "cone construction failed");
        }
        return store_shape(session, builder.Shape(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_sphere(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t center,
    double radius,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(center) || !std::isfinite(radius) || radius <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid sphere parameters");
        }
        BRepPrimAPI_MakeSphere builder(gp_Pnt(center.x, center.y, center.z), radius);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "sphere construction failed");
        }
        return store_shape(session, builder.Shape(), out_shape);
    });
}

}  // extern "C"

namespace {

const char* wire_segment_error(const occt_bridge_wire_segment_t& segment) {
    if ((segment.kind != 0 && segment.kind != 1) || !finite(segment.start)
        || !finite(segment.end) || (segment.kind == 1 && !finite(segment.middle))) {
        return "invalid wire segment";
    }
    if (to_point(segment.start).Distance(to_point(segment.end)) <= Precision::Confusion()) {
        return "wire segment endpoints coincide";
    }
    return nullptr;
}

const char* add_wire_segment(BRepBuilderAPI_MakeWire& wire,
                             const occt_bridge_wire_segment_t& segment) {
    const gp_Pnt start = to_point(segment.start);
    const gp_Pnt end = to_point(segment.end);
    if (segment.kind == 1) {
        GC_MakeArcOfCircle arc(start, to_point(segment.middle), end);
        if (!arc.IsDone()) {
            return "invalid circular arc";
        }
        BRepBuilderAPI_MakeEdge edge(arc.Value());
        if (!edge.IsDone()) {
            return "arc edge construction failed";
        }
        wire.Add(edge.Edge());
    } else {
        BRepBuilderAPI_MakeEdge edge(start, end);
        if (!edge.IsDone()) {
            return "line edge construction failed";
        }
        wire.Add(edge.Edge());
    }
    return wire.IsDone() ? nullptr : "segment wire construction failed";
}

/* Rejects non-finite points, zero-length segments, and a repeated closing point. */
const char* polyline_point_error(const occt_bridge_vec3_t* points, size_t point_count, bool closed) {
    for (size_t index = 0; index < point_count; ++index) {
        if (!finite(points[index])) {
            return "wire point is not finite";
        }
        if (index != 0 && to_point(points[index - 1]).Distance(to_point(points[index]))
                <= std::numeric_limits<double>::epsilon()) {
            return "wire contains a zero-length segment";
        }
    }
    if (closed && to_point(points[0]).Distance(to_point(points[point_count - 1]))
            <= std::numeric_limits<double>::epsilon()) {
        return "closed wire must not repeat its first point";
    }
    return nullptr;
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_create_polyline_wire(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    size_t point_count,
    int closed,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const size_t minimum_points = closed == 1 ? 3 : 2;
        if (points == nullptr || (closed != 0 && closed != 1) || point_count < minimum_points) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid polyline-wire parameters");
        }
        if (const char* error = polyline_point_error(points, point_count, closed == 1)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
        }
        BRepBuilderAPI_MakePolygon builder;
        for (size_t index = 0; index < point_count; ++index) {
            builder.Add(to_point(points[index]));
        }
        if (closed == 1) {
            builder.Close();
        }
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "polyline wire construction failed");
        }
        return store_shape(session, builder.Wire(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_segment_wire(
    occt_bridge_session_t* session,
    const occt_bridge_wire_segment_t* segments,
    size_t segment_count,
    int closed,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (segments == nullptr || segment_count == 0 || (closed != 0 && closed != 1)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid segment-wire parameters");
        }
        BRepBuilderAPI_MakeWire wire;
        for (size_t index = 0; index < segment_count; ++index) {
            const auto& segment = segments[index];
            if (const char* error = wire_segment_error(segment)) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, error);
            }
            if (index > 0 && to_point(segment.start).Distance(to_point(segments[index - 1].end)) > Precision::Confusion()) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "wire segments are disconnected");
            }
            if (const char* error = add_wire_segment(wire, segment)) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
            }
        }
        if (closed == 1 && to_point(segments[0].start).Distance(to_point(segments[segment_count - 1].end)) > Precision::Confusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "segment wire is not closed");
        }
        return store_shape(session, wire.Wire(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_circle_wire(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t center,
    occt_bridge_vec3_t normal,
    double radius,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(center) || !finite(normal) || !std::isfinite(radius) || radius <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid circle-wire parameters");
        }
        const gp_Vec direction(normal.x, normal.y, normal.z);
        if (direction.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "circle normal must be nonzero");
        }
        BRepBuilderAPI_MakeEdge edge(
            gp_Circ(
                gp_Ax2(gp_Pnt(center.x, center.y, center.z), gp_Dir(direction)),
                radius));
        if (!edge.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "circle edge construction failed");
        }
        BRepBuilderAPI_MakeWire wire(edge.Edge());
        if (!wire.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "circle wire construction failed");
        }
        return store_shape(session, wire.Wire(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_ellipse_wire(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t center,
    occt_bridge_vec3_t normal,
    double major_radius,
    double minor_radius,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(center) || !finite(normal) || !std::isfinite(major_radius)
            || !std::isfinite(minor_radius) || minor_radius <= 0.0
            || major_radius < minor_radius) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid ellipse-wire parameters");
        }
        const gp_Vec direction(normal.x, normal.y, normal.z);
        if (direction.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "ellipse normal must be nonzero");
        }
        BRepBuilderAPI_MakeEdge edge(gp_Elips(
            gp_Ax2(gp_Pnt(center.x, center.y, center.z), gp_Dir(direction)),
            major_radius,
            minor_radius));
        if (!edge.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "ellipse edge construction failed");
        }
        BRepBuilderAPI_MakeWire wire(edge.Edge());
        if (!wire.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "ellipse wire construction failed");
        }
        return store_shape(session, wire.Wire(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_face_from_wire(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t wire,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const TopoDS_Shape* value = find_shape(session, wire);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "wire was not found");
        }
        if (value->ShapeType() != TopAbs_WIRE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not a wire");
        }
        BRepBuilderAPI_MakeFace builder(TopoDS::Wire(*value));
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "wire does not define a planar face");
        }
        return store_shape_with_history(
            session,
            builder.Face(),
            out_shape,
            builder,
            {value});
    });
}

occt_bridge_status_t occt_bridge_create_prism_from_face(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    occt_bridge_vec3_t direction,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(direction)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "prism direction must be finite");
        }
        const gp_Vec vector(direction.x, direction.y, direction.z);
        if (vector.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "prism direction must be nonzero");
        }
        const TopoDS_Shape* value = find_shape(session, face);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "face was not found");
        }
        if (value->ShapeType() != TopAbs_FACE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not a face");
        }
        BRepPrimAPI_MakePrism builder(TopoDS::Face(*value), vector, Standard_True);
        builder.Build();
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "prism construction failed");
        }
        return store_shape_with_history(
            session,
            builder.Shape(),
            out_shape,
            builder,
            {value});
    });
}

occt_bridge_status_t occt_bridge_create_polygon_prism(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    size_t point_count,
    occt_bridge_vec3_t direction,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (points == nullptr || point_count < 3 || !finite(direction)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "a polygon prism requires at least three finite points");
        }
        const double direction_length = std::hypot(direction.x, std::hypot(direction.y, direction.z));
        if (direction_length <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "prism direction must be nonzero");
        }
        BRepBuilderAPI_MakePolygon polygon;
        for (size_t index = 0; index < point_count; ++index) {
            if (!finite(points[index])) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "polygon point is not finite");
            }
            polygon.Add(gp_Pnt(points[index].x, points[index].y, points[index].z));
        }
        polygon.Close();
        if (!polygon.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "polygon wire could not be closed");
        }
        BRepBuilderAPI_MakeFace face(polygon.Wire());
        if (!face.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "polygon does not define a planar face");
        }
        BRepPrimAPI_MakePrism prism(
            face.Face(), gp_Vec(direction.x, direction.y, direction.z), Standard_True);
        prism.Build();
        if (!prism.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "prism construction failed");
        }
        /* Degenerate outlines, such as collinear points, sweep to invalid zero-volume solids. */
        const ShapeValidator analyzer(prism.Shape());
        if (!analyzer.IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "polygon prism is not a valid BREP");
        }
        return store_shape(session, prism.Shape(), out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_faceted_stone(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* bottom_points,
    const occt_bridge_vec3_t* top_points,
    size_t point_count,
    occt_bridge_vec3_t top_center,
    double bottom_chamfer,
    double top_fillet,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (bottom_points == nullptr || top_points == nullptr || point_count < 3
            || !finite(top_center) || !std::isfinite(bottom_chamfer) || !std::isfinite(top_fillet)
            || bottom_chamfer < 0.0 || top_fillet < 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid faceted-stone parameters");
        }

        std::vector<gp_Pnt> bottom;
        std::vector<gp_Pnt> top;
        bottom.reserve(point_count);
        top.reserve(point_count);
        for (size_t index = 0; index < point_count; ++index) {
            if (!finite(bottom_points[index]) || !finite(top_points[index])) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "stone point is not finite");
            }
            bottom.emplace_back(
                bottom_points[index].x, bottom_points[index].y, bottom_points[index].z);
            top.emplace_back(top_points[index].x, top_points[index].y, top_points[index].z);
        }

        TopoDS_Shape shape = build_faceted_solid(
            bottom,
            top,
            gp_Pnt(top_center.x, top_center.y, top_center.z),
            bottom_chamfer,
            top_fillet);
        if (shape.IsNull()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "faceted stone could not be closed into a solid");
        }
        const ShapeValidator analyzer(shape);
        if (!analyzer.IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "faceted stone is not a valid BREP");
        }
        return store_shape(session, shape, out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_wall_torch(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t wall_anchor,
    occt_bridge_vec3_t wall_normal,
    double scale,
    occt_bridge_wall_torch_result_t* out_torch) {
    return guarded(session, [&] {
        if (out_torch == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_torch is null");
        }
        *out_torch = {};
        if (!finite(wall_anchor) || !finite(wall_normal) || !std::isfinite(scale) || scale <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid wall-torch parameters");
        }
        const double horizontal_length = std::hypot(wall_normal.x, wall_normal.y);
        if (horizontal_length <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "wall normal must have an XY component");
        }
        const double normal_x = wall_normal.x / horizontal_length;
        const double normal_y = wall_normal.y / horizontal_length;
        const gp_Dir outward(normal_x, normal_y, 0.0);
        const gp_Dir up(0.0, 0.0, 1.0);
        const gp_Pnt anchor(wall_anchor.x, wall_anchor.y, wall_anchor.z);

        BRepPrimAPI_MakeCylinder plate_builder(
            gp_Ax2(anchor, outward), 16.0 * scale, 6.0 * scale);
        plate_builder.Build();

        const gp_Pnt arm_start(
            wall_anchor.x + normal_x * 5.0 * scale,
            wall_anchor.y + normal_y * 5.0 * scale,
            wall_anchor.z - 7.0 * scale);
        const gp_Pnt arm_end(
            wall_anchor.x + normal_x * 49.0 * scale,
            wall_anchor.y + normal_y * 49.0 * scale,
            wall_anchor.z + 4.0 * scale);
        const TopoDS_Shape arm = cylinder_between(arm_start, arm_end, 4.0 * scale);

        const gp_Pnt stem_start(arm_end.X(), arm_end.Y(), arm_end.Z() - 18.0 * scale);
        const gp_Pnt stem_end(arm_end.X(), arm_end.Y(), arm_end.Z() + 17.0 * scale);
        const TopoDS_Shape stem = cylinder_between(stem_start, stem_end, 4.5 * scale);

        const gp_Pnt cup_base(arm_end.X(), arm_end.Y(), wall_anchor.z + 12.0 * scale);
        BRepPrimAPI_MakeCone cup_builder(
            gp_Ax2(cup_base, up), 8.0 * scale, 15.0 * scale, 18.0 * scale);
        cup_builder.Build();
        if (!plate_builder.IsDone() || arm.IsNull() || stem.IsNull() || !cup_builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "wall-torch fixture construction failed");
        }

        TopoDS_Compound fixture;
        BRep_Builder fixture_builder;
        fixture_builder.MakeCompound(fixture);
        fixture_builder.Add(fixture, plate_builder.Shape());
        fixture_builder.Add(fixture, arm);
        fixture_builder.Add(fixture, stem);
        fixture_builder.Add(fixture, cup_builder.Shape());

        const gp_Pnt flame_base(arm_end.X(), arm_end.Y(), wall_anchor.z + 30.0 * scale);
        BRepPrimAPI_MakeCone lower_flame(
            gp_Ax2(flame_base, up), 11.0 * scale, 4.0 * scale, 25.0 * scale);
        lower_flame.Build();
        const gp_Pnt upper_flame_base(
            flame_base.X(), flame_base.Y(), flame_base.Z() + 17.0 * scale);
        BRepPrimAPI_MakeCone upper_flame(
            gp_Ax2(upper_flame_base, up), 6.0 * scale, 0.0, 22.0 * scale);
        upper_flame.Build();
        if (!lower_flame.IsDone() || !upper_flame.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "wall-torch flame construction failed");
        }
        TopoDS_Compound flame;
        BRep_Builder flame_builder;
        flame_builder.MakeCompound(flame);
        flame_builder.Add(flame, lower_flame.Shape());
        flame_builder.Add(flame, upper_flame.Shape());

        occt_bridge_status_t status = store_shape(session, fixture, &out_torch->fixture_shape);
        if (status != OCCT_BRIDGE_OK) {
            return status;
        }
        status = store_shape(session, flame, &out_torch->flame_shape);
        if (status != OCCT_BRIDGE_OK) {
            session->shapes.erase(out_torch->fixture_shape);
            out_torch->fixture_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
            return status;
        }

        out_torch->light.type = OCCT_BRIDGE_LIGHT_POSITIONAL;
        out_torch->light.position = {
            flame_base.X(), flame_base.Y(), flame_base.Z() + 14.0 * scale};
        out_torch->light.direction = {0.0, 0.0, 0.0};
        out_torch->light.color = {1.0, 0.32, 0.06};
        out_torch->light.intensity = 2000000.0;
        out_torch->light.range = 0.0;
        out_torch->light.spot_angle_degrees = 0.0;
        out_torch->light.cast_shadows = 0;
        return succeed(session);
    });
}

}  // extern "C"

namespace {

/* Builds the straight-segment sweep path; on failure records the error and returns its status. */
occt_bridge_status_t make_tube_spine(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* path_points,
    size_t point_count,
    TopoDS_Wire& spine) {
    BRepBuilderAPI_MakeWire spine_builder;
    for (size_t index = 1; index < point_count; ++index) {
        const gp_Pnt first = to_point(path_points[index - 1]);
        const gp_Pnt second = to_point(path_points[index]);
        if (first.Distance(second) <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "tube path contains a zero-length segment");
        }
        BRepBuilderAPI_MakeEdge edge(first, second);
        if (!edge.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "tube path edge construction failed");
        }
        spine_builder.Add(edge.Edge());
    }
    if (!spine_builder.IsDone()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "tube path wire construction failed");
    }
    spine = spine_builder.Wire();
    return OCCT_BRIDGE_OK;
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_create_polyline_tube(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* path_points,
    size_t point_count,
    double radius,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (path_points == nullptr || point_count < 2 || !std::isfinite(radius) || radius <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid polyline-tube parameters");
        }
        if (!std::all_of(path_points, path_points + point_count, [](const occt_bridge_vec3_t& point) {
                return finite(point);
            })) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "tube path point is not finite");
        }
        TopoDS_Wire spine;
        const occt_bridge_status_t spine_status = make_tube_spine(session, path_points, point_count, spine);
        if (spine_status != OCCT_BRIDGE_OK) {
            return spine_status;
        }

        const gp_Pnt start = to_point(path_points[0]);
        const gp_Vec initial_tangent(start, to_point(path_points[1]));
        const gp_Circ profile_circle(gp_Ax2(start, gp_Dir(initial_tangent)), radius);
        BRepBuilderAPI_MakeEdge profile_edge(profile_circle);
        BRepBuilderAPI_MakeWire profile_wire(profile_edge.Edge());
        if (!profile_edge.IsDone() || !profile_wire.IsDone()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "tube profile construction failed");
        }

        BRepOffsetAPI_MakePipe pipe(spine, profile_wire.Wire());
        pipe.Build();
        if (!pipe.IsDone() || pipe.Shape().IsNull()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "polyline tube sweep failed");
        }
        const ShapeValidator analyzer(pipe.Shape());
        if (!analyzer.IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "polyline tube is not a valid BREP");
        }
        return store_shape(session, pipe.Shape(), out_shape);
    });
}

}  // extern "C"

namespace {

/* Validates section sizes (at least three points, no overflow) and point finiteness. */
occt_bridge_status_t validate_loft_points(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    const size_t* section_point_counts,
    size_t section_count) {
    size_t total_points = 0;
    for (size_t section = 0; section < section_count; ++section) {
        const size_t count = section_point_counts[section];
        if (count < 3 || total_points > std::numeric_limits<size_t>::max() - count) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid loft section size");
        }
        total_points += count;
    }
    if (!std::all_of(points, points + total_points, [](const occt_bridge_vec3_t& point) {
            return finite(point);
        })) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "loft point is not finite");
    }
    return OCCT_BRIDGE_OK;
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_create_loft(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* points,
    const size_t* section_point_counts,
    size_t section_count,
    int make_solid,
    int ruled,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (points == nullptr || section_point_counts == nullptr || section_count < 2
            || (make_solid != 0 && make_solid != 1) || (ruled != 0 && ruled != 1)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid loft parameters");
        }
        const occt_bridge_status_t points_status =
            validate_loft_points(session, points, section_point_counts, section_count);
        if (points_status != OCCT_BRIDGE_OK) {
            return points_status;
        }

        BRepOffsetAPI_ThruSections loft(
            make_solid ? Standard_True : Standard_False,
            ruled ? Standard_True : Standard_False);
        size_t offset = 0;
        for (size_t section = 0; section < section_count; ++section) {
            const size_t count = section_point_counts[section];
            const TopoDS_Wire wire = polygon_wire(points + offset, count);
            if (wire.IsNull()) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "loft section could not be closed");
            }
            loft.AddWire(wire);
            offset += count;
        }
        loft.CheckCompatibility(Standard_True);
        loft.Build();
        if (!loft.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "loft construction failed");
        }
        const TopoDS_Shape shape = loft.Shape();
        if (shape.IsNull()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "loft produced a null shape");
        }
        const ShapeValidator analyzer(shape);
        if (!analyzer.IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "loft produced an invalid BREP");
        }
        return store_shape(session, shape, out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_compound(
    occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* shapes,
    size_t shape_count,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (shapes == nullptr || shape_count == 0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "compound requires at least one shape");
        }
        TopoDS_Compound compound;
        BRep_Builder builder;
        builder.MakeCompound(compound);
        for (size_t index = 0; index < shape_count; ++index) {
            const TopoDS_Shape* shape = find_shape(session, shapes[index]);
            if (shape == nullptr) {
                return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "compound child shape was not found");
            }
            builder.Add(compound, *shape);
        }
        return store_shape(session, compound, out_shape);
    });
}

}  // extern "C"

namespace {

/*
 * Presents a sewing context's history through the Generated/Modified/
 * IsDeleted interface used by collect_history. BRepTools_History tracks only
 * vertices, edges, faces, and solids; other types report no relations.
 */
class SewingHistory {
public:
    explicit SewingHistory(opencascade::handle<BRepTools_History> history) : history_(std::move(history)) {}

    const TopTools_ListOfShape& Generated(const TopoDS_Shape& shape) const {
        return tracked(shape) ? history_->Generated(shape) : empty_;
    }

    const TopTools_ListOfShape& Modified(const TopoDS_Shape& shape) const {
        return tracked(shape) ? history_->Modified(shape) : empty_;
    }

    bool IsDeleted(const TopoDS_Shape& shape) const {
        return tracked(shape) && history_->IsRemoved(shape);
    }

private:
    bool tracked(const TopoDS_Shape& shape) const {
        return !history_.IsNull() && BRepTools_History::IsSupportedType(shape);
    }

    opencascade::handle<BRepTools_History> history_;
    TopTools_ListOfShape empty_;
};

/* Resolves sewing inputs, each of which must contain at least one face. */
occt_bridge_status_t sewing_inputs(
    occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* shapes,
    size_t shape_count,
    std::vector<const TopoDS_Shape*>& inputs) {
    for (size_t index = 0; index < shape_count; ++index) {
        const TopoDS_Shape* shape = find_shape(session, shapes[index]);
        if (shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "sewing input shape was not found");
        }
        if (!contains_topology(*shape, TopAbs_FACE)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "sewing input contains no faces");
        }
        inputs.push_back(shape);
    }
    return OCCT_BRIDGE_OK;
}

struct SolidBoundary {
    TopoDS_Solid solid;
    TopoDS_Shell shell;
    double volume = 0.0;
};

double solid_volume(const TopoDS_Shape& shape) {
    GProp_GProps properties;
    BRepGProp::VolumeProperties(shape, properties);
    return std::abs(properties.Mass());
}

/* Normalizes each closed shell as an outward-oriented standalone solid. */
occt_bridge_status_t collect_solid_boundaries(
    occt_bridge_session_t* session,
    const std::vector<const TopoDS_Shape*>& inputs,
    std::vector<SolidBoundary>& boundaries) {
    TopTools_IndexedMapOfShape shells;
    for (const TopoDS_Shape* input : inputs) {
        if (input->ShapeType() == TopAbs_SHELL) {
            shells.Add(*input);
        } else {
            for (TopExp_Explorer explorer(*input, TopAbs_SHELL); explorer.More(); explorer.Next()) {
                shells.Add(explorer.Current());
            }
        }
    }
    if (shells.IsEmpty()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "solid construction requires at least one shell");
    }
    for (int index = 1; index <= shells.Extent(); ++index) {
        TopoDS_Shell shell = TopoDS::Shell(shells(index));
        if (!BRep_Tool::IsClosed(shell)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "solid boundary shell is not closed");
        }
        BRepBuilderAPI_MakeSolid builder(shell);
        if (!builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "solid boundary construction failed");
        }
        TopoDS_Solid solid = builder.Solid();
        BRepLib::OrientClosedSolid(solid);
        const ShapeValidator analyzer(solid);
        if (!analyzer.IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "solid boundary is not a valid BREP");
        }
        const double volume = solid_volume(solid);
        if (!std::isfinite(volume) || volume <= Precision::Confusion()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "solid boundary encloses no volume");
        }
        boundaries.push_back({solid, single_shell(solid), volume});
    }
    std::sort(boundaries.begin(), boundaries.end(), [](const SolidBoundary& left, const SolidBoundary& right) {
        return left.volume > right.volume;
    });
    return OCCT_BRIDGE_OK;
}

/* The common volume gives a topology-independent containment/overlap test. */
occt_bridge_status_t common_volume(
    occt_bridge_session_t* session,
    const TopoDS_Solid& left,
    const TopoDS_Solid& right,
    double& volume) {
    BRepAlgoAPI_Common common(left, right);
    common.Build();
    if (!common.IsDone()) {
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "solid boundary classification failed");
    }
    volume = solid_volume(common.Shape());
    return OCCT_BRIDGE_OK;
}

occt_bridge_status_t build_solid_from_inputs(
    occt_bridge_session_t* session,
    const std::vector<const TopoDS_Shape*>& inputs,
    occt_bridge_shape_id_t* out_shape) {
    std::vector<SolidBoundary> boundaries;
    const occt_bridge_status_t collect_status = collect_solid_boundaries(session, inputs, boundaries);
    if (collect_status != OCCT_BRIDGE_OK) {
        return collect_status;
    }

    const SolidBoundary& outer = boundaries.front();
    double expected_volume = outer.volume;
    for (size_t index = 1; index < boundaries.size(); ++index) {
        double overlap = 0.0;
        const occt_bridge_status_t status = common_volume(
            session, outer.solid, boundaries[index].solid, overlap);
        if (status != OCCT_BRIDGE_OK) {
            return status;
        }
        const double tolerance = std::max(Precision::Confusion(), boundaries[index].volume * 1.0e-9);
        if (std::abs(overlap - boundaries[index].volume) > tolerance) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "void shell is not contained by the outer shell");
        }
        for (size_t other = 1; other < index; ++other) {
            double void_overlap = 0.0;
            const occt_bridge_status_t pair_status = common_volume(
                session, boundaries[other].solid, boundaries[index].solid, void_overlap);
            if (pair_status != OCCT_BRIDGE_OK) {
                return pair_status;
            }
            if (void_overlap > tolerance) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "void shells overlap");
            }
        }
        expected_volume -= boundaries[index].volume;
    }
    if (expected_volume <= Precision::Confusion()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "void shells consume the outer volume");
    }

    BRepBuilderAPI_MakeSolid builder;
    builder.Add(outer.shell);
    for (size_t index = 1; index < boundaries.size(); ++index) {
        TopoDS_Shell void_shell = boundaries[index].shell;
        void_shell.Reverse();
        builder.Add(void_shell);
    }
    if (!builder.IsDone()) {
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "solid construction failed");
    }
    TopoDS_Solid solid = builder.Solid();
    const ShapeValidator analyzer(solid);
    const double result_volume = solid_volume(solid);
    const double volume_tolerance = std::max(Precision::Confusion(), expected_volume * 1.0e-9);
    if (!analyzer.IsValid() || std::abs(result_volume - expected_volume) > volume_tolerance) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "solid boundaries do not form a valid solid");
    }
    return store_shape(session, solid, out_shape);
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_sew(
    occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* shapes,
    size_t shape_count,
    double tolerance,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (shapes == nullptr || shape_count == 0 || !std::isfinite(tolerance) || tolerance <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid sewing parameters");
        }
        std::vector<const TopoDS_Shape*> inputs;
        const occt_bridge_status_t input_status = sewing_inputs(session, shapes, shape_count, inputs);
        if (input_status != OCCT_BRIDGE_OK) {
            return input_status;
        }
        BRepBuilderAPI_Sewing sewing(tolerance);
        for (const TopoDS_Shape* input : inputs) {
            sewing.Add(*input);
        }
        sewing.Perform();
        const TopoDS_Shape sewed = sewing.SewedShape();
        if (sewed.IsNull()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "sewing produced no shape");
        }
        SewingHistory history(sewing.GetContext()->History());
        return store_checked_result(session, "sewing", sewed, out_shape, history, inputs);
    });
}

occt_bridge_status_t occt_bridge_make_solid(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shell,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const TopoDS_Shape* input = find_shape(session, shell);
        if (input == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shell was not found");
        }
        TopTools_IndexedMapOfShape shells;
        if (input->ShapeType() == TopAbs_SHELL) {
            shells.Add(*input);
        } else {
            for (TopExp_Explorer explorer(*input, TopAbs_SHELL); explorer.More(); explorer.Next()) {
                shells.Add(explorer.Current());
            }
        }
        if (shells.Extent() != 1) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape must contain exactly one shell");
        }
        return build_solid_from_inputs(session, {input}, out_shape);
    });
}

occt_bridge_status_t occt_bridge_make_solid_from_shells(
    occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* shells,
    size_t shell_count,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (shells == nullptr || shell_count == 0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "solid construction requires shell inputs");
        }
        std::vector<const TopoDS_Shape*> inputs;
        inputs.reserve(shell_count);
        for (size_t index = 0; index < shell_count; ++index) {
            const TopoDS_Shape* input = find_shape(session, shells[index]);
            if (input == nullptr) {
                return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "solid boundary shape was not found");
            }
            inputs.push_back(input);
        }
        return build_solid_from_inputs(session, inputs, out_shape);
    });
}

occt_bridge_status_t occt_bridge_fuse(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t left,
    occt_bridge_shape_id_t right,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return boolean_operation<BRepAlgoAPI_Fuse>(session, left, right, out_shape, "fuse");
    });
}

occt_bridge_status_t occt_bridge_cut(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t object,
    occt_bridge_shape_id_t tool,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return boolean_operation<BRepAlgoAPI_Cut>(session, object, tool, out_shape, "cut");
    });
}

occt_bridge_status_t occt_bridge_common(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t left,
    occt_bridge_shape_id_t right,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return boolean_operation<BRepAlgoAPI_Common>(session, left, right, out_shape, "common");
    });
}

occt_bridge_status_t occt_bridge_fillet(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* edges,
    size_t edge_count,
    double radius,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return edge_treatment<BRepFilletAPI_MakeFillet>(
            session, shape, edges, edge_count, radius, out_shape, "fillet");
    });
}

occt_bridge_status_t occt_bridge_chamfer(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* edges,
    size_t edge_count,
    double distance,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return edge_treatment<BRepFilletAPI_MakeChamfer>(
            session, shape, edges, edge_count, distance, out_shape, "chamfer");
    });
}

occt_bridge_status_t occt_bridge_offset(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double offset,
    double tolerance,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!std::isfinite(offset) || offset == 0.0
            || !std::isfinite(tolerance) || tolerance <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid offset parameters");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        BRepOffsetAPI_MakeOffsetShape builder;
        const std::string exception = perform_reporting_exception(
            [&] { builder.PerformByJoin(*value, offset, tolerance); });
        if (!exception.empty() || !builder.IsDone() || builder.Shape().IsNull()) {
            return offset_failure(session, builder, "offset", exception, {});
        }
        return store_checked_result(
            session,
            "offset",
            builder.Shape(),
            out_shape,
            builder,
            {value});
    });
}

occt_bridge_status_t occt_bridge_hollow(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* faces_to_remove,
    size_t face_count,
    double thickness,
    double tolerance,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (faces_to_remove == nullptr || face_count == 0 || !std::isfinite(thickness)
            || thickness == 0.0 || !std::isfinite(tolerance) || tolerance <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid hollow parameters");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        TopTools_ListOfShape closing_faces;
        for (size_t index = 0; index < face_count; ++index) {
            const TopoDS_Shape* face = find_shape(session, faces_to_remove[index]);
            if (face == nullptr) {
                return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "hollow face was not found");
            }
            if (face->ShapeType() != TopAbs_FACE || !is_descendant(*value, *face, TopAbs_FACE)) {
                return fail(
                    session,
                    OCCT_BRIDGE_INVALID_GEOMETRY,
                    "hollow selection is not a face of the input shape");
            }
            closing_faces.Append(*face);
        }
        BRepOffsetAPI_MakeThickSolid builder;
        const std::string exception = perform_reporting_exception(
            [&] { builder.MakeThickSolidByJoin(*value, closing_faces, thickness, tolerance); });
        if (!exception.empty() || !builder.IsDone() || builder.Shape().IsNull()) {
            return offset_failure(session, builder, "hollow", exception, closing_faces);
        }
        return store_checked_result(
            session,
            "hollow",
            builder.Shape(),
            out_shape,
            builder,
            {value});
    });
}

occt_bridge_status_t occt_bridge_translate(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t offset,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (!finite(offset)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "translation offset must be finite");
        }
        gp_Trsf transform;
        transform.SetTranslation(gp_Vec(offset.x, offset.y, offset.z));
        return transformed_shape(session, shape, transform, out_shape);
    });
}

occt_bridge_status_t occt_bridge_rotate(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t axis_origin,
    occt_bridge_vec3_t axis_direction,
    double angle_radians,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (!finite(axis_origin) || !finite(axis_direction) || !std::isfinite(angle_radians)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid rotation parameters");
        }
        const gp_Vec direction(axis_direction.x, axis_direction.y, axis_direction.z);
        if (direction.Magnitude() <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "rotation axis must be nonzero");
        }
        gp_Trsf transform;
        transform.SetRotation(
            gp_Ax1(
                gp_Pnt(axis_origin.x, axis_origin.y, axis_origin.z),
                gp_Dir(direction)),
            angle_radians);
        return transformed_shape(session, shape, transform, out_shape);
    });
}

occt_bridge_status_t occt_bridge_scale(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t center,
    double factor,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (!finite(center) || !std::isfinite(factor) || factor <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "scale factor must be finite and positive");
        }
        gp_Trsf transform;
        transform.SetScale(gp_Pnt(center.x, center.y, center.z), factor);
        return transformed_shape(session, shape, transform, out_shape);
    });
}

occt_bridge_status_t occt_bridge_shape_duplicate(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        const TopoDS_Shape copied_shape = *value;
        const auto history = session->histories.find(shape);
        const bool has_history = history != session->histories.end();
        occt_bridge_operation_history copied_history;
        if (has_history) {
            copied_history = history->second;
        }
        const occt_bridge_status_t status = store_shape(session, copied_shape, out_shape);
        if (status != OCCT_BRIDGE_OK || !has_history) {
            return status;
        }
        try {
            session->histories.emplace(*out_shape, std::move(copied_history));
        } catch (...) {
            session->shapes.erase(*out_shape);
            *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
            throw;
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_type(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t* out_type) {
    return guarded(session, [&] {
        if (out_type == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_type is null");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        *out_type = public_shape_type(value->ShapeType());
        if (*out_type == 0) {
            return fail(session, OCCT_BRIDGE_INTERNAL_ERROR, "shape has an unknown topology type");
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_subshape_count(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t subshape_type,
    size_t* out_count) {
    return guarded(session, [&] {
        if (out_count == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_count is null");
        }
        *out_count = 0;
        TopAbs_ShapeEnum topology_type = TopAbs_SHAPE;
        if (!bridge_shape_type(subshape_type, topology_type)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "unknown subshape type");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        *out_count = static_cast<size_t>(descendant_shapes(*value, topology_type).Extent());
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_subshape_at(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_shape_type_t subshape_type,
    size_t index,
    occt_bridge_shape_id_t* out_subshape) {
    return guarded(session, [&] {
        if (out_subshape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_subshape is null");
        }
        *out_subshape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        TopAbs_ShapeEnum topology_type = TopAbs_SHAPE;
        if (!bridge_shape_type(subshape_type, topology_type)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "unknown subshape type");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        const TopTools_IndexedMapOfShape subshapes = descendant_shapes(*value, topology_type);
        if (index >= static_cast<size_t>(subshapes.Extent())) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "subshape index is out of range");
        }
        return store_shape(
            session,
            subshapes.FindKey(static_cast<Standard_Integer>(index + 1)),
            out_subshape);
    });
}

}  // extern "C"

namespace {

/*
 * Axis-aligned bounds of a stored shape. Exact bounds follow the geometry
 * without enlarging the box by shape tolerances; the default bounds keep
 * OCCT's tolerance-padded box.
 */
occt_bridge_status_t shape_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_bounds_t* out_bounds,
    bool exact) {
    if (out_bounds == nullptr) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_bounds is null");
    }
    const TopoDS_Shape* value = find_shape(session, shape);
    if (value == nullptr) {
        return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
    }
    Bnd_Box bounds;
    if (exact) {
        BRepBndLib::AddOptimal(*value, bounds, Standard_False, Standard_False);
    } else {
        BRepBndLib::Add(*value, bounds);
    }
    if (bounds.IsVoid() || bounds.IsOpen()) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape has no finite bounds");
    }
    bounds.Get(
        out_bounds->min.x, out_bounds->min.y, out_bounds->min.z,
        out_bounds->max.x, out_bounds->max.y, out_bounds->max.z);
    return succeed(session);
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_shape_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_bounds_t* out_bounds) {
    return guarded(session, [&] { return shape_bounds(session, shape, out_bounds, false); });
}

occt_bridge_status_t occt_bridge_shape_exact_bounds(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_bounds_t* out_bounds) {
    return guarded(session, [&] { return shape_bounds(session, shape, out_bounds, true); });
}

occt_bridge_status_t occt_bridge_shape_surface_area(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double* out_area) {
    return guarded(session, [&] {
        if (out_area == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_area is null");
        }
        *out_area = 0.0;
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        if (!contains_topology(*value, TopAbs_FACE)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape contains no faces");
        }
        GProp_GProps properties;
        BRepGProp::SurfaceProperties(*value, properties);
        *out_area = std::abs(properties.Mass());
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_volume(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double* out_volume) {
    return guarded(session, [&] {
        if (out_volume == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_volume is null");
        }
        *out_volume = 0.0;
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        if (!contains_topology(*value, TopAbs_SOLID)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape contains no solids");
        }
        GProp_GProps properties;
        BRepGProp::VolumeProperties(*value, properties);
        *out_volume = std::abs(properties.Mass());
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_center_of_mass(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t* out_center) {
    return guarded(session, [&] {
        if (out_center == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_center is null");
        }
        *out_center = {};
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        GProp_GProps properties;
        if (contains_topology(*value, TopAbs_SOLID)) {
            BRepGProp::VolumeProperties(*value, properties);
        } else if (contains_topology(*value, TopAbs_FACE)) {
            BRepGProp::SurfaceProperties(*value, properties);
        } else if (contains_topology(*value, TopAbs_EDGE)) {
            BRepGProp::LinearProperties(*value, properties);
        } else {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_GEOMETRY,
                "shape has no measurable solid, face, or edge topology");
        }
        if (std::abs(properties.Mass()) <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape has zero measure");
        }
        const gp_Pnt center = properties.CentreOfMass();
        *out_center = {center.X(), center.Y(), center.Z()};
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_face_normal(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    occt_bridge_vec3_t* out_normal) {
    return guarded(session, [&] {
        if (out_normal == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_normal is null");
        }
        *out_normal = {};
        const TopoDS_Shape* value = find_shape(session, face);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "face was not found");
        }
        if (value->ShapeType() != TopAbs_FACE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not a face");
        }
        const TopoDS_Face topology_face = TopoDS::Face(*value);
        double first_u = 0.0;
        double last_u = 0.0;
        double first_v = 0.0;
        double last_v = 0.0;
        BRepTools::UVBounds(topology_face, first_u, last_u, first_v, last_v);
        if (!std::isfinite(first_u) || !std::isfinite(last_u)
            || !std::isfinite(first_v) || !std::isfinite(last_v)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "face has no finite UV bounds");
        }
        BRepAdaptor_Surface surface(topology_face, Standard_True);
        BRepLProp_SLProps properties(
            surface,
            (first_u + last_u) * 0.5,
            (first_v + last_v) * 0.5,
            1,
            Precision::Confusion());
        if (!properties.IsNormalDefined()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "face normal is undefined");
        }
        gp_Dir normal = properties.Normal();
        if (topology_face.Orientation() == TopAbs_REVERSED) {
            normal.Reverse();
        }
        *out_normal = {normal.X(), normal.Y(), normal.Z()};
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_face_is_planar(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t face,
    int* out_is_planar) {
    return guarded(session, [&] {
        if (out_is_planar == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_is_planar is null");
        }
        *out_is_planar = 0;
        const TopoDS_Shape* value = find_shape(session, face);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "face was not found");
        }
        if (value->ShapeType() != TopAbs_FACE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not a face");
        }
        const BRepAdaptor_Surface surface(TopoDS::Face(*value), Standard_True);
        *out_is_planar = surface.GetType() == GeomAbs_Plane ? 1 : 0;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_edge_length(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    double* out_length) {
    return guarded(session, [&] {
        if (out_length == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_length is null");
        }
        *out_length = 0.0;
        const TopoDS_Shape* value = find_shape(session, edge);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found");
        }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not an edge");
        }
        GProp_GProps properties;
        BRepGProp::LinearProperties(*value, properties);
        const double length = std::abs(properties.Mass());
        if (!std::isfinite(length) || length <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge has zero or invalid length");
        }
        *out_length = length;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_edge_circle_radius(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    int* out_is_circle,
    double* out_radius) {
    return guarded(session, [&] {
        if (out_is_circle == nullptr || out_radius == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "circle query output is null");
        }
        *out_is_circle = 0;
        *out_radius = 0.0;
        const TopoDS_Shape* value = find_shape(session, edge);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found");
        }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not an edge");
        }
        const BRepAdaptor_Curve curve(TopoDS::Edge(*value));
        if (curve.GetType() == GeomAbs_Circle) {
            *out_is_circle = 1;
            *out_radius = curve.Circle().Radius();
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_edge_curvature(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    int* out_is_defined,
    double* out_curvature) {
    return guarded(session, [&] {
        if (out_is_defined == nullptr || out_curvature == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curvature query output is null");
        }
        *out_is_defined = 0;
        *out_curvature = 0.0;
        const TopoDS_Shape* value = find_shape(session, edge);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found");
        }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not an edge");
        }
        const BRepAdaptor_Curve curve(TopoDS::Edge(*value));
        const double first = curve.FirstParameter();
        const double last = curve.LastParameter();
        if (!std::isfinite(first) || !std::isfinite(last)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge has no finite parameter range");
        }
        BRepLProp_CLProps properties(
            curve,
            (first + last) * 0.5,
            2,
            Precision::Confusion());
        if (properties.IsTangentDefined()) {
            const double curvature = std::abs(properties.Curvature());
            if (!std::isfinite(curvature)) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge curvature is not finite");
            }
            *out_is_defined = 1;
            *out_curvature = curvature;
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_edge_curvature_range(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    size_t sample_count,
    double* out_minimum_curvature,
    double* out_maximum_curvature) {
    return guarded(session, [&] {
        if (out_minimum_curvature == nullptr || out_maximum_curvature == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curvature-range output is null");
        }
        *out_minimum_curvature = 0.0;
        *out_maximum_curvature = 0.0;
        if (sample_count < 2 || sample_count > 100000) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curvature sample count must be 2..100000");
        }
        const TopoDS_Shape* value = find_shape(session, edge);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found");
        }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not an edge");
        }
        const BRepAdaptor_Curve curve(TopoDS::Edge(*value));
        const double first = curve.FirstParameter();
        const double last = curve.LastParameter();
        if (!std::isfinite(first) || !std::isfinite(last)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge has no finite parameter range");
        }
        double minimum = std::numeric_limits<double>::infinity();
        double maximum = 0.0;
        for (size_t index = 0; index < sample_count; ++index) {
            const double fraction = static_cast<double>(index) / static_cast<double>(sample_count - 1);
            BRepLProp_CLProps properties(
                curve,
                first + (last - first) * fraction,
                2,
                Precision::Confusion());
            if (!properties.IsTangentDefined()) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge curvature is undefined at a sample");
            }
            const double curvature = std::abs(properties.Curvature());
            if (!std::isfinite(curvature)) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge curvature is not finite");
            }
            minimum = std::min(minimum, curvature);
            maximum = std::max(maximum, curvature);
        }
        *out_minimum_curvature = minimum;
        *out_maximum_curvature = maximum;
        return succeed(session);
    });
}

}  // extern "C"

namespace {

struct CurvatureExtrema {
    double minimum = std::numeric_limits<double>::infinity();
    double minimum_lower = std::numeric_limits<double>::infinity();
    double maximum = 0.0;
    double maximum_upper = 0.0;
    bool exact = true;

    void attain(double curvature) {
        minimum = std::min(minimum, curvature);
        maximum = std::max(maximum, curvature);
        minimum_lower = std::min(minimum_lower, curvature);
        maximum_upper = std::max(maximum_upper, curvature);
    }
};

/* Scalar polynomial in Bernstein form on [0, 1]; degree is size() - 1. */
using Bernstein = std::vector<double>;

class BinomialTable {
public:
    double operator()(size_t n, size_t k) {
        while (rows_.size() <= n) {
            const size_t row = rows_.size();
            std::vector<double> values(row + 1, 1.0);
            for (size_t index = 1; index < row; ++index) {
                values[index] = rows_[row - 1][index - 1] + rows_[row - 1][index];
            }
            rows_.push_back(std::move(values));
        }
        return rows_[n][k];
    }

private:
    std::vector<std::vector<double>> rows_;
};

Bernstein bernstein_derivative(const Bernstein& value) {
    const size_t degree = value.size() - 1;
    if (degree == 0) {
        return Bernstein{0.0};
    }
    Bernstein result(degree);
    for (size_t index = 0; index < degree; ++index) {
        result[index] = static_cast<double>(degree) * (value[index + 1] - value[index]);
    }
    return result;
}

Bernstein bernstein_product(const Bernstein& left, const Bernstein& right, BinomialTable& binomial) {
    const size_t m = left.size() - 1;
    const size_t n = right.size() - 1;
    Bernstein result(m + n + 1, 0.0);
    for (size_t i = 0; i <= m; ++i) {
        for (size_t j = 0; j <= n; ++j) {
            result[i + j] += binomial(m, i) * binomial(n, j) * left[i] * right[j];
        }
    }
    for (size_t k = 0; k <= m + n; ++k) {
        result[k] /= binomial(m + n, k);
    }
    return result;
}

Bernstein bernstein_combine(const Bernstein& left, double scale, const Bernstein& right) {
    Bernstein result(left.size());
    for (size_t index = 0; index < left.size(); ++index) {
        result[index] = left[index] + scale * right[index];
    }
    return result;
}

/* Homogeneous Bezier piece: point numerator coordinates and weight. */
struct BezierPiece {
    Bernstein x, y, z, w;
    size_t depth = 0;
};

struct PieceBounds {
    double lower = 0.0;
    double upper = std::numeric_limits<double>::infinity();
    double start = 0.0;
    double end = 0.0;
    bool start_defined = false;
    bool end_defined = false;
};

/*
 * For C = P / w with D = P'w - Pw', curvature is |D x D'| w^2 / |D|^3.
 * Every factor is a Bernstein polynomial, whose coefficients bound it and
 * whose end coefficients equal its end values.
 */
PieceBounds bound_piece(const BezierPiece& piece, BinomialTable& binomial) {
    const Bernstein dw = bernstein_derivative(piece.w);
    const Bernstein* coordinates[] = {&piece.x, &piece.y, &piece.z};
    Bernstein d[3];
    Bernstein dd[3];
    for (int axis = 0; axis < 3; ++axis) {
        const Bernstein& p = *coordinates[axis];
        d[axis] = bernstein_combine(
            bernstein_product(bernstein_derivative(p), piece.w, binomial),
            -1.0,
            bernstein_product(p, dw, binomial));
        dd[axis] = bernstein_derivative(d[axis]);
    }
    Bernstein cross[3];
    for (int axis = 0; axis < 3; ++axis) {
        const int a = (axis + 1) % 3;
        const int b = (axis + 2) % 3;
        cross[axis] = bernstein_combine(
            bernstein_product(d[a], dd[b], binomial),
            -1.0,
            bernstein_product(d[b], dd[a], binomial));
    }
    Bernstein numerator = bernstein_product(cross[0], cross[0], binomial);
    Bernstein speed = bernstein_product(d[0], d[0], binomial);
    for (int axis = 1; axis < 3; ++axis) {
        numerator = bernstein_combine(numerator, 1.0, bernstein_product(cross[axis], cross[axis], binomial));
        speed = bernstein_combine(speed, 1.0, bernstein_product(d[axis], d[axis], binomial));
    }
    const Bernstein weight_squared = bernstein_product(piece.w, piece.w, binomial);
    /* Squared curvature is ratio / cube: both have degree 12n - 6. */
    const Bernstein ratio = bernstein_product(
        numerator,
        bernstein_product(weight_squared, weight_squared, binomial),
        binomial);
    const Bernstein cube = bernstein_product(bernstein_product(speed, speed, binomial), speed, binomial);

    PieceBounds bounds;
    const auto curvature_value = [&](size_t index) {
        return std::sqrt(std::max(ratio[index], 0.0) / cube[index]);
    };
    const double floor = std::numeric_limits<double>::min();
    const size_t last = cube.size() - 1;
    bounds.start_defined = cube.front() > floor;
    bounds.end_defined = cube.back() > floor;
    if (bounds.start_defined) {
        bounds.start = curvature_value(0);
    }
    if (bounds.end_defined) {
        bounds.end = curvature_value(last);
    }
    /*
     * With positive denominator coefficients, a Bernstein ratio is a convex
     * combination of its coefficient ratios, which converge quadratically.
     */
    if (ratio.size() != cube.size()
        || std::any_of(cube.begin(), cube.end(), [&](double value) { return !(value > floor); })) {
        return bounds;
    }
    bounds.lower = std::numeric_limits<double>::infinity();
    bounds.upper = 0.0;
    for (size_t index = 0; index <= last; ++index) {
        const double value = curvature_value(index);
        bounds.lower = std::min(bounds.lower, value);
        bounds.upper = std::max(bounds.upper, value);
    }
    return bounds;
}

void split_bernstein(const Bernstein& value, Bernstein& left, Bernstein& right) {
    Bernstein work = value;
    const size_t count = value.size();
    left.assign(count, 0.0);
    right.assign(count, 0.0);
    for (size_t level = 0; level < count; ++level) {
        left[level] = work[0];
        right[count - 1 - level] = work[count - 1 - level];
        for (size_t index = 0; index + 1 < count - level; ++index) {
            work[index] = 0.5 * (work[index] + work[index + 1]);
        }
    }
}

std::pair<BezierPiece, BezierPiece> split_piece(const BezierPiece& piece) {
    std::pair<BezierPiece, BezierPiece> halves;
    split_bernstein(piece.x, halves.first.x, halves.second.x);
    split_bernstein(piece.y, halves.first.y, halves.second.y);
    split_bernstein(piece.z, halves.first.z, halves.second.z);
    split_bernstein(piece.w, halves.first.w, halves.second.w);
    halves.first.depth = halves.second.depth = piece.depth + 1;
    return halves;
}

BezierPiece homogeneous_piece(const Handle(Geom_BezierCurve)& curve) {
    const int count = curve->NbPoles();
    TColgp_Array1OfPnt poles(1, count);
    curve->Poles(poles);
    BezierPiece piece;
    for (int index = 1; index <= count; ++index) {
        const double weight = curve->IsRational() ? curve->Weight(index) : 1.0;
        piece.x.push_back(poles(index).X() * weight);
        piece.y.push_back(poles(index).Y() * weight);
        piece.z.push_back(poles(index).Z() * weight);
        piece.w.push_back(weight);
    }
    return piece;
}

bool bound_polynomial_curvature(
    std::vector<BezierPiece> initial,
    double relative_tolerance,
    CurvatureExtrema& extrema,
    std::string& message) {
    constexpr size_t max_depth = 60;
    constexpr size_t max_splits = 200000;
    /* Radius 1e10 model units: below this, curvature is numerically straight. */
    constexpr double absolute_tolerance = 1e-10;
    BinomialTable binomial;
    struct Entry {
        double excess = 0.0;
        BezierPiece piece;
        PieceBounds bounds;
        bool operator<(const Entry& other) const { return excess < other.excess; }
    };
    extrema = CurvatureExtrema{};
    extrema.exact = false;
    std::vector<Entry> pending;
    const auto record = [&](const PieceBounds& bounds) -> bool {
        if (!bounds.start_defined || !bounds.end_defined) {
            return false;
        }
        extrema.attain(bounds.start);
        extrema.attain(bounds.end);
        return true;
    };
    for (BezierPiece& piece : initial) {
        const PieceBounds bounds = bound_piece(piece, binomial);
        if (!record(bounds)) {
            message = "edge curvature is undefined at a Bezier segment end";
            return false;
        }
        pending.push_back(Entry{0.0, std::move(piece), bounds});
    }
    const auto tolerance = [&] {
        return std::max(relative_tolerance * extrema.maximum, absolute_tolerance);
    };
    const auto excess = [&](const PieceBounds& bounds) {
        return std::max(bounds.upper - extrema.maximum, extrema.minimum - bounds.lower);
    };
    std::priority_queue<Entry> queue;
    for (Entry& entry : pending) {
        entry.excess = excess(entry.bounds);
        queue.push(std::move(entry));
    }
    size_t splits = 0;
    while (!queue.empty()) {
        Entry top = queue.top();
        const double current = excess(top.bounds);
        if (current < top.excess) {
            queue.pop();
            top.excess = current;
            queue.push(std::move(top));
            continue;
        }
        if (current <= tolerance()) {
            break;
        }
        queue.pop();
        if (top.piece.depth >= max_depth || ++splits > max_splits) {
            message = std::isfinite(top.bounds.upper)
                ? "edge curvature bounds did not converge"
                : "edge curvature is unbounded or undefined on the edge";
            return false;
        }
        auto halves = split_piece(top.piece);
        for (BezierPiece* half : {&halves.first, &halves.second}) {
            const PieceBounds bounds = bound_piece(*half, binomial);
            if (!record(bounds)) {
                message = "edge curvature is undefined at an interior point";
                return false;
            }
            queue.push(Entry{excess(bounds), std::move(*half), bounds});
        }
    }
    while (!queue.empty()) {
        const PieceBounds& bounds = queue.top().bounds;
        extrema.minimum_lower = std::min(extrema.minimum_lower, bounds.lower);
        extrema.maximum_upper = std::max(extrema.maximum_upper, bounds.upper);
        queue.pop();
    }
    return true;
}

/*
 * Returns the range ends plus up to three consecutive multiples of step inside it.
 * Critical values alternate between two kinds with period 2 * step, so two
 * consecutive multiples cover every extremum without iterating the range.
 */
std::vector<double> critical_parameters(double first, double last, double step) {
    std::vector<double> parameters{first, last};
    const double lowest = std::ceil(first / step);
    for (int offset = 0; offset < 3; ++offset) {
        const double parameter = (lowest + offset) * step;
        if (parameter > first && parameter < last) {
            parameters.push_back(parameter);
        }
    }
    return parameters;
}

/* Conic curvature from closed forms at the analytic critical parameters. */
bool analytic_curvature_extrema(
    const BRepAdaptor_Curve& curve,
    double first,
    double last,
    CurvatureExtrema& extrema) {
    const auto with_vertex = [&] {
        std::vector<double> parameters{first, last};
        if (first < 0.0 && last > 0.0) {
            parameters.push_back(0.0);
        }
        return parameters;
    };
    switch (curve.GetType()) {
    case GeomAbs_Line:
        extrema.attain(0.0);
        return true;
    case GeomAbs_Circle:
        extrema.attain(1.0 / curve.Circle().Radius());
        return true;
    case GeomAbs_Ellipse: {
        const double a = curve.Ellipse().MajorRadius();
        const double b = curve.Ellipse().MinorRadius();
        for (const double t : critical_parameters(first, last, M_PI_2)) {
            const double s = std::sin(t);
            const double c = std::cos(t);
            extrema.attain(a * b / std::pow(a * a * s * s + b * b * c * c, 1.5));
        }
        return true;
    }
    case GeomAbs_Parabola: {
        const double focal = curve.Parabola().Focal();
        for (const double u : with_vertex()) {
            extrema.attain((0.5 / focal) / std::pow(1.0 + u * u / (4.0 * focal * focal), 1.5));
        }
        return true;
    }
    case GeomAbs_Hyperbola: {
        const double a = curve.Hyperbola().MajorRadius();
        const double b = curve.Hyperbola().MinorRadius();
        for (const double u : with_vertex()) {
            const double sh = std::sinh(u);
            const double ch = std::cosh(u);
            extrema.attain(a * b / std::pow(a * a * sh * sh + b * b * ch * ch, 1.5));
        }
        return true;
    }
    default:
        return false;
    }
}

/* Trimmed Bezier or B-spline edge geometry as homogeneous Bezier pieces. */
std::vector<BezierPiece> bezier_pieces(const BRepAdaptor_Curve& curve, double first, double last) {
    std::vector<BezierPiece> pieces;
    if (curve.GetType() == GeomAbs_BezierCurve) {
        Handle(Geom_BezierCurve) bezier = Handle(Geom_BezierCurve)::DownCast(curve.Bezier()->Copy());
        bezier->Segment(first, last);
        pieces.push_back(homogeneous_piece(bezier));
        return pieces;
    }
    Handle(Geom_BSplineCurve) spline = Handle(Geom_BSplineCurve)::DownCast(curve.BSpline()->Copy());
    spline->Segment(first, last);
    GeomConvert_BSplineCurveToBezierCurve converter(spline);
    for (int index = 1; index <= converter.NbArcs(); ++index) {
        pieces.push_back(homogeneous_piece(converter.Arc(index)));
    }
    return pieces;
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_shape_edge_curvature_extrema(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t edge,
    double relative_tolerance,
    double* out_minimum,
    double* out_minimum_lower_bound,
    double* out_maximum,
    double* out_maximum_upper_bound,
    int* out_is_exact) {
    return guarded(session, [&] {
        if (out_minimum == nullptr || out_minimum_lower_bound == nullptr || out_maximum == nullptr
            || out_maximum_upper_bound == nullptr || out_is_exact == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curvature-extrema output is null");
        }
        *out_minimum = *out_minimum_lower_bound = *out_maximum = *out_maximum_upper_bound = 0.0;
        *out_is_exact = 0;
        // NOLINTNEXTLINE(readability-simplify-boolean-expr): the negated form also rejects NaN.
        if (!(relative_tolerance > 0.0 && relative_tolerance <= 1.0)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "curvature relative tolerance must be in (0, 1]");
        }
        const TopoDS_Shape* value = find_shape(session, edge);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "edge was not found");
        }
        if (value->ShapeType() != TopAbs_EDGE) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "shape is not an edge");
        }
        const BRepAdaptor_Curve curve(TopoDS::Edge(*value));
        const double first = curve.FirstParameter();
        const double last = curve.LastParameter();
        if (!std::isfinite(first) || !std::isfinite(last) || first >= last) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge has no finite parameter range");
        }

        CurvatureExtrema extrema;
        const GeomAbs_CurveType type = curve.GetType();
        if (type == GeomAbs_BezierCurve || type == GeomAbs_BSplineCurve) {
            std::string message;
            if (!bound_polynomial_curvature(bezier_pieces(curve, first, last), relative_tolerance, extrema, message)) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, std::move(message));
            }
        } else if (!analytic_curvature_extrema(curve, first, last, extrema)) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_GEOMETRY,
                "curvature extrema require a line, conic, Bezier, or B-spline edge");
        }
        if (!std::isfinite(extrema.minimum_lower) || !std::isfinite(extrema.maximum_upper)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "edge curvature is not finite");
        }
        *out_minimum = extrema.minimum;
        *out_minimum_lower_bound = extrema.minimum_lower;
        *out_maximum = extrema.maximum;
        *out_maximum_upper_bound = extrema.maximum_upper;
        *out_is_exact = extrema.exact ? 1 : 0;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_is_adjacent(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t parent,
    occt_bridge_shape_id_t first,
    occt_bridge_shape_id_t second,
    int* out_is_adjacent) {
    return guarded(session, [&] {
        if (out_is_adjacent == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_is_adjacent is null");
        }
        *out_is_adjacent = 0;
        const TopoDS_Shape* parent_shape = find_shape(session, parent);
        const TopoDS_Shape* first_shape = find_shape(session, first);
        const TopoDS_Shape* second_shape = find_shape(session, second);
        if (parent_shape == nullptr || first_shape == nullptr || second_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "adjacency shape was not found");
        }
        if (!belongs_to(*parent_shape, *first_shape) || !belongs_to(*parent_shape, *second_shape)) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "adjacency shapes must belong to the parent");
        }
        const TopAbs_ShapeEnum first_type = first_shape->ShapeType();
        const TopAbs_ShapeEnum second_type = second_shape->ShapeType();
        bool adjacent = false;
        if (first_shape->IsSame(*second_shape)) {
            adjacent = false;
        } else if (first_type == TopAbs_FACE && second_type == TopAbs_EDGE) {
            adjacent = is_descendant(*first_shape, *second_shape, TopAbs_EDGE);
        } else if (first_type == TopAbs_EDGE && second_type == TopAbs_FACE) {
            adjacent = is_descendant(*second_shape, *first_shape, TopAbs_EDGE);
        } else if (first_type == TopAbs_FACE && second_type == TopAbs_FACE) {
            adjacent = shares_descendant(*first_shape, *second_shape, TopAbs_EDGE);
        } else if (first_type == TopAbs_EDGE && second_type == TopAbs_EDGE) {
            adjacent = shares_descendant(*first_shape, *second_shape, TopAbs_VERTEX);
        } else {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "unsupported adjacency topology pair");
        }
        *out_is_adjacent = adjacent ? 1 : 0;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_is_same(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t first,
    occt_bridge_shape_id_t second,
    int* out_is_same) {
    return guarded(session, [&] {
        if (out_is_same == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_is_same is null");
        }
        *out_is_same = 0;
        const TopoDS_Shape* first_shape = find_shape(session, first);
        const TopoDS_Shape* second_shape = find_shape(session, second);
        if (first_shape == nullptr || second_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "identity shape was not found");
        }
        *out_is_same = first_shape->IsSame(*second_shape) ? 1 : 0;
        return succeed(session);
    });
}

}  // extern "C"

namespace {

/* True when the faces share an edge with recorded G1-or-better continuity. */
bool faces_share_tangent_edge(const TopoDS_Face& first, const TopoDS_Face& second) {
    for (TopExp_Explorer first_edges(first, TopAbs_EDGE); first_edges.More(); first_edges.Next()) {
        const TopoDS_Edge edge = TopoDS::Edge(first_edges.Current());
        for (TopExp_Explorer second_edges(second, TopAbs_EDGE); second_edges.More(); second_edges.Next()) {
            if (edge.IsSame(second_edges.Current()) && BRep_Tool::HasContinuity(edge, first, second)
                && BRep_Tool::Continuity(edge, first, second) >= GeomAbs_G1) {
                return true;
            }
        }
    }
    return false;
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_shape_faces_are_tangent(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t parent,
    occt_bridge_shape_id_t first_face,
    occt_bridge_shape_id_t second_face,
    int* out_are_tangent) {
    return guarded(session, [&] {
        if (out_are_tangent == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_are_tangent is null");
        }
        *out_are_tangent = 0;
        const TopoDS_Shape* parent_shape = find_shape(session, parent);
        const TopoDS_Shape* first_shape = find_shape(session, first_face);
        const TopoDS_Shape* second_shape = find_shape(session, second_face);
        if (parent_shape == nullptr || first_shape == nullptr || second_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "tangency shape was not found");
        }
        if (first_shape->ShapeType() != TopAbs_FACE || second_shape->ShapeType() != TopAbs_FACE) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "tangency requires two faces");
        }
        if (!belongs_to(*parent_shape, *first_shape) || !belongs_to(*parent_shape, *second_shape)) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "tangency faces must belong to the parent");
        }
        if (!first_shape->IsSame(*second_shape)
            && faces_share_tangent_edge(TopoDS::Face(*first_shape), TopoDS::Face(*second_shape))) {
            *out_are_tangent = 1;
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_history_count(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t source,
    occt_bridge_history_relation_t relation,
    size_t* out_count) {
    return guarded(session, [&] {
        if (out_count == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_count is null");
        }
        *out_count = 0;
        const TopoDS_Shape* result_shape = find_shape(session, result);
        const TopoDS_Shape* source_shape = find_shape(session, source);
        if (result_shape == nullptr || source_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "history shape was not found");
        }
        static_cast<void>(result_shape);
        const auto result_history = session->histories.find(result);
        if (result_history == session->histories.end()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "result shape has no operation history");
        }
        occt_bridge_history_entry resolved;
        if (!resolve_history_entry(result_history->second, *source_shape, resolved)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "source is not an input to the result operation");
        }
        const occt_bridge_history_entry* entry = &resolved;
        const std::vector<TopoDS_Shape>* related = history_relation_shapes(*entry, relation);
        if (related == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "unknown history relation");
        }
        *out_count = related->size();
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_history_at(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t source,
    occt_bridge_history_relation_t relation,
    size_t index,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const TopoDS_Shape* result_shape = find_shape(session, result);
        const TopoDS_Shape* source_shape = find_shape(session, source);
        if (result_shape == nullptr || source_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "history shape was not found");
        }
        static_cast<void>(result_shape);
        const auto result_history = session->histories.find(result);
        if (result_history == session->histories.end()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "result shape has no operation history");
        }
        occt_bridge_history_entry resolved;
        if (!resolve_history_entry(result_history->second, *source_shape, resolved)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "source is not an input to the result operation");
        }
        const occt_bridge_history_entry* entry = &resolved;
        const std::vector<TopoDS_Shape>* related = history_relation_shapes(*entry, relation);
        if (related == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "unknown history relation");
        }
        if (index >= related->size()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "history index is out of range");
        }
        return store_shape(session, (*related)[index], out_shape);
    });
}

occt_bridge_status_t occt_bridge_shape_history_is_deleted(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t result,
    occt_bridge_shape_id_t source,
    int* out_is_deleted) {
    return guarded(session, [&] {
        if (out_is_deleted == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_is_deleted is null");
        }
        *out_is_deleted = 0;
        const TopoDS_Shape* result_shape = find_shape(session, result);
        const TopoDS_Shape* source_shape = find_shape(session, source);
        if (result_shape == nullptr || source_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "history shape was not found");
        }
        static_cast<void>(result_shape);
        const auto result_history = session->histories.find(result);
        if (result_history == session->histories.end()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "result shape has no operation history");
        }
        occt_bridge_history_entry resolved;
        if (!resolve_history_entry(result_history->second, *source_shape, resolved)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "source is not an input to the result operation");
        }
        const occt_bridge_history_entry* entry = &resolved;
        *out_is_deleted = entry->deleted ? 1 : 0;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_is_valid(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    int* out_is_valid) {
    return guarded(session, [&] {
        if (out_is_valid == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_is_valid is null");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        const ShapeValidator analyzer(*value);
        *out_is_valid = analyzer.IsValid() ? 1 : 0;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_remove(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape) {
    return guarded(session, [&] {
        if (session->shapes.erase(shape) == 0) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        session->histories.erase(shape);
        return succeed(session);
    });
}

void occt_bridge_shape_release(occt_bridge_session_t* session, occt_bridge_shape_id_t shape) {
    if (session == nullptr) {
        return;
    }
    try {
        std::lock_guard<std::mutex> lock(session->mutex);
        session->shapes.erase(shape);
        session->histories.erase(shape);
    } catch (...) {  // NOLINT(bugprone-empty-catch): release has no status to report and must not throw across the C ABI.
    }
}

occt_bridge_status_t occt_bridge_brep_save(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const char* path) {
    return guarded(session, [&] {
        if (path == nullptr || path[0] == '\0') {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "path is empty");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        if (!BRepTools::Write(*value, path)) {
            return fail(session, OCCT_BRIDGE_IO_ERROR, "failed to write BREP file");
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_brep_load(
    occt_bridge_session_t* session,
    const char* path,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (path == nullptr || path[0] == '\0') {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "path is empty");
        }
        TopoDS_Shape shape;
        BRep_Builder builder;
        if (!BRepTools::Read(shape, path, builder) || shape.IsNull()) {
            return fail(session, OCCT_BRIDGE_IO_ERROR, "failed to read BREP file");
        }
        return store_checked_import(session, "BREP load", shape, out_shape);
    });
}

occt_bridge_status_t occt_bridge_step_save(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const char* path) {
    return guarded(session, [&] {
        if (path == nullptr || path[0] == '\0') {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "path is empty");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        STEPControl_Writer writer;
        if (writer.Transfer(*value, STEPControl_AsIs) != IFSelect_RetDone) {
            return fail(session, OCCT_BRIDGE_IO_ERROR, "failed to transfer shape to STEP");
        }
        if (writer.Write(path) != IFSelect_RetDone) {
            return fail(session, OCCT_BRIDGE_IO_ERROR, "failed to write STEP file");
        }
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_step_load(
    occt_bridge_session_t* session,
    const char* path,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (path == nullptr || path[0] == '\0') {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "path is empty");
        }
        STEPControl_Reader reader;
        if (reader.ReadFile(path) != IFSelect_RetDone) {
            return fail(session, OCCT_BRIDGE_IO_ERROR, "failed to read STEP file");
        }
        if (reader.TransferRoots() == 0) {
            return fail(session, OCCT_BRIDGE_IO_ERROR, "STEP file contains no transferable roots");
        }
        TopoDS_Shape shape = reader.OneShape();
        if (shape.IsNull()) {
            return fail(session, OCCT_BRIDGE_IO_ERROR, "STEP file contains no shape");
        }
        return store_checked_import(session, "STEP import", shape, out_shape);
    });
}

occt_bridge_status_t occt_bridge_stl_save(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const char* path,
    double linear_deflection,
    double angular_deflection_radians,
    int binary) {
    return guarded(session, [&] {
        if (path == nullptr || path[0] == '\0') {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "path is empty");
        }
        if (!std::isfinite(linear_deflection) || linear_deflection <= 0.0) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "linear deflection must be finite and positive");
        }
        if (!std::isfinite(angular_deflection_radians) || angular_deflection_radians <= 0.0) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "angular deflection must be finite and positive");
        }
        if (binary != 0 && binary != 1) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "binary must be 0 or 1");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        /*
         * Mesh a topology copy that shares geometry. Triangulations are cached
         * on faces, so meshing the session's shape would keep a finer earlier
         * mesh for later exports and leak it into BREP output.
         */
        BRepBuilderAPI_Copy copy(*value, Standard_False, Standard_False);
        if (!copy.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "failed to copy shape for STL export");
        }
        const TopoDS_Shape exported = copy.Shape();
        BRepMesh_IncrementalMesh mesh(
            exported,
            linear_deflection,
            Standard_False,
            angular_deflection_radians,
            Standard_False);
        if (!mesh.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "failed to mesh shape for STL export");
        }
        StlAPI_Writer writer;
        writer.ASCIIMode() = binary == 0 ? Standard_True : Standard_False;
        if (!writer.Write(exported, path)) {
            return fail(session, OCCT_BRIDGE_IO_ERROR, "failed to write STL file");
        }
        return succeed(session);
    });
}

}  // extern "C"
