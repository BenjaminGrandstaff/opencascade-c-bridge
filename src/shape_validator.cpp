#include "shape_validator.hpp"

#include <BRepCheck_Edge.hxx>
#include <BRepCheck_Face.hxx>
#include <BRepCheck_ListOfStatus.hxx>
#include <BRepCheck_Shell.hxx>
#include <BRepCheck_Solid.hxx>
#include <BRepCheck_Vertex.hxx>
#include <BRepCheck_Wire.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <BndLib_Add2dCurve.hxx>
#include <Bnd_Box2d.hxx>
#include <Geom2dAdaptor_Curve.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Curve.hxx>
#include <Precision.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_MapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Wire.hxx>

#include <algorithm>
#include <cmath>
#include <memory>
#include <optional>
#include <vector>

namespace occt_bridge_internal {

namespace {

/*
 * The point BRepCheck_Face's wire classification tests for `wire`: the
 * midpoint of its first edge that is long enough, as OCCT's IsInside picks
 * it. `face` must be forward-oriented, like OCCT's classification faces.
 */
std::optional<gp_Pnt2d> classification_point(const TopoDS_Wire& wire, const TopoDS_Face& face) {
    for (TopExp_Explorer explorer(wire, TopAbs_EDGE); explorer.More(); explorer.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(explorer.Current());
        Standard_Real first = 0.0;
        Standard_Real last = 0.0;
        const Handle(Geom2d_Curve) curve = BRep_Tool::CurveOnSurface(edge, face, first, last);
        Standard_Real parameter = 0.0;
        if (!Precision::IsNegativeInfinite(first) && !Precision::IsPositiveInfinite(last)) {
            parameter = (first + last) * 0.5;
            if (std::abs(parameter - first) < Precision::PConfusion()) {
                continue;
            }
            Standard_Real first3d = 0.0;
            Standard_Real last3d = 0.0;
            const Handle(Geom_Curve) curve3d = BRep_Tool::Curve(edge, first3d, last3d);
            if (curve3d.IsNull()) {
                continue;
            }
            gp_Pnt start;
            gp_Pnt middle;
            curve3d->D0(first, start);
            curve3d->D0((first3d + last3d) / 2.0, middle);
            if (start.Distance(middle) < Precision::Confusion()) {
                continue;
            }
        } else if (Precision::IsNegativeInfinite(first) && Precision::IsPositiveInfinite(last)) {
            parameter = 0.0;
        } else if (Precision::IsNegativeInfinite(first)) {
            parameter = last - 1.0;
        } else {
            parameter = first + 1.0;
        }
        return curve->Value(parameter);
    }
    return std::nullopt;
}

/* The UV box of a wire's pcurves, as BRepCheck_Face::IntersectWires builds it; void if one is missing. */
Bnd_Box2d wire_box(const TopoDS_Wire& wire, const TopoDS_Face& face) {
    Bnd_Box2d box;
    Geom2dAdaptor_Curve adaptor;
    for (TopExp_Explorer explorer(wire, TopAbs_EDGE); explorer.More(); explorer.Next()) {
        Standard_Real first = 0.0;
        Standard_Real last = 0.0;
        const Handle(Geom2d_Curve) curve =
            BRep_Tool::CurveOnSurface(TopoDS::Edge(explorer.Current()), face, first, last);
        if (curve.IsNull()) {
            return {};
        }
        adaptor.Load(curve);
        first = std::max(first, adaptor.FirstParameter());
        last = std::min(last, adaptor.LastParameter());
        BndLib_Add2dCurve::Add(adaptor, first, last, 0.0, box);
    }
    return box;
}

/* A face holding only `wire`, with the face's surface, location, tolerance, and orientation. */
TopoDS_Face proxy_face(const TopoDS_Face& face, const TopoDS_Wire& wire) {
    TopoDS_Face proxy = TopoDS::Face(face.EmptyCopied());
    BRep_Builder().Add(proxy, wire);
    return proxy;
}

struct ClassifiedWire {
    /* The wire alone leaves the infinite point inside the face: it bounds a hole. */
    bool hole = false;
    Bnd_Box2d box;
    std::unique_ptr<BRepTopAdaptor_FClass2d> classifier;
    size_t contains = 0;
};

/* A wire's classification point, sortable by U. */
struct SamplePoint {
    double u = 0.0;
    double v = 0.0;
    size_t wire = 0;
};

/* Classifies each wire of a forward face alone, as ClassifyWires does, and collects sample points. */
std::vector<ClassifiedWire> classify_each(const TopoDS_Face& face, std::vector<SamplePoint>& points) {
    const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
    std::vector<ClassifiedWire> wires;
    for (TopExp_Explorer explorer(forward, TopAbs_WIRE); explorer.More(); explorer.Next()) {
        const TopoDS_Wire& wire = TopoDS::Wire(explorer.Current());
        TopoDS_Face alone = TopoDS::Face(face.EmptyCopied());
        alone.Orientation(TopAbs_FORWARD);
        BRep_Builder().Add(alone, wire);
        ClassifiedWire classified;
        classified.classifier = std::make_unique<BRepTopAdaptor_FClass2d>(alone, Precision::PConfusion());
        classified.hole = classified.classifier->PerformInfinitePoint() != TopAbs_OUT;
        classified.box = wire_box(wire, forward);
        if (const std::optional<gp_Pnt2d> point = classification_point(wire, forward)) {
            points.push_back({point->X(), point->Y(), wires.size()});
        }
        wires.push_back(std::move(classified));
    }
    std::sort(points.begin(), points.end(), [](const SamplePoint& left, const SamplePoint& right) {
        return left.u < right.u;
    });
    return wires;
}

/*
 * Counts, for each wire, the other wires whose sample point it contains,
 * testing only points inside its box: a point outside the box cannot be
 * inside the wire, which bounds a region within its pcurves' hull.
 */
void count_containment(std::vector<ClassifiedWire>& wires, const std::vector<SamplePoint>& points) {
    for (size_t container = 0; container < wires.size(); ++container) {
        ClassifiedWire& outer = wires[container];
        double u_min = -Precision::Infinite();
        double v_min = -Precision::Infinite();
        double u_max = Precision::Infinite();
        double v_max = Precision::Infinite();
        if (!outer.box.IsVoid()) {
            outer.box.Get(u_min, v_min, u_max, v_max);
            const double margin = 1e-7 * (std::hypot(u_max - u_min, v_max - v_min) + 1.0);
            u_min -= margin;
            v_min -= margin;
            u_max += margin;
            v_max += margin;
        }
        const auto first = std::lower_bound(points.begin(), points.end(), u_min, [](const SamplePoint& point, double u) {
            return point.u < u;
        });
        for (auto point = first; point != points.end() && point->u <= u_max; ++point) {
            if (point->wire == container || point->v < v_min || point->v > v_max) {
                continue;
            }
            const TopAbs_State state = outer.classifier->Perform(gp_Pnt2d(point->u, point->v), Standard_False);
            if (outer.hole ? state == TopAbs_OUT : state == TopAbs_IN) {
                ++outer.contains;
            }
        }
    }
}

/*
 * The verdicts of ClassifyWires (one wire contains all others, or none
 * contains any) and OrientationOfWires (the exterior bounds a region and
 * every other wire bounds a hole) for a face with more than one wire. No
 * value when the exterior is reversed, where OCCT's thin-face exception
 * decides.
 */
std::optional<BRepCheck_Status> wire_verdict(const std::vector<ClassifiedWire>& wires, bool infinite) {
    const ClassifiedWire* exterior = nullptr;
    for (const ClassifiedWire& wire : wires) {
        if (wire.contains == 0) {
            continue;
        }
        if (exterior != nullptr) {
            return BRepCheck_InvalidImbricationOfWires;
        }
        exterior = &wire;
    }
    if (exterior != nullptr && exterior->contains != wires.size() - 1) {
        return BRepCheck_InvalidImbricationOfWires;
    }
    if (exterior == nullptr && !infinite) {
        return BRepCheck_InvalidImbricationOfWires;
    }
    if (exterior != nullptr && exterior->hole) {
        return std::nullopt;
    }
    const bool outer_hole = std::any_of(wires.begin(), wires.end(), [exterior](const ClassifiedWire& wire) {
        return &wire != exterior && !wire.hole;
    });
    return outer_hole ? BRepCheck_BadOrientationOfSubshape : BRepCheck_NoError;
}

/*
 * BRepCheck_Face::ClassifyWires followed by OrientationOfWires with the
 * pairwise inclusion test restricted by bounding boxes. Returns no value
 * when OCCT's own check must decide. O(w log w + candidate pairs)
 * classifications for w wires.
 */
std::optional<BRepCheck_Status> classify_wires(const TopoDS_Face& face, const Handle(BRepCheck_Face)& result) {
    const BRepCheck_Status intersection = result->IntersectWires(Standard_False);
    if (intersection != BRepCheck_NoError) {
        return intersection;
    }
    std::vector<SamplePoint> points;
    std::vector<ClassifiedWire> wires = classify_each(face, points);
    if (wires.empty()) {
        return BRepCheck_NoError;
    }
    count_containment(wires, points);
    return wire_verdict(wires, face.Infinite());
}

bool has_error(const BRepCheck_ListOfStatus& statuses) {
    return std::any_of(statuses.begin(), statuses.end(), [](BRepCheck_Status status) {
        return status != BRepCheck_NoError;
    });
}

/* Edge statuses after which OCCT skips wire classification. */
bool lacks_usable_pcurve(const BRepCheck_ListOfStatus& statuses) {
    return std::any_of(statuses.begin(), statuses.end(), [](BRepCheck_Status status) {
        return status == BRepCheck_NoCurveOnSurface || status == BRepCheck_InvalidCurveOnSurface
            || status == BRepCheck_InvalidRange || status == BRepCheck_InvalidCurveOnClosedSurface;
    });
}

opencascade::handle<BRepCheck_Result> make_result(const TopoDS_Shape& shape) {
    switch (shape.ShapeType()) {
        case TopAbs_VERTEX:
            return new BRepCheck_Vertex(TopoDS::Vertex(shape));
        case TopAbs_EDGE: {
            Handle(BRepCheck_Edge) edge = new BRepCheck_Edge(TopoDS::Edge(shape));
            edge->GeometricControls(Standard_True);
            edge->SetExactMethod(Standard_False);
            return edge;
        }
        case TopAbs_WIRE: {
            Handle(BRepCheck_Wire) wire = new BRepCheck_Wire(TopoDS::Wire(shape));
            wire->GeometricControls(Standard_True);
            return wire;
        }
        case TopAbs_FACE: {
            Handle(BRepCheck_Face) face = new BRepCheck_Face(TopoDS::Face(shape));
            face->GeometricControls(Standard_True);
            return face;
        }
        case TopAbs_SHELL:
            return new BRepCheck_Shell(TopoDS::Shell(shape));
        case TopAbs_SOLID:
            return new BRepCheck_Solid(TopoDS::Solid(shape));
        default:
            return {};
    }
}

} // namespace

ShapeValidator::ShapeValidator(const TopoDS_Shape& shape, int many_wires) : shape_(shape) {
    Put(shape);
    Perform(many_wires);
}

/* Creates results in BRepCheck_Analyzer's depth-first order, without recursion. */
void ShapeValidator::Put(const TopoDS_Shape& shape) {
    std::vector<TopoDS_Shape> pending{shape};
    while (!pending.empty()) {
        const TopoDS_Shape current = pending.back();
        pending.pop_back();
        if (results_.Contains(current)) {
            continue;
        }
        Handle(BRepCheck_Result) result = make_result(current);
        if (!result.IsNull()) {
            result->SetParallel(Standard_False);
        }
        results_.Add(current, result);
        std::vector<TopoDS_Shape> children;
        for (TopoDS_Iterator child(current); child.More(); child.Next()) {
            children.push_back(child.Value());
        }
        pending.insert(pending.end(), children.rbegin(), children.rend());
    }
}

void ShapeValidator::Perform(int many_wires) {
    for (int index = 1; index <= results_.Extent(); ++index) {
        const TopoDS_Shape& shape = results_.FindKey(index);
        const Handle(BRepCheck_Result)& result = results_.FindFromIndex(index);
        switch (shape.ShapeType()) {
            case TopAbs_EDGE:
                CheckEdge(shape, result);
                break;
            case TopAbs_FACE:
                CheckFace(TopoDS::Face(shape), result, many_wires);
                break;
            case TopAbs_SOLID:
                CheckSolid(shape, result);
                break;
            default:
                break;
        }
    }
}

/* As BRepCheck_Analyzer: triangulation polygons, then vertices on the edge. */
void ShapeValidator::CheckEdge(const TopoDS_Shape& edge, const ResultHandle& result) {
    try {
        const Handle(BRepCheck_Edge) edge_result = Handle(BRepCheck_Edge)::DownCast(result);
        const BRepCheck_Status status = edge_result->CheckPolygonOnTriangulation(TopoDS::Edge(edge));
        if (status != BRepCheck_NoError) {
            edge_result->SetStatus(status);
        }
    } catch (const Standard_Failure&) {
        result->SetFailStatus(edge);
    }
    TopTools_MapOfShape seen;
    for (TopExp_Explorer explorer(edge, TopAbs_VERTEX); explorer.More(); explorer.Next()) {
        const TopoDS_Shape& vertex = explorer.Current();
        const Handle(BRepCheck_Result)& vertex_result = results_.FindFromKey(vertex);
        try {
            if (seen.Add(vertex)) {
                vertex_result->InContext(edge);
            }
        } catch (const Standard_Failure&) {
            result->SetFailStatus(edge);
            vertex_result->SetFailStatus(vertex);
            vertex_result->SetFailStatus(edge);
        }
    }
}

/*
 * As BRepCheck_Analyzer: vertices, edges, and wires in the face's context,
 * then wire classification and orientation unless an edge lacks a usable
 * pcurve or a wire failed. Many-wire faces check each subshape against its
 * wire's proxy face and classify wires with box pruning.
 */
void ShapeValidator::CheckFace(const TopoDS_Face& face, const ResultHandle& result, int many_wires) {
    int wire_count = 0;
    for (TopoDS_Iterator child(face); child.More(); child.Next()) {
        wire_count += child.Value().ShapeType() == TopAbs_WIRE ? 1 : 0;
    }
    /* OCCT treats single-wire faces specially; they always take its path. */
    const bool scalable = wire_count >= std::max(many_wires, 2);
    if (scalable) {
        BindProxies(face);
    }
    CheckFaceVertices(face, result);
    const bool perform_wires = CheckFaceEdges(face, result);
    const bool orient_wires = CheckFaceWires(face, result, perform_wires);

    const Handle(BRepCheck_Face) face_result = Handle(BRepCheck_Face)::DownCast(result);
    try {
        if (!perform_wires || !orient_wires) {
            face_result->SetUnorientable();
            return;
        }
        const std::optional<BRepCheck_Status> status =
            scalable ? classify_wires(face, face_result) : std::optional<BRepCheck_Status>();
        if (!status) {
            face_result->OrientationOfWires(Standard_True);
        } else if (*status != BRepCheck_NoError) {
            face_result->SetStatus(*status);
        }
    } catch (const Standard_Failure&) {
        result->SetFailStatus(face);
        for (TopExp_Explorer explorer(face, TopAbs_WIRE); explorer.More(); explorer.Next()) {
            const Handle(BRepCheck_Result)& wire_result = results_.FindFromKey(explorer.Current());
            wire_result->SetFailStatus(explorer.Current());
            wire_result->SetFailStatus(face);
            result->SetFailStatus(explorer.Current());
        }
    }
}

/* Records, for each wire, edge, and vertex of a many-wire face, the proxy face of its first wire. */
void ShapeValidator::BindProxies(const TopoDS_Face& face) {
    TopTools_DataMapOfShapeShape& context = *contexts_.Bound(face, TopTools_DataMapOfShapeShape());
    for (TopExp_Explorer wires(face, TopAbs_WIRE); wires.More(); wires.Next()) {
        const TopoDS_Wire& wire = TopoDS::Wire(wires.Current());
        const TopoDS_Face proxy = proxy_face(face, wire);
        context.Bind(wire, proxy);
        for (const TopAbs_ShapeEnum type : {TopAbs_EDGE, TopAbs_VERTEX}) {
            for (TopExp_Explorer subshapes(wire, type); subshapes.More(); subshapes.Next()) {
                if (!context.IsBound(subshapes.Current())) {
                    context.Bind(subshapes.Current(), proxy);
                }
            }
        }
    }
}

void ShapeValidator::CheckFaceVertices(const TopoDS_Face& face, const ResultHandle& result) {
    TopTools_MapOfShape seen;
    for (TopExp_Explorer explorer(face, TopAbs_VERTEX); explorer.More(); explorer.Next()) {
        const Handle(BRepCheck_Result)& vertex_result = results_.FindFromKey(explorer.Current());
        try {
            if (seen.Add(explorer.Current())) {
                vertex_result->InContext(Context(face, explorer.Current()));
            }
        } catch (const Standard_Failure&) {
            result->SetFailStatus(face);
            vertex_result->SetFailStatus(explorer.Current());
            vertex_result->SetFailStatus(face);
        }
    }
}

/* Checks edges in the face's context; false when an edge lacks a usable pcurve. */
bool ShapeValidator::CheckFaceEdges(const TopoDS_Face& face, const ResultHandle& result) {
    bool perform_wires = true;
    TopTools_MapOfShape seen;
    for (TopExp_Explorer explorer(face, TopAbs_EDGE); explorer.More(); explorer.Next()) {
        const Handle(BRepCheck_Result)& edge_result = results_.FindFromKey(explorer.Current());
        try {
            if (!seen.Add(explorer.Current())) {
                continue;
            }
            const TopoDS_Shape context = Context(face, explorer.Current());
            edge_result->InContext(context);
            if (perform_wires && edge_result->IsStatusOnShape(context)
                && lacks_usable_pcurve(edge_result->StatusOnShape(context))) {
                perform_wires = false;
            }
        } catch (const Standard_Failure&) {
            result->SetFailStatus(face);
            edge_result->SetFailStatus(explorer.Current());
            edge_result->SetFailStatus(face);
        }
    }
    return perform_wires;
}

/* Checks wires in the face's context; false when orientation should not be checked. */
bool ShapeValidator::CheckFaceWires(const TopoDS_Face& face, const ResultHandle& result, bool orient_wires) {
    for (TopExp_Explorer explorer(face, TopAbs_WIRE); explorer.More(); explorer.Next()) {
        const Handle(BRepCheck_Result)& wire_result = results_.FindFromKey(explorer.Current());
        try {
            const TopoDS_Shape context = Context(face, explorer.Current());
            wire_result->InContext(context);
            if (orient_wires && wire_result->IsStatusOnShape(context)
                && has_error(wire_result->StatusOnShape(context))) {
                orient_wires = false;
            }
        } catch (const Standard_Failure&) {
            result->SetFailStatus(face);
            wire_result->SetFailStatus(explorer.Current());
            wire_result->SetFailStatus(face);
        }
    }
    return orient_wires;
}

/* As BRepCheck_Analyzer: shells in the solid's context. */
void ShapeValidator::CheckSolid(const TopoDS_Shape& solid, const ResultHandle& result) {
    for (TopExp_Explorer explorer(solid, TopAbs_SHELL); explorer.More(); explorer.Next()) {
        const Handle(BRepCheck_Result)& shell_result = results_.FindFromKey(explorer.Current());
        try {
            shell_result->InContext(solid);
        } catch (const Standard_Failure&) {
            result->SetFailStatus(solid);
            shell_result->SetFailStatus(explorer.Current());
            shell_result->SetFailStatus(solid);
        }
    }
}

TopoDS_Shape ShapeValidator::Context(const TopoDS_Shape& parent, const TopoDS_Shape& subshape) const {
    const TopTools_DataMapOfShapeShape* context = contexts_.Seek(parent);
    const TopoDS_Shape* proxy = context == nullptr ? nullptr : context->Seek(subshape);
    return proxy == nullptr ? parent : *proxy;
}

Handle(BRepCheck_Result) ShapeValidator::Result(const TopoDS_Shape& subshape) const {
    const Handle(BRepCheck_Result)* result = results_.Seek(subshape);
    return result == nullptr ? Handle(BRepCheck_Result)() : *result;
}

/*
 * BRepCheck_Analyzer::IsValid, which requires every subshape reached from
 * the root to pass on its own and within its parent; walked iteratively,
 * visiting each subshape once since its verdict depends only on itself.
 */
bool ShapeValidator::IsValid() const {
    TopTools_MapOfShape visited;
    std::vector<TopoDS_Shape> pending{shape_};
    while (!pending.empty()) {
        const TopoDS_Shape current = pending.back();
        pending.pop_back();
        if (!visited.Add(current)) {
            continue;
        }
        if (!IsValidAlone(current)) {
            return false;
        }
        for (TopoDS_Iterator child(current); child.More(); child.Next()) {
            pending.push_back(child.Value());
        }
    }
    return true;
}

/* One shape's own first status and, by type, its subshapes' statuses in its context. */
bool ShapeValidator::IsValidAlone(const TopoDS_Shape& shape) const {
    const Handle(BRepCheck_Result)& result = results_.FindFromKey(shape);
    if (!result.IsNull() && result->Status().First() != BRepCheck_NoError) {
        return false;
    }
    switch (shape.ShapeType()) {
        case TopAbs_EDGE:
            return ValidSub(shape, TopAbs_VERTEX);
        case TopAbs_FACE:
            return ValidSub(shape, TopAbs_WIRE) && ValidSub(shape, TopAbs_EDGE) && ValidSub(shape, TopAbs_VERTEX);
        case TopAbs_SOLID:
            return ValidSub(shape, TopAbs_SHELL);
        default:
            return true;
    }
}

/*
 * BRepCheck_Analyzer::ValidSub, including its early stop at the first
 * subshape without a status in this context.
 */
bool ShapeValidator::ValidSub(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) const {
    for (TopExp_Explorer explorer(shape, type); explorer.More(); explorer.Next()) {
        const Handle(BRepCheck_Result)& result = results_.FindFromKey(explorer.Current());
        const TopoDS_Shape context = Context(shape, explorer.Current());
        if (!result->IsStatusOnShape(context)) {
            break;
        }
        if (has_error(result->StatusOnShape(context))) {
            return false;
        }
    }
    return true;
}

} // namespace occt_bridge_internal
