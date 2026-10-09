//! Holed planar profiles swept with the same native transport for every wire.
use super::*;

pub(super) fn execute<'a>(
    session: &'a Session,
    profile: &Shape<'_>,
    path: &Shape<'_>,
    orientation: occt_bridge::SweepOrientation,
) -> Result<Shape<'a>, ModelError> {
    if session.shape_type(profile)? != ShapeType::Face
        || session.subshape_count(profile, ShapeType::Wire)? <= 1
    {
        return Ok(session.sweep(profile, path, orientation)?);
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
    let outer = session.sweep(&sections[outer_index].0, path, orientation)?;
    let mut inners = Vec::new();
    for (index, (wire, _)) in sections.iter().enumerate() {
        if index != outer_index {
            inners.push(session.sweep(wire, path, orientation)?);
        }
    }
    super::hollow::subtract(session, &outer, &inners)
}
