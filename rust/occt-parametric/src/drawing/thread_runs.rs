//! Thread representation lines with the runs of threaded through-all and
//! up-to holes measured on the generated solid, which their definitions do
//! not give.
use super::*;
use crate::assembly::{add, scale, subtract, unit};
use occt_parametric_core::drawing::thread_lines::{PlacedThread, placed_threads, represent};

/// Most surface crossings followed along one measuring line.
const MAXIMUM_CROSSINGS: usize = 64;

/// Where the measuring line `origin + t·axis` crosses the solid's surface,
/// as values of t in order, from repeated nearest hits. O(crossings · ray).
fn crossings(
    session: &Session,
    shape: &Shape<'_>,
    origin: Vec3,
    axis: Vec3,
    reach: f64,
) -> Result<Vec<f64>, ModelError> {
    let mut hits = Vec::new();
    let mut travelled = 0.0;
    let step = 1e-7 * reach.max(1.0);
    while hits.len() < MAXIMUM_CROSSINGS && travelled < reach {
        let from = add(origin, scale(axis, travelled));
        let Some((_, distance)) = session
            .ray_first_hit(shape, from, axis, reach - travelled)
            .map_err(|e| ModelError::new(e.to_string()))?
        else {
            break;
        };
        travelled += distance;
        hits.push(travelled);
        travelled += step;
    }
    Ok(hits)
}

/// The run of a threaded through-all or up-to hole: from where material
/// begins around the hole to where the bore ends, or the material does.
/// A line parallel to the axis just outside the thread's crests finds the
/// material (first and last crossing); a line down the axis from the hole's
/// position finds the bore's bottom when it has one. None when the line
/// meets no material.
fn measure(
    session: &Session,
    shape: &Shape<'_>,
    thread: &PlacedThread,
) -> Result<Option<PlacedThread>, ModelError> {
    let bounds = session
        .exact_bounds(shape)
        .map_err(|e| ModelError::new(e.to_string()))?;
    let span = length(subtract(bounds.max, bounds.min));
    let reach = 2.0 * (span + length(subtract(thread.start, bounds.min)));
    let axis = unit(thread.axis)?;
    let seed = if axis.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let side = unit(crate::assembly::cross(axis, seed))?;
    let outside = thread.major / 2.0 + (0.05 * thread.major).max(0.1);
    // Material around the hole, measured from well before the part.
    let back = reach / 2.0;
    let around = add(add(thread.start, scale(side, outside)), scale(axis, -back));
    let material = crossings(session, shape, around, axis, reach)?;
    let (Some(first), Some(last)) = (material.first(), material.last()) else {
        return Ok(None);
    };
    let (entry, exit) = (first - back, last - back);
    // The bore's bottom, if the hole stops inside the part.
    let start = add(
        thread.start,
        scale(axis, entry.max(0.0) + 1e-6 * span.max(1.0)),
    );
    let bottom = session
        .ray_first_hit(shape, start, axis, (exit - entry).max(0.0) + span)
        .map_err(|e| ModelError::new(e.to_string()))?
        .map(|(_, distance)| entry.max(0.0) + distance);
    let end = bottom.map_or(exit, |b| b.min(exit));
    if end <= entry {
        return Ok(None);
    }
    Ok(Some(PlacedThread {
        start: add(thread.start, scale(axis, entry)),
        length: Some(end - entry),
        ..thread.clone()
    }))
}

fn length(v: Vec3) -> f64 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

/// The thread representation lines of every view that asks for them, in
/// view order, measuring runs the definitions leave open on the generated
/// output. O(views · outputs · (features + measured holes · crossings)).
pub(super) fn lines(
    views: &[DrawingView],
    graph: &InstanceGraph<'_>,
    session: &Session,
    generation: &GraphRegeneration<'_>,
) -> Result<Vec<DrawingPolyline>, ModelError> {
    let mut lines = Vec::new();
    for view in views.iter().filter(|v| v.thread_representation) {
        if matches!(view.kind, DrawingViewKind::Slice) {
            continue;
        }
        for output in &view.outputs {
            for thread in placed_threads(graph, output)? {
                let thread = if thread.length.is_some() {
                    Some(thread)
                } else {
                    match generation
                        .result(&output.instance)
                        .and_then(|result| result.shape(&output.output))
                    {
                        Some(shape) => measure(session, shape, &thread)?,
                        None => None,
                    }
                };
                if let Some(thread) = thread {
                    lines.extend(represent(view, &thread)?);
                }
            }
            for thread in crate::features::joint_threads(session, graph, generation, output)? {
                lines.extend(represent(view, &thread)?);
            }
        }
    }
    Ok(lines)
}
