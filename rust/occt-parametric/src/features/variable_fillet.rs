//! Linear endpoint-radius fillets using semantic contour selections.

use super::*;

pub(super) fn execute<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[EdgeSelector],
    radii: (f64, f64),
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    if session.subshape_count(input, ShapeType::Solid)? != 1 || !session.is_valid(input)? {
        return Err(ModelError::new(
            "variable fillet input must contain one valid solid",
        ));
    }
    let (selected, sizes) = resolve_edge_selectors(
        session,
        input,
        selectors,
        parameters,
        shapes,
        "variable fillet",
    )?;
    let references = selected.iter().collect::<Vec<_>>();
    let result = session
        .variable_fillet(input, &references, radii.0, radii.1)
        .map_err(|error| ModelError::from(error).locate_selections(&sizes, "edge"));
    cleanup_shapes(session, selected);
    let result = result?;
    if session.subshape_count(&result, ShapeType::Solid)? != 1
        || !session.is_valid(&result)?
        || session.volume(&result)? <= 0.0
    {
        return Err(ModelError::new(
            "variable fillet must produce one valid solid with positive volume",
        ));
    }
    Ok(result)
}
