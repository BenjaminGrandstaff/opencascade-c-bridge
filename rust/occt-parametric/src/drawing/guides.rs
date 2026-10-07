//! Datum-linked drawing guides; fixed paper sizes, current instance poses.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingGuide {
    pub id: String,
    pub view: String,
    pub kind: DrawingGuideKind,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DrawingGuideKind {
    CenterMark {
        center: DatumRef,
        half_length_mm: f64,
    },
    Centerline {
        first: DatumRef,
        second: DatumRef,
        extension_mm: f64,
    },
    /// Endpoints lie on the referenced section's cut plane. Arrows show sight
    /// direction (opposite section view direction), not retained material side.
    CuttingPlane {
        first: DatumRef,
        second: DatumRef,
        section_view: String,
        label: String,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawingGuideLineKind {
    Center,
    CenterMark,
    CuttingPlane,
    Arrow,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingGuideLine {
    pub points_mm: Vec<[f64; 2]>,
    pub kind: DrawingGuideLineKind,
}

/// O(guides) time and constant extra storage. Includes the existing 9 frame
/// vertices. Generation checks this before allocating guide geometry.
pub(super) fn vertex_count(guides: &[DrawingGuide]) -> Result<usize, ModelError> {
    guides.iter().try_fold(9usize, |sum, guide| {
        let count = match guide.kind {
            DrawingGuideKind::CenterMark { .. } => 4,
            DrawingGuideKind::Centerline { .. } => 2,
            DrawingGuideKind::CuttingPlane { .. } => 14,
        };
        sum.checked_add(count)
            .ok_or_else(|| ModelError::new("drawing guide vertex count overflow"))
    })
}

struct GuideGeometry {
    first: [f64; 2],
    second: [f64; 2],
    direction: [f64; 2],
}
fn pair(
    first: &DatumRef,
    second: &DatumRef,
    view: &DrawingView,
    graph: &InstanceGraph<'_>,
) -> Result<GuideGeometry, ModelError> {
    let first = view.paper(view.project(datum_origin(graph, first)?)?)?;
    let second = view.paper(view.project(datum_origin(graph, second)?)?)?;
    let delta = [second[0] - first[0], second[1] - first[1]];
    let length = delta[0].hypot(delta[1]);
    if !length.is_finite() || length <= 1e-12 {
        return Err(ModelError::new(
            "drawing guide needs distinct projected endpoints",
        ));
    }
    Ok(GuideGeometry {
        first,
        second,
        direction: [delta[0] / length, delta[1] / length],
    })
}
fn offset(point: [f64; 2], direction: [f64; 2], amount: f64) -> Result<[f64; 2], ModelError> {
    let point = [
        point[0] + direction[0] * amount,
        point[1] + direction[1] * amount,
    ];
    if !finite_pair(point) {
        return Err(ModelError::new(
            "drawing guide exceeds finite paper coordinates",
        ));
    }
    Ok(point)
}
fn positive(value: f64) -> Result<(), ModelError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(ModelError::new(
            "center mark half-length must be finite and positive",
        ));
    }
    Ok(())
}

fn section_direction(
    kind: &DrawingGuideKind,
    source: &DrawingView,
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<[f64; 2], ModelError> {
    let DrawingGuideKind::CuttingPlane {
        first,
        second,
        section_view,
        label,
    } = kind
    else {
        unreachable!()
    };
    if label.trim().is_empty() || label.chars().any(char::is_control) {
        return Err(ModelError::new(
            "cutting-plane label must be nonempty plain text",
        ));
    }
    validate_text(&format!("SECTION {label}-{label}"))?;
    let section = views
        .get(section_view.as_str())
        .ok_or_else(|| ModelError::new("cutting plane references an unknown section view"))?;
    if section.id == source.id {
        return Err(ModelError::new(
            "cutting plane source and section views must differ",
        ));
    }
    let DrawingViewKind::Section { origin, normal, .. } = section.kind else {
        return Err(ModelError::new("cutting plane requires a Section view"));
    };
    let origin = origin.normalized(Dimension::Length)?;
    let normal = axis(normal)?;
    let section_frame = section.frame()?;
    if dot(normal, section_frame.direction).abs() < 1.0 - 1e-10 {
        return Err(ModelError::new(
            "section view must look normal to its cutting plane",
        ));
    }
    for reference in [first, second] {
        let delta = subtract(datum_origin(graph, reference)?, origin);
        let scale = delta.x.abs().max(delta.y.abs()).max(delta.z.abs()).max(1.0);
        if dot(delta, normal).abs() > 1e-7 + 32.0 * f64::EPSILON * scale {
            return Err(ModelError::new(
                "cutting-plane guide endpoints must lie on its section plane",
            ));
        }
    }
    let frame = source.frame()?;
    if dot(normal, frame.direction).abs() > 1e-10 {
        return Err(ModelError::new(
            "cutting plane must be edge-on in its source view",
        ));
    }
    let sight = [
        -dot(section_frame.direction, frame.x_axis),
        -dot(
            section_frame.direction,
            cross(frame.direction, frame.x_axis),
        ),
    ];
    Ok(sight)
}

/// Validation and generation are O(guides + datum-frame resolution work) with
/// O(guides) output storage. View lookup is indexed once by the drawing caller.
pub(super) fn validate(
    guides: &[DrawingGuide],
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    let mut ids = HashSet::new();
    for guide in guides {
        if guide.id.is_empty() || !ids.insert(&guide.id) {
            return Err(ModelError::new("drawing guides need unique nonempty IDs"));
        }
        let view = views
            .get(guide.view.as_str())
            .ok_or_else(|| ModelError::new("drawing guide references an unknown view"))?;
        validate_kind(&guide.kind, view, views, graph)?;
    }
    Ok(())
}
fn validate_kind(
    kind: &DrawingGuideKind,
    view: &DrawingView,
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    match kind {
        DrawingGuideKind::CenterMark {
            center,
            half_length_mm,
        } => {
            positive(*half_length_mm)?;
            let p = view.paper(view.project(datum_origin(graph, center)?)?)?;
            for direction in [[1.0, 0.0], [0.0, 1.0]] {
                offset(p, direction, *half_length_mm)?;
                offset(p, direction, -*half_length_mm)?;
            }
        }
        DrawingGuideKind::Centerline {
            first,
            second,
            extension_mm,
        } => {
            if !extension_mm.is_finite() || *extension_mm < 0.0 {
                return Err(ModelError::new(
                    "centerline extension must be finite and nonnegative",
                ));
            }
            let pair = pair(first, second, view, graph)?;
            offset(pair.first, pair.direction, -*extension_mm)?;
            offset(pair.second, pair.direction, *extension_mm)?;
        }
        DrawingGuideKind::CuttingPlane { first, second, .. } => {
            pair(first, second, view, graph)?;
            section_direction(kind, view, views, graph)?;
        }
    }
    Ok(())
}
fn line(drawing: &mut GeneratedDrawing, points_mm: Vec<[f64; 2]>, kind: DrawingGuideLineKind) {
    drawing.guides.push(DrawingGuideLine { points_mm, kind });
}
fn text(drawing: &mut GeneratedDrawing, position_mm: [f64; 2], text: String) {
    drawing.labels.push(DrawingLabel {
        position_mm,
        text,
        stack: None,
    });
}
pub(super) fn append(
    guides: &[DrawingGuide],
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
    drawing: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    for guide in guides {
        let view = views[guide.view.as_str()];
        match &guide.kind {
            DrawingGuideKind::CenterMark {
                center,
                half_length_mm,
            } => {
                let p = view.paper(view.project(datum_origin(graph, center)?)?)?;
                for direction in [[1.0, 0.0], [0.0, 1.0]] {
                    line(
                        drawing,
                        vec![
                            offset(p, direction, -*half_length_mm)?,
                            offset(p, direction, *half_length_mm)?,
                        ],
                        DrawingGuideLineKind::CenterMark,
                    );
                }
            }
            DrawingGuideKind::Centerline {
                first,
                second,
                extension_mm,
            } => {
                let pair = pair(first, second, view, graph)?;
                line(
                    drawing,
                    vec![
                        offset(pair.first, pair.direction, -*extension_mm)?,
                        offset(pair.second, pair.direction, *extension_mm)?,
                    ],
                    DrawingGuideLineKind::Center,
                );
            }
            DrawingGuideKind::CuttingPlane {
                first,
                second,
                section_view,
                label,
            } => {
                let pair = pair(first, second, view, graph)?;
                let sight = section_direction(&guide.kind, view, views, graph)?;
                append_cut(pair, sight, label, views[section_view.as_str()], drawing)?;
            }
        }
    }
    Ok(())
}
fn append_cut(
    pair: GuideGeometry,
    sight: [f64; 2],
    label: &str,
    section: &DrawingView,
    drawing: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    line(
        drawing,
        vec![pair.first, pair.second],
        DrawingGuideLineKind::CuttingPlane,
    );
    let across = [-sight[1], sight[0]];
    for tip in [pair.first, pair.second] {
        let tail = offset(tip, sight, -4.0)?;
        line(drawing, vec![tail, tip], DrawingGuideLineKind::Arrow);
        let base = offset(tip, sight, -2.0)?;
        for side in [-0.7, 0.7] {
            line(
                drawing,
                vec![tip, offset(base, across, side)?],
                DrawingGuideLineKind::Arrow,
            );
        }
        text(drawing, offset(tail, sight, -4.0)?, label.to_owned());
    }
    text(
        drawing,
        offset(section.paper_origin_mm, [0.0, 1.0], -8.0)?,
        format!("SECTION {label}-{label}"),
    );
    Ok(())
}
