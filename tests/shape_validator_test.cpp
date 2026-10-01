/*
 * Differential test: ShapeValidator must reach BRepCheck_Analyzer's verdict
 * and report the same statuses for every subshape, both with its default
 * many-wire threshold and with the scalable path forced on every face with
 * more than one wire.
 */
#include "../src/shape_validator.hpp"

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_ListOfStatus.hxx>
#include <BRepCheck_Result.hxx>
#include <Standard_Failure.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <iterator>
#include <set>
#include <string>
#include <vector>

using occt_bridge_internal::ShapeValidator;

namespace {

int failures = 0;

/* Every non-error-free status a result holds, on itself or in any context. */
std::set<int> statuses(const Handle(BRepCheck_Result)& result) {
    std::set<int> found;
    if (result.IsNull()) {
        return found;
    }
    std::copy(result->Status().begin(), result->Status().end(), std::inserter(found, found.end()));
    for (result->InitContextIterator(); result->MoreShapeInContext(); result->NextShapeInContext()) {
        const BRepCheck_ListOfStatus& context_statuses = result->StatusOnShape();
        std::copy(context_statuses.begin(), context_statuses.end(), std::inserter(found, found.end()));
    }
    found.erase(BRepCheck_NoError);
    return found;
}

/* The case must fail where intended: some face carries `face_status`. */
void require_face_status(const char* name, const BRepCheck_Analyzer& reference, const TopoDS_Shape& shape, int face_status) {
    TopTools_IndexedMapOfShape faces;
    TopExp::MapShapes(shape, TopAbs_FACE, faces);
    for (int index = 1; index <= faces.Extent(); ++index) {
        if (statuses(reference.Result(faces.FindKey(index))).count(face_status) != 0) {
            return;
        }
    }
    (void)std::fprintf(stderr, "%s: no face has status %d\n", name, face_status);
    ++failures;
}

/* Compares every subshape's statuses between the reference and one validator. */
void compare_statuses(const char* name, int many_wires, const BRepCheck_Analyzer& reference, const ShapeValidator& validator, const TopoDS_Shape& shape) {
    for (const TopAbs_ShapeEnum type : {TopAbs_SOLID, TopAbs_SHELL, TopAbs_FACE, TopAbs_WIRE, TopAbs_EDGE, TopAbs_VERTEX}) {
        TopTools_IndexedMapOfShape subshapes;
        TopExp::MapShapes(shape, type, subshapes);
        for (int index = 1; index <= subshapes.Extent(); ++index) {
            const std::set<int> expected = statuses(reference.Result(subshapes.FindKey(index)));
            const std::set<int> actual = statuses(validator.Result(subshapes.FindKey(index)));
            if (expected == actual) {
                continue;
            }
            std::string detail;
            for (const int status : expected) {
                detail += " e" + std::to_string(status);
            }
            for (const int status : actual) {
                detail += " a" + std::to_string(status);
            }
            (void)std::fprintf(
                stderr, "%s (threshold %d): type %d #%d statuses differ:%s\n", name, many_wires, type, index, detail.c_str());
            ++failures;
        }
    }
}

void compare(const char* name, const TopoDS_Shape& shape, bool expected_valid, int face_status = -1) {
    const BRepCheck_Analyzer reference(shape, Standard_True);
    if (face_status >= 0) {
        require_face_status(name, reference, shape, face_status);
    }
    if (reference.IsValid() != expected_valid) {
        (void)std::fprintf(stderr, "%s: fixture is %svalid, expected otherwise\n", name, reference.IsValid() ? "" : "in");
        ++failures;
    }
    for (const int many_wires : {occt_bridge_internal::MANY_WIRES, 2}) {
        const ShapeValidator validator(shape, many_wires);
        if (validator.IsValid() != reference.IsValid()) {
            (void)std::fprintf(stderr, "%s (threshold %d): verdict differs from BRepCheck_Analyzer\n", name, many_wires);
            ++failures;
        }
        compare_statuses(name, many_wires, reference, validator, shape);
    }
}

TopoDS_Wire circle(double x, double y, double radius) {
    return BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(x, y, 0), gp::DZ()), radius)))
        .Wire();
}

