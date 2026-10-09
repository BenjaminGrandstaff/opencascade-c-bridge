//! Holed planar profiles swept with the same native transport for every wire.
use super::regions::boxes_separated;
use super::*;

fn solid_volume(session: &Session, shape: &Shape<'_>) -> Result<f64, ModelError> {
    let volume = session.volume(shape)?;
    if session.shape_type(shape)? != ShapeType::Solid
        || !session.is_valid(shape)?
        || !(volume.is_finite() && volume > 0.0)
    {
        return Err(ModelError::new(
            "hollow sweep boundaries must produce valid positive-volume solids",
        ));
    }
    Ok(volume)
}

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
    let outer_volume = solid_volume(session, &outer)?;
    let mut inners = Vec::new();
    let mut inner_bounds = Vec::new();
    let mut removed = 0.0;
    for (index, (wire, _)) in sections.iter().enumerate() {
        if index == outer_index {
            continue;
        }
        let inner = session.sweep(wire, path, orientation)?;
        let volume = solid_volume(session, &inner)?;
        let common = session.common(&outer, &inner)?;
        if (session.volume(&common)? - volume).abs() > 1e-12_f64.max(volume * 1e-7) {
            return Err(ModelError::new("swept hole leaves the outer solid"));
        }
        let bounds = session.exact_bounds(&inner)?;
        for (previous, previous_bounds) in inners.iter().zip(&inner_bounds) {
            if !boxes_separated(&bounds, previous_bounds)
                && session.distance(previous, &inner)?.distance <= 1e-7
            {
                return Err(ModelError::new("swept holes overlap or touch"));
            }
        }
        removed += volume;
        inner_bounds.push(bounds);
        inners.push(inner);
    }
    let compound = session.create_compound(&inners.iter().collect::<Vec<_>>())?;
    let cut = session.cut(&outer, &compound)?;
    let mut result = session.compose_history(&cut, &outer)?;
    for inner in &inners {
        result = session.compose_history(&result, inner)?;
    }
    let result = if session.shape_type(&result)? == ShapeType::Solid {
        result
    } else {
        if session.subshape_count(&result, ShapeType::Solid)? != 1 {
            return Err(ModelError::new(
                "hollow sweep must produce one connected solid",
            ));
        }
        session.subshape_with_history(&result, ShapeType::Solid, 0)?
    };
    let volume = solid_volume(session, &result)?;
    if (volume - (outer_volume - removed)).abs() > 1e-12_f64.max(outer_volume * 1e-7) {
        return Err(ModelError::new(
            "hollow sweep has unexpected material volume",
        ));
    }
    Ok(result)
}
