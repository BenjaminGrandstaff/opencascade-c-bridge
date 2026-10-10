/*
 * Draft angles on selected faces, with failure diagnostics and history.
 */

#include "bridge_internal.hpp"

#include <BRepAdaptor_Surface.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <Standard_NoSuchObject.hxx>
#include <TopoDS.hxx>

#include <algorithm>
#include <cmath>

using namespace occt_bridge_internal;

namespace {

occt_bridge_status_t select_draft_faces(
    occt_bridge_session_t* session,
    const TopoDS_Shape& input,
    const occt_bridge_shape_id_t* faces,
    size_t face_count,
    std::vector<TopoDS_Face>& selected) {
    for (size_t index = 0; index < face_count; ++index) {
        const TopoDS_Shape* face = find_shape(session, faces[index]);
        if (face == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "draft face was not found");
        }
        if (face->ShapeType() != TopAbs_FACE || !is_descendant(input, *face, TopAbs_FACE)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "draft selection is not a face of the input");
        }
        if (std::any_of(selected.begin(), selected.end(), [&](const TopoDS_Face& prior) { return prior.IsSame(*face); })) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "duplicate draft face");
        }
        const TopoDS_Face typed = TopoDS::Face(*face);
        const GeomAbs_SurfaceType type = BRepAdaptor_Surface(typed).GetType();
        if (type != GeomAbs_Plane && type != GeomAbs_Cylinder && type != GeomAbs_Cone) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "draft requires planar, cylindrical, or conical faces");
        }
        selected.push_back(typed);
    }
    return OCCT_BRIDGE_OK;
}

occt_bridge_status_t draft_failure(
    occt_bridge_session_t* session,
    const BRepOffsetAPI_DraftAngle& builder,
    int64_t index,
    const std::string& exception) {
    const Draft_ErrorStatus error = builder.Status();
    const char* const names[] = {"Draft_NoError", "Draft_FaceRecomputation", "Draft_EdgeRecomputation", "Draft_VertexRecomputation"};
    static_assert(Draft_VertexRecomputation == 3, "Draft_ErrorStatus changed");
    add_diagnostic(session, {OCCT_BRIDGE_DIAGNOSTIC_DRAFT, static_cast<int32_t>(error), index,
        enum_name(names, static_cast<int32_t>(error), "Draft_ErrorStatus_"), builder.ProblematicShape()});
    return fail(session, OCCT_BRIDGE_KERNEL_ERROR, with_diagnostics(session,
        "draft construction failed" + (exception.empty() ? std::string{} : ": " + exception)));
}

std::vector<occt_bridge_history_entry> draft_history(
    BRepOffsetAPI_DraftAngle& builder, const TopoDS_Shape* value) {
    auto history = collect_history(builder, {value});
    // ModifiedShape supplies corrected counterparts omitted by Modified().
    for (auto& entry : history) {
        if (!entry.modified.empty()) {
            continue;
        }
        try {
            const TopoDS_Shape changed = builder.ModifiedShape(entry.source);
            if (!changed.IsNull() && !changed.IsSame(entry.source)) {
                append_same_type(entry.modified, changed, entry.source.ShapeType());
            }
        } catch (const Standard_NoSuchObject&) {
            continue; // This source was not transformed.
        }
    }
    return history;
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_draft(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* faces,
    size_t face_count,
    occt_bridge_vec3_t neutral_origin,
    occt_bridge_vec3_t neutral_normal,
    occt_bridge_vec3_t pull_direction,
    double angle_radians,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const auto nonzero = [](occt_bridge_vec3_t v) {
            return finite(v) && std::hypot(v.x, v.y, v.z) > 0.0;
        };
        if (faces == nullptr || face_count == 0 || !finite(neutral_origin)
            || !nonzero(neutral_normal) || !nonzero(pull_direction)
            || !std::isfinite(angle_radians) || angle_radians == 0.0
            || std::abs(angle_radians) >= std::acos(-1.0) / 2.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid draft parameters");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "draft input was not found");
        }
        std::vector<TopoDS_Face> selected;
        const occt_bridge_status_t selection_status = select_draft_faces(session, *value, faces, face_count, selected);
        if (selection_status != OCCT_BRIDGE_OK) {
            return selection_status;
        }
        BRepOffsetAPI_DraftAngle builder(*value);
        const gp_Pln plane(gp_Pnt(neutral_origin.x, neutral_origin.y, neutral_origin.z),
                           gp_Dir(neutral_normal.x, neutral_normal.y, neutral_normal.z));
        const gp_Dir direction(pull_direction.x, pull_direction.y, pull_direction.z);
        for (size_t index = 0; index < selected.size(); ++index) {
            const std::string exception = perform_reporting_exception([&] {
                builder.Add(selected[index], direction, angle_radians, plane);
            });
            if (!exception.empty() || !builder.AddDone()) {
                return draft_failure(session, builder, static_cast<int64_t>(index), exception);
            }
        }
        const std::string exception = perform_reporting_exception([&] { builder.Build(); });
        if (!exception.empty() || !builder.IsDone() || builder.Shape().IsNull()) {
            return draft_failure(session, builder, -1, exception);
        }
        auto history = draft_history(builder, value);
        TopoDS_Shape result = builder.Shape();
        const occt_bridge_status_t status = check_result(session, "draft", result, &history);
        if (status != OCCT_BRIDGE_OK) {
            return status;
        }
        return store_shape_with_entries(session, result, out_shape, std::move(history));
    });
}

}  // extern "C"
