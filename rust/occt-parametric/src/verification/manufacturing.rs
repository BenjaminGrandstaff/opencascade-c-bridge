//! Manufacturing requirements: wall thickness, draft, and overhang. Draft is
//! exact on faces the kernel bounds in closed form (planes, cylinders,
//! cones, and most spheres and tori); other faces, walls, and overhang are
//! screened on the output's tessellation and carry `Sampled` evidence.

use super::*;
use crate::assembly::{add, cross, dot, length, scale, subtract};
use occt_bridge::{FacePullRange, MeshTriangle};
use std::collections::BTreeMap;

pub(crate) enum Screen {
    Wall {
        minimum: Quantity,
        maximum_samples: usize,
    },
    Draft {
        pull_direction: VectorQuantity,
        minimum_radians: f64,
    },
    Overhang {
        build_direction: VectorQuantity,
        maximum_radians: f64,
    },
}

fn centroid(triangle: &MeshTriangle) -> Vec3 {
    let [a, b, c] = triangle.points;
    Vec3::new(
        (a.x + b.x + c.x) / 3.0,
        (a.y + b.y + c.y) / 3.0,
        (a.z + b.z + c.z) / 3.0,
    )
}

fn unit_normal(triangle: &MeshTriangle) -> Vec3 {
    let [a, b, c] = triangle.points;
    let normal = cross(subtract(b, a), subtract(c, a));
    scale(normal, 1.0 / length(normal))
}

fn face_witness(triangle: &MeshTriangle, points_mm: Vec<Vec3>) -> Witness {
    Witness {
        subjects: vec![format!("face {}", triangle.face_index)],
        points_mm,
    }
}

/// Tessellates once within `mesh`'s triangle budget, then runs one screen:
/// O(triangles) for draft and overhang, plus an indexed ray per wall sample
/// (at most 20,000). Triangle face indices follow `Session::subshapes`.
pub(crate) fn screen(
    session: &Session,
    id: &str,
    shape: &Shape<'_>,
    mesh: MeshSettings,
    rule: Screen,
) -> Result<VerificationResult, ModelError> {
    if let Screen::Draft {
        pull_direction,
        minimum_radians,
    } = rule
    {
        return draft(session, id, shape, mesh, pull_direction, minimum_radians);
    }
    let triangles = session.surface_mesh(shape, mesh.options()?)?;
    if triangles.is_empty() {
        return Err(ModelError::new("output has no surface to screen"));
    }
    let surface = TaggedSurfaceMesh {
        id: String::new(),
        triangles,
        face_tags: Vec::new(),
        names: Vec::new(),
        faces: Vec::new(),
    };
    let mut settings = ManufacturingSettings::default();
    let walls = matches!(rule, Screen::Wall { .. });
    match rule {
        Screen::Wall {
            minimum,
            maximum_samples,
        } => {
            settings.minimum_wall = minimum;
            settings.maximum_wall_samples = maximum_samples;
        }
        Screen::Draft { .. } => unreachable!("draft is screened above"),
        Screen::Overhang {
            build_direction,
            maximum_radians,
        } => {
            settings.build_direction = build_direction;
            settings.maximum_overhang_radians = maximum_radians;
        }
    }
    let report = surface.screen(settings, walls)?;
    let triangles = &surface.triangles;
    let facets = Evidence::Sampled {
        samples: triangles.len(),
        unresolved: 0,
    };
    let mut result = match rule {
        Screen::Wall { minimum, .. } => {
            let limit = minimum.normalized()?;
            let thinnest = report
                .wall_samples
                .iter()
                .filter_map(|sample| sample.thickness_mm.map(|value| (sample, value)))
                .min_by(|(_, a), (_, b)| a.total_cmp(b));
            let Some((sample, thickness)) = thinnest else {
                return Err(ModelError::new("no wall sample found an opposite surface"));
            };
            let thin = report
                .wall_samples
                .iter()
                .filter(|sample| sample.below_minimum)
                .count();
            let mut result = VerificationResult::exact(
                id,
                thin == 0,
                format!(
                    "{thin} of {} wall sample(s) below {limit} mm; thinnest {thickness} mm; \
                     {} unresolved",
                    report.wall_samples.len(),
                    report.unresolved_wall_samples
                ),
            )
            .measured(Measurement {
                value: thickness,
                unit: MeasurementUnit::Millimeter,
                minimum: Some(limit),
                maximum: None,
            });
            if thin != 0 {
                let triangle = &triangles[sample.triangle];
                let entry = centroid(triangle);
                let exit = add(entry, scale(unit_normal(triangle), -thickness));
                result = result.witnessed(face_witness(triangle, vec![entry, exit]));
            }
            result.evidence = Evidence::Sampled {
                samples: report.wall_samples.len(),
                unresolved: report.unresolved_wall_samples,
            };
            return Ok(result);
        }
        Screen::Draft { .. } => unreachable!("draft is screened above"),
        Screen::Overhang {
            maximum_radians, ..
        } => {
            let count = report.overhang_triangles.len();
            let result = VerificationResult::exact(
                id,
                count == 0,
                format!("{count} facet(s) overhang more than {maximum_radians} rad from vertical"),
            )
            .measured(Measurement {
                value: count as f64,
                unit: MeasurementUnit::Count,
                minimum: None,
                maximum: Some(0.0),
            });
            match report.overhang_triangles.first() {
                Some(index) => {
                    let triangle = &triangles[*index];
                    result.witnessed(face_witness(triangle, vec![centroid(triangle)]))
                }
                None => result,
            }
        }
    };
    result.evidence = facets;
    Ok(result)
}