/* A square plate face with circular holes; each hole is reversed unless `forward_hole` names it. */
TopoDS_Face holed_face(const std::vector<gp_Pnt2d>& centers, double radius, int forward_hole = -1) {
    const TopoDS_Wire outer =
        BRepBuilderAPI_MakePolygon(gp_Pnt(0, 0, 0), gp_Pnt(100, 0, 0), gp_Pnt(100, 100, 0), gp_Pnt(0, 100, 0), true)
            .Wire();
    BRepBuilderAPI_MakeFace maker(gp_Pln(), outer);
    for (size_t index = 0; index < centers.size(); ++index) {
        const TopoDS_Wire hole = circle(centers[index].X(), centers[index].Y(), radius);
        maker.Add(static_cast<int>(index) == forward_hole ? hole : TopoDS::Wire(hole.Reversed()));
    }
    return maker.Face();
}

std::vector<gp_Pnt2d> grid(int count) {
    std::vector<gp_Pnt2d> centers;
    centers.reserve(static_cast<size_t>(count));
    for (int index = 0; index < count; ++index) {
        centers.emplace_back(10.0 + 10.0 * (index % 9), 10.0 + 10.0 * std::floor(index / 9.0));
    }
    return centers;
}

TopoDS_Shape drilled_plate(int holes) {
    const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 20.0 * 21, 20.0 * 21, 10).Shape();
    BRep_Builder builder;
    TopoDS_Compound tools;
    builder.MakeCompound(tools);
    for (int index = 0; index < holes; ++index) {
        builder.Add(
            tools,
            BRepPrimAPI_MakeCylinder(
                gp_Ax2(gp_Pnt(10.0 + 20.0 * (index % 20), 10.0 + 20.0 * std::floor(index / 20.0), -1), gp::DZ()), 4, 12)
                .Shape());
    }
    return BRepAlgoAPI_Cut(plate, tools).Shape();
}

