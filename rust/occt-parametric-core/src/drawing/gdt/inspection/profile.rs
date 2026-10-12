//! Profile of a surface (schema 139) and of a line: measured points against
//! their nominal surface points and outward normals, inside a zone located by
//! the datum reference frame and disposed about the true profile. A line
//! profile applies the same fit to each line element (a section normal to the
//! control's view direction) on its own, moving only within that section.
//!
//! The nominal geometry is first placed on the measured datum simulators;
//! the rigid motions those datums leave free (all six without datums) are
//! then fitted by minimizing the largest deviation from the zone center
//! (Chebyshev), relinearized about each step. Controls sharing a pattern
//! move as one rigid group (simultaneous requirement). A composite profile's
//! lower segment repeats the fit with its datums constraining orientation
//! only. Deviations are measured along each nominal normal from its nominal
//! point (the coordinate-measuring convention), so they are exact for planes
//! and first-order elsewhere.
use super::*;
use fit::{Fit, chebyshev, rotate};

const MAX_LINEARIZATIONS: usize = 16;
/// Points solved exactly at once; larger sets use an active set.
const ACTIVE_SET: usize = 512;

/// Nominal geometry placed on the measured datums, and the rigid motions of
/// the measured points that the datums leave free.
#[derive(Clone)]
struct Alignment {
    /// Images of the model x, y and z axes.
    rotation: [Vec3; 3],
    translation: Vec3,
    /// Free unit translation directions and rotation axes.
    translations: Vec<Vec3>,
    rotations: Vec<Vec3>,
    /// A point on a free rotation axis that a datum fixes, if any.
    pivot: Option<Vec3>,
}
impl Alignment {
    /// The free motions that keep a section plane normal to unit `normal` in
    /// itself: free translations within the plane, and rotation about the
    /// normal when that rotation is free.
    fn in_plane(&self, normal: Vec3) -> Self {
        // The free translation nearest the normal leaves; the rest are kept.
        let along = self
            .translations
            .iter()
            .fold(Vec3::new(0.0, 0.0, 0.0), |w, &t| {
                add(w, scale(t, dot(t, normal)))
            });
        let mut translations: Vec<Vec3> = Vec::new();
        for &t in &self.translations {
            let mut q = t;
            if length(along) > 1e-9 {
                let w = scale(along, 1.0 / length(along));
                q = subtract(q, scale(w, dot(q, w)));
            }
            for &r in &translations {
                q = subtract(q, scale(r, dot(q, r)));
            }
            if length(q) > 1e-6 {
                translations.push(scale(q, 1.0 / length(q)));
            }
        }
        let rest = self
            .rotations
            .iter()
            .fold(normal, |n, &r| subtract(n, scale(r, dot(n, r))));
        let rotations = if !self.rotations.is_empty() && length(rest) < 1e-9 {
            vec![normal]
        } else {
            Vec::new()
        };
        Self {
            translations,
            rotations,
            ..self.clone()
        }
    }

    fn turn(&self, v: Vec3) -> Vec3 {
        add(
            add(scale(self.rotation[0], v.x), scale(self.rotation[1], v.y)),
            scale(self.rotation[2], v.z),
        )
    }
    fn place(&self, (point, normal): (Vec3, Vec3)) -> (Vec3, Vec3) {
        (add(self.turn(point), self.translation), self.turn(normal))
    }
}

/// Result of moving one group's measured points within their free motions.
struct ProfileFit {
    /// Signed deviation of each point along its nominal normal.
    deviations: Vec<f64>,
    translation: f64,
    rotation: f64,
    elements: usize,
}

/// Each member control's ID and measurement (or reason it was not evaluated).
type MemberResults = Vec<(String, Result<ControlMeasurement, String>)>;

/// One control in a profile group: its points' range in the group and the
/// center of its zone along the outward normal.
struct Member<'f> {
    frame: &'f DrawingFeatureControlFrame,
    range: std::ops::Range<usize>,
    tolerance: f64,
    center: f64,
}

