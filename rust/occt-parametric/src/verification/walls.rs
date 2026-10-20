//! Exact minimum wall (schema 181): the least distance between two faces
//! that do not share an edge and whose closest points face each other
//! through material. Distances are the kernel's exact face extrema; only
//! face pairs whose bounding boxes come within the required minimum are
//! measured, found by a sort and sweep over the boxes.

use super::*;
use occt_bridge::Bounds;
use std::collections::HashSet;

/// Most faces an output may have for the exact check.
pub const MAX_WALL_FACES: usize = 20_000;
/// Most candidate face pairs measured; beyond it the check is refused.
pub const MAX_WALL_PAIRS: usize = 20_000;
/// Closest points must face each other within 60°: cos 60°.
const OPPOSED: f64 = 0.5;

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

/// Gap between two boxes, zero when they overlap.
fn box_gap(a: &Bounds, b: &Bounds) -> f64 {
    let axis = |lo1: f64, hi1: f64, lo2: f64, hi2: f64| (lo2 - hi1).max(lo1 - hi2).max(0.0);
    let (x, y, z) = (
        axis(a.min.x, a.max.x, b.min.x, b.max.x),
        axis(a.min.y, a.max.y, b.min.y, b.max.y),
        axis(a.min.z, a.max.z, b.min.z, b.max.z),
    );
    (x * x + y * y + z * z).sqrt()
}

/// O(F log F + P) box work and P exact face distances for the P candidate
/// pairs within `minimum`; face handles are released on return.
pub(crate) fn exact_minimum_wall(
    session: &Session,
    id: &str,
    shape: &Shape<'_>,
    minimum: Quantity,
) -> Result<VerificationResult, ModelError> {
    let minimum = minimum.normalized()?;
    if !(minimum.is_finite() && minimum > 0.0) {
        return Err(ModelError::new(
            "exact minimum wall needs a positive finite length",
        ));
    }
    let faces = session.subshapes(shape, ShapeType::Face)?;
    if faces.len() > MAX_WALL_FACES {
        return Err(ModelError::new(format!(
            "exact minimum wall supports at most {MAX_WALL_FACES} faces; use minimum_wall"
        )));
    }
    // Each face's edges as indices into the output's edge map, from one map
    // traversal for all faces: faces sharing an index are adjacent.
    let owned = faces
        .iter()
        .map(|face| session.subshapes(face, ShapeType::Edge))
        .collect::<Result<Vec<_>, _>>()?;
    let refs: Vec<&Shape<'_>> = owned.iter().flatten().collect();
    let indices = session.subshape_indices(shape, ShapeType::Edge, &refs)?;
    let mut edges = Vec::with_capacity(faces.len());
    let mut next = indices.into_iter();
    for own in &owned {
        edges.push(next.by_ref().take(own.len()).collect::<HashSet<usize>>());
    }
    drop(refs);
    drop(owned);
    let bounds = faces
        .iter()
        .map(|face| session.exact_bounds(face))
        .collect::<Result<Vec<_>, _>>()?;
    // Sweep along x: a pair can only be within `minimum` if their x ranges are.
    let mut order: Vec<usize> = (0..faces.len()).collect();
    order.sort_by(|&a, &b| bounds[a].min.x.total_cmp(&bounds[b].min.x));
    let mut pairs = Vec::new();
    for (k, &i) in order.iter().enumerate() {
        for &j in &order[k + 1..] {
            if bounds[j].min.x - bounds[i].max.x > minimum {
                break;
            }
            if box_gap(&bounds[i], &bounds[j]) < minimum {
                pairs.push((i.min(j), i.max(j)));
                if pairs.len() > MAX_WALL_PAIRS {
                    return Err(ModelError::new(format!(
                        "more than {MAX_WALL_PAIRS} face pairs lie within the minimum; use minimum_wall"
                    )));
                }
            }
        }
    }
    let size = {
        let b = session.exact_bounds(shape)?;
        dot(sub(b.max, b.min), sub(b.max, b.min)).sqrt()
    };
    let tolerance = 1e-7 * (1.0 + size);
    let candidates = pairs.len();
    let (mut thinnest, mut witness) = (f64::INFINITY, None);
    for (i, j) in pairs {
        if !edges[i].is_disjoint(&edges[j]) {
            continue;
        }
        let gap = session.distance(&faces[i], &faces[j])?;
        if gap.distance <= tolerance || gap.distance >= thinnest {
            continue;
        }
        let across = sub(gap.second, gap.first);
        let unit = Vec3::new(
            across.x / gap.distance,
            across.y / gap.distance,
            across.z / gap.distance,
        );
        let normals =
            session.project_points_to_faces(&[&faces[i], &faces[j]], &[gap.first, gap.second])?;
        // Outward normals point away from the segment at both ends.
        if dot(normals[0].normal, unit) > -OPPOSED || dot(normals[1].normal, unit) < OPPOSED {
            continue;
        }
        // The segment stays in material: the first surface it meets is the far face.
        let hit = session.ray_first_hit(shape, gap.first, unit, gap.distance * 2.0)?;
        if hit.is_some_and(|(_, length)| length < gap.distance - 10.0 * tolerance) {
            continue;
        }
        thinnest = gap.distance;
        witness = Some((i, j, gap.first, gap.second));
    }
    let passed = thinnest >= minimum;
    let message = match witness {
        Some((i, j, _, _)) => format!(
            "thinnest wall {thinnest:.6} mm between faces {i} and {j}; minimum {minimum} mm; \
             {candidates} face pair(s) within the minimum checked"
        ),
        None => format!(
            "no opposed faces closer than {minimum} mm; {candidates} face pair(s) within it checked"
        ),
    };
    let mut result = VerificationResult::exact(id, passed, message);
    if let Some((i, j, first, second)) = witness {
        result = result
            .measured(Measurement {
                value: thinnest,
                unit: MeasurementUnit::Millimeter,
                minimum: Some(minimum),
                maximum: None,
            })
            .witnessed(Witness {
                subjects: vec![format!("face {i}"), format!("face {j}")],
                points_mm: vec![first, second],
            });
    }
    Ok(result)
}
