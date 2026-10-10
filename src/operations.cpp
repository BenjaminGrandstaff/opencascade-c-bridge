/*
 * Modeling operations: booleans, same-domain unification, and rigid
 * and scaling transforms, with failure diagnostics.
 */

#include "bridge_internal.hpp"

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_AlertWithShape.hxx>

#include <cmath>
#include <limits>
#include <sstream>

using namespace occt_bridge_internal;

namespace {

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
    // Near-unit scaling is still scaling, especially for large source parts.
    const bool rigid = transform.ScaleFactor() == 1.0 && !transform.IsNegative();
    if (rigid) {
        return store_located_shape(session, *value, TopLoc_Location(transform), out_shape);
    }
    BRepBuilderAPI_Transform operation(*value, transform, Standard_True);
    operation.Build();
    if (!operation.IsDone() || operation.Shape().IsNull()) {
        return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "shape transform failed");
    }
    return store_checked_result(
        session,
        "shape transform",
        operation.Shape(),
        out_shape,
        operation,
        {value});
}

/*
 * Presents a BRepTools_History through the Generated/Modified/IsDeleted
 * interface collect_history expects; unsupported types have no records.
 */
struct ToolsHistory {
    opencascade::handle<BRepTools_History> history;
    TopTools_ListOfShape none;

    const TopTools_ListOfShape& Generated(const TopoDS_Shape& source) const {
        return BRepTools_History::IsSupportedType(source) ? history->Generated(source) : none;
    }
    const TopTools_ListOfShape& Modified(const TopoDS_Shape& source) const {
        return BRepTools_History::IsSupportedType(source) ? history->Modified(source) : none;
    }
    bool IsDeleted(const TopoDS_Shape& source) const {
        return BRepTools_History::IsSupportedType(source) && history->IsRemoved(source);
    }
};

}  // namespace

extern "C" {

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

occt_bridge_status_t occt_bridge_unify_same_domain(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    double linear_tolerance,
    double angular_tolerance,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!std::isfinite(linear_tolerance) || linear_tolerance <= 0.0 || !std::isfinite(angular_tolerance)
            || angular_tolerance <= 0.0 || angular_tolerance >= M_PI_2) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_ARGUMENT,
                "unify needs a positive linear tolerance and an angular tolerance in (0, pi/2) radians");
        }
        const TopoDS_Shape* value = find_shape(session, shape);
        if (value == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        ShapeUpgrade_UnifySameDomain unify(*value, Standard_True, Standard_True, Standard_False);
        unify.SetLinearTolerance(linear_tolerance);
        unify.SetAngularTolerance(angular_tolerance);
        const std::string exception = perform_reporting_exception([&] { unify.Build(); });
        if (!exception.empty() || unify.Shape().IsNull()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, exception.empty() ? "unify same domain failed" : "unify same domain failed: " + exception);
        }
        ToolsHistory history{unify.History(), {}};
        return store_checked_result(session, "unify same domain", unify.Shape(), out_shape, history, {value});
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

}  // extern "C"
