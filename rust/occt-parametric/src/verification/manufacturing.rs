//! Sampled manufacturing requirements on the output's tessellation: wall
//! thickness, draft, and overhang. Facet normals and centroid rays stand in
//! for exact BREP geometry, so every result carries `Sampled` evidence.

use super::*;
use crate::assembly::{add, cross, dot, length, scale, subtract};
use occt_bridge::MeshTriangle;
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
    let triangles = session.surface_mesh(shape, mesh.options()?)?;
    if triangles.is_empty() {
        return Err(ModelError::new("output has no surface to screen"));
    }
    if let Screen::Draft {
        pull_direction,
        minimum_radians,
    } = rule
    {
        return draft(id, &triangles, pull_direction, minimum_radians);
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
fn draft(
    id: &str,
    triangles: &[MeshTriangle],
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
    // Smallest draft magnitude per face, and the facet where it occurs.
    let mut faces: BTreeMap<usize, (f64, usize)> = BTreeMap::new();
    for (index, triangle) in triangles.iter().enumerate() {
        let projection = dot(unit_normal(triangle), pull).clamp(-1.0, 1.0);
        if !projection.is_finite() || projection.abs() >= 1.0 - 1e-8 {
            continue;
        }
        let angle = projection.asin().abs();
        let entry = faces.entry(triangle.face_index).or_insert((angle, index));
        if angle < entry.0 {
            *entry = (angle, index);
        }
    }
    let shallow = faces
        .values()
        .filter(|(angle, _)| *angle < minimum_radians)
        .count();
    let least = faces
        .iter()
        .min_by(|a, b| a.1.0.total_cmp(&b.1.0))
        .map(|(face, (angle, index))| (*face, *angle, *index));
    let mut result = match least {
        None => VerificationResult::exact(id, true, "no face runs along the pull direction".into()),
        Some((face, angle, index)) => {
            let result = VerificationResult::exact(
                id,
                shallow == 0,
                format!(
                    "{shallow} of {} face(s) below {minimum_radians} rad; least draft {angle} \
                     rad on face {face}",
                    faces.len()
                ),
            )
            .measured(Measurement {
                value: angle,
                unit: MeasurementUnit::Radian,
                minimum: Some(minimum_radians),
                maximum: None,
            });
            if shallow == 0 {
                result
            } else {
                let triangle = &triangles[index];
                result.witnessed(face_witness(triangle, vec![centroid(triangle)]))
            }
        }
    };
    result.evidence = Evidence::Sampled {
        samples: triangles.len(),
        unresolved: 0,
    };
    Ok(result)
}
