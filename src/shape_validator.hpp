#ifndef OCCT_BRIDGE_SHAPE_VALIDATOR_HPP
#define OCCT_BRIDGE_SHAPE_VALIDATOR_HPP

#include <BRepCheck_IndexedDataMapOfShapeResult.hxx>
#include <BRepCheck_Result.hxx>
#include <NCollection_DataMap.hxx>
#include <TopTools_DataMapOfShapeShape.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>

namespace occt_bridge_internal {

/*
 * Faces with at least this many wires take the scalable path; smaller faces
 * are checked exactly as BRepCheck_Analyzer checks them.
 */
constexpr int MANY_WIRES = 16;

/*
 * Validates a shape with OCCT's BRepCheck checks and BRepCheck_Analyzer's
 * verdict, at a cost that scales with faces that have many wires.
 *
 * BRepCheck_Analyzer checks each vertex, edge, and wire of a face in the
 * context of the whole face, and each of those checks rescans or re-derives
 * face-wide data, so a face with w holes costs O(w^2); classifying its
 * wires compares every pair, another O(w^2). For faces with at least
 * `many_wires` wires this validator runs the same per-subshape checks
 * against a proxy face holding only the subshape's own wire (the checks
 * depend only on that wire and the face's surface, location, and
 * tolerance), and classifies wires with OCCT's own point test applied only
 * to pairs whose sample point lies in the candidate's bounding box. Other
 * faces, and all other checks, run exactly as in BRepCheck_Analyzer.
 * Wire intersection still uses BRepCheck_Face::IntersectWires, which
 * prefilters pairs by bounding box.
 */
class ShapeValidator {
public:
    explicit ShapeValidator(const TopoDS_Shape& shape, int many_wires = MANY_WIRES);

    /* BRepCheck_Analyzer::IsValid for the whole shape. */
    bool IsValid() const;

    /* The check result for a subshape, or a null handle. Statuses in the
       context of a many-wire face may be recorded against a proxy face. */
    opencascade::handle<BRepCheck_Result> Result(const TopoDS_Shape& subshape) const;

private:
    using ContextMap = NCollection_DataMap<TopoDS_Shape, TopTools_DataMapOfShapeShape, TopTools_ShapeMapHasher>;

    using ResultHandle = opencascade::handle<BRepCheck_Result>;

    void Put(const TopoDS_Shape& shape);
    void Perform(int many_wires);
    void CheckEdge(const TopoDS_Shape& edge, const ResultHandle& result);
    void CheckFace(const TopoDS_Face& face, const ResultHandle& result, int many_wires);
    void BindProxies(const TopoDS_Face& face);
    void CheckFaceVertices(const TopoDS_Face& face, const ResultHandle& result);
    bool CheckFaceEdges(const TopoDS_Face& face, const ResultHandle& result);
    bool CheckFaceWires(const TopoDS_Face& face, const ResultHandle& result, bool orient_wires);
    void CheckSolid(const TopoDS_Shape& solid, const ResultHandle& result);
    TopoDS_Shape Context(const TopoDS_Shape& parent, const TopoDS_Shape& subshape) const;
    bool IsValidAlone(const TopoDS_Shape& shape) const;
    bool ValidSub(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) const;

    TopoDS_Shape shape_;
    BRepCheck_IndexedDataMapOfShapeResult results_;
    /* Per many-wire face: the proxy face each subshape was checked against. */
    ContextMap contexts_;
};

} // namespace occt_bridge_internal

#endif
