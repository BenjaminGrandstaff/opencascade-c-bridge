//! Conventional thread representation in drawing views (schema 154), after
//! ISO 6410-1: modeled threads and tapped holes are drawn by their roots
//! rather than by the helical faces of the thread itself.
use super::*;
use crate::assembly::{add, dot, scale, unit};

/// A thread placed in the assembly: its start on the axis, the unit axis
/// along which it runs, its length, and its major and basic minor diameters.
/// A through or up-to hole's thread has no length in its definition; the
/// kernel measures it on the generated solid (see `PlacedThread::length`).
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedThread {
    pub start: Vec3,
    pub axis: Vec3,
    /// None for a through-all or up-to hole without a modeled length.
    pub length: Option<f64>,
    pub major: f64,
    pub minor: f64,
    pub internal: bool,
}

/// Features that move the geometry before them, so a thread beneath one is
/// not where its own definition puts it.
fn moves(operation: &FeatureOperation) -> bool {
    matches!(
        operation,
        FeatureOperation::Translate { .. }
            | FeatureOperation::Rotate { .. }
            | FeatureOperation::Mirror { .. }
            | FeatureOperation::Scale { .. }
            | FeatureOperation::LinearPattern { .. }
            | FeatureOperation::CircularPattern { .. }
            | FeatureOperation::CurvePattern { .. }
            | FeatureOperation::FillPattern { .. }
            | FeatureOperation::TablePattern { .. }
    )
}

/// Threads of `output` on `instance` in assembly coordinates: thread
/// features and threaded holes among the output's ancestors, short of any
/// feature that moves them. A hole's thread runs its modeled length, or its
/// blind depth; through and up-to holes without a modeled length have none
/// here. O(features).
pub fn placed_threads(
    graph: &InstanceGraph<'_>,
    output: &InstanceOutputRef,
) -> Result<Vec<PlacedThread>, ModelError> {
    let resolved = graph.resolve_with_placement(&output.instance)?;
    let definition = resolved.instance.definition;
    let parameters = resolve_parameters(definition, &resolved.instance.overrides)?;
    let features: HashMap<&str, &FeatureDefinition> = definition
        .features
        .iter()
        .map(|f| (f.id.as_str(), f))
        .collect();
    let length = |e: &ScalarExpr| crate::expressions::scalar(e, &parameters, Dimension::Length);
    let vector = |e: &VectorExpr, dimension| -> Result<Vec3, ModelError> {
        let value = e.evaluate(&parameters)?.normalized(dimension)?;
        Ok(Vec3::new(value.x, value.y, value.z))
    };
    let mut local = Vec::new();
    let mut pending = vec![output.output.as_str()];
    let mut seen = HashSet::new();
    while let Some(id) = pending.pop() {
        let Some(feature) = features.get(id) else {
            continue;
        };
        if !seen.insert(id) || moves(&feature.operation) {
            continue;
        }
        match &feature.operation {
            FeatureOperation::Thread {
                origin,
                axis,
                major_diameter,
                pitch,
                length: run,
                internal,
                ..
            } => local.push((
                vector(origin, Dimension::Length)?,
                vector(axis, Dimension::Scalar)?,
                Some(length(run)?),
                length(major_diameter)?,
                length(pitch)?,
                *internal,
            )),
            FeatureOperation::Hole {
                position,
                axis,
                extent,
                thread: Some(thread),
                ..
            } => {
                let run = match (
                    thread.modeled.as_ref().and_then(|m| m.length.as_ref()),
                    extent,
                ) {
                    (Some(run), _) | (None, HoleExtent::Blind { depth: run }) => Some(length(run)?),
                    _ => None,
                };
                local.push((
                    vector(position, Dimension::Length)?,
                    vector(axis, Dimension::Scalar)?,
                    run,
                    length(&thread.nominal_diameter)?,
                    length(&thread.pitch)?,
                    true,
                ));
            }
            _ => {}
        }
        pending.extend(feature.operation.dependencies());
    }
    let mut placed = Vec::with_capacity(local.len());
    for (start, axis, run, major, pitch, internal) in local {
        let mut value = ResolvedDatum::Axis {
            origin: start,
            direction: unit(axis)?,
        };
        for placement in std::iter::once(resolved.placement).chain(resolved.frames.clone()) {
            value = value.transformed(&placement.normalized()?);
        }
        let ResolvedDatum::Axis { origin, direction } = value else {
            unreachable!("an axis stays an axis");
        };
        placed.push(PlacedThread {
            start: origin,
            axis: direction,
            length: run,
            major,
            // ISO 68-1 basic minor diameter, D − 2 · 5H/8.
            minor: major - 1.082_532 * pitch,
            internal,
        });
    }
    Ok(placed)
}