void run() {
    compare("drilled plate", drilled_plate(40), true);

    /* Located copies share faces; contexts must stay per location. */
    {
        const TopoDS_Shape plate = drilled_plate(20);
        gp_Trsf shift;
        shift.SetTranslation(gp_Vec(500, 0, 0));
        BRep_Builder builder;
        TopoDS_Compound pair;
        builder.MakeCompound(pair);
        builder.Add(pair, plate);
        builder.Add(pair, plate.Moved(TopLoc_Location(shift)));
        compare("located copies", pair, true);
    }

    /* Holes through a tube wall: many wires on a periodic surface. */
    {
        TopoDS_Shape tube = BRepAlgoAPI_Cut(
            BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp::DZ()), 50, 200).Shape(),
            BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp::DZ()), 45, 202).Shape()).Shape();
        BRep_Builder builder;
        TopoDS_Compound drills;
        builder.MakeCompound(drills);
        for (int index = 0; index < 24; ++index) {
            const double angle = 2.0 * M_PI * (index % 8) / 8.0 + 0.2;
            const gp_Dir direction(std::cos(angle), std::sin(angle), 0);
            builder.Add(
                drills,
                BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 40.0 + 50.0 * std::floor(index / 8.0)), direction), 6, 60).Shape());
        }
        compare("drilled tube", BRepAlgoAPI_Cut(tube, drills).Shape(), true);
    }

    /* A fillet that OCCT builds but whose faces have self-intersecting wires. */
    {
        const TopoDS_Shape box = BRepPrimAPI_MakeBox(10, 10, 10).Shape();
        BRepFilletAPI_MakeFillet fillet(box);
        for (TopExp_Explorer edge(box, TopAbs_EDGE); edge.More(); edge.Next()) {
            fillet.Add(6, TopoDS::Edge(edge.Current()));
        }
        fillet.Build();
        compare("self-intersecting fillet", fillet.Shape(), false);
    }

    /* Hand-built many-wire faces, each breaking one rule. */
    compare("holed face", holed_face(grid(30), 3), true);
    {
        std::vector<gp_Pnt2d> centers = grid(30);
        centers.emplace_back(150, 50);
        compare("hole outside the outer wire", holed_face(centers, 3), false, BRepCheck_InvalidImbricationOfWires);
    }
    compare("hole with outer orientation", holed_face(grid(30), 3, 7), false, BRepCheck_BadOrientationOfSubshape);
    {
        std::vector<gp_Pnt2d> centers = grid(30);
        compare("overlapping holes", holed_face(centers, 6), false, BRepCheck_IntersectingWires);
    }
    {
        /* A small hole inside a large one. */
        std::vector<gp_Pnt2d> centers = grid(20);
        const TopoDS_Face face = holed_face(centers, 2);
        BRepBuilderAPI_MakeFace maker(face);
        maker.Add(TopoDS::Wire(circle(85, 85, 8).Reversed()));
        maker.Add(TopoDS::Wire(circle(85, 85, 2).Reversed()));
        compare("nested holes", maker.Face(), false, BRepCheck_InvalidImbricationOfWires);
    }
    {
        /* Outer wire reversed: OCCT's thin-face exception decides. */
        const TopoDS_Face face = holed_face(grid(20), 3);
        BRep_Builder builder;
        TopoDS_Face reversed = TopoDS::Face(face.EmptyCopied());
        bool first = true;
        for (TopoDS_Iterator wire(face); wire.More(); wire.Next()) {
            builder.Add(reversed, first ? wire.Value().Reversed() : wire.Value());
            first = false;
        }
        compare("reversed outer wire", reversed, false, BRepCheck_BadOrientationOfSubshape);
    }
    {
        /* Holes without an outer wire. */
        BRep_Builder builder;
        TopoDS_Face face = TopoDS::Face(holed_face({}, 1).EmptyCopied());
        for (const gp_Pnt2d& center : grid(20)) {
            builder.Add(face, circle(center.X(), center.Y(), 3).Reversed());
        }
        compare("holes without an outer wire", face, false, BRepCheck_InvalidImbricationOfWires);
    }

    /* Timing at scale, for the record. */
    {
        const TopoDS_Shape plate = drilled_plate(400);
        const auto start = std::chrono::steady_clock::now();
        const bool reference = BRepCheck_Analyzer(plate, Standard_True).IsValid();
        const auto middle = std::chrono::steady_clock::now();
        const bool scalable = ShapeValidator(plate).IsValid();
        const auto end = std::chrono::steady_clock::now();
        (void)std::printf(
            "400-hole plate: BRepCheck_Analyzer %.3f s, ShapeValidator %.3f s\n",
            std::chrono::duration<double>(middle - start).count(),
            std::chrono::duration<double>(end - middle).count());
        if (!reference || !scalable) {
            (void)std::fprintf(stderr, "400-hole plate must be valid\n");
            ++failures;
        }
    }

}

} // namespace

int main() {
    try {
        run();
    } catch (const Standard_Failure& error) {
        (void)std::fprintf(stderr, "OCCT failure: %s\n", error.GetMessageString());
        return EXIT_FAILURE;
    } catch (const std::exception& error) {
        (void)std::fprintf(stderr, "failure: %s\n", error.what());
        return EXIT_FAILURE;
    }
    if (failures != 0) {
        (void)std::fprintf(stderr, "%d shape validator mismatches\n", failures);
        return EXIT_FAILURE;
    }
    (void)std::printf("shape validator matches BRepCheck_Analyzer\n");
    return EXIT_SUCCESS;
}
