// Regenerates curvature_edges.brep, a compound of edges with known curvature.
// Build: g++ -std=c++17 -I/usr/include/opencascade generate_curvature_fixtures.cpp \
//   -lTKernel -lTKMath -lTKG3d -lTKBRep -lTKTopAlgo -o generate && ./generate
//
// Edge order (subshape index):
//   0  quadratic Bezier y = x^2, x in [-1, 1]: curvature 2 at x = 0, 2 / 5^1.5 at the ends
//   1  rational quadratic B-spline quarter circle, radius 2: curvature 0.5 everywhere
//   2  non-rational cubic B-spline with three spans: no closed form
//   3  ellipse a = 4, b = 2 trimmed to t in [0.3, 2.0]: minimum 0.125 at t = pi / 2
//   4  collinear cubic Bezier: curvature 0
//   5  parabola, focal 0.5, u in [-1, 2]: curvature 1 / (1 + u^2)^1.5, maximum 1 at the vertex
//   6  hyperbola a = 3, b = 2, u in [-0.5, 1]: curvature 6 / (9 sinh^2 u + 4 cosh^2 u)^1.5,
//      maximum 0.75 at the vertex
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_Ellipse.hxx>
#include <Geom_Hyperbola.hxx>
#include <Geom_Parabola.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <cmath>

int main() {
    BRep_Builder builder;
    TopoDS_Compound compound;
    builder.MakeCompound(compound);

    TColgp_Array1OfPnt parabola(1, 3);
    parabola(1) = gp_Pnt(-1, 1, 0);
    parabola(2) = gp_Pnt(0, -1, 0);
    parabola(3) = gp_Pnt(1, 1, 0);
    builder.Add(compound, BRepBuilderAPI_MakeEdge(new Geom_BezierCurve(parabola)).Edge());

    TColgp_Array1OfPnt arc(1, 3);
    arc(1) = gp_Pnt(2, 0, 0);
    arc(2) = gp_Pnt(2, 2, 0);
    arc(3) = gp_Pnt(0, 2, 0);
    TColStd_Array1OfReal weights(1, 3);
    weights(1) = 1.0;
    weights(2) = std::sqrt(0.5);
    weights(3) = 1.0;
    TColStd_Array1OfReal arc_knots(1, 2);
    arc_knots(1) = 0.0;
    arc_knots(2) = 1.0;
    TColStd_Array1OfInteger arc_mults(1, 2);
    arc_mults(1) = 3;
    arc_mults(2) = 3;
    builder.Add(
        compound,
        BRepBuilderAPI_MakeEdge(new Geom_BSplineCurve(arc, weights, arc_knots, arc_mults, 2)).Edge());

    TColgp_Array1OfPnt poles(1, 6);
    poles(1) = gp_Pnt(0, 0, 0);
    poles(2) = gp_Pnt(1, 2, 0);
    poles(3) = gp_Pnt(3, 3, 1);
    poles(4) = gp_Pnt(4, 0, 0);
    poles(5) = gp_Pnt(6, -1, -1);
    poles(6) = gp_Pnt(7, 1, 0);
    TColStd_Array1OfReal knots(1, 4);
    TColStd_Array1OfInteger mults(1, 4);
    for (int index = 1; index <= 4; ++index) {
        knots(index) = index - 1;
        mults(index) = (index == 1 || index == 4) ? 4 : 1;
    }
    builder.Add(
        compound,
        BRepBuilderAPI_MakeEdge(new Geom_BSplineCurve(poles, knots, mults, 3)).Edge());

    builder.Add(
        compound,
        BRepBuilderAPI_MakeEdge(new Geom_Ellipse(gp_Ax2(), 4.0, 2.0), 0.3, 2.0).Edge());

    TColgp_Array1OfPnt line(1, 4);
    for (int index = 1; index <= 4; ++index) {
        line(index) = gp_Pnt(index - 1, 0, 0);
    }
    builder.Add(compound, BRepBuilderAPI_MakeEdge(new Geom_BezierCurve(line)).Edge());

    builder.Add(
        compound,
        BRepBuilderAPI_MakeEdge(new Geom_Parabola(gp_Ax2(), 0.5), -1.0, 2.0).Edge());
    builder.Add(
        compound,
        BRepBuilderAPI_MakeEdge(new Geom_Hyperbola(gp_Ax2(), 3.0, 2.0), -0.5, 1.0).Edge());

    return BRepTools::Write(compound, "curvature_edges.brep") ? 0 : 1;
}
