/*
 * Application constructors: the faceted stone and the wall torch.
 */

#include "bridge_internal.hpp"

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include <cmath>
#include <limits>

using namespace occt_bridge_internal;

namespace {

TopoDS_Face triangle_face(const gp_Pnt& first, const gp_Pnt& second, const gp_Pnt& third) {
    BRepBuilderAPI_MakePolygon wire;
    wire.Add(first);
    wire.Add(second);
    wire.Add(third);
    wire.Close();
    if (!wire.IsDone()) {
        return {};
    }
    BRepBuilderAPI_MakeFace face(wire.Wire());
    return face.IsDone() ? face.Face() : TopoDS_Face{};
}

TopoDS_Face polygon_face(const std::vector<gp_Pnt>& points) {
    BRepBuilderAPI_MakePolygon wire;
    for (const gp_Pnt& point : points) {
        wire.Add(point);
    }
    wire.Close();
    if (!wire.IsDone()) {
        return {};
    }
    BRepBuilderAPI_MakeFace face(wire.Wire());
    return face.IsDone() ? face.Face() : TopoDS_Face{};
}

TopoDS_Shape cylinder_between(const gp_Pnt& start, const gp_Pnt& end, double radius) {
    const gp_Vec axis(start, end);
    const double length = axis.Magnitude();
    if (length <= std::numeric_limits<double>::epsilon()) {
        return {};
    }
    BRepPrimAPI_MakeCylinder cylinder(gp_Ax2(start, gp_Dir(axis)), radius, length);
    cylinder.Build();
    return cylinder.IsDone() ? cylinder.Shape() : TopoDS_Shape{};
}

bool add_ring_band(
    BRepBuilderAPI_Sewing& sewing,
    const std::vector<gp_Pnt>& lower,
    const std::vector<gp_Pnt>& upper) {
    for (size_t index = 0; index < lower.size(); ++index) {
        const size_t next = (index + 1) % lower.size();
        const TopoDS_Face first = triangle_face(lower[index], lower[next], upper[next]);
        const TopoDS_Face second = triangle_face(lower[index], upper[next], upper[index]);
        if (first.IsNull() || second.IsNull()) {
            return false;
        }
        sewing.Add(first);
        sewing.Add(second);
    }
    return true;
}

/* Insets the base ring toward its centroid and raises the side ring by the chamfer. */
bool chamfer_rings(
    const std::vector<gp_Pnt>& bottom,
    double chamfer,
    std::vector<gp_Pnt>& base,
    std::vector<gp_Pnt>& lower_side) {
    double center_x = 0.0;
    double center_y = 0.0;
    for (const gp_Pnt& point : bottom) {
        center_x += point.X();
        center_y += point.Y();
    }
    center_x /= static_cast<double>(bottom.size());
    center_y /= static_cast<double>(bottom.size());
    for (size_t index = 0; index < bottom.size(); ++index) {
        const double dx = center_x - bottom[index].X();
        const double dy = center_y - bottom[index].Y();
        const double length = std::hypot(dx, dy);
        if (length <= chamfer) {
            return false;
        }
        base[index].SetX(bottom[index].X() + chamfer * dx / length);
        base[index].SetY(bottom[index].Y() + chamfer * dy / length);
        lower_side[index].SetZ(bottom[index].Z() + chamfer);
    }
    return true;
}

/* Quarter-round rings from the side wall into the top ring, or the top ring alone. */
bool shoulder_rings(
    const std::vector<gp_Pnt>& top,
    const gp_Pnt& top_center,
    double fillet,
    std::vector<std::vector<gp_Pnt>>& rings) {
    if (fillet <= 0.0) {
        rings.push_back(top);
        return true;
    }
    constexpr size_t segments = 5;
    constexpr double half_pi = 1.57079632679489661923;
    rings.reserve(segments + 1);
    for (size_t step = 0; step <= segments; ++step) {
        const double angle = half_pi * static_cast<double>(step) / static_cast<double>(segments);
        const double horizontal_offset = fillet * std::cos(angle);
        const double vertical_offset = fillet * (1.0 - std::sin(angle));
        std::vector<gp_Pnt> ring;
        ring.reserve(top.size());
        for (const gp_Pnt& point : top) {
            const double dx = point.X() - top_center.X();
            const double dy = point.Y() - top_center.Y();
            const double length = std::hypot(dx, dy);
            if (length <= fillet) {
                return false;
            }
            ring.emplace_back(
                point.X() + horizontal_offset * dx / length,
                point.Y() + horizontal_offset * dy / length,
                point.Z() - vertical_offset);
        }
        rings.push_back(std::move(ring));
    }
    return true;
}

/* Fans the crown ring into triangles meeting at the top center. */
bool add_crown(BRepBuilderAPI_Sewing& sewing, const std::vector<gp_Pnt>& crown_edge, const gp_Pnt& top_center) {
    for (size_t index = 0; index < crown_edge.size(); ++index) {
        const size_t next = (index + 1) % crown_edge.size();
        const TopoDS_Face top_face = triangle_face(crown_edge[index], crown_edge[next], top_center);
        if (top_face.IsNull()) {
            return false;
        }
        sewing.Add(top_face);
    }
    return true;
}

TopoDS_Shape build_faceted_solid(
    const std::vector<gp_Pnt>& bottom,
    const std::vector<gp_Pnt>& top,
    const gp_Pnt& top_center,
    double bottom_chamfer,
    double top_fillet) {
    BRepBuilderAPI_Sewing sewing(1.0e-6, Standard_True, Standard_True, Standard_True, Standard_False);

    std::vector<gp_Pnt> base = bottom;
    std::vector<gp_Pnt> lower_side = bottom;
    if (bottom_chamfer > 0.0 && !chamfer_rings(bottom, bottom_chamfer, base, lower_side)) {
        return {};
    }

    TopoDS_Face bottom_face = polygon_face(base);
    if (bottom_face.IsNull()) {
        return {};
    }
    sewing.Add(bottom_face);

    // NOLINTNEXTLINE(readability-suspicious-call-argument): base is the lower ring; lower_side is the raised upper ring.
    if (bottom_chamfer > 0.0 && !add_ring_band(sewing, base, lower_side)) {
        return {};
    }

    std::vector<std::vector<gp_Pnt>> rings;
    if (!shoulder_rings(top, top_center, top_fillet, rings)
        || !add_ring_band(sewing, lower_side, rings.front())) {
        return {};
    }
    for (size_t index = 1; index < rings.size(); ++index) {
        if (!add_ring_band(sewing, rings[index - 1], rings[index])) {
            return {};
        }
    }
    if (!add_crown(sewing, rings.back(), top_center)) {
        return {};
    }

    sewing.Perform();
    const TopoDS_Shell shell = single_shell(sewing.SewedShape());
    if (shell.IsNull()) {
        return {};
    }
    BRepBuilderAPI_MakeSolid solid_builder(shell);
    if (!solid_builder.IsDone()) {
        return {};
    }
    TopoDS_Solid solid = solid_builder.Solid();
    BRepLib::OrientClosedSolid(solid);
    return solid;
}

}  // namespace

