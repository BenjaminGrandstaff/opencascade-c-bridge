/* Explicit topological orientation/location round trip through the C ABI. */
#include "bridge_internal.hpp"
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <TopoDS_Edge.hxx>
#include <gp_Trsf.hxx>
#include <cmath>
#include <cstdio>
int main() {
    occt_bridge_session_t* session=nullptr;
    if (occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION,&session)!=OCCT_BRIDGE_OK) return 1;
    auto edge=BRepBuilderAPI_MakeEdge(gp_Pnt(0,0,0),gp_Pnt(10,0,0)).Edge();
    edge.Reverse();
    gp_Trsf shift; shift.SetTranslation(gp_Vec(3,4,5));
    edge.Location(TopLoc_Location(shift));
    const auto id=session->next_shape_id++;
    session->shapes.emplace(id,edge);
    bool passed=true;
    double fraction=0; occt_bridge_vec3_t point{};
    passed=passed && occt_bridge_curve_closest_fraction(session,id,{11,4,5},&fraction,&point)==OCCT_BRIDGE_OK;
    passed=passed && std::abs(fraction-0.2)<1e-10 && std::abs(point.x-11)<1e-10 && std::abs(point.z-5)<1e-10;
    occt_bridge_vec3_t evaluated{};
    passed=passed && occt_bridge_curve_point_at_fraction(session,id,fraction,&evaluated)==OCCT_BRIDGE_OK;
    passed=passed && std::abs(evaluated.x-point.x)<1e-10 && std::abs(evaluated.y-point.y)<1e-10;
    passed=passed && occt_bridge_curve_closest_fraction(session,id,{20,4,5},&fraction,&point)==OCCT_BRIDGE_OK && fraction==0.;
    passed=passed && occt_bridge_curve_closest_fraction(session,id,{-20,4,5},&fraction,&point)==OCCT_BRIDGE_OK && fraction==1.;
    occt_bridge_session_destroy(session);
    if (!passed) (void)std::fprintf(stderr,"oriented curve parameter round trip failed\n");
    return passed ? 0 : 1;
}
