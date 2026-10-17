//! Straightness (ASME Y14.5-2018 §5.4.1) of measured points, in its two
//! forms:
//!
//! - **Line elements of a surface** (characteristic zone): each line element
//!   must lie between two parallel lines the tolerance apart, oriented freely
//!   in its section. A planar control's elements are sections normal to its
//!   attachment view direction, the view showing the surface edge-on (as for a
//!   profile of a line); a cylinder's (axis anchor) are its generator lines,
//!   points grouped by angular position about the nominal axis, split at
//!   angular gaps wider than a quarter of the widest gap (so a slightly
//!   shifted or tilted part keeps each line together). Each element is
//!   projected onto the plane through the nominal axis and its mean radial
//!   direction; projection keeps a straight generator straight, so axis
//!   misalignment does not add to the result.
//! - **Derived median line** (Ø zone on a cylindrical feature of size): the
//!   centers of the cross sections normal to the nominal axis must lie in a
//!   cylinder of the tolerance diameter, its axis free to shift and tilt.
//!   Each section's center and actual local size come from its actual mating
//!   circle (smallest circumscribed for a shaft, largest inscribed for a
//!   hole). At MMC or LMC the bonus is the departure from that material
//!   condition of the section nearest to it, and every local size must lie
//!   within the size limits; Rule #1 does not apply, so the feature may
//!   exceed its maximum-material envelope.
//!
//! Points group into elements or sections where consecutive keys differ by
//! more than 1e-3 mm or 1e-6 of the key span. Widths are exact for the
//! reported orientation; the median-line zone is the optimum of a linear
//! program over a circumscribed 64-gon, recomputed exactly for that axis, so
//! it can overstate the minimum by at most 0.13% and never understates it.
use super::*;

/// Sides of the polygon standing in for the median-line zone's circle.
const SIDES: usize = 64;

impl Context<'_, '_> {
    pub(super) fn straightness(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
        tolerance: f64,
    ) -> Result<ControlMeasurement, String> {
        let nominal = self.nominal(&frame.attachment.anchor)?;
        if frame.zone == GeometricToleranceZone::Diameter {
            let ResolvedDatum::Axis { origin, direction } = nominal else {
                return Err("straightness of a derived median line needs an axis anchor".into());
            };
            return median_line(frame, points, tolerance, origin, direction);
        }
        if frame.material != ToleranceMaterialCondition::Regardless {
            return Err("straightness of surface line elements is evaluated regardless of feature size; a material condition needs a diameter zone".into());
        }
        let elements = match nominal {
            ResolvedDatum::Plane { normal, .. } => {
                let view = self
                    .views
                    .get(frame.attachment.view.as_str())
                    .ok_or("straightness of a planar surface needs its attachment view")?;
                let w = unit(view.frame().map_err(|e| e.to_string())?.direction)
                    .map_err(|e| e.to_string())?;
                let n = unit(normal).map_err(|e| e.to_string())?;
                if dot(w, n).abs() > 1e-6 {
                    return Err(
                        "straightness of a planar surface needs a view showing it edge-on".into(),
                    );
                }
                let along = cross(n, w);
                planar_elements(points, w, along, n)
            }
            ResolvedDatum::Axis { origin, direction } => {
                let axis = unit(direction).map_err(|e| e.to_string())?;
                generator_elements(points, origin, axis)
            }
            _ => return Err("straightness needs a planar or axis anchor".into()),
        };
        let mut worst: f64 = 0.0;
        for element in &elements {
            if element.len() < 2 {
                return Err("each straightness line element needs at least two points".into());
            }
            worst = worst.max(strip_width(element));
        }
        Ok(form_measurement(worst, tolerance, 0.0, None, None))
    }
}

fn form_measurement(
    deviation: f64,
    tolerance: f64,
    bonus: f64,
    actual_size: Option<f64>,
    size_conforms: Option<bool>,
) -> ControlMeasurement {
    ControlMeasurement {
        deviation_mm: deviation,
        tolerance_mm: tolerance,
        bonus_mm: bonus,
        actual_size_mm: actual_size,
        size_conforms,
        conforms: size_conforms.unwrap_or(true) && deviation <= tolerance + bonus,
        datum_shift_mm: 0.0,
        datum_shift_rotation_rad: 0.0,
        datum_shift_limit_mm: 0.0,
        refinement: None,
        profile: None,
        uncertainty_mm: 0.0,
        indeterminate: false,
    }
}

