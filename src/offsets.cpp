/*
 * Skin offsets and hollowing, with OCCT offset error diagnostics.
 */

#include "bridge_internal.hpp"

#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>

#include <cmath>

using namespace occt_bridge_internal;

namespace {

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

}  // namespace

extern "C" {

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

}  // extern "C"
