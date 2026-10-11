/*
 * Planar text outlines: fontconfig resolves an installed family strictly,
 * FreeType supplies unscaled glyph outlines, and each contour becomes exact
 * native lines and quadratic or cubic Bezier edges. Contours are classified
 * by nesting depth, not winding, so TrueType and CFF fonts agree.
 */

#include "bridge_internal.hpp"

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRep_Builder.hxx>
#include <Geom_BezierCurve.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pln.hxx>

#include <fontconfig/fontconfig.h>
#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_OUTLINE_H

#include <algorithm>
#include <cmath>
#include <cstring>
#include <memory>
#include <string>
#include <vector>

using namespace occt_bridge_internal;

namespace {

constexpr size_t kMaxCharacters = 1000;

/* Decodes UTF-8, rejecting malformed input and control characters. */
bool decode_utf8(const char* text, size_t length, std::vector<char32_t>& out) {
    for (size_t i = 0; i < length;) {
        const auto byte = static_cast<unsigned char>(text[i]);
        char32_t code = 0;
        size_t extra = 0;
        if (byte < 0x80) {
            code = byte;
        } else if ((byte & 0xE0) == 0xC0) {
            code = byte & 0x1F;
            extra = 1;
        } else if ((byte & 0xF0) == 0xE0) {
            code = byte & 0x0F;
            extra = 2;
        } else if ((byte & 0xF8) == 0xF0) {
            code = byte & 0x07;
            extra = 3;
        } else {
            return false;
        }
        for (size_t k = 1; k <= extra; ++k) {
            if (i + k >= length) return false;
            const auto next = static_cast<unsigned char>(text[i + k]);
            if ((next & 0xC0) != 0x80) return false;
            code = (code << 6) | (next & 0x3F);
        }
        // UTF-8 must use the shortest encoding and represent a Unicode
        // scalar value; surrogate code points are not valid UTF-8.
        if ((extra == 1 && code < 0x80) || (extra == 2 && code < 0x800)
            || (extra == 3 && code < 0x10000) || (code >= 0xD800 && code <= 0xDFFF)
            || code < 0x20 || (code >= 0x7F && code <= 0x9F) || code > 0x10FFFF) return false;
        out.push_back(code);
        i += extra + 1;
    }
    return true;
}

struct Contour {
    std::vector<TopoDS_Edge> edges;
    std::vector<gp_XY> samples;  // local 2D polygon for area and nesting
    double area = 0.0;           // absolute
    bool counterclockwise = true;
    int parent = -1;
    int depth = 0;
};

/* Builds contours of one glyph in local font units shifted by `pen`. */
struct OutlineBuilder {
    std::vector<Contour>* contours;
    double pen_x;
    double scale;
    gp_Pnt origin;
    gp_Vec x_axis;
    gp_Vec y_axis;
    gp_XY start{0, 0};
    gp_XY last{0, 0};
    bool open = false;
    const char* error = nullptr;

