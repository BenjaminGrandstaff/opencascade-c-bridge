use occt_bridge::{ProjectionFrame, Session, Shape, ShapeType, Vec3};
use std::{error::Error, fmt::Write};

/// Isometric HLR preview; curve sampling is deliberately separate from CAD
/// export and engineering verification. Bounded to one million SVG vertices.
pub fn svg(session: &Session, shape: &Shape<'_>) -> Result<String, Box<dyn Error>> {
    let bounds = session.exact_bounds(shape)?;
    let origin = Vec3::new(
        bounds.min.x * 0.5 + bounds.max.x * 0.5,
        bounds.min.y * 0.5 + bounds.max.y * 0.5,
        bounds.min.z * 0.5 + bounds.max.z * 0.5,
    );
    let projection = session.orthographic_projection(
        shape,
        ProjectionFrame {
            origin,
            direction: Vec3::new(1.0, -1.0, 1.0),
            x_axis: Vec3::new(1.0, 1.0, 0.0),
        },
    )?;
    let mut paths = String::new();
    let mut count = 0;
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for (compound, hidden) in [(&projection.hidden, true), (&projection.visible, false)] {
        let edges = session.subshapes(compound, ShapeType::Edge)?;
        if edges.len() > (1_000_000 - count) / 32 {
            return Err("preview vertex budget exceeded".into());
        }
        write!(
            paths,
            "<g fill=\"none\" stroke=\"{}\" {}>",
            if hidden { "#aaa" } else { "#222" },
            if hidden {
                "stroke-dasharray=\"4 3\""
            } else {
                ""
            }
        )?;
        for edge in edges {
            let points = session.edge_sample_points(&edge, 32)?;
            count += points.len();
            if count > 1_000_000 {
                return Err("preview vertex budget exceeded".into());
            }
            paths.push_str("<polyline points=\"");
            for p in points {
                if !p.x.is_finite() || !p.y.is_finite() {
                    return Err("nonfinite preview coordinate".into());
                }
                min[0] = min[0].min(p.x);
                min[1] = min[1].min(-p.y);
                max[0] = max[0].max(p.x);
                max[1] = max[1].max(-p.y);
                write!(paths, "{},{} ", p.x, -p.y)?;
            }
            paths.push_str("\" vector-effect=\"non-scaling-stroke\"/>");
        }
        paths.push_str("</g>");
    }
    if count == 0 {
        return Err("selected output has no preview edges".into());
    }
    let width = (max[0] - min[0]).max(1.0);
    let height = (max[1] - min[1]).max(1.0);
    let pad = width.max(height) * 0.05;
    if !width.is_finite()
        || !height.is_finite()
        || !(width + 2.0 * pad).is_finite()
        || !(height + 2.0 * pad).is_finite()
    {
        return Err("preview extent is not representable".into());
    }
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"800\" height=\"600\" viewBox=\"{} {} {} {}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"white\"/>{paths}</svg>",
        min[0] - pad,
        min[1] - pad,
        width + 2.0 * pad,
        height + 2.0 * pad,
        min[0] - pad,
        min[1] - pad,
        width + 2.0 * pad,
        height + 2.0 * pad
    ))
}
