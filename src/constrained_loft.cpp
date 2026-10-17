/* Exact rational cubic Hermite lofts. Constraints are derivatives, not hints. */
#include "bridge_internal.hpp"
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepLib.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <GeomFill_Profiler.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <TColgp_Array2OfPnt.hxx>
#include <TColStd_Array2OfReal.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Wire.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Solid.hxx>
#include <cmath>

using namespace occt_bridge_internal;

occt_bridge_status_t occt_bridge_create_constrained_loft(
    occt_bridge_session_t* session, const occt_bridge_shape_id_t* sections,
    size_t count, const occt_bridge_vec3_t* start_tangent,
    const occt_bridge_vec3_t* end_tangent, occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (!out_shape) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        auto finite = [](const occt_bridge_vec3_t* v) {
            return !v || (std::isfinite(v->x) && std::isfinite(v->y) && std::isfinite(v->z)
                && std::hypot(v->x, v->y, v->z) > 1e-9);
        };
        if (!sections || count < 2 || count > 64 || (!start_tangent && !end_tangent)
            || !finite(start_tangent) || !finite(end_tangent))
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "constrained loft needs 2-64 sections and finite nonzero end derivatives");
        std::vector<TopoDS_Wire> wires;
        std::vector<std::vector<TopoDS_Edge>> edges;
        std::vector<TopoDS_Face> caps;
        std::vector<occt_bridge_history_entry> history;
        TopTools_IndexedMapOfShape seen;
        size_t edge_count = 0;
        for (size_t s = 0; s < count; ++s) {
            const auto* source = find_shape(session, sections[s]);
            if (!source) return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "loft section not found");
            if (source->ShapeType() != TopAbs_WIRE || !BRep_Tool::IsClosed(*source)
                || !ShapeValidator(*source).IsValid() || seen.Contains(*source))
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "constrained loft requires distinct valid closed planar wires");
            seen.Add(*source);
            BRepBuilderAPI_Copy copy(*source);
            wires.push_back(TopoDS::Wire(copy.Shape()));
            BRepBuilderAPI_MakeFace cap(wires.back(), Standard_True);
            if (!cap.IsDone() || BRepAdaptor_Surface(cap.Face()).GetType() != GeomAbs_Plane)
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "constrained loft sections must be planar");
            const auto* constraint = s == 0 ? start_tangent : s + 1 == count ? end_tangent : nullptr;
            if (constraint) {
                const gp_Vec tangent(constraint->x, constraint->y, constraint->z);
                const gp_Dir normal = BRepAdaptor_Surface(cap.Face()).Plane().Axis().Direction();
                if (std::abs(tangent.Dot(gp_Vec(normal))) <= tangent.Magnitude()*1e-8)
                    return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "loft end tangent must cross its profile plane");
            }
            if (s == 0 || s + 1 == count) caps.push_back(cap.Face());
            edges.emplace_back();
            for (BRepTools_WireExplorer e(wires.back()); e.More(); e.Next()) edges.back().push_back(e.Current());
            if (s == 0) edge_count = edges.back().size();
            if (edge_count == 0 || edge_count > 128 || edges.back().size() != edge_count)
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "constrained loft needs matching ordered edge topology (at most 128 edges)");
            history.push_back({*source, {}, {}, false});
        }
        BRepBuilderAPI_Sewing sew(1e-7);
        std::vector<std::vector<TopoDS_Face>> generated(edge_count);
        size_t pole_budget = 0;
        for (size_t e = 0; e < edge_count; ++e) {
            GeomFill_Profiler profiler;
            for (size_t s = 0; s < count; ++s) {
                Standard_Real first, last;
                TopLoc_Location location;
                auto curve = BRep_Tool::Curve(edges[s][e], location, first, last);
                if (curve.IsNull()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "loft section has no curve");
                Handle(Geom_Curve) trimmed = new Geom_TrimmedCurve(curve, first, last);
                trimmed = Handle(Geom_Curve)::DownCast(trimmed->Transformed(location.Transformation()));
                if (edges[s][e].Orientation() == TopAbs_REVERSED) trimmed->Reverse();
                profiler.AddCurve(trimmed);
            }
            profiler.Perform(1e-10);
            const int n = profiler.NbPoles(), knots = profiler.NbKnots();
            pole_budget += size_t(n)*count;
            if (pole_budget > 100000) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "constrained loft exceeds 100000 section poles");
            TColStd_Array1OfReal uk(1,knots), vk(1,2);
            TColStd_Array1OfInteger um(1,knots), vm(1,2);
            profiler.KnotsAndMults(uk,um);vk(1)=0;vk(2)=1;vm(1)=4;vm(2)=4;
            std::vector<std::vector<gp_Pnt>> points(count);
            std::vector<std::vector<double>> weights(count);
            for (size_t s=0;s<count;++s) {
                TColgp_Array1OfPnt p(1,n);TColStd_Array1OfReal w(1,n);
                profiler.Poles(int(s+1),p);profiler.Weights(int(s+1),w);
                for(int i=1;i<=n;++i){points[s].push_back(p(i));weights[s].push_back(w(i));}
            }
            auto derivative = [&](size_t s,int i) {
                const auto* t = s == 0 ? start_tangent : s+1 == count ? end_tangent : nullptr;
                if(t) return gp_Vec(t->x,t->y,t->z);
                if(s==0) return gp_Vec(points[0][i],points[1][i]);
                if(s+1==count) return gp_Vec(points[s-1][i],points[s][i]);
                return gp_Vec(points[s-1][i],points[s+1][i])*0.5;
            };
            for(size_t s=0;s+1<count;++s) {
                TColgp_Array2OfPnt poles(1,n,1,4);TColStd_Array2OfReal w(1,n,1,4);
                for(int i=0;i<n;++i) {
                    poles(i+1,1)=points[s][i];poles(i+1,2)=points[s][i].Translated(derivative(s,i)/3.0);
                    poles(i+1,3)=points[s+1][i].Translated(-derivative(s+1,i)/3.0);poles(i+1,4)=points[s+1][i];
                    w(i+1,1)=w(i+1,2)=weights[s][i];w(i+1,3)=w(i+1,4)=weights[s+1][i];
                }
                // Keep each rational Bezier span forward in one common direction.
                // Monotone control coordinates exclude longitudinal fold-back,
                // including excessive strengths which would otherwise self-overlap.
                gp_Vec advance;
                for(int i=0;i<n;++i)advance+=gp_Vec(points[s][i],points[s+1][i]);
                if(advance.Magnitude()<=1e-9)return fail(session,OCCT_BRIDGE_INVALID_GEOMETRY,"constrained loft stations need forward separation");
                advance.Normalize();
                for(int i=1;i<=n;++i)for(int j=1;j<4;++j)
                    if(gp_Vec(poles(i,j),poles(i,j+1)).Dot(advance)<-1e-9)
                        return fail(session,OCCT_BRIDGE_INVALID_GEOMETRY,"loft tangent strength or correspondence causes longitudinal fold-back");
                Handle(Geom_BSplineSurface) surface = new Geom_BSplineSurface(poles,w,uk,vk,um,vm,profiler.Degree(),3,profiler.IsPeriodic(),Standard_False);
                // Independently evaluate the generated surface at each constrained end.
                for(int end=0;end<2;++end) {
                    const auto* t=end==0&&s==0?start_tangent:end==1&&s+2==count?end_tangent:nullptr;
                    if(!t)continue;
                    for(int j=0;j<=16;++j) {
                        gp_Pnt p;gp_Vec du,dv;
                        surface->D1(uk(1)+(uk(knots)-uk(1))*j/16.0,double(end),p,du,dv);
                        if((dv-gp_Vec(t->x,t->y,t->z)).Magnitude()>1e-7*std::max(1.0,dv.Magnitude()))
                            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "generated loft violates an end derivative constraint");
                    }
                }
                BRepBuilderAPI_MakeFace face(surface,1e-7);
                if(!face.IsDone())return fail(session,OCCT_BRIDGE_KERNEL_ERROR,"constrained loft face construction failed");
                generated[e].push_back(face.Face());sew.Add(face.Face());
            }
        }
        for(const auto& cap:caps)sew.Add(cap);
        sew.Perform();
        auto shell=sew.SewedShape();
        if(shell.IsNull()||shell.ShapeType()!=TopAbs_SHELL||!BRep_Tool::IsClosed(shell))
            return fail(session,OCCT_BRIDGE_INVALID_GEOMETRY,"constrained loft did not close into one shell; check section correspondence");
        BRepBuilderAPI_MakeSolid maker(TopoDS::Shell(shell));
        auto solid=maker.Solid();
        if(!BRepLib::OrientClosedSolid(solid)||!ShapeValidator(solid).IsValid())
            return fail(session,OCCT_BRIDGE_INVALID_GEOMETRY,"constrained loft produced invalid solid geometry");
        auto final_face=[&](const TopoDS_Face& face){return sew.IsModified(face)?sew.Modified(face):TopoDS_Shape(face);};
        for(size_t s=0;s<count;++s) {
            for(size_t e=0;e<edge_count;++e) {
                std::vector<TopoDS_Shape> faces;
                if(s>0)faces.push_back(final_face(generated[e][s-1]));
                if(s+1<count)faces.push_back(final_face(generated[e][s]));
                history[s].generated.insert(history[s].generated.end(),faces.begin(),faces.end());
                // Copied edges map back through the copy's immutable wire traversal.
                const auto* source=find_shape(session,sections[s]);size_t k=0;
                for(BRepTools_WireExplorer it(TopoDS::Wire(*source));it.More();it.Next(),++k)
                    if(k==e){history.push_back({it.Current(),faces,{},false});break;}
            }
            if(s==0)history[s].generated.push_back(final_face(caps.front()));
            if(s+1==count)history[s].generated.push_back(final_face(caps.back()));
        }
        return store_shape_with_entries(session,solid,out_shape,std::move(history));
    });
}
