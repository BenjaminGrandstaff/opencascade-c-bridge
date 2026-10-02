/*
 * BREP, STEP, and STL exchange.
 */

#include "bridge_internal.hpp"

#include <BRepBuilderAPI_Copy.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPControl_Writer.hxx>
#include <StlAPI_Writer.hxx>

#include <cmath>

using namespace occt_bridge_internal;

namespace {

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

}  // namespace

extern "C" {

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
