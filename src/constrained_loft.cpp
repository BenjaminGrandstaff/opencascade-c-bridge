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
#include <optional>

using namespace occt_bridge_internal;

namespace {

struct LoftError {
    occt_bridge_status_t status;
    const char* message;
};

struct EndDerivatives {
    const occt_bridge_vec3_t* start;
    const occt_bridge_vec3_t* finish;

    const occt_bridge_vec3_t* at_station(size_t station, size_t count) const {
        if (station == 0) return start;
        return station + 1 == count ? finish : nullptr;
    }

    const occt_bridge_vec3_t* at_span_end(size_t station, int end, size_t count) const {
        if (end == 0) return station == 0 ? start : nullptr;
        return station + 2 == count ? finish : nullptr;
    }
};

bool valid_derivative(const occt_bridge_vec3_t* value) {
    return value == nullptr || (std::isfinite(value->x) && std::isfinite(value->y) && std::isfinite(value->z)
        && std::hypot(value->x, value->y, value->z) > 1e-9);
}

bool valid_request(const occt_bridge_shape_id_t* sections, size_t count, const EndDerivatives& derivatives) {
    return sections != nullptr && count >= 2 && count <= 64
        && (derivatives.start != nullptr || derivatives.finish != nullptr)
        && valid_derivative(derivatives.start) && valid_derivative(derivatives.finish);
}

struct LoftSections {
    std::vector<TopoDS_Wire> wires;
    std::vector<std::vector<TopoDS_Edge>> edges;
    std::vector<TopoDS_Face> caps;
    std::vector<occt_bridge_history_entry> history;
};

const char* copy_section(const TopoDS_Shape& source, TopTools_IndexedMapOfShape& seen,
                         TopoDS_Wire& wire, TopoDS_Face& cap) {
    if (source.ShapeType() != TopAbs_WIRE || !BRep_Tool::IsClosed(source)
        || !ShapeValidator(source).IsValid() || seen.Contains(source)) {
        return "constrained loft requires distinct valid closed planar wires";
    }
    seen.Add(source);
    BRepBuilderAPI_Copy copy(source);
    wire = TopoDS::Wire(copy.Shape());
    BRepBuilderAPI_MakeFace face(wire, Standard_True);
    if (!face.IsDone() || BRepAdaptor_Surface(face.Face()).GetType() != GeomAbs_Plane) {
        return "constrained loft sections must be planar";
    }
    cap = face.Face();
    return nullptr;
}

const char* plane_constraint_error(const TopoDS_Face& cap, const occt_bridge_vec3_t* constraint) {
    if (constraint == nullptr) return nullptr;
    const gp_Vec tangent(constraint->x, constraint->y, constraint->z);
    const gp_Dir normal = BRepAdaptor_Surface(cap).Plane().Axis().Direction();
    if (std::abs(tangent.Dot(gp_Vec(normal))) <= tangent.Magnitude() * 1e-8) {
        return "loft end tangent must cross its profile plane";
    }
    return nullptr;
}

std::vector<TopoDS_Edge> ordered_edges(const TopoDS_Wire& wire) {
    std::vector<TopoDS_Edge> edges;
    for (BRepTools_WireExplorer edge(wire); edge.More(); edge.Next()) {
        edges.push_back(edge.Current());
    }
    return edges;
}

std::optional<LoftError> collect_sections(occt_bridge_session_t* session,
    const occt_bridge_shape_id_t* sections, size_t count, const EndDerivatives& derivatives, LoftSections& result) {
    TopTools_IndexedMapOfShape seen;
    size_t edge_count = 0;
    for (size_t station = 0; station < count; ++station) {
        const auto* source = find_shape(session, sections[station]);
        if (source == nullptr) return LoftError{OCCT_BRIDGE_SHAPE_NOT_FOUND, "loft section not found"};
        TopoDS_Wire wire;
        TopoDS_Face cap;
        if (const char* error = copy_section(*source, seen, wire, cap)) {
            return LoftError{OCCT_BRIDGE_INVALID_GEOMETRY, error};
        }
        result.wires.push_back(wire);
        if (const char* error = plane_constraint_error(cap, derivatives.at_station(station, count))) {
            return LoftError{OCCT_BRIDGE_INVALID_GEOMETRY, error};
        }
        if (station == 0 || station + 1 == count) result.caps.push_back(cap);
        result.edges.push_back(ordered_edges(result.wires.back()));
        if (station == 0) edge_count = result.edges.back().size();
        if (edge_count == 0 || edge_count > 128 || result.edges.back().size() != edge_count) {
            return LoftError{OCCT_BRIDGE_INVALID_GEOMETRY,
                "constrained loft needs matching ordered edge topology (at most 128 edges)"};
        }
        result.history.push_back({*source, {}, {}, false});
    }
    return std::nullopt;
}

const char* profile_edge(const LoftSections& sections, size_t edge, GeomFill_Profiler& profiler) {
    for (const auto& station : sections.edges) {
        Standard_Real first, last;
        TopLoc_Location location;
        auto curve = BRep_Tool::Curve(station[edge], location, first, last);
        if (curve.IsNull()) return "loft section has no curve";
        Handle(Geom_Curve) trimmed = new Geom_TrimmedCurve(curve, first, last);
        trimmed = Handle(Geom_Curve)::DownCast(trimmed->Transformed(location.Transformation()));
        if (station[edge].Orientation() == TopAbs_REVERSED) trimmed->Reverse();
        profiler.AddCurve(trimmed);
    }
    profiler.Perform(1e-10);
    return nullptr;
}

struct SurfaceBasis {
    TColStd_Array1OfReal u_knots;
    TColStd_Array1OfReal v_knots;
    TColStd_Array1OfInteger u_mults;
    TColStd_Array1OfInteger v_mults;

