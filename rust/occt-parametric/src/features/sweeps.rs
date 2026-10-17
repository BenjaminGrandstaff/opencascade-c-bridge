//! Sweeps, plain or with a twist, scale, guide or sections (schema 175).
//! Holed planar profiles sweep every wire with the same native transport,
//! twist, scale and guide.
use super::*;

/// A plain sweep keeps the original native path; anything more uses the
/// advanced pipe shell.
fn sweep_one<'a>(
    session: &'a Session,
    profile: &Shape<'_>,
    path: &Shape<'_>,
    options: &occt_bridge::SweepOptions<'_, '_>,
) -> Result<Shape<'a>, ModelError> {
    let plain = options.twist_radians == 0.0
        && options.end_scale == 1.0
        && options.guide.is_none()
        && options.sections.is_empty();
    Ok(if plain {
        session.sweep(profile, path, options.orientation)?
    } else {
        session.sweep_advanced(profile, path, options)?
    })
}

pub(super) fn execute<'a>(
    session: &'a Session,
    profile: &Shape<'_>,
    path: &Shape<'_>,
    options: &occt_bridge::SweepOptions<'_, '_>,
) -> Result<Shape<'a>, ModelError> {
    if session.shape_type(profile)? != ShapeType::Face
        || session.subshape_count(profile, ShapeType::Wire)? <= 1
    {
        return sweep_one(session, profile, path, options);
    }
    if !options.sections.is_empty() {
        return Err(ModelError::new(
            "a sweep with sections needs profiles with one boundary",
        ));
    }
    if !session.face_is_planar(profile)? || !session.is_valid(profile)? {
        return Err(ModelError::new(
            "holed sweep profile must be one valid planar face",
        ));
    }
    let wires = session.subshapes(profile, ShapeType::Wire)?;
    if wires.len() > 101 {
        return Err(ModelError::new(
            "hollow sweep supports at most 100 inner boundaries",
        ));
    }
    let mut sections = Vec::new();
    for wire in wires {
        let face = session.create_face_from_wire(&wire)?;
        let area = session.surface_area(&face)?;
        if !session.is_valid(&face)? || !(area.is_finite() && area > 0.0) {
            return Err(ModelError::new(
                "sweep boundary must define a valid planar region",
            ));
        }
        sections.push((wire, area));
    }
    let outer_index = sections
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.1.total_cmp(&b.1))
        .expect("face has multiple wires")
        .0;
    let outer = sweep_one(session, &sections[outer_index].0, path, options)?;
    let mut inners = Vec::new();
    for (index, (wire, _)) in sections.iter().enumerate() {
        if index != outer_index {
            inners.push(sweep_one(session, wire, path, options)?);
        }
    }
    super::hollow::subtract(session, &outer, &inners)
}