extern "C" {

occt_bridge_status_t occt_bridge_create_faceted_stone(
    occt_bridge_session_t* session,
    const occt_bridge_vec3_t* bottom_points,
    const occt_bridge_vec3_t* top_points,
    size_t point_count,
    occt_bridge_vec3_t top_center,
    double bottom_chamfer,
    double top_fillet,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(session, [&] {
        if (out_shape == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_shape is null");
        }
        *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
        if (bottom_points == nullptr || top_points == nullptr || point_count < 3
            || !finite(top_center) || !std::isfinite(bottom_chamfer) || !std::isfinite(top_fillet)
            || bottom_chamfer < 0.0 || top_fillet < 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid faceted-stone parameters");
        }

        std::vector<gp_Pnt> bottom;
        std::vector<gp_Pnt> top;
        bottom.reserve(point_count);
        top.reserve(point_count);
        for (size_t index = 0; index < point_count; ++index) {
            if (!finite(bottom_points[index]) || !finite(top_points[index])) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "stone point is not finite");
            }
            bottom.emplace_back(
                bottom_points[index].x, bottom_points[index].y, bottom_points[index].z);
            top.emplace_back(top_points[index].x, top_points[index].y, top_points[index].z);
        }

        TopoDS_Shape shape = build_faceted_solid(
            bottom,
            top,
            gp_Pnt(top_center.x, top_center.y, top_center.z),
            bottom_chamfer,
            top_fillet);
        if (shape.IsNull()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "faceted stone could not be closed into a solid");
        }
        const ShapeValidator analyzer(shape);
        if (!analyzer.IsValid()) {
            return fail(session, OCCT_BRIDGE_INVALID_GEOMETRY, "faceted stone is not a valid BREP");
        }
        return store_shape(session, shape, out_shape);
    });
}