/// Faces nearly parallel to the pull "require draft": the smallest facet
/// angle magnitude on each face must reach `minimum_radians`. Faces tilted
/// either way release from one mold half or the other, so the sign does not
/// matter, and facets normal to the pull (caps) are skipped. This does not
/// detect undercuts, which depend on the parting line.
/// Least draft of one face and where it occurs.
struct FaceDraft {
    angle: f64,
    points: Vec<Vec3>,
}

/// Projections at least this close to +-1 are pull-facing caps, not walls.
const CAP: f64 = 1.0 - 1e-8;

/// Exact least draft of a face from its pull range, or `None` for a cap.
/// A range crossing zero has a vertical line somewhere between its ends.
fn exact_draft(range: &FacePullRange) -> Option<FaceDraft> {
    let ((low, low_at), (high, high_at)) = (range.minimum, range.maximum);
    if low >= CAP || high <= -CAP {
        return None;
    }
    Some(if low <= 0.0 && high >= 0.0 {
        FaceDraft {
            angle: 0.0,
            points: if low == high {
                vec![low_at]
            } else {
                vec![low_at, high_at]
            },
        }
    } else if low > 0.0 {
        FaceDraft {
            angle: low.asin(),
            points: vec![low_at],
        }
    } else {
        FaceDraft {
            angle: (-high).asin(),
            points: vec![high_at],
        }
    })
}

/// Least facet draft of each listed face; facets nearly normal to the pull
/// are caps and skipped.
fn sampled_drafts(
    triangles: &[MeshTriangle],
    pull: Vec3,
    faces: &BTreeMap<usize, ()>,
) -> BTreeMap<usize, FaceDraft> {
    let mut drafts: BTreeMap<usize, FaceDraft> = BTreeMap::new();
    for triangle in triangles {
        if !faces.contains_key(&triangle.face_index) {
            continue;
        }
        let projection = dot(unit_normal(triangle), pull).clamp(-1.0, 1.0);
        if !projection.is_finite() || projection.abs() >= CAP {
            continue;
        }
        let angle = projection.asin().abs();
        let least = drafts.entry(triangle.face_index).or_insert(FaceDraft {
            angle: f64::INFINITY,
            points: Vec::new(),
        });
        if angle < least.angle {
            *least = FaceDraft {
                angle,
                points: vec![centroid(triangle)],
            };
        }
    }
    drafts
}

