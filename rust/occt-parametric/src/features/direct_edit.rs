//! Direct face edits (schema 183): remove, offset and move selected faces of
//! a solid without its feature history.
use super::*;
use std::collections::HashSet;

/// Most faces one `remove_faces` or `move_faces` feature may select. OCCT
/// defeaturing reworks the whole solid for each removed detail, so its time
/// grows faster than linearly: 100 hole walls of a 200-face plate take about
/// 12 s in a release build.
pub const MAX_REMOVED_FACES: usize = 100;
/// Most faces one `offset_faces` feature may select (one offset pass).
pub const MAX_OFFSET_FACES: usize = 10_000;

/// The faces the selectors pick on `input`, each once in first-selected
/// order, with how many each selector added (for locating diagnostics).
fn selected_faces<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[FaceSelector],
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
    (name, limit): (&str, usize),
) -> Result<(Vec<Shape<'session>>, Vec<usize>), ModelError> {
    if session.subshape_count(input, ShapeType::Solid)? != 1 || !session.is_valid(input)? {
        return Err(ModelError::new(format!(
            "{name} input must contain one valid solid"
        )));
    }
    if selectors.is_empty() {
        return Err(ModelError::new(format!(
            "{name} requires at least one face selector"
        )));
    }
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    let mut sizes = Vec::new();
    for selector in selectors {
        let matches =
            resolve_face_selector(session, input, selector, parameters, shapes, definitions)?;
        let indices = session.subshape_indices(
            input,
            ShapeType::Face,
            &matches.iter().collect::<Vec<_>>(),
        )?;
        let before = selected.len();
        for (face, index) in matches.into_iter().zip(indices) {
            if seen.insert(index) {
                selected.push(face);
            }
        }
        sizes.push(selected.len() - before);
        if selected.len() > limit {
            return Err(ModelError::new(format!(
                "{name} selects more than {limit} faces; split the edit into several features"
            )));
        }
    }
    Ok((selected, sizes))
}

/// One valid positive-volume solid, or an error naming the edit.
fn one_solid(session: &Session, result: &Shape<'_>, name: &str) -> Result<(), ModelError> {
    if session.subshape_count(result, ShapeType::Solid)? != 1
        || !session.is_valid(result)?
        || session.volume(result)? <= 0.0
    {
        return Err(ModelError::new(format!(
            "{name} must leave one valid solid with positive volume"
        )));
    }
    Ok(())
}

pub(super) fn remove_faces<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[FaceSelector],
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
) -> Result<Shape<'session>, ModelError> {
    let (faces, sizes) = selected_faces(
        session,
        input,
        selectors,
        parameters,
        shapes,
        definitions,
        ("remove faces", MAX_REMOVED_FACES),
    )?;
    let result = session
        .remove_faces(input, &faces.iter().collect::<Vec<_>>())
        .map_err(|error| ModelError::from(error).locate_selections(&sizes, "face"))?;
    one_solid(session, &result, "remove faces")?;
    Ok(result)
}

pub(super) fn offset_faces<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[FaceSelector],
    (distance, tolerance): (f64, f64),
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
) -> Result<Shape<'session>, ModelError> {
    if !(distance.is_finite() && distance != 0.0) {
        return Err(ModelError::new(
            "offset faces distance must be a finite nonzero length",
        ));
    }
    if !(tolerance.is_finite() && tolerance > 0.0) {
        return Err(ModelError::new(
            "offset faces tolerance must be a positive length",
        ));
    }
    let (faces, sizes) = selected_faces(
        session,
        input,
        selectors,
        parameters,
        shapes,
        definitions,
        ("offset faces", MAX_OFFSET_FACES),
    )?;
    let moves: Vec<_> = faces.iter().map(|face| (face, distance)).collect();
    let result = session
        .offset_faces(input, &moves, tolerance)
        .map_err(|error| ModelError::from(error).locate_selections(&sizes, "face"))?;
    one_solid(session, &result, "offset faces")?;
    Ok(result)
}

/// `left - right` when it holds a solid, else `None`.
fn difference<'session>(
    session: &'session Session,
    left: &Shape<'_>,
    right: &Shape<'_>,
) -> Result<Option<Shape<'session>>, ModelError> {
    let rest = session.cut(left, right)?;
    Ok((session.subshape_count(&rest, ShapeType::Solid)? > 0).then_some(rest))
}

/// Removes the faces, then cuts the region they took away and fuses the
/// region they added at `offset`. Four booleans after the removal; history
/// traces the input's faces through every step.
pub(super) fn move_faces<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[FaceSelector],
    offset: Vec3,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
) -> Result<Shape<'session>, ModelError> {
    if !(offset.x.is_finite() && offset.y.is_finite() && offset.z.is_finite())
        || offset.x * offset.x + offset.y * offset.y + offset.z * offset.z == 0.0
    {
        return Err(ModelError::new(
            "move faces offset must be a finite nonzero vector",
        ));
    }
    let healed = remove_faces(session, input, selectors, parameters, shapes, definitions)?;
    let taken = difference(session, &healed, input)?;
    let added = difference(session, input, &healed)?;
    if taken.is_none() && added.is_none() {
        return Err(ModelError::new(
            "move faces selection bounds no material to move",
        ));
    }
    let mut result = healed;
    if let Some(taken) = taken {
        let tool = session.translate(&taken, offset)?;
        let cut = session.cut(&result, &tool)?;
        result = session.compose_history(&cut, &result)?;
    }
    if let Some(added) = added {
        let tool = session.translate(&added, offset)?;
        let fused = session.fuse(&result, &tool)?;
        result = session.compose_history(&fused, &result)?;
    }
    one_solid(session, &result, "move faces")?;
    Ok(result)
}