occt_bridge_status_t occt_bridge_create_wall_torch(
    occt_bridge_session_t* session,
    occt_bridge_vec3_t wall_anchor,
    occt_bridge_vec3_t wall_normal,
    double scale,
    occt_bridge_wall_torch_result_t* out_torch) {
    return guarded(session, [&] {
        if (out_torch == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_torch is null");
        }
        *out_torch = {};
        if (!finite(wall_anchor) || !finite(wall_normal) || !std::isfinite(scale) || scale <= 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid wall-torch parameters");
        }
        const double horizontal_length = std::hypot(wall_normal.x, wall_normal.y);
        if (horizontal_length <= std::numeric_limits<double>::epsilon()) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "wall normal must have an XY component");
        }
        const double normal_x = wall_normal.x / horizontal_length;
        const double normal_y = wall_normal.y / horizontal_length;
        const gp_Dir outward(normal_x, normal_y, 0.0);
        const gp_Dir up(0.0, 0.0, 1.0);
        const gp_Pnt anchor(wall_anchor.x, wall_anchor.y, wall_anchor.z);

        BRepPrimAPI_MakeCylinder plate_builder(
            gp_Ax2(anchor, outward), 16.0 * scale, 6.0 * scale);
        plate_builder.Build();

        const gp_Pnt arm_start(
            wall_anchor.x + normal_x * 5.0 * scale,
            wall_anchor.y + normal_y * 5.0 * scale,
            wall_anchor.z - 7.0 * scale);
        const gp_Pnt arm_end(
            wall_anchor.x + normal_x * 49.0 * scale,
            wall_anchor.y + normal_y * 49.0 * scale,
            wall_anchor.z + 4.0 * scale);
        const TopoDS_Shape arm = cylinder_between(arm_start, arm_end, 4.0 * scale);

        const gp_Pnt stem_start(arm_end.X(), arm_end.Y(), arm_end.Z() - 18.0 * scale);
        const gp_Pnt stem_end(arm_end.X(), arm_end.Y(), arm_end.Z() + 17.0 * scale);
        const TopoDS_Shape stem = cylinder_between(stem_start, stem_end, 4.5 * scale);

        const gp_Pnt cup_base(arm_end.X(), arm_end.Y(), wall_anchor.z + 12.0 * scale);
        BRepPrimAPI_MakeCone cup_builder(
            gp_Ax2(cup_base, up), 8.0 * scale, 15.0 * scale, 18.0 * scale);
        cup_builder.Build();
        if (!plate_builder.IsDone() || arm.IsNull() || stem.IsNull() || !cup_builder.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "wall-torch fixture construction failed");
        }

        TopoDS_Compound fixture;
        BRep_Builder fixture_builder;
        fixture_builder.MakeCompound(fixture);
        fixture_builder.Add(fixture, plate_builder.Shape());
        fixture_builder.Add(fixture, arm);
        fixture_builder.Add(fixture, stem);
        fixture_builder.Add(fixture, cup_builder.Shape());

        const gp_Pnt flame_base(arm_end.X(), arm_end.Y(), wall_anchor.z + 30.0 * scale);
        BRepPrimAPI_MakeCone lower_flame(
            gp_Ax2(flame_base, up), 11.0 * scale, 4.0 * scale, 25.0 * scale);
        lower_flame.Build();
        const gp_Pnt upper_flame_base(
            flame_base.X(), flame_base.Y(), flame_base.Z() + 17.0 * scale);
        BRepPrimAPI_MakeCone upper_flame(
            gp_Ax2(upper_flame_base, up), 6.0 * scale, 0.0, 22.0 * scale);
        upper_flame.Build();
        if (!lower_flame.IsDone() || !upper_flame.IsDone()) {
            return fail(session, OCCT_BRIDGE_KERNEL_ERROR, "wall-torch flame construction failed");
        }
        TopoDS_Compound flame;
        BRep_Builder flame_builder;
        flame_builder.MakeCompound(flame);
        flame_builder.Add(flame, lower_flame.Shape());
        flame_builder.Add(flame, upper_flame.Shape());

        occt_bridge_status_t status = store_shape(session, fixture, &out_torch->fixture_shape);
        if (status != OCCT_BRIDGE_OK) {
            return status;
        }
        status = store_shape(session, flame, &out_torch->flame_shape);
        if (status != OCCT_BRIDGE_OK) {
            session->shapes.erase(out_torch->fixture_shape);
            out_torch->fixture_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
            return status;
        }

        out_torch->light.type = OCCT_BRIDGE_LIGHT_POSITIONAL;
        out_torch->light.position = {
            flame_base.X(), flame_base.Y(), flame_base.Z() + 14.0 * scale};
        out_torch->light.direction = {0.0, 0.0, 0.0};
        out_torch->light.color = {1.0, 0.32, 0.06};
        out_torch->light.intensity = 2000000.0;
        out_torch->light.range = 0.0;
        out_torch->light.spot_angle_degrees = 0.0;
        out_torch->light.cast_shadows = 0;
        return succeed(session);
    });
}

}  // extern "C"
