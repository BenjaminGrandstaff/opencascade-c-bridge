/*
 * Fillets, variable fillets, and chamfers, with per-contour failure
 * isolation and diagnostics.
 */

#include "bridge_internal.hpp"

#include <BRep_Tool.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <gp_Pnt.hxx>
#include <Precision.hxx>
#include <TColgp_Array1OfPnt2d.hxx>
#include <TopExp.hxx>
#include <TopoDS.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_MapOfShape.hxx>

#include <algorithm>
#include <cmath>
#include <limits>

using namespace occt_bridge_internal;

namespace {

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

struct LinearFilletRadii {
    double start;
    double end;
};

struct StationFilletRadii {
    const occt_bridge_fillet_station_t* stations;
    size_t count;
    int32_t direction;
    occt_bridge_vec3_t start_point;
};

// Concurrent variable-radius builds in independent OCCT 7.9 sessions can
// fail spuriously with ChFiDS_WalkingFailure. Serialize the shared fillet /
// chamfer builder path, including its failure-isolation rebuilds.
std::mutex edge_treatment_mutex;

bool valid_treatment_value(double value) {
    return std::isfinite(value) && value > 0.0;
}

bool valid_treatment_value(const LinearFilletRadii& value) {
    return valid_treatment_value(value.start) && valid_treatment_value(value.end);
}

bool valid_treatment_value(const StationFilletRadii& value) {
    if (value.stations == nullptr || value.count < 2
        || value.count > static_cast<size_t>(std::numeric_limits<int>::max())
        || value.direction < 0 || value.direction > 2
        || (value.direction == 2 && !finite(value.start_point))) {
        return false;
    }
    if (value.stations[0].position != 0.0 || value.stations[value.count - 1].position != 1.0) {
        return false;
    }
    double previous = -1.0;
    for (size_t index = 0; index < value.count; ++index) {
        const auto& station = value.stations[index];
        if (!std::isfinite(station.position) || station.position <= previous
            || !valid_treatment_value(station.radius)) {
            return false;
        }
        previous = station.position;
    }
    return true;
}

// -1 is an ambiguous start point; 0/1 select kernel/reversed spine order.
int station_direction(const BRepFilletAPI_MakeFillet& builder, int contour, const StationFilletRadii& value) {
    if (value.direction != 2) {
        return value.direction;
    }
    const gp_Pnt point(value.start_point.x, value.start_point.y, value.start_point.z);
    const auto distance = [&point](const TopoDS_Vertex& vertex) {
        const auto end = BRep_Tool::Pnt(vertex);
        return std::hypot(point.X() - end.X(), point.Y() - end.Y(), point.Z() - end.Z());
    };
    const double first = distance(builder.FirstVertex(contour));
    const double last = distance(builder.LastVertex(contour));
    if (!std::isfinite(first) || !std::isfinite(last)) {
        return -1;
    }
    const double tolerance = std::max(Precision::Confusion(),
        64.0 * std::numeric_limits<double>::epsilon() * std::max(first, last));
    return std::abs(first - last) <= tolerance ? -1 : static_cast<int>(last < first);
}

template <typename Builder>
void add_treatment(Builder& builder, const TopoDS_Edge& edge, double value) {
    builder.Add(value, edge);
}

void add_treatment(BRepFilletAPI_MakeFillet& builder, const TopoDS_Edge& edge, const LinearFilletRadii& value) {
    // One law per tangent contour, even when several selected edges share it.
    if (builder.Contour(edge) == 0) {
        builder.Add(value.start, value.end, edge);
    }
}

// One interpolated law per contour; O(stations) setup/storage per new contour.
void add_treatment(BRepFilletAPI_MakeFillet& builder, const TopoDS_Edge& edge, const StationFilletRadii& value) {
    if (builder.Contour(edge) != 0) {
        return;
    }
    builder.Add(edge);
    const int contour = builder.Contour(edge);
    if (contour == 0 || builder.Closed(contour)) {
        return;
    }
    const int direction = station_direction(builder, contour, value);
    if (direction < 0) {
        return;
    }
    TColgp_Array1OfPnt2d law(1, static_cast<int>(value.count));
    for (size_t index = 0; index < value.count; ++index) {
        const auto& station = value.stations[direction == 1 ? value.count - index - 1 : index];
        law(static_cast<int>(index) + 1) = gp_Pnt2d(
            direction == 1 ? 1.0 - station.position : station.position, station.radius);
    }
    builder.SetRadius(law, contour, 1);
}

template <typename Builder>
bool valid_treatment_contours(const Builder&, double) {
    return true;
}

bool valid_treatment_contours(const BRepFilletAPI_MakeFillet& builder, const LinearFilletRadii&) {
    for (int contour = 1; contour <= builder.NbContours(); ++contour) {
        if (builder.Closed(contour)) {
            return false;
        }
    }
    return true;
}

bool valid_treatment_contours(const BRepFilletAPI_MakeFillet& builder, const StationFilletRadii& value) {
    for (int contour = 1; contour <= builder.NbContours(); ++contour) {
        if (builder.Closed(contour) || station_direction(builder, contour, value) < 0) {
            return false;
        }
    }
    return true;
}

bool rejects_duplicate_edges(const StationFilletRadii&) {
    return true;
}

bool rejects_duplicate_edges(double) {
    return false;
}

bool rejects_duplicate_edges(const LinearFilletRadii&) {
    return true;
}

/*
 * Rebuilds each contour of a failed fillet or chamfer on its own and records
 * the selected edges of contours that still fail. Costs one local build per
 * contour, at most MAX_ISOLATED_CONTOURS, and runs only after a failure.
 */
template <typename Builder, typename Value>
void record_isolated_contours(
    occt_bridge_session_t* session,
    const TopoDS_Shape& shape,
    const std::vector<TopoDS_Edge>& selection,
    const std::vector<std::vector<size_t>>& members,
    const Value& value) {
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
            add_treatment(alone, selection[index], value);
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
 * their own, so the error says which selected edges are at fault. Selection
 * validation is O(input topology + selected edges) time and memory; OCCT
 * construction cost depends on local contour geometry.
 */
template <typename Builder, typename Value>
occt_bridge_status_t edge_treatment(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* edges,
    size_t edge_count,
    const Value& value,
    occt_bridge_shape_id_t* out_shape,
    const std::string& name) {
    if (out_shape == nullptr) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
    }
    *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
    if (edges == nullptr || edge_count == 0 || !valid_treatment_value(value)) {
        return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid " + name + " parameters");
    }
    const TopoDS_Shape* input = find_shape(session, shape);
    if (input == nullptr) {
        return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
    }
    const std::lock_guard<std::mutex> treatment_lock(edge_treatment_mutex);
    std::vector<TopoDS_Edge> selection;
    selection.reserve(edge_count);
    TopTools_IndexedMapOfShape descendants;
    TopExp::MapShapes(*input, TopAbs_EDGE, descendants);
    TopTools_MapOfShape seen;
    Builder builder(*input);
    for (size_t index = 0; index < edge_count; ++index) {
        const TopoDS_Shape* edge = find_shape(session, edges[index]);
        if (edge == nullptr) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, name + " edge was not found");
        }
        if (edge->ShapeType() != TopAbs_EDGE || !descendants.Contains(*edge)) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_GEOMETRY,
                name + " selection is not an edge of the input shape");
        }
        if (rejects_duplicate_edges(value) && !seen.Add(*edge)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, name + " selection contains duplicate edges");
        }
        selection.push_back(TopoDS::Edge(*edge));
        add_treatment(builder, selection.back(), value);
    }
    if (!valid_treatment_contours(builder, value)) {
        return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, name + " requires open tangent contours and an unambiguous spine direction");
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

}  // namespace

extern "C" {

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

occt_bridge_status_t occt_bridge_variable_fillet(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* edges,
    size_t edge_count,
    double start_radius,
    double end_radius,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return edge_treatment<BRepFilletAPI_MakeFillet>(
            session, shape, edges, edge_count, LinearFilletRadii{start_radius, end_radius}, out_shape, "variable fillet");
    });
}

occt_bridge_status_t occt_bridge_variable_fillet_stations(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape,
    const occt_bridge_shape_id_t* edges,
    size_t edge_count,
    const occt_bridge_fillet_station_t* stations,
    size_t station_count,
    int32_t spine_direction,
    occt_bridge_vec3_t start_point,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        return edge_treatment<BRepFilletAPI_MakeFillet>(session, shape, edges, edge_count,
            StationFilletRadii{stations, station_count, spine_direction, start_point}, out_shape, "station fillet");
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

}  // extern "C"
