//! Linear or multi-station radius laws over semantic contour selections.

use super::*;

pub(super) struct EvaluatedLaw {
    stations: Vec<occt_bridge::FilletRadiusStation>,
    direction: occt_bridge::FilletSpineDirection,
}

/// Expression evaluation and sample storage are O(stations), with no graph scans.
pub(super) fn evaluate_law(
    start: &ScalarExpr,
    end: &ScalarExpr,
    stations: &[FilletRadiusStation],
    direction: &FilletSpineDirection,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<EvaluatedLaw, ModelError> {
    let mut values = Vec::with_capacity(stations.len() + 2);
    values.push(occt_bridge::FilletRadiusStation {
        position: 0.0,
        radius: scalar(start, parameters, Dimension::Length)?,
    });
    for station in stations {
        values.push(occt_bridge::FilletRadiusStation {
            position: scalar(&station.position, parameters, Dimension::Scalar)?,
            radius: scalar(&station.radius, parameters, Dimension::Length)?,
        });
    }
    values.push(occt_bridge::FilletRadiusStation {
        position: 1.0,
        radius: scalar(end, parameters, Dimension::Length)?,
    });
    let direction = match direction {
        FilletSpineDirection::Kernel => occt_bridge::FilletSpineDirection::Kernel,
        FilletSpineDirection::Reversed => occt_bridge::FilletSpineDirection::Reversed,
        FilletSpineDirection::FromPoint { point } => occt_bridge::FilletSpineDirection::FromPoint(
            vector(point, parameters, Dimension::Length)?,
        ),
    };
    Ok(EvaluatedLaw {
        stations: values,
        direction,
    })
}

pub(super) fn execute<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[EdgeSelector],
    law: EvaluatedLaw,
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
    let result = if law.stations.len() == 2
        && matches!(law.direction, occt_bridge::FilletSpineDirection::Kernel)
    {
        session.variable_fillet(
            input,
            &references,
            law.stations[0].radius,
            law.stations[1].radius,
        )
    } else {
        session.variable_fillet_stations(input, &references, &law.stations, law.direction)
    }
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