    explicit SurfaceBasis(GeomFill_Profiler& profiler)
        : u_knots(1, profiler.NbKnots()), v_knots(1, 2),
          u_mults(1, profiler.NbKnots()), v_mults(1, 2) {
        profiler.KnotsAndMults(u_knots, u_mults);
        v_knots(1) = 0;
        v_knots(2) = 1;
        v_mults(1) = v_mults(2) = 4;
    }
};

struct ProfilePoles {
    std::vector<std::vector<gp_Pnt>> points;
    std::vector<std::vector<double>> weights;
};

ProfilePoles read_profile(const GeomFill_Profiler& profiler, size_t count) {
    ProfilePoles profile;
    profile.points.resize(count);
    profile.weights.resize(count);
    const int n = profiler.NbPoles();
    for (size_t station = 0; station < count; ++station) {
        TColgp_Array1OfPnt points(1, n);
        TColStd_Array1OfReal weights(1, n);
        profiler.Poles(static_cast<int>(station + 1), points);
        profiler.Weights(static_cast<int>(station + 1), weights);
        for (int i = 1; i <= n; ++i) {
            profile.points[station].push_back(points(i));
            profile.weights[station].push_back(weights(i));
        }
    }
    return profile;
}

gp_Vec derivative(const ProfilePoles& profile, const EndDerivatives& derivatives, size_t station, int pole) {
    const size_t count = profile.points.size();
    if (const auto* tangent = derivatives.at_station(station, count)) {
        return gp_Vec(tangent->x, tangent->y, tangent->z);
    }
    if (station == 0) return gp_Vec(profile.points[0][pole], profile.points[1][pole]);
    if (station + 1 == count) return gp_Vec(profile.points[station - 1][pole], profile.points[station][pole]);
    return gp_Vec(profile.points[station - 1][pole], profile.points[station + 1][pole]) * 0.5;
}

struct HermiteSpan {
    TColgp_Array2OfPnt poles;
    TColStd_Array2OfReal weights;

