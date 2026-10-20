//! Requirement verification results: measured values, evidence quality, and
//! witnesses, plus exact topology measurements used by part rules.

use super::*;
use occt_bridge::EdgeConcavity;
#[allow(unused_imports)]
pub use occt_parametric_core::verification::*;

mod manufacturing;
pub(crate) use manufacturing::{Screen, screen, undercut};
mod materials;
pub(crate) use materials::{Bending, bending_stress, mass_range, temperature_limit};

/// O(topology) time and transient handles: each solid's subshapes are mapped
/// to the output's unique topology indices once, then released.
pub(crate) fn connectivity(
    session: &Session,
    shape: &Shape<'_>,
) -> Result<Connectivity, ModelError> {
    // Descendant traversal excludes the shape itself, so a bare solid has no
    // solid subshapes and, being one solid, no loose topology.
    let shape_type = session.shape_type(shape)?;
    if shape_type == ShapeType::Solid {
        return Ok(Connectivity {
            solids: 1,
            maximum_shells_per_solid: session.subshape_count(shape, ShapeType::Shell)?,
            loose_faces: 0,
            loose_edges: 0,
            loose_vertices: 0,
        });
    }
    let solids = session.subshapes(shape, ShapeType::Solid)?;
    let mut maximum_shells_per_solid = 0;
    for solid in &solids {
        maximum_shells_per_solid =
            maximum_shells_per_solid.max(session.subshape_count(solid, ShapeType::Shell)?);
    }
    let mut loose = [0; 3];
    for (slot, kind) in [ShapeType::Face, ShapeType::Edge, ShapeType::Vertex]
        .into_iter()
        .enumerate()
    {
        // A bare face, edge, or vertex output is itself loose topology.
        let total = session.subshape_count(shape, kind)? + usize::from(shape_type == kind);
        let mut owned = vec![false; total];
        for solid in &solids {
            let members = session.subshapes(solid, kind)?;
            let references = members.iter().collect::<Vec<_>>();
            for index in session.subshape_indices(shape, kind, &references)? {
                owned[index] = true;
            }
        }
        loose[slot] = owned.iter().filter(|owned| !**owned).count();
    }
    Ok(Connectivity {
        solids: solids.len(),
        maximum_shells_per_solid,
        loose_faces: loose[0],
        loose_edges: loose[1],
        loose_vertices: loose[2],
    })
}

fn consider_sharp_edges(
    session: &Session,
    shape: &Shape<'_>,
    sharp_edges: SharpEdges,
    convex: bool,
    concave: bool,
    smallest: &mut Option<Smallest>,
) -> Result<usize, ModelError> {
    let mut sharp = 0;
    if let SharpEdges::ZeroRadius { tangency_radians } = sharp_edges {
        for (index, concavity) in session
            .edge_concavities(shape, tangency_radians)?
            .into_iter()
            .enumerate()
        {
            let counted = match concavity {
                EdgeConcavity::Convex => convex,
                EdgeConcavity::Concave => concave,
                EdgeConcavity::Mixed => true,
                EdgeConcavity::Smooth | EdgeConcavity::Other => false,
            };
            if !counted {
                continue;
            }
            sharp += 1;
            if smallest.as_ref().is_none_or(|found| found.radius > 0.0) {
                let edge = session.subshape(shape, ShapeType::Edge, index)?;
                let point = session.edge_sample_points(&edge, 3)?[1];
                consider(smallest, 0.0, || format!("edge {index}"), point);
            }
        }
    }
    Ok(sharp)
}