impl Context<'_, '_> {
    /// Evaluates the controls of one profile pattern (or one control) and
    /// records each member's measurement. Errors are recorded per member.
    pub(super) fn profile_group(
        &mut self,
        group: &[&DrawingFeatureControlFrame],
        measured: &HashMap<&str, Vec<Vec3>>,
        nominals: &Nominals<'_>,
    ) {
        match self.fit_profile_group(group, measured, nominals) {
            Ok(results) => self.profiles.extend(results),
            Err(reason) => {
                for frame in group {
                    self.profiles.insert(frame.id.clone(), Err(reason.clone()));
                }
            }
        }
    }

    fn fit_profile_group(
        &mut self,
        group: &[&DrawingFeatureControlFrame],
        measured: &HashMap<&str, Vec<Vec3>>,
        nominals: &Nominals<'_>,
    ) -> Result<MemberResults, String> {
        let first = group[0];
        let mut points = Vec::new();
        let mut targets = Vec::new();
        let mut members = Vec::with_capacity(group.len());
        for &frame in group {
            let measured = &measured[frame.id.as_str()];
            let start = points.len();
            points.extend_from_slice(measured);
            match nominals.get(frame.id.as_str()) {
                Some(n) => targets.extend_from_slice(n),
                None => {
                    let (origin, normal) = plane_datum(
                        self.nominal(&frame.attachment.anchor)?,
                        "profile without nominal points",
                    )?;
                    targets.extend(measured.iter().map(|&p| {
                        (
                            subtract(p, scale(normal, dot(subtract(p, origin), normal))),
                            normal,
                        )
                    }));
                }
            }
            let tolerance = frame.tolerance.normalized().map_err(|e| e.to_string())?;
            let outward = match &frame.unequal {
                Some(u) => u.normalized().map_err(|e| e.to_string())?,
                None => 0.5 * tolerance,
            };
            members.push(Member {
                frame,
                range: start..points.len(),
                tolerance,
                center: outward - 0.5 * tolerance,
            });
        }
        if points.len() < 3 {
            return Err("profile evaluation needs at least three measured points".into());
        }
        let centers: Vec<f64> = members
            .iter()
            .flat_map(|m| std::iter::repeat_n(m.center, m.range.len()))
            .collect();
        let upper = self.profile_alignment(&first.datums, false)?;
        let upper_fit = self.fit_members(&members, &points, &targets, &centers, &upper)?;
        let lower = match &first.refinement {
            None => None,
            Some(refinement) => {
                let tolerance = refinement
                    .tolerance
                    .normalized()
                    .map_err(|e| e.to_string())?;
                let alignment = self.profile_alignment(&refinement.datums, true)?;
                let zero = vec![0.0; points.len()];
                Some((
                    tolerance,
                    self.fit_members(&members, &points, &targets, &zero, &alignment)?,
                ))
            }
        };
        Ok(members
            .iter()
            .map(|m| {
                let span = &upper_fit.deviations[m.range.clone()];
                let deviation = zone_width(span, m.center);
                let refinement = lower.as_ref().map(|(tolerance, fit)| {
                    let deviation = zone_width(&fit.deviations[m.range.clone()], 0.0);
                    RefinementMeasurement {
                        deviation_mm: deviation,
                        tolerance_mm: *tolerance,
                        bonus_mm: 0.0,
                        conforms: deviation <= *tolerance,
                        pattern_translation_mm: fit.translation,
                        pattern_rotation_rad: fit.rotation,
                    }
                });
                let conforms =
                    deviation <= m.tolerance && refinement.as_ref().is_none_or(|r| r.conforms);
                (
                    m.frame.id.clone(),
                    Ok(ControlMeasurement {
                        deviation_mm: deviation,
                        tolerance_mm: m.tolerance,
                        bonus_mm: 0.0,
                        actual_size_mm: None,
                        size_conforms: None,
                        conforms,
                        datum_shift_mm: 0.0,
                        datum_shift_rotation_rad: 0.0,
                        datum_shift_limit_mm: 0.0,
                        refinement,
                        profile: Some(ProfileMeasurement {
                            min_deviation_mm: span.iter().copied().fold(f64::INFINITY, f64::min),
                            max_deviation_mm: span
                                .iter()
                                .copied()
                                .fold(f64::NEG_INFINITY, f64::max),
                            zone_center_mm: m.center,
                            translation_mm: upper_fit.translation,
                            rotation_rad: upper_fit.rotation,
                            elements: upper_fit.elements,
                        }),
                        uncertainty_mm: 0.0,
                        indeterminate: false,
                    }),
                )
            })
            .collect())
    }

