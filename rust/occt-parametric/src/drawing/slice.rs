//! Intersect valid solids with a bounded face enclosing the entire view plane
//! footprint. Unlike clipped-solid HLR, nothing behind the cut is projected.
use super::*;

pub(super) fn append(
    session: &Session,
    view: &DrawingView,
    shape: &Shape<'_>,
    options: DrawingRenderOptions,
    vertices: &mut usize,
    drawing: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let topology = crate::verification::connectivity(session, shape)?;
    if topology.solids == 0
        || topology.loose_faces + topology.loose_edges + topology.loose_vertices != 0
        || !session.is_valid(shape)?
    {
        return Err(ModelError::new(
            "slice views require valid solid geometry without loose topology",
        ));
    }
    let frame = view.frame()?;
    let bounds = session.exact_bounds(shape)?;
    let (minimum, maximum) = projected_bounds(view, bounds)?;
    let corners = plane_corners(frame, bounds, minimum, maximum);
    let wire = session.create_polyline_wire(&corners, true)?;
    let plane = session.create_face_from_wire(&wire)?;
    let section = session.common(shape, &plane)?;
    append_edges(session, view, &section, false, options, vertices, drawing)
}

fn projected_bounds(
    view: &DrawingView,
    bounds: occt_bridge::Bounds,
) -> Result<([f64; 2], [f64; 2]), ModelError> {
    let mut minimum = [f64::INFINITY; 2];
    let mut maximum = [f64::NEG_INFINITY; 2];
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            for z in [bounds.min.z, bounds.max.z] {
                let point = view.project(Vec3::new(x, y, z))?;
                for axis in 0..2 {
                    minimum[axis] = minimum[axis].min(point[axis]);
                    maximum[axis] = maximum[axis].max(point[axis]);
                }
            }
        }
    }
    Ok((minimum, maximum))
}

fn plane_corners(
    frame: occt_bridge::ProjectionFrame,
    bounds: occt_bridge::Bounds,
    minimum: [f64; 2],
    maximum: [f64; 2],
) -> [Vec3; 4] {
    // Keep the tool's perimeter strictly outside the shape's projection, even
    // with large world coordinates; it must never become a cutting contour.
    let coordinate_scale = [
        bounds.min.x,
        bounds.min.y,
        bounds.min.z,
        bounds.max.x,
        bounds.max.y,
        bounds.max.z,
    ]
    .into_iter()
    .chain(minimum)
    .chain(maximum)
    .map(f64::abs)
    .fold(1.0, f64::max);
    let margin = 1.0_f64.max(coordinate_scale * (256.0 * f64::EPSILON));
    let lower = [minimum[0] - margin, minimum[1] - margin];
    let upper = [maximum[0] + margin, maximum[1] + margin];
    let up = cross(frame.direction, frame.x_axis);
    [
        [lower[0], lower[1]],
        [upper[0], lower[1]],
        [upper[0], upper[1]],
        [lower[0], upper[1]],
    ]
    .map(|[x, y]| {
        Vec3::new(
            frame.origin.x + frame.x_axis.x * x + up.x * y,
            frame.origin.y + frame.x_axis.y * x + up.y * y,
            frame.origin.z + frame.x_axis.z * x + up.z * y,
        )
    })
}
