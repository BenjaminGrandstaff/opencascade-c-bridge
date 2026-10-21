/*
 * Direct face edits (ABI 86): removing faces by defeaturing, with OCCT's
 * alerts mapped to the selected faces they concern.
 */

#include "bridge_internal.hpp"

#include <BRepAlgoAPI_Defeaturing.hxx>
#include <Message_Report.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS_AlertWithShape.hxx>
#include <TopExp.hxx>

using namespace occt_bridge_internal;

namespace {

/*
 * Records the defeaturing's error and warning alerts, errors first, each with
 * the index of the first selected face its shape contains, or -1.
 * O(alerts * selection) on failure only.
 */
void record_defeaturing_alerts(
    occt_bridge_session_t* session,
    const BRepAlgoAPI_Defeaturing& operation,
    const std::vector<TopoDS_Shape>& selection) {
    for (const Message_Gravity gravity : {Message_Fail, Message_Warning}) {
        const Message_ListOfAlert& alerts = operation.GetReport()->GetAlerts(gravity);
        for (Message_ListOfAlert::Iterator alert(alerts); alert.More(); alert.Next()) {
            TopoDS_Shape shape;
            const Handle(TopoDS_AlertWithShape) with_shape = Handle(TopoDS_AlertWithShape)::DownCast(alert.Value());
            if (!with_shape.IsNull()) {
                shape = with_shape->GetShape();
            }
            int64_t selected = -1;
            if (!shape.IsNull()) {
                TopTools_IndexedMapOfShape faces;
                TopExp::MapShapes(shape, TopAbs_FACE, faces);
                for (size_t index = 0; index < selection.size(); ++index) {
                    if (faces.Contains(selection[index])) {
                        selected = static_cast<int64_t>(index);
                        break;
                    }
                }
            }
            add_diagnostic(
                session,
                {OCCT_BRIDGE_DIAGNOSTIC_DEFEATURING, 0, selected, alert.Value()->GetMessageKey(), shape});
        }
    }
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_remove_faces(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* faces,
    size_t face_count,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (faces == nullptr || face_count == 0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "remove faces needs at least one face");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        if (!contains_topology(*value, TopAbs_SOLID)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "remove faces input must contain a solid");
        }
        TopTools_IndexedMapOfShape owned;
        TopExp::MapShapes(*value, TopAbs_FACE, owned);
        std::vector<TopoDS_Shape> selection;
        selection.reserve(face_count);
        BRepAlgoAPI_Defeaturing operation;
        operation.SetShape(*value);
        for (size_t index = 0; index < face_count; ++index) {
            const TopoDS_Shape* face = find_shape(session, faces[index]);
            if (face == nullptr) {
                return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "remove faces face was not found");
            }
            if (face->ShapeType() != TopAbs_FACE || !owned.Contains(*face)) {
                return fail(
                    session,
                    OCCT_BRIDGE_INVALID_GEOMETRY,
                    "remove faces selection is not a face of the input shape");
            }
            selection.push_back(*face);
            operation.AddFaceToRemove(*face);
        }
        operation.SetRunParallel(Standard_False);
        operation.SetToFillHistory(Standard_True);
        const std::string exception = perform_reporting_exception([&] { operation.Build(); });
        // OCCT keeps a feature it cannot remove and only warns; a partial
        // removal is a failure here.
        if (!exception.empty() || !operation.IsDone() || operation.HasErrors()
            || operation.HasWarnings() || operation.Shape().IsNull()) {
            record_defeaturing_alerts(session, operation, selection);
            std::string message = "remove faces failed";
            if (!exception.empty()) {
                message += " (" + exception + ")";
            }
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, with_diagnostics(session, message));
        }
        return store_checked_result(
            session,
            "remove faces",
            operation.Shape(),
            out_shape,
            operation,
            {value});
    });
}

}  // extern "C"
