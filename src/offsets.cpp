/*
 * Skin offsets and hollowing, with OCCT offset error diagnostics.
 */

#include "bridge_internal.hpp"

#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Solid.hxx>

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
    const BRepOffset_MakeOffset& offset,
    const std::string& name,
    const std::string& exception,
    const TopTools_ListOfShape& selection) {
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
 * A skin offset of a sheet by `distance` with intersection joins; with
 * `thicken`, the sheet and its offset close into a solid. Returns the OCCT
 * exception name, or "" when the step ran.
 */
std::string offset_sheet(BRepOffset_MakeOffset& builder, const TopoDS_Shape& sheet, double distance, double tolerance, bool thicken) {
    return perform_reporting_exception([&] {
        builder.Initialize(
            sheet, distance, tolerance, BRepOffset_Skin, Standard_False, Standard_False,
            GeomAbs_Intersection, thicken ? Standard_True : Standard_False);
        builder.MakeOffsetShape();
    });
}

bool offset_done(const BRepOffset_MakeOffset& builder, const std::string& exception) {
    return exception.empty() && builder.IsDone() && !builder.Shape().IsNull();
}

/*
 * History of a thicken. `thick` closed `skin` (the sheet, or with `moved` its
 * half offset) into a solid. Each sheet subshape maps to its skin image
 * (modified, symmetric only), the images OCCT recorded for that skin shape,
 * and for edges the side walls that contain them: result faces adjacent to
 * the skin edge that are not skin faces.
 */
std::vector<occt_bridge_history_entry> thicken_history(
    BRepOffset_MakeOffset& thick,
    BRepOffset_MakeOffset* moved,
    const TopoDS_Shape& sheet) {
    const TopoDS_Shape& skin = moved == nullptr ? sheet : moved->Shape();
    TopTools_IndexedMapOfShape result;
    TopExp::MapShapes(thick.Shape(), result);
    TopTools_IndexedMapOfShape skin_faces;
    TopExp::MapShapes(skin, TopAbs_FACE, skin_faces);
    TopTools_IndexedDataMapOfShapeListOfShape edge_faces;
    TopExp::MapShapesAndAncestors(thick.Shape(), TopAbs_EDGE, TopAbs_FACE, edge_faces);
    const auto through_thick = [&](const TopoDS_Shape& image, occt_bridge_history_entry& entry) {
        if (moved != nullptr && result.Contains(image)) {
            append_unique_shape(entry.modified, image);
        }
        for (TopTools_ListIteratorOfListOfShape it(thick.Generated(image)); it.More(); it.Next()) {
            append_unique_shape(entry.generated, it.Value());
        }
        for (TopTools_ListIteratorOfListOfShape it(thick.Modified(image)); it.More(); it.Next()) {
            append_unique_shape(entry.modified, it.Value());
        }
        if (image.ShapeType() == TopAbs_EDGE && edge_faces.Contains(image)) {
            for (TopTools_ListIteratorOfListOfShape it(edge_faces.FindFromKey(image)); it.More(); it.Next()) {
                if (!skin_faces.Contains(it.Value())) {
                    append_unique_shape(entry.generated, it.Value());
                }
            }
        }
    };
    std::vector<occt_bridge_history_entry> history;
    for (const TopoDS_Shape& source : history_sources({&sheet})) {
        occt_bridge_history_entry entry;
        entry.source = source;
        if (moved == nullptr) {
            through_thick(source, entry);
            entry.deleted = !result.Contains(source) && entry.modified.empty() && entry.generated.empty();
        } else {
            std::vector<TopoDS_Shape> images;
            for (TopTools_ListIteratorOfListOfShape it(moved->Generated(source)); it.More(); it.Next()) {
                append_unique_shape(images, it.Value());
            }
            for (TopTools_ListIteratorOfListOfShape it(moved->Modified(source)); it.More(); it.Next()) {
                append_unique_shape(images, it.Value());
            }
            for (const TopoDS_Shape& image : images) {
                through_thick(image, entry);
            }
            entry.deleted = entry.modified.empty() && entry.generated.empty();
        }
        history.push_back(std::move(entry));
    }
    return history;
}

/* Checks and stores a thickened solid with its sheet history. */
occt_bridge_status_t store_thickened(
    occt_bridge_session_t* session,
    BRepOffset_MakeOffset& thick,
    BRepOffset_MakeOffset* moved,
    const TopoDS_Shape& sheet,
    occt_bridge_shape_id_t* out_shape) {
    TopoDS_Shape solid = thick.Shape();
    std::vector<occt_bridge_history_entry> history = thicken_history(thick, moved, sheet);
    const occt_bridge_status_t status = check_result(session, "thicken", solid, &history);
    if (status != OCCT_BRIDGE_OK) {
        return status;
    }
    return store_shape_with_entries(session, solid, out_shape, std::move(history));
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
            return offset_failure(session, builder.MakeOffset(), "offset", exception, {});
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
            return offset_failure(session, builder.MakeOffset(), "hollow", exception, closing_faces);
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

occt_bridge_status_t occt_bridge_thicken(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double thickness,
    int symmetric,
    double tolerance,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!std::isfinite(thickness) || thickness == 0.0
            || !std::isfinite(tolerance) || tolerance <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid thicken parameters");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        if (!contains_topology(*value, TopAbs_FACE) || contains_topology(*value, TopAbs_SOLID)) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_GEOMETRY,
                "thicken input must be faces or shells without solids");
        }
        if (symmetric == 0) {
            BRepOffset_MakeOffset thick;
            const std::string exception = offset_sheet(thick, *value, thickness, tolerance, true);
            if (!offset_done(thick, exception)) {
                return offset_failure(session, thick, "thicken", exception, {});
            }
            return store_thickened(session, thick, nullptr, *value, out_shape);
        }
        BRepOffset_MakeOffset moved;
        std::string exception = offset_sheet(moved, *value, -thickness / 2.0, tolerance, false);
        if (!offset_done(moved, exception)) {
            return offset_failure(session, moved, "thicken", exception, {});
        }
        BRepOffset_MakeOffset thick;
        exception = offset_sheet(thick, moved.Shape(), thickness, tolerance, true);
        if (!offset_done(thick, exception)) {
            return offset_failure(session, thick, "thicken", exception, {});
        }
        return store_thickened(session, thick, &moved, *value, out_shape);
    });
}