/// Splits `(key, item)` pairs, sorted by key, where consecutive keys differ by
/// more than 1e-3 mm or 1e-6 of the span.
fn split<T: Copy>(mut keyed: Vec<(f64, T)>) -> Vec<Vec<T>> {
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let span = keyed.last().map_or(0.0, |k| k.0) - keyed.first().map_or(0.0, |k| k.0);
    let gap = (1e-6 * span).max(1e-3);
    let mut out: Vec<Vec<T>> = Vec::new();
    for (i, &(key, item)) in keyed.iter().enumerate() {
        if i == 0 || key - keyed[i - 1].0 > gap {
            out.push(Vec::new());
        }
        out.last_mut().expect("pushed").push(item);
    }
    out
}

/// Line elements of a planar surface: sections normal to view direction `w`,
/// each as (position along the surface, height along its normal).
fn planar_elements(points: &[Vec3], w: Vec3, along: Vec3, n: Vec3) -> Vec<Vec<[f64; 2]>> {
    split(
        points
            .iter()
            .map(|&p| (dot(p, w), [dot(p, along), dot(p, n)]))
            .collect(),
    )
}

/// Generator lines of a cylinder about the nominal axis, each projected
/// onto its own plane through the axis as (position along the axis, offset
/// along the element's mean radial direction). O(n log n).
fn generator_elements(points: &[Vec3], origin: Vec3, axis: Vec3) -> Vec<Vec<[f64; 2]>> {
    use std::f64::consts::TAU;
    let (u, v) = perpendiculars(axis);
    let mut placed: Vec<(f64, Vec3)> = points
        .iter()
        .map(|&p| {
            let d = subtract(p, origin);
            (dot(d, v).atan2(dot(d, u)).rem_euclid(TAU), d)
        })
        .collect();
    if placed.is_empty() {
        return Vec::new();
    }
    placed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let gaps: Vec<f64> = (0..placed.len())
        .map(|i| {
            let next = placed.get(i + 1).map_or(placed[0].0 + TAU, |p| p.0);
            next - placed[i].0
        })
        .collect();
    let widest = gaps.iter().copied().fold(0.0, f64::max);
    let radius = placed
        .iter()
        .map(|(_, d)| length(subtract(*d, scale(axis, dot(*d, axis)))))
        .fold(0.0, f64::max)
        .max(1e-12);
    let threshold = (0.25 * widest).max(1e-3 / radius);
    // Start after the widest gap so no element straddles the seam.
    let first = (gaps.iter().position(|&g| g == widest).unwrap_or(0) + 1) % placed.len();
    let mut elements: Vec<Vec<Vec3>> = Vec::new();
    for k in 0..placed.len() {
        let i = (first + k) % placed.len();
        let previous = (i + placed.len() - 1) % placed.len();
        if k == 0 || gaps[previous] > threshold {
            elements.push(Vec::new());
        }
        elements.last_mut().expect("pushed").push(placed[i].1);
    }
    elements
        .into_iter()
        .map(|element| {
            let sum = element.iter().fold(Vec3::new(0.0, 0.0, 0.0), |s, &d| {
                let r = subtract(d, scale(axis, dot(d, axis)));
                let l = length(r);
                if l > 0.0 {
                    add(s, scale(r, 1.0 / l))
                } else {
                    s
                }
            });
            let radial = if length(sum) > 0.0 {
                scale(sum, 1.0 / length(sum))
            } else {
                u
            };
            element
                .iter()
                .map(|&d| [dot(d, axis), dot(d, radial)])
                .collect()
        })
        .collect()
}

/// Width of the narrowest strip holding planar points, its direction free:
/// the smallest hull-edge height (rotating calipers). Exact; O(n log n).
fn strip_width(points: &[[f64; 2]]) -> f64 {
    let hull = convex_hull(points);
    if hull.len() < 3 {
        return 0.0;
    }
    let m = hull.len();
    let height = |i: usize, j: usize| {
        let (a, b, p) = (hull[i], hull[(i + 1) % m], hull[j]);
        let e = [b[0] - a[0], b[1] - a[1]];
        ((e[0] * (p[1] - a[1]) - e[1] * (p[0] - a[0])) / e[0].hypot(e[1])).abs()
    };
    let mut best = f64::INFINITY;
    let mut j = 1;
    for i in 0..m {
        while height(i, (j + 1) % m) >= height(i, j) {
            j = (j + 1) % m;
            if j == i {
                break;
            }
        }
        best = best.min(height(i, j));
    }
    best
}