    /// One segment's fit of a group. A surface profile moves the group as
    /// one rigid body. A line profile fits each member's line elements on
    /// their own: points sharing a position along the member's view direction
    /// (within 1e-3 mm or 1e-6 of the span), each moving only within its
    /// section plane, so patterns do not couple line elements.
    fn fit_members(
        &self,
        members: &[Member<'_>],
        points: &[Vec3],
        targets: &[(Vec3, Vec3)],
        centers: &[f64],
        alignment: &Alignment,
    ) -> Result<ProfileFit, String> {
        if members[0].frame.characteristic != GeometricCharacteristic::ProfileLine {
            return fit_profile(points, targets, centers, alignment);
        }
        let mut fit = ProfileFit {
            deviations: vec![0.0; points.len()],
            translation: 0.0,
            rotation: 0.0,
            elements: 0,
        };
        for member in members {
            let view = self
                .views
                .get(member.frame.attachment.view.as_str())
                .ok_or("profile of a line needs its attachment view")?;
            let normal = view.frame().map_err(|e| e.to_string())?.direction;
            let restricted = alignment.in_plane(normal);
            let mut order: Vec<(f64, usize)> = member
                .range
                .clone()
                .map(|i| (dot(points[i], normal), i))
                .collect();
            order.sort_by(|a, b| a.0.total_cmp(&b.0));
            let span = order.last().map_or(0.0, |o| o.0) - order.first().map_or(0.0, |o| o.0);
            let gap = (1e-6 * span).max(1e-3);
            let mut start = 0;
            for end in 1..=order.len() {
                if end < order.len() && order[end].0 - order[end - 1].0 <= gap {
                    continue;
                }
                let element: Vec<usize> = order[start..end].iter().map(|o| o.1).collect();
                start = end;
                if element.len() < 2 {
                    return Err("each profile line element needs at least two points".into());
                }
                let element_fit = fit_profile(
                    &element.iter().map(|&i| points[i]).collect::<Vec<_>>(),
                    &element.iter().map(|&i| targets[i]).collect::<Vec<_>>(),
                    &element.iter().map(|&i| centers[i]).collect::<Vec<_>>(),
                    &restricted,
                )?;
                for (&i, &d) in element.iter().zip(&element_fit.deviations) {
                    fit.deviations[i] = d;
                }
                fit.translation = fit.translation.max(element_fit.translation);
                fit.rotation = fit.rotation.max(element_fit.rotation);
                fit.elements += 1;
            }
        }
        Ok(fit)
    }

    /// Places nominal geometry on the measured simulators of `datums` and
    /// lists the motions they leave free. The first two independent datum
    /// directions fix orientation (one fixes all but rotation about it);
    /// planes fix translation along their normals and axes across them.
    /// With `orientation_only` (a composite lower segment) every translation
    /// stays free.
    fn profile_alignment(
        &mut self,
        datums: &[DrawingDatumReference],
        orientation_only: bool,
    ) -> Result<Alignment, String> {
        if datums
            .iter()
            .any(|d| d.boundary != DatumMaterialBoundary::Regardless)
        {
            return Err("profile datums are evaluated regardless of material boundary".into());
        }
        let simulators = self.simulators(datums)?;
        let nominal = self.resolved_frame(datums, None)?;
        // (nominal point, nominal direction, measured point, measured
        // direction, is an axis) per datum.
        let mut pairs = Vec::with_capacity(datums.len());
        for (resolved, simulator) in nominal.datums.iter().zip(&simulators) {
            let (point, direction) = match resolved.geometry {
                ResolvedDatum::Plane { origin, normal } => (origin, normal),
                ResolvedDatum::Axis { origin, direction } => (origin, direction),
                _ => return Err("profile datums must be planes or axes".into()),
            };
            let direction = unit(direction).map_err(|e| e.to_string())?;
            let (measured, measured_direction, axis) = match simulator {
                Simulator::Plane(p, n, _) => (*p, *n, false),
                Simulator::Axis(p, d, ..) => (*p, *d, true),
            };
            pairs.push((point, direction, measured, measured_direction, axis));
        }
        let independent = pairs
            .iter()
            .skip(1)
            .find(|p| length(cross(p.1, pairs[0].1)) > 1e-6);
        let (rotation, rotations) = match (pairs.first(), independent) {
            (None, _) => (
                [
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                ],
                vec![
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                ],
            ),
            (Some(first), None) => (minimal_rotation(first.1, first.3), vec![first.3]),
            (Some(first), Some(second)) => {
                let from = orthonormal(first.1, second.1)?;
                let to = orthonormal(first.3, second.3)?;
                let image = |e: Vec3| {
                    add(
                        add(scale(to[0], dot(e, from[0])), scale(to[1], dot(e, from[1]))),
                        scale(to[2], dot(e, from[2])),
                    )
                };
                (
                    [
                        image(Vec3::new(1.0, 0.0, 0.0)),
                        image(Vec3::new(0.0, 1.0, 0.0)),
                        image(Vec3::new(0.0, 0.0, 1.0)),
                    ],
                    Vec::new(),
                )
            }
        };
        let mut alignment = Alignment {
            rotation,
            translation: Vec3::new(0.0, 0.0, 0.0),
            translations: Vec::new(),
            rotations,
            pivot: pairs.iter().find(|p| p.4).map(|p| p.2),
        };
        // Translation constraints as orthonormal rows `q·τ = b`.
        let mut rows: Vec<(Vec3, f64)> = Vec::new();
        for &(point, _, measured, direction, axis) in &pairs {
            let gap = subtract(measured, alignment.turn(point));
            let normals = if axis {
                let (u, v) = perpendiculars(direction);
                vec![u, v]
            } else {
                vec![direction]
            };
            for n in normals {
                let mut q = n;
                let mut b = dot(n, gap);
                for &(r, c) in &rows {
                    let along = dot(q, r);
                    q = subtract(q, scale(r, along));
                    b -= along * c;
                }
                let size = length(q);
                if size > 1e-6 {
                    rows.push((scale(q, 1.0 / size), b / size));
                }
            }
        }
        alignment.translation = rows
            .iter()
            .fold(Vec3::new(0.0, 0.0, 0.0), |t, &(q, b)| add(t, scale(q, b)));
        if orientation_only {
            rows.clear();
        }
        for e in [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ] {
            let free = rows
                .iter()
                .map(|r| r.0)
                .chain(alignment.translations.iter().copied())
                .fold(e, |q, r| subtract(q, scale(r, dot(q, r))));
            if length(free) > 1e-6 {
                alignment
                    .translations
                    .push(unit(free).map_err(|e| e.to_string())?);
            }
        }
        if orientation_only {
            alignment.pivot = None;
        }
        Ok(alignment)
    }
}

/// Width of the bilateral zone about `center` that holds `deviations`.
fn zone_width(deviations: &[f64], center: f64) -> f64 {
    2.0 * deviations
        .iter()
        .map(|d| (d - center).abs())
        .fold(0.0, f64::max)
}

/// Unit frame whose first axis is `a` and whose second lies in the plane of
/// `a` and `b`.
pub(super) fn orthonormal(a: Vec3, b: Vec3) -> Result<[Vec3; 3], String> {
    let a = unit(a).map_err(|e| e.to_string())?;
    let b = unit(subtract(b, scale(a, dot(a, b))))
        .map_err(|_| "datum directions are parallel".to_string())?;
    Ok([a, b, cross(a, b)])
}

/// Columns of the smallest rotation taking unit `from` onto unit `to`.
pub(super) fn minimal_rotation(from: Vec3, to: Vec3) -> [Vec3; 3] {
    let axis = cross(from, to);
    let sine = length(axis);
    let angle = sine.atan2(dot(from, to));
    let columns = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    if sine < 1e-15 {
        return columns;
    }
    let axis = scale(axis, 1.0 / sine);
    columns.map(|e| rotate(e, axis, angle))
}

/// Moves the measured points within the alignment's free motions to minimize
/// the largest deviation from each point's zone center, relinearizing up to
/// 16 times. O(linearizations · (pivots · active points + points)).
fn fit_profile(
    points: &[Vec3],
    targets: &[(Vec3, Vec3)],
    centers: &[f64],
    alignment: &Alignment,
) -> Result<ProfileFit, String> {
    let targets: Vec<(Vec3, Vec3)> = targets.iter().map(|&t| alignment.place(t)).collect();
    let mut moved = points.to_vec();
    let count = moved.len() as f64;
    let centroid = scale(
        moved
            .iter()
            .fold(Vec3::new(0.0, 0.0, 0.0), |s, &p| add(s, p)),
        1.0 / count,
    );
    let mut pivot = alignment.pivot.unwrap_or(centroid);
    let size = moved
        .iter()
        .map(|&p| length(subtract(p, pivot)))
        .fold(1.0, f64::max);
    let deviation = |p: Vec3, (point, normal): (Vec3, Vec3)| dot(subtract(p, point), normal);
    let mut translated = Vec3::new(0.0, 0.0, 0.0);
    // Images of the model axes under the accumulated rotation.
    let mut turned = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let k = alignment.translations.len() + alignment.rotations.len();
    for _ in 0..if k == 0 { 0 } else { MAX_LINEARIZATIONS } {
        let residuals: Vec<f64> = moved
            .iter()
            .zip(&targets)
            .zip(centers)
            .map(|((&p, &t), c)| deviation(p, t) - c)
            .collect();
        // Gradient of each residual: n·t for a translation, ((p−o)×n)·r for
        // a rotation, the latter per radian times `size` (unit-scale columns).
        let gradient = |i: usize, column: usize| -> f64 {
            let (p, n) = (moved[i], targets[i].1);
            match alignment.translations.get(column) {
                Some(&t) => dot(n, t),
                None => {
                    let r = alignment.rotations[column - alignment.translations.len()];
                    dot(cross(subtract(p, pivot), n), r) / size
                }
            }
        };
        let columns = independent_columns(moved.len(), k, &gradient);
        if columns.is_empty() {
            break;
        }
        let step = zone_fit(&residuals, &columns, &gradient)?;
        let mut delta = Vec3::new(0.0, 0.0, 0.0);
        let mut omega = Vec3::new(0.0, 0.0, 0.0);
        for (&column, &value) in columns.iter().zip(&step) {
            match alignment.translations.get(column) {
                Some(&t) => delta = add(delta, scale(t, value)),
                None => {
                    let r = alignment.rotations[column - alignment.translations.len()];
                    omega = add(omega, scale(r, value / size));
                }
            }
        }
        let angle = length(omega);
        for p in &mut moved {
            let mut q = subtract(*p, pivot);
            if angle > 0.0 {
                q = rotate(q, scale(omega, 1.0 / angle), angle);
            }
            *p = add(add(pivot, q), delta);
        }
        if angle > 0.0 {
            turned = turned.map(|e| rotate(e, scale(omega, 1.0 / angle), angle));
        }
        pivot = add(pivot, delta);
        translated = add(translated, delta);
        if length(delta) + angle * size < 1e-12 * size {
            break;
        }
    }
    let trace = turned[0].x + turned[1].y + turned[2].z;
    Ok(ProfileFit {
        deviations: moved
            .iter()
            .zip(&targets)
            .map(|(&p, &t)| deviation(p, t))
            .collect(),
        translation: length(translated),
        rotation: (0.5 * (trace - 1.0)).clamp(-1.0, 1.0).acos(),
        elements: 1,
    })
}

/// Free-motion columns whose gradients are linearly independent over the
/// points (greedy Gram-Schmidt), so the zone fit is determined: a motion
/// that moves the points along their normals by less than 1e-9 per unit
/// (RMS, columns being unit-scale) is not fitted. O(m · k²).
fn independent_columns(m: usize, k: usize, gradient: &impl Fn(usize, usize) -> f64) -> Vec<usize> {
    let mut kept: Vec<(usize, Vec<f64>)> = Vec::with_capacity(k);
    for column in 0..k {
        let mut g: Vec<f64> = (0..m).map(|i| gradient(i, column)).collect();
        let norm = g.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm == 0.0 {
            continue;
        }
        for (_, q) in &kept {
            let along: f64 = g.iter().zip(q).map(|(a, b)| a * b).sum();
            for (a, b) in g.iter_mut().zip(q) {
                *a -= along * b;
            }
        }
        let rest = g.iter().map(|v| v * v).sum::<f64>().sqrt();
        if rest > 1e-6 * norm && rest > 1e-9 * (m as f64).sqrt() {
            g.iter_mut().for_each(|v| *v /= rest);
            kept.push((column, g));
        }
    }
    kept.into_iter().map(|(c, _)| c).collect()
}

/// Step minimizing `max |r_i + g_i·p|` over the chosen columns. Large sets
/// solve a strided sample and add the worst violators until every point lies
/// within the sample's zone, falling back to the full set.
fn zone_fit(
    residuals: &[f64],
    columns: &[usize],
    gradient: &impl Fn(usize, usize) -> f64,
) -> Result<Vec<f64>, String> {
    let k = columns.len();
    let solve = |set: &[usize]| {
        let t: Vec<f64> = set.iter().map(|&i| residuals[i]).collect();
        let phi: Vec<f64> = set
            .iter()
            .flat_map(|&i| columns.iter().map(move |&c| -gradient(i, c)))
            .collect();
        chebyshev(&t, &phi, k, Fit::Zone)
    };
    let m = residuals.len();
    if m <= ACTIVE_SET {
        let all: Vec<usize> = (0..m).collect();
        return solve(&all).map_err(|e| e.to_string());
    }
    let stride = m.div_ceil(ACTIVE_SET / 2);
    let mut set: Vec<usize> = (0..m).step_by(stride).collect();
    let mut chosen = vec![false; m];
    set.iter().for_each(|&i| chosen[i] = true);
    let excess = |p: &[f64], i: usize| {
        (residuals[i]
            + columns
                .iter()
                .zip(p)
                .map(|(&c, v)| gradient(i, c) * v)
                .sum::<f64>())
        .abs()
    };
    for _ in 0..64 {
        let Ok(p) = solve(&set) else { break };
        let zone = set.iter().map(|&i| excess(&p, i)).fold(0.0, f64::max);
        let mut violators: Vec<(f64, usize)> = (0..m)
            .filter(|&i| !chosen[i])
            .map(|i| (excess(&p, i), i))
            .filter(|&(e, _)| e > zone * (1.0 + 1e-9) + 1e-12)
            .collect();
        if violators.is_empty() {
            return Ok(p);
        }
        violators.sort_by(|a, b| b.0.total_cmp(&a.0));
        for &(_, i) in violators.iter().take(64) {
            chosen[i] = true;
            set.push(i);
        }
    }
    let all: Vec<usize> = (0..m).collect();
    solve(&all).map_err(|e| e.to_string())
}

impl Context<'_, '_> {
    /// Runout of a surface of revolution about the datum axis, read along
    /// the nominal normals: the nominal geometry is placed on the measured
    /// datums (its rotation about the axis does not change a reading), and
    /// each point reads its signed distance from its nominal point, as an
    /// indicator held square to the surface. Circular elements are points
    /// whose nominal points share an axial position and then a radius (each
    /// within 1e-3 mm or 1e-6 of its span), so cones, spheres and faces all
    /// form circles however far the measured surface wanders. The
    /// datums need not fix the axial position: a shift along the axis moves
    /// every reading of a circle together. O(n log n).
    pub(super) fn surface_runout(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
        tolerance: f64,
        origin: Vec3,
        axis: Vec3,
    ) -> Result<ControlMeasurement, String> {
        let alignment = self.profile_alignment(&frame.datums, false)?;
        let targets = &self.nominals[frame.id.as_str()];
        // (nominal axial position, nominal radius, reading).
        let mut readings: Vec<(f64, f64, f64)> = points
            .iter()
            .zip(targets)
            .map(|(&p, &target)| {
                let (point, normal) = alignment.place(target);
                let d = subtract(point, origin);
                let along = dot(d, axis);
                let radius = length(subtract(d, scale(axis, along)));
                (along, radius, dot(subtract(p, point), normal))
            })
            .collect();
        let range = |values: &mut dyn Iterator<Item = f64>| {
            values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            })
        };
        let deviation = if frame.characteristic == GeometricCharacteristic::TotalRunout {
            let (lo, hi) = range(&mut readings.iter().map(|r| r.2));
            hi - lo
        } else {
            let mut worst: f64 = 0.0;
            for section in groups(&mut readings, |r| r.0) {
                for circle in groups(section, |r| r.1) {
                    if circle.len() < 3 {
                        return Err(
                            "each circular runout element needs at least three points".into()
                        );
                    }
                    let (lo, hi) = range(&mut circle.iter().map(|r| r.2));
                    worst = worst.max(hi - lo);
                }
            }
            worst
        };
        Ok(ControlMeasurement {
            deviation_mm: deviation,
            tolerance_mm: tolerance,
            bonus_mm: 0.0,
            actual_size_mm: None,
            size_conforms: None,
            conforms: deviation <= tolerance,
            datum_shift_mm: 0.0,
            datum_shift_rotation_rad: 0.0,
            datum_shift_limit_mm: 0.0,
            refinement: None,
            profile: None,
            uncertainty_mm: 0.0,
            indeterminate: false,
        })
    }
}

/// Sorts by `key` and splits where consecutive keys differ by more than
/// 1e-3 mm or 1e-6 of the key span.
fn groups<T>(items: &mut [T], key: impl Fn(&T) -> f64) -> Vec<&mut [T]> {
    items.sort_by(|a, b| key(a).total_cmp(&key(b)));
    let span = items.last().map_or(0.0, &key) - items.first().map_or(0.0, &key);
    let gap = (1e-6 * span).max(1e-3);
    let mut cuts = Vec::new();
    for i in 1..items.len() {
        if key(&items[i]) - key(&items[i - 1]) > gap {
            cuts.push(i);
        }
    }
    let mut out = Vec::with_capacity(cuts.len() + 1);
    let mut rest = items;
    let mut taken = 0;
    for cut in cuts {
        let (head, tail) = rest.split_at_mut(cut - taken);
        out.push(head);
        rest = tail;
        taken = cut;
    }
    out.push(rest);
    out
}