occt_bridge_status_t occt_bridge_offset_faces(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* faces,
    const double* distances,
    size_t face_count,
    double tolerance,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (faces == nullptr || distances == nullptr || face_count == 0
            || !std::isfinite(tolerance) || tolerance <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid offset faces parameters");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        TopTools_IndexedMapOfShape solids;
        TopExp::MapShapes(*value, TopAbs_SOLID, solids);
        if (solids.Extent() != 1) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "offset faces input must be one solid");
        }
        TopTools_IndexedMapOfShape owned;
        TopExp::MapShapes(*value, TopAbs_FACE, owned);
        TopTools_IndexedMapOfShape seen;
        TopTools_ListOfShape selection;
        for (size_t index = 0; index < face_count; ++index) {
            const TopoDS_Shape* face = find_shape(session, faces[index]);
            if (face == nullptr) {
                return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "offset faces face was not found");
            }
            if (face->ShapeType() != TopAbs_FACE || !owned.Contains(*face)) {
                return fail(
                    session,
                    OCCT_BRIDGE_INVALID_GEOMETRY,
                    "offset faces selection is not a face of the input shape");
            }
            if (!std::isfinite(distances[index]) || distances[index] == 0.0) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "offset faces distances must be finite and nonzero");
            }
            if (seen.Add(*face) != static_cast<int>(index) + 1) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "offset faces selects a face twice");
            }
            selection.Append(*face);
        }
        BRepOffset_MakeOffset builder;
        const std::string exception = perform_reporting_exception([&] {
            builder.Initialize(
                *value, 0.0, tolerance, BRepOffset_Skin, Standard_False, Standard_False,
                GeomAbs_Intersection, Standard_False);
            size_t index = 0;
            for (TopTools_ListIteratorOfListOfShape face(selection); face.More(); face.Next(), ++index) {
                builder.SetOffsetOnFace(TopoDS::Face(face.Value()), distances[index]);
            }
            builder.MakeOffsetShape();
        });
        if (!offset_done(builder, exception)) {
            return offset_failure(session, builder, "offset faces", exception, selection);
        }
        // A zero skin offset returns the closed shells; the solid reuses
        // them, so the recorded face history still applies.
        TopoDS_Shape result = builder.Shape();
        if (!contains_topology(result, TopAbs_SOLID)) {
            BRepBuilderAPI_MakeSolid solid;
            for (TopExp_Explorer shell(result, TopAbs_SHELL); shell.More(); shell.Next()) {
                solid.Add(TopoDS::Shell(shell.Current()));
            }
            if (!solid.IsDone()) {
                return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "offset faces left no closed shell");
            }
            TopoDS_Solid closed = solid.Solid();
            BRepLib::OrientClosedSolid(closed);
            result = closed;
        }
        // OCCT reports every offset image as generated; an image of the
        // source's own type is the same face, edge or vertex moved.
        std::vector<occt_bridge_history_entry> history = collect_history(builder, {value});
        for (occt_bridge_history_entry& entry : history) {
            std::vector<TopoDS_Shape> generated;
            for (const TopoDS_Shape& image : entry.generated) {
                if (image.ShapeType() == entry.source.ShapeType()) {
                    append_unique_shape(entry.modified, image);
                } else {
                    generated.push_back(image);
                }
            }
            entry.generated = std::move(generated);
        }
        const occt_bridge_status_t status = check_result(session, "offset faces", result, &history);
        if (status != OCCT_BRIDGE_OK) {
            return status;
        }
        return store_shape_with_entries(session, result, out_shape, std::move(history));
    });
}

}  // extern "C"