fn polyline(points: Vec<[f64; 2]>, hidden: bool) -> DrawingPolyline {
    DrawingPolyline {
        points_mm: points,
        hidden,
        thin: true,
    }
}

/// Points of the circle of `radius` about `center` square to unit `axis`
/// (32 segments), in paper coordinates.
fn circle(
    view: &DrawingView,
    center: Vec3,
    axis: Vec3,
    radius: f64,
) -> Result<Vec<[f64; 2]>, ModelError> {
    let seed = if axis.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u = unit(crate::assembly::cross(axis, seed))?;
    let v = crate::assembly::cross(axis, u);
    (0..=32)
        .map(|i| {
            let angle = std::f64::consts::TAU * i as f64 / 32.0;
            view.paper(view.project(add(
                center,
                add(
                    scale(u, radius * angle.cos()),
                    scale(v, radius * angle.sin()),
                ),
            ))?)
        })
        .collect()
}

/// The representation of one thread in `view` (ISO 6410-1), or nothing
/// when its length is unknown.
///
/// - Square to the axis: a root line each side and a straight thread-end
///   line across the crests.
/// - Along the axis: three quarters of the root circle.
/// - At any other angle: the root cylinder's two silhouette lines (exact
///   in orthographic projection) and the crest circle at the thread end,
///   which projects to an ellipse.
pub fn represent(
    view: &DrawingView,
    thread: &PlacedThread,
) -> Result<Vec<DrawingPolyline>, ModelError> {
    let Some(length) = thread.length else {
        return Ok(Vec::new());
    };
    let frame = view.frame()?;
    let along = dot(thread.axis, frame.direction);
    let paper = |point: Vec3| view.paper(view.project(point)?);
    let section = matches!(view.kind, DrawingViewKind::Section { .. });
    let end = add(thread.start, scale(thread.axis, length));
    let root = if thread.internal {
        thread.major
    } else {
        thread.minor
    } / 2.0;
    let crest = if thread.internal {
        thread.minor
    } else {
        thread.major
    } / 2.0;
    if (along.abs() - 1.0).abs() < 1e-9 {
        // End view: three quarters of the root circle, open in the upper
        // right quadrant. A tapped hole shows from its entry side.
        let x = frame.x_axis;
        let y = crate::assembly::cross(frame.direction, frame.x_axis);
        let hidden = thread.internal && along > 0.0;
        let points = (0..=24)
            .map(|i| {
                let angle =
                    std::f64::consts::FRAC_PI_2 + 1.5 * std::f64::consts::PI * i as f64 / 24.0;
                paper(add(
                    thread.start,
                    add(scale(x, root * angle.cos()), scale(y, root * angle.sin())),
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(vec![polyline(points, hidden)]);
    }
    // Side or oblique view: the root cylinder's silhouette lines lie where
    // its surface turns away from the viewer, square to both the axis and
    // the view direction. External roots show; an internal thread shows
    // only cut open in a section view.
    let up = unit(crate::assembly::cross(frame.direction, thread.axis))?;
    let hidden = thread.internal && !section;
    let at = |base: Vec3, radius: f64| paper(add(base, scale(up, radius)));
    let mut lines = vec![
        polyline(vec![at(thread.start, root)?, at(end, root)?], hidden),
        polyline(vec![at(thread.start, -root)?, at(end, -root)?], hidden),
    ];
    let edge = crest.max(root);
    if along.abs() < 1e-9 {
        lines.push(polyline(vec![at(end, edge)?, at(end, -edge)?], hidden));
    } else {
        lines.push(polyline(circle(view, end, thread.axis, edge)?, hidden));
    }
    Ok(lines)
}

/// The thread representation lines of every view that asks for them, in
/// view order. O(views · (outputs · features)).
pub fn lines(
    views: &[DrawingView],
    graph: &InstanceGraph<'_>,
) -> Result<Vec<DrawingPolyline>, ModelError> {
    let mut lines = Vec::new();
    for view in views.iter().filter(|v| v.thread_representation) {
        if matches!(view.kind, DrawingViewKind::Slice) {
            continue;
        }
        for output in &view.outputs {
            for thread in placed_threads(graph, output)? {
                lines.extend(represent(view, &thread)?);
            }
        }
    }
    Ok(lines)
}