/// O(faces) analytic evaluation, O(samples^2) per freeform face, plus one
/// edge analysis pass when sharp edges count. Face and edge indices in
/// witnesses follow `Session::subshapes` order.
pub(crate) fn minimum_radius(
    session: &Session,
    id: &str,
    shape: &Shape<'_>,
    minimum: Quantity,
    side: RadiusSide,
    sharp_edges: SharpEdges,
    samples_per_direction: u32,
) -> Result<VerificationResult, ModelError> {
    if minimum.dimension != Dimension::Length {
        return Err(ModelError::new("minimum radius must be a length"));
    }
    let limit = minimum.normalized()?;
    if !(limit.is_finite() && limit > 0.0) {
        return Err(ModelError::new(
            "minimum radius must be finite and positive",
        ));
    }
    // Radii come from 1 / curvature; a part exactly at its limit must pass.
    let accepted = limit * (1.0 - RADIUS_ROUNDOFF);
    let convex = matches!(side, RadiusSide::Convex | RadiusSide::Both);
    let concave = matches!(side, RadiusSide::Concave | RadiusSide::Both);
    let mut smallest = None;
    let mut samples = 0;
    let mut sampled_faces = 0;
    for (index, face) in session
        .face_radius_bounds(shape, samples_per_direction)?
        .into_iter()
        .enumerate()
    {
        if !face.exact {
            sampled_faces += 1;
            samples += face.samples as usize;
        }
        let sides = [(convex, face.convex), (concave, face.concave)];
        for (radius, point) in sides
            .into_iter()
            .filter_map(|(wanted, found)| wanted.then_some(found).flatten())
        {
            consider(&mut smallest, radius, || format!("face {index}"), point);
        }
    }
    let sharp = consider_sharp_edges(session, shape, sharp_edges, convex, concave, &mut smallest)?;
    let side_name = match side {
        RadiusSide::Convex => "convex",
        RadiusSide::Concave => "concave",
        RadiusSide::Both => "convex or concave",
    };
    let sharp_note = match sharp_edges {
        SharpEdges::Ignore => String::new(),
        SharpEdges::ZeroRadius { .. } => format!("; {sharp} sharp {side_name} edge(s)"),
    };
    let evidence = if sampled_faces == 0 {
        Evidence::Exact
    } else {
        Evidence::Sampled {
            samples,
            unresolved: 0,
        }
    };
    let mut result = match &smallest {
        Some(found) => VerificationResult::exact(
            id,
            found.radius >= accepted,
            format!(
                "smallest {side_name} radius {} mm at {}; expected >= {limit} mm{sharp_note}",
                found.radius, found.subject
            ),
        )
        .measured(Measurement {
            value: found.radius,
            unit: MeasurementUnit::Millimeter,
            minimum: Some(limit),
            maximum: None,
        }),
        None => {
            VerificationResult::exact(id, true, format!("no {side_name} curvature{sharp_note}"))
        }
    };
    result.evidence = evidence;
    if let Some(found) = smallest.filter(|found| found.radius < accepted) {
        result = result.witnessed(Witness {
            subjects: vec![found.subject],
            points_mm: vec![found.point],
        });
    }
    Ok(result)
}

/// Exact bounding-box extents against an envelope, smallest to smallest, so
/// any axis-aligned orientation that fits passes. O(topology).
pub(crate) fn fits_within(
    session: &Session,
    id: &str,
    shape: &Shape<'_>,
    envelope: VectorQuantity,
) -> Result<VerificationResult, ModelError> {
    let envelope = envelope.normalized(Dimension::Length)?;
    let limits = sorted_extents(envelope);
    if limits
        .iter()
        .any(|value| !(value.is_finite() && *value > 0.0))
    {
        return Err(ModelError::new("envelope must be finite and positive"));
    }
    let bounds = session.exact_bounds(shape)?;
    let extents = sorted_extents(Vec3::new(
        bounds.max.x - bounds.min.x,
        bounds.max.y - bounds.min.y,
        bounds.max.z - bounds.min.z,
    ));
    let passed = extents
        .iter()
        .zip(limits)
        .all(|(extent, limit)| *extent <= limit);
    let largest = extents[2];
    let mut result = VerificationResult::exact(
        id,
        passed,
        format!(
            "extents {:.3} x {:.3} x {:.3} mm; envelope {:.3} x {:.3} x {:.3} mm",
            extents[0], extents[1], extents[2], limits[0], limits[1], limits[2]
        ),
    )
    .measured(Measurement {
        value: largest,
        unit: MeasurementUnit::Millimeter,
        minimum: None,
        maximum: Some(limits[2]),
    });
    if !passed {
        result = result.witnessed(Witness {
            subjects: vec!["bounding box".into()],
            points_mm: vec![bounds.min, bounds.max],
        });
    }
    Ok(result)
}
