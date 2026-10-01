// Regenerates deliberately invalid shapes for result-validation tests.
// Build: g++ -std=c++17 -I/usr/include/opencascade generate_defect_fixtures.cpp \
//   -lTKernel -lTKMath -lTKG3d -lTKBRep -lTKTopAlgo -lTKXSBase -lTKDESTEP \
//   -o generate_defects && ./generate_defects
//
//   bowtie_face.brep / .step  planar face bounded by a self-intersecting
//                             wire; invalid, and shape healing repairs it
//   gapped_face.brep          planar square whose four edges do not share
//                             vertices (1e-5 gaps); invalid, and healing at
//                             default precision cannot repair it
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Geom_Plane.hxx>
#include <STEPControl_Writer.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>

int main() {
    BRepBuilderAPI_MakePolygon bow;
    bow.Add(gp_Pnt(0, 0, 0));
    bow.Add(gp_Pnt(10, 10, 0));
    bow.Add(gp_Pnt(10, 0, 0));
    bow.Add(gp_Pnt(0, 10, 0));
    bow.Close();
    const TopoDS_Face bowtie = BRepBuilderAPI_MakeFace(bow.Wire(), Standard_True).Face();

    BRep_Builder builder;
    TopoDS_Wire wire;
    builder.MakeWire(wire);
    const gp_Pnt corners[] = {gp_Pnt(0, 0, 0), gp_Pnt(10, 0, 0), gp_Pnt(10, 10, 0), gp_Pnt(0, 10, 0)};
    for (int index = 0; index < 4; ++index) {
        const gp_Pnt end = index == 3 ? gp_Pnt(1e-5, 0, 0) : corners[index + 1];
        builder.Add(wire, BRepBuilderAPI_MakeEdge(corners[index], end).Edge());
    }
    TopoDS_Face gapped;
    builder.MakeFace(gapped, new Geom_Plane(gp_Ax3()), 1e-7);
    builder.Add(gapped, wire);

    STEPControl_Writer writer;
    writer.Transfer(bowtie, STEPControl_AsIs);
    const bool written = BRepTools::Write(bowtie, "bowtie_face.brep")
        && BRepTools::Write(gapped, "gapped_face.brep")
        && writer.Write("bowtie_face.step") == IFSelect_RetDone;
    return written ? 0 : 1;
}
