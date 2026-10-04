/*
 * Sewing faces and shells and constructing solids from shells.
 */

#include "bridge_internal.hpp"

#include <BRepAlgoAPI_Common.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Solid.hxx>

#include <algorithm>
#include <cmath>

using namespace occt_bridge_internal;

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
    return std::abs(measure_volume(shape));
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

}  // extern "C"