    gp_XY local(const FT_Vector* v) const {
        return gp_XY((pen_x + static_cast<double>(v->x)) * scale, static_cast<double>(v->y) * scale);
    }
    gp_Pnt place(const gp_XY& p) const {
        return origin.Translated(x_axis * p.X() + y_axis * p.Y());
    }
    void add_bezier(const std::vector<gp_XY>& poles) {
        if (poles.front().IsEqual(poles.back(), 1e-12)) {
            bool all_same = true;
            for (const auto& p : poles) all_same = all_same && p.IsEqual(poles.front(), 1e-12);
            if (all_same) return;
        }
        TopoDS_Edge edge;
        if (poles.size() == 2) {
            BRepBuilderAPI_MakeEdge maker(place(poles[0]), place(poles[1]));
            if (!maker.IsDone()) {
                error = "glyph line edge failed";
                return;
            }
            edge = maker.Edge();
        } else {
            TColgp_Array1OfPnt points(1, static_cast<int>(poles.size()));
            for (size_t k = 0; k < poles.size(); ++k) points(static_cast<int>(k) + 1) = place(poles[k]);
            BRepBuilderAPI_MakeEdge maker(opencascade::handle<Geom_BezierCurve>(new Geom_BezierCurve(points)));
            if (!maker.IsDone()) {
                error = "glyph curve edge failed";
                return;
            }
            edge = maker.Edge();
        }
        Contour& c = contours->back();
        c.edges.push_back(edge);
        // Samples along the control polygon at Bezier parameters.
        const int steps = poles.size() == 2 ? 1 : 8;
        for (int s = 1; s <= steps; ++s) {
            const double t = static_cast<double>(s) / steps;
            std::vector<gp_XY> b(poles);
            for (size_t r = 1; r < b.size(); ++r)
                for (size_t k = 0; k + r < b.size(); ++k) b[k] = b[k] * (1 - t) + b[k + 1] * t;
            c.samples.push_back(b[0]);
        }
    }
    void close() {
        if (open && !last.IsEqual(start, 1e-12)) add_bezier({last, start});
        open = false;
    }
};

int move_to(const FT_Vector* to, void* user) {
    auto* b = static_cast<OutlineBuilder*>(user);
    b->close();
    b->contours->emplace_back();
    b->start = b->last = b->local(to);
    b->contours->back().samples.push_back(b->start);
    b->open = true;
    return 0;
}
int line_to(const FT_Vector* to, void* user) {
    auto* b = static_cast<OutlineBuilder*>(user);
    const gp_XY p = b->local(to);
    b->add_bezier({b->last, p});
    b->last = p;
    return b->error ? 1 : 0;
}
int conic_to(const FT_Vector* control, const FT_Vector* to, void* user) {
    auto* b = static_cast<OutlineBuilder*>(user);
    const gp_XY p = b->local(to);
    b->add_bezier({b->last, b->local(control), p});
    b->last = p;
    return b->error ? 1 : 0;
}
int cubic_to(const FT_Vector* c1, const FT_Vector* c2, const FT_Vector* to, void* user) {
    auto* b = static_cast<OutlineBuilder*>(user);
    const gp_XY p = b->local(to);
    b->add_bezier({b->last, b->local(c1), b->local(c2), p});
    b->last = p;
    return b->error ? 1 : 0;
}

bool inside(const std::vector<gp_XY>& polygon, const gp_XY& p) {
    bool in = false;
    for (size_t i = 0, j = polygon.size() - 1; i < polygon.size(); j = i++) {
        const gp_XY& a = polygon[i];
        const gp_XY& b = polygon[j];
        if ((a.Y() > p.Y()) != (b.Y() > p.Y())
            && p.X() < (b.X() - a.X()) * (p.Y() - a.Y()) / (b.Y() - a.Y()) + a.X()) {
            in = !in;
        }
    }
    return in;
}

struct FreeTypeLibrary {
    FT_Library library = nullptr;
    FT_Face face = nullptr;
    ~FreeTypeLibrary() {
        if (face) FT_Done_Face(face);
        if (library) FT_Done_FreeType(library);
    }
};

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_font_file(
    occt_bridge_session_t* session,
    const char* family,
    int style,
    char* buffer,
    size_t capacity,
    size_t* out_length) {
    return guarded(session, [&] {
        if (family == nullptr || out_length == nullptr || style < 0 || style > 3
            || (buffer == nullptr && capacity != 0) || family[0] == '\0') {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid font query");
        }
        *out_length = 0;
        if (!FcInit()) return fail(session, OCCT_BRIDGE_INTERNAL_ERROR, "fontconfig failed to initialize");
        const bool bold = (style & 1) != 0, italic = (style & 2) != 0;
        std::unique_ptr<FcPattern, decltype(&FcPatternDestroy)> pattern(FcPatternCreate(), FcPatternDestroy);
        FcPatternAddString(pattern.get(), FC_FAMILY, reinterpret_cast<const FcChar8*>(family));
        FcPatternAddInteger(pattern.get(), FC_WEIGHT, bold ? FC_WEIGHT_BOLD : FC_WEIGHT_REGULAR);
        FcPatternAddInteger(pattern.get(), FC_SLANT, italic ? FC_SLANT_ITALIC : FC_SLANT_ROMAN);
        FcPatternAddBool(pattern.get(), FC_SCALABLE, FcTrue);
        FcConfigSubstitute(nullptr, pattern.get(), FcMatchPattern);
        FcDefaultSubstitute(pattern.get());
        FcResult result;
        std::unique_ptr<FcPattern, decltype(&FcPatternDestroy)> match(
            FcFontMatch(nullptr, pattern.get(), &result), FcPatternDestroy);
        if (!match) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "no installed font matches");
        // Strict: the match must carry the requested family, weight and slant.
        bool same_family = false;
        FcChar8* name = nullptr;
        for (int k = 0; FcPatternGetString(match.get(), FC_FAMILY, k, &name) == FcResultMatch; ++k) {
            same_family = same_family || FcStrCmpIgnoreCase(name, reinterpret_cast<const FcChar8*>(family)) == 0;
        }
        int weight = 0, slant = 0;
        FcPatternGetInteger(match.get(), FC_WEIGHT, 0, &weight);
        FcPatternGetInteger(match.get(), FC_SLANT, 0, &slant);
        if (!same_family || (bold != (weight >= FC_WEIGHT_DEMIBOLD)) || (italic != (slant != FC_SLANT_ROMAN))) {
            return fail(
                session,
                OCCT_BRIDGE_INVALID_GEOMETRY,
                std::string("font family '") + family + "' is not installed in the requested style");
        }
        FcChar8* file = nullptr;
        if (FcPatternGetString(match.get(), FC_FILE, 0, &file) != FcResultMatch || file == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "matched font has no file");
        }
        const size_t length = std::strlen(reinterpret_cast<const char*>(file));
        *out_length = length;
        if (buffer == nullptr) return succeed(session);
        if (capacity <= length) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "font path buffer is too small");
        std::memcpy(buffer, file, length + 1);
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_create_text(
    occt_bridge_session_t* session,
    const char* utf8,
    size_t length,
    const char* font_file,
    double height,
    occt_bridge_vec3_t origin,
    occt_bridge_vec3_t x_axis,
    occt_bridge_vec3_t y_axis,
    int align,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (utf8 == nullptr || length == 0 || font_file == nullptr || !std::isfinite(height) || height <= 0
            || !finite(origin) || !finite(x_axis) || !finite(y_axis) || align < 0 || align > 2) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid text parameters");
        }
        gp_Vec x(x_axis.x, x_axis.y, x_axis.z), y(y_axis.x, y_axis.y, y_axis.z);
        if (x.Magnitude() <= Precision::Confusion() || y.Magnitude() <= Precision::Confusion()
            || std::abs(x.Normalized().Dot(y.Normalized())) > 1e-9) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "text axes must be nonzero and perpendicular");
        }
        x.Normalize();
        y.Normalize();
        std::vector<char32_t> characters;
        if (!decode_utf8(utf8, length, characters) || characters.empty()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "text must be printable UTF-8 on one line");
        }
        if (characters.size() > kMaxCharacters) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "text is limited to 1000 characters");
        }
        FreeTypeLibrary ft;
        if (FT_Init_FreeType(&ft.library) != 0) return fail(session, OCCT_BRIDGE_INTERNAL_ERROR, "FreeType failed to initialize");
        if (FT_New_Face(ft.library, font_file, 0, &ft.face) != 0 || !FT_IS_SCALABLE(ft.face)) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "font file is not a readable scalable font");
        }
        const double scale = height / static_cast<double>(ft.face->units_per_EM);
        std::vector<Contour> contours;
        OutlineBuilder builder{&contours, 0.0, scale, to_point(origin), x, y};
        FT_Outline_Funcs funcs{move_to, line_to, conic_to, cubic_to, 0, 0};
        double pen = 0.0;
        FT_UInt previous = 0;
        for (char32_t code : characters) {
            const FT_UInt index = FT_Get_Char_Index(ft.face, code);
            if (index == 0 && code != U' ') {
                char hex[16];
                std::snprintf(hex, sizeof hex, "U+%04X", static_cast<unsigned>(code));
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, std::string("font has no glyph for ") + hex);
            }
            if (previous != 0 && index != 0 && FT_HAS_KERNING(ft.face)) {
                FT_Vector kern;
                if (FT_Get_Kerning(ft.face, previous, index, FT_KERNING_UNSCALED, &kern) == 0) pen += static_cast<double>(kern.x);
            }
            if (FT_Load_Glyph(ft.face, index, FT_LOAD_NO_SCALE | FT_LOAD_NO_BITMAP) != 0) {
                return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "glyph failed to load");
            }
            if (ft.face->glyph->format == FT_GLYPH_FORMAT_OUTLINE && ft.face->glyph->outline.n_contours > 0) {
                builder.pen_x = pen;
                if (FT_Outline_Decompose(&ft.face->glyph->outline, &funcs, &builder) != 0 || builder.error) {
                    return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, builder.error ? builder.error : "glyph outline failed");
                }
                builder.close();
            }
            pen += static_cast<double>(ft.face->glyph->advance.x);
            previous = index;
        }
        contours.erase(std::remove_if(contours.begin(), contours.end(), [](const Contour& c) { return c.edges.empty(); }), contours.end());
        if (contours.empty()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "text has no visible glyphs");
        // Alignment shifts every edge along x.
        const double shift = align == 0 ? 0.0 : -(align == 1 ? 0.5 : 1.0) * pen * scale;
        gp_Trsf move;
        move.SetTranslation(x * shift);
        // Nesting by containment of each contour's first sample, smallest
        // containing contour as parent; even depth is an outer boundary.
        for (auto& c : contours) {
            double area = 0.0;
            for (size_t i = 0, j = c.samples.size() - 1; i < c.samples.size(); j = i++)
                area += c.samples[j].X() * c.samples[i].Y() - c.samples[i].X() * c.samples[j].Y();
            c.area = std::abs(area) / 2.0;
            c.counterclockwise = area > 0.0;
        }
        for (size_t i = 0; i < contours.size(); ++i) {
            for (size_t j = 0; j < contours.size(); ++j) {
                if (i == j || contours[j].area <= contours[i].area) continue;
                if (inside(contours[j].samples, contours[i].samples.front())) {
                    ++contours[i].depth;
                    if (contours[i].parent < 0 || contours[j].area < contours[static_cast<size_t>(contours[i].parent)].area) {
                        contours[i].parent = static_cast<int>(j);
                    }
                }
            }
        }
        const gp_Pln plane(to_point(origin), gp_Dir(x.Crossed(y)));
        TopoDS_Compound compound;
        BRep_Builder compound_builder;
        compound_builder.MakeCompound(compound);
        size_t faces = 0;
        for (size_t i = 0; i < contours.size(); ++i) {
            if (contours[i].depth % 2 != 0) continue;
            // Outer boundaries run counterclockwise about the plane normal
            // (x × y), holes clockwise, whatever the font's convention.
            const auto wire_of = [&](const Contour& c, bool counterclockwise) {
                BRepBuilderAPI_MakeWire wire;
                for (const auto& e : c.edges) wire.Add(e);
                TopoDS_Wire result;
                if (wire.IsDone()) {
                    result = wire.Wire();
                    if (c.counterclockwise != counterclockwise) result.Reverse();
                }
                return result;
            };
            const TopoDS_Wire outer = wire_of(contours[i], true);
            if (outer.IsNull() || !outer.Closed()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "glyph contour is not closed");
            BRepBuilderAPI_MakeFace face(plane, outer, Standard_True);
            for (size_t j = 0; j < contours.size(); ++j) {
                if (contours[j].parent == static_cast<int>(i) && contours[j].depth % 2 != 0) {
                    const TopoDS_Wire hole = wire_of(contours[j], false);
                    if (hole.IsNull() || !hole.Closed()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "glyph hole is not closed");
                    face.Add(hole);
                }
            }
            if (!face.IsDone()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "glyph face construction failed");
            TopoDS_Shape fixed = face.Face().Moved(TopLoc_Location(move));
            if (!BRepCheck_Analyzer(fixed).IsValid()) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "glyph face is invalid");
            compound_builder.Add(compound, fixed);
            ++faces;
        }
        if (faces == 0) return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "text has no outer glyph contours");
        return store_shape(session, compound, out_shape);
    });
}

}  // extern "C"
