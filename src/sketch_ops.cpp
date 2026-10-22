/* Native planar sketch editing. Inputs are immutable; every result is a new
 * session handle. Lookup/join traverse edges; native construction and
 * validity checks depend on the current boundary. */
#include "bridge_internal.hpp"
#include <BRepAdaptor_Curve.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet2d.hxx>
#include <BRepBuilderAPI_FindPlane.hxx>
#include <ChFi2d_AnaFilletAlgo.hxx>
#include <ChFi2d_FilletAlgo.hxx>
#include <Geom_Plane.hxx>
#include <BRepTools.hxx>
#include <TopExp.hxx>
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
#include <gp_Circ.hxx>
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

    gp_Pnt edge_start(const TopoDS_Edge& e) { return BRep_Tool::Pnt(TopExp::FirstVertex(e,true)); }
    gp_Pnt edge_end(const TopoDS_Edge& e) { return BRep_Tool::Pnt(TopExp::LastVertex(e,true)); }
    bool orient_from(TopoDS_Edge& edge, const gp_Pnt& start) {
        if(edge.IsNull()) return false;
        if(edge_start(edge).Distance(start)<=Precision::Confusion()) return true;
        if(edge_end(edge).Distance(start)<=Precision::Confusion()) {edge.Reverse();return true;}
        return false;
    }
    bool point_on_bounded_edge(const TopoDS_Edge& edge,const gp_Pnt& point) {
        double first,last;const auto curve=BRep_Tool::Curve(edge,first,last);
        if(curve.IsNull()) return false;
        double distance=std::min(point.Distance(curve->Value(first)),point.Distance(curve->Value(last)));
        GeomAPI_ProjectPointOnCurve projection(point,curve,first,last);
        if(projection.NbPoints()>0) distance=std::min(distance,projection.LowerDistance());
        return distance<=Precision::Confusion();
    }
    bool same_remote_tangent(const TopoDS_Edge& source,const TopoDS_Edge& edited,bool at_start) {
        BRepAdaptor_Curve a(source),b(edited);
        if(a.GetType()!=b.GetType()) return false;
        if(a.GetType()==GeomAbs_Circle && (std::abs(a.Circle().Radius()-b.Circle().Radius())>Precision::Confusion()
            || a.Circle().Location().Distance(b.Circle().Location())>Precision::Confusion())) return false;
        const auto parameter=[&](const TopoDS_Edge& edge,const BRepAdaptor_Curve& curve) {
            return (at_start!=(edge.Orientation()==TopAbs_REVERSED))?curve.FirstParameter():curve.LastParameter();
        };
        gp_Pnt point;gp_Vec u,v;a.D1(parameter(source,a),point,u);b.D1(parameter(edited,b),point,v);
        if(source.Orientation()==TopAbs_REVERSED)u.Reverse();
        if(edited.Orientation()==TopAbs_REVERSED)v.Reverse();
        return u.Magnitude()>0. && v.Magnitude()>0. && u.Normalized().Dot(v.Normalized())>=1.-1e-9;
    }
    double edge_length(const TopoDS_Edge& edge) {
        BRepAdaptor_Curve curve(edge);
        return GCPnts_AbscissaPoint::Length(curve,curve.FirstParameter(),curve.LastParameter());
    }
    TopoDS_Edge shorten_edge(const TopoDS_Edge& edge,double distance,bool trim_end) {
        double first,last;const auto curve=BRep_Tool::Curve(edge,first,last);
        GeomAdaptor_Curve adaptor(curve,first,last);
        const bool reversed=edge.Orientation()==TopAbs_REVERSED;
        const bool trim_last=trim_end!=reversed;
        const double start=trim_last?last:first;
        GCPnts_AbscissaPoint step(adaptor,trim_last?-distance:distance,start,Precision::Confusion());
        if(!step.IsDone()) return TopoDS_Edge();
        const double cut=step.Parameter();
        if(cut<=first+Precision::PConfusion() || cut>=last-Precision::PConfusion()) return TopoDS_Edge();
        BRepBuilderAPI_MakeEdge maker(curve,trim_last?first:cut,trim_last?cut:last);
        if(!maker.IsDone()) return TopoDS_Edge();
        auto result=maker.Edge();if(reversed)result.Reverse();return result;
    }
    bool line_or_arc(const TopoDS_Edge& edge) {
        const auto type=BRepAdaptor_Curve(edge).GetType();
        return type==GeomAbs_Line || type==GeomAbs_Circle;
    }
    bool general_planar_curve(const TopoDS_Edge& edge) {
        const auto type=BRepAdaptor_Curve(edge).GetType();
        return type==GeomAbs_Line || type==GeomAbs_Circle || type==GeomAbs_Ellipse
            || type==GeomAbs_BSplineCurve || type==GeomAbs_BezierCurve;
    }
    gp_Vec end_tangent(const TopoDS_Edge& edge,bool at_end) {
        BRepAdaptor_Curve curve(edge);gp_Pnt point;gp_Vec tangent;
        const bool reversed=edge.Orientation()==TopAbs_REVERSED;
        curve.D1((at_end!=reversed)?curve.LastParameter():curve.FirstParameter(),point,tangent);
        if(reversed) tangent.Reverse();
        return tangent;
    }
    // Native fillets can propose supporting-curve or wrong-side solutions.
    // Accept only shrinkage within the two bounded source edges.
    bool shrinks_within(const TopoDS_Edge& incoming,const TopoDS_Edge& outgoing,const TopoDS_Edge& a,const TopoDS_Edge& b) {
        return edge_start(a).Distance(edge_start(incoming))<=Precision::Confusion()
            && edge_end(b).Distance(edge_end(outgoing))<=Precision::Confusion()
            && same_remote_tangent(incoming,a,true) && same_remote_tangent(outgoing,b,false)
            && point_on_bounded_edge(incoming,edge_end(a)) && point_on_bounded_edge(outgoing,edge_start(b))
            && edge_length(a)>Precision::Confusion() && edge_length(b)>Precision::Confusion()
            && edge_length(a)<=edge_length(incoming)+Precision::Confusion()
            && edge_length(b)<=edge_length(outgoing)+Precision::Confusion();
    }
    bool tangent_join(const TopoDS_Edge& before,const TopoDS_Edge& after) {
        const auto u=end_tangent(before,true),v=end_tangent(after,false);
        return u.Magnitude()>0. && v.Magnitude()>0. && u.Normalized().Dot(v.Normalized())>=1.-1e-8;
    }
    // Orient a native fillet result along the source traversal: a ends at the
    // fillet start, the fillet runs from a to b, and b keeps the outgoing end.
    bool orient_fillet(const TopoDS_Edge& incoming,const TopoDS_Edge& outgoing,TopoDS_Edge& a,TopoDS_Edge& join,TopoDS_Edge& b) {
        if(a.IsNull() || b.IsNull() || join.IsNull() || !orient_from(a,edge_start(incoming))) return false;
        if(edge_end(b).Distance(edge_end(outgoing))>Precision::Confusion()) b.Reverse();
        return edge_end(b).Distance(edge_end(outgoing))<=Precision::Confusion() && orient_from(join,edge_end(a))
            && edge_end(join).Distance(edge_start(b))<=Precision::Confusion();
    }
    // A corner fillet is a minor arc of the requested radius. Major arcs come
    // from solutions at another shared vertex (closed two-edge profiles).
    bool fillet_arc_is(const TopoDS_Edge& join,double radius) {
        BRepAdaptor_Curve curve(join);
        return curve.GetType()==GeomAbs_Circle
            && std::abs(curve.Circle().Radius()-radius)<=std::max(Precision::Confusion(),1e-9*radius)
            && curve.LastParameter()-curve.FirstParameter()<std::acos(-1.0)-1e-9;
    }
    // Forward copy of an edge, or of its half nearest the corner.
    TopoDS_Edge corner_part(const TopoDS_Edge& edge,bool corner_at_end,bool half) {
        double first,last;const auto curve=BRep_Tool::Curve(edge,first,last);
        if(curve.IsNull()) return TopoDS_Edge();
        if(half) {
            const bool corner_last=corner_at_end!=(edge.Orientation()==TopAbs_REVERSED);
            const double middle=0.5*(first+last);
            (corner_last?first:last)=middle;
        }
        BRepBuilderAPI_MakeEdge maker(curve,first,last);
        return maker.IsDone()?maker.Edge():TopoDS_Edge();
    }
    // The source edge cut at a point on it, keeping its traversal start or end.
    TopoDS_Edge trim_at(const TopoDS_Edge& edge,const gp_Pnt& point,bool keep_start) {
        double first,last;const auto curve=BRep_Tool::Curve(edge,first,last);
        if(curve.IsNull()) return TopoDS_Edge();
        GeomAPI_ProjectPointOnCurve projection(point,curve,first,last);
        if(projection.NbPoints()==0 || projection.LowerDistance()>Precision::Confusion()) return TopoDS_Edge();
        const double cut=projection.LowerDistanceParameter();
        if(cut<=first+Precision::PConfusion() || cut>=last-Precision::PConfusion()) return TopoDS_Edge();
        const bool reversed=edge.Orientation()==TopAbs_REVERSED;
        const bool keep_first=keep_start!=reversed;
        BRepBuilderAPI_MakeEdge maker(curve,keep_first?first:cut,keep_first?cut:last);
        if(!maker.IsDone()) return TopoDS_Edge();
        auto result=maker.Edge();if(reversed)result.Reverse();return result;
    }
    struct CornerEdit {
        TopoDS_Edge before;
        TopoDS_Edge after;
        TopoDS_Edge join;
    };

    const char* chamfer_edges(const TopoDS_Edge& incoming, const TopoDS_Edge& outgoing,
                              double first, double second, CornerEdit& edit) {
        if(first>=edge_length(incoming)-Precision::Confusion() || second>=edge_length(outgoing)-Precision::Confusion())
            return "chamfer distances consume an adjacent edge";
        const auto u=end_tangent(incoming,true),v=end_tangent(outgoing,false);
        if(u.Magnitude()==0. || v.Magnitude()==0. || u.Normalized().Crossed(v.Normalized()).Magnitude()<=1e-9)
            return "chamfer requires a corner with distinct tangent directions";
        edit.before=shorten_edge(incoming,first,true);
        edit.after=shorten_edge(outgoing,second,false);
        if(edit.before.IsNull() || edit.after.IsNull()) return "chamfer could not trim adjacent edges";
        BRepBuilderAPI_MakeEdge bridge(edge_end(edit.before),edge_start(edit.after));
        if(!bridge.IsDone()) return "chamfer could not make a joining edge";
        edit.join=bridge.Edge();
        return shrinks_within(incoming,outgoing,edit.before,edit.after)
            ? nullptr : "corner edit exceeds bounded neighbors";
    }

    const char* analytical_fillet_edges(const TopoDS_Edge& incoming, const TopoDS_Edge& outgoing,
                                        const gp_Pln& plane, double radius, CornerEdit& edit) {
        ChFi2d_AnaFilletAlgo fillet(incoming,outgoing,plane);
        if(!fillet.Perform(radius)) return "fillet radius does not fit the adjacent edges";
        edit.join=fillet.Result(edit.before,edit.after);
        if(!orient_fillet(incoming,outgoing,edit.before,edit.join,edit.after))
            return "fillet did not preserve ordered endpoints";
        return shrinks_within(incoming,outgoing,edit.before,edit.after)
            ? nullptr : "corner edit exceeds bounded neighbors";
    }

    bool accept_corner_fillet(const TopoDS_Edge& incoming, const TopoDS_Edge& outgoing,
                               const TopoDS_Edge& candidate, double radius, CornerEdit& edit) {
        auto join=candidate;
        if(join.IsNull()) return false;
        if(!point_on_bounded_edge(incoming,edge_start(join))) join.Reverse();
        auto before=trim_at(incoming,edge_start(join),true),after=trim_at(outgoing,edge_end(join),false);
        if(!before.IsNull() && !after.IsNull() && shrinks_within(incoming,outgoing,before,after)
            && edge_end(before).Distance(edge_start(join))<=Precision::Confusion()
            && edge_end(join).Distance(edge_start(after))<=Precision::Confusion()
            && fillet_arc_is(join,radius) && tangent_join(before,join) && tangent_join(join,after)) {
            edit.before=before;edit.after=after;edit.join=join;
            return true;
        }
        return false;
    }

    bool try_corner_fillets(const TopoDS_Edge& incoming, const TopoDS_Edge& outgoing,
                             const TopoDS_Edge& e_in, const TopoDS_Edge& e_out,
                             const gp_Pln& plane, double radius, CornerEdit& edit) {
        const auto corner=edge_end(incoming);
        bool found=false;
        // The iterative solver walks its first edge and projects onto its
        // second. Retain native solution order, trying both edge orders.
        for(const bool swapped:{false,true}) {
            ChFi2d_FilletAlgo fillet(swapped?e_out:e_in,swapped?e_in:e_out,plane);
            if(!fillet.Perform(radius)) continue;
            const int solutions=fillet.NbResults(corner);
            for(int i=0;i<solutions && !found;++i) {
                TopoDS_Edge c1,c2;
                const auto join=fillet.Result(corner,c1,c2,i);
                found=accept_corner_fillet(incoming,outgoing,join,radius,edit);
            }
            if(found) break;
        }
        return found;
    }

    const char* iterative_fillet_edges(const TopoDS_Edge& incoming, const TopoDS_Edge& outgoing,
                                       const gp_Pln& plane, double radius, CornerEdit& edit) {
        const auto u=end_tangent(incoming,true),v=end_tangent(outgoing,false);
        if(u.Magnitude()==0. || v.Magnitude()==0. || u.Normalized().Crossed(v.Normalized()).Magnitude()<=1e-9)
            return "fillet requires a corner with distinct tangent directions";
        // When the far ends also meet, isolate the halves near the selected
        // corner. Keep only the solver's arc and trim the original curves.
        const bool loop=edge_start(incoming).Distance(edge_end(outgoing))<=Precision::Confusion();
        const auto e_in=corner_part(incoming,true,loop),e_out=corner_part(outgoing,false,loop);
        if(e_in.IsNull() || e_out.IsNull()) return "fillet could not isolate the corner";
        return try_corner_fillets(incoming,outgoing,e_in,e_out,plane,radius,edit)
            ? nullptr : "fillet radius does not fit the bounded adjacent curves";
    }

    const char* build_corner_wire(const TopoDS_Wire& wire, const TopoDS_Edge& incoming,
                                  const TopoDS_Edge& outgoing, const CornerEdit& edit, bool closed, TopoDS_Wire& result) {
        BRepBuilderAPI_MakeWire builder;size_t source_count=0,result_count=0;
        for(BRepTools_WireExplorer it(wire);it.More();it.Next()) {
            ++source_count;const auto& edge=it.Current();
            if(edge.IsSame(incoming)) {builder.Add(edit.before);builder.Add(edit.join);}
            else if(edge.IsSame(outgoing)) builder.Add(edit.after);
            else builder.Add(edge);
            if(!builder.IsDone()) return "corner edit left disconnected edges";
        }
        if(!builder.IsDone() || BRep_Tool::IsClosed(builder.Wire())!=closed || !BRepCheck_Analyzer(builder.Wire()).IsValid())
            return closed?"corner edit produced an invalid closed wire":"corner edit produced an invalid open wire";
        for(BRepTools_WireExplorer it(builder.Wire());it.More();it.Next()) ++result_count;
        if(result_count!=source_count+1) return "corner edit lost profile edges";
        if(closed) {
            BRepBuilderAPI_MakeFace face(builder.Wire(),true);
            if(!face.IsDone() || !BRepCheck_Analyzer(face.Face()).IsValid()) return "corner edit produced an invalid face";
        }
        result=builder.Wire();
        return nullptr;
    }

    // Line/arc open corners use the analytical constructor. Spline/ellipse
    // neighbors use bounded iterative solutions, then rebuild the same traversal.
    occt_bridge_status_t edit_rebuilt_corner(occt_bridge_session_t* s,const TopoDS_Wire& wire,
        const TopoDS_Edge& incoming,const TopoDS_Edge& outgoing,
        double first,double second,bool chamfer,occt_bridge_shape_id_t* out) {
        const bool closed=BRep_Tool::IsClosed(wire);
        BRepBuilderAPI_FindPlane plane(wire,Precision::Confusion());
        if(!plane.Found() || !BRepCheck_Analyzer(wire).IsValid())
            return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"corner profile must be a valid planar wire");
        CornerEdit edit;
        const char* error=nullptr;
        if(chamfer) error=chamfer_edges(incoming,outgoing,first,second,edit);
        else if(line_or_arc(incoming) && line_or_arc(outgoing))
            error=analytical_fillet_edges(incoming,outgoing,plane.Plane()->Pln(),first,edit);
        else error=iterative_fillet_edges(incoming,outgoing,plane.Plane()->Pln(),first,edit);
        if(error) return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,error);
        TopoDS_Wire result;
        if(const char* build_error=build_corner_wire(wire,incoming,outgoing,edit,closed,result))
            return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,build_error);
        return store_shape(s,result,out);
    }
    const char* corner_neighbors(const TopoDS_Wire& wire, const gp_Pnt& corner,
                                  TopoDS_Vertex& vertex, TopoDS_Edge& incoming, TopoDS_Edge& outgoing) {
        TopTools_IndexedMapOfShape vertices;
        TopExp::MapShapes(wire,TopAbs_VERTEX,vertices);
        for(int i=1;i<=vertices.Extent();++i) {
            const auto v=TopoDS::Vertex(vertices(i));
            if(BRep_Tool::Pnt(v).Distance(corner)<=std::max(Precision::Confusion(),BRep_Tool::Tolerance(v))) {
                if(!vertex.IsNull()) return "corner point matches more than one vertex";
                vertex=v;
            }
        }
        if(vertex.IsNull()) return "corner point is not a remaining profile vertex";
        int incoming_count=0,outgoing_count=0;
        for(BRepTools_WireExplorer it(wire);it.More();it.Next()) {
            const auto& edge=it.Current();
            if(TopExp::LastVertex(edge,true).IsSame(vertex)) {incoming=edge;++incoming_count;}
            if(TopExp::FirstVertex(edge,true).IsSame(vertex)) {outgoing=edge;++outgoing_count;}
        }
        if(incoming_count!=1 || outgoing_count!=1 || incoming.IsNull() || outgoing.IsNull() || incoming.IsSame(outgoing))
            return "corner needs two distinct adjacent edges";
        for(const auto& edge:{incoming,outgoing})
            if(!general_planar_curve(edge))
                return "corner neighbors must be lines, circular arcs, ellipses or splines";
        return nullptr;
    }

    occt_bridge_status_t edit_corner(occt_bridge_session_t* s, occt_bridge_shape_id_t id,
                                     occt_bridge_vec3_t corner, double first, double second,
                                     bool chamfer, occt_bridge_shape_id_t* out) {
        if (!out) return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        *out=OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (!finite(corner) || !std::isfinite(first) || first<=Precision::Confusion()
            || (chamfer && (!std::isfinite(second) || second<=Precision::Confusion())))
            return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "corner edit needs finite positive sizes and a finite point");
        const auto* shape=find_shape(s,id);
        if (!shape) return fail(s, OCCT_BRIDGE_SHAPE_NOT_FOUND, "corner profile was not found");
        if (shape->ShapeType()!=TopAbs_WIRE)
            return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "corner edit requires a planar wire");
        const auto wire=TopoDS::Wire(*shape);
        TopoDS_Vertex vertex;
        TopoDS_Edge incoming,outgoing;
        if(const char* error=corner_neighbors(wire,to_point(corner),vertex,incoming,outgoing))
            return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,error);
        if(!BRep_Tool::IsClosed(wire) || !line_or_arc(incoming) || !line_or_arc(outgoing))
            return edit_rebuilt_corner(s,wire,incoming,outgoing,first,second,chamfer,out);
        BRepBuilderAPI_MakeFace face(wire,true);
        if(!face.IsDone() || !BRepCheck_Analyzer(face.Face()).IsValid())
            return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"corner profile must make a valid planar face");
        BRepFilletAPI_MakeFillet2d edit(face.Face());
        if(chamfer) edit.AddChamfer(incoming,outgoing,first,second);
        else edit.AddFillet(vertex,first);
        if(edit.Status()!=ChFi2d_IsDone)
            return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"corner size does not fit the adjacent edges");
        edit.Build();
        if(!edit.IsDone() || edit.Shape().ShapeType()!=TopAbs_FACE || !BRepCheck_Analyzer(edit.Shape()).IsValid())
            return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"corner edit produced an invalid face");
        const auto result=BRepTools::OuterWire(TopoDS::Face(edit.Shape()));
        if(result.IsNull() || !BRep_Tool::IsClosed(result) || !BRepCheck_Analyzer(result).IsValid())
            return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"corner edit produced an invalid closed wire");
        return store_shape(s,result,out);
    }

}
extern "C" {
    occt_bridge_status_t occt_bridge_fillet_wire_corner(occt_bridge_session_t* s, occt_bridge_shape_id_t id, occt_bridge_vec3_t point, double radius, occt_bridge_shape_id_t* out) {
        return guarded(s,[&] { return edit_corner(s,id,point,radius,0.,false,out); });
    }
    occt_bridge_status_t occt_bridge_chamfer_wire_corner(occt_bridge_session_t* s, occt_bridge_shape_id_t id, occt_bridge_vec3_t point, double incoming, double outgoing, occt_bridge_shape_id_t* out) {
        return guarded(s,[&] { return edit_corner(s,id,point,incoming,outgoing,true,out); });
    }
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
    occt_bridge_status_t occt_bridge_trim_curve_between(occt_bridge_session_t* s, occt_bridge_shape_id_t id, occt_bridge_vec3_t from, occt_bridge_vec3_t to, occt_bridge_shape_id_t* out) {
        return guarded(s,[&] {
            if(out==nullptr) {
                return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"out_shape is null");
            }
            *out=OCCT_BRIDGE_INVALID_SHAPE_ID;
            if(!finite(from)||!finite(to))return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"trim points must be finite");
            const auto* shape=find_shape(s,id);
            if(!shape)return fail(s,OCCT_BRIDGE_SHAPE_NOT_FOUND,"trim curve was not found");
            auto edge=single_edge(*shape);
            if(edge.IsNull())return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"curve editing requires exactly one edge");
            double first,last;
            auto curve=BRep_Tool::Curve(edge,first,last);
            if(curve.IsNull())return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"curve has no geometry");
            // Parameters of both points, each within 1e-6 mm of the curve.
            double u[2];
            const gp_Pnt points[2]={to_point(from),to_point(to)};
            if(points[0].Distance(points[1])<=Precision::Confusion())return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"trim points coincide");
            for(int k=0;k<2;++k) {
                GeomAPI_ProjectPointOnCurve projection(points[k],curve,first,last);
                if(projection.NbPoints()==0||projection.LowerDistance()>1e-6)return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"trim point is not on the curve");
                u[k]=projection.LowerDistanceParameter();
            }
            // Keep from `from` to `to` along the edge's traversal; a closed
            // edge wraps through its seam, an open one needs them in order.
            const bool reversed=edge.Orientation()==TopAbs_REVERSED;
            double a=reversed?u[1]:u[0],b=reversed?u[0]:u[1];
            const bool closed=BRep_Tool::IsClosed(edge)&&curve->IsPeriodic();
            if(closed) {
                const double period=curve->Period();
                while(b<=a+Precision::PConfusion())b+=period;
                while(b-a>period+Precision::PConfusion())b-=period;
            }
            if(b-a<=Precision::PConfusion())return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"trim points are out of order along the curve");
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
            // OCCT 7.9's general medial-axis offset can crash on a full
            // equal-radius ellipse. Circular offsets are exact and need no
            // medial-axis construction; positive distance expands the loop
            // regardless of its traversal (including face hole wires).
            if (closed && !only.IsNull()) {
                BRepAdaptor_Curve analytic(only);
                const bool is_circle = analytic.GetType() == GeomAbs_Circle;
                const bool circular_ellipse = analytic.GetType() == GeomAbs_Ellipse
                    && analytic.Ellipse().MajorRadius() == analytic.Ellipse().MinorRadius();
                if (is_circle || circular_ellipse) {
                    gp_Circ circle = is_circle ? analytic.Circle()
                        : gp_Circ(analytic.Ellipse().Position(), analytic.Ellipse().MajorRadius());
                    const double radius = circle.Radius() + distance;
                    if (radius <= Precision::Confusion()) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "circular offset collapses its boundary");
                    circle.SetRadius(radius);
                    auto edge = BRepBuilderAPI_MakeEdge(circle).Edge();
                    if (only.Orientation() == TopAbs_REVERSED) edge.Reverse();
                    return store_shape(s, BRepBuilderAPI_MakeWire(edge).Wire(), out);
                }
            }
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
    occt_bridge_status_t occt_bridge_curve_closest_fraction(
        occt_bridge_session_t* s, occt_bridge_shape_id_t id, occt_bridge_vec3_t hint,
        double* fraction, occt_bridge_vec3_t* point) {
        return guarded(s, [&] {
            if (fraction == nullptr || point == nullptr) return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "closest parameter outputs are null");
            *fraction = 0.; *point = {0.,0.,0.};
            if (!finite(hint)) return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "closest parameter hint is not finite");
            const auto* shape = find_shape(s, id);
            if (!shape) return fail(s, OCCT_BRIDGE_SHAPE_NOT_FOUND, "parameter curve was not found");
            if (shape->ShapeType() != TopAbs_EDGE && shape->ShapeType() != TopAbs_WIRE) return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "parameter query requires an edge or wire");
            const auto edge = single_edge(*shape);
            if (edge.IsNull()) return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "parameter query requires one edge");
            double first, last;
            const auto curve = BRep_Tool::Curve(edge, first, last);
            if (curve.IsNull() || !std::isfinite(first) || !std::isfinite(last) || !(last > first)) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "parameter curve has no finite active span");
            const gp_Pnt query = to_point(hint);
            const bool reversed = edge.Orientation() == TopAbs_REVERSED;
            const bool closed = curve->Value(first).Distance(curve->Value(last)) <= Precision::Confusion();
            auto normalized = [&](double u) {
                double f = std::clamp((u-first)/(last-first), 0., 1.);
                if (reversed) f = 1.-f;
                if (closed && (f < 1e-10 || f > 1.-1e-10)) f = 0.;
                return f;
            };
            std::vector<std::pair<double,double>> candidates;
            auto append = [&](double u) { candidates.emplace_back(normalized(u), curve->Value(u).Distance(query)); };
            append(first); append(last);
            GeomAPI_ProjectPointOnCurve projection(query, curve, first, last);
            if (projection.NbPoints() > 1000) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "closest parameter exceeds 1000 native extrema");
            if (closed && projection.NbPoints() == 0 && candidates[0].second > Precision::Confusion()) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "closed curve has no unique nearest parameter");
            for (int i=1; i<=projection.NbPoints(); ++i) append(projection.Parameter(i));
            const auto best = std::min_element(candidates.begin(), candidates.end(), [](const auto& a, const auto& b) { return a.second < b.second; });
            if (!std::isfinite(best->second)) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "closest parameter projection failed");
            for (const auto& candidate : candidates) {
                if (std::abs(candidate.second-best->second) <= Precision::Confusion() && std::abs(candidate.first-best->first) > 1e-10) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "closest parameter is ambiguous; pick nearer one branch");
            }
            const double f = best->first;
            const auto p = curve->Value(first+(reversed ? 1.-f : f)*(last-first));
            const occt_bridge_vec3_t position = {p.X(),p.Y(),p.Z()};
            if (!finite(position)) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "closest parameter point is not finite");
            *fraction = f; *point = position;
            return succeed(s);
        });
    }
    occt_bridge_status_t occt_bridge_curve_closest_data(
        occt_bridge_session_t* s, occt_bridge_shape_id_t id, occt_bridge_vec3_t point,
        occt_bridge_vec3_t* closest, occt_bridge_vec3_t* tangent) {
        return guarded(s, [&] {
            if (closest == nullptr || tangent == nullptr) return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "curve contact outputs are null");
            *closest = *tangent = {0.,0.,0.};
            if (!finite(point)) return fail(s, OCCT_BRIDGE_INVALID_ARGUMENT, "curve contact point is not finite");
            const auto* shape = find_shape(s, id);
            if (!shape) return fail(s, OCCT_BRIDGE_SHAPE_NOT_FOUND, "contact curve was not found");
            if (shape->ShapeType() != TopAbs_EDGE && shape->ShapeType() != TopAbs_WIRE) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "contact query requires an edge or wire");
            const auto edge = single_edge(*shape);
            if (edge.IsNull()) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "contact query requires one edge");
            double first, last;
            const auto curve = BRep_Tool::Curve(edge, first, last);
            if (curve.IsNull()) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "contact edge has no curve");
            const gp_Pnt query(point.x, point.y, point.z);
            double distance = std::numeric_limits<double>::infinity(), at = first;
            auto choose = [&](double u) {
                const double d = curve->Value(u).SquareDistance(query);
                if (d < distance) { distance = d; at = u; }
            };
            choose(first); choose(last);
            GeomAPI_ProjectPointOnCurve projection(query, curve, first, last);
            for (int i = 1; i <= projection.NbPoints(); ++i) choose(projection.Parameter(i));
            if (!std::isfinite(distance)) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "curve contact projection failed");
            auto basis = curve;
            while (const auto trimmed = Handle(Geom_TrimmedCurve)::DownCast(basis)) basis = trimmed->BasisCurve();
            const auto spline = Handle(Geom_BSplineCurve)::DownCast(basis);
            if (!spline.IsNull() && !spline->IsCN(1)) {
                const double tolerance = 1e-12 * std::max(1., std::abs(last-first));
                for (int i = spline->FirstUKnotIndex(); i <= spline->LastUKnotIndex(); ++i) {
                    const double knot = spline->Knot(i);
                    if (spline->Multiplicity(i) >= spline->Degree() && std::abs(at-knot) <= tolerance
                        && (spline->IsPeriodic() || (knot > first && knot < last))) {
                        return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "curve contact at a C0 knot has no unique smooth tangent");
                    }
                }
            }
            gp_Pnt p; gp_Vec d;
            curve->D1(at, p, d);
            if (edge.Orientation() == TopAbs_REVERSED) d.Reverse();
            const occt_bridge_vec3_t position = {p.X(),p.Y(),p.Z()}, derivative = {d.X(),d.Y(),d.Z()};
            if (!finite(position) || !finite(derivative) || (derivative.x == 0. && derivative.y == 0. && derivative.z == 0.)) return fail(s, OCCT_BRIDGE_INVALID_GEOMETRY, "curve contact tangent is zero or not finite");
            *closest = position; *tangent = derivative;
            return succeed(s);
        });
    }
    occt_bridge_status_t occt_bridge_curve_curvature_at(occt_bridge_session_t* s,occt_bridge_shape_id_t id,occt_bridge_vec3_t point,occt_bridge_vec3_t* out) {
        return guarded(s,[&] {
            if(out==nullptr||!finite(point))return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"invalid curvature query inputs");
            *out={0.,0.,0.};
            const auto* shape=find_shape(s,id);
            if(!shape)return fail(s,OCCT_BRIDGE_SHAPE_NOT_FOUND,"curvature curve was not found");
            auto edge=single_edge(*shape);
            if(edge.IsNull())return fail(s,OCCT_BRIDGE_INVALID_ARGUMENT,"curvature query requires exactly one edge");
            double first,last;
            auto curve=BRep_Tool::Curve(edge,first,last);
            if(curve.IsNull())return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"curve has no geometry");
            // Nearest parameter, ends included.
            const gp_Pnt query=to_point(point);
            double u=first,best=curve->Value(first).SquareDistance(query);
            if(curve->Value(last).SquareDistance(query)<best){u=last;best=curve->Value(last).SquareDistance(query);}
            GeomAPI_ProjectPointOnCurve projection(query,curve,first,last);
            for(int i=1;i<=projection.NbPoints();++i) {
                const double d=projection.Point(i).SquareDistance(query);
                if(d<best){best=d;u=projection.Parameter(i);}
            }
            // k = (C'' − (C''·T) T) / |C'|², T = C'/|C'|: zero where straight.
            gp_Pnt at;
            gp_Vec d1,d2;
            curve->D2(u,at,d1,d2);
            const double speed2=d1.SquareMagnitude();
            if(!(speed2>Precision::SquareConfusion()))return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"curve tangent is undefined at the query point");
            const gp_Vec k=(d2-d1*(d2.Dot(d1)/speed2))/speed2;
            if(!std::isfinite(k.X())||!std::isfinite(k.Y())||!std::isfinite(k.Z()))return fail(s,OCCT_BRIDGE_INVALID_GEOMETRY,"curvature is not finite");
            *out={k.X(),k.Y(),k.Z()};
            return succeed(s);
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
