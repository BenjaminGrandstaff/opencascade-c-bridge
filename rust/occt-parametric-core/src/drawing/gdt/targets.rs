//! Datum targets (ASME Y14.5-2018 §7.24; schema 166): points, lines or
//! circular areas at basic locations on a planar datum feature. When a datum
//! feature lists targets, they alone establish its datum: measured
//! inspection fits the simulator to the points measured at the targets, and
//! the drawing marks each target and labels it with a divided-circle symbol
//! (`A1`, with an area's size in the upper half).
use super::*;
use crate::assembly::{add, scale};

/// Radius (mm) of the datum target symbol on paper.
const SYMBOL_RADIUS_MM: f64 = 5.0;
/// Most targets on one datum feature.
pub const MAX_DATUM_TARGETS: usize = 100;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingDatumTarget {
    /// Number after the datum feature's label: 1 makes target `A1`. Unique
    /// within the feature, from 1 to 99.
    pub index: u32,
    pub shape: DatumTargetShape,
    /// Paper offset (mm) of the target symbol's center from the target in
    /// the datum feature's view; at least 8 mm long.
    pub offset_mm: [f64; 2],
}

/// A target's basic geometry, in the datum feature's instance coordinates,
/// lying on the feature's nominal datum plane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DatumTargetShape {
    Point {
        at: VectorQuantity,
    },
    Line {
        start: VectorQuantity,
        end: VectorQuantity,
    },
    /// A circular area of the given diameter.
    Area {
        center: VectorQuantity,
        diameter: Quantity,
    },
}

impl DrawingDatumTarget {
    /// The target's name, such as `A1`.
    pub fn name(&self, feature: &DrawingDatumFeature) -> String {
        format!("{}{}", feature.label, self.index)
    }

    /// Basic points defining the target (the point; the line's ends; the
    /// area's center) in instance coordinates, mm.
    fn local_points(&self) -> Result<Vec<Vec3>, ModelError> {
        let point = |v: &VectorQuantity| -> Result<Vec3, ModelError> {
            let p = v.normalized(Dimension::Length)?;
            if [p.x, p.y, p.z]
                .iter()
                .all(|c| c.is_finite() && c.abs() <= 1e9)
            {
                Ok(p)
            } else {
                Err(ModelError::new("datum target coordinates must be finite"))
            }
        };
        Ok(match &self.shape {
            DatumTargetShape::Point { at } => vec![point(at)?],
            DatumTargetShape::Line { start, end } => vec![point(start)?, point(end)?],
            DatumTargetShape::Area { center, .. } => vec![point(center)?],
        })
    }

    /// An area's diameter in mm, positive and finite.
    fn diameter(&self) -> Result<Option<f64>, ModelError> {
        match &self.shape {
            DatumTargetShape::Area { diameter, .. } => {
                let d = diameter.normalized()?;
                if d.is_finite() && d > 0.0 {
                    Ok(Some(d))
                } else {
                    Err(ModelError::new(
                        "a datum target area needs a positive finite diameter",
                    ))
                }
            }
            _ => Ok(None),
        }
    }
}

/// The nominal datum plane (origin, unit normal) of a feature with targets.
fn target_plane(
    feature: &DrawingDatumFeature,
    graph: &InstanceGraph<'_>,
) -> Result<(Vec3, Vec3), ModelError> {
    match graph.datum(
        &feature.attachment.anchor.instance,
        &feature.attachment.anchor.datum,
    )? {
        ResolvedDatum::Plane { origin, normal } => Ok((origin, unit(normal)?)),
        _ => Err(ModelError::new(
            "datum targets are supported on planar datum features",
        )),
    }
}

/// Checks a feature's targets: unique indexes, finite geometry on the
/// nominal datum plane, distinct line ends, positive area sizes and paper
/// offsets of at least 8 mm. O(targets).
pub(super) fn validate(
    feature: &DrawingDatumFeature,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    if feature.targets.is_empty() {
        return Ok(());
    }
    if feature.targets.len() > MAX_DATUM_TARGETS {
        return Err(ModelError::new(
            "a datum feature has at most 100 datum targets",
        ));
    }
    if feature.feature_of_size {
        return Err(ModelError::new(
            "datum targets are supported on planar datum features, not features of size",
        ));
    }
    let (origin, normal) = target_plane(feature, graph)?;
    let mut seen = HashSet::new();
    for target in &feature.targets {
        if !(1..=99).contains(&target.index) || !seen.insert(target.index) {
            return Err(ModelError::new(
                "datum targets need unique indexes from 1 to 99 within their feature",
            ));
        }
        let offset = target.offset_mm[0].hypot(target.offset_mm[1]);
        if !offset.is_finite() || offset < 8.0 {
            return Err(ModelError::new(
                "a datum target symbol needs a finite paper offset at least 8 mm long",
            ));
        }
        target.diameter()?;
        let points = target
            .local_points()?
            .into_iter()
            .map(|p| graph.instance_point(&feature.attachment.anchor.instance, p))
            .collect::<Result<Vec<_>, _>>()?;
        for &p in &points {
            let off = dot(subtract(p, origin), normal);
            if off.abs() > 1e-6 * (1.0 + length(subtract(p, origin))) {
                return Err(ModelError::new(format!(
                    "datum target {} lies off its feature's datum plane",
                    target.name(feature)
                )));
            }
        }
        if let [start, end] = points.as_slice()
            && length(subtract(*end, *start)) < 1e-9
        {
            return Err(ModelError::new("a datum target line needs distinct ends"));
        }
    }
    Ok(())
}