/// Exact pull ranges per face, plus one tessellation within `mesh` only when
/// some face has no exact range. O(faces + fallback triangles).
fn draft(
    session: &Session,
    id: &str,
    shape: &Shape<'_>,
    mesh: MeshSettings,
    pull_direction: VectorQuantity,
    minimum_radians: f64,
) -> Result<VerificationResult, ModelError> {
    if !(minimum_radians.is_finite()
        && (0.0..std::f64::consts::FRAC_PI_2).contains(&minimum_radians))
    {
        return Err(ModelError::new(
            "minimum draft must be in [0, pi/2) radians",
        ));
    }
    let pull = pull_direction.normalized(Dimension::Scalar)?;
    let magnitude = length(pull);
    if !(magnitude.is_finite() && magnitude > 0.0) {
        return Err(ModelError::new("pull direction must be finite and nonzero"));
    }
    let pull = scale(pull, 1.0 / magnitude);
    let ranges = session.face_pull_ranges(shape, pull)?;
    if ranges.is_empty() {
        return Err(ModelError::new("output has no surface to screen"));
    }
    let mut faces = BTreeMap::new();
    let mut unmeasured = BTreeMap::new();
    for (face, range) in ranges.iter().enumerate() {
        match range {
            Some(range) => {
                if let Some(draft) = exact_draft(range) {
                    faces.insert(face, draft);
                }
            }
            None => {
                unmeasured.insert(face, ());
            }
        }
    }
    let mut evidence = Evidence::Exact;
    if !unmeasured.is_empty() {
        let triangles = session.surface_mesh(shape, mesh.options()?)?;
        let samples = triangles
            .iter()
            .filter(|triangle| unmeasured.contains_key(&triangle.face_index))
            .count();
        faces.extend(sampled_drafts(&triangles, pull, &unmeasured));
        evidence = Evidence::Sampled {
            samples,
            unresolved: 0,
        };
    }
    let shallow = faces
        .values()
        .filter(|draft| draft.angle < minimum_radians)
        .count();
    let least = faces.iter().min_by(|a, b| a.1.angle.total_cmp(&b.1.angle));
    let sampled_note = if unmeasured.is_empty() {
        String::new()
    } else {
        format!("; {} face(s) sampled on the tessellation", unmeasured.len())
    };
    let mut result = match least {
        None => VerificationResult::exact(id, true, "no face runs along the pull direction".into()),
        Some((face, least)) => {
            let result = VerificationResult::exact(
                id,
                shallow == 0,
                format!(
                    "{shallow} of {} face(s) below {minimum_radians} rad; least draft {} \
                     rad on face {face}{sampled_note}",
                    faces.len(),
                    least.angle
                ),
            )
            .measured(Measurement {
                value: least.angle,
                unit: MeasurementUnit::Radian,
                minimum: Some(minimum_radians),
                maximum: None,
            });
            if shallow == 0 {
                result
            } else {
                result.witnessed(Witness {
                    subjects: vec![format!("face {face}")],
                    points_mm: least.points.clone(),
                })
            }
        }
    };
    result.evidence = evidence;
    Ok(result)
}

/// The worst release of one mold half: the most negative sine of release
/// angle (normal along the half's pull) and where it occurs.
struct Release {
    sine: f64,
    face: usize,
    point: Vec3,
}

/// Faces of one clipped half and their worst release, with the faces left to
/// the tessellation and their facet count. `sign` is +1 above the parting
/// plane (faces must turn along the pull) and -1 below.
fn half_release(
    session: &Session,
    half: &Shape<'_>,
    mesh: MeshSettings,
    pull: Vec3,
    origin: Vec3,
    sign: f64,
    tolerance: f64,
) -> Result<(Vec<Release>, usize, usize), ModelError> {
    let ranges = session.face_pull_ranges(half, pull)?;
    let height = |point: Vec3| dot(subtract(point, origin), pull);
    let mut releases = Vec::new();
    let mut unmeasured = BTreeMap::new();
    for (face, range) in ranges.iter().enumerate() {
        let Some(range) = range else {
            unmeasured.insert(face, ());
            continue;
        };
        let (sine, point) = if sign > 0.0 {
            range.minimum
        } else {
            (-range.maximum.0, range.maximum.1)
        };
        // The cut cap and any face lying in the parting plane part the mold.
        let flat = range.minimum.0 == range.maximum.0 && range.minimum.0.abs() >= CAP;
        if flat && height(point).abs() <= tolerance {
            continue;
        }
        releases.push(Release { sine, face, point });
    }
    let mut samples = 0;
    if !unmeasured.is_empty() {
        let triangles = session.surface_mesh(half, mesh.options()?)?;
        for triangle in triangles
            .iter()
            .filter(|triangle| unmeasured.contains_key(&triangle.face_index))
        {
            samples += 1;
            let sine = sign * dot(unit_normal(triangle), pull);
            if sine.is_finite() {
                releases.push(Release {
                    sine,
                    face: triangle.face_index,
                    point: centroid(triangle),
                });
            }
        }
    }
    Ok((releases, unmeasured.len(), samples))
}

