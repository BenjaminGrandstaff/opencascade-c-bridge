/*
 * Helpers shared by the C entry points: failure reporting, result storage
 * with operation history, validation and healing, diagnostics, and topology
 * membership.
 */

#include "bridge_internal.hpp"

#include <BRepCheck_ListOfStatus.hxx>
#include <BRepCheck_Result.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeFix_Shape.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>

#include <algorithm>
#include <cmath>

namespace occt_bridge_internal {

bool finite(double value) {
    return std::isfinite(value);
}

bool finite(const occt_bridge_vec3_t& value) {
    return finite(value.x) && finite(value.y) && finite(value.z);
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

void append_same_type(std::vector<TopoDS_Shape>& shapes, const TopoDS_Shape& replacement, TopAbs_ShapeEnum type) {
    if (replacement.ShapeType() == type) {
        append_unique_shape(shapes, replacement);
        return;
    }
    for (TopExp_Explorer explorer(replacement, type); explorer.More(); explorer.Next()) {
        append_unique_shape(shapes, explorer.Current());
    }
}

void map_through_reshape(std::vector<TopoDS_Shape>& targets, const opencascade::handle<BRepTools_History>& reshape) {
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

void compose_with_reshape(std::vector<occt_bridge_history_entry>& history, const opencascade::handle<BRepTools_History>& reshape) {
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

} // namespace occt_bridge_internal