/// Counterclockwise convex hull without collinear points (monotone chain).
fn convex_hull(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    sorted.dedup();
    if sorted.len() < 3 {
        return sorted;
    }
    let turn = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut hull: Vec<[f64; 2]> = Vec::with_capacity(2 * sorted.len());
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &[f64; 2]>> = if pass == 0 {
            Box::new(sorted.iter())
        } else {
            Box::new(sorted.iter().rev())
        };
        for &p in ordered {
            while hull.len() >= start + 2
                && turn(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Straightness of the derived median line of a cylindrical feature about
/// the nominal axis `(origin, direction)`.
fn median_line(
    frame: &DrawingFeatureControlFrame,
    points: &[Vec3],
    tolerance: f64,
    origin: Vec3,
    direction: Vec3,
) -> Result<ControlMeasurement, String> {
    let axis = unit(direction).map_err(|e| e.to_string())?;
    let (u, v) = perpendiculars(axis);
    let limits = frame.size_limits.as_ref();
    if frame.material != ToleranceMaterialCondition::Regardless && limits.is_none() {
        return Err("straightness at a material condition needs size limits".into());
    }
    let internal = limits.is_some_and(|l| l.kind == FeatureOfSizeKind::Internal);
    let (mating, minimum_material) = if internal {
        (Fit::Inscribed, Fit::Circumscribed)
    } else {
        (Fit::Circumscribed, Fit::Inscribed)
    };
    let sections = split(
        points
            .iter()
            .map(|&p| {
                let d = subtract(p, origin);
                let along = dot(d, axis);
                (along, (along, [dot(d, u), dot(d, v)]))
            })
            .collect(),
    );
    if sections.len() < 2 {
        return Err(
            "straightness of a derived median line needs at least two cross sections".into(),
        );
    }
    let mut centers: Vec<(f64, [f64; 2])> = Vec::with_capacity(sections.len());
    let (mut largest, mut smallest) = (f64::NEG_INFINITY, f64::INFINITY);
    let (mut largest_minimum, mut smallest_minimum) = (f64::NEG_INFINITY, f64::INFINITY);
    for section in &sections {
        if section.len() < 4 {
            return Err("each derived median line cross section needs at least four points".into());
        }
        let at = section.iter().map(|s| s.0).sum::<f64>() / section.len() as f64;
        let flat: Vec<[f64; 2]> = section.iter().map(|s| s.1).collect();
        let (center, radius) = fit_circle(&flat, mating).map_err(|e| e.to_string())?;
        let size = 2.0 * radius;
        (largest, smallest) = (largest.max(size), smallest.min(size));
        if limits.is_some() {
            let (_, r) = fit_circle(&flat, minimum_material).map_err(|e| e.to_string())?;
            (largest_minimum, smallest_minimum) =
                (largest_minimum.max(2.0 * r), smallest_minimum.min(2.0 * r));
        }
        centers.push((at, center));
    }
    let deviation = enclosing_line(&centers)?;
    let (mut bonus, mut actual, mut conforms) = (0.0, None, None);
    if let Some(limits) = limits {
        let (lower, upper) = limits.bounds().map_err(|e| e.to_string())?;
        // Local sizes: the mating side must not pass the maximum-material
        // limit, the minimum-material side the least-material limit.
        conforms = Some(if internal {
            smallest >= lower && largest_minimum <= upper
        } else {
            largest <= upper && smallest_minimum >= lower
        });
        actual = Some(if internal { smallest } else { largest });
        bonus = match frame.material {
            ToleranceMaterialCondition::Regardless => 0.0,
            ToleranceMaterialCondition::Maximum => {
                if internal {
                    smallest - lower
                } else {
                    upper - largest
                }
            }
            ToleranceMaterialCondition::Least => {
                if internal {
                    upper - largest_minimum
                } else {
                    smallest_minimum - lower
                }
            }
        }
        .max(0.0);
    }
    Ok(form_measurement(
        deviation, tolerance, bonus, actual, conforms,
    ))
}

/// Diameter of the thinnest cylinder holding section centers `(position
/// along the nominal axis, offset across it)`, its axis free to shift and
/// tilt. The line `c(z) = p + a·z` minimizing the largest offset over a
/// circumscribed polygon is found by one linear program; the reported
/// diameter is exact for that line. O(pivots · sections · 64).
fn enclosing_line(centers: &[(f64, [f64; 2])]) -> Result<f64, String> {
    let n = centers.len() as f64;
    let mean_z = centers.iter().map(|c| c.0).sum::<f64>() / n;
    let mean = [
        centers.iter().map(|c| c.1[0]).sum::<f64>() / n,
        centers.iter().map(|c| c.1[1]).sum::<f64>() / n,
    ];
    let spread = centers
        .iter()
        .map(|c| (c.1[0] - mean[0]).hypot(c.1[1] - mean[1]))
        .fold(0.0, f64::max);
    if spread <= 1e-15 {
        return Ok(0.0);
    }
    let length = centers
        .iter()
        .map(|c| (c.0 - mean_z).abs())
        .fold(0.0, f64::max)
        .max(1e-12);
    // Unit-scale data keeps the program well conditioned.
    let q: Vec<(f64, [f64; 2])> = centers
        .iter()
        .map(|&(z, c)| {
            (
                (z - mean_z) / length,
                [(c[0] - mean[0]) / spread, (c[1] - mean[1]) / spread],
            )
        })
        .collect();
    let mut rows = Vec::with_capacity(q.len() * SIDES * 5);
    let mut bounds = Vec::with_capacity(q.len() * SIDES);
    for &(z, c) in &q {
        for k in 0..SIDES {
            let angle = 2.0 * std::f64::consts::PI * k as f64 / SIDES as f64;
            let e = [angle.cos(), angle.sin()];
            // e·(c - p - a·z) ≤ r
            rows.extend([-e[0], -e[1], -e[0] * z, -e[1] * z, -1.0]);
            bounds.push(-(e[0] * c[0] + e[1] * c[1]));
        }
    }
    let x =
        fit::maximize(&[0.0, 0.0, 0.0, 0.0, -1.0], &rows, &bounds).map_err(|e| e.to_string())?;
    // Exact distances from the 3D line through p along (a·spread/length, 1).
    let slope = [x[2] * spread / length, x[3] * spread / length];
    let norm = (1.0 + slope[0] * slope[0] + slope[1] * slope[1]).sqrt();
    let radius = centers
        .iter()
        .map(|&(z, c)| {
            let t = z - mean_z;
            let d = [
                c[0] - mean[0] - x[0] * spread - slope[0] * t,
                c[1] - mean[1] - x[1] * spread - slope[1] * t,
                0.0,
            ];
            // Remove the component along the line direction (slope, 1)/norm.
            let along = (d[0] * slope[0] + d[1] * slope[1]) / norm;
            let r = [
                d[0] - along * slope[0] / norm,
                d[1] - along * slope[1] / norm,
                -along / norm,
            ];
            (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
        })
        .fold(0.0, f64::max);
    Ok(2.0 * radius)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_width_is_the_narrowest_band_in_any_direction() {
        // A thin parallelogram tilted 30°: the band is its height, not its
        // vertical extent.
        let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
        let points: Vec<[f64; 2]> = [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 0.2],
            [0.0, 0.2],
            [5.0, 0.05],
            [3.0, 0.2],
        ]
        .iter()
        .map(|p| [p[0] * c - p[1] * s, p[0] * s + p[1] * c])
        .collect();
        assert!((strip_width(&points) - 0.2).abs() < 1e-12);
        assert_eq!(strip_width(&[[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]]), 0.0);
    }

    #[test]
    fn the_median_line_zone_follows_a_tilted_bow() {
        // Centers on a tilted line, the middle one bowed 0.05 off it: the
        // thinnest cylinder has diameter 0.05 (it may tilt to the line).
        let centers: Vec<(f64, [f64; 2])> = (0..=10)
            .map(|i| {
                let z = i as f64 * 5.0;
                let bow = if i == 5 { 0.05 } else { 0.0 };
                (z, [0.01 * z + bow, -0.02 * z])
            })
            .collect();
        let d = enclosing_line(&centers).unwrap();
        assert!((0.05 - 1e-9..0.05 * 1.0013).contains(&d), "{d}");
        let straight: Vec<(f64, [f64; 2])> =
            (0..5).map(|i| (i as f64, [0.3 * i as f64, 1.0])).collect();
        assert!(enclosing_line(&straight).unwrap() < 1e-9);
    }
}