    HermiteSpan(const ProfilePoles& profile, const EndDerivatives& derivatives, size_t station, int count)
        : poles(1, count, 1, 4), weights(1, count, 1, 4) {
        for (int i = 0; i < count; ++i) {
            poles(i + 1, 1) = profile.points[station][i];
            poles(i + 1, 2) = profile.points[station][i].Translated(derivative(profile, derivatives, station, i) / 3.0);
            poles(i + 1, 3) = profile.points[station + 1][i].Translated(-derivative(profile, derivatives, station + 1, i) / 3.0);
            poles(i + 1, 4) = profile.points[station + 1][i];
            weights(i + 1, 1) = weights(i + 1, 2) = profile.weights[station][i];
            weights(i + 1, 3) = weights(i + 1, 4) = profile.weights[station + 1][i];
        }
    }
};

const char* forward_span_error(const ProfilePoles& profile, size_t station, const TColgp_Array2OfPnt& poles) {
    // Monotone control coordinates exclude longitudinal fold-back, including
    // excessive strengths which would otherwise self-overlap.
    gp_Vec advance;
    for (int i = 0; i < poles.ColLength(); ++i) {
        advance += gp_Vec(profile.points[station][i], profile.points[station + 1][i]);
    }
    if (advance.Magnitude() <= 1e-9) return "constrained loft stations need forward separation";
    advance.Normalize();
    for (int i = 1; i <= poles.ColLength(); ++i) {
        for (int j = 1; j < 4; ++j) {
            if (gp_Vec(poles(i, j), poles(i, j + 1)).Dot(advance) < -1e-9) {
                return "loft tangent strength or correspondence causes longitudinal fold-back";
            }
        }
    }
    return nullptr;
}

const char* end_constraint_error(const Handle(Geom_BSplineSurface)& surface, const SurfaceBasis& basis,
                                 const EndDerivatives& derivatives, size_t station, size_t count) {
    // Independently evaluate the generated surface at each constrained end.
    for (int end = 0; end < 2; ++end) {
        const auto* tangent = derivatives.at_span_end(station, end, count);
        if (tangent == nullptr) continue;
        for (int j = 0; j <= 16; ++j) {
            gp_Pnt point;
            gp_Vec du, dv;
            surface->D1(basis.u_knots(1) + (basis.u_knots(basis.u_knots.Upper()) - basis.u_knots(1)) * j / 16.0,
                static_cast<double>(end), point, du, dv);
            if ((dv - gp_Vec(tangent->x, tangent->y, tangent->z)).Magnitude() > 1e-7 * std::max(1.0, dv.Magnitude())) {
                return "generated loft violates an end derivative constraint";
            }
        }
    }
    return nullptr;
}

std::optional<LoftError> make_span_face(const GeomFill_Profiler& profiler, const SurfaceBasis& basis,
    const ProfilePoles& profile, const EndDerivatives& derivatives, size_t station, TopoDS_Face& result) {
    const HermiteSpan span(profile, derivatives, station, profiler.NbPoles());
    if (const char* error = forward_span_error(profile, station, span.poles)) {
        return LoftError{OCCT_BRIDGE_INVALID_GEOMETRY, error};
    }
    Handle(Geom_BSplineSurface) surface = new Geom_BSplineSurface(span.poles, span.weights,
        basis.u_knots, basis.v_knots, basis.u_mults, basis.v_mults,
        profiler.Degree(), 3, profiler.IsPeriodic(), Standard_False);
    if (const char* error = end_constraint_error(surface, basis, derivatives, station, profile.points.size())) {
        return LoftError{OCCT_BRIDGE_INVALID_GEOMETRY, error};
    }
    BRepBuilderAPI_MakeFace face(surface, 1e-7);
    if (!face.IsDone()) return LoftError{OCCT_BRIDGE_KERNEL_ERROR, "constrained loft face construction failed"};
    result = face.Face();
    return std::nullopt;
}

using GeneratedFaces = std::vector<std::vector<TopoDS_Face>>;

std::optional<LoftError> build_sides(const LoftSections& sections, const EndDerivatives& derivatives,
    BRepBuilderAPI_Sewing& sew, GeneratedFaces& generated) {
    const size_t count = sections.edges.size();
    size_t pole_budget = 0;
    for (size_t edge = 0; edge < generated.size(); ++edge) {
        GeomFill_Profiler profiler;
        if (const char* error = profile_edge(sections, edge, profiler)) {
            return LoftError{OCCT_BRIDGE_INVALID_GEOMETRY, error};
        }
        pole_budget += static_cast<size_t>(profiler.NbPoles()) * count;
        if (pole_budget > 100000) return LoftError{OCCT_BRIDGE_INVALID_ARGUMENT, "constrained loft exceeds 100000 section poles"};
        const SurfaceBasis basis(profiler);
        const ProfilePoles profile = read_profile(profiler, count);
        for (size_t station = 0; station + 1 < count; ++station) {
            TopoDS_Face face;
            if (const auto error = make_span_face(profiler, basis, profile, derivatives, station, face)) return error;
            generated[edge].push_back(face);
            sew.Add(face);
        }
    }
    return std::nullopt;
}

const char* assemble_solid(const LoftSections& sections, BRepBuilderAPI_Sewing& sew, TopoDS_Solid& solid) {
    for (const auto& cap : sections.caps) sew.Add(cap);
    sew.Perform();
    const auto shell = sew.SewedShape();
    if (shell.IsNull() || shell.ShapeType() != TopAbs_SHELL || !BRep_Tool::IsClosed(shell)) {
        return "constrained loft did not close into one shell; check section correspondence";
    }
    BRepBuilderAPI_MakeSolid maker(TopoDS::Shell(shell));
    solid = maker.Solid();
    if (!BRepLib::OrientClosedSolid(solid) || !ShapeValidator(solid).IsValid()) {
        return "constrained loft produced invalid solid geometry";
    }
    return nullptr;
}

TopoDS_Shape final_face(const BRepBuilderAPI_Sewing& sew, const TopoDS_Face& face) {
    return sew.IsModified(face) ? sew.Modified(face) : TopoDS_Shape(face);
}

std::vector<TopoDS_Shape> adjacent_faces(const BRepBuilderAPI_Sewing& sew,
                                        const std::vector<TopoDS_Face>& generated, size_t station, size_t count) {
    std::vector<TopoDS_Shape> faces;
    if (station > 0) faces.push_back(final_face(sew, generated[station - 1]));
    if (station + 1 < count) faces.push_back(final_face(sew, generated[station]));
    return faces;
}

void record_edge_history(occt_bridge_session_t* session, occt_bridge_shape_id_t section,
                         size_t edge, const std::vector<TopoDS_Shape>& faces,
                         std::vector<occt_bridge_history_entry>& history) {
    // Copied edges map back through the copy's immutable wire traversal.
    const auto* source = find_shape(session, section);
    size_t index = 0;
    for (BRepTools_WireExplorer it(TopoDS::Wire(*source)); it.More(); it.Next(), ++index) {
        if (index == edge) {
            history.push_back({it.Current(), faces, {}, false});
            break;
        }
    }
}

void record_history(occt_bridge_session_t* session, const occt_bridge_shape_id_t* source_ids,
    LoftSections& sections, const BRepBuilderAPI_Sewing& sew, const GeneratedFaces& generated) {
    const size_t count = sections.edges.size();
    for (size_t station = 0; station < count; ++station) {
        for (size_t edge = 0; edge < generated.size(); ++edge) {
            const auto faces = adjacent_faces(sew, generated[edge], station, count);
            auto& section_faces = sections.history[station].generated;
            section_faces.insert(section_faces.end(), faces.begin(), faces.end());
            record_edge_history(session, source_ids[station], edge, faces, sections.history);
        }
        if (station == 0) sections.history[station].generated.push_back(final_face(sew, sections.caps.front()));
        if (station + 1 == count) sections.history[station].generated.push_back(final_face(sew, sections.caps.back()));
    }
}

}  // namespace

occt_bridge_status_t occt_bridge_create_constrained_loft(
    occt_bridge_session_t* session, const occt_bridge_shape_id_t* sections,
    size_t count, const occt_bridge_vec3_t* start_tangent,
    const occt_bridge_vec3_t* end_tangent, occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        const EndDerivatives derivatives{start_tangent, end_tangent};
        if (!valid_request(sections, count, derivatives)) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT,
                "constrained loft needs 2-64 sections and finite nonzero end derivatives");
        }
        LoftSections profiles;
        if (const auto error = collect_sections(session, sections, count, derivatives, profiles)) {
            return fail(session, error->status, error->message);
        }
        BRepBuilderAPI_Sewing sew(1e-7);
        GeneratedFaces generated(profiles.edges.front().size());
        if (const auto error = build_sides(profiles, derivatives, sew, generated)) {
            return fail(session, error->status, error->message);
        }
        TopoDS_Solid solid;
        if (const char* error = assemble_solid(profiles, sew, solid)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, error);
        }
        record_history(session, sections, profiles, sew, generated);
        return store_shape_with_entries(session, solid, out_shape, std::move(profiles.history));
    });
}