/// Draws a feature's targets in its view: an × at a point, a phantom line,
/// or a hatched circle for an area, each with its divided-circle symbol and
/// a leader. O(targets).
pub(super) fn draw(
    feature: &DrawingDatumFeature,
    view: &DrawingView,
    graph: &InstanceGraph<'_>,
    d: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    if feature.targets.is_empty() {
        return Ok(());
    }
    let (_, normal) = target_plane(feature, graph)?;
    let (u, v) = {
        let seed = if normal.x.abs() < 0.9 {
            Vec3::new(1.0, 0.0, 0.0)
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };
        let u = unit(cross(normal, seed))?;
        (u, cross(normal, u))
    };
    let instance = &feature.attachment.anchor.instance;
    let paper = |p: Vec3| -> Result<[f64; 2], ModelError> { view.paper(view.project(p)?) };
    for target in &feature.targets {
        let points = target
            .local_points()?
            .into_iter()
            .map(|p| graph.instance_point(instance, p))
            .collect::<Result<Vec<_>, _>>()?;
        let at = match (&target.shape, points.as_slice()) {
            (DatumTargetShape::Point { .. }, [p]) => {
                let q = paper(*p)?;
                line(d, vec![[q[0] - 1.5, q[1] - 1.5], [q[0] + 1.5, q[1] + 1.5]]);
                line(d, vec![[q[0] - 1.5, q[1] + 1.5], [q[0] + 1.5, q[1] - 1.5]]);
                q
            }
            (DatumTargetShape::Line { .. }, [a, b]) => {
                let (a, b) = (paper(*a)?, paper(*b)?);
                phantom(d, a, b);
                [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
            }
            (DatumTargetShape::Area { .. }, [c]) => {
                let r = target.diameter()?.unwrap_or(0.0) / 2.0;
                let ring = (0..=48)
                    .map(|i| {
                        let a = std::f64::consts::TAU * i as f64 / 48.0;
                        paper(add(*c, add(scale(u, r * a.cos()), scale(v, r * a.sin()))))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                line(d, ring);
                // Hatching: chords along u + v, a fifth of the diameter apart.
                let (w, across) = (unit(add(u, v))?, unit(subtract(v, u))?);
                for k in -2..=2 {
                    let offset = 0.4 * r * k as f64;
                    let half = (r * r - offset * offset).max(0.0).sqrt();
                    let middle = add(*c, scale(across, offset));
                    line(
                        d,
                        vec![
                            paper(subtract(middle, scale(w, half)))?,
                            paper(add(middle, scale(w, half)))?,
                        ],
                    );
                }
                paper(*c)?
            }
            _ => unreachable!("each shape has its defining points"),
        };
        let center = [at[0] + target.offset_mm[0], at[1] + target.offset_mm[1]];
        symbol(feature, target, at, center, d)?;
    }
    Ok(())
}

/// The divided circle at `center`: the target's name below the line, an
/// area's diameter above it, and a plain leader to the target at `at`.
fn symbol(
    feature: &DrawingDatumFeature,
    target: &DrawingDatumTarget,
    at: [f64; 2],
    center: [f64; 2],
    d: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let r = SYMBOL_RADIUS_MM;
    circle(d, center, r, 0.0, std::f64::consts::TAU);
    line(
        d,
        vec![[center[0] - r, center[1]], [center[0] + r, center[1]]],
    );
    let name = target.name(feature);
    text(
        d,
        [center[0] - name.len() as f64, center[1] - r * 0.75],
        name,
    );
    if let Some(diameter) = target.diameter()? {
        let size = format!("Ø{}", trim_number(diameter));
        text(
            d,
            [
                center[0] - size.chars().count() as f64 * 0.9,
                center[1] + r * 0.2,
            ],
            size,
        );
    }
    let delta = [at[0] - center[0], at[1] - center[1]];
    let n = delta[0].hypot(delta[1]);
    line(
        d,
        vec![
            [center[0] + r * delta[0] / n, center[1] + r * delta[1] / n],
            at,
        ],
    );
    Ok(())
}

/// Cycles of a phantom line (a long dash, then two short ones), closed by a
/// final long dash so both ends show.
const PHANTOM_CYCLES: usize = 5;

/// A phantom line in a fixed pattern scaled to its length, so its vertex
/// count does not depend on the geometry.
fn phantom(d: &mut GeneratedDrawing, a: [f64; 2], b: [f64; 2]) {
    let at = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    // Cycle units: long 6, gap 1, short 1, gap 1, short 1, gap 1; then 6.
    let unit = 1.0 / (11.0 * PHANTOM_CYCLES as f64 + 6.0);
    let mut t = 0.0;
    for _ in 0..PHANTOM_CYCLES {
        for (dash, gap) in [(6.0, 1.0), (1.0, 1.0), (1.0, 1.0)] {
            line(d, vec![at(t), at(t + dash * unit)]);
            t += (dash + gap) * unit;
        }
    }
    line(d, vec![at(t), at(1.0)]);
}

/// Paper vertices of a feature's targets: the symbol (33-point circle,
/// divider, leader) plus a point's ×, a phantom line's dashes or an area's
/// ring and five hatch chords. O(targets).
pub fn vertex_count(feature: &DrawingDatumFeature) -> usize {
    feature
        .targets
        .iter()
        .map(|t| {
            37 + match t.shape {
                DatumTargetShape::Point { .. } => 4,
                DatumTargetShape::Line { .. } => 2 * (3 * PHANTOM_CYCLES + 1),
                DatumTargetShape::Area { .. } => 49 + 10,
            }
        })
        .sum()
}

fn trim_number(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
