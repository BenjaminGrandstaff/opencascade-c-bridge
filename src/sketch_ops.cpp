/* Native planar sketch editing. Inputs are immutable; every result is a new
 * session handle. Work is linear in wire edges except native offset/projection. */
#include "bridge_internal.hpp"
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepFill_OffsetWire.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <GeomConvert.hxx>
#include <GeomLib.hxx>
#include <Geom_BoundedCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <Geom_OffsetCurve.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Elips.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pln.hxx>
#include <algorithm>
#include <cmath>
#include <limits>
using namespace occt_bridge_internal;
namespace {
    TopoDS_Edge single_edge(const TopoDS_Shape& shape) {
        TopoDS_Edge edge;
        size_t count=0;
        for(TopExp_Explorer it(shape,TopAbs_EDGE);it.More();it.Next()) {
            edge=TopoDS::Edge(it.Current());
            ++count;
        }
        if(count!=1) return TopoDS_Edge();
        return edge;
    }
    occt_bridge_status_t store_curve(occt_bridge_session_t* session, const Handle(Geom_Curve)& curve, double first, double last, bool reversed, occt_bridge_shape_id_t* out) {
        BRepBuilderAPI_MakeEdge maker(curve,first,last);
        if(!maker.IsDone())return fail(session,OCCT_BRIDGE_INVALID_GEOMETRY,"curve edit did not produce an edge");
        auto edge=maker.Edge();
        if(reversed)edge.Reverse();
        BRepBuilderAPI_MakeWire wire(edge);
        return store_shape(session,wire.Wire(),out);
    }
}
extern "C" {
    occt_bridge_status_t occt_bridge_create_ellipse_wire_axes(occt_bridge_session_t* s, occt_bridge_vec3_t center, occt_bridge_vec3_t normal, occt_bridge_vec3_t major_axis, double major, double minor, occt_bridge_shape_id_t* out) {
        return guarded(s,[&] {
            if(out==nullptr) {
                return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"out_shape is null");
            }
            *out=OCCT_BRIDGE_INVALID_SHAPE_ID;
            if(!finite(center)||!finite(normal)||!finite(major_axis)||!std::isfinite(major)||!std::isfinite(minor)||minor<=Precision::Confusion()||major<minor)return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"invalid ellipse axes/radii");
            gp_Vec n(normal.x,normal.y,normal.z),a(major_axis.x,major_axis.y,major_axis.z);
            if(n.Magnitude()<=Precision::Confusion()||a.Magnitude()<=Precision::Confusion()||std::abs(n.Normalized().Dot(a.Normalized()))>1e-9)return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"ellipse axes must be nonzero and perpendicular");
            gp_Elips ellipse(gp_Ax2(to_point(center),gp_Dir(n),gp_Dir(a)),major,minor);
            BRepBuilderAPI_MakeEdge edge(ellipse);
            BRepBuilderAPI_MakeWire wire(edge.Edge());
            return store_shape(s,wire.Wire(),out);
        });
    }
    occt_bridge_status_t occt_bridge_trim_curve(occt_bridge_session_t* s, occt_bridge_shape_id_t id, double from, double to, occt_bridge_shape_id_t* out) {
        return guarded(s,[&] {
            if(out==nullptr) {
                return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"out_shape is null");
            }
            *out=OCCT_BRIDGE_INVALID_SHAPE_ID;
            if(!std::isfinite(from)||!std::isfinite(to)||from<0||to>1||from>=to)return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"invalid trim fractions");
            const auto* shape=find_shape(s,id);
            if(!shape)return fail(s,OCCT_BRIDGE_SHAPE_NOT_FOUND,"trim curve was not found");
            auto edge=single_edge(*shape);
            if(edge.IsNull())return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"curve editing requires exactly one edge");
            double first,last;
            auto curve=BRep_Tool::Curve(edge,first,last);
            if(curve.IsNull())return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"curve has no geometry");
            const bool reversed=edge.Orientation()==TopAbs_REVERSED;
            const double span=last-first;
            const double a=reversed?last-to*span:first+from*span,b=reversed?last-from*span:first+to*span;
            if(b-a<=Precision::PConfusion())return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"trim interval is too small");
            return store_curve(s,curve,a,b,reversed,out);
        });
    }
    occt_bridge_status_t occt_bridge_extend_curve(occt_bridge_session_t* s, occt_bridge_shape_id_t id, double start, double end, occt_bridge_shape_id_t* out) {
        return guarded(s,[&] {
            if(out==nullptr) {
                return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"out_shape is null");
            }
            *out=OCCT_BRIDGE_INVALID_SHAPE_ID;
            if(!std::isfinite(start)||!std::isfinite(end)||start<0||end<0||start+end<=0)return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"invalid extension distances");
            const auto* shape=find_shape(s,id);
            if(!shape)return fail(s,OCCT_BRIDGE_SHAPE_NOT_FOUND,"extension curve was not found");
            auto edge=single_edge(*shape);
            if(edge.IsNull())return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"curve editing requires exactly one edge");
            double first,last;
            auto original=BRep_Tool::Curve(edge,first,last);
            if(original.IsNull())return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"curve has no geometry");
            if(BRep_Tool::IsClosed(*shape))return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"trim a closed curve before extending it");
            const bool reversed=edge.Orientation()==TopAbs_REVERSED;
            if(reversed)std::swap(start,end);
            Handle(Geom_Curve) curve=Handle(Geom_Curve)::DownCast(original->Copy());
            GeomAdaptor_Curve adaptor(curve);
            if(adaptor.GetType()==GeomAbs_Line||adaptor.GetType()==GeomAbs_Circle||adaptor.GetType()==GeomAbs_Ellipse) {
                if(start>0) {
                    GCPnts_AbscissaPoint a(Precision::Confusion(),adaptor,-start,first);
                    if(!a.IsDone())return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"start extension failed");
                    first=a.Parameter();
                }
                if(end>0) {
                    GCPnts_AbscissaPoint a(Precision::Confusion(),adaptor,end,last);
                    if(!a.IsDone())return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"end extension failed");
                    last=a.Parameter();
                }
                if(curve->IsPeriodic()&&last-first>curve->Period()+Precision::PConfusion())return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"extension overlaps a full periodic curve");
            } else {
                gp_Pnt a,b;
                gp_Vec da,db;
                curve->D1(first,a,da);
                curve->D1(last,b,db);
                if(da.Magnitude()<=Precision::Confusion()||db.Magnitude()<=Precision::Confusion())return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"extension tangent is degenerate");
                const gp_Pnt target_a=a.Translated(-da.Normalized()*start),target_b=b.Translated(db.Normalized()*end);
                Handle(Geom_BoundedCurve) bounded=GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve,first,last));
                if(start>0)GeomLib::ExtendCurveToPoint(bounded,target_a,1,Standard_False);
                if(end>0)GeomLib::ExtendCurveToPoint(bounded,target_b,1,Standard_True);
                curve=bounded;
                first=curve->FirstParameter();
                last=curve->LastParameter();
            }
            return store_curve(s,curve,first,last,reversed,out);
        });
    }
    occt_bridge_status_t occt_bridge_join_wires(occt_bridge_session_t* s,const occt_bridge_shape_id_t* ids,size_t count,int closed,occt_bridge_shape_id_t* out) {
        return guarded(s,[&] {
            if(out==nullptr) {
                return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"out_shape is null");
            }
            *out=OCCT_BRIDGE_INVALID_SHAPE_ID;
            if(ids==nullptr||count==0||count>100000||(closed!=0&&closed!=1))return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"invalid wire join inputs");
            BRepBuilderAPI_MakeWire builder;
            for(size_t i=0;i<count;++i) {
                const auto* shape=find_shape(s,ids[i]);
                if(!shape)return fail(s,OCCT_BRIDGE_SHAPE_NOT_FOUND,"wire join input was not found");
                if(shape->ShapeType()!=TopAbs_WIRE)return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"wire join needs wires");
                for(BRepTools_WireExplorer edges(TopoDS::Wire(*shape));edges.More();edges.Next()) {
                    builder.Add(edges.Current());
                    if(!builder.IsDone())return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"profile edits leave disconnected edges");
                }
            }
            if(!builder.IsDone())return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"empty joined profile");
            if(closed==1&&!BRep_Tool::IsClosed(builder.Wire()))return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"edited profile is not closed");
            return store_shape(s,builder.Wire(),out);
        });
    }
    occt_bridge_status_t occt_bridge_offset_wire( occt_bridge_session_t* s, occt_bridge_shape_id_t id, occt_bridge_vec3_t normal, double distance, int join, occt_bridge_shape_id_t* out) {
        return guarded(s, [&] {
            if (out == nullptr) {
                return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
            }
            *out = OCCT_BRIDGE_INVALID_SHAPE_ID;
            if (!finite(normal) || !std::isfinite(distance) || std::abs(distance) <= Precision::Confusion() || (join != 0 && join != 1)) {
                return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid planar offset inputs");
            }
            const auto* shape = find_shape(s, id);
            if (shape == nullptr) {
                return fail(s, OCCT_BRIDGE_SHAPE_NOT_FOUND, "offset wire was not found");
            }
            if (shape->ShapeType() != TopAbs_WIRE) {
                return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "offset requires a wire");
            }
            gp_Vec n(normal.x, normal.y, normal.z);
            if (n.Magnitude() <= Precision::Confusion()) {
                return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "offset normal is zero");
            }
            BRepTools_WireExplorer first_edge(TopoDS::Wire(*shape));
            if (!first_edge.More()) {
                return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "offset wire is empty");
            }
            double first, last;
            auto curve = BRep_Tool::Curve(first_edge.Current(), first, last);
            if (curve.IsNull()) {
                return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "offset curve has no geometry");
            }
            const gp_Pln plane(curve->Value(first), gp_Dir(n));
            for (TopExp_Explorer edges(*shape, TopAbs_EDGE); edges.More(); edges.Next()) {
                double a, b;
                auto c = BRep_Tool::Curve(TopoDS::Edge(edges.Current()), a, b);
                if (c.IsNull()) {
                    return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "offset edge has no curve");
                }
                for (int i = 0; i <= 8; ++i) {
                    if (plane.Distance(c->Value(a + (b-a) * i / 8.0)) > Precision::Confusion()) {
                        return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "offset wire is not in its plane");
                    }
                }
            }
            const bool closed = BRep_Tool::IsClosed(*shape);
            const auto only = single_edge(*shape);
            if (!closed && !only.IsNull()) {
                double a, b;
                auto source = BRep_Tool::Curve(only, a, b);
                const bool reversed = only.Orientation() == TopAbs_REVERSED;
                Handle(Geom_Curve) parallel = new Geom_OffsetCurve( source, reversed ? -distance : distance, gp_Dir(n));
                return store_curve(s, parallel, a, b, reversed, out);
            }
            const auto join_type = join == 0 ? GeomAbs_Arc : GeomAbs_Intersection;
            TopoDS_Shape edited;
            if (closed) {
                BRepOffsetAPI_MakeOffset offset(TopoDS::Wire(*shape), join_type, Standard_False);
                offset.Perform(distance);
                if (!offset.IsDone()) {
                    return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "closed profile offset failed");
                }
                edited = offset.Shape();
            } else {
                auto spine = TopoDS::Wire(*shape);
                // The planar face's wire is a left-region boundary. Reverse it
                // for a right-side offset, and restore output traversal below.
                if (distance > 0.0) {
                    spine.Reverse();
                }
                BRepBuilderAPI_MakeFace support(plane, spine, Standard_False);
                if (!support.IsDone()) {
                    return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "offset support face failed");
                }
                BRepFill_OffsetWire offset(support.Face(), join_type, Standard_True);
                offset.Perform(std::abs(distance));
                if (!offset.IsDone()) {
                    return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "open profile offset failed");
                }
                edited = offset.Shape();
            }
            TopoDS_Wire result;
            size_t wires = 0;
            if (edited.ShapeType() == TopAbs_WIRE) {
                result = TopoDS::Wire(edited);
                wires = 1;
            } else {
                for (TopExp_Explorer it(edited, TopAbs_WIRE); it.More(); it.Next()) {
                    result = TopoDS::Wire(it.Current());
                    ++wires;
                }
            }
            if (wires != 1) {
                return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "offset must produce exactly one connected profile");
            }
            if (!closed && distance > 0.0) {
                result.Reverse();
            }
            return store_shape(s, result, out);
        });
    }
    occt_bridge_status_t occt_bridge_curve_closest_point(occt_bridge_session_t* s,occt_bridge_shape_id_t id,occt_bridge_vec3_t point,occt_bridge_vec3_t* out) {
        return guarded(s,[&] {
            if(out==nullptr||!finite(point))return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"invalid curve projection inputs");
            const auto* shape=find_shape(s,id);
            if(!shape)return fail(s,OCCT_BRIDGE_SHAPE_NOT_FOUND,"projection curve was not found");
            const gp_Pnt query(point.x,point.y,point.z);
            gp_Pnt best;
            double distance=std::numeric_limits<double>::infinity();
            for(TopExp_Explorer edges(*shape,TopAbs_EDGE);edges.More();edges.Next()) {
                double first,last;
                auto curve=BRep_Tool::Curve(TopoDS::Edge(edges.Current()),first,last);
                if(curve.IsNull())continue;
                auto choose=[&](const gp_Pnt& p) {
                    const double d=p.SquareDistance(query);
                    if(d<distance) {
                        distance=d;
                        best=p;
                    }
                };
                choose(curve->Value(first));
                choose(curve->Value(last));
                GeomAPI_ProjectPointOnCurve projection(query,curve,first,last);
                for(int i=1;i<=projection.NbPoints();++i)choose(projection.Point(i));
            }
            if(!std::isfinite(distance))return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"curve projection found no finite point");
            *out={best.X(),best.Y(),best.Z()};
            return succeed(s);
        });
    }
    occt_bridge_status_t occt_bridge_wire_is_closed(occt_bridge_session_t* s,occt_bridge_shape_id_t id,int* out) {
        return guarded(s,[&] {
            if(out==nullptr)return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"out_closed is null");
            const auto* shape=find_shape(s,id);
            if(!shape)return fail(s,OCCT_BRIDGE_SHAPE_NOT_FOUND,"wire was not found");
            if(shape->ShapeType()!=TopAbs_WIRE)return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"closure query needs a wire");
            *out=BRep_Tool::IsClosed(*shape)?1:0;
            return succeed(s);
        });
    }
}