/// Splits the output at the parting plane and checks each half's faces
/// release toward their mold half. Complete for a planar parting: a line
/// along the pull that met material again would enter it through a face
/// turned against the pull on the same side, which this rejects. O(faces) exact ranges per half plus one
/// tessellation per half only when some face is unmeasured.
pub(crate) fn undercut(
    session: &Session,
    id: &str,
    shape: &Shape<'_>,
    mesh: MeshSettings,
    pull_direction: VectorQuantity,
    parting_origin: VectorQuantity,
    tolerance_radians: f64,
) -> Result<VerificationResult, ModelError> {
    if !(tolerance_radians.is_finite()
        && (0.0..std::f64::consts::FRAC_PI_2).contains(&tolerance_radians))
    {
        return Err(ModelError::new(
            "undercut tolerance must be in [0, pi/2) radians",
        ));
    }
    let pull = pull_direction.normalized(Dimension::Scalar)?;
    let magnitude = length(pull);
    if !(magnitude.is_finite() && magnitude > 0.0) {
        return Err(ModelError::new("pull direction must be finite and nonzero"));
    }
    let pull = scale(pull, 1.0 / magnitude);
    let origin = parting_origin.normalized(Dimension::Length)?;
    let bounds = session.bounds(shape)?;
    let span = length(subtract(bounds.max, bounds.min));
    let on_plane = (span * 1e-9).max(1e-9);
    let limit = -tolerance_radians.sin() - 1e-9;
    let mut worst: Option<(Release, &str)> = None;
    let mut violations = 0;
    let mut sampled_faces = 0;
    let mut samples = 0;
    for (side, keep_positive, sign) in [("above", true, 1.0), ("below", false, -1.0)] {
        let half = session.clip_by_plane(shape, origin, pull, keep_positive)?;
        let released = half_release(session, &half, mesh, pull, origin, sign, on_plane);
        let _ = session.remove(half);
        let (releases, unmeasured, facets) = released?;
        sampled_faces += unmeasured;
        samples += facets;
        let mut bad_faces = std::collections::BTreeSet::new();
        for release in releases {
            if release.sine < limit {
                bad_faces.insert(release.face);
            }
            if worst
                .as_ref()
                .is_none_or(|(current, _)| release.sine < current.sine)
            {
                worst = Some((release, side));
            }
        }
        violations += bad_faces.len();
    }
    let sampled_note = if sampled_faces == 0 {
        String::new()
    } else {
        format!("; {sampled_faces} face(s) sampled on the tessellation")
    };
    let mut result = match worst {
        None => VerificationResult::exact(id, true, "no face to release".into()),
        Some((release, side)) => {
            let angle = release.sine.clamp(-1.0, 1.0).asin();
            let result = VerificationResult::exact(
                id,
                violations == 0,
                format!(
                    "{violations} face(s) undercut; least release {angle} rad on face {} \
                     {side} the parting plane{sampled_note}",
                    release.face
                ),
            )
            .measured(Measurement {
                value: angle,
                unit: MeasurementUnit::Radian,
                minimum: Some(-tolerance_radians),
                maximum: None,
            });
            if violations == 0 {
                result
            } else {
                result.witnessed(Witness {
                    subjects: vec![format!("face {} {side} the parting plane", release.face)],
                    points_mm: vec![release.point],
                })
            }
        }
    };
    if sampled_faces != 0 {
        result.evidence = Evidence::Sampled {
            samples,
            unresolved: 0,
        };
    }
    Ok(result)
}
