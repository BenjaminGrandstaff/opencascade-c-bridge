/*
 * Modeling operations: booleans, fillets, chamfers, offsets, hollowing,
 * and transforms, with failure diagnostics.
 */

#include "bridge_internal.hpp"

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRep_Tool.hxx>
#include <TColgp_Array1OfPnt2d.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <Precision.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_NoSuchObject.hxx>
#include <TopoDS.hxx>
#include <TopoDS_AlertWithShape.hxx>
#include <TopExp.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_MapOfShape.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include <algorithm>
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

/* Runs a kernel step, returning the OCCT exception's type name or "". */
template <typename Step>
std::string perform_reporting_exception(Step&& step) {
    try {
        step();
    } catch (const Standard_Failure& error) {
        return error.DynamicType()->Name();
    }
    return {};
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
