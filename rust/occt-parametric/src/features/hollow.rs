//! Native containment, subtraction and ancestry shared by hollow constructions.
use super::regions::boxes_separated;
use super::*;

fn solid_volume(session: &Session, shape: &Shape<'_>) -> Result<f64, ModelError> {
    let volume = session.volume(shape)?;
    if session.shape_type(shape)? != ShapeType::Solid
        || !session.is_valid(shape)?
        || !(volume.is_finite() && volume > 0.0)
    {
        return Err(ModelError::new(
            "hollow construction boundaries must produce valid positive-volume solids",
        ));
    }
    Ok(volume)
}

pub(super) fn subtract<'a>(
    session: &'a Session,
    outer: &Shape<'_>,
    inners: &[Shape<'_>],
) -> Result<Shape<'a>, ModelError> {
    if inners.is_empty() || inners.len() > 100 {
        return Err(ModelError::new(
            "hollow construction needs 1-100 inner solids",
        ));
    }
    let outer_volume = solid_volume(session, outer)?;
    let mut inner_bounds = Vec::new();
    let mut removed = 0.0;
    for (index, inner) in inners.iter().enumerate() {
        let volume = solid_volume(session, inner)?;
        let common = session.common(outer, inner)?;
        if (session.volume(&common)? - volume).abs() > 1e-12_f64.max(volume * 1e-7) {
            return Err(ModelError::new("inner solid leaves the outer solid"));
        }
        let bounds = session.exact_bounds(inner)?;
        for (previous, previous_bounds) in inners[..index].iter().zip(&inner_bounds) {
            if !boxes_separated(&bounds, previous_bounds)
                && session.distance(previous, inner)?.distance <= 1e-7
            {
                return Err(ModelError::new("inner solids overlap or touch"));
            }
        }
        removed += volume;
        inner_bounds.push(bounds);
    }
    let compound = session.create_compound(&inners.iter().collect::<Vec<_>>())?;
    let cut = session.cut(outer, &compound)?;
    let mut result = session.compose_history(&cut, outer)?;
    for inner in inners {
        result = session.compose_history(&result, inner)?;
    }
    let result = if session.shape_type(&result)? == ShapeType::Solid {
        result
    } else {
        if session.subshape_count(&result, ShapeType::Solid)? != 1 {
            return Err(ModelError::new(
                "hollow construction must produce one connected solid",
            ));
        }
        session.subshape_with_history(&result, ShapeType::Solid, 0)?
    };
    let volume = solid_volume(session, &result)?;
    if (volume - (outer_volume - removed)).abs() > 1e-12_f64.max(outer_volume * 1e-7) {
        return Err(ModelError::new(
            "hollow construction has unexpected material volume",
        ));
    }
    Ok(result)
}
