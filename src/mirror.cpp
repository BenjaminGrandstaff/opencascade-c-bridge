/* Plane reflection with transformed topology and native modification history. */
#include "bridge_internal.hpp"
#include <BRepBuilderAPI_Transform.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <TopExp.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <algorithm>
#include <cmath>
using namespace occt_bridge_internal;

namespace {
void orient_targets(std::vector<TopoDS_Shape>& targets, const TopTools_IndexedMapOfShape& output) {
    for (auto& target : targets) {
        const int index = output.FindIndex(target);
        if (index != 0) target = output.FindKey(index);
    }
}
}

occt_bridge_status_t occt_bridge_mirror(
    occt_bridge_session_t* session, occt_bridge_shape_id_t shape,
    occt_bridge_vec3_t origin, occt_bridge_vec3_t normal,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(origin) || !finite(normal)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "mirror plane must be finite");
        }
        const double scale = std::max({std::abs(normal.x), std::abs(normal.y), std::abs(normal.z)});
        if (scale == 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "mirror plane normal must be nonzero");
        }
        const auto* input = find_shape(session, shape);
        if (input == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "mirror input was not found");
        }
        gp_Trsf transform;
        transform.SetMirror(gp_Ax2(to_point(origin),
            gp_Dir(normal.x / scale, normal.y / scale, normal.z / scale)));
        // Reflection has negative determinant; OCCT copies transformed geometry
        // rather than using a rigid location. Existing source topology stays intact.
        BRepBuilderAPI_Transform operation(*input, transform, Standard_False);
        if (!operation.IsDone() || operation.Shape().IsNull()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "mirror construction failed");
        }
        TopoDS_Shape result = operation.Shape();
        auto history = collect_history(operation, {input});
        const auto status = check_result(session, "mirror", result, &history);
        if (status != OCCT_BRIDGE_OK) return status;
        // OCCT modification targets can omit the face reversal applied within
        // the result shell. Canonicalize to actual output members after healing.
        TopTools_IndexedMapOfShape output;
        TopExp::MapShapes(result, output);
        for (auto& entry : history) {
            orient_targets(entry.generated, output);
            orient_targets(entry.modified, output);
        }
        return store_shape_with_entries(session, result, out_shape, std::move(history));
    });
}
