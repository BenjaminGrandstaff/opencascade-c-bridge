//! Semantic sketch references are resolved against native output geometry,
//! then existing Persistent selectors carry them through downstream history.
use super::*;
use crate::assembly::{add, scale};

fn resolved(
    session: &Session,
    result: &Shape<'_>,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'_>>,
    definitions: &Features<'_>,
) -> Result<(SketchDefinition, Option<ResolvedDatum>), ModelError> {
    let feature = shapes
        .iter()
        .find_map(|(id, shape)| {
            if !session.is_same(shape, result).unwrap_or(false) {
                return None;
            }
            definitions.by_id.get(id.as_str()).copied()
        })
        .ok_or_else(|| ModelError::new("sketch reference requires a sketch feature output"))?;
    let sketch = match &feature.operation {
        FeatureOperation::SketchFace { sketch }
        | FeatureOperation::SketchWire { sketch }
        | FeatureOperation::SketchOpenWire { sketch } => sketch,
        _ => {
            return Err(ModelError::new(
                "sketch reference requires a sketch feature output",
            ));
        }
    };
    let plane =
        crate::features::sketch_support::resolve(session, sketch, parameters, shapes, definitions)?;
    let sketch = if sketch.projections.is_empty() {
        sketch.as_ref().clone()
    } else {
        crate::features::sketch_projection::materialize(
            session,
            sketch,
            parameters,
            shapes,
            definitions,
            plane,
        )?
    };
    Ok((sketch.with_patterns(Some(parameters))?.into_owned(), plane))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn curve<'a>(
    session: &'a Session,
    result: &Shape<'a>,
    entity: &str,
    require_complete: bool,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'a>>,
    definitions: &Features<'_>,
) -> Result<Vec<Shape<'a>>, ModelError> {
    let (sketch, plane) = resolved(session, result, parameters, shapes, definitions)?;
    let solution = sketch.solve(parameters)?;
    let library = sketch.curve_library(
        session,
        parameters,
        &solution,
        sketch.resolved_frame(parameters, plane)?,
    )?;
    let source = library.curves.get(entity).ok_or_else(|| {
        ModelError::new(format!(
            "lost sketch curve reference '{entity}': name no longer exists"
        ))
    })?;
    let mut selected = Vec::new();
    let mut covered = 0.;
    // Exact native overlap, not sampled similarity: a split yields every
    // surviving boundary piece; construction curves contribute no boundary.
    for edge in session.subshapes(result, ShapeType::Edge)? {
        let overlap = session.common(source, &edge)?;
        let length = session
            .subshapes(&overlap, ShapeType::Edge)?
            .iter()
            .map(|e| session.edge_length(e))
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .sum::<f64>();
        if length > 1e-7 {
            covered += length;
            selected.push(edge);
        }
    }
    if selected.is_empty() {
        return Err(ModelError::new(format!(
            "lost sketch curve reference '{entity}': no surviving boundary"
        )));
    }
    if require_complete {
        let length = session
            .subshapes(source, ShapeType::Edge)?
            .iter()
            .map(|e| session.edge_length(e))
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .sum::<f64>();
        if (covered - length).abs() > 1e-7 + length * 1e-8 {
            return Err(ModelError::new(format!(
                "incomplete sketch curve reference '{entity}': boundary was trimmed or removed"
            )));
        }
    }
    Ok(selected)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn region<'a>(
    session: &'a Session,
    result: &Shape<'a>,
    witnesses: &[String],
    excludes: &[String],
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'a>>,
    definitions: &Features<'_>,
) -> Result<Vec<Shape<'a>>, ModelError> {
    let unique: HashSet<_> = witnesses.iter().chain(excludes).collect();
    if witnesses.is_empty()
        || witnesses.len() > 1000
        || excludes.len() > 1000
        || unique.len() != witnesses.len() + excludes.len()
    {
        return Err(ModelError::new(
            "sketch region reference needs 1–1000 unique witnesses and at most 1000 distinct excludes",
        ));
    }
    let (sketch, plane) = resolved(session, result, parameters, shapes, definitions)?;
    let solution = sketch.solve(parameters)?;
    let (origin, x, y) = sketch.resolved_frame(parameters, plane)?;
    let point = |id: &str| -> Result<Vec3, ModelError> {
        let p = solution.points.get(id).ok_or_else(|| {
            ModelError::new(format!(
                "lost sketch region reference: unknown point '{id}'"
            ))
        })?;
        Ok(add(origin, add(scale(x, p.x), scale(y, p.y))))
    };
    let faces = session.subshapes(result, ShapeType::Face)?;
    let mut selected = None;
    for id in witnesses {
        let p = point(id)?;
        let mut matches = Vec::new();
        for (i, face) in faces.iter().enumerate() {
            match session.planar_face_point_state(face, p)? {
                occt_bridge::FacePointState::Inside => matches.push(i),
                occt_bridge::FacePointState::Boundary => {
                    return Err(ModelError::new(format!(
                        "ambiguous sketch region reference: witness '{id}' lies on a boundary"
                    )));
                }
                occt_bridge::FacePointState::Outside => {}
            }
        }
        if matches.is_empty() {
            return Err(ModelError::new(format!(
                "lost sketch region reference: witness '{id}' is outside material"
            )));
        }
        if matches.len() != 1 {
            return Err(ModelError::new(format!(
                "ambiguous sketch region reference: witness '{id}' lies in multiple regions"
            )));
        }
        if selected.is_some_and(|i| i != matches[0]) {
            return Err(ModelError::new(
                "split sketch region reference: witnesses now identify different regions",
            ));
        }
        selected = Some(matches[0]);
    }
    let index = selected.expect("nonempty witnesses");
    for id in excludes {
        if session.planar_face_point_state(&faces[index], point(id)?)?
            != occt_bridge::FacePointState::Outside
        {
            return Err(ModelError::new(format!(
                "merged or ambiguous sketch region reference: excluded point '{id}' now touches selected material"
            )));
        }
    }
    Ok(faces
        .into_iter()
        .enumerate()
        .filter_map(|(i, f)| (i == index).then_some(f))
        .collect())
}
